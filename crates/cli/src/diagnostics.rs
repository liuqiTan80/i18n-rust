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
    // 方言文件计数：src/ 全层级递归（多层模块布局）+ 项目根顶层
    //（教学项目 src/main.zh 为主，项目根也可能放 main.zh）；
    // 超过 1 个视为多文件项目
    let mut dialect_count =
        crate::collect_dialect_files(&project_root.join("src"), &extensions).len();
    if let Ok(entries) = fs::read_dir(project_root) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|ft| ft.is_file()) {
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
                entry_product: source_path.to_path_buf(),
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
    // 编译成功：运行程序并传播退出码（无论成败都清理临时 exe）。
    // stderr 改为管道逐行过滤：panic 框头/消息与 cargo 进度本地化，
    // 其余字节原样透传（lossy 解码，不因非 UTF-8 输出中断）；
    // stdout/stdin 保持继承，交互式程序不受影响
    let mut child = match Command::new(&exe)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = std::fs::remove_file(&exe);
            return Err(anyhow::anyhow!("运行失败: {e}"));
        }
    };
    if let Some(stderr) = child.stderr.take() {
        use std::io::{BufRead, BufReader};
        let mut reader = BufReader::new(stderr);
        let mut translator = StreamTranslator::new();
        let mut buf: Vec<u8> = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let raw = String::from_utf8_lossy(&buf);
                    let line = raw.trim_end_matches(['\n', '\r']);
                    eprintln!("{}", translator.translate(line, ui));
                }
            }
        }
    }
    let status = match child.wait() {
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
            entry_product: source_path.to_path_buf(),
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

/// 程序 stderr 流式翻译器：cargo 进度行 + 运行时 panic 输出的本地化
///
/// rustc 1.98 实测的程序 panic 输出形态（框头 → 消息 → note 三行）：
/// ```text
/// thread 'main' (621725) panicked at src/main.rs:8:5:
/// index out of bounds: the len is 3 but the index is 5
/// note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
/// ```
/// 框头：线程名 `main` 映射为各语言主函数词（`<unnamed>` 等自定义名保留）、
/// 线程 ID `(621725) ` 剥离；消息六型（下标越界 / 字节索引越界 ×2 /
/// 字符边界 / RefCell 借用冲突 ×2）按 ui.toml 模板本地化；note 行固定翻译；
/// 其余行回退 cargo 进度翻译（无匹配则原样返回）。
pub(crate) struct StreamTranslator {
    /// 上一行是 panic 框头：下一行按 panic 消息匹配
    expect_panic_message: bool,
    /// cargo 纯文本错误行本地化用消息表（`None` = 只处理 panic/进度行）
    ///
    /// 直调 rustc 运行时管道里只有被运行程序的 stderr，无消息表时
    /// 不得拿 `error:` 行去查表（会把程序自己打印的错误误当 cargo 错误）。
    tool_manager: Option<i18n_rust_engine::diagnostic::ErrorTranslationManager>,
    /// 待译清单登记语言（空串取当前界面语言）
    gap_lang: String,
    /// 程序已开始运行（见过 `Running` 进度行）：之后的 `error:` 行属于程序输出
    program_started: bool,
}

impl StreamTranslator {
    pub(crate) fn new() -> Self {
        Self {
            expect_panic_message: false,
            tool_manager: None,
            gap_lang: String::new(),
            program_started: false,
        }
    }

    /// 启用 cargo 纯文本错误行本地化（仅 `cargo run` 流式透传通道使用）
    pub(crate) fn with_tool_text(
        mut self,
        manager: i18n_rust_engine::diagnostic::ErrorTranslationManager,
        gap_lang: impl Into<String>,
    ) -> Self {
        self.tool_manager = Some(manager);
        self.gap_lang = gap_lang.into();
        self
    }

    /// 翻译一行程序/cargo stderr（无匹配时原样返回）
    pub(crate) fn translate(&mut self, line: &str, ui: &ui::Ui) -> String {
        // 框头之后的第一行是 panic 消息；无论命中与否都退出该状态
        if std::mem::take(&mut self.expect_panic_message)
            && let Some(msg) = translate_panic_message(line, ui)
        {
            return msg;
        }
        if let Some(header) = translate_panic_header(line, ui) {
            self.expect_panic_message = true;
            return header;
        }
        if line.trim_end() == PANIC_NOTE {
            return ui.t("panic_note");
        }
        // cargo 的运行标记之后，stderr 属于被运行的程序：
        // 不再按 cargo 自身错误处理（避免误译程序打印的 `error:` 行）
        if line.trim_start().starts_with("Running ") {
            self.program_started = true;
        } else if !self.program_started
            && let Some(localized) =
                localize_tool_line(line, self.tool_manager.as_ref(), ui, &self.gap_lang)
        {
            return localized;
        }
        translate_cargo_progress(line, ui)
    }
}

/// 程序 panic 输出末尾的 note 行（rustc 稳定文本，原样匹配）
const PANIC_NOTE: &str =
    "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace";

/// 翻译 panic 框头 `thread 'NAME' [(TID)] panicked at FILE:LINE:COL:`
///
/// 返回 None 表示该行不是 panic 框头（由调用方原样透传）。
fn translate_panic_header(line: &str, ui: &ui::Ui) -> Option<String> {
    let rest = line.trim_start().strip_prefix("thread '")?;
    let (name, after_name) = rest.split_once('\'')?;
    // rustc 1.98 起框头含线程 ID（如 `thread 'main' (621725) panicked at ...`），
    // 旧格式无 ID；两种形态都归一为「name + panicked at」
    let after = match after_name.strip_prefix(" (") {
        Some(id_and_rest) => id_and_rest
            .split_once(") ")
            .map(|(_, rest)| rest)
            .unwrap_or(after_name),
        None => after_name,
    };
    // 剥离线程 ID 后 `after` 以 `panicked at ...` 开头（无 ID 时带前导空格），
    // trim_start 统一两种形态后再匹配
    let loc = after.trim_start().strip_prefix("panicked at ")?;
    let loc = loc.trim_end().strip_suffix(':')?;
    if name.is_empty() || loc.is_empty() {
        return None;
    }
    let display_name = if name == "main" {
        ui.t("panic_thread_main")
    } else {
        name.to_string()
    };
    Some(ui.f("panic_header", &[&display_name, loc]))
}

/// 翻译 panic 消息（六型；未识别返回 None 由调用方原样透传）
fn translate_panic_message(line: &str, ui: &ui::Ui) -> Option<String> {
    let line = line.trim();
    if line == "RefCell already borrowed" {
        return Some(ui.t("panic_msg_borrow"));
    }
    if line == "RefCell already mutably borrowed" {
        return Some(ui.t("panic_msg_borrow_mut"));
    }
    if let Some(rest) = line.strip_prefix("index out of bounds: the len is ") {
        let (len_s, idx_s) = rest.split_once(" but the index is ")?;
        return Some(ui.f("panic_msg_index_oob", &[len_s.trim(), idx_s.trim()]));
    }
    if let Some(rest) = line.strip_prefix("start byte index ") {
        return translate_byte_index_message(rest, ui, true);
    }
    if let Some(rest) = line.strip_prefix("end byte index ") {
        return translate_byte_index_message(rest, ui, false);
    }
    None
}

/// 字节索引 panic 消息：越界（`... is out of bounds for string of length N`）或
/// 非字符边界（`... is not a char boundary; it is inside 'C' (bytes A..B of string)`）
fn translate_byte_index_message(rest: &str, ui: &ui::Ui, is_start: bool) -> Option<String> {
    let (idx_s, tail) = rest.split_once(' ')?;
    if let Some(len_s) = tail.strip_prefix("is out of bounds for string of length ") {
        let key = if is_start {
            "panic_msg_byte_start_oob"
        } else {
            "panic_msg_byte_end_oob"
        };
        return Some(ui.f(key, &[idx_s.trim(), len_s.trim()]));
    }
    if let Some(inside) = tail.strip_prefix("is not a char boundary; it is inside '") {
        let (ch, range_s) = inside.split_once("' (bytes ")?;
        let (a, b) = range_s.split_once("..")?;
        let b = b.strip_suffix(" of string)")?;
        return Some(ui.f(
            "panic_msg_char_boundary",
            &[idx_s.trim(), ch, a.trim(), b.trim()],
        ));
    }
    None
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
    /// 本次实际编译的**入口产物**路径（请求文件的转译产物，如 `src/main.rs`、
    /// `src/工具.rs`、`examples/演示.rs`）。
    ///
    /// 诊断归属按「被编译目标」而非「请求文件」判定：`rzc check src/工具.zh`
    /// 时 cargo 仍编译整个目标，`src/main.rs` 里的报错属于 `main.zh`，
    /// 不能因为用户请求的是 `工具.zh` 就把它标成 `工具.zh`（旧实现硬编码
    /// `src/main.rs` 作入口，等于把入口钉死在默认 main 上）。
    pub(crate) entry_product: PathBuf,
    /// 转译产物的列映射：把 rustc 诊断的（英文产物）列号回译到母语源码列号。
    ///
    /// rustc 看到的是转译后的英文源码，其列号对母语源码无效
    ///（如 `让` → `let` 后整行右移）。`None` 表示不做映射，保持旧行为。
    pub(crate) column_map: Option<&'a i18n_rust_engine::column_map::ColumnMap>,
    /// 入口磁盘产物行 → 引擎直出行映射（见
    /// [`annotate_product`](crate::annotate_product)）。
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
    let handled = translate_cargo_diagnostics_impl(
        rustc_output,
        stderr_text,
        ctx,
        cargo_ok,
        silent_success,
        streamed_output,
    );
    // 本轮若有英文残留（未命中消息表），统一提示一次并落盘待译清单；
    // 逐条加标记会污染诊断输出，故只在本轮结束时汇总
    report_translation_gaps(ctx.ui);
    handled
}

/// 提示并持久化本轮登记的英文残留（无登记时静默）
///
/// 单元测试进程只清空登记表不落盘：测试用样例语言代码登记的残留不该写进
/// 用户主目录（落盘/读取/去重逻辑另有 tempdir 用例覆盖，见 `diagnostic::gaps`）。
#[allow(unused_variables, clippy::needless_pass_by_value)]
pub(crate) fn report_translation_gaps(ui: &ui::Ui) {
    use i18n_rust_engine::diagnostic::gaps;
    let recorded = gaps::drain_gaps();
    if recorded.is_empty() {
        return;
    }
    #[cfg(test)]
    return;
    #[cfg(not(test))]
    {
        let dir = gaps::gaps_dir();
        gaps::flush_gaps_to_dir(&dir, &recorded);
        eprintln!(
            "{}",
            ui.f(
                "translation_gap_notice",
                &[&recorded.len().to_string(), &dir.display().to_string()]
            )
        );
    }
}

/// cargo 纯文本错误行的前缀与本地化模板（模板含 `{}` 占位符，与进度行同源）
///
/// `help:` 是工具给的可执行下一步（如 rustup 未配默认工具链时的
/// `run 'rustup default stable'`），不接就只剩一行英文指引留在母语输出里
const TOOL_LINE_MARKERS: [(&str, &str); 5] = [
    ("error:", "cargo_progress_error"),
    ("warning:", "tool_prefix_warning"),
    ("note:", "tool_prefix_note"),
    ("help:", "tool_prefix_help"),
    ("Caused by:", "tool_prefix_caused_by"),
];

/// 交回进度行翻译器处理的 cargo 固定句（已有专门本地化，补表无意义）
const TOOL_LINE_SKIP: [&str; 2] = ["could not compile ", "aborting due to "];

/// 按请求文件扩展名取诊断语言代码（无扩展名/未知时跟随界面语言）
pub(crate) fn diag_lang_code(file: &Path) -> String {
    file.extension()
        .and_then(|e| e.to_str())
        .and_then(get_lang_code_from_extension)
        .unwrap_or_else(ui::detect_ui_lang)
}

/// 加载消息表：`--lang-pack` 目录 > 项目内 `lang-packs/<lang>/` > 全局语言目录 > 内置
///
/// 文件不存在或加载失败时降级到内置表（只警告不阻断）；返回 `None`
/// 意味着连内置表都无法解析。
pub(crate) fn load_error_manager(
    ui: &ui::Ui,
    lang_pack: &Option<PathBuf>,
    project_root: &Path,
    lang_code: &str,
) -> Option<i18n_rust_engine::diagnostic::ErrorTranslationManager> {
    use i18n_rust_engine::diagnostic::ErrorTranslationManager;
    let error_msg_path = if let Some(path) = lang_pack {
        path.join("errors.toml")
    } else if lang_pack_root_of(project_root)
        .join(lang_code)
        .join("errors.toml")
        .exists()
    {
        lang_pack_root_of(project_root)
            .join(lang_code)
            .join("errors.toml")
    } else {
        lang_manager::global_lang_dir()
            .join(lang_code)
            .join("errors.toml")
    };
    if error_msg_path.exists() {
        // 加载失败时降级到内置表，不因错误消息文件损坏阻断诊断展示
        match ErrorTranslationManager::load_from_file(&error_msg_path) {
            Ok(manager) => return Some(manager),
            Err(e) => eprintln!("{}", ui.f("load_error_msg_failed", &[&e.to_string()])),
        }
    }
    // 文件路径不可用/加载失败时回退内置语言包（未知语言代码自动回退中文）
    let builtin = builtin_lang::get_builtin_data(lang_code);
    match ErrorTranslationManager::load_from_string(builtin.errors_toml) {
        Ok(manager) => Some(manager),
        Err(e) => {
            eprintln!("{}", ui.f("warn_builtin_errors_failed", &[&e.to_string()]));
            None
        }
    }
}

/// 用消息表渲染一句工具错误正文
///
/// 返回 `None` 表示未命中且不兜底（调用方继续原流程）；`record_miss` 为真时
/// 未命中返回原文（绝不吞诊断）并登记待译清单。判「是否已译」只看消息表
/// 是否命中，不看渲染后还剩多少英文词：德/法/西/葡等拉丁语系译文里夹着
/// 路径与 crate 名是常态，按词串判会把已译好的句子打回英文。JSON 通道的
/// 登记点同此口径（见 translator 的 `note_gap`：未命中或残段未被 `{qN}` 消费才记）。
fn render_tool_detail(
    detail: &str,
    manager: &i18n_rust_engine::diagnostic::ErrorTranslationManager,
    gap_lang: &str,
    record_miss: bool,
) -> Option<String> {
    use i18n_rust_engine::diagnostic::{fill_dynamic_placeholders, gaps};
    let Some((entry, rest)) = manager.query_by_message(detail) else {
        if !record_miss {
            return None;
        }
        gaps::record_gap(gap_lang, gaps::LEVEL_TOOL, "", detail);
        return Some(detail.to_string());
    };
    let mut text = entry.message_template.clone();
    if let Some(rest) = rest {
        let (filled, consumed) = fill_dynamic_placeholders(&text, &rest);
        text = filled;
        if !consumed {
            // 模板没有对应占位符，英文残段拼回尾巴（兜底）→ 记待译
            text.push_str(rest.text());
            gaps::record_gap(gap_lang, gaps::LEVEL_TOOL, "", detail);
        }
    }
    Some(text)
}

/// 本地化单行 cargo 纯文本错误；非错误形态行返回 `None`（调用方继续原流程）
///
/// 保留行首缩进；`manager` 为 `None`（无消息表）时不处理，避免把
/// 被运行程序自己打印的 `error:` 行误标。已知固定汇总句返回 `None`，
/// 由调用方交给进度行翻译器（那里已有 "could not compile" 的专门处理）。
fn localize_tool_line(
    line: &str,
    manager: Option<&i18n_rust_engine::diagnostic::ErrorTranslationManager>,
    ui: &ui::Ui,
    gap_lang: &str,
) -> Option<String> {
    let manager = manager?;
    let indent_len = line.len() - line.trim_start().len();
    let body = line.trim_start();
    let (marker, key) = match TOOL_LINE_MARKERS
        .iter()
        .find(|(prefix, _)| body.starts_with(prefix))
    {
        Some((marker, key)) => (*marker, *key),
        // 无标记的缩进行：cargo 把错误链的因由正文写在 `Caused by:` 下一行。
        // 只在消息表命中时换译，未命中交回进度行翻译（缩进输出也可能是
        // 被运行程序自己的打印，不能当成工具错误登记待译）
        None if indent_len > 0 => {
            return render_tool_detail(body, manager, gap_lang, false)
                .map(|text| format!("{}{}", " ".repeat(indent_len), text));
        }
        None => return None,
    };
    let detail = body[marker.len()..].trim_start();
    if detail.is_empty() {
        // 光杆标记行（cargo 的 `Caused by:` 单独占一行）：只换标签，正文在缩进续行
        return Some(ui.f(key, &[""]).trim_end().to_string());
    }
    if TOOL_LINE_SKIP.iter().any(|skip| detail.starts_with(skip)) {
        return None;
    }
    let rendered = render_tool_detail(detail, manager, gap_lang, true)?;
    Some(format!(
        "{}{}",
        " ".repeat(indent_len),
        ui.f(key, &[&rendered])
    ))
}

/// 本地化 cargo 自身错误（非 JSON 通道：manifest 解析、无 target、依赖拉不到等）
///
/// rustc 诊断走 JSON 翻译链，cargo 自己的报错是纯文本，旧行为原样输出英文——
/// 新手最先撞的就是这一类（不在项目里、Cargo.toml 写坏、没声明编译目标）。
///
/// 逐行交给 [`localize_tool_line`]：命中消息表则换本地化前缀输出译文，
/// 未命中保留原文并登记待译清单（`tool` 通道）。无标记前缀的辅助行
///（如缩进后的补充说明）不单独登记，避免把进度行刷进清单。
pub(crate) fn localize_tool_text(
    text: &str,
    manager: &i18n_rust_engine::diagnostic::ErrorTranslationManager,
    ui: &ui::Ui,
    gap_lang: &str,
) -> String {
    let mut out = String::with_capacity(text.len());
    for (idx, raw) in text.lines().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        match localize_tool_line(raw, Some(manager), ui, gap_lang) {
            Some(localized) => out.push_str(&localized),
            None => out.push_str(&translate_cargo_progress(raw, ui)),
        }
    }
    out
}

fn translate_cargo_diagnostics_impl(
    rustc_output: &str,
    stderr_text: &str,
    ctx: &DiagContext<'_>,
    cargo_ok: bool,
    silent_success: bool,
    streamed_output: bool,
) -> bool {
    use i18n_rust_engine::diagnostic::{
        DiagnosticTranslator, build_type_map, parse_diagnostic_output, suggested_crate_name,
    };
    let ui = ctx.ui;
    let lang_pack = ctx.lang_pack;
    let project_root = ctx.project_root;
    let manager = ctx.manager;
    let file = ctx.file;

    // 未解析导入提取：诊断展示后附带 `rzc add` 加依赖提示（教学化引导）
    let unresolved_crates = extract_unresolved_crates(rustc_output);
    // 本项目自身可 `use` 的 crate 名（包名 / `[lib] name`）：命中它的候选不能
    // 建议 `rzc add`（Cargo 拒绝包依赖自己），见 [`print_dependency_hints`]
    let local_names = local_crate_names(project_root);
    // 库入口源文件按请求文件的扩展名推断（`src/main.zh` → `src/lib.zh`）：
    // 同项目内的方言文件通常同一种语言，且 `lib` 正是方言入口名之一
    let lib_entry = file.extension().and_then(|e| e.to_str()).map(|ext| {
        let lib_src = project_root.join("src").join(format!("lib.{ext}"));
        (format!("src/lib.{ext}"), lib_src.is_file())
    });
    let dep_hints = || {
        print_dependency_hints(
            &unresolved_crates,
            ui,
            &local_names,
            lib_entry
                .as_ref()
                .map(|(path, exists)| (path.as_str(), *exists)),
        )
    };

    // 按语言代码选择错误消息表（与 cargo 纯文本通道共用同一张表）
    let lang_code = diag_lang_code(file);
    // 类型映射（英文 → 母语）：供诊断消息中的类型/特征名中文化（如
    // `std::fmt::Display` → `标准库::格式化::可显示`）。构建逻辑下沉引擎，
    // 与 LSP 共用同一来源，避免两侧漂移（keywords [类型] 反转为主，别名/模块路径补充）。
    let reverse_map = build_type_map(manager);
    let translator =
        load_error_manager(ui, lang_pack, project_root, &lang_code).map(|translation_manager| {
            DiagnosticTranslator::new(translation_manager, reverse_map.clone())
                .with_gap_lang(lang_code.clone())
        });

    // cargo 自身错误（纯文本通道）本地化：manifest 写坏、不在项目里、
    // 未声明编译目标这类新手高频错误旧行为是直接喷英文
    let cargo_error_text = || match &translator {
        Some(t) => localize_tool_text(stderr_text.trim(), t.translation_manager(), ui, &lang_code),
        None => stderr_text.trim().to_string(),
    };

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

    // rustc 的「若要使用名为 X 的第三方库，请 `cargo add X`」建议：X 命中本项目
    // 包名时是**错建议**——Cargo 不允许一个包依赖自己，读者照着做只会撞
    // `cannot add `calc` as a dependency to itself`。引擎侧没有项目上下文（不知道
    // 包名），故在 CLI 按本地 crate 名剔除这条子诊断；真能走通的动作由
    // `dep_hints()` 给出。引擎已按同一形状丢弃非 ASCII 名（母语模块误提取），
    // 这里复用它的提取函数，不另写一套解析。
    if !local_names.is_empty() {
        for diag in &mut diagnostics {
            diag.children.retain(|child| {
                !suggested_crate_name(&child.message)
                    .is_some_and(|name| local_names.iter().any(|local| local == name))
            });
        }
    }

    if diagnostics.is_empty() {
        if cargo_ok {
            if !silent_success {
                println!("{}", ui.t("success_compile"));
            }
        } else if !streamed_output {
            // cargo 失败但无可解析的 JSON 诊断（Cargo.toml 语法错误、
            // 链接错误等）：本地化后输出 cargo 消息，绝不虚报“编译成功”；
            // streamed_output 场景跳过（见函数文档，避免运行时失败被误标）
            eprintln!("{}", ui.f("compile_error", &[&cargo_error_text()]));
        }
        dep_hints();
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
                eprintln!("{}", ui.f("compile_error", &[&cargo_error_text()]));
            }
            dep_hints();
            return false;
        }
        println!(
            "{}",
            i18n_rust_engine::diagnostic::TeachingDiagnostic::batch_format_as_text(&teaching_list)
        );
        dep_hints();
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
        dep_hints();
        false
    }
}

/// 诊断文件上下文：rustc 报告的（英文产物）文件解析回母语源文件后
/// 得到的显示名、源码与列映射，供行号/列号/源行统一回译
struct DiagFileContext {
    display_name: String,
    source: String,
    column_map: i18n_rust_engine::column_map::ColumnMap,
    /// 磁盘产物行 → 引擎直出行映射（`#[path]` 注解插入行时非恒等）。
    ///
    /// 托管布局下入口与子模块产物都由
    /// [`annotate_product_mod_paths_with_lines`](i18n_rust_engine::product_layout::annotate_product_mod_paths_with_lines)
    /// 注解，插入行数随该文件的 `mod` 声明而异，故按产物自身重算。
    line_map: Vec<usize>,
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
    // 托管布局：rustc 报的是 `.rz/gen/src/接口.rs`（且父目录与 `#[path]` 值
    // 直接拼接，形如 `src/../.rz/gen/src/接口.rs`），剥掉托管前缀才是源文件
    // 所在的源码树位置，否则同名方言源文件永远找不到（诊断停在英文坐标）
    let abs = i18n_rust_engine::product_layout::strip_gen_prefix(project_root, &abs).unwrap_or(abs);
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
    // 子模块产物写盘时按托管布局插入了 `#[path]` 行：列映射以未注解的引擎
    // 直出产物为基准，须先用同一函数的行映射换算回直出行号，否则注解越多、
    // 行号偏移越大（多文件项目下的错行）。产物路径按同一规则重算，
    // 与 `transpile_project_files` 写盘时的注解保持一致。
    let product = i18n_rust_engine::product_layout::gen_product_path(project_root, &source_path);
    let line_map = {
        let extensions = lang_manager::all_available_extensions();
        crate::annotate_product(
            &transpiled.output,
            project_root,
            &source_path,
            &product,
            false,
            &extensions,
        )
        .1
    };
    Some(DiagFileContext {
        display_name: source_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        source,
        column_map,
        line_map,
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
    /// 被编译入口产物的规范路径（本次请求文件的产物），用于识别入口产物的诊断
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
            // 按被编译目标而非固定 `src/main.rs`：请求文件非入口时（如
            // `rzc check src/工具.zh`，产物 src/工具.rs），入口产物是它自己
            entry_canon: ctx.entry_product.canonicalize().ok(),
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
        // 注解插入行先行换算（1-based 映射），再回译列号
        let engine_line = context
            .line_map
            .get(loc.line_start.saturating_sub(1) as usize)
            .copied()
            .map(|engine| engine as u32 + 1)
            .unwrap_or(loc.line_start);
        let (line, column) = context
            .column_map
            .map_position(engine_line, loc.column_start);
        loc.line_start = line;
        loc.column_start = column;
        loc.source_text = get_chinese_source_line(&context.source, loc.line_start);
    }

    /// 判断诊断文件是否为本次编译入口（请求文件）的转译产物
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

/// 本项目自身可被 `use` 的 crate 名（`[package] name` 与 `[lib] name`）
///
/// Cargo 把包名里的 `-` 换成 `_` 当作 crate 名，`[lib] name` 可再覆盖一次。
/// rustc 分不出「自己的库还没生成产物」和「没声明的第三方库」，两种情况都报
/// E0432 并建议 `cargo add`——拿包名去加依赖会被 Cargo 直接拒绝
/// （实测 `error: cannot add `calc` as a dependency to itself`），所以这条建议
/// 在本地包名上**必然失败**。无 Cargo.toml 或 TOML 非法时返回空，不改任何行为。
fn local_crate_names(project_root: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(text) = fs::read_to_string(project_root.join("Cargo.toml")) else {
        return names;
    };
    let Ok(manifest) = toml::from_str::<toml::Value>(&text) else {
        return names;
    };
    let raw = [
        manifest
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|v| v.as_str()),
        manifest
            .get("lib")
            .and_then(|l| l.get("name"))
            .and_then(|v| v.as_str()),
    ];
    for name in raw.into_iter().flatten() {
        let crate_name = name.replace('-', "_");
        if !names.contains(&crate_name) {
            names.push(crate_name);
        }
    }
    names
}

/// 输出未解析导入的加依赖提示（无候选时静默）
///
/// 候选命中本项目包名时**不能**建议 `rzc add`（见 [`local_crate_names`]）：
/// 那样只会把读者引向一条 Cargo 必然拒绝的命令。此时按库入口在不在分两种说法——
/// 有 `src/lib.<扩展名>` 就指向「先 check 它生成 src/lib.rs」，没有就点明这个文件
/// 还不存在、`use 包名::…` 引用的正是它编译出的库。其余候选照旧走第三方库提示。
fn print_dependency_hints(
    crates: &[String],
    ui: &ui::Ui,
    local_names: &[String],
    lib_entry: Option<(&str, bool)>,
) {
    for name in crates {
        if !local_names.iter().any(|local| local == name) {
            eprintln!("{}", ui.f("hint_add_dependency", &[name, name]));
            continue;
        }
        // 本地包名：只给「生成/新建库入口」这一条真能走通的路。拿不到扩展名
        // （无从推断 `src/lib.<ext>`）时宁可不说话，也不给一条 Cargo 会拒绝的命令。
        let Some((path, exists)) = lib_entry else {
            continue;
        };
        if exists {
            eprintln!("{}", ui.f("hint_own_crate_lib", &[name, name, path]));
        } else {
            eprintln!(
                "{}",
                ui.f("hint_own_crate_no_lib", &[name, name, path, path])
            );
        }
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

    /// 测试用界面消息：直接读仓库语言包（绕开 ~/.rz 全局安装副本的
    /// 缺键干扰，保证断言与仓库内容一致）
    fn ui_for_test(lang: &str) -> crate::ui::Ui {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../engine/lang-packs")
            .join(lang);
        crate::ui::Ui::for_explicit_dir(&dir)
    }

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
            entry_product: entry.with_extension("rs"),
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

    /// 托管布局子模块产物（`.rz/gen/src/…`）的诊断仍须回译到母语源文件
    ///
    /// 回归：④ 生成物迁出 `src/` 后，rustc 报的是托管产物路径，且是父目录与
    /// `#[path]` 值的未规整拼接（`src/../.rz/gen/src/数学.rs`）；剥前缀 +
    /// 按产物自身重算行映射缺一不可，否则诊断停在英文坐标。
    #[test]
    fn test_diag_location_fixer_gen_product_backtranslate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/数学")).unwrap();
        std::fs::create_dir_all(root.join(".rz/gen/src")).unwrap();
        // 子模块自己还有子模块：产物会插入 `#[path]` 行，行映射非恒等
        std::fs::write(
            root.join("src/数学/工具.zh"),
            "函数 恒等(a: 整数) -> 整数 { a }\n",
        )
        .unwrap();
        let module_source = "模块 工具;\n\n函数 加(a: 整数, b: 整数) -> 整数 {\n    让 _ = 未定义的名字;\n    a + b\n}\n";
        let module_file = root.join("src/数学.zh");
        std::fs::write(&module_file, module_source).unwrap();

        let manager = zh_manager();
        let transpiled = i18n_rust_engine::transpile_pipeline(module_source, &manager);
        let product = i18n_rust_engine::product_layout::gen_product_path(root, &module_file);
        assert_eq!(product, root.join(".rz/gen/src/数学.rs"));
        let extensions = lang_manager::all_available_extensions();
        let (annotated, line_map) = crate::annotate_product(
            &transpiled.output,
            root,
            &module_file,
            &product,
            false,
            &extensions,
        );
        // 托管目录镜像源码布局：子模块之间仍与旧布局逐字一致（`数学/工具.rs`）
        assert!(
            annotated.contains("#[path = \"数学/工具.rs\"]"),
            "{annotated}"
        );
        std::fs::write(&product, &annotated).unwrap();
        // 源文件里报错在第 4 行，磁盘产物被 `#[path]` 顶到第 5 行
        let disk_line = annotated
            .lines()
            .position(|l| l.contains("未定义的名字"))
            .expect("产物应含报错行")
            + 1;
        assert_eq!(disk_line, 5);
        assert_ne!(line_map[4], 5, "行映射应把磁盘行换算回直出行");

        let entry = root.join("src/main.zh");
        std::fs::write(&entry, "函数 主函数() {}\n").unwrap();
        let entry_source = "函数 主函数() {}\n";
        let ui = crate::ui::Ui::for_lang("zh");
        let ctx = DiagContext {
            ui: &ui,
            lang_pack: &None,
            project_root: root,
            manager: &manager,
            source: entry_source,
            file: &entry,
            entry_product: entry.with_extension("rs"),
            column_map: None,
            entry_line_map: None,
            project: None,
        };
        let mut fixer = DiagLocationFixer::new(&ctx, "main.zh", "zh");
        let mut loc = i18n_rust_engine::diagnostic::DiagnosticLocation {
            file_name: "src/../.rz/gen/src/数学.rs".to_string(),
            line_start: disk_line as u32,
            column_start: 13,
            line_end: disk_line as u32,
            column_end: 18,
            source_text: None,
            label: None,
            is_primary: true,
        };
        fixer.fix_location(&mut loc);

        assert_eq!(loc.file_name, "数学.zh");
        assert_eq!(loc.line_start, 4);
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

    /// 本项目可 `use` 的 crate 名：包名按 Cargo 规则把 `-` 换成 `_`，`[lib] name` 另计
    ///
    /// 这组名决定「`use X::…` 解析失败」该不该建议 `rzc add X`：Cargo 不允许一个包
    /// 依赖自己，命中本地名时那条命令必然失败。清单读不出来（无 Cargo.toml、TOML
    /// 非法）时返回空，等于不改任何既有行为，而不是猜一个名字去压制正确提示。
    #[test]
    fn test_local_crate_names() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"my-calc\"\nversion = \"0.1.0\"\n\n[lib]\nname = \"calc_lib\"\n",
        )
        .unwrap();
        assert_eq!(local_crate_names(root), vec!["my_calc", "calc_lib"]);

        // 无 [lib] 时只有包名一种叫法
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"calc\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        assert_eq!(local_crate_names(root), vec!["calc"]);

        std::fs::write(root.join("Cargo.toml"), "[package]\nname = ").unwrap();
        assert!(
            local_crate_names(root).is_empty(),
            "非法 TOML 不该给出任何名字"
        );
        assert!(
            local_crate_names(&root.join("nope")).is_empty(),
            "无清单不该报错"
        );
    }

    /// 流式翻译器：panic 三行序列（含线程 ID 框头 → 消息 → note）本地化
    #[test]
    fn test_stream_translator_panic_sequence() {
        let ui = ui_for_test("zh");
        let mut t = StreamTranslator::new();
        assert_eq!(
            t.translate("thread 'main' (621725) panicked at a.rs:1:50:", &ui),
            "线程 '主函数' 恐慌于 a.rs:1:50:"
        );
        assert_eq!(
            t.translate("index out of bounds: the len is 3 but the index is 5", &ui),
            "下标越界：长度是 3，但下标是 5"
        );
        assert_eq!(
            t.translate(
                "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace",
                &ui
            ),
            "提示：设置 RUST_BACKTRACE=1 环境变量可显示回溯"
        );
    }

    /// 框头变体：旧格式（无线程 ID）与命名线程（`<unnamed>` 保留不译）
    #[test]
    fn test_stream_translator_panic_header_variants() {
        let ui = ui_for_test("zh");
        let mut t = StreamTranslator::new();
        assert_eq!(
            t.translate("thread 'main' panicked at b.rs:2:3:", &ui),
            "线程 '主函数' 恐慌于 b.rs:2:3:"
        );
        assert_eq!(
            t.translate("thread '<unnamed>' (42) panicked at c.rs:9:9:", &ui),
            "线程 '<unnamed>' 恐慌于 c.rs:9:9:"
        );
        // 消息未识别（如 unwrap None）：原样透传，且不残留状态影响后续行
        assert_eq!(
            t.translate("called `Option::unwrap()` on a `None` value", &ui),
            "called `Option::unwrap()` on a `None` value"
        );
    }

    /// 六型 panic 消息：越界 start/end、字符边界、RefCell 借用冲突两向
    #[test]
    fn test_stream_translator_panic_messages() {
        let ui = ui_for_test("zh");
        let header = "thread 'main' panicked at a.rs:1:1:";
        let mut t = StreamTranslator::new();
        let _ = t.translate(header, &ui);
        assert_eq!(
            t.translate(
                "start byte index 10 is out of bounds for string of length 6",
                &ui
            ),
            "切片起点字节索引 10 超出字符串长度 6"
        );
        let mut t = StreamTranslator::new();
        let _ = t.translate(header, &ui);
        assert_eq!(
            t.translate(
                "end byte index 10 is out of bounds for string of length 6",
                &ui
            ),
            "切片终点字节索引 10 超出字符串长度 6"
        );
        let mut t = StreamTranslator::new();
        let _ = t.translate(header, &ui);
        assert_eq!(
            t.translate(
                "start byte index 1 is not a char boundary; it is inside '你' (bytes 0..3 of string)",
                &ui
            ),
            "字节索引 1 不是字符边界；它位于 '你'（字节 0..3）内部"
        );
        let mut t = StreamTranslator::new();
        let _ = t.translate(header, &ui);
        assert_eq!(
            t.translate("RefCell already borrowed", &ui),
            "引用单元格已被借用"
        );
        let mut t = StreamTranslator::new();
        let _ = t.translate(header, &ui);
        assert_eq!(
            t.translate("RefCell already mutably borrowed", &ui),
            "引用单元格已被可变借用"
        );
    }

    /// en 界面：panic 输出除线程 ID 剥离外保持英文原文（不引入本地化偏差）
    #[test]
    fn test_stream_translator_en_unchanged() {
        let ui = ui_for_test("en");
        let mut t = StreamTranslator::new();
        assert_eq!(
            t.translate("thread 'main' (621725) panicked at a.rs:1:50:", &ui),
            "thread 'main' panicked at a.rs:1:50:"
        );
        assert_eq!(
            t.translate("index out of bounds: the len is 3 but the index is 5", &ui),
            "index out of bounds: the len is 3 but the index is 5"
        );
    }

    /// 非 panic 行回退 cargo 进度翻译（Finished 等本地化）
    #[test]
    fn test_stream_translator_falls_back_to_cargo_progress() {
        let ui = ui_for_test("zh");
        let mut t = StreamTranslator::new();
        let out = t.translate("    Finished `dev` profile [unoptimized]", &ui);
        assert!(out.contains("编译完成"), "应本地化进度行：{out}");
    }

    /// cargo 进度五种前缀：编译/检查/完成/运行/错误摘要均本地化，
    /// 错误摘要的 "could not compile … due to N" 二段短语也翻译；
    /// 无匹配行与非 could-not-compile 的 error: 原样透传
    #[test]
    fn test_translate_cargo_progress_prefixes() {
        let ui = ui_for_test("zh");
        assert!(
            translate_cargo_progress("   Compiling serde v1.0.0", &ui).contains("正在编译 serde")
        );
        assert!(
            translate_cargo_progress("    Checking myapp v0.1.0", &ui).contains("正在检查 myapp")
        );
        assert!(translate_cargo_progress("Running `target/debug/t`", &ui).contains("正在运行"));
        let due = translate_cargo_progress(
            "error: could not compile `myapp` (bin \"myapp\") due to 2 previous errors",
            &ui,
        );
        assert!(due.contains("无法编译"), "应翻译无法编译：{due}");
        assert!(due.contains("2"), "应保留错误数量：{due}");
        // could not compile 无 due to 尾段
        assert!(
            translate_cargo_progress("error: could not compile `myapp`", &ui).contains("无法编译")
        );
        // error: 但不是 could-not-compile：rest 原样保留
        let raw = translate_cargo_progress("error: some custom failure", &ui);
        assert!(
            raw.contains("some custom failure"),
            "未识别错误摘要应保留：{raw}"
        );
        // 完全无匹配：原样返回
        assert_eq!(translate_cargo_progress("hello world", &ui), "hello world");
    }

    /// 框头 None 分支：缺尾冒号 / 空线程名 / 非框头行均不匹配，原样透传
    #[test]
    fn test_translate_panic_header_reject_malformed() {
        let ui = ui_for_test("zh");
        assert!(translate_panic_header("thread 'main' panicked at a.rs:1:1", &ui).is_none());
        assert!(translate_panic_header("thread '' panicked at a.rs:1:1:", &ui).is_none());
        assert!(translate_panic_header("not a panic line", &ui).is_none());
        // 字节索引消息：缺尾结构时返回 None（不硬译半截）
        assert!(translate_byte_index_message("10 unexpected tail", &ui, true).is_none());
        assert!(translate_panic_message("totally unknown message", &ui).is_none());
    }

    /// 源码行取值：0 行与越界返回 None，正常行返回原文
    #[test]
    fn test_get_chinese_source_line_bounds() {
        let src = "第一行\n第二行\n";
        assert_eq!(get_chinese_source_line(src, 0), None);
        assert_eq!(get_chinese_source_line(src, 1).as_deref(), Some("第一行"));
        assert_eq!(get_chinese_source_line(src, 3), None);
    }

    /// 产物路径解析：绝对路径原样；相对路径先查项目根；不存在再回退当前目录
    #[test]
    fn test_resolve_product_path_order() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let abs = if cfg!(windows) {
            std::path::PathBuf::from("C:/abs/main.rs")
        } else {
            std::path::PathBuf::from("/abs/main.rs")
        };
        assert_eq!(
            resolve_product_path(root, &abs.to_string_lossy()),
            Some(abs)
        );
        std::fs::write(root.join("in-root.rs"), b"").unwrap();
        assert_eq!(
            resolve_product_path(root, "in-root.rs"),
            Some(root.join("in-root.rs"))
        );
        // 项目根下不存在：回退当前工作目录拼接（始终 Some）
        let got = resolve_product_path(root, "nowhere-12345.rs").expect("应回退 cwd");
        assert!(got.ends_with("nowhere-12345.rs"));
    }

    /// 构造最小诊断回译上下文（列映射/行映射均为 None）
    fn minimal_ctx<'a>(
        ui: &'a crate::ui::Ui,
        root: &'a std::path::Path,
        manager: &'a i18n_rust_engine::mapping_manager::MappingManager,
        file: &'a std::path::Path,
        source: &'a str,
    ) -> DiagContext<'a> {
        DiagContext {
            ui,
            lang_pack: &None,
            project_root: root,
            manager,
            source,
            file,
            entry_product: file.with_extension("rs"),
            column_map: None,
            entry_line_map: None,
            project: None,
        }
    }

    /// 非方言产物（不存在的第三方 .rs）：定位保持 rustc 原样，绝不误标入口文件
    #[test]
    fn test_fix_location_non_dialect_passthrough() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        let entry = root.join("src/main.zh");
        let manager = zh_manager();
        let ui = crate::ui::Ui::for_lang("zh");
        let ctx = minimal_ctx(&ui, root, &manager, &entry, "函数 主函数() {}\n");
        let mut fixer = DiagLocationFixer::new(&ctx, "main.zh", "zh");
        let external = if cfg!(windows) {
            "C:/nonexistent-crate-9f3/src/lib.rs".to_string()
        } else {
            "/nonexistent-crate-9f3/src/lib.rs".to_string()
        };
        let mut loc = i18n_rust_engine::diagnostic::DiagnosticLocation {
            file_name: external.clone(),
            line_start: 7,
            column_start: 3,
            line_end: 7,
            column_end: 9,
            source_text: Some("orig".to_string()),
            label: None,
            is_primary: true,
        };
        fixer.fix_location(&mut loc);
        assert_eq!(loc.file_name, external);
        assert_eq!(loc.line_start, 7);
        assert_eq!(loc.column_start, 3);
        assert_eq!(loc.source_text.as_deref(), Some("orig"));
    }

    /// 子模块方言产物（src/数学.rs）：解析回同名 .zh 源文件并重建坐标/源码行
    #[test]
    fn test_fix_location_submodule_dialect() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        let sub_source = "函数 加(甲: 整数, 乙: 整数) -> 整数 { 甲 + 乙 }\n";
        std::fs::write(root.join("src/数学.zh"), sub_source).unwrap();
        std::fs::write(root.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        let manager = zh_manager();
        let sub_transpiled = i18n_rust_engine::transpile_pipeline(sub_source, &manager);
        std::fs::write(root.join("src/数学.rs"), &sub_transpiled.output).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        let entry = root.join("src/main.zh");
        let ui = crate::ui::Ui::for_lang("zh");
        let ctx = minimal_ctx(&ui, root, &manager, &entry, "函数 主函数() {}\n");
        let mut fixer = DiagLocationFixer::new(&ctx, "main.zh", "zh");
        let mut loc = i18n_rust_engine::diagnostic::DiagnosticLocation {
            file_name: "src/数学.rs".to_string(),
            line_start: 1,
            column_start: 1,
            line_end: 1,
            column_end: 1,
            source_text: None,
            label: None,
            is_primary: true,
        };
        fixer.fix_location(&mut loc);
        assert_eq!(loc.file_name, "数学.zh");
        assert_eq!(loc.source_text.as_deref(), Some(sub_source.trim_end()));
    }

    /// 诊断归属按**被编译目标**而非请求文件：`rzc check src/工具.zh` 时
    /// cargo 仍编译入口产物 src/main.rs，那里的报错属于 main.zh，
    /// 不得因为请求的是工具.zh 就标成工具.zh（旧实现硬编码入口为
    /// src/main.rs，反过来把非入口请求的其他文件报错全部误归入口）
    #[test]
    fn test_fix_location_attributes_by_compiled_target_not_request_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        let main_source = "函数 主函数() { 让 甲 = 1; }\n";
        let tool_source = "函数 加(甲: 整数) -> 整数 { 甲 }\n";
        std::fs::write(root.join("src/main.zh"), main_source).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() { let a = 1; }\n").unwrap();
        std::fs::write(root.join("src/工具.zh"), tool_source).unwrap();
        std::fs::write(root.join("src/工具.rs"), "fn add(a: i32) -> i32 { a }\n").unwrap();
        // 请求的是 src/工具.zh（其产物 src/工具.rs 为入口），main.rs 的报错属于另一个目标文件
        let entry = root.join("src/工具.zh");
        let manager = zh_manager();
        let ui = crate::ui::Ui::for_lang("zh");
        let ctx = minimal_ctx(&ui, root, &manager, &entry, tool_source);
        let mut fixer = DiagLocationFixer::new(&ctx, "工具.zh", "zh");
        let mut loc = i18n_rust_engine::diagnostic::DiagnosticLocation {
            file_name: "src/main.rs".to_string(),
            line_start: 1,
            column_start: 19,
            line_end: 1,
            column_end: 20,
            source_text: None,
            label: None,
            is_primary: true,
        };
        fixer.fix_location(&mut loc);
        assert_eq!(loc.file_name, "main.zh", "应归到报错所在的源文件");
        assert_eq!(
            loc.source_text.as_deref(),
            Some(main_source.trim_end()),
            "源码行取自 main.zh而非工具.zh"
        );
    }

    /// E0432 cargo JSON：翻译器返回存在教学诊断（true），并提取未声明 crate
    #[test]
    fn test_translate_cargo_diagnostics_e0432_teaching() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        let entry = root.join("src/main.zh");
        let manager = zh_manager();
        let ui = crate::ui::Ui::for_lang("zh");
        let ctx = minimal_ctx(&ui, root, &manager, &entry, "函数 主函数() {}\n");
        let json = r#"{"reason":"compiler-message","message":{"message":"unresolved import `serde_json`","code":{"code":"E0432"},"level":"error","spans":[],"children":[]}}"#;
        assert!(translate_cargo_diagnostics(
            json, "", &ctx, false, true, false
        ));
    }

    /// 项目内 lang-packs/zh/errors.toml 损坏：打印加载警告后降级内置表，
    /// 诊断仍正常展示（不因消息文件损坏阻断）
    #[test]
    fn test_translate_cargo_diagnostics_broken_local_errors_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("lang-packs/zh")).unwrap();
        std::fs::write(
            root.join("lang-packs/zh/errors.toml"),
            "this is = = not valid toml [[[\n",
        )
        .unwrap();
        std::fs::write(root.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        let entry = root.join("src/main.zh");
        let manager = zh_manager();
        let ui = crate::ui::Ui::for_lang("zh");
        let ctx = minimal_ctx(&ui, root, &manager, &entry, "函数 主函数() {}\n");
        let json = r#"{"reason":"compiler-message","message":{"message":"unresolved import `serde_json`","code":{"code":"E0432"},"level":"error","spans":[],"children":[]}}"#;
        assert!(translate_cargo_diagnostics(
            json, "", &ctx, false, true, false
        ));
    }

    /// 无可解析诊断时：编译成功打印成功行（silent 时静默）；
    /// 编译失败输出原始 cargo 摘要（streamed 时跳过，避免运行时失败误标）
    #[test]
    fn test_translate_cargo_diagnostics_empty_branches() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        let entry = root.join("src/main.zh");
        let manager = zh_manager();
        let ui = crate::ui::Ui::for_lang("zh");
        // 成功 + 非静默
        let ctx = minimal_ctx(&ui, root, &manager, &entry, "函数 主函数() {}\n");
        assert!(!translate_cargo_diagnostics(
            "", "", &ctx, true, false, false
        ));
        // 成功 + 静默
        assert!(!translate_cargo_diagnostics(
            "", "", &ctx, true, true, false
        ));
        // 失败 + 非流式：输出 cargo 摘要
        assert!(!translate_cargo_diagnostics(
            "",
            "linker `cc` not found",
            &ctx,
            false,
            true,
            false
        ));
        // 失败 + 流式：跳过摘要
        assert!(!translate_cargo_diagnostics(
            "",
            "should not appear",
            &ctx,
            false,
            true,
            true
        ));
        // 非 JSON 杂讯不构成诊断：成功路径
        assert!(!translate_cargo_diagnostics(
            "   Compiling t v0.1.0",
            "",
            &ctx,
            true,
            true,
            false
        ));
    }

    /// 内置中文消息表（cargo 纯文本通道与 JSON 通道共用同一张表）
    fn zh_error_manager() -> i18n_rust_engine::diagnostic::ErrorTranslationManager {
        i18n_rust_engine::diagnostic::ErrorTranslationManager::load_from_string(
            crate::builtin_lang::get_builtin_data("zh").errors_toml,
        )
        .expect("内置中文错误消息表应可加载")
    }

    /// cargo 自身错误（纯文本通道）本地化：新手高频的四类都要换译
    #[test]
    fn test_localize_tool_line_translates_cargo_errors() {
        let manager = zh_error_manager();
        let ui = ui_for_test("zh");
        let cases = [
            (
                "error: could not find `Cargo.toml` in `/home/oop` or any parent directory",
                "错误：在「/home/oop」及其上级目录中都未找到 `Cargo.toml`",
            ),
            (
                "error: failed to parse manifest at `/x/Cargo.toml`",
                "错误：解析清单文件失败：「/x/Cargo.toml」",
            ),
            (
                "error: no targets specified in the manifest",
                "错误：清单中没有指定编译目标",
            ),
            (
                "error: no matching package named `foo` found",
                "错误：找不到名为「foo」的包",
            ),
            (
                "error: current package believes it's in a workspace when it's not:",
                "错误：当前包以为自己在某个工作空间内，但实际不是：",
            ),
            // rustup 包装器未配默认工具链：新手装完 Rust 后撞上的第一句，
            // 工具名（cargo/rustc）走通配段捕获，行首前缀同样要本地化
            (
                "error: rustup could not choose a version of cargo to run, because one wasn't specified explicitly, and no default is configured.",
                "错误：rustup 无法为「cargo」选出要运行的版本：未显式指定，且没有配置默认工具链。",
            ),
            (
                "help: run 'rustup default stable' to download the latest stable release of Rust and set it as your default toolchain.",
                "提示：运行「rustup default stable」下载最新的 Rust 稳定版，并把它设为默认工具链。",
            ),
        ];
        for (line, expected) in cases {
            let got = localize_tool_line(line, Some(&manager), &ui, "zh")
                .unwrap_or_else(|| panic!("应命中消息表：{line}"));
            assert_eq!(got, expected, "行：{line}");
        }
    }

    /// 未命中的 cargo 错误行：原文照旧输出（绝不吞诊断）+ 前缀本地化 + 登记待译
    #[test]
    fn test_localize_tool_line_falls_back_and_records_gap() {
        use i18n_rust_engine::diagnostic::gaps;
        let manager = zh_error_manager();
        let ui = ui_for_test("zh");
        // 独占语言代码：登记表是进程全局的，避免与其他并行测试互相干扰
        let gap_lang = "tool-gap-test-lang";
        let line = "error: unsupported subfeatures in target `x86_64-unknown-linux`";
        let got = localize_tool_line(line, Some(&manager), &ui, gap_lang).expect("标记行应被处理");
        assert_eq!(
            got,
            format!("错误：{}", line.trim_start_matches("error: ")),
            "未译句子应保留原文，仅换行首前缀"
        );
        assert!(
            gaps::peek_gaps()
                .iter()
                .any(|(lang, level, _, message)| lang == gap_lang
                    && level == gaps::LEVEL_TOOL
                    && message.starts_with("unsupported subfeatures")),
            "未命中应登记 tool 通道待译清单"
        );
        // 无消息表时完全不介入：避免把被运行程序自己打印的 error: 行误标
        assert_eq!(
            localize_tool_line(line, None, &ui, gap_lang),
            None,
            "manager 为 None 时应交回原有流程"
        );
        // 已有专门处理的固定汇总句：交回进度行翻译器
        assert_eq!(
            localize_tool_line(
                "error: could not compile `t` (bin \"t\") due to 1 previous error",
                Some(&manager),
                &ui,
                gap_lang,
            ),
            None
        );
    }

    /// cargo 错误链：`Caused by:` 标签行与缩进续行都要换译
    ///
    /// cargo 把因由拆成「标记行 + 缩进行」两行输出，只处理标记行会留下
    /// 整段英文尾巴；而缩进行的查表只在命中时换译，未命中不得登记待译
    /// （缩进输出也可能是被运行程序自己的打印）。
    #[test]
    fn test_localize_tool_line_handles_caused_by_chain() {
        use i18n_rust_engine::diagnostic::gaps;
        let manager = zh_error_manager();
        let ui = ui_for_test("zh");
        assert_eq!(
            localize_tool_line("Caused by:", Some(&manager), &ui, "zh").as_deref(),
            Some("原因："),
            "光杆标记行也应换标签（正文在下一行）"
        );
        assert_eq!(
            localize_tool_line(
                "  failed to read `/no/such/dir/Cargo.toml`",
                Some(&manager),
                &ui,
                "zh"
            )
            .as_deref(),
            Some("  读取「/no/such/dir/Cargo.toml」失败"),
            "缩进续行命中时应换译并保留缩进"
        );
        let gap_lang = "tool-indent-gap-lang";
        assert_eq!(
            localize_tool_line(
                "  some indented program output",
                Some(&manager),
                &ui,
                gap_lang
            ),
            None,
            "未命中的缩进行应交回原有流程"
        );
        assert!(
            !gaps::peek_gaps()
                .iter()
                .any(|(lang, _, _, _)| lang == gap_lang),
            "缩进行未命中不得登记待译清单"
        );
    }

    /// 整段 cargo 输出：标记行走消息表，进度行仍走进度翻译，其他行原样
    #[test]
    fn test_localize_tool_text_keeps_progress_and_plain_lines() {
        let manager = zh_error_manager();
        let ui = ui_for_test("zh");
        let text = "error: no targets specified in the manifest\n\
                    note: this sentence is definitely not in any message table\n\
                    未命中的普通说明行";
        let got = localize_tool_text(text, &manager, &ui, "zh");
        let lines: Vec<&str> = got.lines().collect();
        assert_eq!(lines[0], "错误：清单中没有指定编译目标");
        // note 句不在表内：保留原文，只换前缀
        assert_eq!(
            lines[1],
            "说明：this sentence is definitely not in any message table"
        );
        assert_eq!(lines[2], "未命中的普通说明行");
    }
}
