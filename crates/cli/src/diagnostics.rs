//! CLI 侧诊断翻译子系统：cargo/rustc 编译诊断的教学化翻译管线
//!
//! 与 LSP 侧 `response_map/diag_text.rs` 是姊妹职责：从 rustc JSON 诊断
//! 解析 → 教学化翻译（错误消息 + 类型/模块路径中文化）→ 位置回译
//!（英文产物坐标 → 母语源码坐标，含子模块与 `#[path]` 注解行换算）。
//!
//! 供 `run` / `check` 子命令共用：单文件项目走 [`run_direct_rustc`] /
//! [`check_direct_rustc`] 直调 rustc 快速路径（绕开 cargo 索引开销），
//! 多文件或有依赖的项目回退 cargo（[`translate_cargo_progress`] 翻译进度行、
//! [`translate_cargo_diagnostics`] 翻译 JSON 诊断）。
//!
//! 模块边界：本模块只消费转译结果（源码 + 列映射 + 行映射 + 项目上下文），
//! 不负责转译编排（见 `main.rs` 的 `transpile_project_files` 等）。

use crate::{
    builtin_lang, get_lang_code_from_extension, lang_manager, lang_pack_root_of, resolve_rustc,
    temp_guard, ui,
};
use i18n_rust_engine::mapping_manager::MappingManager;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 判断是否可单文件直调 rustc：src/ 下仅一个方言文件且 Cargo.toml 无依赖
///
/// 教学单文件项目（仅 main.zh）直调 rustc 绕开 cargo：无索引网络开销、
/// 无需 Cargo.lock 预生成，编译诊断格式与 cargo 完全一致；
/// 多文件（mod 引用）或有依赖的项目回退 cargo 流程。
pub(crate) fn can_use_direct_rustc(project_root: &Path, file: &Path) -> bool {
    // 动态方言扩展名（内置 + 用户安装），避免硬编码列表与
    // `rzc lang install` 安装的新语言包脱节（新增语言走不了快速路径）
    let extensions = lang_manager::all_available_extensions();
    let is_dialect_file = |name: &str| extensions.iter().any(|e| name.ends_with(&format!(".{e}")));
    // 方言文件计数：src/ 与项目根都扫（教学项目 src/main.zh 为主，
    // 项目根也可能放 main.zh）；超过 1 个视为多文件项目
    let mut dialect_count = 0usize;
    for dir in [project_root.join("src"), project_root.to_path_buf()] {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if is_dialect_file(&name) {
                    dialect_count += 1;
                }
            }
        }
    }
    if dialect_count != 1 {
        return false;
    }
    // 入口文件必须是 src/main.<方言扩展名>（聚合 main.rs 已写入）
    let is_main_entry = file
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|name| {
            let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
            stem == "main" && extensions.iter().any(|e| e == ext)
        });
    if !is_main_entry {
        return false;
    }
    // Cargo.toml 的 [dependencies] 非空（有第三方依赖）时回退 cargo
    let cargo_toml = project_root.join("Cargo.toml");
    if let Ok(content) = fs::read_to_string(&cargo_toml)
        && let Some(after) = content.split("[dependencies]").nth(1)
    {
        // 依赖行形如 `rand = "0.8"`；注释/空行/子表头不算依赖
        let has_dep = after.lines().any(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#') && !t.starts_with('[') && t.contains('=')
        });
        if has_dep {
            return false;
        }
    }
    true
}

/// 单文件直调 rustc 运行：编译（--error-format=json）→ 翻译诊断 → 运行 exe
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_direct_rustc(
    ui: &ui::Ui,
    project_root: &Path,
    source_path: &Path,
    lang_pack: &Option<PathBuf>,
    manager: &MappingManager,
    source: &str,
    file: &Path,
    column_map: &i18n_rust_engine::column_map::ColumnMap,
    entry_line_map: &[usize],
    project_ctx: &i18n_rust_engine::alias::ProjectContext,
) -> anyhow::Result<std::process::ExitCode> {
    let exe = temp_guard::secure_temp_path(&format!(
        "rzc-run-{}-{}.exe",
        temp_guard::safe_user_segment(),
        std::process::id()
    ))?;
    let output = Command::new(resolve_rustc())
        .args(["--edition", "2024", "--error-format=json"])
        .arg(source_path)
        .arg("-o")
        .arg(&exe)
        .output()
        .map_err(|e| anyhow::anyhow!("rustc 启动失败: {e}"))?;
    let stderr_text = String::from_utf8_lossy(&output.stderr).to_string();
    let rustc_output = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        stderr_text
    );
    let ok = output.status.success();
    if !rustc_output.trim().is_empty() {
        let _ = translate_cargo_diagnostics(
            &rustc_output,
            &stderr_text,
            &DiagContext {
                ui,
                lang_pack,
                project_root,
                manager,
                source,
                file,
                column_map: Some(column_map),
                entry_line_map: Some(entry_line_map),
                project: Some(project_ctx),
            },
            ok,
            true,
            // 直调 rustc：编译失败文本由本函数输出（非流式透传）
            false,
        );
    }
    if !ok {
        return Ok(std::process::ExitCode::FAILURE);
    }
    // 编译成功：运行程序并传播退出码（无论成败都清理临时 exe）
    let status = match Command::new(&exe).status() {
        Ok(s) => s,
        Err(e) => {
            let _ = std::fs::remove_file(&exe);
            return Err(anyhow::anyhow!("运行失败: {e}"));
        }
    };
    let _ = std::fs::remove_file(&exe);
    Ok(status
        .code()
        .map(|c| std::process::ExitCode::from(c as u8))
        .unwrap_or(std::process::ExitCode::FAILURE))
}

/// 单文件直调 rustc 检查：编译（--emit=metadata，不生成可执行文件）
#[allow(clippy::too_many_arguments)]
pub(crate) fn check_direct_rustc(
    ui: &ui::Ui,
    project_root: &Path,
    source_path: &Path,
    lang_pack: &Option<PathBuf>,
    manager: &MappingManager,
    source: &str,
    file: &Path,
    column_map: &i18n_rust_engine::column_map::ColumnMap,
    entry_line_map: &[usize],
    project_ctx: &i18n_rust_engine::alias::ProjectContext,
) -> anyhow::Result<std::process::ExitCode> {
    let output = Command::new(resolve_rustc())
        .args([
            "--edition",
            "2024",
            "--error-format=json",
            "--emit=metadata",
        ])
        .arg(source_path)
        .output()
        .map_err(|e| anyhow::anyhow!("rustc 启动失败: {e}"))?;
    let exit_code = if output.status.success() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    };
    let stderr_text = String::from_utf8_lossy(&output.stderr).to_string();
    let rustc_output = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        stderr_text
    );
    let _ = translate_cargo_diagnostics(
        &rustc_output,
        &stderr_text,
        &DiagContext {
            ui,
            lang_pack,
            project_root,
            manager,
            source,
            file,
            column_map: Some(column_map),
            entry_line_map: Some(entry_line_map),
            project: Some(project_ctx),
        },
        output.status.success(),
        false,
        // 直调 rustc 检查：诊断输出由本函数负责（非流式透传）
        false,
    );
    Ok(exit_code)
}

/// 翻译 cargo 的人类可读进度行（json 模式下这些行仍输出到 stderr）
///
/// 命中固定前缀（Compiling/Finished/Running 等）时翻译；
/// 其余行（程序 stderr 等）原样返回。
pub(crate) fn translate_cargo_progress(line: &str, ui: &ui::Ui) -> String {
    // cargo 进度行带行首缩进（如 "   Compiling ..."），先去除空白再匹配前缀
    let trimmed = line.trim_start();
    for (prefix, key) in [
        ("Compiling ", "cargo_progress_compiling"),
        ("Checking ", "cargo_progress_checking"),
        ("Finished ", "cargo_progress_finished"),
        ("Running ", "cargo_progress_running"),
        ("error: ", "cargo_progress_error"),
    ] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            // error 摘要（如 "could not compile `__` due to N previous error"）
            // 二次翻译固定短语，其余保留原文
            if key == "cargo_progress_error" {
                let rest = rest
                    .strip_prefix("could not compile ")
                    .map(|r| {
                        // 尾部 "due to N previous errors" 一并本地化：
                        // 如 `abc` (bin "abc") due to 9 previous errors
                        let trimmed = r.trim_start();
                        match trimmed.split_once(" due to ") {
                            Some((main_part, due_part)) => {
                                let count = due_part.split_whitespace().next().unwrap_or("");
                                format!(
                                    "{}{}",
                                    ui.f("cargo_progress_could_not_compile", &[main_part]),
                                    ui.f("cargo_progress_due_to", &[count])
                                )
                            }
                            None => ui.f("cargo_progress_could_not_compile", &[trimmed]),
                        }
                    })
                    .unwrap_or_else(|| rest.to_string());
                return ui.f(key, &[&rest]);
            }
            return ui.f(key, &[rest.trim_start()]);
        }
    }
    line.to_string()
}

/// 诊断翻译上下文：封装 `translate_cargo_diagnostics` 的共享引用参数，
/// 避免 8+ 位置参数的可读性灾难
pub(crate) struct DiagContext<'a> {
    pub(crate) ui: &'a ui::Ui,
    pub(crate) lang_pack: &'a Option<PathBuf>,
    pub(crate) project_root: &'a Path,
    pub(crate) manager: &'a MappingManager,
    pub(crate) source: &'a str,
    pub(crate) file: &'a Path,
    /// 转译产物的列映射：把 rustc 诊断的（英文产物）列号回译到母语源码列号。
    ///
    /// rustc 看到的是转译后的英文源码，其列号对母语源码无效
    ///（如 `让` → `let` 后整行右移）。`None` 表示不做映射，保持旧行为。
    pub(crate) column_map: Option<&'a i18n_rust_engine::column_map::ColumnMap>,
    /// 入口磁盘产物行 → 引擎直出行映射（见
    /// [`annotate_non_ascii_mods_with_lines`](i18n_rust_engine::module_path::annotate_non_ascii_mods_with_lines)）。
    ///
    /// 入口产物写盘前经 `#[path]` 注解插入整行（每个非 ASCII `模块 xxx;`
    /// 一行），而 `column_map` 以未注解的引擎直出产物为基准；
    /// 回译前须先用本映射把 rustc 的磁盘行号换算回引擎直出行号。
    /// `None` 表示无注解（磁盘产物与引擎直出逐行一致）。
    pub(crate) entry_line_map: Option<&'a [usize]>,
    /// 项目级声明上下文：诊断重放（[`resolve_dialect_context`]）须与
    /// 写盘转译共享同一上下文，否则列映射与磁盘产物不一致（#8）
    pub(crate) project: Option<&'a i18n_rust_engine::alias::ProjectContext>,
}

/// 解析 cargo --message-format=json 输出并翻译为教学化诊断（check 与 run 共用）
///
/// 返回是否成功输出了翻译后的教学诊断；调用方据此决定是否回退原始文本。
/// `cargo_ok=false` 且无可解析诊断时原样输出 cargo 消息，绝不虚报“编译成功”。
/// `silent_success=true`（run 场景）时，无诊断且编译成功保持静默——
/// 程序已运行，不再提示“编译成功”。
/// `streamed_output=true`（cargo run 场景）时，cargo 消息与程序输出均已实时
/// 透传：「失败且无可解析诊断」不再补打“编译错误”标签——构建可能已成功，
/// 失败发生在程序运行阶段（如 panic 退出），补打空标签会误导。
pub(crate) fn translate_cargo_diagnostics(
    rustc_output: &str,
    stderr_text: &str,
    ctx: &DiagContext<'_>,
    cargo_ok: bool,
    silent_success: bool,
    streamed_output: bool,
) -> bool {
    use i18n_rust_engine::diagnostic::{
        DiagnosticTranslator, ErrorTranslationManager, parse_diagnostic_output,
    };
    let ui = ctx.ui;
    let lang_pack = ctx.lang_pack;
    let project_root = ctx.project_root;
    let manager = ctx.manager;
    let file = ctx.file;

    // 未解析导入提取：诊断展示后附带 `rzc add` 加依赖提示（教学化引导）
    let unresolved_crates = extract_unresolved_crates(rustc_output);

    // 按语言代码选择错误消息：--lang-pack 目录 > 项目内 lang-packs/<lang>/ > 内置
    let lang_code = file
        .extension()
        .and_then(|e| e.to_str())
        .and_then(get_lang_code_from_extension)
        .unwrap_or_else(ui::detect_ui_lang);
    let error_msg_path = if let Some(path) = lang_pack {
        path.join("errors.toml")
    } else if lang_pack_root_of(project_root)
        .join(&lang_code)
        .join("errors.toml")
        .exists()
    {
        lang_pack_root_of(project_root)
            .join(&lang_code)
            .join("errors.toml")
    } else {
        lang_manager::global_lang_dir()
            .join(&lang_code)
            .join("errors.toml")
    };
    // 类型映射（英文 → 中文）：keywords ["类型"] 节反转 + stdlib 标识符别名反转补充，
    // 供诊断消息中的类型/特征名中文化（如 `std::fmt::Display` → `标准库::格式化::可显示`）；
    // 类型节优先，stdlib 仅补充缺失条目（不覆盖）。
    // 过滤英文键的反向修正条目（如 stdlib 的 "format" = "fmt"，仅供转译管线修正路径段），
    // 避免诊断翻译中出现英文值。
    let 是中文键 = |键: &str| {
        !键.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    let mut reverse_map: HashMap<String, String> = manager
        .get_section_mapping("类型")
        .map(|section| {
            section
                .iter()
                .filter(|(k, _)| 是中文键(k))
                .map(|(k, v)| (v.clone(), k.clone()))
                .collect()
        })
        .unwrap_or_default();
    for (中文, 英文) in manager.get_alias_map() {
        if 是中文键(中文) {
            reverse_map
                .entry(英文.clone())
                .or_insert_with(|| 中文.clone());
        }
    }
    // 模块路径映射补充（std → 标准库 等，供 `std::fmt::Display` 的路径段中文化）；
    // 覆盖第三方库的同名条目（如 log crate 的 fmt → 格式化层），保证标准库路径翻译稳定
    for (中文, 英文) in manager.get_module_path_map() {
        if 是中文键(中文) {
            reverse_map.insert(英文.clone(), 中文.clone());
        }
    }
    let translator = if error_msg_path.exists() {
        // 加载失败时降级到内置表，不因错误消息文件损坏阻断诊断展示
        match ErrorTranslationManager::load_from_file(&error_msg_path) {
            Ok(translation_manager) => Some(DiagnosticTranslator::new(
                translation_manager,
                reverse_map.clone(),
            )),
            Err(e) => {
                eprintln!("{}", ui.f("load_error_msg_failed", &[&e.to_string()]));
                None
            }
        }
    } else {
        None
    };
    // 文件路径不可用/加载失败时回退内置语言包（未知语言代码自动回退中文）
    let translator = translator.or_else(|| {
        let builtin = builtin_lang::get_builtin_data(&lang_code);
        match ErrorTranslationManager::load_from_string(builtin.errors_toml) {
            Ok(translation_manager) => Some(DiagnosticTranslator::new(
                translation_manager,
                reverse_map.clone(),
            )),
            Err(e) => {
                eprintln!("{}", ui.f("warn_builtin_errors_failed", &[&e.to_string()]));
                None
            }
        }
    });

    let original_filename = file
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let mut diagnostics = parse_diagnostic_output(rustc_output);
    // 保留 error/warning；不要求有错误码——无码的解析错误（如缺括号）
    // 也必须显示，否则会被静默吞掉导致假“编译成功”
    diagnostics.retain(|d| d.level == "error" || d.level == "warning");
    // rustc 的汇总元消息（"aborting due to N previous error(s)"）：无代码位置、
    // 无教学内容，具体错误已逐条列出；省略它可以避免英文残句与伪“错误”行
    // 混入诊断列表（数字无法用消息表占位符捕获，故不在翻译层处理）
    diagnostics.retain(|d| !d.message.starts_with("aborting due to "));
    let mut seen_codes = std::collections::HashSet::new();
    diagnostics.retain(|d| {
        if let Some(ref code) = d.code {
            seen_codes.insert(code.code.clone())
        } else {
            true // 无错误码的诊断（解析错误等）不去重，直接保留
        }
    });

    if diagnostics.is_empty() {
        if cargo_ok {
            if !silent_success {
                println!("{}", ui.t("success_compile"));
            }
        } else if !streamed_output {
            // cargo 失败但无可解析的 JSON 诊断（Cargo.toml 语法错误、
            // 链接错误等）：原样输出 cargo 消息，绝不虚报“编译成功”；
            // streamed_output 场景跳过（见函数文档，避免运行时失败被误标）
            eprintln!("{}", ui.f("compile_error", &[stderr_text.trim()]));
        }
        print_dependency_hints(&unresolved_crates, ui);
        return false;
    }

    if let Some(ref translator) = translator {
        let mut teaching_list = translator.batch_translate(&diagnostics);
        let mut seen_teaching_codes = std::collections::HashSet::new();
        teaching_list.retain(|t| {
            t.error_code
                .as_ref()
                .is_none_or(|code| seen_teaching_codes.insert(code.clone()))
        });
        // 诊断定位回译：入口产物（src/main.rs）用调用方传入的源码与列映射；
        // 多文件项目的子模块产物（如 src/接口.rs）解析回母语源文件
        // （src/接口.zh）后用其源码与列映射单独回译——修复子模块错误被统一
        // 误标为入口文件且源码行/列号错位的问题。
        let dialect_ext = file.extension().and_then(|e| e.to_str()).unwrap_or("zh");
        let mut fixer = DiagLocationFixer::new(ctx, &original_filename, dialect_ext);
        for teaching in &mut teaching_list {
            fixer.fix(teaching);
        }
        if teaching_list.is_empty() {
            if cargo_ok {
                if !silent_success {
                    println!("{}", ui.t("success_compile"));
                }
            } else if !streamed_output {
                eprintln!("{}", ui.f("compile_error", &[stderr_text.trim()]));
            }
            print_dependency_hints(&unresolved_crates, ui);
            return false;
        }
        println!(
            "{}",
            i18n_rust_engine::diagnostic::TeachingDiagnostic::batch_format_as_text(&teaching_list)
        );
        print_dependency_hints(&unresolved_crates, ui);
        true
    } else {
        // 无翻译表：输出 JSON 中的原始 message，保证诊断不丢失
        for line in rustc_output.lines() {
            if let Ok(raw) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(message) = raw.get("message") {
                    println!("{}", message.as_str().unwrap_or(""));
                }
            } else if !line.trim().is_empty() {
                println!("{}", line);
            }
        }
        print_dependency_hints(&unresolved_crates, ui);
        false
    }
}

/// 诊断文件上下文：rustc 报告的（英文产物）文件解析回母语源文件后
/// 得到的显示名、源码与列映射，供行号/列号/源行统一回译
struct DiagFileContext {
    display_name: String,
    source: String,
    column_map: i18n_rust_engine::column_map::ColumnMap,
}

/// 解析 rustc 报告的诊断文件路径为实际路径：
/// 绝对路径原样；相对路径先相对项目根（cargo 的 cwd），再相对当前目录。
/// 无法定位时返回 None。
fn resolve_product_path(project_root: &Path, name: &str) -> Option<PathBuf> {
    let p = Path::new(name);
    if p.is_absolute() {
        return Some(p.to_path_buf());
    }
    let from_root = project_root.join(p);
    if from_root.exists() {
        return Some(from_root);
    }
    std::env::current_dir().ok().map(|dir| dir.join(p))
}

/// 把 rustc 报告的诊断文件（转译产物路径）解析为母语源文件上下文。
///
/// 多文件项目中 rustc 报告的是子模块转译产物（如 src/接口.rs），其行列是
/// 英文产物的坐标、源码行也是英文；须找到同名方言源文件（src/接口.zh）并用
/// 其源码重建列映射。对应方言文件不存在（手写 .rs、第三方库源码等）时
/// 返回 None，调用方保持 rustc 原始定位，绝不误标成入口文件。
fn resolve_dialect_context(
    project_root: &Path,
    dialect_ext: &str,
    product_file: &str,
    manager: &MappingManager,
    project: Option<&i18n_rust_engine::alias::ProjectContext>,
) -> Option<DiagFileContext> {
    let abs = resolve_product_path(project_root, product_file)?;
    // 产物名以 .rs 结尾时换成方言扩展名（src/接口.rs → src/接口.zh）
    let source_path = if abs.extension().is_some_and(|e| e == "rs") {
        abs.with_extension(dialect_ext)
    } else {
        abs
    };
    let source = fs::read_to_string(&source_path).ok()?;
    // 现场重放转译管线取列映射：与写盘时同一管线（同一项目上下文，
    // 确定性输出），保证列号回译与磁盘上的转译产物一致；静默重放：
    // 教学告警（lint/全角/Unicode）已在写盘转译时输出过，此处重放不得重复告警
    let transpiled =
        i18n_rust_engine::transpile_pipeline_quiet_with_project(&source, manager, project);
    let column_map =
        i18n_rust_engine::column_map::ColumnMap::build(&source, &transpiled.pipeline_map);
    Some(DiagFileContext {
        display_name: source_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        source,
        column_map,
    })
}

/// 诊断定位回译器：把教学诊断的位置从英文产物坐标回译到母语源码坐标
///
/// 入口产物用调用方传入的源码/列映射；项目内其他方言文件的转译产物按
/// 文件名解析回母语源文件，用其源码与列映射单独回译；非方言文件保持
/// rustc 原始定位。
pub(crate) struct DiagLocationFixer<'a> {
    /// 入口文件的母语源码
    source: &'a str,
    /// 入口产物的列映射（rustc 列号 → 母语列号）
    column_map: Option<&'a i18n_rust_engine::column_map::ColumnMap>,
    /// 入口磁盘产物行 → 引擎直出行映射（含 `#[path]` 注解插入行时非恒等）
    entry_line_map: Option<&'a [usize]>,
    project_root: &'a Path,
    manager: &'a MappingManager,
    /// 入口文件名（如 main.zh）
    original_filename: &'a str,
    /// 入口产物的规范路径（src/main.rs），用于识别入口产物的诊断
    entry_canon: Option<PathBuf>,
    /// 入口文件的方言扩展名（如 zh），用于把产物 .rs 还原为源文件
    dialect_ext: &'a str,
    /// 项目级声明上下文（诊断重放与写盘转译一致，见 [`DiagContext::project`]）
    project: Option<&'a i18n_rust_engine::alias::ProjectContext>,
    /// 诊断文件 → 母语源文件上下文缓存（None 表示非方言文件）
    file_contexts: HashMap<String, Option<DiagFileContext>>,
}

impl<'a> DiagLocationFixer<'a> {
    pub(crate) fn new(
        ctx: &'a DiagContext<'a>,
        original_filename: &'a str,
        dialect_ext: &'a str,
    ) -> Self {
        Self {
            source: ctx.source,
            column_map: ctx.column_map,
            entry_line_map: ctx.entry_line_map,
            project_root: ctx.project_root,
            manager: ctx.manager,
            original_filename,
            entry_canon: ctx.project_root.join("src/main.rs").canonicalize().ok(),
            dialect_ext,
            project: ctx.project,
            file_contexts: HashMap::new(),
        }
    }

    /// 回译一条教学诊断的全部位置（含子诊断）
    fn fix(&mut self, teaching: &mut i18n_rust_engine::diagnostic::TeachingDiagnostic) {
        for loc in &mut teaching.locations {
            self.fix_location(loc);
        }
        for child in &mut teaching.children {
            self.fix(child);
        }
    }

    pub(crate) fn fix_location(
        &mut self,
        loc: &mut i18n_rust_engine::diagnostic::DiagnosticLocation,
    ) {
        if self.is_entry_product(&loc.file_name) {
            loc.file_name = self.original_filename.to_string();
            // 先把 rustc 的（英文产物）行列回译到母语源码坐标，
            // 再用回译后的行号取源码行——顺序不可颠倒，否则源码行与列号错位。
            if let Some(cm) = self.column_map {
                // 磁盘产物含 `#[path]` 注解插入行时，行号先换算回引擎直出行
                //（列映射以引擎直出产物为基准，1-based 换算后传入）
                let engine_line = self
                    .entry_line_map
                    .and_then(|map| map.get(loc.line_start.saturating_sub(1) as usize).copied())
                    .map(|engine| engine as u32 + 1)
                    .unwrap_or(loc.line_start);
                let (line, column) = cm.map_position(engine_line, loc.column_start);
                loc.line_start = line;
                loc.column_start = column;
            }
            loc.source_text = get_chinese_source_line(self.source, loc.line_start);
            return;
        }
        let context = self
            .file_contexts
            .entry(loc.file_name.clone())
            .or_insert_with(|| {
                resolve_dialect_context(
                    self.project_root,
                    self.dialect_ext,
                    &loc.file_name,
                    self.manager,
                    self.project,
                )
            });
        let Some(context) = context else {
            // 非方言文件（手写 .rs、第三方源码等）：保持 rustc 原始定位
            return;
        };
        loc.file_name = context.display_name.clone();
        let (line, column) = context
            .column_map
            .map_position(loc.line_start, loc.column_start);
        loc.line_start = line;
        loc.column_start = column;
        loc.source_text = get_chinese_source_line(&context.source, loc.line_start);
    }

    /// 判断诊断文件是否为入口文件的转译产物（src/main.rs）
    fn is_entry_product(&self, name: &str) -> bool {
        let Some(entry_canon) = self.entry_canon.as_ref() else {
            return false;
        };
        resolve_product_path(self.project_root, name)
            .and_then(|abs| abs.canonicalize().ok())
            .is_some_and(|abs| &abs == entry_canon)
    }
}

/// 从 cargo --message-format=json 输出提取未声明的 crate 名
///
/// 识别 unresolved import（E0432）与 failed to resolve（E0433）诊断，
/// 候选提取复用 engine 共享逻辑（已去重，排除标准库与保留路径）。
pub(crate) fn extract_unresolved_crates(rustc_output: &str) -> Vec<String> {
    use i18n_rust_engine::diagnostic::{is_unresolved_import_message, unresolved_crate_candidates};
    let mut result: Vec<String> = Vec::new();
    for line in rustc_output.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        // cargo JSON 流中诊断嵌套在 compiler-message 条目里；
        // 兼容直接的诊断对象两种形态
        let message_obj = if entry.get("reason").is_some() {
            entry.get("message")
        } else {
            Some(&entry)
        };
        let Some(msg) = message_obj else { continue };
        let level = msg.get("level").and_then(|v| v.as_str()).unwrap_or("");
        if level != "error" {
            continue;
        }
        let code = msg
            .get("code")
            .and_then(|c| c.get("code"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let text = msg.get("message").and_then(|v| v.as_str()).unwrap_or("");
        if !matches!(code, "E0432" | "E0433") && !is_unresolved_import_message(text) {
            continue;
        }
        for seg in unresolved_crate_candidates(text) {
            if !result.contains(&seg) {
                result.push(seg);
            }
        }
        // 码命中但消息文本未命中时（消息格式变化），退化到纯首段提取
        if matches!(code, "E0432" | "E0433") && unresolved_crate_candidates(text).is_empty() {
            use i18n_rust_engine::diagnostic::extract_backtick_first_segments;
            for seg in extract_backtick_first_segments(text) {
                // 非 ASCII 段不是 crate 名（母语标识符误提取），
                // 与 unresolved_crate_candidates 的过滤保持一致
                if !matches!(
                    seg.as_str(),
                    "std" | "core" | "alloc" | "self" | "super" | "crate" | "proc_macro"
                ) && !seg.chars().next().is_some_and(|c| c.is_ascii_digit())
                    && seg.is_ascii()
                    && !result.contains(&seg)
                {
                    result.push(seg);
                }
            }
        }
    }
    result
}

/// 输出未声明依赖的 `rzc add` 提示（无候选时静默）
fn print_dependency_hints(crates: &[String], ui: &ui::Ui) {
    for name in crates {
        eprintln!("{}", ui.f("hint_add_dependency", &[name, name]));
    }
}

fn get_chinese_source_line(source: &str, line_num: u32) -> Option<String> {
    if line_num == 0 {
        return None;
    }
    source
        .lines()
        .nth((line_num - 1) as usize)
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use i18n_rust_engine::module_path::annotate_non_ascii_mods_with_lines;

    /// 加载内置中文映射管理器（测试诊断回译用）
    fn zh_manager() -> i18n_rust_engine::mapping_manager::MappingManager {
        let builtin = crate::builtin_lang::get_builtin_data("zh");
        i18n_rust_engine::mapping_manager::MappingManager::load_from_builtin(
            builtin.keywords_toml,
            builtin.module_paths_toml,
            builtin.stdlib_toml,
            builtin.crates_data,
        )
        .expect("内置中文语言包应可加载")
    }

    /// 单文件直调 rustc 的启用条件：仅 main.zh 且无依赖
    #[test]
    fn test_can_use_direct_rustc_single_file_no_deps() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(root.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        assert!(can_use_direct_rustc(root, &root.join("src/main.zh")));
    }

    /// 有第三方依赖时回退 cargo（依赖行 `rand = \"0.8\"`）
    #[test]
    fn test_can_use_direct_rustc_deps_fall_back() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n[dependencies]\nrand = \"0.8\"\n",
        )
        .unwrap();
        std::fs::write(root.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        assert!(!can_use_direct_rustc(root, &root.join("src/main.zh")));
    }

    /// 多文件项目（src/ 下有第二个方言文件）回退 cargo
    #[test]
    fn test_can_use_direct_rustc_multi_file_fall_back() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(root.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        std::fs::write(
            root.join("src/数学.zh"),
            "函数 加(a: 整数, b: 整数) -> 整数 { a + b }\n",
        )
        .unwrap();
        assert!(!can_use_direct_rustc(root, &root.join("src/main.zh")));
    }

    /// 入口产物含 #[path] 注解时，诊断回译先换算行号再列映射
    ///（回归：多模块 main.zh 中所有诊断行号被注解行整体顶偏移）
    #[test]
    fn test_diag_location_fixer_entry_line_map() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        let source = "// 头\n模块 数学;\n\n函数 主函数() {\n    让 _ = 未定义的名字;\n}\n";
        let entry = root.join("src/main.zh");
        std::fs::write(&entry, source).unwrap();

        let manager = zh_manager();
        let transpiled = i18n_rust_engine::transpile_pipeline(source, &manager);
        let column_map =
            i18n_rust_engine::column_map::ColumnMap::build(source, &transpiled.pipeline_map);
        let (annotated, line_map) = annotate_non_ascii_mods_with_lines(&transpiled.output);
        // 磁盘产物（rustc 看到的就是它）：注解插在第 2 行，其后行号 +1
        std::fs::write(root.join("src/main.rs"), &annotated).unwrap();

        let ui = crate::ui::Ui::for_lang("zh");
        let ctx = DiagContext {
            ui: &ui,
            lang_pack: &None,
            project_root: root,
            manager: &manager,
            source,
            file: &entry,
            column_map: Some(&column_map),
            entry_line_map: Some(&line_map),
            project: None,
        };
        let mut fixer = DiagLocationFixer::new(&ctx, "main.zh", "zh");
        // rustc 报磁盘第 6 行第 13 列（`未定义的名字` 首字符）
        let mut loc = i18n_rust_engine::diagnostic::DiagnosticLocation {
            file_name: "src/main.rs".to_string(),
            line_start: 6,
            column_start: 13,
            line_end: 6,
            column_end: 18,
            source_text: None,
            label: None,
            is_primary: true,
        };
        fixer.fix_location(&mut loc);

        // 回译到母语源码：第 5 行第 11 列（`让` → `let` 的列差已补回）
        assert_eq!(loc.file_name, "main.zh");
        assert_eq!(loc.line_start, 5);
        assert_eq!(loc.column_start, 11);
        assert_eq!(loc.source_text.as_deref(), Some("    让 _ = 未定义的名字;"));
    }

    /// cargo JSON 流中提取未声明 crate：E0432/E0433 命中，标准库与重复项排除
    #[test]
    fn test_extract_unresolved_crates() {
        let line1 = r#"{"reason":"compiler-message","message":{"message":"unresolved import `serde_json`","code":{"code":"E0432"},"level":"error"}}"#;
        let line2 = r#"{"reason":"compiler-message","message":{"message":"failed to resolve: use of undeclared crate or module `tokio`","code":{"code":"E0433"},"level":"error"}}"#;
        let line3 = r#"{"reason":"compiler-message","message":{"message":"unresolved import `std::collections`","code":{"code":"E0432"},"level":"error"}}"#;
        let line4 = r#"{"reason":"compiler-message","message":{"message":"unresolved import `serde_json`","code":{"code":"E0432"},"level":"error"}}"#;
        let line5 = r#"{"reason":"compiler-message","message":{"message":"unused variable `x`","code":{"code":"E0432"},"level":"warning"}}"#;
        let output = [line1, line2, line3, line4, line5].join("\n");
        assert_eq!(
            extract_unresolved_crates(&output),
            vec!["serde_json", "tokio"]
        );
    }
}
