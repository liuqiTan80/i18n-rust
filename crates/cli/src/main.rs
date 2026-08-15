// rzc 命令行入口 - 多语言 Rust 教学方言编译器
//
// 提供 init / run / check / eject / lang / mapping 等子命令，
// 将母语 Rust 源码实时转译为标准 Rust 并调用 cargo 编译/运行。

use clap::{Parser, Subcommand};
use i18n_rust_engine::mapping_manager::MappingManager;
use i18n_rust_engine::lexer;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod builtin_lang;
mod mapping_gen;
mod lang_manager;

use lang_manager::Source;

#[derive(Parser)]
#[command(name = "rzc")]
#[command(about = "多语言 Rust 教学方言编译器")]
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
    let args = CliArgs::parse();

    match args.command {
        CliCommand::Init { project_name, lang } => {
            create_project(&project_name, &lang)?;
            Ok(())
        }
        CliCommand::Run { file, lang_pack } => {
            let source = fs::read_to_string(&file)?;
            let manager = load_mapping(lang_pack, Some(&file))?;
            let macro_set = manager.get_macro_names();
            let mut english_code =
                lexer::transpile_source_with_macros(&source, manager.get_keyword_map(), &macro_set);
            if !manager.module_path_map.is_empty() {
                english_code = i18n_rust_engine::module_path::replace_module_paths(
                    &english_code,
                    manager.get_module_path_map(),
                );
            }
            if !manager.alias_map.is_empty() {
                english_code =
                    i18n_rust_engine::alias::replace_aliases(&english_code, manager.get_alias_map());
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
                        "执行 cargo run 失败，项目根: {}，错误: {}",
                        project_root.display(),
                        e
                    )
                })?;

            if output.status.success() {
                println!("{}", String::from_utf8_lossy(&output.stdout));
            } else {
                eprintln!("编译错误：\n{}", String::from_utf8_lossy(&output.stderr));
            }
            Ok(())
        }
        CliCommand::Check { file, lang_pack } => {
            let source = fs::read_to_string(&file)?;
            let manager = load_mapping(lang_pack.clone(), Some(&file))?;
            let macro_set = manager.get_macro_names();
            let mut english_code =
                lexer::transpile_source_with_macros(&source, manager.get_keyword_map(), &macro_set);
            if !manager.module_path_map.is_empty() {
                english_code = i18n_rust_engine::module_path::replace_module_paths(
                    &english_code,
                    manager.get_module_path_map(),
                );
            }
            if !manager.alias_map.is_empty() {
                english_code =
                    i18n_rust_engine::alias::replace_aliases(&english_code, manager.get_alias_map());
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
                        "执行 cargo check 失败，项目根: {}，错误: {}",
                        project_root.display(),
                        e
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
                parse_diagnostic_output, DiagnosticTranslator, ErrorTranslationManager,
            };

            // 按语言代码选择错误消息：--lang-pack 目录 > 项目内 lang-packs/<lang>/ > 内置
            let lang_code = file
                .extension()
                .and_then(|e| e.to_str())
                .and_then(get_lang_code_from_extension)
                .unwrap_or_else(|| "zh".to_string());
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
                    .map_err(|e| anyhow::anyhow!("加载错误消息失败: {}", e))?;
                let reverse_map: HashMap<String, String> = manager
                    .get_section_mapping("类型")
                    .map(|section| section.iter().map(|(k, v)| (v.clone(), k.clone())).collect())
                    .unwrap_or_default();
                Some(DiagnosticTranslator::new(translation_manager, reverse_map))
            } else {
                match builtin_lang::get_builtin_data(&lang_code) {
                    Some(builtin) => {
                        match ErrorTranslationManager::load_from_string(builtin.errors_toml) {
                            Ok(translation_manager) => {
                                let reverse_map: HashMap<String, String> = manager
                                    .get_section_mapping("类型")
                                    .map(|section| {
                                        section.iter().map(|(k, v)| (v.clone(), k.clone())).collect()
                                    })
                                    .unwrap_or_default();
                                Some(DiagnosticTranslator::new(translation_manager, reverse_map))
                            }
                            Err(e) => {
                                eprintln!("警告: 加载内置错误消息失败: {}", e);
                                None
                            }
                        }
                    }
                    None => None,
                }
            };

            let original_filename = file
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mut diagnostics = parse_diagnostic_output(&rustc_output);
            diagnostics.retain(|d| d.code.is_some() && (d.level == "error" || d.level == "warning"));
            let mut seen_codes = std::collections::HashSet::new();
            diagnostics.retain(|d| {
                if let Some(ref code) = d.code {
                    seen_codes.insert(code.code.clone())
                } else {
                    false
                }
            });

            if diagnostics.is_empty() {
                println!("✅ 编译成功，没有错误。");
                return Ok(());
            }

            if let Some(ref translator) = translator {
                let mut teaching_list = translator.batch_translate(&diagnostics);
                let mut seen_teaching_codes = std::collections::HashSet::new();
                teaching_list.retain(|t| {
                    t.error_code
                        .as_ref()
                        .map_or(false, |code| seen_teaching_codes.insert(code.clone()))
                });
                for teaching in &mut teaching_list {
                    teaching.locations.iter_mut().for_each(|loc| {
                        loc.file_name = original_filename.clone();
                        loc.source_text = get_chinese_source_line(&source, loc.line_start);
                    });
                }
                if teaching_list.is_empty() {
                    println!("✅ 编译成功，没有错误。");
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
                    println!("✅ 编译成功，没有错误。");
                }
            }
            Ok(())
        }
        CliCommand::Eject { file, lang_pack } => {
            let source = fs::read_to_string(&file)?;
            let manager = load_mapping(lang_pack, Some(&file))?;
            let macro_set = manager.get_macro_names();
            let mut english_code =
                lexer::transpile_source_with_macros(&source, manager.get_keyword_map(), &macro_set);
            if !manager.module_path_map.is_empty() {
                english_code = i18n_rust_engine::module_path::replace_module_paths(
                    &english_code,
                    manager.get_module_path_map(),
                );
            }
            if !manager.alias_map.is_empty() {
                english_code =
                    i18n_rust_engine::alias::replace_aliases(&english_code, manager.get_alias_map());
            }
            let output_path = file.with_extension("rs");
            fs::write(&output_path, english_code)?;
            println!("已导出到 {}", output_path.display());
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
            let output_path = output.unwrap_or_else(|| {
                PathBuf::from(format!("lang-packs/{}/crates/{}.toml", lang, crate_name))
            });
            mapping_gen::run_auto_generate(&crate_name, &lang, &provider, &output_path)
        }
    }
}

/// 处理 `rzc lang` 子命令
fn handle_lang_command(subcommand: LangCommand) -> anyhow::Result<()> {
    match subcommand {
        LangCommand::List => {
            let list = lang_manager::list_langs();
            if list.is_empty() {
                println!("没有已安装的语言包。");
                return Ok(());
            }
            println!("已安装的语言包（共 {} 个）：", list.len());
            for info in &list {
                let tag = match info.source {
                    Source::Builtin => "内置",
                    Source::UserInstalled => "用户安装",
                };
                let ext = info
                    .extension
                    .as_deref()
                    .map(|e| format!(".{}", e))
                    .unwrap_or_else(|| "未知".to_string());
                let version = info.version.as_deref().unwrap_or("未知");
                let removable = if info.source == Source::Builtin {
                    "（不可删除）"
                } else {
                    ""
                };
                println!(
                    "  [{}] {}（扩展名：{}，版本：{}）{}",
                    tag, info.lang_code, ext, version, removable
                );
            }
            println!("全局语言包目录：{}", lang_manager::global_lang_dir().display());
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
    std::env::current_dir().map_err(|e| anyhow::anyhow!("无法确定项目根: {}", e))
}

fn get_chinese_source_line(source: &str, line_num: u32) -> Option<String> {
    if line_num == 0 {
        return None;
    }
    source.lines().nth((line_num - 1) as usize).map(|s| s.to_string())
}

fn create_project(project_name: &str, _lang: &str) -> anyhow::Result<()> {
    let project_path = PathBuf::from(project_name);
    if project_path.exists() {
        anyhow::bail!("目录 {} 已存在", project_name);
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
    // 语言包已内置到 rzc 可执行文件中，无需复制
    fs::write(
        project_path.join("src/main.zh"),
        "函数 主函数() {\n    打印行!(\"你好，世界！\");\n}\n",
    )?;
    fs::write(
        project_path.join("README.md"),
        format!("# {}\n\n使用 `rzc run src/main.zh` 运行。\n", project_name),
    )?;
    println!("✅ 项目 '{}' 创建成功！", project_name);
    println!("   语言包已内置，无需手动复制。如需自定义，可创建 lang-packs/zh/ 目录覆盖。");
    Ok(())
}

fn load_mapping(
    lang_pack_path: Option<PathBuf>,
    source_file: Option<&Path>,
) -> anyhow::Result<MappingManager> {
    // 1. 如果用户通过 --lang-pack 指定了外部目录，强制使用
    if let Some(path) = lang_pack_path {
        return MappingManager::load_from_dir(&path)
            .map_err(|e| anyhow::anyhow!("加载语言包失败: {}", e));
    }
    // 2. 根据源文件扩展名确定语言代码
    let extension = source_file
        .and_then(|f| f.extension())
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let lang_code = get_lang_code_from_extension(extension).ok_or_else(|| {
        let available = lang_manager::all_available_extensions();
        anyhow::anyhow!(
            "无法识别源文件扩展名 '{}'。当前可用扩展名：{}。或通过 --lang-pack 指定语言包目录。",
            extension,
            if available.is_empty() {
                "无".to_string()
            } else {
                available
                    .iter()
                    .map(|e| format!(".{}", e))
                    .collect::<Vec<_>>()
                    .join("、")
            }
        )
    })?;
    // 3. 如果项目根目录存在 lang-packs/<lang>/ 目录，优先使用（自定义覆盖）
    let local_path = PathBuf::from(format!("lang-packs/{}", lang_code));
    if local_path.exists() {
        return MappingManager::load_from_dir(&local_path)
            .map_err(|e| anyhow::anyhow!("加载本地语言包失败: {}", e));
    }
    // 4. 全局用户语言包目录
    let global_path = lang_manager::global_lang_dir().join(&lang_code);
    if global_path.exists() {
        return MappingManager::load_from_dir(&global_path)
            .map_err(|e| anyhow::anyhow!("加载全局语言包失败: {}", e));
    }
    // 5. 回退到内置语言包
    let builtin = builtin_lang::get_builtin_data(&lang_code).ok_or_else(|| {
        anyhow::anyhow!(
            "语言包 '{}' 未内置，请通过 `rzc lang install {}` 下载安装。",
            lang_code,
            lang_code
        )
    })?;
    MappingManager::load_from_builtin(builtin.keywords_toml, builtin.module_paths_toml, builtin.crates_data)
        .map_err(|e| anyhow::anyhow!("加载内置语言包失败: {}", e))
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
