//! 多语言 Rust 教学方言编译器 CLI
//!
//! 提供 init / run / check / eject / lang / mapping 等子命令，
//! 将母语 Rust 源码实时转译为标准 Rust 并调用 cargo 编译/运行。

use clap::{FromArgMatches, Parser, Subcommand};
use i18n_rust_engine::映射管理::映射管理器;
use i18n_rust_engine::缓存::转译缓存;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod builtin_lang;
mod crate_registry;
mod diagnostics;
mod install;
mod lang_manager;
mod mapping_check;
mod mapping_coverage;
mod mapping_gen;
mod temp_guard;
mod ui;

use diagnostics::{
    流式翻译器, 直调rustc检查, 直调rustc运行, 翻译cargo诊断, 能否直调rustc, 诊断上下文,
};
use lang_manager::语言来源;
// 非 ASCII 模块名 #[path] 注解：CLI 与 LSP 镜像共享同一引擎实现
use i18n_rust_engine::模块路径::{标注非西文模块, 标注非西文模块并行号};

#[derive(Parser)]
#[command(name = "rzc", version)]
// 兜底文案（本地化clap 会按界面语言覆盖）；用英文避免硬编码中文
#[command(about = "Multi-language Rust teaching dialect compiler")]
struct CliArgs {
    /// Suppress teaching lint hints (beginner code-style warnings; Unicode confusables / full-width punctuation warnings are unaffected)
    #[arg(long, global = true)]
    no_lint: bool,
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    Init {
        project_name: String,
        // 缺省跟随系统 locale（LC_ALL/LANG）：全球用户的第一个项目应是自己的母语
        #[arg(short, long, default_value_t = mapping_gen::检测系统语言())]
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
        /// Auto-fix full-width punctuation (writes to the source file; only characters with a half-width counterpart are converted)
        #[arg(long)]
        fix: bool,
    },
    Eject {
        file: PathBuf,
        #[arg(short, long)]
        lang_pack: Option<PathBuf>,
    },
    /// Transpile preview: convert dialect source to standard Rust and print to stdout (no files written)
    Transpile {
        file: PathBuf,
        #[arg(short, long)]
        lang_pack: Option<PathBuf>,
    },
    /// Add third-party dependencies to the current project (wraps cargo add, with native-language mapping hints)
    Add {
        /// crate name or name@version, repeatable (e.g. serde tokio@1)
        #[arg(required = true)]
        crates: Vec<String>,
    },
    /// Language pack management (list / install / remove)
    Lang {
        #[command(subcommand)]
        subcommand: LangCommand,
    },
    /// Generate third-party crate mappings (extract APIs from installed crates)
    Mapping {
        #[command(subcommand)]
        subcommand: MappingCommand,
    },
    /// Shared third-party crate registry (search / install / list / remove / update / publish)
    Crate {
        #[command(subcommand)]
        subcommand: CrateCommand,
    },
    /// Install companion components (language server i18n-rust-lsp, etc.)
    Install {
        #[command(subcommand)]
        subcommand: Option<InstallCommand>,
    },
    /// Diagnose the toolchain environment: bundled toolchain / PATH / version comparison
    Doctor,
    /// Native-language ↔ Rust mapping cheat sheet (keywords / module paths / aliases / derive traits)
    Cheat {
        /// Built-in language code (e.g. zh) or a language-pack directory path; auto-detected from the system language when omitted
        lang: Option<String>,
        /// Output as a Markdown table for embedding into docs
        #[arg(long)]
        markdown: bool,
    },
}

#[derive(Subcommand)]
enum InstallCommand {
    /// Install the language server i18n-rust-lsp (completion / diagnostics backend for the VS Code extension)
    Lsp {
        /// Force overwrite if already installed
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    /// One-click install of the bundled toolchain (standalone rustc/cargo/rust-analyzer, no rustup required)
    Toolchain {
        /// Toolchain version (defaults to the version rzc is locked to, e.g. 1.98.0)
        #[arg(long, default_value = i18n_rust_engine::工具链::锁定工具链版本)]
        version: String,
        /// Official rust-analyzer release tag (defaults to the locked version)
        #[arg(long, default_value = crate::install::分析器发布标签)]
        ra_tag: String,
        /// Upgrade rust-analyzer only (skip the ~300 MB rustc/cargo re-download)
        #[arg(long)]
        ra_only: bool,
        /// Force reinstall if already installed
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
}

#[derive(Subcommand)]
enum MappingCommand {
    /// Auto-generate third-party crate mapping files: extract the crate's public API; AI or rules generate names and explanations in the target language
    Auto {
        /// Target crate name (must be installed or available from crates.io)
        crate_name: String,
        /// Target language (language-pack directory name, e.g. zh, ru; auto-detected from the system language by default)
        #[arg(long)]
        lang: Option<String>,
        /// AI provider: deepseek (default; requires the DEEPSEEK_API_KEY environment variable) or rule (offline rule mode)
        #[arg(long, default_value = "deepseek")]
        provider: String,
        /// Pin the target crate version (e.g. 2.11.5 exact; 2.11 / 2 = latest in that line); uses the latest when omitted (not reproducible)
        #[arg(long)]
        target_version: Option<String>,
        /// Output file path (defaults to the project language-pack root: <lang>/crates/<crate_name>.toml)
        #[arg(long)]
        output: Option<PathBuf>,
        /// Also add the crate to the current project's Cargo.toml after generating mappings (cargo add)
        #[arg(long)]
        install: bool,
    },
    /// Validate third-party crate mapping quality: duplicate keys / keyword avoidance / cross-file conflicts / entry-count consistency
    Check {
        /// Built-in language code (e.g. zh) or a language-pack directory path; validates all built-in languages when omitted
        target: Option<String>,
    },
    /// Corpus coverage matrix: validate language-pack keyword/API coverage against real backend sources; lists missing native-language mappings
    Coverage {
        /// Built-in language code (e.g. zh); checks all built-in languages when omitted
        #[arg(long)]
        lang: Option<String>,
    },
    /// Generate a translation skeleton for the target language from the source-language crates mappings (keys kept for translation, English values unchanged)
    Scaffold {
        /// Source language code (a built-in language, e.g. zh)
        source: String,
        /// Target language code (new language-pack directory name, e.g. vi)
        target: String,
        /// Output directory (defaults to the project language-pack root: <target>/crates/)
        #[arg(long)]
        output: Option<PathBuf>,
        /// Translation mode: rule (default; generates a TODO skeleton for manual translation) or deepseek (AI translates keys; requires DEEPSEEK_API_KEY)
        #[arg(long, default_value = "rule")]
        provider: String,
    },
}

#[derive(Subcommand)]
enum CrateCommand {
    /// Search third-party crate mappings published in the registry (optional keyword filter)
    Search {
        /// Keyword (matches crate name / language / author); lists everything when omitted (sorted by downloads)
        keyword: Option<String>,
    },
    /// Install a single third-party crate mapping: copy it from the registry into the global language pack
    Install {
        /// Crate name (hyphens normalized to underscores)
        crate_name: String,
        /// Target language code (e.g. zh)
        #[arg(long)]
        lang: String,
        /// Force overwrite if already installed
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    /// List installed community mappings
    List,
    /// Remove an installed community mapping (from the global language pack and the manifest)
    Remove {
        /// Crate name
        crate_name: String,
        /// Language code
        #[arg(long)]
        lang: String,
    },
    /// Re-fetch all mappings according to the installed manifest (pick up others' updates)
    Update,
    /// Publish a local mapping to the registry (quality gate runs first)
    Publish {
        /// Crate name
        crate_name: String,
        /// Language code
        #[arg(long)]
        lang: String,
        /// Mapping file path (when omitted, searched in common locations: global/project language pack or the current directory)
        #[arg(long)]
        file: Option<PathBuf>,
        /// Translator credit (defaults to git user.name)
        #[arg(long)]
        author: Option<String>,
    },
}

#[derive(Subcommand)]
enum LangCommand {
    /// List all installed language packs (built-in + user-installed)
    List,
    /// Install a language pack: a local directory path is copied directly; a language code is downloaded from the remote repository
    Install {
        /// Local language-pack directory path, or a remote language code
        source: String,
        /// Force overwrite if already installed
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    /// Remove a user-installed language pack (built-in packs cannot be removed)
    Remove {
        /// Language code (language-pack directory name)
        lang_code: String,
    },
    /// Browse the remote language-pack marketplace (scan the remote repository; optional keyword filter)
    Search {
        /// Keyword (matches language code or display name); lists all remote packs when omitted
        keyword: Option<String>,
    },
}

fn main() -> std::process::ExitCode {
    use std::io::{IsTerminal, Read};
    // Windows 双击 rzc.exe（无参数且 stdout 是终端）：显示环境安装向导（逐项检查
    // 组件状态与解决方式）并停留，避免黑窗一闪而过。用 stdout 检测（双击时 stdout
    // 连控制台）而非 stdin（双击场景 stdin 句柄可能无效，read 立即 EOF 导致秒关）；
    // stdin 读取失败时停留数秒兜底。管道/重定向场景 stdout 非终端，走正常流程。
    if std::env::args().len() == 1 && std::io::stdout().is_terminal() {
        install::显示安装向导();
        println!();
        println!("按任意键退出...");
        let mut 缓冲 = [0u8; 1];
        if std::io::stdin().read_exact(&mut 缓冲).is_err() {
            std::thread::sleep(std::time::Duration::from_secs(8));
        }
        return std::process::ExitCode::SUCCESS;
    }
    match 执行() {
        Ok(码) => 码,
        Err(错) => {
            eprintln!("Error: {错:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn 执行() -> anyhow::Result<std::process::ExitCode> {
    // 按当前界面语言本地化 clap 帮助文本
    let 界面 = ui::界面::全局();
    // 同步引擎全局语言（错误/诊断/日志随界面语言输出）
    i18n_rust_engine::语言::设定语言(&ui::检测界面语言());
    let 命令行 = 本地化clap(&界面);
    // clap 自身错误（--help/--version 输出、非法参数等）按 clap 退出码直接退出
    let 参数集 = match CliArgs::from_arg_matches(&命令行.get_matches()) {
        Ok(参数集) => 参数集,
        Err(错) => 错.exit(),
    };

    // `--no-lint`：项目开发（非教学）场景静默教学 lint（初学者代码风格提示，
    // 每次转译刷屏）；Unicode 混淆/全角标点告警不受影响
    if 参数集.no_lint {
        i18n_rust_engine::教学检查::设定教学检查开关(false);
    }

    // 首次运行引导：终端交互场景下，首次执行教学核心命令时打印
    // 欢迎语与环境检查（rustc 缺失提示 + 下一步建议），仅一次
    // （~/.rz/first-run 标记文件）；CI/管道等非终端场景自动跳过。
    match &参数集.command {
        CliCommand::Run { .. } | CliCommand::Check { .. } | CliCommand::Init { .. } => {
            显示首次运行引导(&界面);
        }
        _ => {}
    }

    match 参数集.command {
        CliCommand::Init { project_name, lang } => {
            i18n_rust_engine::语言::设定语言(&lang);
            创建项目(&project_name, &lang)?;
            Ok(std::process::ExitCode::SUCCESS)
        }
        CliCommand::Run { file, lang_pack } => {
            let 界面 = 界面按文件(&file, &lang_pack);
            let 源码 = fs::read_to_string(&file)?;
            let 管理器 = 加载映射(lang_pack.clone(), Some(&file))?;
            let 项目根 = 定位项目根(&file)?;
            // 项目级声明上下文（跨文件声明豁免）：扫描 src/ 全部方言文件 +
            // 入口，收集模块名与声明名；入口文件、项目内文件与诊断重放
            // 共享同一上下文（否则跨文件调用的成员名被当库别名替换，E0599）
            let 项目上下文 = 收集项目上下文(&项目根, &file, &管理器);
            // 入口文件写入 src/main.rs 作为编译目标；会话缓存贯穿入口文件与
            // 项目内其他文件（并行转译共享命中，见 转译项目文件）
            let 源码路径 = 入口产物路径(&项目根, &file, &管理器);
            let 缓存 = std::sync::Mutex::new(i18n_rust_engine::缓存::转译缓存::持久默认项());
            let 转译产物 = 项目转译映射带缓存(
                &源码,
                &管理器,
                &mut 缓存
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
                Some(&项目上下文),
            );
            // 列映射：把 rustc 诊断的英文产物列号回译到母语源码列号
            let 列映射 =
                i18n_rust_engine::列映射::列映射表::r#构建(&源码, &转译产物.管线映射);
            // 写盘产物含 `#[path]` 注解插入行：保留行映射供诊断回译先行换算
            let (注解产物, 入口行映射) = 标注非西文模块并行号(&转译产物.产出);
            写入转译产物(&源码路径, &注解产物, &界面)?;
            // 同步转译项目内其他方言文件，保证多文件项目的 mod 引用链可用
            转译项目文件(&项目根, &file, &管理器, &缓存, &项目上下文)?;

            // 单文件项目直调 rustc：绕开 cargo 的索引/项目结构（教学单文件
            // 场景编译更快、无网络索引问题）；多文件/有依赖项目回退 cargo
            if 能否直调rustc(&项目根, &file) {
                return 直调rustc运行(
                    &界面,
                    &项目根,
                    &源码路径,
                    &lang_pack,
                    &管理器,
                    &源码,
                    &file,
                    &列映射,
                    &入口行映射,
                    &项目上下文,
                );
            }

            // --message-format=json：编译诊断（warning/error）走 JSON 行翻译，
            // 程序自身 stdout/stderr 原样透传（cargo 不包装子进程输出），
            // 避免英文警告与程序输出混淆，也无需二次编译。
            let mut 子进程 = Command::new(解析cargo路径())
                .args(["run", "--message-format=json"])
                .current_dir(&项目根)
                .stdin(Stdio::inherit())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|错| {
                    anyhow::anyhow!(
                        "{}",
                        界面.取文带参(
                            "cargo_run_failed",
                            &[&项目根.display().to_string(), &错.to_string()]
                        )
                    )
                })?;
            let 标准输出 = 子进程
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("cargo run stdout 管道不可用"))?;
            // stderr 线程逐行翻译：cargo 进度（Compiling/Finished 等）与程序
            // panic 输出本地化，其余行（程序普通 stderr）原样透传
            let 错误输出管道 = 子进程
                .stderr
                .take()
                .ok_or_else(|| anyhow::anyhow!("cargo run stderr 管道不可用"))?;
            let 错误输出线程 = std::thread::spawn(move || {
                let 界面 = ui::界面::全局();
                let mut 翻译器 = 流式翻译器::r#新建();
                let 读取器 = BufReader::new(错误输出管道);
                for 文本行 in 读取器.lines() {
                    match 文本行 {
                        Ok(文本行) => eprintln!("{}", 翻译器.翻译一行(&文本行, &界面)),
                        Err(_) => break,
                    }
                }
            });
            let 读取器 = BufReader::new(标准输出);
            // 收集 cargo JSON 诊断行（含 reason 字段），其余行视为程序输出原样透传
            let mut 诊断文本行 = String::new();
            for 文本行 in 读取器.lines() {
                let 文本行 = 文本行.map_err(|错| {
                    anyhow::anyhow!(
                        "{}",
                        界面.取文带参(
                            "cargo_run_failed",
                            &[&项目根.display().to_string(), &错.to_string()]
                        )
                    )
                })?;
                if 文本行.starts_with('{')
                    && let Ok(值) = serde_json::from_str::<serde_json::Value>(&文本行)
                    && 值.get("reason").is_some()
                {
                    诊断文本行.push_str(&文本行);
                    诊断文本行.push('\n');
                    continue;
                }
                println!("{文本行}");
            }
            let 状态 = 子进程.wait().map_err(|错| {
                anyhow::anyhow!(
                    "{}",
                    界面.取文带参(
                        "cargo_run_failed",
                        &[&项目根.display().to_string(), &错.to_string()]
                    )
                )
            })?;
            let _ = 错误输出线程.join();

            // 编译诊断翻译（warning/error 均覆盖）；
            // 无诊断且成功时静默（程序已运行，不再提示编译状态）
            if !诊断文本行.is_empty() {
                let _ = 翻译cargo诊断(
                    &诊断文本行,
                    "",
                    &诊断上下文 {
                        界面: &界面,
                        语言包: &lang_pack,
                        项目根: &项目根,
                        管理器: &管理器,
                        源码: &源码,
                        入口文件: &file,
                        列映射: Some(&列映射),
                        入口行映射: Some(&入口行映射),
                        项目: Some(&项目上下文),
                    },
                    状态.success(),
                    true,
                    // 输出已实时透传：构建成功而程序运行失败（如 panic）时
                    // 不补打“编译错误”标签
                    true,
                );
            }

            // 传播被运行程序的退出码（信号终止等无码场景回退 1）
            Ok(状态
                .code()
                .map(|码| std::process::ExitCode::from(码 as u8))
                .unwrap_or(std::process::ExitCode::FAILURE))
        }
        CliCommand::Check {
            file,
            lang_pack,
            fix,
        } => {
            let 界面 = 界面按文件(&file, &lang_pack);
            let mut 源码 = fs::read_to_string(&file)?;
            // 全角标点教学修复：中文输入法下最常见的编译错误来源之一。
            // --fix 时自动将代码位置（字符串/注释内除外）的全角标点改写为半角，
            // 并提示剩余需人工修改的字符（顿号/全角空格等）；
            // 不带 --fix 时警告由转译管线（log_warn）自动输出。
            if fix {
                let (修复文本, 数量) = i18n_rust_engine::全角标点::修复全角标点(&源码);
                if 数量 > 0 {
                    fs::write(&file, &修复文本)?;
                    源码 = 修复文本;
                    println!(
                        "{}",
                        界面.取文带参(
                            "fullwidth_fix_done",
                            &[&数量.to_string(), &file.display().to_string()]
                        )
                    );
                }
                let 剩余 = i18n_rust_engine::全角标点::查找全角标点(&源码).len();
                if 剩余 > 0 {
                    println!(
                        "{}",
                        界面.取文带参("fullwidth_fix_remaining", &[&剩余.to_string()])
                    );
                }
            }
            let 管理器 = 加载映射(lang_pack.clone(), Some(&file))?;
            let 项目根 = 定位项目根(&file)?;
            // 项目级声明上下文（跨文件声明豁免，同 run）
            let 项目上下文 = 收集项目上下文(&项目根, &file, &管理器);
            let 源码路径 = 入口产物路径(&项目根, &file, &管理器);
            let 缓存 = std::sync::Mutex::new(i18n_rust_engine::缓存::转译缓存::持久默认项());
            let 转译产物 = 项目转译映射带缓存(
                &源码,
                &管理器,
                &mut 缓存
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
                Some(&项目上下文),
            );
            // 列映射：把 rustc 诊断的英文产物列号回译到母语源码列号
            let 列映射 =
                i18n_rust_engine::列映射::列映射表::r#构建(&源码, &转译产物.管线映射);
            // 写盘产物含 `#[path]` 注解插入行：保留行映射供诊断回译先行换算
            let (注解产物, 入口行映射) = 标注非西文模块并行号(&转译产物.产出);
            写入转译产物(&源码路径, &注解产物, &界面)?;
            // 同步转译项目内其他方言文件，保证多文件项目的 mod 引用链可用
            转译项目文件(&项目根, &file, &管理器, &缓存, &项目上下文)?;

            // 单文件项目直调 rustc（绕开 cargo）；多文件/有依赖项目回退 cargo
            if 能否直调rustc(&项目根, &file) {
                return 直调rustc检查(
                    &界面,
                    &项目根,
                    &源码路径,
                    &lang_pack,
                    &管理器,
                    &源码,
                    &file,
                    &列映射,
                    &入口行映射,
                    &项目上下文,
                );
            }

            let 输出结果 = Command::new(解析cargo路径())
                .arg("check")
                .arg("--message-format=json")
                .current_dir(&项目根)
                .output()
                .map_err(|错| {
                    anyhow::anyhow!(
                        "{}",
                        界面.取文带参(
                            "cargo_check_failed",
                            &[&项目根.display().to_string(), &错.to_string()]
                        )
                    )
                })?;
            // cargo 整体是否成功（决定最终退出码）
            let 退出码 = if 输出结果.status.success() {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::FAILURE
            };

            // 结构化诊断翻译（与 run 编译失败路径共用同一管线）
            let 错误文本 = String::from_utf8_lossy(&输出结果.stderr).to_string();
            let 引擎输出 = format!(
                "{}\n{}",
                String::from_utf8_lossy(&输出结果.stdout),
                错误文本
            );
            let _ = 翻译cargo诊断(
                &引擎输出,
                &错误文本,
                &诊断上下文 {
                    界面: &界面,
                    语言包: &lang_pack,
                    项目根: &项目根,
                    管理器: &管理器,
                    源码: &源码,
                    入口文件: &file,
                    列映射: Some(&列映射),
                    入口行映射: Some(&入口行映射),
                    项目: Some(&项目上下文),
                },
                输出结果.status.success(),
                false, // check 场景：无诊断且成功时提示"编译成功"
                false, // check：诊断输出由本函数负责（非流式透传）
            );
            Ok(退出码)
        }
        CliCommand::Eject { file, lang_pack } => {
            let 界面 = 界面按文件(&file, &lang_pack);
            let 源码 = fs::read_to_string(&file)?;
            let 管理器 = 加载映射(lang_pack, Some(&file))?;
            let 英文代码 = 转译为英文(&源码, &管理器);
            let 输出路径 = file.with_extension("rs");
            fs::write(&输出路径, 英文代码)?;
            println!(
                "{}",
                界面.取文带参("exported_to", &[&输出路径.display().to_string()])
            );
            Ok(std::process::ExitCode::SUCCESS)
        }
        CliCommand::Transpile { file, lang_pack } => {
            // 转译预览：仅输出到 stdout，不产生任何文件（与 eject 互补）
            let 界面 = 界面按文件(&file, &lang_pack);
            let 源码 = fs::read_to_string(&file)?;
            let 管理器 = 加载映射(lang_pack, Some(&file))?;
            let 英文代码 = 转译为英文(&源码, &管理器);
            print!("{英文代码}");
            let _ = 界面; // stdout 模式无需额外提示
            Ok(std::process::ExitCode::SUCCESS)
        }
        CliCommand::Install { subcommand } => {
            // 省略子命令时默认安装全部组件（当前仅语言服务器）
            match subcommand.unwrap_or(InstallCommand::Lsp { force: false }) {
                InstallCommand::Lsp { force } => install::安装语言服务器(&界面, force)?,
                InstallCommand::Toolchain {
                    version,
                    ra_tag,
                    ra_only,
                    force,
                } => install::安装工具链(&界面, &version, &ra_tag, force, ra_only)?,
            }
            Ok(std::process::ExitCode::SUCCESS)
        }
        CliCommand::Doctor => install::诊断环境().map(|()| std::process::ExitCode::SUCCESS),
        CliCommand::Cheat { lang, markdown } => {
            let 语言代码 = lang.clone().unwrap_or_else(mapping_gen::检测系统语言);
            // 速查表标题固定英文：全球用户的第一张卡不依赖界面语言
            i18n_rust_engine::语言::设定语言(&语言代码);
            let 虚拟源码 = PathBuf::from(format!("main.{语言代码}"));
            // 借虚拟扩展名让 加载映射 走既有解析链（项目内 → 全局 → 内置语言包）
            let 管理器 = 加载映射(None, Some(&虚拟源码))?;
            打印速查表(&管理器, &语言代码, markdown);
            Ok(std::process::ExitCode::SUCCESS)
        }
        CliCommand::Lang { subcommand } => {
            处理语言子命令(subcommand).map(|()| std::process::ExitCode::SUCCESS)
        }
        CliCommand::Add { crates } => 处理添加子命令(&crates),
        CliCommand::Mapping { subcommand } => match subcommand {
            MappingCommand::Auto {
                crate_name,
                lang,
                provider,
                target_version,
                output,
                install,
            } => {
                let 语言代码 = lang.unwrap_or_else(mapping_gen::检测系统语言);
                i18n_rust_engine::语言::设定语言(&语言代码);
                let 输出路径 = output.unwrap_or_else(|| {
                    // 默认写入项目语言包根：从 cwd 向上找 Cargo.toml，
                    // 保证任意子目录下执行都落到项目本地语言包（加载映射 同一位置查找）；
                    // 主仓库内落到 crates/engine/lang-packs/（单一数据源），用户项目落 lang-packs/
                    let 基础目录 = std::env::current_dir()
                        .ok()
                        .and_then(|工作目录| 向上定位项目根(&工作目录))
                        .unwrap_or_else(|| PathBuf::from("."));
                    语言包根目录(&基础目录).join(format!("{}/crates/{}.toml", 语言代码, crate_name))
                });
                mapping_gen::运行自动生成(
                    &crate_name,
                    &语言代码,
                    &provider,
                    &输出路径,
                    target_version.as_deref(),
                )
                .map(|()| std::process::ExitCode::SUCCESS)
                .inspect(|_| {
                    // --install：生成成功后把 crate 加入当前项目依赖（用户项目内执行时）
                    if install {
                        安装依赖到当前项目(&crate_name, target_version.as_deref());
                    }
                    // 生成后自动对所在语言包跑一次冲突检测（仅提示，不改变退出码：
                    // 语言包可能存在历史遗留问题，生成成功与否以写入结果为准）
                    if let Some(语言目录) = 输出路径.parent().and_then(|父| 父.parent())
                        && 语言目录.join("keywords.toml").exists()
                        && let Some(目录串) = 语言目录.to_str()
                    {
                        let _ = mapping_check::运行校验(Some(目录串));
                    }
                })
            }
            MappingCommand::Check { target } => {
                // check 输出的语言默认跟随系统语言
                let 语言代码 = mapping_gen::检测系统语言();
                i18n_rust_engine::语言::设定语言(&语言代码);
                match mapping_check::运行校验(target.as_deref()) {
                    Ok(true) => Ok(std::process::ExitCode::SUCCESS),
                    Ok(false) => Ok(std::process::ExitCode::FAILURE),
                    Err(错) => Err(错),
                }
            }
            MappingCommand::Coverage { lang } => {
                // coverage 输出的语言默认跟随系统语言
                let 界面语言 = mapping_gen::检测系统语言();
                i18n_rust_engine::语言::设定语言(&界面语言);
                match mapping_coverage::运行覆盖(lang.as_deref()) {
                    Ok(true) => Ok(std::process::ExitCode::SUCCESS),
                    Ok(false) => Ok(std::process::ExitCode::FAILURE),
                    Err(错) => Err(错),
                }
            }
            MappingCommand::Scaffold {
                source,
                target,
                output,
                provider,
            } => {
                let 语言代码 = mapping_gen::检测系统语言();
                i18n_rust_engine::语言::设定语言(&语言代码);
                mapping_check::运行脚手架(&source, &target, output.as_deref(), &provider)
                    .map(|()| std::process::ExitCode::SUCCESS)
            }
        },
        CliCommand::Crate { subcommand } => match subcommand {
            CrateCommand::Search { keyword } => crate_registry::检索映射(keyword.as_deref())
                .map(|()| std::process::ExitCode::SUCCESS),
            CrateCommand::Install {
                crate_name,
                lang,
                force,
            } => crate_registry::安装映射(&crate_name, &lang, force)
                .map(|()| std::process::ExitCode::SUCCESS),
            CrateCommand::List => {
                crate_registry::列出映射().map(|()| std::process::ExitCode::SUCCESS)
            }
            CrateCommand::Remove { crate_name, lang } => {
                crate_registry::移除映射(&crate_name, &lang)
                    .map(|()| std::process::ExitCode::SUCCESS)
            }
            CrateCommand::Update => {
                crate_registry::刷新映射().map(|()| std::process::ExitCode::SUCCESS)
            }
            CrateCommand::Publish {
                crate_name,
                lang,
                file,
                author,
            } => crate_registry::发布映射(&crate_name, &lang, file, author.as_deref())
                .map(|()| std::process::ExitCode::SUCCESS),
        },
    }
}

/// 首次运行引导：欢迎 + rustc 缺失提示 + 下一步建议
///
/// 仅交互终端（stdout 是终端）且标记文件不存在时显示；
/// 显示后写入标记文件，保证每个用户只看到一次。
fn 显示首次运行引导(界面: &ui::界面) {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        return;
    }
    // 与 lang_manager::全局语言目录 相同的跨平台主目录解析
    let 主目录 = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok();
    let 标记文件 = 主目录.map(|首| PathBuf::from(首).join(".rz").join("first-run"));
    if let Some(标记文件) = &标记文件
        && 标记文件.exists()
    {
        return;
    }
    println!();
    println!("{}", 界面.取文("first_run_hello"));
    // rustc 检测：内置工具链目录或 PATH（含 rustc.exe/rustc）任一命中
    let rustc就绪 = i18n_rust_engine::工具链::查找工具链程序("rustc").is_some()
        || std::env::var_os("PATH").is_some_and(|路径| {
            std::env::split_paths(&路径).any(|目录| {
                let 可执行 = if cfg!(windows) { "rustc.exe" } else { "rustc" };
                目录.join(可执行).exists()
            })
        });
    if !rustc就绪 {
        println!("{}", 界面.取文("first_run_rustc_missing"));
    }
    println!("{}", 界面.取文("first_run_next_steps"));
    println!();
    if let Some(标记文件) = 标记文件 {
        if let Some(父目录) = 标记文件.parent() {
            let _ = std::fs::create_dir_all(父目录);
        }
        let _ = std::fs::write(标记文件, "");
    }
}

/// 处理 `rzc lang` 子命令
fn 处理语言子命令(子命令: LangCommand) -> anyhow::Result<()> {
    let 界面 = ui::界面::全局();
    match 子命令 {
        LangCommand::List => {
            let 列表 = lang_manager::列语言();
            if 列表.is_empty() {
                println!("{}", 界面.取文("no_lang_installed"));
                return Ok(());
            }
            println!(
                "{}",
                界面.取文带参("installed_langs_count", &[&列表.len().to_string()])
            );
            for 信息 in &列表 {
                let 标签 = match 信息.语言来源 {
                    语言来源::内置 => 界面.取文("tag_builtin"),
                    语言来源::用户安装 => 界面.取文("tag_user"),
                };
                let 扩展名 = 信息
                    .文件扩展名
                    .as_deref()
                    .map(|后| format!(".{}", 后))
                    .unwrap_or_else(|| 界面.取文("unknown"));
                let 版本 = match 信息.版本号.as_deref() {
                    Some(v) => v.to_string(),
                    None => 界面.取文("unknown"),
                };
                let 可删除 = if 信息.语言来源 == 语言来源::内置 {
                    界面.取文("not_removable")
                } else {
                    String::new()
                };
                let 显示名 = 信息
                    .显示名称
                    .as_deref()
                    .map(|名| format!("{} ({})", 名, 信息.语言代码))
                    .unwrap_or_else(|| 信息.语言代码.clone());
                println!(
                    "{}",
                    界面.取文带参(
                        "lang_list_display",
                        &[&标签, &显示名, &扩展名, &版本, &可删除]
                    )
                );
            }
            println!(
                "{}",
                界面.取文带参(
                    "global_lang_dir",
                    &[&lang_manager::全局语言目录().display().to_string()]
                )
            );
            Ok(())
        }
        LangCommand::Install { source, force } => lang_manager::安装语言(&source, force),
        LangCommand::Search { keyword } => {
            // 市场浏览：下载仓库 ZIP 扫描语言包（含显示名/版本），
            // 可选关键词过滤；与 install 共用同一远程源回退策略
            println!(
                "{}",
                界面.取文带参("lang_search_header", &[&界面.取文("lang_search_source")])
            );
            let 找到 = lang_manager::搜索远程语言(keyword.as_deref())?;
            if 找到.is_empty() {
                match keyword.as_deref() {
                    Some(关键词) if !关键词.trim().is_empty() => {
                        println!("{}", 界面.取文带参("lang_search_empty", &[关键词]));
                    }
                    _ => println!("{}", 界面.取文("lang_search_empty_all")),
                }
                return Ok(());
            }
            for 信息 in &找到 {
                let 名 = 信息.显示名称.as_deref().unwrap_or("—");
                let 版本 = 信息.版本号.as_deref().unwrap_or("—");
                println!("  {:<12} {:<16} {}", 信息.语言代码, 名, 版本);
            }
            println!();
            println!("{}", 界面.取文("lang_search_hint"));
            Ok(())
        }
        LangCommand::Remove { lang_code } => lang_manager::删除语言(&lang_code),
    }
}

/// 处理 `rzc add` 子命令：封装 cargo add，成功后提示母语映射可用性
fn 处理添加子命令(crates: &[String]) -> anyhow::Result<std::process::ExitCode> {
    let 界面 = ui::界面::全局();
    let 当前目录 = std::env::current_dir()?;
    let 项目根 = 向上定位项目根(&当前目录)
        .ok_or_else(|| anyhow::anyhow!("{}", 界面.取文("add_no_project")))?;
    let 状态 = Command::new(解析cargo路径())
        .arg("add")
        .args(crates)
        .current_dir(&项目根)
        .status()
        .map_err(|错| {
            anyhow::anyhow!("{}", 界面.取文带参("add_cargo_failed", &[&错.to_string()]))
        })?;
    if !状态.success() {
        // cargo add 自身已输出错误详情，直接传播退出码
        return Ok(std::process::ExitCode::FAILURE);
    }
    let 语言代码 = ui::检测界面语言();
    for 规格 in crates {
        // 依赖名取 @版本 前段，并将 - 归一为 _（代码中 use 路径用下划线）
        let crate名 = 规格.split('@').next().unwrap_or(规格).replace('-', "_");
        match 查找crate映射别名(&语言代码, &项目根, &crate名) {
            Some(别名) => println!("{}", 界面.取文带参("add_mapping_ready", &[&crate名, &别名])),
            None => println!(
                "{}",
                界面.取文带参("add_mapping_missing", &[&crate名, &crate名])
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
fn 查找crate映射别名(语言代码: &str, 项目根: &Path, crate名: &str) -> Option<String> {
    // 1. 项目内语言包与全局用户语言包的 crates/ 目录
    let 目录列表 = [
        语言包根目录(项目根).join(语言代码).join("crates"),
        lang_manager::全局语言目录().join(语言代码).join("crates"),
    ];
    for 目录 in &目录列表 {
        let Ok(条目列表) = fs::read_dir(目录) else {
            continue;
        };
        for 条目 in 条目列表.flatten() {
            let 路径 = 条目.path();
            if 路径.extension().and_then(|后| 后.to_str()) != Some("toml") {
                continue;
            }
            if let Ok(内容) = fs::read_to_string(&路径)
                && let Some(别名) = 解析映射别名(&内容, crate名)
            {
                return Some(别名);
            }
        }
    }
    // 2. 内置语言包（未知语言代码自动回退中文）
    let 内置 = builtin_lang::获取内置数据(语言代码);
    for (_, 内容) in 内置.三方库数据 {
        if let Some(别名) = 解析映射别名(内容, crate名) {
            return Some(别名);
        }
    }
    None
}

/// 在单个映射 TOML 内容中查找 crate 对应的母语别名
fn 解析映射别名(内容: &str, crate名: &str) -> Option<String> {
    let 值: toml::Value = toml::from_str(内容).ok()?;
    let 路径表 = 值.get("模块路径")?.as_table()?;
    for (键, 项) in 路径表 {
        let Some(英文路径) = 项.as_str() else {
            continue;
        };
        let 首段 = 英文路径.split("::").next().unwrap_or(英文路径);
        if 首段.replace('-', "_") == crate名 {
            return Some(键.clone());
        }
    }
    None
}

/// mapping auto --install：把 crate 加入当前项目依赖（找不到项目时仅告警）
///
/// 指定 --target-version 时按同一版本需求添加（`crate@=x.y.z` 精确锁定 /
/// `crate@x.y.*` 前缀），保证应用依赖与映射生成基准一致。
fn 安装依赖到当前项目(crate名: &str, 目标版本: Option<&str>) {
    let 界面 = ui::界面::全局();
    let Some(根) = std::env::current_dir()
        .ok()
        .and_then(|工作目录| 向上定位项目根(&工作目录))
    else {
        println!("{}", 界面.取文("mapping_auto_install_no_project"));
        return;
    };
    let 规格 = match 目标版本.and_then(mapping_gen::版本需求) {
        Some(需求) => format!("{}@{}", crate名, 需求),
        None => crate名.to_string(),
    };
    match Command::new(解析cargo路径())
        .arg("add")
        .arg(&规格)
        .current_dir(&根)
        .status()
    {
        Ok(状态) if 状态.success() => {
            println!("{}", 界面.取文带参("mapping_auto_installed", &[crate名]))
        }
        Ok(状态) => println!(
            "{}",
            界面.取文带参("mapping_auto_install_failed", &[crate名, &状态.to_string()])
        ),
        Err(错) => println!(
            "{}",
            界面.取文带参("mapping_auto_install_failed", &[crate名, &错.to_string()])
        ),
    }
}

/// 根据源码文件定位项目根（包含 Cargo.toml 的目录）
fn 定位项目根(file: &Path) -> anyhow::Result<PathBuf> {
    let 文件目录 = if file.is_absolute() {
        file.parent()
            .map(|父| 父.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        std::env::current_dir()?
            .join(file)
            .parent()
            .map(|父| 父.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    };

    if let Some(根) = 向上定位项目根(&文件目录) {
        return Ok(根);
    }
    // 未找到 Cargo.toml 时明确报错：静默回退当前目录会在无关目录写入 src/main.rs
    anyhow::bail!(
        "{}",
        ui::界面::全局().取文带参("no_project_root", &[&file.display().to_string()])
    )
}

/// 计算 `run`/`check` 的入口产物路径。
///
/// 仅当源文件确为**项目 src 直属**入口（父目录恰为 `project_root/src`，且词干为
/// [`是否入口词干`]，即 `main` 或语言包主函数词，如 `src/main.zh`、旧项目里的
/// `src/主函数.zh`）时才写入 Cargo 固定编译目标 `src/main.rs`；其余文件产物跟随
/// 自身扩展名（`file.with_extension("rs")`），绝不占用 `src/main.rs`：
/// - 项目根的 `build.zh` → `build.rs`，不把 build 脚本覆盖为入口（weix 工具异常 #4）；
/// - 子目录/示例里恰好名为 `main` 的文件（`src/sub/main.zh`、`examples/main.zh`）→
///   产物留在自身目录，不越界覆盖宿主 `src/main.rs`（越界写出坑）。
fn 入口产物路径(项目根: &Path, file: &Path, 管理器: &映射管理器) -> PathBuf {
    let 词干 = file.file_stem().and_then(|干| 干.to_str());
    if 词干.is_some_and(|干| 是否入口词干(干, 管理器)) && 是否直属src(file, 项目根)
    {
        项目根.join("src/main.rs")
    } else {
        file.with_extension("rs")
    }
}

/// 文件父目录（规范化为绝对路径，纯比较不依赖文件必已存在）是否恰为
/// `project_root/src`——即该文件是否为项目真正的 src 直属入口，用于防止子目录
/// 里恰好名为 `main` 的文件越界聚合、覆盖宿主入口产物。
fn 是否直属src(file: &Path, 项目根: &Path) -> bool {
    let Some(父目录) = file.parent() else {
        return false;
    };
    // 相对路径按当前目录补齐为绝对，再与已规范化的 项目根 对齐比较；
    // 路径可能尚未落盘（单元测试、新建文件），canonicalize 失败时退回词法路径。
    let 绝对父目录 = if 父目录.is_absolute() {
        父目录.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|当前| 当前.join(父目录))
            .unwrap_or_else(|_| 父目录.to_path_buf())
    };
    let 规范父目录 = 绝对父目录
        .canonicalize()
        .unwrap_or_else(|_| 绝对父目录.clone());
    let src目录 = 项目根.join("src");
    let 规范src = src目录.canonicalize().unwrap_or_else(|_| src目录.clone());
    规范父目录 == 规范src
}

/// 词干是否为项目入口主函数名：字面 `main`，或语言包中映射到 `main` 的
/// 母语词（zh「主函数」、ja「主関数」、ru「главная」等）。
///
/// `init` 生成 `src/main.<lang>`，教程与示例（.zh-demo）同样使用 `src/main.zh`；
/// 早期项目还可能有母语词干入口（如 zh 的 `src/主函数.zh`）——都须聚合到
/// Cargo 固定入口 `src/main.rs`，否则 cargo 报「no targets specified」。
fn 是否入口词干(词干: &str, 管理器: &映射管理器) -> bool {
    词干 == "main"
        || 管理器
            .取关键词映射表()
            .iter()
            .any(|(母语词, 英文)| 英文 == "main" && 母语词 == 词干)
}

/// 从指定目录向上查找项目根（含 Cargo.toml 的目录），未找到返回 None
fn 向上定位项目根(起点: &Path) -> Option<PathBuf> {
    let mut 当前 = 起点.canonicalize().unwrap_or_else(|_| 起点.to_path_buf());
    loop {
        if 当前.join("Cargo.toml").exists() {
            return Some(当前);
        }
        当前 = 当前.parent()?.to_path_buf();
    }
}

/// 项目内语言包根目录：主仓库 zrRust 为单副本结构 `crates/engine/lang-packs/`
///（编译期内嵌与文件系统消费共用同一份数据）；
/// 普通用户项目仍沿用 `lang-packs/` 约定（自定义覆盖）
pub(crate) fn 语言包根目录(基础: &Path) -> PathBuf {
    let 引擎包 = 基础.join("crates/engine/lang-packs");
    if 引擎包.is_dir() {
        引擎包
    } else {
        基础.join("lang-packs")
    }
}

/// 统一转译管线（复用 engine）：Unicode 检查 → 关键字/宏转译 → 模块路径替换 → 别名替换 → 非 ASCII 模块注解
///
/// 使用磁盘持久化增量缓存（~/.rz/cache/transpile-v1.json）：上次运行转译过
/// 且内容未变的文件直接命中，省去整条转译管线；语言包变化时语境指纹失效。
fn 转译为英文(源码: &str, 管理器: &映射管理器) -> String {
    let mut 缓存 = i18n_rust_engine::缓存::转译缓存::持久默认项();
    转译为英文带缓存(源码, 管理器, &mut 缓存)
}

/// 同 [`转译为英文`]，复用调用方提供的缓存实例
///
/// 多文件场景（run/check 命令）共享同一会话缓存：入口文件与项目内其他
/// 方言文件内容指纹一致时直接命中，避免每次调用重建缓存、反复读写磁盘。
fn 转译为英文带缓存(
    源码: &str,
    管理器: &映射管理器,
    缓存: &mut i18n_rust_engine::缓存::转译缓存,
) -> String {
    标注非西文模块(&转译映射带缓存(源码, 管理器, 缓存).产出)
}

/// 同 [`转译为英文带缓存`]，但保留转译产物的源映射
///
/// 源映射（`管线映射`）是诊断列号回译的唯一依据：rustc 报的是英文产物
/// 的列号，须据此回放替换过程才能还原母语源码列号。
/// 缓存命中时映射表随产物一并复用，无需重算。
fn 转译映射带缓存(
    源码: &str,
    管理器: &映射管理器,
    缓存: &mut i18n_rust_engine::缓存::转译缓存,
) -> i18n_rust_engine::缓存::转译产出 {
    i18n_rust_engine::源码转译并映射(源码, 管理器, 缓存).unwrap_or_else(|_错| {
        // 缓存失败不阻断转译：回退无缓存管线（与旧行为一致）
        i18n_rust_engine::警告日志!(
            "cli",
            "{}",
            i18n_rust_engine::语言::查句("log_transpile_cache_fallback")
        );
        i18n_rust_engine::转译管线(源码, 管理器)
    })
}

/// 同 [`转译映射带缓存`]，附项目级声明上下文（跨文件声明豁免）
///
/// 缓存语境指纹由引擎并入项目上下文指纹（[`源码转译并项目`]）：
/// 项目声明集合变化时相关缓存自动失效。
fn 项目转译映射带缓存(
    源码: &str,
    管理器: &映射管理器,
    缓存: &mut i18n_rust_engine::缓存::转译缓存,
    项目: Option<&i18n_rust_engine::别名替换::项目上下文>,
) -> i18n_rust_engine::缓存::转译产出 {
    i18n_rust_engine::源码转译并项目(源码, 管理器, 缓存, 项目).unwrap_or_else(|_错| {
        // 缓存失败不阻断转译：回退无缓存管线（与旧行为一致）
        i18n_rust_engine::警告日志!(
            "cli",
            "{}",
            i18n_rust_engine::语言::查句("log_transpile_cache_fallback")
        );
        i18n_rust_engine::转译管线并项目(源码, 管理器, 项目)
    })
}

/// 解析 cargo 可执行文件：内置工具链（~/.rz/toolchain）优先，PATH 回退；
/// 找不到时返回 "cargo" 由系统报错（保持与旧行为一致的报错信息）
pub fn 解析cargo路径() -> PathBuf {
    i18n_rust_engine::工具链::查找工具链程序("cargo").unwrap_or_else(|| PathBuf::from("cargo"))
}

/// 解析 rustc 可执行文件：内置工具链优先，PATH 回退
pub fn 解析rustc路径() -> PathBuf {
    i18n_rust_engine::工具链::查找工具链程序("rustc").unwrap_or_else(|| PathBuf::from("rustc"))
}

/// 解析与 [`解析rustc路径`] 同工具链的 rustdoc：优先内建/PATH 的独立 rustdoc，
/// 缺失时改用 rustc 的 sysroot 精确定位其配套 rustdoc（rustdoc 与 rustc 同
/// sysroot，版本一致），否则回退 PATH 的 "rustdoc" 由系统报错。
///
/// `mapping auto` 手调 rustdoc 生成 JSON 时必须与 cargo 编译 rlib 用同一
/// rustc，否则跨版本链接触发 E0514（见 doc_json.rs 的 RUSTC 统一注入）。
pub fn 解析rustdoc路径() -> PathBuf {
    if let Some(路径) = i18n_rust_engine::工具链::查找工具链程序("rustdoc") {
        return 路径;
    }
    if let Ok(输出结果) = std::process::Command::new(解析rustc路径())
        .arg("--print")
        .arg("sysroot")
        .output()
    {
        let sysroot = String::from_utf8_lossy(&输出结果.stdout).trim().to_string();
        if !sysroot.is_empty() {
            let 二进制 = PathBuf::from(sysroot)
                .join("bin")
                .join(format!("rustdoc{}", std::env::consts::EXE_SUFFIX));
            if 二进制.is_file() {
                return 二进制;
            }
        }
    }
    PathBuf::from("rustdoc")
}

/// 递归收集目录下全部方言源文件（路径排序保证确定性）
///
/// 支持多层模块布局（`src/领域/工具.zh`）：模块文件可放在以父模块
/// 命名的子目录中，与 rustc 的 `父模块.rs` + `父模块/` 目录规则一致。
fn 收集方言文件(目录: &Path, 扩展名列表: &[String]) -> Vec<PathBuf> {
    let mut 输出 = Vec::new();
    递归收集方言文件(目录, 扩展名列表, &mut 输出);
    输出.sort();
    输出
}

fn 递归收集方言文件(目录: &Path, 扩展名列表: &[String], 输出: &mut Vec<PathBuf>) {
    let Ok(条目列表) = fs::read_dir(目录) else {
        return;
    };
    for 目录项 in 条目列表.flatten() {
        let 路径 = 目录项.path();
        let Ok(文件类型) = 目录项.file_type() else {
            continue;
        };
        if 文件类型.is_dir() {
            // target 等构建产物目录不会出现在 src 树内；无特殊排除
            递归收集方言文件(&路径, 扩展名列表, 输出);
        } else if 文件类型.is_file()
            && 路径
                .extension()
                .and_then(|后| 后.to_str())
                .is_some_and(|后缀| {
                    扩展名列表
                        .iter()
                        .any(|扩展| 扩展 == 后缀 || 扩展 == &format!(".{后缀}"))
                })
        {
            输出.push(路径);
        }
    }
}

/// 收集项目级声明上下文（跨文件声明豁免）
///
/// 递归扫描 src/ 下全部层级的方言文件与入口文件（可能在项目根），收集：
/// - 模块名：各方言文件词干（含嵌套层级，供 `crate::领域::工具::成员`
///   等多层路径链豁免）；
/// - 声明名：各文件的项名与结构体字段（裸使用处豁免）。
///
/// 与 [`转译项目文件`] 扫描范围保持一致（src/ 全层级递归），
/// 入口文件不在 src/ 时单独补扫；读取失败的文件跳过（转译阶段会报错）。
fn 收集项目上下文(
    项目根: &Path,
    入口文件: &Path,
    管理器: &映射管理器,
) -> i18n_rust_engine::别名替换::项目上下文 {
    use std::collections::HashSet;
    let 扩展名列表 = lang_manager::全部可用扩展名();
    let 入口规范 = 入口文件.canonicalize().ok();
    let mut 模块集 = HashSet::new();
    let mut 源文件列表: Vec<String> = Vec::new();
    let mut 入口已见 = false;

    let src目录 = 项目根.join("src");
    for 路径 in 收集方言文件(&src目录, &扩展名列表) {
        if let Some(词干) = 路径.file_stem().and_then(|干| 干.to_str()) {
            模块集.insert(词干.to_string());
        }
        if 入口规范.is_some() && 路径.canonicalize().ok() == 入口规范 {
            入口已见 = true;
        }
        if let Ok(源码) = fs::read_to_string(&路径) {
            源文件列表.push(源码);
        }
    }
    // 入口文件不在 src/（如项目根的自定义路径）时单独补扫
    if !入口已见 && 入口文件.is_file() {
        if let Some(词干) = 入口文件.file_stem().and_then(|干| 干.to_str()) {
            模块集.insert(词干.to_string());
        }
        if let Ok(源码) = fs::read_to_string(入口文件) {
            源文件列表.push(源码);
        }
    }

    i18n_rust_engine::别名替换::项目上下文::自源文件新建(
        模块集,
        源文件列表.iter().map(String::as_str),
        管理器,
    )
}

/// 同步转译项目 src/ 下的全部方言源文件（入口文件除外）为对应 .rs 文件，
/// 使多文件项目的 mod 引用链可用；非已注册方言扩展名的文件（如手写 .rs）跳过。
/// 并行转译项目内其他方言文件，保证多文件项目的 mod 引用链可用
///
/// 转译在 `thread::scope` 中并行执行（教学项目文件相互独立，无共享可变
/// 状态）；共享缓存用 `Mutex` 保护——查询/插入为短临界区，转译本身在锁外
/// 并行，文件多时与串行相比显著提速（缓存命中时仅查表，开销可忽略）。
fn 转译项目文件(
    项目根: &Path,
    入口文件: &Path,
    管理器: &映射管理器,
    缓存: &std::sync::Mutex<转译缓存>,
    项目: &i18n_rust_engine::别名替换::项目上下文,
) -> anyhow::Result<()> {
    let 界面 = ui::界面::全局();
    let src目录 = 项目根.join("src");
    // 无 src 目录时不处理，由 cargo 自行报错
    if !src目录.is_dir() {
        return Ok(());
    }
    let 扩展名列表 = lang_manager::全部可用扩展名();
    // 入口产物固定写入 src/main.rs：src/ 顶层任何入口词干的方言文件
    // （main.zh、旧项目的 主函数.zh 等）转译后都会覆盖入口产物，必须跳过；
    // 嵌套层级中的同名文件不是入口（rustc 模块树按路径区分），照常转译。
    let 入口绝对 = 入口文件.canonicalize().ok();
    let mut 文件列表 = Vec::new();
    for 路径 in 收集方言文件(&src目录, &扩展名列表) {
        if Some(&路径) == 入口绝对.as_ref() {
            continue;
        }
        let 处于src顶层 = 路径.parent() == Some(src目录.as_path());
        if 处于src顶层
            && 路径
                .file_stem()
                .and_then(|干| 干.to_str())
                .is_some_and(|干| 是否入口词干(干, 管理器))
        {
            continue;
        }
        let Some(扩展名) = 路径.extension().and_then(|后| 后.to_str()) else {
            continue;
        };
        if 按扩展名取语言代码(扩展名).is_none() {
            continue;
        }
        文件列表.push(路径);
    }

    // 语境指纹只算一次（全部文件共享同一语言包与项目上下文）；
    // 项目上下文指纹并入后，跨文件声明变化时旧缓存自动失效。
    // 必须与引擎 `源码转译并项目` 使用同一组合函数：
    // 入口文件与项目内其它文件共用同一个 转译缓存 实例，
    // 两处指纹算法若不同，同一语境会算出两个键，缓存互相不可见。
    let 指纹 = i18n_rust_engine::缓存::转译缓存::合并语境指纹(
        管理器.语境指纹(),
        Some(项目.计算指纹()),
    );
    let 首错: std::sync::Mutex<Option<anyhow::Error>> = std::sync::Mutex::new(None);
    std::thread::scope(|作用域| {
        let mut 句柄列表 = Vec::with_capacity(文件列表.len());
        for 路径 in 文件列表 {
            // 界面/首错 遮蔽为引用：move 闭包捕获的是 Copy 的共享引用
            let 界面 = &界面;
            let 首错 = &首错;
            // 相对 src 的显示名在闭包外算好（避免 PathBuf 被 move 进线程）
            let 显示名称 = 路径
                .strip_prefix(&src目录)
                .unwrap_or(&路径)
                .display()
                .to_string();
            let 句柄 = 作用域.spawn(move || {
                if 首错
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .is_some()
                {
                    return; // 已有失败文件：跳过剩余工作
                }
                let 源码 = match fs::read_to_string(&路径) {
                    Ok(源) => 源,
                    Err(错) => {
                        *首错
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) =
                            Some(anyhow::anyhow!(
                                "{}",
                                界面.取文带参(
                                    "transpile_file_failed",
                                    &[&路径.display().to_string(), &错.to_string()]
                                )
                            ));
                        return;
                    }
                };
                // 短临界区：查询缓存（命中直接复用产物）
                let 命中 = 缓存
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .检索(&源码, 指纹)
                    .cloned();
                let 产物 = match 命中 {
                    Some(产物) => 产物,
                    None => {
                        // 锁外并行转译，完成后短临界区写回缓存；
                        // 静默管线：教学告警改由下方带文件名输出（多文件项目
                        // 中裸行列无法定位到具体文件）；项目上下文保证
                        // 跨文件调用的成员名与声明侧一致（#8）
                        let 产物 = i18n_rust_engine::转译管线静默并项目(
                            &源码,
                            管理器,
                            Some(项目),
                        );
                        输出教学告警(
                            &源码,
                            &显示名称,
                            管理器.取教学检查词(),
                            &管理器.歧义构造词集(),
                        );
                        缓存
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .写入缓存(&源码, 指纹, 产物.clone());
                        产物
                    }
                };
                // 非入口模块按多层布局补 `#[path]`：src/领域.rs 中的
                // `mod 工具;` 需指向 src/领域/工具.rs（#[path] 相对当前
                // 文件目录解析，前缀为当前文件词干细胞目录）。
                let 成品 = if let Some(词干) = 路径.file_stem().and_then(|干| 干.to_str()) {
                    i18n_rust_engine::模块路径::标注嵌套模块并行号(&产物.产出, 词干).0
                } else {
                    产物.产出.clone()
                };
                if let Err(错) = 写入转译产物(&路径.with_extension("rs"), &成品, 界面)
                {
                    *首错
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(错);
                }
            });
            句柄列表.push(句柄);
        }
        for 句柄 in 句柄列表 {
            let _ = 句柄.join();
        }
    });
    match 首错.into_inner().unwrap_or(None) {
        Some(错) => Err(错),
        None => Ok(()),
    }
}

/// 输出单个方言文件的教学告警（Unicode 混淆/全角标点/lint），带文件名归属
///
/// 多文件项目中项目内文件分散在多个方言文件，裸行列无法定位具体文件；
/// 入口文件的告警仍由转译管线直接输出（裸行列即命令传入的入口文件）。
/// 时机与转译一致（仅缓存未命中时输出），静默管线保证不重复输出。
fn 输出教学告警(
    源码: &str,
    显示名称: &str,
    检查词表: &HashSet<String>,
    歧义构造集: &HashSet<String>,
) {
    for 警告 in i18n_rust_engine::混淆字符::检查混淆字符(源码) {
        i18n_rust_engine::警告日志!("unicode_confusion", "{}：{}", 显示名称, 警告.格式化输出());
    }
    for 警告 in i18n_rust_engine::全角标点::查找全角标点(源码) {
        i18n_rust_engine::警告日志!("fullwidth", "{}：{}", 显示名称, 警告.格式化输出());
    }
    if i18n_rust_engine::教学检查::教学检查开关() {
        for 警告 in
            i18n_rust_engine::教学检查::执行教学检查并词表(源码, 检查词表, 歧义构造集)
        {
            i18n_rust_engine::警告日志!("lint", "{}：{}", 显示名称, 警告.格式化提示());
        }
    }
}

/// 写转译产物：目标已存在且内容不同时先备份为 `.rs.bak`，绝不静默覆盖用户文件。
///
/// 幂等重跑（内容一致）不产生备份；无同名手写文件时行为与直接写入完全一致。
fn 写入转译产物(
    路径: &Path, 内容: &str, 界面: &crate::ui::界面
) -> anyhow::Result<()> {
    // 方言文件直接放在项目根等场景下 src/ 可能尚不存在，先创建父目录
    //（否则 fs::write 报裸 ENOENT，LSP 侧 translation_cache 已有同款处理）
    if let Some(父目录) = 路径.parent() {
        std::fs::create_dir_all(父目录)?;
    }
    if let Ok(已存在) = fs::read_to_string(路径)
        && 已存在 != 内容
    {
        let 备份 = 路径.with_extension("rs.bak");
        fs::rename(路径, &备份)?;
        println!(
            "{}",
            界面.取文带参(
                "transpile_backup",
                &[&路径.display().to_string(), &备份.display().to_string()]
            )
        );
    }
    fs::write(路径, 内容)?;
    Ok(())
}

/// 探测本机当前生效工具链的通道号（如 `1.98` / `nightly`）
///
/// 解析 `rustc --version` 输出的第二段（形如 `1.98.0 (哈希 日期)` 或 `1.98.0-nightly`），
/// 取主次版本号作为 channel；nightly/beta 通道原样返回。
/// rustc 不在 PATH 或输出格式异常时返回 None（调用方跳过生成锁定文件）。
fn 探测工具链通道() -> Option<String> {
    let 输出结果 = std::process::Command::new(解析rustc路径())
        .arg("--version")
        .output()
        .ok()?;
    if !输出结果.status.success() {
        return None;
    }
    let 标准输出 = String::from_utf8_lossy(&输出结果.stdout);
    let 版本 = 标准输出.split_whitespace().nth(1)?;
    if 版本.contains("nightly") {
        return Some("nightly".to_string());
    }
    if 版本.contains("beta") {
        return Some("beta".to_string());
    }
    // "1.98.0" → "1.98"（channel 只保留主次版本，补丁版本由工具链自行解析）
    let mut 段 = 版本.split('.');
    let 主版本 = 段.next()?;
    let 次版本 = 段.next()?;
    if 主版本.chars().all(|char| char.is_ascii_digit())
        && 次版本.chars().all(|char| char.is_ascii_digit())
    {
        Some(format!("{主版本}.{次版本}"))
    } else {
        None
    }
}

/// 输出母语 ↔ Rust 映射速查表（`rzc cheat`）
///
/// 四张表（关键字/模块路径/别名/派生特征）逐节输出，母语词按显示宽度对齐；
/// 母语词与英文原词相同的恒等条目不输出（如 en 语言包整表恒等，仅提示）。
/// `markdown` 模式输出可嵌入教程/README 的表格。
fn 打印速查表(管理器: &映射管理器, 语言代码: &str, markdown: bool) {
    let 派生表 = 管理器.取派生映射表();
    let 章节表: [(&str, &HashMap<String, String>); 4] = [
        ("Keywords / 关键字", 管理器.取关键词映射表()),
        ("Module paths / 模块路径", 管理器.取模块路径映射表()),
        ("Aliases / 别名", 管理器.取别名映射表()),
        ("Derives / 派生特征", 派生表),
    ];

    // 过滤恒等条目（native == english）后统计剩余量：全恒等则无需速查
    let 章节表: Vec<(&str, Vec<(String, String)>)> = 章节表
        .into_iter()
        .map(|(标题, 映射内容)| {
            let 行列表: Vec<(String, String)> = 映射内容
                .iter()
                .filter(|(母语, 英文)| 母语 != 英文)
                .map(|(母语, 英文)| (母语.clone(), 英文.clone()))
                .collect();
            (标题, 行列表)
        })
        .collect();

    let 总数: usize = 章节表.iter().map(|(_, 行列表)| 行列表.len()).sum();
    if 总数 == 0 {
        println!("rzc cheat — {语言代码} ↔ Rust");
        println!();
        println!("This language pack maps every identifier to itself (identity mapping).");
        println!("No cheat sheet is needed — write Rust as usual.");
        return;
    }

    if markdown {
        println!("# rzc cheat — {语言代码} ↔ Rust ({总数})");
    } else {
        println!("rzc cheat — {语言代码} ↔ Rust（共 {总数} 条）");
    }
    println!();
    for (标题, 行列表) in &章节表 {
        if 行列表.is_empty() {
            continue;
        }
        if markdown {
            println!("## {标题}");
            println!();
            println!("| {语言代码} | Rust |");
            println!("|---|---|");
            for (母语, 英文) in 行列表 {
                println!("| {母语} | `{英文}` |");
            }
            println!();
        } else {
            let 宽度 = 行列表
                .iter()
                .map(|(母语, _)| 母语.chars().count())
                .max()
                .unwrap_or(0);
            println!("── {标题} ──");
            for (母语, 英文) in 行列表 {
                let 补齐 = 宽度 - 母语.chars().count();
                println!("  {母语}{}  {英文}", " ".repeat(补齐));
            }
            println!();
        }
    }
    if markdown {
        println!("> Generated by `rzc cheat {语言代码} --markdown`.");
    }
}

fn 创建项目(项目名: &str, 语言: &str) -> anyhow::Result<()> {
    let 界面 = ui::界面::按语言加载(语言);
    i18n_rust_engine::语言::设定语言(语言);
    let 项目路径 = PathBuf::from(项目名);
    if 项目路径.exists() {
        anyhow::bail!("{}", 界面.取文带参("dir_exists", &[项目名]));
    }
    // 包名取路径最后一段（支持传入绝对/相对路径），并将 cargo 不允许的字符替换为下划线
    let 包名 = 项目路径
        .file_name()
        .and_then(|名| 名.to_str())
        .unwrap_or(项目名);
    let 包名: String = 包名
        .chars()
        .map(|char| {
            if char.is_ascii_alphanumeric() || char == '-' || char == '_' {
                char
            } else {
                '_'
            }
        })
        .collect();
    fs::create_dir_all(项目路径.join("src"))?;
    // 版本锁定：固定到本机当前工具链版本（动态探测，避免硬编码随时间过时，
    // 导致 rust-analyzer 等工具报"工具链过于陈旧"）；探测失败时不生成锁定文件，
    // 项目跟随系统默认工具链。components 含 rust-analyzer/rust-src 供 IDE 使用。
    if let Some(通道号) = 探测工具链通道() {
        fs::write(
            项目路径.join("rust-toolchain.toml"),
            format!(
                "[toolchain]\nchannel = \"{通道号}\"\ncomponents = [\"rustc\", \"cargo\", \"rust-analyzer\", \"rust-src\"]\n"
            ),
        )?;
    }

    fs::write(
        项目路径.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n\n[workspace]\n",
            包名
        ),
    )?;
    // 语言包已内置到 rzc 可执行文件中，无需复制；主文件模板随 --lang 变化
    fs::write(
        项目路径.join(format!("src/main.{}", 语言)),
        界面.取文("template_main"),
    )?;
    fs::write(
        项目路径.join("README.md"),
        界面.取文带参("readme_template", &[项目名, 语言]),
    )?;
    println!("{}", 界面.取文带参("project_created", &[项目名]));
    println!("{}", 界面.取文带参("project_created_hint", &[语言]));
    println!("{}", 界面.取文带参("project_run_hint", &[语言]));
    Ok(())
}

fn 加载映射(
    语言包路径: Option<PathBuf>,
    源文件: Option<&Path>,
) -> anyhow::Result<映射管理器> {
    // 映射数据来源日志（语言包开发调试：RZ_LOG=info rzc check 可见）
    // 注意：加载映射 先于转译管线执行，logger 需在此提前初始化（幂等）
    i18n_rust_engine::日志::r#初始化();
    let 界面 = 界面按文件(源文件.unwrap_or(Path::new("")), &语言包路径);
    // 1. 如果用户通过 --lang-pack 指定了外部目录，强制使用
    if let Some(路径) = 语言包路径 {
        i18n_rust_engine::信息日志!(
            "映射加载",
            "{}",
            界面.取文带参("mapping_source_explicit", &[&路径.display().to_string()])
        );
        return 映射管理器::自目录加载(&路径).map_err(|错| {
            anyhow::anyhow!(
                "{}",
                界面.取文带参("load_lang_pack_failed", &[&错.to_string()])
            )
        });
    }
    // 2. 根据源文件扩展名确定语言代码
    let 扩展名 = 源文件
        .and_then(|file| file.extension())
        .and_then(|后| 后.to_str())
        .unwrap_or("");
    let 语言代码 = 按扩展名取语言代码(扩展名).ok_or_else(|| {
        let 可用 = lang_manager::全部可用扩展名();
        let 可用文本 = if 可用.is_empty() {
            界面.取文("no_available_ext")
        } else {
            可用
                .iter()
                .map(|后| format!(".{}", 后))
                .collect::<Vec<_>>()
                .join(", ")
        };
        anyhow::anyhow!(
            "{}",
            界面.取文带参("unknown_extension", &[扩展名, &可用文本])
        )
    })?;
    // 3. 项目内语言包目录存在时优先使用（自定义覆盖）：
    //    主仓库为 crates/engine/lang-packs/<lang>（单一数据源），用户项目为 lang-packs/<lang>；
    //    项目根从源文件向上查找；源文件不在项目内时回退 cwd，兼容旧用法
    let mut 本地候选: Vec<PathBuf> = Vec::new();
    if let Some(file) = 源文件
        && let Some(父目录) = file.parent()
        && let Some(根) = 向上定位项目根(父目录)
    {
        本地候选.push(语言包根目录(&根).join(&语言代码));
    }
    let 当前目录 = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    本地候选.push(语言包根目录(&当前目录).join(&语言代码));
    for 本地路径 in &本地候选 {
        if 本地路径.exists() {
            i18n_rust_engine::信息日志!(
                "映射加载",
                "{}",
                界面.取文带参("mapping_source_project", &[&本地路径.display().to_string()])
            );
            return 映射管理器::自目录加载(本地路径).map_err(|错| {
                anyhow::anyhow!(
                    "{}",
                    界面.取文带参("load_local_lang_pack_failed", &[&错.to_string()])
                )
            });
        }
    }
    // 4. 全局用户语言包目录
    let 全局路径 = lang_manager::全局语言目录().join(&语言代码);
    if 全局路径.exists() {
        i18n_rust_engine::信息日志!(
            "映射加载",
            "{}",
            界面.取文带参("mapping_source_global", &[&全局路径.display().to_string()])
        );
        return 映射管理器::自目录加载(&全局路径).map_err(|错| {
            anyhow::anyhow!(
                "{}",
                界面.取文带参("load_global_lang_pack_failed", &[&错.to_string()])
            )
        });
    }
    // 5. 回退到内置语言包（未内置的语言提示用户安装，避免静默使用中文）
    if !builtin_lang::拥有内置语言(&语言代码) {
        return Err(anyhow::anyhow!(
            "{}",
            界面.取文带参("lang_not_builtin", &[&语言代码, &语言代码])
        ));
    }
    let 内置 = builtin_lang::获取内置数据(&语言代码);
    i18n_rust_engine::信息日志!(
        "映射加载",
        "{}",
        界面.取文带参("mapping_source_builtin", &[&语言代码])
    );
    映射管理器::自内置加载(
        内置.关键字文本,
        内置.模块路径文本,
        内置.标准库文本,
        内置.三方库数据,
    )
    .map_err(|错| {
        anyhow::anyhow!(
            "{}",
            界面.取文带参("load_builtin_lang_pack_failed", &[&错.to_string()])
        )
    })
}

/// 根据源文件与 --lang-pack 参数选择界面消息语言
///
/// --lang-pack 显式目录优先；否则按源文件扩展名确定语言代码；
/// 无法识别时回退 RZ_LANG / 系统语言 / 中文。
fn 界面按文件(file: &Path, 语言包: &Option<PathBuf>) -> ui::界面 {
    if let Some(路径) = 语言包 {
        let 界面 = ui::界面::按显式目录加载(路径);
        // 同步引擎全局语言（目录名即语言代码）
        let 代码 = 路径
            .file_name()
            .and_then(|名| 名.to_str())
            .unwrap_or("zh")
            .to_string();
        i18n_rust_engine::语言::设定语言(&代码);
        return 界面;
    }
    let 语言代码 = file
        .extension()
        .and_then(|后| 后.to_str())
        .and_then(按扩展名取语言代码)
        .unwrap_or_else(ui::检测界面语言);
    i18n_rust_engine::语言::设定语言(&语言代码);
    ui::界面::按语言加载(&语言代码)
}

/// 按当前界面语言本地化 clap 帮助文本
///
/// 利用 `CommandFactory` 生成命令后逐项覆盖 about / help，
/// 使 `rzc --help` 与各子命令帮助均使用目标语言。
fn 本地化clap(界面: &ui::界面) -> clap::Command {
    use clap::CommandFactory;
    CliArgs::command()
        .about(界面.取文("cli_about"))
        .mut_arg("no_lint", |参数| 参数.help(界面.取文("arg_no_lint_help")))
        .mut_subcommand("init", |命令| {
            命令
                .about(界面.取文("cmd_init_about"))
                .mut_arg("lang", |参数| 参数.help(界面.取文("arg_lang_help")))
        })
        .mut_subcommand("run", |命令| 命令.about(界面.取文("cmd_run_about")))
        .mut_subcommand("check", |命令| {
            命令
                .about(界面.取文("cmd_check_about"))
                .mut_arg("fix", |参数| 参数.help(界面.取文("arg_check_fix_help")))
        })
        .mut_subcommand("eject", |命令| 命令.about(界面.取文("cmd_eject_about")))
        .mut_subcommand("transpile", |命令| {
            命令.about(界面.取文("cmd_transpile_about"))
        })
        .mut_subcommand("add", |命令| {
            命令
                .about(界面.取文("cmd_add_about"))
                .mut_arg("crates", |参数| 参数.help(界面.取文("arg_add_crates_help")))
        })
        .mut_subcommand("install", |命令| {
            命令
                .about(界面.取文("cmd_install_about"))
                .mut_subcommand("lsp", |子| {
                    子.about(界面.取文("cmd_install_lsp_about"))
                        .mut_arg("force", |参数| 参数.help(界面.取文("arg_force_help")))
                })
                .mut_subcommand("toolchain", |子| {
                    子.about(界面.取文("tc_install_help"))
                        .mut_arg("version", |参数| {
                            参数.help(界面.取文("arg_tc_version_help"))
                        })
                        .mut_arg("ra_tag", |参数| 参数.help(界面.取文("arg_tc_ra_tag_help")))
                        .mut_arg("ra_only", |参数| {
                            参数.help(界面.取文("arg_tc_ra_only_help"))
                        })
                        .mut_arg("force", |参数| 参数.help(界面.取文("arg_force_help")))
                })
        })
        .mut_subcommand("doctor", |命令| 命令.about(界面.取文("tc_doctor_help")))
        .mut_subcommand("cheat", |命令| {
            命令
                .about(界面.取文("cmd_cheat_about"))
                .mut_arg("lang", |参数| 参数.help(界面.取文("arg_cheat_lang_help")))
                .mut_arg("markdown", |参数| 参数.help(界面.取文("arg_markdown_help")))
        })
        .mut_subcommand("lang", |命令| {
            命令
                .about(界面.取文("cmd_lang_about"))
                .mut_subcommand("list", |子| 子.about(界面.取文("cmd_lang_list_about")))
                .mut_subcommand("install", |子| {
                    子.about(界面.取文("cmd_lang_install_about"))
                        .mut_arg("source", |参数| {
                            参数.help(界面.取文("arg_lang_source_help"))
                        })
                        .mut_arg("force", |参数| 参数.help(界面.取文("arg_force_help")))
                })
                .mut_subcommand("search", |子| {
                    子.about(界面.取文("cmd_lang_search_about"))
                        .mut_arg("keyword", |参数| {
                            参数.help(界面.取文("arg_lang_search_keyword_help"))
                        })
                })
                .mut_subcommand("remove", |子| {
                    子.about(界面.取文("cmd_lang_remove_about"))
                        .mut_arg("lang_code", |参数| {
                            参数.help(界面.取文("arg_lang_code_help"))
                        })
                })
        })
        .mut_subcommand("mapping", |命令| {
            命令
                .about(界面.取文("cmd_mapping_about"))
                .mut_subcommand("auto", |子| {
                    子.about(界面.取文("cmd_mapping_auto_about"))
                        .mut_arg("crate_name", |参数| {
                            参数.help(界面.取文("arg_mapping_auto_crate_help"))
                        })
                        .mut_arg("lang", |参数| 参数.help(界面.取文("arg_lang_help")))
                        .mut_arg("provider", |参数| 参数.help(界面.取文("arg_provider_help")))
                        .mut_arg("target_version", |参数| {
                            参数.help(界面.取文("arg_target_version_help"))
                        })
                        .mut_arg("output", |参数| 参数.help(界面.取文("arg_output_help")))
                        .mut_arg("install", |参数| 参数.help(界面.取文("arg_install_help")))
                })
                .mut_subcommand("check", |子| {
                    子.about(界面.取文("cmd_mapping_check_about"))
                        .mut_arg("target", |参数| {
                            参数.help(界面.取文("cmd_mapping_check_target_help"))
                        })
                })
                .mut_subcommand("scaffold", |子| {
                    子.about(界面.取文("cmd_mapping_scaffold_about"))
                        .mut_arg("source", |参数| {
                            参数.help(界面.取文("cmd_mapping_scaffold_source_help"))
                        })
                        .mut_arg("target", |参数| {
                            参数.help(界面.取文("cmd_mapping_scaffold_target_help"))
                        })
                        .mut_arg("output", |参数| 参数.help(界面.取文("arg_output_help")))
                        .mut_arg("provider", |参数| {
                            参数.help(界面.取文("cmd_mapping_scaffold_provider_help"))
                        })
                })
                .mut_subcommand("coverage", |子| {
                    子.about(界面.取文("cmd_mapping_coverage_about"))
                        .mut_arg("lang", |参数| {
                            参数.help(界面.取文("arg_coverage_lang_help"))
                        })
                })
        })
        .mut_subcommand("crate", |命令| {
            命令
                .about(界面.取文("cmd_crate_about"))
                .mut_subcommand("search", |子| {
                    子.about(界面.取文("cmd_crate_search_about"))
                        .mut_arg("keyword", |参数| {
                            参数.help(界面.取文("arg_crate_keyword_help"))
                        })
                })
                .mut_subcommand("install", |子| {
                    子.about(界面.取文("cmd_crate_install_about"))
                        .mut_arg("crate_name", |参数| {
                            参数.help(界面.取文("arg_crate_name_normalize_help"))
                        })
                        .mut_arg("lang", |参数| 参数.help(界面.取文("arg_lang_help")))
                        .mut_arg("force", |参数| 参数.help(界面.取文("arg_force_help")))
                })
                .mut_subcommand("list", |子| 子.about(界面.取文("cmd_crate_list_about")))
                .mut_subcommand("remove", |子| {
                    子.about(界面.取文("cmd_crate_remove_about"))
                        .mut_arg("crate_name", |参数| {
                            参数.help(界面.取文("arg_crate_name_help"))
                        })
                        .mut_arg("lang", |参数| 参数.help(界面.取文("arg_lang_help")))
                })
                .mut_subcommand("update", |子| 子.about(界面.取文("cmd_crate_update_about")))
                .mut_subcommand("publish", |子| {
                    子.about(界面.取文("cmd_crate_publish_about"))
                        .mut_arg("crate_name", |参数| {
                            参数.help(界面.取文("arg_crate_name_help"))
                        })
                        .mut_arg("lang", |参数| 参数.help(界面.取文("arg_lang_help")))
                        .mut_arg("file", |参数| 参数.help(界面.取文("arg_crate_file_help")))
                        .mut_arg("author", |参数| {
                            参数.help(界面.取文("arg_crate_author_help"))
                        })
                })
        })
}

/// 根据源码文件扩展名获取语言代码
///
/// 优先查询动态映射表，未命中时回退静态映射。
fn 按扩展名取语言代码(扩展名: &str) -> Option<String> {
    if let Some(代码) = lang_manager::查询扩展名映射(扩展名) {
        return Some(代码);
    }
    lang_manager::静态扩展名映射().get(扩展名).cloned()
}

#[cfg(test)]
mod 单元测试 {
    use super::{
        入口产物路径, 写入转译产物, 按扩展名取语言代码, 探测工具链通道, 收集项目上下文,
        标注非西文模块, 标注非西文模块并行号, 解析映射别名, 转译为英文, 转译项目文件,
        项目转译映射带缓存,
    };

    /// 加载内置中文映射管理器（测试转译管线用）
    fn 中文管理器() -> i18n_rust_engine::映射管理::映射管理器 {
        let 内置 = crate::builtin_lang::获取内置数据("zh");
        i18n_rust_engine::映射管理::映射管理器::自内置加载(
            内置.关键字文本,
            内置.模块路径文本,
            内置.标准库文本,
            内置.三方库数据,
        )
        .expect("内置中文语言包应可加载")
    }

    /// 入口产物路径：入口词干（main / 语言包主函数词）写入 src/main.rs，
    /// 非入口文件跟随自身扩展名，不占用 src/main.rs
    /// （weix 工具异常 #4：build.zh 不得覆盖入口产物）
    #[test]
    fn 测试_入口产物路径() {
        use std::path::{Path, PathBuf};
        let 根 = Path::new("/proj");
        let 管理 = 中文管理器();
        // src/main.zh（词干 main）→ 聚合到 Cargo 固定入口
        assert_eq!(
            入口产物路径(根, Path::new("/proj/src/main.zh"), &管理),
            PathBuf::from("/proj/src/main.rs")
        );
        // 母语词干入口「主函数」（映射 main，旧项目命名）→ 同样聚合到 src/main.rs
        // （回归：此前仅认字面 main，主函数.zh 产物写自身名致 cargo 找不到目标）
        assert_eq!(
            入口产物路径(根, Path::new("/proj/src/主函数.zh"), &管理),
            PathBuf::from("/proj/src/main.rs")
        );
        // 项目根的 build.zh（词干非入口）→ 同目录 build.rs，绝不触碰 src/main.rs
        assert_eq!(
            入口产物路径(根, Path::new("/proj/build.zh"), &管理),
            PathBuf::from("/proj/build.rs")
        );
        // src 下非入口模块（如 helper.zh）→ src/helper.rs
        assert_eq!(
            入口产物路径(根, Path::new("/proj/src/helper.zh"), &管理),
            PathBuf::from("/proj/src/helper.rs")
        );
    }

    /// 越界写出回归：入口词干但**不在 project_root/src 直属**的文件（子目录/示例
    /// 里恰好名为 main）不得聚合覆盖宿主 src/main.rs，产物须留在自身目录。
    #[test]
    fn 测试_入口产物路径不越界覆盖src外() {
        use std::path::{Path, PathBuf};
        let 根 = Path::new("/proj");
        let 管理 = 中文管理器();
        // src/sub/main.zh（词干 main 但在子目录）→ src/sub/main.rs，不覆盖 src/main.rs
        assert_eq!(
            入口产物路径(根, Path::new("/proj/src/sub/main.zh"), &管理),
            PathBuf::from("/proj/src/sub/main.rs")
        );
        // examples/main.zh（示例入口）→ examples/main.rs，不占用宿主 src/main.rs
        assert_eq!(
            入口产物路径(根, Path::new("/proj/examples/main.zh"), &管理),
            PathBuf::from("/proj/examples/main.rs")
        );
        // 母语主函数词但位于子目录（src/sub/主函数.zh）同样不越界
        assert_eq!(
            入口产物路径(根, Path::new("/proj/src/sub/主函数.zh"), &管理),
            PathBuf::from("/proj/src/sub/主函数.rs")
        );
    }

    /// 正向聚合：真实磁盘项目的 src 直属入口（英文词干与母语词干）都聚合到
    /// src/main.rs；相对词法路径（新建未落盘项目）按词法比较同样判定
    #[test]
    fn 测试_入口产物路径聚合src直属入口() {
        let 管理 = 中文管理器();
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path().join("proj");
        std::fs::create_dir_all(根.join("src/sub")).unwrap();

        // 已落盘：真实目录的 canonicalize 比较
        assert_eq!(
            入口产物路径(&根, &根.join("src/main.zh"), &管理),
            根.join("src/main.rs")
        );
        assert_eq!(
            入口产物路径(&根, &根.join("src/主函数.zh"), &管理),
            根.join("src/main.rs")
        );
        // 同目录的非入口模块不聚合
        assert_eq!(
            入口产物路径(&根, &根.join("src/工具.zh"), &管理),
            根.join("src/工具.rs")
        );

        // 未落盘：root/src 尚不存在，canonicalize 双双失败退回词法路径，
        // 词干为入口时仍应聚合（编辑器里新建项目骨架的场景）
        let 幽灵 = 临时.path().join("ghost-proj");
        assert_eq!(
            入口产物路径(&幽灵, &幽灵.join("src/main.zh"), &管理),
            幽灵.join("src/main.rs")
        );
        assert_eq!(
            入口产物路径(&幽灵, &幽灵.join("src/sub/main.zh"), &管理),
            幽灵.join("src/sub/main.rs")
        );
    }

    /// 工具链通道探测：开发/CI 环境必有 rustc；主次版本号形如 `1.98`，通道词原样
    #[test]
    fn 测试_探测工具链通道() {
        let 通道 = 探测工具链通道().expect("测试环境应有 rustc");
        let 是版本 = 通道.split_once('.').is_some_and(|(前, 后)| {
            !前.is_empty()
                && !后.is_empty()
                && 前
                    .chars()
                    .chain(后.chars())
                    .all(|char| char.is_ascii_digit())
        });
        assert!(
            是版本 || matches!(通道.as_str(), "nightly" | "beta"),
            "意外的通道格式：{通道}"
        );
    }

    /// 已内置的语言包扩展名可解析出语言代码
    ///
    /// 持环境变量锁：扩展名映射会扫描全局语言包目录（受 RZ_LANG_DIR 影响），
    /// lang_manager 的环境变量测试并发修改该变量时会污染本测试
    #[test]
    fn 测试_语言代码从扩展名_内置() {
        let _锁 = crate::lang_manager::单元测试::环境锁();
        assert_eq!(按扩展名取语言代码("zh").as_deref(), Some("zh"));
        assert_eq!(按扩展名取语言代码("de").as_deref(), Some("de"));
        assert_eq!(按扩展名取语言代码("ru").as_deref(), Some("ru"));
        assert_eq!(按扩展名取语言代码("ja").as_deref(), Some("ja"));
        assert_eq!(按扩展名取语言代码("hi").as_deref(), Some("hi"));
    }

    /// 未知扩展名返回 None（同样受 RZ_LANG_DIR 影响，需持锁）
    #[test]
    fn 测试_语言代码从扩展名_未知() {
        let _锁 = crate::lang_manager::单元测试::环境锁();
        assert_eq!(按扩展名取语言代码("xyz"), None);
    }

    /// 统一转译管线：中文关键字转为标准 Rust
    #[test]
    fn 测试_转译英文_中文关键字() {
        let 管理 = 中文管理器();
        let 产物 = 转译为英文("公开 函数 help() {}\n", &管理);
        assert!(产物.contains("pub fn help()"), "实际输出：{产物}");
    }

    /// 多文件转译：src/ 下的其他方言文件生成同名 .rs，入口与手写 .rs 不受影响；
    /// 跨文件声明豁免（#8）：其他文件声明的与映射词同名的成员不被替换
    #[test]
    fn 测试_转译项目文件() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path();
        std::fs::create_dir_all(根.join("src")).unwrap();
        let 入口 = 根.join("src/main.zh");
        // 跨文件调用本项目的 `函数 新建()`（撞 `新建`=new 映射）：
        // 项目上下文让调用位与声明侧一致（不被替换出 `new`）
        std::fs::write(
            &入口,
            "模组 辅助;\n\n函数 main() {\n    让 x = 辅助::r#新建();\n}\n",
        )
        .unwrap();
        std::fs::write(根.join("src/辅助.zh"), "公开 函数 新建() {}\n").unwrap();
        std::fs::write(根.join("src/manual.rs"), "// 手写文件不覆盖\n").unwrap();

        let 管理 = 中文管理器();
        let 缓存 = std::sync::Mutex::new(i18n_rust_engine::缓存::转译缓存::新建缓存(8));
        // 项目级声明上下文：模块名（文件词干）+ 声明名（入口+src/ 全扫）
        let 上下文 = 收集项目上下文(根, &入口, &管理);
        assert!(上下文.模块名集.contains("辅助"), "模块名应含文件名词干");
        assert!(上下文.声明名.contains("新建"), "声明名应含其他文件的函数名");
        转译项目文件(根, &入口, &管理, &缓存, &上下文).unwrap();

        // 其他方言文件已转译为同名 .rs
        let 辅助rs = std::fs::read_to_string(根.join("src/辅助.rs")).unwrap();
        assert!(辅助rs.contains("pub fn 新建()"), "实际输出：{辅助rs}");
        // 手写 .rs 不被触碰
        let 手写rs = std::fs::read_to_string(根.join("src/manual.rs")).unwrap();
        assert!(手写rs.contains("手写文件不覆盖"));
        // 入口文件未被重复转译（无 main.rs 产生，由调用方单独写入）
        assert!(!根.join("src/main.rs").exists());

        // 入口转译（带项目上下文）：跨文件调用位 `新建` 不被替换（#8）
        let 入口产物 = 项目转译映射带缓存(
            &std::fs::read_to_string(&入口).unwrap(),
            &管理,
            &mut 缓存.lock().unwrap(),
            Some(&上下文),
        )
        .产出;
        assert!(
            入口产物.contains("辅助::r#新建()"),
            "跨文件调用位应与声明侧一致：{入口产物}"
        );
    }

    /// write_transpiled 备份语义（let-chain 守卫）：目标存在且内容不同才备份为 .rs.bak；
    /// 内容相同（幂等重跑）不产生备份，绝不静默覆盖用户文件
    #[test]
    fn 测试_写入转译产物仅变更时备份() {
        let 目录 = tempfile::tempdir().unwrap();
        let 路径 = 目录.path().join("main.rs");
        let 界面 = crate::ui::界面::按语言加载("zh");

        // 首次写入：文件不存在，直接写盘，无备份
        写入转译产物(&路径, "v1", &界面).unwrap();
        assert_eq!(std::fs::read_to_string(&路径).unwrap(), "v1");
        assert!(!路径.with_extension("rs.bak").exists());

        // 内容相同：幂等重跑，不产生备份，文件保持 v1
        写入转译产物(&路径, "v1", &界面).unwrap();
        assert!(!路径.with_extension("rs.bak").exists());

        // 内容不同：旧内容备份为 .rs.bak，文件更新为新内容
        写入转译产物(&路径, "v2", &界面).unwrap();
        assert_eq!(std::fs::read_to_string(&路径).unwrap(), "v2");
        assert_eq!(
            std::fs::read_to_string(路径.with_extension("rs.bak")).unwrap(),
            "v1"
        );
    }

    /// 非 ASCII 文件式 mod 声明补 #[path] 注解（绕过 rustc E0754）
    #[test]
    fn 测试_标注非西文模块_基础() {
        let 产物 = 标注非西文模块("mod 数学;");
        assert_eq!(产物, "#[path = \"数学.rs\"]\nmod 数学;");
    }

    /// 带 pub 可见性时注解插在 pub 之前
    #[test]
    fn 测试_标注非西文模块_带pub() {
        let 产物 = 标注非西文模块("pub mod 数学;");
        assert_eq!(产物, "#[path = \"数学.rs\"]\npub mod 数学;");
    }

    /// ASCII 模块名与内联模块块不处理
    #[test]
    fn 测试_标注非西文模块_跳过ascii和内联() {
        assert_eq!(标注非西文模块("mod math;"), "mod math;");
        assert_eq!(标注非西文模块("mod 数学 { }"), "mod 数学 { }");
    }

    /// 已有 #[path] 注解时不重复添加；缩进保持
    #[test]
    fn 测试_标注非西文模块_已存在与缩进() {
        let 源 = "#[path = \"数学.rs\"]\nmod 数学;";
        assert_eq!(标注非西文模块(源), 源);
        let 产物 = 标注非西文模块("函数 main() {\n    mod 数学;\n}");
        assert_eq!(
            产物,
            "函数 main() {\n    #[path = \"数学.rs\"]\n    mod 数学;\n}"
        );
    }

    /// 注解行映射：磁盘行 → 引擎直出行（0-based），注解行归属其 mod 声明行，
    /// 多处注解时偏移逐处累积（对应多模块入口 main.zh 的诊断行号换算）
    #[test]
    fn 测试_标注非西文模块_行映射() {
        // 单处注解：磁盘 0/1 行（#[path] 与 mod）均归属引擎第 0 行
        let (产物, 行映射) = 标注非西文模块并行号("mod 数学;\nfn main() {}");
        assert_eq!(产物, "#[path = \"数学.rs\"]\nmod 数学;\nfn main() {}");
        assert_eq!(行映射, vec![0, 0, 1]);

        // 无注解时映射恒等
        let (_, 行映射) = 标注非西文模块并行号("fn main() {}\nfn f() {}");
        assert_eq!(行映射, vec![0, 1]);

        // 两处注解：后续行偏移为 2
        let 两处 = "mod 数学;\nmod 物理;\nfn main() {}";
        let (产物, 行映射) = 标注非西文模块并行号(两处);
        assert_eq!(
            产物,
            "#[path = \"数学.rs\"]\nmod 数学;\n#[path = \"物理.rs\"]\nmod 物理;\nfn main() {}"
        );
        assert_eq!(行映射, vec![0, 0, 1, 1, 2]);
    }

    /// 映射 TOML 中按 crate 首段查找母语别名；连字符 crate 名归一匹配
    #[test]
    fn 测试_解析映射别名() {
        let toml文本 =
            "[\"模块路径\"]\n\"HTTP客户端\" = \"reqwest\"\n\"时间\" = \"chrono::prelude\"\n";
        assert_eq!(
            解析映射别名(toml文本, "reqwest").as_deref(),
            Some("HTTP客户端")
        );
        assert_eq!(解析映射别名(toml文本, "chrono").as_deref(), Some("时间"));
        assert_eq!(解析映射别名(toml文本, "tokio"), None);
        assert_eq!(
            解析映射别名(
                "[\"模块路径\"]\n\"序列化\" = \"serde-json\"\n",
                "serde_json"
            )
            .as_deref(),
            Some("序列化")
        );
    }
}
