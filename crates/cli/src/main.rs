//! rzc 命令行入口 - 多语言 Rust 教学方言编译器
//!
//! 提供 init / run / check / eject / lang / mapping 等子命令，
//! 将母语 Rust 源码实时转译为标准 Rust 并调用 cargo 编译/运行。

use clap::{FromArgMatches, Parser, Subcommand};
use i18n_rust_engine::cache::TranslationCache;
use i18n_rust_engine::mapping_manager::MappingManager;
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
    DiagContext, can_use_direct_rustc, check_direct_rustc, run_direct_rustc,
    translate_cargo_diagnostics, translate_cargo_progress,
};
use lang_manager::Source;
// 非 ASCII 模块名 #[path] 注解：CLI 与 LSP 镜像共享同一引擎实现
use i18n_rust_engine::module_path::{annotate_non_ascii_mods, annotate_non_ascii_mods_with_lines};

#[derive(Parser)]
#[command(name = "rzc", version)]
// 兜底文案（localize_clap 会按界面语言覆盖）；用英文避免硬编码中文
#[command(about = "Multi-language Rust teaching dialect compiler")]
struct CliArgs {
    /// 不输出教学 lint 提示（初学者代码风格警告；Unicode 混淆/全角标点告警不受影响）
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
        #[arg(short, long, default_value_t = mapping_gen::detect_system_language())]
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
        /// 自动修复全角标点（写入源文件，仅转换有半角对应的字符）
        #[arg(long)]
        fix: bool,
    },
    Eject {
        file: PathBuf,
        #[arg(short, long)]
        lang_pack: Option<PathBuf>,
    },
    /// 转译预览：将方言源码转译为标准 Rust 并输出到 stdout（不写文件）
    Transpile {
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
    /// 第三方库共享注册中心（search / install / list / remove / update / publish）
    Crate {
        #[command(subcommand)]
        subcommand: CrateCommand,
    },
    /// 安装配套组件（语言服务器 i18n-rust-lsp 等）
    Install {
        #[command(subcommand)]
        subcommand: Option<InstallCommand>,
    },
    /// 诊断工具链环境：内置工具链 / PATH / 版本对比
    Doctor,
    /// 母语 ↔ Rust 映射速查表（关键字/模块路径/别名/派生特征）
    ///
    /// 全球用户的"第一张卡"：可用 --markdown 粘贴进教程/README。
    Cheat {
        /// 内置语言代码（如 zh）或语言包目录路径；省略时按系统语言检测
        lang: Option<String>,
        /// 以 Markdown 表格输出，便于嵌入文档
        #[arg(long)]
        markdown: bool,
    },
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
        /// 仅升级 rust-analyzer（跳过 rustc/cargo 的 300MB 重下）
        #[arg(long)]
        ra_only: bool,
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
        /// 锁定目标 crate 版本（如 2.11.5 精确锁定；2.11 / 2 为该线最新）；省略时用最新版（不可复现）
        #[arg(long)]
        target_version: Option<String>,
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
    /// 语料覆盖矩阵：用后端真实源码检验语言包关键字/API 覆盖度，自动列出缺失的母语映射
    Coverage {
        /// 内置语言代码（如 zh）；省略时检测全部内置语言
        #[arg(long)]
        lang: Option<String>,
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
enum CrateCommand {
    /// 检索注册中心已发布的第三方库映射（可选关键词过滤）
    Search {
        /// 关键词（匹配 crate 名 / 语言 / 作者）；省略时列出全部（按下载量排序）
        keyword: Option<String>,
    },
    /// 安装单个第三方库映射：从注册中心复制到全局语言包
    Install {
        /// crate 名（连字符归一为下划线）
        crate_name: String,
        /// 目标语言代码（如 zh）
        #[arg(long)]
        lang: String,
        /// 已存在时强制覆盖安装
        #[arg(short = 'f', long = "force")]
        force: bool,
    },
    /// 列出已安装的社区映射
    List,
    /// 删除已安装的社区映射（从全局语言包与清单移除）
    Remove {
        /// crate 名
        crate_name: String,
        /// 语言代码
        #[arg(long)]
        lang: String,
    },
    /// 按已安装清单重新拉取所有映射（获取他人更新）
    Update,
    /// 发布本地映射到注册中心（先经质量门禁）
    Publish {
        /// crate 名
        crate_name: String,
        /// 语言代码
        #[arg(long)]
        lang: String,
        /// 映射文件路径（缺省按常见位置查找：全局/项目语言包或当前目录）
        #[arg(long)]
        file: Option<PathBuf>,
        /// 译者署名（缺省取 git user.name）
        #[arg(long)]
        author: Option<String>,
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
    /// 浏览远程语言包市场（下载远程仓库扫描可用语言包，可选关键词过滤）
    Search {
        /// 关键词（匹配语言代码或显示名称）；省略时列出全部远程语言包
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
        install::show_setup_wizard();
        println!();
        println!("按任意键退出...");
        let mut buf = [0u8; 1];
        if std::io::stdin().read_exact(&mut buf).is_err() {
            std::thread::sleep(std::time::Duration::from_secs(8));
        }
        return std::process::ExitCode::SUCCESS;
    }
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

    // `--no-lint`：项目开发（非教学）场景静默教学 lint（初学者代码风格提示，
    // 每次转译刷屏）；Unicode 混淆/全角标点告警不受影响
    if args.no_lint {
        i18n_rust_engine::lint::set_teaching_lint_enabled(false);
    }

    // 首次运行引导：终端交互场景下，首次执行教学核心命令时打印
    // 欢迎语与环境检查（rustc 缺失提示 + 下一步建议），仅一次
    // （~/.rz/first-run 标记文件）；CI/管道等非终端场景自动跳过。
    match &args.command {
        CliCommand::Run { .. } | CliCommand::Check { .. } | CliCommand::Init { .. } => {
            maybe_show_first_run(&ui);
        }
        _ => {}
    }

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
            // 项目级声明上下文（跨文件声明豁免）：扫描 src/ 全部方言文件 +
            // 入口，收集模块名与声明名；入口文件、项目内文件与诊断重放
            // 共享同一上下文（否则跨文件调用的成员名被当库别名替换，E0599）
            let project_ctx = collect_project_context(&project_root, &file, &manager);
            // 入口文件写入 src/main.rs 作为编译目标；会话缓存贯穿入口文件与
            // 项目内其他文件（并行转译共享命中，见 transpile_project_files）
            let source_path = entry_output_path(&project_root, &file, &manager);
            let cache = std::sync::Mutex::new(
                i18n_rust_engine::cache::TranslationCache::persistent_default(),
            );
            let transpiled = transpile_with_map_cached_in_project(
                &source,
                &manager,
                &mut cache
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
                Some(&project_ctx),
            );
            // 列映射：把 rustc 诊断的英文产物列号回译到母语源码列号
            let column_map =
                i18n_rust_engine::column_map::ColumnMap::build(&source, &transpiled.pipeline_map);
            // 写盘产物含 `#[path]` 注解插入行：保留行映射供诊断回译先行换算
            let (annotated, entry_line_map) =
                annotate_non_ascii_mods_with_lines(&transpiled.output);
            write_transpiled(&source_path, &annotated, &ui)?;
            // 同步转译项目内其他方言文件，保证多文件项目的 mod 引用链可用
            transpile_project_files(&project_root, &file, &manager, &cache, &project_ctx)?;

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
                    &column_map,
                    &entry_line_map,
                    &project_ctx,
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
                    &DiagContext {
                        ui: &ui,
                        lang_pack: &lang_pack,
                        project_root: &project_root,
                        manager: &manager,
                        source: &source,
                        file: &file,
                        column_map: Some(&column_map),
                        entry_line_map: Some(&entry_line_map),
                        project: Some(&project_ctx),
                    },
                    status.success(),
                    true,
                    // 输出已实时透传：构建成功而程序运行失败（如 panic）时
                    // 不补打“编译错误”标签
                    true,
                );
            }

            // 传播被运行程序的退出码（信号终止等无码场景回退 1）
            Ok(status
                .code()
                .map(|c| std::process::ExitCode::from(c as u8))
                .unwrap_or(std::process::ExitCode::FAILURE))
        }
        CliCommand::Check {
            file,
            lang_pack,
            fix,
        } => {
            let ui = ui_for_file(&file, &lang_pack);
            let mut source = fs::read_to_string(&file)?;
            // 全角标点教学修复：中文输入法下最常见的编译错误来源之一。
            // --fix 时自动将代码位置（字符串/注释内除外）的全角标点改写为半角，
            // 并提示剩余需人工修改的字符（顿号/全角空格等）；
            // 不带 --fix 时警告由转译管线（log_warn）自动输出。
            if fix {
                let (fixed, count) = i18n_rust_engine::fullwidth::fix_fullwidth_punct(&source);
                if count > 0 {
                    fs::write(&file, &fixed)?;
                    source = fixed;
                    println!(
                        "{}",
                        ui.f(
                            "fullwidth_fix_done",
                            &[&count.to_string(), &file.display().to_string()]
                        )
                    );
                }
                let remaining = i18n_rust_engine::fullwidth::find_fullwidth_punct(&source).len();
                if remaining > 0 {
                    println!(
                        "{}",
                        ui.f("fullwidth_fix_remaining", &[&remaining.to_string()])
                    );
                }
            }
            let manager = load_mapping(lang_pack.clone(), Some(&file))?;
            let project_root = find_project_root(&file)?;
            // 项目级声明上下文（跨文件声明豁免，同 run）
            let project_ctx = collect_project_context(&project_root, &file, &manager);
            let source_path = entry_output_path(&project_root, &file, &manager);
            let cache = std::sync::Mutex::new(
                i18n_rust_engine::cache::TranslationCache::persistent_default(),
            );
            let transpiled = transpile_with_map_cached_in_project(
                &source,
                &manager,
                &mut cache
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
                Some(&project_ctx),
            );
            // 列映射：把 rustc 诊断的英文产物列号回译到母语源码列号
            let column_map =
                i18n_rust_engine::column_map::ColumnMap::build(&source, &transpiled.pipeline_map);
            // 写盘产物含 `#[path]` 注解插入行：保留行映射供诊断回译先行换算
            let (annotated, entry_line_map) =
                annotate_non_ascii_mods_with_lines(&transpiled.output);
            write_transpiled(&source_path, &annotated, &ui)?;
            // 同步转译项目内其他方言文件，保证多文件项目的 mod 引用链可用
            transpile_project_files(&project_root, &file, &manager, &cache, &project_ctx)?;

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
                    &column_map,
                    &entry_line_map,
                    &project_ctx,
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
                &DiagContext {
                    ui: &ui,
                    lang_pack: &lang_pack,
                    project_root: &project_root,
                    manager: &manager,
                    source: &source,
                    file: &file,
                    column_map: Some(&column_map),
                    entry_line_map: Some(&entry_line_map),
                    project: Some(&project_ctx),
                },
                output.status.success(),
                false, // check 场景：无诊断且成功时提示"编译成功"
                false, // check：诊断输出由本函数负责（非流式透传）
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
        CliCommand::Transpile { file, lang_pack } => {
            // 转译预览：仅输出到 stdout，不产生任何文件（与 eject 互补）
            let ui = ui_for_file(&file, &lang_pack);
            let source = fs::read_to_string(&file)?;
            let manager = load_mapping(lang_pack, Some(&file))?;
            let english_code = transpile_to_english(&source, &manager);
            print!("{english_code}");
            let _ = ui; // stdout 模式无需额外提示
            Ok(std::process::ExitCode::SUCCESS)
        }
        CliCommand::Install { subcommand } => {
            // 省略子命令时默认安装全部组件（当前仅语言服务器）
            match subcommand.unwrap_or(InstallCommand::Lsp { force: false }) {
                InstallCommand::Lsp { force } => install::install_lsp(&ui, force)?,
                InstallCommand::Toolchain {
                    version,
                    ra_tag,
                    ra_only,
                    force,
                } => install::install_toolchain(&ui, &version, &ra_tag, force, ra_only)?,
            }
            Ok(std::process::ExitCode::SUCCESS)
        }
        CliCommand::Doctor => install::doctor().map(|()| std::process::ExitCode::SUCCESS),
        CliCommand::Cheat { lang, markdown } => {
            let lang_code = lang
                .clone()
                .unwrap_or_else(mapping_gen::detect_system_language);
            // 速查表标题固定英文：全球用户的第一张卡不依赖界面语言
            i18n_rust_engine::语言::set_language(&lang_code);
            let fake_source = PathBuf::from(format!("main.{lang_code}"));
            // 借虚拟扩展名让 load_mapping 走既有解析链（项目内 → 全局 → 内置语言包）
            let manager = load_mapping(None, Some(&fake_source))?;
            print_cheat(&manager, &lang_code, markdown);
            Ok(std::process::ExitCode::SUCCESS)
        }
        CliCommand::Lang { subcommand } => {
            handle_lang_command(subcommand).map(|()| std::process::ExitCode::SUCCESS)
        }
        CliCommand::Add { crates } => handle_add_command(&crates),
        CliCommand::Mapping { subcommand } => match subcommand {
            MappingCommand::Auto {
                crate_name,
                lang,
                provider,
                target_version,
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
                mapping_gen::run_auto_generate(
                    &crate_name,
                    &lang,
                    &provider,
                    &output_path,
                    target_version.as_deref(),
                )
                .map(|()| std::process::ExitCode::SUCCESS)
                .inspect(|_| {
                    // --install：生成成功后把 crate 加入当前项目依赖（用户项目内执行时）
                    if install {
                        install_crate_to_current_project(&crate_name, target_version.as_deref());
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
            MappingCommand::Coverage { lang } => {
                // coverage 输出的语言默认跟随系统语言
                let ui_lang = mapping_gen::detect_system_language();
                i18n_rust_engine::语言::set_language(&ui_lang);
                match mapping_coverage::run_coverage(lang.as_deref()) {
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
        CliCommand::Crate { subcommand } => match subcommand {
            CrateCommand::Search { keyword } => {
                crate_registry::search(keyword.as_deref()).map(|()| std::process::ExitCode::SUCCESS)
            }
            CrateCommand::Install {
                crate_name,
                lang,
                force,
            } => crate_registry::install(&crate_name, &lang, force)
                .map(|()| std::process::ExitCode::SUCCESS),
            CrateCommand::List => crate_registry::list().map(|()| std::process::ExitCode::SUCCESS),
            CrateCommand::Remove { crate_name, lang } => {
                crate_registry::remove(&crate_name, &lang).map(|()| std::process::ExitCode::SUCCESS)
            }
            CrateCommand::Update => {
                crate_registry::update().map(|()| std::process::ExitCode::SUCCESS)
            }
            CrateCommand::Publish {
                crate_name,
                lang,
                file,
                author,
            } => crate_registry::publish(&crate_name, &lang, file, author.as_deref())
                .map(|()| std::process::ExitCode::SUCCESS),
        },
    }
}

/// 首次运行引导：欢迎 + rustc 缺失提示 + 下一步建议
///
/// 仅交互终端（stdout 是终端）且标记文件不存在时显示；
/// 显示后写入标记文件，保证每个用户只看到一次。
fn maybe_show_first_run(ui: &ui::Ui) {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        return;
    }
    // 与 lang_manager::global_lang_dir 相同的跨平台主目录解析
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok();
    let marker = home.map(|h| PathBuf::from(h).join(".rz").join("first-run"));
    if let Some(marker) = &marker
        && marker.exists()
    {
        return;
    }
    println!();
    println!("{}", ui.t("first_run_hello"));
    // rustc 检测：内置工具链目录或 PATH（含 rustc.exe/rustc）任一命中
    let rustc_ok = i18n_rust_engine::toolchain::find_toolchain_bin("rustc").is_some()
        || std::env::var_os("PATH").is_some_and(|path| {
            std::env::split_paths(&path).any(|dir| {
                let exe = if cfg!(windows) { "rustc.exe" } else { "rustc" };
                dir.join(exe).exists()
            })
        });
    if !rustc_ok {
        println!("{}", ui.t("first_run_rustc_missing"));
    }
    println!("{}", ui.t("first_run_next_steps"));
    println!();
    if let Some(marker) = marker {
        if let Some(parent) = marker.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(marker, "");
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
        LangCommand::Search { keyword } => {
            // 市场浏览：下载仓库 ZIP 扫描语言包（含显示名/版本），
            // 可选关键词过滤；与 install 共用同一远程源回退策略
            println!(
                "{}",
                ui.f("lang_search_header", &[&ui.t("lang_search_source")])
            );
            let found = lang_manager::search_remote_langs(keyword.as_deref())?;
            if found.is_empty() {
                match keyword.as_deref() {
                    Some(kw) if !kw.trim().is_empty() => {
                        println!("{}", ui.f("lang_search_empty", &[kw]));
                    }
                    _ => println!("{}", ui.t("lang_search_empty_all")),
                }
                return Ok(());
            }
            for info in &found {
                let name = info.display_name.as_deref().unwrap_or("—");
                let version = info.version.as_deref().unwrap_or("—");
                println!("  {:<12} {:<16} {}", info.lang_code, name, version);
            }
            println!();
            println!("{}", ui.t("lang_search_hint"));
            Ok(())
        }
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
///
/// 指定 --target-version 时按同一版本需求添加（`crate@=x.y.z` 精确锁定 /
/// `crate@x.y.*` 前缀），保证应用依赖与映射生成基准一致。
fn install_crate_to_current_project(crate_name: &str, target_version: Option<&str>) {
    let ui = ui::Ui::global();
    let Some(root) = std::env::current_dir()
        .ok()
        .and_then(|cwd| find_project_root_upward(&cwd))
    else {
        println!("{}", ui.t("mapping_auto_install_no_project"));
        return;
    };
    let spec = match target_version.and_then(mapping_gen::version_requirement) {
        Some(requirement) => format!("{}@{}", crate_name, requirement),
        None => crate_name.to_string(),
    };
    match Command::new(resolve_cargo())
        .arg("add")
        .arg(&spec)
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

/// 计算 `run`/`check` 的入口产物路径。
///
/// 仅当源文件确为入口（词干为 [`is_entry_stem`]，即 `main` 或语言包主函数
/// 词，如 `src/main.zh`、旧项目里的 `src/主函数.zh`）时才写入 Cargo 固定编译目标
/// `src/main.rs`；其余文件（如项目根的 `build.zh`）产物跟随自身扩展名
/// （`build.rs`），绝不占用 `src/main.rs`——否则会把 build 脚本静默覆盖为
/// 项目入口（weix 工具异常 #4）。
fn entry_output_path(project_root: &Path, file: &Path, manager: &MappingManager) -> PathBuf {
    let stem = file.file_stem().and_then(|s| s.to_str());
    if stem.is_some_and(|s| is_entry_stem(s, manager)) {
        project_root.join("src/main.rs")
    } else {
        file.with_extension("rs")
    }
}

/// 词干是否为项目入口主函数名：字面 `main`，或语言包中映射到 `main` 的
/// 母语词（zh「主函数」、ja「主関数」、ru「главная」等）。
///
/// `init` 生成 `src/main.<lang>`，教程与示例（.zh-demo）同样使用 `src/main.zh`；
/// 早期项目还可能有母语词干入口（如 zh 的 `src/主函数.zh`）——都须聚合到
/// Cargo 固定入口 `src/main.rs`，否则 cargo 报「no targets specified」。
fn is_entry_stem(stem: &str, manager: &MappingManager) -> bool {
    stem == "main"
        || manager
            .get_keyword_map()
            .iter()
            .any(|(母语词, 英文)| 英文 == "main" && 母语词 == stem)
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
///
/// 使用磁盘持久化增量缓存（~/.rz/cache/transpile-v1.json）：上次运行转译过
/// 且内容未变的文件直接命中，省去整条转译管线；语言包变化时语境指纹失效。
fn transpile_to_english(source: &str, manager: &MappingManager) -> String {
    let mut cache = i18n_rust_engine::cache::TranslationCache::persistent_default();
    transpile_to_english_cached(source, manager, &mut cache)
}

/// 同 [`transpile_to_english`]，复用调用方提供的缓存实例
///
/// 多文件场景（run/check 命令）共享同一会话缓存：入口文件与项目内其他
/// 方言文件内容指纹一致时直接命中，避免每次调用重建缓存、反复读写磁盘。
fn transpile_to_english_cached(
    source: &str,
    manager: &MappingManager,
    cache: &mut i18n_rust_engine::cache::TranslationCache,
) -> String {
    annotate_non_ascii_mods(&transpile_with_map_cached(source, manager, cache).output)
}

/// 同 [`transpile_to_english_cached`]，但保留转译产物的源映射
///
/// 源映射（`pipeline_map`）是诊断列号回译的唯一依据：rustc 报的是英文产物
/// 的列号，须据此回放替换过程才能还原母语源码列号。
/// 缓存命中时映射表随产物一并复用，无需重算。
fn transpile_with_map_cached(
    source: &str,
    manager: &MappingManager,
    cache: &mut i18n_rust_engine::cache::TranslationCache,
) -> i18n_rust_engine::cache::TranspileOutput {
    i18n_rust_engine::transpile_source_with_map(source, manager, cache).unwrap_or_else(|_e| {
        // 缓存失败不阻断转译：回退无缓存管线（与旧行为一致）
        i18n_rust_engine::log_warn!(
            "cli",
            "{}",
            i18n_rust_engine::语言::t("log_transpile_cache_fallback")
        );
        i18n_rust_engine::transpile_pipeline(source, manager)
    })
}

/// 同 [`transpile_with_map_cached`]，附项目级声明上下文（跨文件声明豁免）
///
/// 缓存语境指纹由引擎并入项目上下文指纹（[`transpile_source_with_project`]）：
/// 项目声明集合变化时相关缓存自动失效。
fn transpile_with_map_cached_in_project(
    source: &str,
    manager: &MappingManager,
    cache: &mut i18n_rust_engine::cache::TranslationCache,
    project: Option<&i18n_rust_engine::alias::ProjectContext>,
) -> i18n_rust_engine::cache::TranspileOutput {
    i18n_rust_engine::transpile_source_with_project(source, manager, cache, project).unwrap_or_else(
        |_e| {
            // 缓存失败不阻断转译：回退无缓存管线（与旧行为一致）
            i18n_rust_engine::log_warn!(
                "cli",
                "{}",
                i18n_rust_engine::语言::t("log_transpile_cache_fallback")
            );
            i18n_rust_engine::transpile_pipeline_with_project(source, manager, project)
        },
    )
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

/// 解析与 [`resolve_rustc`] 同工具链的 rustdoc：优先内建/PATH 的独立 rustdoc，
/// 缺失时改用 rustc 的 sysroot 精确定位其配套 rustdoc（rustdoc 与 rustc 同
/// sysroot，版本一致），否则回退 PATH 的 "rustdoc" 由系统报错。
///
/// `mapping auto` 手调 rustdoc 生成 JSON 时必须与 cargo 编译 rlib 用同一
/// rustc，否则跨版本链接触发 E0514（见 doc_json.rs 的 RUSTC 统一注入）。
pub fn resolve_rustdoc() -> PathBuf {
    if let Some(p) = i18n_rust_engine::toolchain::find_toolchain_bin("rustdoc") {
        return p;
    }
    if let Ok(out) = std::process::Command::new(resolve_rustc())
        .arg("--print")
        .arg("sysroot")
        .output()
    {
        let sysroot = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !sysroot.is_empty() {
            let bin = PathBuf::from(sysroot)
                .join("bin")
                .join(format!("rustdoc{}", std::env::consts::EXE_SUFFIX));
            if bin.is_file() {
                return bin;
            }
        }
    }
    PathBuf::from("rustdoc")
}

/// 收集项目级声明上下文（跨文件声明豁免）
///
/// 扫描 src/ 下全部方言文件与入口文件（可能在项目根），收集：
/// - 模块名：方言文件名词干（`模块::成员` 路径链根）；
/// - 声明名：各文件的项名与结构体字段（裸使用处豁免）。
///
/// 与 [`transpile_project_files`] 扫描范围保持一致（src/ 顶层），
/// 入口文件不在 src/ 时单独补扫；读取失败的文件跳过（转译阶段会报错）。
fn collect_project_context(
    project_root: &Path,
    entry_file: &Path,
    manager: &MappingManager,
) -> i18n_rust_engine::alias::ProjectContext {
    use std::collections::HashSet;
    let extensions = lang_manager::all_available_extensions();
    let is_dialect = |name: &str| extensions.iter().any(|e| name.ends_with(&format!(".{e}")));
    let entry_canon = entry_file.canonicalize().ok();
    let mut modules = HashSet::new();
    let mut sources: Vec<String> = Vec::new();
    let mut entry_seen = false;

    if let Ok(entries) = fs::read_dir(project_root.join("src")) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !path.is_file() || !is_dialect(name) {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                modules.insert(stem.to_string());
            }
            if entry_canon.is_some() && path.canonicalize().ok() == entry_canon {
                entry_seen = true;
            }
            if let Ok(source) = fs::read_to_string(&path) {
                sources.push(source);
            }
        }
    }
    // 入口文件不在 src/（如项目根的自定义路径）时单独补扫
    if !entry_seen && entry_file.is_file() {
        if let Some(stem) = entry_file.file_stem().and_then(|s| s.to_str()) {
            modules.insert(stem.to_string());
        }
        if let Ok(source) = fs::read_to_string(entry_file) {
            sources.push(source);
        }
    }

    i18n_rust_engine::alias::ProjectContext::from_sources(
        modules,
        sources.iter().map(String::as_str),
        manager,
    )
}

/// 同步转译项目 src/ 下的全部方言源文件（入口文件除外）为对应 .rs 文件，
/// 使多文件项目的 mod 引用链可用；非已注册方言扩展名的文件（如手写 .rs）跳过。
/// 并行转译项目内其他方言文件，保证多文件项目的 mod 引用链可用
///
/// 转译在 `thread::scope` 中并行执行（教学项目文件相互独立，无共享可变
/// 状态）；共享缓存用 `Mutex` 保护——查询/插入为短临界区，转译本身在锁外
/// 并行，文件多时与串行相比显著提速（缓存命中时仅查表，开销可忽略）。
fn transpile_project_files(
    project_root: &Path,
    entry_file: &Path,
    manager: &MappingManager,
    cache: &std::sync::Mutex<TranslationCache>,
    project: &i18n_rust_engine::alias::ProjectContext,
) -> anyhow::Result<()> {
    let ui = ui::Ui::global();
    let src_dir = project_root.join("src");
    // 无 src 目录时不处理，由 cargo 自行报错
    let Ok(entries) = fs::read_dir(&src_dir) else {
        return Ok(());
    };
    // 入口产物固定写入 src/main.rs：src/ 下任何入口词干的方言文件
    // （main.zh、旧项目的主函数.zh 等）转译后都会覆盖
    // 入口产物，必须跳过
    let entry_abs = entry_file.canonicalize().ok();
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if Some(&path) == entry_abs.as_ref() {
            continue;
        }
        if path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| is_entry_stem(s, manager))
        {
            continue;
        }
        let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if get_lang_code_from_extension(extension).is_none() {
            continue;
        }
        files.push(path);
    }

    // 语境指纹只算一次（全部文件共享同一语言包与项目上下文）；
    // 项目上下文指纹并入后，跨文件声明变化时旧缓存自动失效。
    // 必须与引擎 `transpile_source_with_project` 使用同一组合函数：
    // 入口文件与项目内其它文件共用同一个 TranslationCache 实例，
    // 两处指纹算法若不同，同一语境会算出两个键，缓存互相不可见。
    let fingerprint = i18n_rust_engine::cache::TranslationCache::combine_fingerprint(
        manager.context_fingerprint(),
        Some(project.fingerprint()),
    );
    let first_error: std::sync::Mutex<Option<anyhow::Error>> = std::sync::Mutex::new(None);
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(files.len());
        for path in files {
            // ui/first_error 遮蔽为引用：move 闭包捕获的是 Copy 的共享引用
            let ui = &ui;
            let first_error = &first_error;
            let handle = scope.spawn(move || {
                if first_error
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .is_some()
                {
                    return; // 已有失败文件：跳过剩余工作
                }
                let source = match fs::read_to_string(&path) {
                    Ok(s) => s,
                    Err(e) => {
                        *first_error
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) =
                            Some(anyhow::anyhow!(
                                "{}",
                                ui.f(
                                    "transpile_file_failed",
                                    &[&path.display().to_string(), &e.to_string()]
                                )
                            ));
                        return;
                    }
                };
                // 短临界区：查询缓存（命中直接复用产物）
                let cached = cache
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .query(&source, fingerprint)
                    .cloned();
                let output = match cached {
                    Some(output) => output,
                    None => {
                        // 锁外并行转译，完成后短临界区写回缓存；
                        // 静默管线：教学告警改由下方带文件名输出（多文件项目
                        // 中裸行列无法定位到具体文件）；项目上下文保证
                        // 跨文件调用的成员名与声明侧一致（#8）
                        let output = i18n_rust_engine::transpile_pipeline_quiet_with_project(
                            &source,
                            manager,
                            Some(project),
                        );
                        let display_name = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        emit_teaching_warnings_for_file(
                            &source,
                            &display_name,
                            manager.get_lint_words(),
                        );
                        cache
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .insert(&source, fingerprint, output.clone());
                        output
                    }
                };
                if let Err(e) = write_transpiled(&path.with_extension("rs"), &output.output, ui) {
                    *first_error
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(e);
                }
            });
            handles.push(handle);
        }
        for handle in handles {
            let _ = handle.join();
        }
    });
    match first_error.into_inner().unwrap_or(None) {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

/// 输出单个方言文件的教学告警（Unicode 混淆/全角标点/lint），带文件名归属
///
/// 多文件项目中项目内文件分散在多个方言文件，裸行列无法定位具体文件；
/// 入口文件的告警仍由转译管线直接输出（裸行列即命令传入的入口文件）。
/// 时机与转译一致（仅缓存未命中时输出），静默管线保证不重复输出。
fn emit_teaching_warnings_for_file(source: &str, display_name: &str, lint_words: &HashSet<String>) {
    for warning in i18n_rust_engine::unicode_confusion::check_unicode_confusion(source) {
        i18n_rust_engine::log_warn!(
            "unicode_confusion",
            "{}：{}",
            display_name,
            warning.format()
        );
    }
    for warning in i18n_rust_engine::fullwidth::find_fullwidth_punct(source) {
        i18n_rust_engine::log_warn!("fullwidth", "{}：{}", display_name, warning.format());
    }
    if i18n_rust_engine::lint::teaching_lint_enabled() {
        for warning in i18n_rust_engine::lint::lint_teaching_with_words(source, lint_words) {
            i18n_rust_engine::log_warn!("lint", "{}：{}", display_name, warning.format());
        }
    }
}

/// 写转译产物：目标已存在且内容不同时先备份为 `.rs.bak`，绝不静默覆盖用户文件。
///
/// 幂等重跑（内容一致）不产生备份；无同名手写文件时行为与直接写入完全一致。
fn write_transpiled(path: &Path, content: &str, ui: &crate::ui::Ui) -> anyhow::Result<()> {
    // 方言文件直接放在项目根等场景下 src/ 可能尚不存在，先创建父目录
    //（否则 fs::write 报裸 ENOENT，LSP 侧 translation_cache 已有同款处理）
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Ok(existing) = fs::read_to_string(path)
        && existing != content
    {
        let backup = path.with_extension("rs.bak");
        fs::rename(path, &backup)?;
        println!(
            "{}",
            ui.f(
                "transpile_backup",
                &[&path.display().to_string(), &backup.display().to_string()]
            )
        );
    }
    fs::write(path, content)?;
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

/// 输出母语 ↔ Rust 映射速查表（`rzc cheat`）
///
/// 四张表（关键字/模块路径/别名/派生特征）逐节输出，母语词按显示宽度对齐；
/// 母语词与英文原词相同的恒等条目不输出（如 en 语言包整表恒等，仅提示）。
/// `markdown` 模式输出可嵌入教程/README 的表格。
fn print_cheat(manager: &MappingManager, lang_code: &str, markdown: bool) {
    let derive_map = manager.get_derive_map();
    let sections: [(&str, &HashMap<String, String>); 4] = [
        ("Keywords / 关键字", manager.get_keyword_map()),
        ("Module paths / 模块路径", manager.get_module_path_map()),
        ("Aliases / 别名", manager.get_alias_map()),
        ("Derives / 派生特征", derive_map),
    ];

    // 过滤恒等条目（native == english）后统计剩余量：全恒等则无需速查
    let sections: Vec<(&str, Vec<(String, String)>)> = sections
        .into_iter()
        .map(|(title, map)| {
            let rows: Vec<(String, String)> = map
                .iter()
                .filter(|(native, en)| native != en)
                .map(|(native, en)| (native.clone(), en.clone()))
                .collect();
            (title, rows)
        })
        .collect();

    let total: usize = sections.iter().map(|(_, rows)| rows.len()).sum();
    if total == 0 {
        println!("rzc cheat — {lang_code} ↔ Rust");
        println!();
        println!("This language pack maps every identifier to itself (identity mapping).");
        println!("No cheat sheet is needed — write Rust as usual.");
        return;
    }

    if markdown {
        println!("# rzc cheat — {lang_code} ↔ Rust ({total})");
    } else {
        println!("rzc cheat — {lang_code} ↔ Rust（共 {total} 条）");
    }
    println!();
    for (title, rows) in &sections {
        if rows.is_empty() {
            continue;
        }
        if markdown {
            println!("## {title}");
            println!();
            println!("| {lang_code} | Rust |");
            println!("|---|---|");
            for (native, en) in rows {
                println!("| {native} | `{en}` |");
            }
            println!();
        } else {
            let width = rows
                .iter()
                .map(|(n, _)| n.chars().count())
                .max()
                .unwrap_or(0);
            println!("── {title} ──");
            for (native, en) in rows {
                let pad = width - native.chars().count();
                println!("  {native}{}  {en}", " ".repeat(pad));
            }
            println!();
        }
    }
    if markdown {
        println!("> Generated by `rzc cheat {lang_code} --markdown`.");
    }
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
    // 映射数据来源日志（语言包开发调试：RZ_LOG=info rzc check 可见）
    // 注意：load_mapping 先于转译管线执行，logger 需在此提前初始化（幂等）
    i18n_rust_engine::logger::init();
    let ui = ui_for_file(source_file.unwrap_or(Path::new("")), &lang_pack_path);
    // 1. 如果用户通过 --lang-pack 指定了外部目录，强制使用
    if let Some(path) = lang_pack_path {
        i18n_rust_engine::log_info!(
            "映射加载",
            "{}",
            ui.f("mapping_source_explicit", &[&path.display().to_string()])
        );
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
            i18n_rust_engine::log_info!(
                "映射加载",
                "{}",
                ui.f(
                    "mapping_source_project",
                    &[&local_path.display().to_string()]
                )
            );
            return MappingManager::load_from_dir(local_path).map_err(|e| {
                anyhow::anyhow!("{}", ui.f("load_local_lang_pack_failed", &[&e.to_string()]))
            });
        }
    }
    // 4. 全局用户语言包目录
    let global_path = lang_manager::global_lang_dir().join(&lang_code);
    if global_path.exists() {
        i18n_rust_engine::log_info!(
            "映射加载",
            "{}",
            ui.f(
                "mapping_source_global",
                &[&global_path.display().to_string()]
            )
        );
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
    i18n_rust_engine::log_info!(
        "映射加载",
        "{}",
        ui.f("mapping_source_builtin", &[&lang_code])
    );
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
        .mut_arg("no_lint", |arg| arg.help(ui.t("arg_no_lint_help")))
        .mut_subcommand("init", |cmd| {
            cmd.about(ui.t("cmd_init_about"))
                .mut_arg("lang", |arg| arg.help(ui.t("arg_lang_help")))
        })
        .mut_subcommand("run", |cmd| cmd.about(ui.t("cmd_run_about")))
        .mut_subcommand("check", |cmd| cmd.about(ui.t("cmd_check_about")))
        .mut_subcommand("eject", |cmd| cmd.about(ui.t("cmd_eject_about")))
        .mut_subcommand("transpile", |cmd| cmd.about(ui.t("cmd_transpile_about")))
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
                .mut_subcommand("search", |sub| sub.about(ui.t("cmd_lang_search_about")))
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
        .mut_subcommand("crate", |cmd| {
            cmd.about(ui.t("cmd_crate_about"))
                .mut_subcommand("search", |sub| {
                    sub.about(ui.t("cmd_crate_search_about"))
                        .mut_arg("keyword", |arg| arg.help(ui.t("arg_crate_keyword_help")))
                })
                .mut_subcommand("install", |sub| {
                    sub.about(ui.t("cmd_crate_install_about"))
                        .mut_arg("lang", |arg| arg.help(ui.t("arg_lang_help")))
                        .mut_arg("force", |arg| arg.help(ui.t("arg_force_help")))
                })
                .mut_subcommand("list", |sub| sub.about(ui.t("cmd_crate_list_about")))
                .mut_subcommand("remove", |sub| {
                    sub.about(ui.t("cmd_crate_remove_about"))
                        .mut_arg("lang", |arg| arg.help(ui.t("arg_lang_help")))
                })
                .mut_subcommand("update", |sub| sub.about(ui.t("cmd_crate_update_about")))
                .mut_subcommand("publish", |sub| {
                    sub.about(ui.t("cmd_crate_publish_about"))
                        .mut_arg("lang", |arg| arg.help(ui.t("arg_lang_help")))
                        .mut_arg("file", |arg| arg.help(ui.t("arg_crate_file_help")))
                        .mut_arg("author", |arg| arg.help(ui.t("arg_crate_author_help")))
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
        annotate_non_ascii_mods, annotate_non_ascii_mods_with_lines, collect_project_context,
        detect_toolchain_channel, entry_output_path, find_alias_in_toml,
        get_lang_code_from_extension, transpile_project_files, transpile_to_english,
        transpile_with_map_cached_in_project, write_transpiled,
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

    /// 入口产物路径：入口词干（main / 语言包主函数词）写入 src/main.rs，
    /// 非入口文件跟随自身扩展名，不占用 src/main.rs
    /// （weix 工具异常 #4：build.zh 不得覆盖入口产物）
    #[test]
    fn test_entry_output_path() {
        use std::path::{Path, PathBuf};
        let root = Path::new("/proj");
        let m = zh_manager();
        // src/main.zh（词干 main）→ 聚合到 Cargo 固定入口
        assert_eq!(
            entry_output_path(root, Path::new("/proj/src/main.zh"), &m),
            PathBuf::from("/proj/src/main.rs")
        );
        // 母语词干入口「主函数」（映射 main，旧项目命名）→ 同样聚合到 src/main.rs
        // （回归：此前仅认字面 main，主函数.zh 产物写自身名致 cargo 找不到目标）
        assert_eq!(
            entry_output_path(root, Path::new("/proj/src/主函数.zh"), &m),
            PathBuf::from("/proj/src/main.rs")
        );
        // 项目根的 build.zh（词干非入口）→ 同目录 build.rs，绝不触碰 src/main.rs
        assert_eq!(
            entry_output_path(root, Path::new("/proj/build.zh"), &m),
            PathBuf::from("/proj/build.rs")
        );
        // src 下非入口模块（如 helper.zh）→ src/helper.rs
        assert_eq!(
            entry_output_path(root, Path::new("/proj/src/helper.zh"), &m),
            PathBuf::from("/proj/src/helper.rs")
        );
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

    /// 多文件转译：src/ 下的其他方言文件生成同名 .rs，入口与手写 .rs 不受影响；
    /// 跨文件声明豁免（#8）：其他文件声明的与映射词同名的成员不被替换
    #[test]
    fn test_transpile_project_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        let entry = root.join("src/main.zh");
        // 跨文件调用本项目的 `函数 新建()`（撞 `新建`=new 映射）：
        // 项目上下文让调用位与声明侧一致（不被替换出 `new`）
        std::fs::write(
            &entry,
            "模组 辅助;\n\n函数 main() {\n    让 x = 辅助::新建();\n}\n",
        )
        .unwrap();
        std::fs::write(root.join("src/辅助.zh"), "公开 函数 新建() {}\n").unwrap();
        std::fs::write(root.join("src/manual.rs"), "// 手写文件不覆盖\n").unwrap();

        let manager = zh_manager();
        let cache = std::sync::Mutex::new(i18n_rust_engine::cache::TranslationCache::new(8));
        // 项目级声明上下文：模块名（文件词干）+ 声明名（入口+src/ 全扫）
        let ctx = collect_project_context(root, &entry, &manager);
        assert!(ctx.modules.contains("辅助"), "模块名应含文件名词干");
        assert!(ctx.names.contains("新建"), "声明名应含其他文件的函数名");
        transpile_project_files(root, &entry, &manager, &cache, &ctx).unwrap();

        // 其他方言文件已转译为同名 .rs
        let helper_rs = std::fs::read_to_string(root.join("src/辅助.rs")).unwrap();
        assert!(helper_rs.contains("pub fn 新建()"), "实际输出：{helper_rs}");
        // 手写 .rs 不被触碰
        let manual_rs = std::fs::read_to_string(root.join("src/manual.rs")).unwrap();
        assert!(manual_rs.contains("手写文件不覆盖"));
        // 入口文件未被重复转译（无 main.rs 产生，由调用方单独写入）
        assert!(!root.join("src/main.rs").exists());

        // 入口转译（带项目上下文）：跨文件调用位 `新建` 不被替换（#8）
        let entry_out = transpile_with_map_cached_in_project(
            &std::fs::read_to_string(&entry).unwrap(),
            &manager,
            &mut cache.lock().unwrap(),
            Some(&ctx),
        )
        .output;
        assert!(
            entry_out.contains("辅助::新建()"),
            "跨文件调用位应与声明侧一致：{entry_out}"
        );
    }

    /// write_transpiled 备份语义（let-chain 守卫）：目标存在且内容不同才备份为 .rs.bak；
    /// 内容相同（幂等重跑）不产生备份，绝不静默覆盖用户文件
    #[test]
    fn test_write_transpiled_backup_only_on_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.rs");
        let ui = crate::ui::Ui::for_lang("zh");

        // 首次写入：文件不存在，直接写盘，无备份
        write_transpiled(&path, "v1", &ui).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v1");
        assert!(!path.with_extension("rs.bak").exists());

        // 内容相同：幂等重跑，不产生备份，文件保持 v1
        write_transpiled(&path, "v1", &ui).unwrap();
        assert!(!path.with_extension("rs.bak").exists());

        // 内容不同：旧内容备份为 .rs.bak，文件更新为新内容
        write_transpiled(&path, "v2", &ui).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v2");
        assert_eq!(
            std::fs::read_to_string(path.with_extension("rs.bak")).unwrap(),
            "v1"
        );
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

    /// 注解行映射：磁盘行 → 引擎直出行（0-based），注解行归属其 mod 声明行，
    /// 多处注解时偏移逐处累积（对应多模块入口 main.zh 的诊断行号换算）
    #[test]
    fn test_annotate_non_ascii_mod_line_map() {
        // 单处注解：磁盘 0/1 行（#[path] 与 mod）均归属引擎第 0 行
        let (out, line_map) = annotate_non_ascii_mods_with_lines("mod 数学;\nfn main() {}");
        assert_eq!(out, "#[path = \"数学.rs\"]\nmod 数学;\nfn main() {}");
        assert_eq!(line_map, vec![0, 0, 1]);

        // 无注解时映射恒等
        let (_, line_map) = annotate_non_ascii_mods_with_lines("fn main() {}\nfn f() {}");
        assert_eq!(line_map, vec![0, 1]);

        // 两处注解：后续行偏移为 2
        let two = "mod 数学;\nmod 物理;\nfn main() {}";
        let (out, line_map) = annotate_non_ascii_mods_with_lines(two);
        assert_eq!(
            out,
            "#[path = \"数学.rs\"]\nmod 数学;\n#[path = \"物理.rs\"]\nmod 物理;\nfn main() {}"
        );
        assert_eq!(line_map, vec![0, 0, 1, 1, 2]);
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
