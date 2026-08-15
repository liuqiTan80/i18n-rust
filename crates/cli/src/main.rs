// rzc 命令行入口 - 多语言 Rust 教学方言编译器
//
// 提供 init / run / check / eject / lang / mapping 等子命令，
// 将母语 Rust 源码实时转译为标准 Rust 并调用 cargo 编译/运行。

use clap::{FromArgMatches, Parser, Subcommand};
use i18n_rust_engine::lexer;
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
#[command(name = "rzc")]
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

fn main() -> anyhow::Result<()> {
    let cargo_cmd = "cargo";
    // 按当前界面语言本地化 clap 帮助文本
    let ui = ui::Ui::global();
    // 同步引擎全局语言（错误/诊断/日志随界面语言输出）
    i18n_rust_engine::语言::set_language(&ui::detect_ui_lang());
    let cli = localize_clap(&ui);
    let args = CliArgs::from_arg_matches(&cli.get_matches())?;

    match args.command {
        CliCommand::Init { project_name, lang } => {
            i18n_rust_engine::语言::set_language(&lang);
            create_project(&project_name, &lang)?;
            Ok(())
        }
        CliCommand::Run { file, lang_pack } => {
            let ui = ui_for_file(&file, &lang_pack);
            let source = fs::read_to_string(&file)?;
            let manager = load_mapping(lang_pack, Some(&file))?;
            let macro_map = manager.get_macro_map();
            let mut english_code =
                lexer::transpile_source_with_macro_map(&source, manager.get_keyword_map(), &macro_map);
            if !manager.module_path_map.is_empty() {
                english_code = i18n_rust_engine::module_path::replace_module_paths(
                    &english_code,
                    manager.get_module_path_map(),
                );
            }
            if !manager.alias_map.is_empty() {
                english_code = i18n_rust_engine::alias::replace_aliases(
                    &english_code,
                    manager.get_alias_map(),
                );
            }

            let project_root = find_project_root(&file)?;
            let source_path = project_root.join("src/main.rs");
            fs::write(&source_path, &english_code)?;

            let output = Command::new(cargo_cmd)
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

            if output.status.success() {
                println!("{}", String::from_utf8_lossy(&output.stdout));
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                eprintln!("{}", ui.f("compile_error", &[&stderr]));
            }
            Ok(())
        }
        CliCommand::Check { file, lang_pack } => {
            let ui = ui_for_file(&file, &lang_pack);
            let source = fs::read_to_string(&file)?;
            let manager = load_mapping(lang_pack.clone(), Some(&file))?;
            let macro_map = manager.get_macro_map();
            let mut english_code =
                lexer::transpile_source_with_macro_map(&source, manager.get_keyword_map(), &macro_map);
            if !manager.module_path_map.is_empty() {
                english_code = i18n_rust_engine::module_path::replace_module_paths(
                    &english_code,
                    manager.get_module_path_map(),
                );
            }
            if !manager.alias_map.is_empty() {
                english_code = i18n_rust_engine::alias::replace_aliases(
                    &english_code,
                    manager.get_alias_map(),
                );
            }

            let project_root = find_project_root(&file)?;
            let source_path = project_root.join("src/main.rs");
            fs::write(&source_path, &english_code)?;

            let output = Command::new(cargo_cmd)
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

            // cargo --message-format=json 的编译器诊断输出到 stdout，cargo 自身消息在 stderr
            let output_text = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let rustc_output = output_text;
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
                    .map_err(|e| anyhow::anyhow!("{}", ui.f("load_error_msg_failed", &[&e.to_string()])))?;
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
                        eprintln!(
                            "{}",
                            ui.f("warn_builtin_errors_failed", &[&e.to_string()])
                        );
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
                println!("{}", ui.t("success_compile"));
                return Ok(());
            }

            if let Some(ref translator) = translator {
                let mut teaching_list = translator.batch_translate(&diagnostics);
                let mut seen_teaching_codes = std::collections::HashSet::new();
                teaching_list.retain(|t| {
                    t.error_code.as_ref().map_or(true, |code| {
                        seen_teaching_codes.insert(code.clone())
                    })
                });
                for teaching in &mut teaching_list {
                    teaching.locations.iter_mut().for_each(|loc| {
                        loc.file_name = original_filename.clone();
                        loc.source_text = get_chinese_source_line(&source, loc.line_start);
                    });
                }
                if teaching_list.is_empty() {
                    println!("{}", ui.t("success_compile"));
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
            Ok(())
        }
        CliCommand::Eject { file, lang_pack } => {
            let ui = ui_for_file(&file, &lang_pack);
            let source = fs::read_to_string(&file)?;
            let manager = load_mapping(lang_pack, Some(&file))?;
            let macro_map = manager.get_macro_map();
            let mut english_code =
                lexer::transpile_source_with_macro_map(&source, manager.get_keyword_map(), &macro_map);
            if !manager.module_path_map.is_empty() {
                english_code = i18n_rust_engine::module_path::replace_module_paths(
                    &english_code,
                    manager.get_module_path_map(),
                );
            }
            if !manager.alias_map.is_empty() {
                english_code = i18n_rust_engine::alias::replace_aliases(
                    &english_code,
                    manager.get_alias_map(),
                );
            }
            let output_path = file.with_extension("rs");
            fs::write(&output_path, english_code)?;
            println!("{}", ui.f("exported_to", &[&output_path.display().to_string()]));
            Ok(())
        }
        CliCommand::Lang { subcommand } => handle_lang_command(subcommand),
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
                PathBuf::from(format!("lang-packs/{}/crates/{}.toml", lang, crate_name))
            });
            mapping_gen::run_auto_generate(&crate_name, &lang, &provider, &output_path)
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
                ui.f("global_lang_dir", &[&lang_manager::global_lang_dir().display().to_string()])
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

    let mut current = file_dir.canonicalize().unwrap_or(file_dir);
    loop {
        if current.join("Cargo.toml").exists() {
            return Ok(current);
        }
        if let Some(parent) = current.parent() {
            current = parent.to_path_buf();
        } else {
            break;
        }
    }
    std::env::current_dir().map_err(|e| {
        anyhow::anyhow!("{}", ui::Ui::global().f("cli_err_cwd", &[&e.to_string()]))
    })
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
    fs::create_dir_all(project_path.join("src"))?;
    fs::write(
        project_path.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.85\"\ncomponents = [\"rustc\", \"cargo\"]\n",
    )?;
    fs::write(
        project_path.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n\n[workspace]\n",
            project_name
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
    // 3. 如果项目根目录存在 lang-packs/<lang>/ 目录，优先使用（自定义覆盖）
    let local_path = PathBuf::from(format!("lang-packs/{}", lang_code));
    if local_path.exists() {
        return MappingManager::load_from_dir(&local_path)
            .map_err(|e| anyhow::anyhow!("{}", ui.f("load_local_lang_pack_failed", &[&e.to_string()])));
    }
    // 4. 全局用户语言包目录
    let global_path = lang_manager::global_lang_dir().join(&lang_code);
    if global_path.exists() {
        return MappingManager::load_from_dir(&global_path)
            .map_err(|e| anyhow::anyhow!("{}", ui.f("load_global_lang_pack_failed", &[&e.to_string()])));
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
    .map_err(|e| anyhow::anyhow!("{}", ui.f("load_builtin_lang_pack_failed", &[&e.to_string()])))
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
    use super::get_lang_code_from_extension;

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
}
