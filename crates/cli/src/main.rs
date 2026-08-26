// rzc 命令行入口 - 多语言 Rust 教学方言编译器
//
// 提供 init / run / check / eject / lang / mapping 等子命令，
// 将母语 Rust 源码实时转译为标准 Rust 并调用 cargo 编译/运行。

use clap::{FromArgMatches, Parser, Subcommand};
use i18n_rust_engine::mapping_manager::MappingManager;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod builtin_lang;
mod install;
mod lang_manager;
mod mapping_check;
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
    /// 为当前项目添加第三方依赖（封装 cargo add，附带母语映射提示）
    Add {
        /// crate 名或 名称@版本，可多个（如 serde tokio@1）
        #[arg(required = true)]
        crates: Vec<String>,
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
    /// 安装配套组件（语言服务器 i18n-rust-lsp 等）
    Install {
        #[command(subcommand)]
        subcommand: Option<InstallCommand>,
    },
    /// 诊断工具链环境：内置工具链 / PATH / 版本对比
    Doctor,
}

#[derive(Subcommand)]
enum InstallCommand {
    /// 安装语言服务器 i18n-rust-lsp（VS Code 扩展的补全/诊断后端）
    Lsp {
        /// 已存在时强制覆盖安装
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    /// 一键安装内置工具链（standalone rustc/cargo/rust-analyzer，脱离 rustup）
    Toolchain {
        /// 工具链版本（默认与 rzc 锁定版本一致，如 1.98.0）
        #[arg(long, default_value = i18n_rust_engine::toolchain::LOCKED_TOOLCHAIN_VERSION)]
        version: String,
        /// rust-analyzer 官方 Release tag（默认锁定版本）
        #[arg(long, default_value = crate::install::RA_RELEASE_TAG)]
        ra_tag: String,
        /// 已存在时强制重新安装
        #[arg(short = 'f', long = "force")]
        force: bool,
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
        /// 输出文件路径（默认项目语言包根：<lang>/crates/<crate_name>.toml）
        #[arg(long)]
        output: Option<PathBuf>,
        /// 生成映射后同时将该 crate 加入当前项目的 Cargo.toml（cargo add）
        #[arg(long)]
        install: bool,
    },
    /// 校验第三方库映射质量：重复键/关键字避让/跨文件冲突/条目数一致性
    Check {
        /// 内置语言代码（如 zh）或语言包目录路径；省略时校验全部内置语言
        target: Option<String>,
    },
    /// 从源语言 crates 映射生成目标语言的翻译骨架（键保留待翻译，英文值不变）
    Scaffold {
        /// 源语言代码（内置语言，如 zh）
        source: String,
        /// 目标语言代码（新语言包目录名，如 vi）
        target: String,
        /// 输出目录（默认项目语言包根：<target>/crates/）
        #[arg(long)]
        output: Option<PathBuf>,
        /// 翻译方式：rule（默认，生成 TODO 骨架待人工翻译）或 deepseek（AI 自动翻译键名，需 DEEPSEEK_API_KEY）
        #[arg(long, default_value = "rule")]
        provider: String,
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
    use std::io::Read;
    let code = match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("Error: {err:#}");
            std::process::ExitCode::FAILURE
        }
    };
    // Windows 双击 rzc.exe（无参数）：帮助后等待按键，避免黑窗一闪而过。
    // 不依赖 is_terminal——双击场景的 stdin 终端检测在 Windows 上不可靠；
    // 管道/重定向场景 read 立即返回（EOF 或已有数据），不会阻塞。
    if std::env::args().len() == 1 {
        println!();
        println!("按任意键退出...");
        let mut buf = [0u8; 1];
        let _ = std::io::stdin().read_exact(&mut buf);
    }
    code
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
            let manager = load_mapping(lang_pack.clone(), Some(&file))?;
            let project_root = find_project_root(&file)?;
            // 入口文件写入 src/main.rs 作为编译目标
            let source_path = project_root.join("src/main.rs");
            fs::write(&source_path, transpile_to_english(&source, &manager))?;
            // 同步转译项目内其他方言文件，保证多文件项目的 mod 引用链可用
            transpile_project_files(&project_root, &file, &manager)?;

            // 单文件项目直调 rustc：绕开 cargo 的索引/项目结构（教学单文件
            // 场景编译更快、无网络索引问题）；多文件/有依赖项目回退 cargo
            if can_use_direct_rustc(&project_root, &file) {
                return run_direct_rustc(
                    &ui,
                    &project_root,
                    &source_path,
                    &lang_pack,
                    &manager,
                    &source,
                    &file,
                );
            }

            // --message-format=json：编译诊断（warning/error）走 JSON 行翻译，
            // 程序自身 stdout/stderr 原样透传（cargo 不包装子进程输出），
            // 避免英文警告与程序输出混淆，也无需二次编译。
            let mut child = Command::new(resolve_cargo())
                .args(["run", "--message-format=json"])
                .current_dir(&project_root)
                .stdin(Stdio::inherit())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| {
                    anyhow::anyhow!(
                        "{}",
                        ui.f(
                            "cargo_run_failed",
                            &[&project_root.display().to_string(), &e.to_string()]
                        )
                    )
                })?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("cargo run stdout 管道不可用"))?;
            // stderr 线程逐行翻译 cargo 进度（Compiling/Finished 等），
            // 其余行（程序 stderr）原样透传
            let stderr_pipe = child
                .stderr
                .take()
                .ok_or_else(|| anyhow::anyhow!("cargo run stderr 管道不可用"))?;
            let stderr_handle = std::thread::spawn(move || {
                let ui = ui::Ui::global();
                let reader = BufReader::new(stderr_pipe);
                for line in reader.lines() {
                    match line {
                        Ok(line) => eprintln!("{}", translate_cargo_progress(&line, &ui)),
                        Err(_) => break,
                    }
                }
            });
            let reader = BufReader::new(stdout);
            // 收集 cargo JSON 诊断行（含 reason 字段），其余行视为程序输出原样透传
            let mut json_lines = String::new();
            for line in reader.lines() {
                let line = line.map_err(|e| {
                    anyhow::anyhow!(
                        "{}",
                        ui.f(
                            "cargo_run_failed",
                            &[&project_root.display().to_string(), &e.to_string()]
                        )
                    )
                })?;
                if line.starts_with('{')
                    && let Ok(value) = serde_json::from_str::<serde_json::Value>(&line)
                    && value.get("reason").is_some()
                {
                    json_lines.push_str(&line);
                    json_lines.push('\n');
                    continue;
                }
                println!("{line}");
            }
            let status = child.wait().map_err(|e| {
                anyhow::anyhow!(
                    "{}",
                    ui.f(
                        "cargo_run_failed",
                        &[&project_root.display().to_string(), &e.to_string()]
                    )
                )
            })?;
            let _ = stderr_handle.join();

            // 编译诊断翻译（warning/error 均覆盖）；
            // 无诊断且成功时静默（程序已运行，不再提示编译状态）
            if !json_lines.is_empty() {
                let _ = translate_cargo_diagnostics(
                    &json_lines,
                    "",
                    &ui,
                    &lang_pack,
                    &project_root,
                    &manager,
                    &source,
                    &file,
                    status.success(),
                    true,
                );
            }

            // 传播被运行程序的退出码（信号终止等无码场景回退 1）
            Ok(status
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

            // 单文件项目直调 rustc（绕开 cargo）；多文件/有依赖项目回退 cargo
            if can_use_direct_rustc(&project_root, &file) {
                return check_direct_rustc(
                    &ui,
                    &project_root,
                    &source_path,
                    &lang_pack,
                    &manager,
                    &source,
                    &file,
                );
            }

            let output = Command::new(resolve_cargo())
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

            // 结构化诊断翻译（与 run 编译失败路径共用同一管线）
            let stderr_text = String::from_utf8_lossy(&output.stderr).to_string();
            let rustc_output = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                stderr_text
            );
            let _ = translate_cargo_diagnostics(
                &rustc_output,
                &stderr_text,
                &ui,
                &lang_pack,
                &project_root,
                &manager,
                &source,
                &file,
                output.status.success(),
                false, // check 场景：无诊断且成功时提示“编译成功”
            );
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
        CliCommand::Install { subcommand } => {
            // 省略子命令时默认安装全部组件（当前仅语言服务器）
            match subcommand.unwrap_or(InstallCommand::Lsp { force: false }) {
                InstallCommand::Lsp { force } => install::install_lsp(&ui, force)?,
                InstallCommand::Toolchain {
                    version,
                    ra_tag,
                    force,
                } => install::install_toolchain(&ui, &version, &ra_tag, force)?,
            }
            Ok(std::process::ExitCode::SUCCESS)
        }
        CliCommand::Doctor => install::doctor().map(|()| std::process::ExitCode::SUCCESS),
        CliCommand::Lang { subcommand } => {
            handle_lang_command(subcommand).map(|()| std::process::ExitCode::SUCCESS)
        }
        CliCommand::Add { crates } => handle_add_command(&crates),
        CliCommand::Mapping { subcommand } => match subcommand {
            MappingCommand::Auto {
                crate_name,
                lang,
                provider,
                output,
                install,
            } => {
                let lang = lang.unwrap_or_else(mapping_gen::detect_system_language);
                i18n_rust_engine::语言::set_language(&lang);
                let output_path = output.unwrap_or_else(|| {
                    // 默认写入项目语言包根：从 cwd 向上找 Cargo.toml，
                    // 保证任意子目录下执行都落到项目本地语言包（load_mapping 同一位置查找）；
                    // 主仓库内落到 crates/engine/lang-packs/（单一数据源），用户项目落 lang-packs/
                    let base = std::env::current_dir()
                        .ok()
                        .and_then(|cwd| find_project_root_upward(&cwd))
                        .unwrap_or_else(|| PathBuf::from("."));
                    lang_pack_root_of(&base).join(format!("{}/crates/{}.toml", lang, crate_name))
                });
                mapping_gen::run_auto_generate(&crate_name, &lang, &provider, &output_path)
                    .map(|()| std::process::ExitCode::SUCCESS)
                    .inspect(|_| {
                        // --install：生成成功后把 crate 加入当前项目依赖（用户项目内执行时）
                        if install {
                            install_crate_to_current_project(&crate_name);
                        }
                        // 生成后自动对所在语言包跑一次冲突检测（仅提示，不改变退出码：
                        // 语言包可能存在历史遗留问题，生成成功与否以写入结果为准）
                        if let Some(lang_dir) = output_path.parent().and_then(|p| p.parent())
                            && lang_dir.join("keywords.toml").exists()
                            && let Some(dir_str) = lang_dir.to_str()
                        {
                            let _ = mapping_check::run_check(Some(dir_str));
                        }
                    })
            }
            MappingCommand::Check { target } => {
                // check 输出的语言默认跟随系统语言
                let lang = mapping_gen::detect_system_language();
                i18n_rust_engine::语言::set_language(&lang);
                match mapping_check::run_check(target.as_deref()) {
                    Ok(true) => Ok(std::process::ExitCode::SUCCESS),
                    Ok(false) => Ok(std::process::ExitCode::FAILURE),
                    Err(err) => Err(err),
                }
            }
            MappingCommand::Scaffold {
                source,
                target,
                output,
                provider,
            } => {
                let lang = mapping_gen::detect_system_language();
                i18n_rust_engine::语言::set_language(&lang);
                mapping_check::run_scaffold(&source, &target, output.as_deref(), &provider)
                    .map(|()| std::process::ExitCode::SUCCESS)
            }
        },
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

/// 处理 `rzc add` 子命令：封装 cargo add，成功后提示母语映射可用性
fn handle_add_command(crates: &[String]) -> anyhow::Result<std::process::ExitCode> {
    let ui = ui::Ui::global();
    let cwd = std::env::current_dir()?;
    let project_root = find_project_root_upward(&cwd)
        .ok_or_else(|| anyhow::anyhow!("{}", ui.t("add_no_project")))?;
    let status = Command::new(resolve_cargo())
        .arg("add")
        .args(crates)
        .current_dir(&project_root)
        .status()
        .map_err(|e| anyhow::anyhow!("{}", ui.f("add_cargo_failed", &[&e.to_string()])))?;
    if !status.success() {
        // cargo add 自身已输出错误详情，直接传播退出码
        return Ok(std::process::ExitCode::FAILURE);
    }
    let lang_code = ui::detect_ui_lang();
    for spec in crates {
        // 依赖名取 @版本 前段，并将 - 归一为 _（代码中 use 路径用下划线）
        let crate_name = spec.split('@').next().unwrap_or(spec).replace('-', "_");
        match find_crate_mapping_alias(&lang_code, &project_root, &crate_name) {
            Some(alias) => println!("{}", ui.f("add_mapping_ready", &[&crate_name, &alias])),
            None => println!(
                "{}",
                ui.f("add_mapping_missing", &[&crate_name, &crate_name])
            ),
        }
    }
    Ok(std::process::ExitCode::SUCCESS)
}

/// 查找 crate 在当前语言映射中的母语别名（项目/全局语言包 > 内置）
///
/// 扫描 crates/*.toml 的 ["模块路径"] 节：值的首段（:: 分隔）与 crate 名
/// 匹配即命中（如 "HTTP客户端" = "reqwest" → 首段 reqwest）；
/// 命中时返回对应母语键作为示例提示。
fn find_crate_mapping_alias(
    lang_code: &str,
    project_root: &Path,
    crate_name: &str,
) -> Option<String> {
    // 1. 项目内语言包与全局用户语言包的 crates/ 目录
    let dirs = [
        lang_pack_root_of(project_root)
            .join(lang_code)
            .join("crates"),
        lang_manager::global_lang_dir()
            .join(lang_code)
            .join("crates"),
    ];
    for dir in &dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path)
                && let Some(alias) = find_alias_in_toml(&content, crate_name)
            {
                return Some(alias);
            }
        }
    }
    // 2. 内置语言包（未知语言代码自动回退中文）
    let builtin = builtin_lang::get_builtin_data(lang_code);
    for (_, content) in builtin.crates_data {
        if let Some(alias) = find_alias_in_toml(content, crate_name) {
            return Some(alias);
        }
    }
    None
}

/// 在单个映射 TOML 内容中查找 crate 对应的母语别名
fn find_alias_in_toml(content: &str, crate_name: &str) -> Option<String> {
    let value: toml::Value = toml::from_str(content).ok()?;
    let paths = value.get("模块路径")?.as_table()?;
    for (key, val) in paths {
        let Some(en_path) = val.as_str() else {
            continue;
        };
        let first_seg = en_path.split("::").next().unwrap_or(en_path);
        if first_seg.replace('-', "_") == crate_name {
            return Some(key.clone());
        }
    }
    None
}

/// mapping auto --install：把 crate 加入当前项目依赖（找不到项目时仅告警）
fn install_crate_to_current_project(crate_name: &str) {
    let ui = ui::Ui::global();
    let Some(root) = std::env::current_dir()
        .ok()
        .and_then(|cwd| find_project_root_upward(&cwd))
    else {
        println!("{}", ui.t("mapping_auto_install_no_project"));
        return;
    };
    match Command::new(resolve_cargo())
        .arg("add")
        .arg(crate_name)
        .current_dir(&root)
        .status()
    {
        Ok(status) if status.success() => {
            println!("{}", ui.f("mapping_auto_installed", &[crate_name]))
        }
        Ok(status) => println!(
            "{}",
            ui.f(
                "mapping_auto_install_failed",
                &[crate_name, &status.to_string()]
            )
        ),
        Err(e) => println!(
            "{}",
            ui.f("mapping_auto_install_failed", &[crate_name, &e.to_string()])
        ),
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

/// 项目内语言包根目录：主仓库 zrRust 为单副本结构 `crates/engine/lang-packs/`
///（编译期内嵌与文件系统消费共用同一份数据）；
/// 普通用户项目仍沿用 `lang-packs/` 约定（自定义覆盖）
pub(crate) fn lang_pack_root_of(base: &Path) -> PathBuf {
    let engine_pack = base.join("crates/engine/lang-packs");
    if engine_pack.is_dir() {
        engine_pack
    } else {
        base.join("lang-packs")
    }
}

/// 统一转译管线（复用 engine）：Unicode 检查 → 关键字/宏转译 → 模块路径替换 → 别名替换 → 非 ASCII 模块注解
fn transpile_to_english(source: &str, manager: &MappingManager) -> String {
    let code = i18n_rust_engine::transpile_pipeline(source, manager).output;
    annotate_non_ascii_mods(&code)
}

/// 解析 cargo 可执行文件：内置工具链（~/.rz/toolchain）优先，PATH 回退；
/// 找不到时返回 "cargo" 由系统报错（保持与旧行为一致的报错信息）
pub fn resolve_cargo() -> PathBuf {
    i18n_rust_engine::toolchain::find_toolchain_bin("cargo")
        .unwrap_or_else(|| PathBuf::from("cargo"))
}

/// 解析 rustc 可执行文件：内置工具链优先，PATH 回退
pub fn resolve_rustc() -> PathBuf {
    i18n_rust_engine::toolchain::find_toolchain_bin("rustc")
        .unwrap_or_else(|| PathBuf::from("rustc"))
}

/// 判断是否可单文件直调 rustc：src/ 下仅一个方言文件且 Cargo.toml 无依赖
///
/// 教学单文件项目（仅 main.zh）直调 rustc 绕开 cargo：无索引网络开销、
/// 无需 Cargo.lock 预生成，编译诊断格式与 cargo 完全一致；
/// 多文件（mod 引用）或有依赖的项目回退 cargo 流程。
fn can_use_direct_rustc(project_root: &Path, file: &Path) -> bool {
    let dialects = ["zh", "ja", "de", "es", "fr", "pt", "ru", "ko", "hi", "ar"];
    // 方言文件计数：src/ 与项目根都扫（教学项目 src/main.zh 为主，
    // 项目根也可能放 main.zh）；超过 1 个视为多文件项目
    let mut dialect_count = 0usize;
    for dir in [project_root.join("src"), project_root.to_path_buf()] {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if dialects.iter().any(|d| name.ends_with(&format!(".{d}"))) {
                    dialect_count += 1;
                }
            }
        }
    }
    if dialect_count != 1 {
        return false;
    }
    // 入口文件必须是 src/main.zh（聚合 main.rs 已写入）
    if file.file_name().and_then(|s| s.to_str()) != Some("main.zh") {
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
fn run_direct_rustc(
    ui: &ui::Ui,
    project_root: &Path,
    source_path: &Path,
    lang_pack: &Option<PathBuf>,
    manager: &MappingManager,
    source: &str,
    file: &Path,
) -> anyhow::Result<std::process::ExitCode> {
    let exe = std::env::temp_dir().join(format!("rzc-run-{}.exe", std::process::id()));
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
            ui,
            lang_pack,
            project_root,
            manager,
            source,
            file,
            ok,
            true,
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
fn check_direct_rustc(
    ui: &ui::Ui,
    project_root: &Path,
    source_path: &Path,
    lang_pack: &Option<PathBuf>,
    manager: &MappingManager,
    source: &str,
    file: &Path,
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
        ui,
        lang_pack,
        project_root,
        manager,
        source,
        file,
        output.status.success(),
        false,
    );
    Ok(exit_code)
}

/// 翻译 cargo 的人类可读进度行（json 模式下这些行仍输出到 stderr）
///
/// 命中固定前缀（Compiling/Finished/Running 等）时翻译；
/// 其余行（程序 stderr 等）原样返回。
fn translate_cargo_progress(line: &str, ui: &ui::Ui) -> String {
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

/// 解析 cargo --message-format=json 输出并翻译为教学化诊断（check 与 run 共用）
///
/// 返回是否成功输出了翻译后的教学诊断；调用方据此决定是否回退原始文本。
/// `cargo_ok=false` 且无可解析诊断时原样输出 cargo 消息，绝不虚报“编译成功”。
/// `silent_success=true`（run 场景）时，无诊断且编译成功保持静默——
/// 程序已运行，不再提示“编译成功”。
#[allow(clippy::too_many_arguments)]
fn translate_cargo_diagnostics(
    rustc_output: &str,
    stderr_text: &str,
    ui: &ui::Ui,
    lang_pack: &Option<PathBuf>,
    project_root: &Path,
    manager: &MappingManager,
    source: &str,
    file: &Path,
    cargo_ok: bool,
    silent_success: bool,
) -> bool {
    use i18n_rust_engine::diagnostic::{
        DiagnosticTranslator, ErrorTranslationManager, parse_diagnostic_output,
    };

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
        } else {
            // cargo 失败但无可解析的 JSON 诊断（Cargo.toml 语法错误、
            // 链接错误等）：原样输出 cargo 消息，绝不虚报“编译成功”
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
        for teaching in &mut teaching_list {
            teaching.locations.iter_mut().for_each(|loc| {
                loc.file_name = original_filename.clone();
                loc.source_text = get_chinese_source_line(source, loc.line_start);
            });
        }
        if teaching_list.is_empty() {
            if cargo_ok {
                if !silent_success {
                    println!("{}", ui.t("success_compile"));
                }
            } else {
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

/// 从 cargo --message-format=json 输出提取未声明的 crate 名
///
/// 识别 unresolved import（E0432）与 failed to resolve（E0433）诊断，
/// 候选提取复用 engine 共享逻辑（已去重，排除标准库与保留路径）。
fn extract_unresolved_crates(rustc_output: &str) -> Vec<String> {
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
                if !matches!(
                    seg.as_str(),
                    "std" | "core" | "alloc" | "self" | "super" | "crate" | "proc_macro"
                ) && !seg.chars().next().is_some_and(|c| c.is_ascii_digit())
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

/// 探测本机当前生效工具链的通道号（如 `1.98` / `nightly`）
///
/// 解析 `rustc --version` 输出的第二段（形如 `1.98.0 (哈希 日期)` 或 `1.98.0-nightly`），
/// 取主次版本号作为 channel；nightly/beta 通道原样返回。
/// rustc 不在 PATH 或输出格式异常时返回 None（调用方跳过生成锁定文件）。
fn detect_toolchain_channel() -> Option<String> {
    let output = std::process::Command::new(resolve_rustc())
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout.split_whitespace().nth(1)?;
    if version.contains("nightly") {
        return Some("nightly".to_string());
    }
    if version.contains("beta") {
        return Some("beta".to_string());
    }
    // "1.98.0" → "1.98"（channel 只保留主次版本，补丁版本由工具链自行解析）
    let mut parts = version.split('.');
    let major = parts.next()?;
    let minor = parts.next()?;
    if major.chars().all(|c| c.is_ascii_digit()) && minor.chars().all(|c| c.is_ascii_digit()) {
        Some(format!("{major}.{minor}"))
    } else {
        None
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
    // 版本锁定：固定到本机当前工具链版本（动态探测，避免硬编码随时间过时，
    // 导致 rust-analyzer 等工具报"工具链过于陈旧"）；探测失败时不生成锁定文件，
    // 项目跟随系统默认工具链。components 含 rust-analyzer/rust-src 供 IDE 使用。
    if let Some(channel) = detect_toolchain_channel() {
        fs::write(
            project_path.join("rust-toolchain.toml"),
            format!(
                "[toolchain]\nchannel = \"{channel}\"\ncomponents = [\"rustc\", \"cargo\", \"rust-analyzer\", \"rust-src\"]\n"
            ),
        )?;
    }

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
    // 3. 项目内语言包目录存在时优先使用（自定义覆盖）：
    //    主仓库为 crates/engine/lang-packs/<lang>（单一数据源），用户项目为 lang-packs/<lang>；
    //    项目根从源文件向上查找；源文件不在项目内时回退 cwd，兼容旧用法
    let mut local_candidates: Vec<PathBuf> = Vec::new();
    if let Some(file) = source_file
        && let Some(parent) = file.parent()
        && let Some(root) = find_project_root_upward(parent)
    {
        local_candidates.push(lang_pack_root_of(&root).join(&lang_code));
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    local_candidates.push(lang_pack_root_of(&cwd).join(&lang_code));
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
        .mut_subcommand("add", |cmd| {
            cmd.about(ui.t("cmd_add_about"))
                .mut_arg("crates", |arg| arg.help(ui.t("arg_add_crates_help")))
        })
        .mut_subcommand("install", |cmd| {
            cmd.about(ui.t("cmd_install_about"))
                .mut_subcommand("lsp", |sub| sub.about(ui.t("cmd_install_lsp_about")))
        })
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
                        .mut_arg("install", |arg| arg.help(ui.t("arg_install_help")))
                })
                .mut_subcommand("check", |sub| {
                    sub.about(ui.t("cmd_mapping_check_about"))
                        .mut_arg("target", |arg| {
                            arg.help(ui.t("cmd_mapping_check_target_help"))
                        })
                })
                .mut_subcommand("scaffold", |sub| {
                    sub.about(ui.t("cmd_mapping_scaffold_about"))
                        .mut_arg("source", |arg| {
                            arg.help(ui.t("cmd_mapping_scaffold_source_help"))
                        })
                        .mut_arg("target", |arg| {
                            arg.help(ui.t("cmd_mapping_scaffold_target_help"))
                        })
                        .mut_arg("output", |arg| arg.help(ui.t("arg_output_help")))
                        .mut_arg("provider", |arg| {
                            arg.help(ui.t("cmd_mapping_scaffold_provider_help"))
                        })
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
        annotate_non_ascii_mods, can_use_direct_rustc, detect_toolchain_channel,
        extract_unresolved_crates, find_alias_in_toml, get_lang_code_from_extension,
        transpile_project_files, transpile_to_english,
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

    /// 工具链通道探测：开发/CI 环境必有 rustc；主次版本号形如 `1.98`，通道词原样
    #[test]
    fn test_detect_toolchain_channel() {
        let channel = detect_toolchain_channel().expect("测试环境应有 rustc");
        let is_version = channel.split_once('.').is_some_and(|(a, b)| {
            !a.is_empty() && !b.is_empty() && a.chars().chain(b.chars()).all(|c| c.is_ascii_digit())
        });
        assert!(
            is_version || matches!(channel.as_str(), "nightly" | "beta"),
            "意外的通道格式：{channel}"
        );
    }

    /// 已内置的语言包扩展名可解析出语言代码
    ///
    /// 持环境变量锁：扩展名映射会扫描全局语言包目录（受 RZ_LANG_DIR 影响），
    /// lang_manager 的环境变量测试并发修改该变量时会污染本测试
    #[test]
    fn test_lang_code_from_extension_builtin() {
        let _lock = crate::lang_manager::tests::env_lock();
        assert_eq!(get_lang_code_from_extension("zh").as_deref(), Some("zh"));
        assert_eq!(get_lang_code_from_extension("de").as_deref(), Some("de"));
        assert_eq!(get_lang_code_from_extension("ru").as_deref(), Some("ru"));
        assert_eq!(get_lang_code_from_extension("ja").as_deref(), Some("ja"));
        assert_eq!(get_lang_code_from_extension("hi").as_deref(), Some("hi"));
    }

    /// 未知扩展名返回 None（同样受 RZ_LANG_DIR 影响，需持锁）
    #[test]
    fn test_lang_code_from_extension_unknown() {
        let _lock = crate::lang_manager::tests::env_lock();
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

    /// 映射 TOML 中按 crate 首段查找母语别名；连字符 crate 名归一匹配
    #[test]
    fn test_find_alias_in_toml() {
        let toml = "[\"模块路径\"]\n\"HTTP客户端\" = \"reqwest\"\n\"时间\" = \"chrono::prelude\"\n";
        assert_eq!(
            find_alias_in_toml(toml, "reqwest").as_deref(),
            Some("HTTP客户端")
        );
        assert_eq!(find_alias_in_toml(toml, "chrono").as_deref(), Some("时间"));
        assert_eq!(find_alias_in_toml(toml, "tokio"), None);
        assert_eq!(
            find_alias_in_toml(
                "[\"模块路径\"]\n\"序列化\" = \"serde-json\"\n",
                "serde_json"
            )
            .as_deref(),
            Some("序列化")
        );
    }
}
