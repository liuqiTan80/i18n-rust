// rzc 命令行入口 - 多语言 Rust 教学方言编译器
//
// 提供 init / run / check / eject / lang / mapping 等子命令，
// 将母语 Rust 源码实时转译为标准 Rust 并调用 cargo 编译/运行。

use clap::{FromArgMatches, Parser, Subcommand};
use i18n_rust_engine::mapping_manager::MappingManager;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod builtin_lang;
mod lang_manager;
mod mapping_gen;
mod ui;

use lang_manager::Source;

#[derive(Parser)]
#[command(name = "rzc", version)]
// 兜底文案（localize_clap 会按界面语言覆盖）；用英文避免硬编码中文
#[command(about = "Multi-language Rust teaching dialect compiler")]
struct CliArgs {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    Init {
        project_name: String,
        #[arg(short, long, default_value = "zh")]
        lang: String,
    },
    Run {
        file: PathBuf,
        #[arg(short, long)]
        lang_pack: Option<PathBuf>,
    },
    Check {
        file: PathBuf,
        #[arg(short, long)]
        lang_pack: Option<PathBuf>,
    },
    Eject {
        file: PathBuf,
        #[arg(short, long)]
        lang_pack: Option<PathBuf>,
    },
    /// 语言包管理（list / install / remove）
    Lang {
        #[command(subcommand)]
        subcommand: LangCommand,
    },
    /// 自动生成第三方库映射（从已安装 crate 提取 API）
    Mapping {
        #[command(subcommand)]
        subcommand: MappingCommand,
    },
}

#[derive(Subcommand)]
enum MappingCommand {
    /// 自动生成第三方库映射文件：提取 crate 公开 API，AI 或规则生成中文名与解释
    Auto {
        /// 目标 crate 名（需已安装或可从 crates.io 获取）
        crate_name: String,
        /// 目标语言（语言包目录名，如 zh、ru；默认按系统语言检测）
        #[arg(long)]
        lang: Option<String>,
        /// AI 服务商：deepseek（默认，需 DEEPSEEK_API_KEY 环境变量）或 rule（离线规则模式）
        #[arg(long, default_value = "deepseek")]
        provider: String,
        /// 输出文件路径（默认 lang-packs/<lang>/crates/<crate_name>.toml）
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum LangCommand {
    /// 列出所有已安装的语言包（内置 + 用户安装）
    List,
    /// 安装语言包：本地目录路径直接复制；语言代码从远程仓库下载
    Install {
        /// 本地语言包目录路径，或远程语言代码
        source: String,
        /// 已存在时强制覆盖安装
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    /// 删除用户安装的语言包（内置语言包不可删除）
    Remove {
        /// 语言代码（语言包目录名）
        lang_code: String,
    },
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("Error: {err:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<std::process::ExitCode> {
    // 按当前界面语言本地化 clap 帮助文本
    let ui = ui::Ui::global();
    // 同步引擎全局语言（错误/诊断/日志随界面语言输出）
    i18n_rust_engine::语言::set_language(&ui::detect_ui_lang());
    let cli = localize_clap(&ui);
    // clap 自身错误（--help/--version 输出、非法参数等）按 clap 退出码直接退出
    let args = match CliArgs::from_arg_matches(&cli.get_matches()) {
        Ok(args) => args,
        Err(err) => err.exit(),
    };

    match args.command {
        CliCommand::Init { project_name, lang } => {
            i18n_rust_engine::语言::set_language(&lang);
            create_project(&project_name, &lang)?;
            Ok(std::process::ExitCode::SUCCESS)
        }
        CliCommand::Run { file, lang_pack } => {
            let ui = ui_for_file(&file, &lang_pack);
            let source = fs::read_to_string(&file)?;
            let manager = load_mapping(lang_pack, Some(&file))?;
            let project_root = find_project_root(&file)?;
            // 入口文件写入 src/main.rs 作为 cargo run 的编译目标
            let source_path = project_root.join("src/main.rs");
            fs::write(&source_path, transpile_to_english(&source, &manager))?;
            // 同步转译项目内其他方言文件，保证多文件项目的 mod 引用链可用
            transpile_project_files(&project_root, &file, &manager)?;

            let output = Command::new("cargo")
                .arg("run")
                .current_dir(&project_root)
                .output()
                .map_err(|e| {
                    anyhow::anyhow!(
                        "{}",
                        ui.f(
                            "cargo_run_failed",
                            &[&project_root.display().to_string(), &e.to_string()]
                        )
                    )
                })?;

            // 无论成败先输出程序 stdout
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.is_empty() {
                print!("{}", stdout);
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            if output.status.success() {
                // 成功时 stderr 含编译警告，同样展示
                if !stderr.trim().is_empty() {
                    eprint!("{}", stderr);
                }
                return Ok(std::process::ExitCode::SUCCESS);
            }
            // 失败时区分编译错误与程序运行时错误：
            // cargo 编译失败会在 stderr 输出 error[E....] / error: 前缀
            if stderr.contains("error[E") || stderr.contains("error:") {
                eprintln!("{}", ui.f("compile_error", &[stderr.as_ref()]));
            } else {
                eprintln!(
                    "{}",
                    ui.f(
                        "run_program_failed",
                        &[&output.status.to_string(), stderr.trim()]
                    )
                );
            }
            // 传播被运行程序的退出码（信号终止等无码场景回退 1）
            Ok(output
                .status
                .code()
                .map(|c| std::process::ExitCode::from(c as u8))
                .unwrap_or(std::process::ExitCode::FAILURE))
        }
        CliCommand::Check { file, lang_pack } => {
            let ui = ui_for_file(&file, &lang_pack);
            let source = fs::read_to_string(&file)?;
            let manager = load_mapping(lang_pack.clone(), Some(&file))?;
            let project_root = find_project_root(&file)?;
            let source_path = project_root.join("src/main.rs");
            fs::write(&source_path, transpile_to_english(&source, &manager))?;
            // 同步转译项目内其他方言文件，保证多文件项目的 mod 引用链可用
            transpile_project_files(&project_root, &file, &manager)?;

            let output = Command::new("cargo")
                .arg("check")
                .arg("--message-format=json")
                .current_dir(&project_root)
                .output()
                .map_err(|e| {
                    anyhow::anyhow!(
                        "{}",
                        ui.f(
                            "cargo_check_failed",
                            &[&project_root.display().to_string(), &e.to_string()]
                        )
                    )
                })?;
            // cargo 整体是否成功（决定最终退出码）
            let exit_code = if output.status.success() {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::FAILURE
            };

            // cargo --message-format=json 的编译器诊断输出到 stdout，cargo 自身消息在 stderr
            let stderr_text = String::from_utf8_lossy(&output.stderr).to_string();
            let rustc_output = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                stderr_text
            );
            use i18n_rust_engine::diagnostic::{
                DiagnosticTranslator, ErrorTranslationManager, parse_diagnostic_output,
            };

            // 按语言代码选择错误消息：--lang-pack 目录 > 项目内 lang-packs/<lang>/ > 内置
            let lang_code = file
                .extension()
                .and_then(|e| e.to_str())
                .and_then(get_lang_code_from_extension)
                .unwrap_or_else(ui::detect_ui_lang);
            let error_msg_path = if let Some(path) = &lang_pack {
                path.join("errors.toml")
            } else if project_root
                .join(format!("lang-packs/{}/errors.toml", lang_code))
                .exists()
            {
                project_root.join(format!("lang-packs/{}/errors.toml", lang_code))
            } else {
                lang_manager::global_lang_dir()
                    .join(&lang_code)
                    .join("errors.toml")
            };
            let translator = if error_msg_path.exists() {
                let translation_manager = ErrorTranslationManager::load_from_file(&error_msg_path)
                    .map_err(|e| {
                        anyhow::anyhow!("{}", ui.f("load_error_msg_failed", &[&e.to_string()]))
                    })?;
                let reverse_map: HashMap<String, String> = manager
                    .get_section_mapping("类型")
                    .map(|section| {
                        section
                            .iter()
                            .map(|(k, v)| (v.clone(), k.clone()))
                            .collect()
                    })
                    .unwrap_or_default();
                Some(DiagnosticTranslator::new(translation_manager, reverse_map))
            } else {
                // 回退到内置语言包（未知语言代码自动回退中文）
                let builtin = builtin_lang::get_builtin_data(&lang_code);
                match ErrorTranslationManager::load_from_string(builtin.errors_toml) {
                    Ok(translation_manager) => {
                        let reverse_map: HashMap<String, String> = manager
                            .get_section_mapping("类型")
                            .map(|section| {
                                section
                                    .iter()
                                    .map(|(k, v)| (v.clone(), k.clone()))
                                    .collect()
                            })
                            .unwrap_or_default();
                        Some(DiagnosticTranslator::new(translation_manager, reverse_map))
                    }
                    Err(e) => {
                        eprintln!("{}", ui.f("warn_builtin_errors_failed", &[&e.to_string()]));
                        None
                    }
                }
            };

            let original_filename = file
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mut diagnostics = parse_diagnostic_output(&rustc_output);
            // 保留 error/warning；不要求有错误码——无码的解析错误（如缺括号）
            // 也必须显示，否则会被静默吞掉导致假“编译成功”
            diagnostics.retain(|d| d.level == "error" || d.level == "warning");
            let mut seen_codes = std::collections::HashSet::new();
            diagnostics.retain(|d| {
                if let Some(ref code) = d.code {
                    seen_codes.insert(code.code.clone())
                } else {
                    true // 无错误码的诊断（解析错误等）不去重，直接保留
                }
            });

            if diagnostics.is_empty() {
                if output.status.success() {
                    println!("{}", ui.t("success_compile"));
                } else {
                    // cargo 失败但无可解析的 JSON 诊断（Cargo.toml 语法错误、
                    // 链接错误等）：原样输出 cargo 消息，绝不虚报“编译成功”
                    eprintln!("{}", ui.f("compile_error", &[stderr_text.trim()]));
                }
                return Ok(exit_code);
            }

            if let Some(ref translator) = translator {
                let mut teaching_list = translator.batch_translate(&diagnostics);
                let mut seen_teaching_codes = std::collections::HashSet::new();
                teaching_list.retain(|t| {
                    t.error_code
                        .as_ref()
                        .map_or(true, |code| seen_teaching_codes.insert(code.clone()))
                });
                for teaching in &mut teaching_list {
                    teaching.locations.iter_mut().for_each(|loc| {
                        loc.file_name = original_filename.clone();
                        loc.source_text = get_chinese_source_line(&source, loc.line_start);
                    });
                }
                if teaching_list.is_empty() {
                    if output.status.success() {
                        println!("{}", ui.t("success_compile"));
                    } else {
                        eprintln!("{}", ui.f("compile_error", &[stderr_text.trim()]));
                    }
                } else {
                    println!(
                        "{}",
                        i18n_rust_engine::diagnostic::TeachingDiagnostic::batch_format_as_text(
                            &teaching_list
                        )
                    );
                }
            } else {
                if !rustc_output.is_empty() {
                    for line in rustc_output.lines() {
                        if let Ok(raw) = serde_json::from_str::<serde_json::Value>(line) {
                            if let Some(message) = raw.get("message") {
                                println!("{}", message.as_str().unwrap_or(""));
                            }
                        } else {
                            println!("{}", line);
                        }
                    }
                } else {
                    println!("{}", ui.t("success_compile"));
                }
            }
            Ok(exit_code)
        }
        CliCommand::Eject { file, lang_pack } => {
            let ui = ui_for_file(&file, &lang_pack);
            let source = fs::read_to_string(&file)?;
            let manager = load_mapping(lang_pack, Some(&file))?;
            let english_code = transpile_to_english(&source, &manager);
            let output_path = file.with_extension("rs");
            fs::write(&output_path, english_code)?;
            println!(
                "{}",
                ui.f("exported_to", &[&output_path.display().to_string()])
            );
            Ok(std::process::ExitCode::SUCCESS)
        }
        CliCommand::Lang { subcommand } => {
            handle_lang_command(subcommand).map(|()| std::process::ExitCode::SUCCESS)
        }
        CliCommand::Mapping { subcommand } => {
            let MappingCommand::Auto {
                crate_name,
                lang,
                provider,
                output,
            } = subcommand;
            let lang = lang.unwrap_or_else(mapping_gen::detect_system_language);
            i18n_rust_engine::语言::set_language(&lang);
            let output_path = output.unwrap_or_else(|| {
                // 默认写入项目根的 lang-packs/：从 cwd 向上找 Cargo.toml，
                // 保证任意子目录下执行都落到项目本地语言包（load_mapping 同一位置查找）
                let base = std::env::current_dir()
                    .ok()
                    .and_then(|cwd| find_project_root_upward(&cwd))
                    .unwrap_or_else(|| PathBuf::from("."));
                base.join(format!("lang-packs/{}/crates/{}.toml", lang, crate_name))
            });
            mapping_gen::run_auto_generate(&crate_name, &lang, &provider, &output_path)
                .map(|()| std::process::ExitCode::SUCCESS)
        }
    }
}

/// 处理 `rzc lang` 子命令
fn handle_lang_command(subcommand: LangCommand) -> anyhow::Result<()> {
    let ui = ui::Ui::global();
    match subcommand {
        LangCommand::List => {
            let list = lang_manager::list_langs();
            if list.is_empty() {
                println!("{}", ui.t("no_lang_installed"));
                return Ok(());
            }
            println!(
                "{}",
                ui.f("installed_langs_count", &[&list.len().to_string()])
            );
            for info in &list {
                let tag = match info.source {
                    Source::Builtin => ui.t("tag_builtin"),
                    Source::UserInstalled => ui.t("tag_user"),
                };
                let ext = info
                    .extension
                    .as_deref()
                    .map(|e| format!(".{}", e))
                    .unwrap_or_else(|| ui.t("unknown"));
                let version = match info.version.as_deref() {
                    Some(v) => v.to_string(),
                    None => ui.t("unknown"),
                };
                let removable = if info.source == Source::Builtin {
                    ui.t("not_removable")
                } else {
                    String::new()
                };
                let display = info
                    .display_name
                    .as_deref()
                    .map(|n| format!("{} ({})", n, info.lang_code))
                    .unwrap_or_else(|| info.lang_code.clone());
                println!(
                    "{}",
                    ui.f(
                        "lang_list_display",
                        &[&tag, &display, &ext, &version, &removable]
                    )
                );
            }
            println!(
                "{}",
                ui.f(
                    "global_lang_dir",
                    &[&lang_manager::global_lang_dir().display().to_string()]
                )
            );
            Ok(())
        }
        LangCommand::Install { source, force } => lang_manager::install_lang(&source, force),
        LangCommand::Remove { lang_code } => lang_manager::remove_lang(&lang_code),
    }
}

/// 根据源码文件定位项目根（包含 Cargo.toml 的目录）
fn find_project_root(file: &Path) -> anyhow::Result<PathBuf> {
    let file_dir = if file.is_absolute() {
        file.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        std::env::current_dir()?
            .join(file)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    };

    if let Some(root) = find_project_root_upward(&file_dir) {
        return Ok(root);
    }
    // 未找到 Cargo.toml 时明确报错：静默回退当前目录会在无关目录写入 src/main.rs
    anyhow::bail!(
        "{}",
        ui::Ui::global().f("no_project_root", &[&file.display().to_string()])
    )
}

/// 从指定目录向上查找项目根（含 Cargo.toml 的目录），未找到返回 None
fn find_project_root_upward(start: &Path) -> Option<PathBuf> {
    let mut current = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    loop {
        if current.join("Cargo.toml").exists() {
            return Some(current);
        }
        current = current.parent()?.to_path_buf();
    }
}

/// 统一转译管线（复用 engine）：Unicode 检查 → 关键字/宏转译 → 模块路径替换 → 别名替换 → 非 ASCII 模块注解
fn transpile_to_english(source: &str, manager: &MappingManager) -> String {
    let code = i18n_rust_engine::transpile_pipeline(source, manager).output;
    annotate_non_ascii_mods(&code)
}

/// 为非 ASCII 模块名的文件式声明 `mod 名称;` 补充 `#[path = "名称.rs"]` 注解。
///
/// rustc 拒绝加载非 ASCII 标识符对应的模块文件（E0754），而方言项目
/// 的模块文件常以母语命名（如 `src/数学.zh` → `src/数学.rs`）；
/// 显式指定 path 后 rustc 可正常加载。仅处理以分号结尾的文件式声明，
/// 内联模块块（`mod 名称 { ... }`）与 ASCII 名不受影响。
fn annotate_non_ascii_mods(code: &str) -> String {
    use rustc_lexer::{TokenKind, tokenize};
    let tokens: Vec<_> = tokenize(code).collect();
    // 逐 token 的字节偏移（rustc_lexer 词法流覆盖全源，偏移连续）
    let mut offsets: Vec<usize> = Vec::with_capacity(tokens.len());
    let mut acc = 0usize;
    for t in &tokens {
        offsets.push(acc);
        acc += t.len;
    }
    let skip_trivia = |mut idx: usize| {
        while idx < tokens.len()
            && matches!(
                tokens[idx].kind,
                TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
            )
        {
            idx += 1;
        }
        idx
    };
    let mut insertions: Vec<(usize, String)> = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Ident {
            continue;
        }
        let text = &code[offsets[i]..offsets[i] + token.len];
        if text != "mod" {
            continue;
        }
        let name_idx = skip_trivia(i + 1);
        let Some(name_t) = tokens.get(name_idx) else {
            continue;
        };
        if name_t.kind != TokenKind::Ident {
            continue;
        }
        let semi_idx = skip_trivia(name_idx + 1);
        // 仅文件式声明（分号结尾）需要注解；内联模块块以 `{` 开头
        if !matches!(tokens.get(semi_idx).map(|t| t.kind), Some(TokenKind::Semi)) {
            continue;
        }
        let name = &code[offsets[name_idx]..offsets[name_idx] + name_t.len];
        if name.is_ascii() {
            continue;
        }
        // 插入点：若有可见性修饰 `pub`，注解必须在 pub 之前
        let mut insert_at = offsets[i];
        let mut back = i;
        while back > 0
            && matches!(
                tokens[back - 1].kind,
                TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
            )
        {
            back -= 1;
        }
        if back > 0
            && tokens[back - 1].kind == TokenKind::Ident
            && &code[offsets[back - 1]..offsets[back - 1] + tokens[back - 1].len] == "pub"
        {
            insert_at = offsets[back - 1];
        }
        // 已有 #[path] 注解时不重复添加：注解可独占多行，需向上逐行扫描属性链
        let line_start = code[..insert_at].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let mut scan_start = line_start;
        let mut has_path_attr = false;
        loop {
            let line = code[scan_start..insert_at].lines().next().unwrap_or("");
            // 首行可能是 mod 所在行的缩进前缀；上移后的行应为属性行
            if scan_start != line_start || line.trim_start().starts_with("#[") {
                if line.contains("#[path") {
                    has_path_attr = true;
                    break;
                }
                if !line.trim_start().starts_with("#[") {
                    break; // 属性链被普通行/空行打断
                }
            }
            if scan_start == 0 {
                break;
            }
            let prev_start = code[..scan_start - 1]
                .rfind('\n')
                .map(|p| p + 1)
                .unwrap_or(0);
            if prev_start == scan_start {
                break;
            }
            scan_start = prev_start;
        }
        if has_path_attr {
            continue;
        }
        let indent = &code[line_start..insert_at];
        insertions.push((insert_at, format!("#[path = \"{name}.rs\"]\n{indent}")));
    }
    let mut result = code.to_string();
    for (pos, text) in insertions.into_iter().rev() {
        result.insert_str(pos, &text);
    }
    result
}

/// 同步转译项目 src/ 下的全部方言源文件（入口文件除外）为对应 .rs 文件，
/// 使多文件项目的 mod 引用链可用；非已注册方言扩展名的文件（如手写 .rs）跳过。
fn transpile_project_files(
    project_root: &Path,
    entry_file: &Path,
    manager: &MappingManager,
) -> anyhow::Result<()> {
    let ui = ui::Ui::global();
    let src_dir = project_root.join("src");
    // 无 src 目录时不处理，由 cargo 自行报错
    let Ok(entries) = fs::read_dir(&src_dir) else {
        return Ok(());
    };
    // 入口产物固定写入 src/main.rs：src/ 下任何词干为 main 的方言文件
    // （如 init 生成的 main.zh）转译后会覆盖入口产物，必须跳过
    let entry_abs = entry_file.canonicalize().ok();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if Some(&path) == entry_abs.as_ref() {
            continue;
        }
        if path.file_stem().is_some_and(|s| s == "main") {
            continue;
        }
        let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if get_lang_code_from_extension(extension).is_none() {
            continue;
        }
        let source = fs::read_to_string(&path).map_err(|e| {
            anyhow::anyhow!(
                "{}",
                ui.f(
                    "transpile_file_failed",
                    &[&path.display().to_string(), &e.to_string()]
                )
            )
        })?;
        fs::write(
            path.with_extension("rs"),
            transpile_to_english(&source, manager),
        )?;
    }
    Ok(())
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

fn create_project(project_name: &str, lang: &str) -> anyhow::Result<()> {
    let ui = ui::Ui::for_lang(lang);
    i18n_rust_engine::语言::set_language(lang);
    let project_path = PathBuf::from(project_name);
    if project_path.exists() {
        anyhow::bail!("{}", ui.f("dir_exists", &[project_name]));
    }
    // 包名取路径最后一段（支持传入绝对/相对路径），并将 cargo 不允许的字符替换为下划线
    let package_name = project_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(project_name);
    let package_name: String = package_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    fs::create_dir_all(project_path.join("src"))?;
    fs::write(
        project_path.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.85\"\ncomponents = [\"rustc\", \"cargo\"]\n",
    )?;
    fs::write(
        project_path.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n\n[workspace]\n",
            package_name
        ),
    )?;
    // 语言包已内置到 rzc 可执行文件中，无需复制；主文件模板随 --lang 变化
    fs::write(
        project_path.join(format!("src/main.{}", lang)),
        ui.t("template_main"),
    )?;
    fs::write(
        project_path.join("README.md"),
        ui.f("readme_template", &[project_name, lang]),
    )?;
    println!("{}", ui.f("project_created", &[project_name]));
    println!("{}", ui.f("project_created_hint", &[lang]));
    println!("{}", ui.f("project_run_hint", &[lang]));
    Ok(())
}

fn load_mapping(
    lang_pack_path: Option<PathBuf>,
    source_file: Option<&Path>,
) -> anyhow::Result<MappingManager> {
    let ui = ui_for_file(source_file.unwrap_or(Path::new("")), &lang_pack_path);
    // 1. 如果用户通过 --lang-pack 指定了外部目录，强制使用
    if let Some(path) = lang_pack_path {
        return MappingManager::load_from_dir(&path)
            .map_err(|e| anyhow::anyhow!("{}", ui.f("load_lang_pack_failed", &[&e.to_string()])));
    }
    // 2. 根据源文件扩展名确定语言代码
    let extension = source_file
        .and_then(|f| f.extension())
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let lang_code = get_lang_code_from_extension(extension).ok_or_else(|| {
        let available = lang_manager::all_available_extensions();
        let available_text = if available.is_empty() {
            ui.t("no_available_ext")
        } else {
            available
                .iter()
                .map(|e| format!(".{}", e))
                .collect::<Vec<_>>()
                .join(", ")
        };
        anyhow::anyhow!(
            "{}",
            ui.f("unknown_extension", &[extension, &available_text])
        )
    })?;
    // 3. 项目根的 lang-packs/<lang>/ 目录存在时优先使用（自定义覆盖）
    //    项目根从源文件向上查找；源文件不在项目内时回退 cwd，兼容旧用法
    let mut local_candidates: Vec<PathBuf> = Vec::new();
    if let Some(file) = source_file
        && let Some(parent) = file.parent()
        && let Some(root) = find_project_root_upward(parent)
    {
        local_candidates.push(root.join("lang-packs").join(&lang_code));
    }
    local_candidates.push(PathBuf::from(format!("lang-packs/{}", lang_code)));
    for local_path in &local_candidates {
        if local_path.exists() {
            return MappingManager::load_from_dir(local_path).map_err(|e| {
                anyhow::anyhow!("{}", ui.f("load_local_lang_pack_failed", &[&e.to_string()]))
            });
        }
    }
    // 4. 全局用户语言包目录
    let global_path = lang_manager::global_lang_dir().join(&lang_code);
    if global_path.exists() {
        return MappingManager::load_from_dir(&global_path).map_err(|e| {
            anyhow::anyhow!(
                "{}",
                ui.f("load_global_lang_pack_failed", &[&e.to_string()])
            )
        });
    }
    // 5. 回退到内置语言包（未内置的语言提示用户安装，避免静默使用中文）
    if !builtin_lang::has_builtin_lang(&lang_code) {
        return Err(anyhow::anyhow!(
            "{}",
            ui.f("lang_not_builtin", &[&lang_code, &lang_code])
        ));
    }
    let builtin = builtin_lang::get_builtin_data(&lang_code);
    MappingManager::load_from_builtin(
        builtin.keywords_toml,
        builtin.module_paths_toml,
        builtin.stdlib_toml,
        builtin.crates_data,
    )
    .map_err(|e| {
        anyhow::anyhow!(
            "{}",
            ui.f("load_builtin_lang_pack_failed", &[&e.to_string()])
        )
    })
}

/// 根据源文件与 --lang-pack 参数选择界面消息语言
///
/// --lang-pack 显式目录优先；否则按源文件扩展名确定语言代码；
/// 无法识别时回退 RZ_LANG / 系统语言 / 中文。
fn ui_for_file(file: &Path, lang_pack: &Option<PathBuf>) -> ui::Ui {
    if let Some(path) = lang_pack {
        let ui = ui::Ui::for_explicit_dir(path);
        // 同步引擎全局语言（目录名即语言代码）
        let code = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("zh")
            .to_string();
        i18n_rust_engine::语言::set_language(&code);
        return ui;
    }
    let lang_code = file
        .extension()
        .and_then(|e| e.to_str())
        .and_then(get_lang_code_from_extension)
        .unwrap_or_else(ui::detect_ui_lang);
    i18n_rust_engine::语言::set_language(&lang_code);
    ui::Ui::for_lang(&lang_code)
}

/// 按当前界面语言本地化 clap 帮助文本
///
/// 利用 `CommandFactory` 生成命令后逐项覆盖 about / help，
/// 使 `rzc --help` 与各子命令帮助均使用目标语言。
fn localize_clap(ui: &ui::Ui) -> clap::Command {
    use clap::CommandFactory;
    CliArgs::command()
        .about(ui.t("cli_about"))
        .mut_subcommand("init", |cmd| {
            cmd.about(ui.t("cmd_init_about"))
                .mut_arg("lang", |arg| arg.help(ui.t("arg_lang_help")))
        })
        .mut_subcommand("run", |cmd| cmd.about(ui.t("cmd_run_about")))
        .mut_subcommand("check", |cmd| cmd.about(ui.t("cmd_check_about")))
        .mut_subcommand("eject", |cmd| cmd.about(ui.t("cmd_eject_about")))
        .mut_subcommand("lang", |cmd| {
            cmd.about(ui.t("cmd_lang_about"))
                .mut_subcommand("list", |sub| sub.about(ui.t("cmd_lang_list_about")))
                .mut_subcommand("install", |sub| sub.about(ui.t("cmd_lang_install_about")))
                .mut_subcommand("remove", |sub| sub.about(ui.t("cmd_lang_remove_about")))
        })
        .mut_subcommand("mapping", |cmd| {
            cmd.about(ui.t("cmd_mapping_about"))
                .mut_subcommand("auto", |sub| {
                    sub.about(ui.t("cmd_mapping_auto_about"))
                        .mut_arg("lang", |arg| arg.help(ui.t("arg_lang_help")))
                        .mut_arg("provider", |arg| arg.help(ui.t("arg_provider_help")))
                        .mut_arg("output", |arg| arg.help(ui.t("arg_output_help")))
                })
        })
}

/// 根据源码文件扩展名获取语言代码
///
/// 优先查询动态映射表，未命中时回退静态映射。
fn get_lang_code_from_extension(extension: &str) -> Option<String> {
    if let Some(code) = lang_manager::query_extension_map(extension) {
        return Some(code);
    }
    lang_manager::static_extension_map().get(extension).cloned()
}

#[cfg(test)]
mod tests {
    use super::{
        annotate_non_ascii_mods, get_lang_code_from_extension, transpile_project_files,
        transpile_to_english,
    };

    /// 加载内置中文映射管理器（测试转译管线用）
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

    /// 已内置的语言包扩展名可解析出语言代码
    #[test]
    fn test_lang_code_from_extension_builtin() {
        assert_eq!(get_lang_code_from_extension("zh").as_deref(), Some("zh"));
        assert_eq!(get_lang_code_from_extension("en").as_deref(), Some("en"));
        assert_eq!(get_lang_code_from_extension("de").as_deref(), Some("de"));
        assert_eq!(get_lang_code_from_extension("ru").as_deref(), Some("ru"));
        assert_eq!(get_lang_code_from_extension("ja").as_deref(), Some("ja"));
        assert_eq!(get_lang_code_from_extension("hi").as_deref(), Some("hi"));
    }

    /// 未知扩展名返回 None
    #[test]
    fn test_lang_code_from_extension_unknown() {
        assert_eq!(get_lang_code_from_extension("xyz"), None);
    }

    /// 统一转译管线：中文关键字转为标准 Rust
    #[test]
    fn test_transpile_to_english_zh_keywords() {
        let manager = zh_manager();
        let out = transpile_to_english("公开 函数 help() {}\n", &manager);
        assert!(out.contains("pub fn help()"), "实际输出：{out}");
    }

    /// 多文件转译：src/ 下的其他方言文件生成同名 .rs，入口与手写 .rs 不受影响
    #[test]
    fn test_transpile_project_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        let entry = root.join("src/main.zh");
        std::fs::write(&entry, "函数 main() {}\n").unwrap();
        std::fs::write(root.join("src/helper.zh"), "公开 函数 help() {}\n").unwrap();
        std::fs::write(root.join("src/manual.rs"), "// 手写文件不覆盖\n").unwrap();

        let manager = zh_manager();
        transpile_project_files(root, &entry, &manager).unwrap();

        // 其他方言文件已转译为同名 .rs
        let helper_rs = std::fs::read_to_string(root.join("src/helper.rs")).unwrap();
        assert!(helper_rs.contains("pub fn help()"), "实际输出：{helper_rs}");
        // 手写 .rs 不被触碰
        let manual_rs = std::fs::read_to_string(root.join("src/manual.rs")).unwrap();
        assert!(manual_rs.contains("手写文件不覆盖"));
        // 入口文件未被重复转译（无 main.rs 产生，由调用方单独写入）
        assert!(!root.join("src/main.rs").exists());
    }

    /// 非 ASCII 文件式 mod 声明补 #[path] 注解（绕过 rustc E0754）
    #[test]
    fn test_annotate_non_ascii_mod_basic() {
        let out = annotate_non_ascii_mods("mod 数学;");
        assert_eq!(out, "#[path = \"数学.rs\"]\nmod 数学;");
    }

    /// 带 pub 可见性时注解插在 pub 之前
    #[test]
    fn test_annotate_non_ascii_mod_with_pub() {
        let out = annotate_non_ascii_mods("pub mod 数学;");
        assert_eq!(out, "#[path = \"数学.rs\"]\npub mod 数学;");
    }

    /// ASCII 模块名与内联模块块不处理
    #[test]
    fn test_annotate_non_ascii_mod_skip_ascii_and_inline() {
        assert_eq!(annotate_non_ascii_mods("mod math;"), "mod math;");
        assert_eq!(annotate_non_ascii_mods("mod 数学 { }"), "mod 数学 { }");
    }

    /// 已有 #[path] 注解时不重复添加；缩进保持
    #[test]
    fn test_annotate_non_ascii_mod_existing_and_indent() {
        let src = "#[path = \"数学.rs\"]\nmod 数学;";
        assert_eq!(annotate_non_ascii_mods(src), src);
        let out = annotate_non_ascii_mods("函数 main() {\n    mod 数学;\n}");
        assert_eq!(
            out,
            "函数 main() {\n    #[path = \"数学.rs\"]\n    mod 数学;\n}"
        );
    }
}
