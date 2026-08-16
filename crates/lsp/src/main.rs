//! i18n-rust LSP 代理服务器
//!
//! 作为 LSP 服务器接收编辑器的连接，将方言 Rust 文件（.zh/.en/.de 等）
//! 翻译为标准 Rust 后交给 rust-analyzer 处理，实现母语代码的
//! 智能补全、错误提示等功能。
//!
//! 用法：
//!   i18n-rust-lsp [--language-pack <路径>] [--extensions .zh,.en]
//!
//! 默认语言包路径：lang-packs/zh；默认扩展名：全部 11 个内置语言包的扩展名

/// rust-analyzer 子进程管理（启动、消息收发、关闭）
mod analyzer;
/// 响应位置映射（虚拟 .rs 坐标 → 原始 .zh 坐标 + 诊断翻译）
mod response_map;
/// LSP 代理服务器核心（握手、文档同步、请求转发、消息路由）
mod server;
/// 翻译缓存（虚拟文件系统，维护 .zh → .rs 翻译缓存与行号映射）
mod translation_cache;
/// 界面消息本地化（帮助与错误提示随语言包/系统语言变化）
mod ui;

use std::path::PathBuf;

/// 命令行参数
struct CliArgs {
    /// 语言包目录路径
    lang_pack_path: PathBuf,
    /// 支持的方言文件扩展名列表（如 `.zh`）
    extensions: Vec<String>,
}

/// 解析逗号分隔的扩展名列表，自动补充 `.` 前缀
///
/// 空项（如 `"zh,,en"`）被忽略。
/// 示例：`"zh, .en,de"` → `[".zh", ".en", ".de"]`
fn parse_extensions(list: &str) -> Vec<String> {
    list.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.starts_with('.') {
                s.to_string()
            } else {
                format!(".{}", s)
            }
        })
        .collect()
}

/// 解析命令行参数
fn parse_args() -> CliArgs {
    let mut lang_pack_path = PathBuf::from("lang-packs/zh");
    let mut extensions: Vec<String> = Vec::new();

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--language-pack" | "-l" => {
                if i + 1 < args.len() {
                    lang_pack_path = PathBuf::from(&args[i + 1]);
                    i += 2;
                } else {
                    let ui = ui::Ui::load(&lang_pack_path);
                    eprintln!("{}", ui.t("lsp_err_lang_pack"));
                    std::process::exit(1);
                }
            }
            "--extensions" | "-e" => {
                if i + 1 < args.len() {
                    extensions = parse_extensions(&args[i + 1]);
                    i += 2;
                } else {
                    let ui = ui::Ui::load(&lang_pack_path);
                    eprintln!("{}", ui.t("lsp_err_extensions"));
                    std::process::exit(1);
                }
            }
            "--help" | "-h" => {
                let ui = ui::Ui::load(&lang_pack_path);
                print_help(&ui);
                std::process::exit(0);
            }
            // VSCode LanguageClient 会传递 --stdio 参数，我们默认就使用 stdio，直接忽略
            "--stdio" => {
                i += 1;
            }
            _ => {
                let ui = ui::Ui::load(&lang_pack_path);
                eprintln!("{}", ui.f("lsp_unknown_arg", &[&args[i]]));
                eprintln!("{}", ui.t("lsp_use_help"));
                std::process::exit(1);
            }
        }
    }

    CliArgs {
        lang_pack_path,
        extensions,
    }
}

/// 打印帮助文本（提示语随语言包/系统语言本地化）
fn print_help(ui: &ui::Ui) {
    println!("{}", ui.t("lsp_about"));
    println!();
    println!("{}", ui.t("lsp_usage"));
    println!();
    println!("{}", ui.t("lsp_options"));
    println!("{}", ui.t("lsp_lang_pack_opt"));
    println!("{}", ui.t("lsp_extensions_opt"));
    println!("{}", ui.t("lsp_help_opt"));
}

/// 当默认语言包路径不存在时，自动搜索常见位置
///
/// 搜索顺序：
/// 1. 二进制所在目录向上搜索（最多 5 级）
/// 2. 当前工作目录向上搜索
/// 3. $HOME 下常见项目目录（code/zrRust、zrRust）
fn find_lang_pack_fallback(default: &std::path::Path, lang_code: &str) -> PathBuf {
    if default.exists() {
        return default.to_path_buf();
    }
    log::warn!("默认语言包路径 {} 不存在，正在搜索...", default.display());
    // 1. 二进制所在目录向上搜索
    if let Ok(exe) = std::env::current_exe()
        && let Some(found) = search_upward(&exe, lang_code)
    {
        log::info!("在二进制目录找到语言包: {}", found.display());
        return found;
    }
    // 2. 当前工作目录向上搜索
    if let Ok(cwd) = std::env::current_dir()
        && let Some(found) = search_upward(&cwd, lang_code)
    {
        log::info!("在工作目录找到语言包: {}", found.display());
        return found;
    }
    // 3. $HOME 下常见项目目录
    if let Ok(home) = std::env::var("HOME") {
        for project in &["code/zrRust", "zrRust"] {
            let candidate = PathBuf::from(&home)
                .join(project)
                .join("lang-packs")
                .join(lang_code);
            if candidate.exists() {
                log::info!("在 HOME 目录找到语言包: {}", candidate.display());
                return candidate;
            }
        }
    }
    log::warn!("未找到语言包目录，使用内置映射");
    default.to_path_buf()
}

/// 从指定路径向上搜索 lang-packs/<lang_code>（最多 5 级）
fn search_upward(start: &std::path::Path, lang_code: &str) -> Option<PathBuf> {
    let mut dir = start.parent()?.to_path_buf();
    for _ in 0..5 {
        let candidate = dir.join("lang-packs").join(lang_code);
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

fn main() -> anyhow::Result<()> {
    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    // 解析命令行参数
    let mut args = parse_args();

    // 如果语言包路径不存在，自动搜索（仅在未显式指定时）
    let 显式指定 = std::env::args().any(|a| a == "--language-pack" || a == "-l");
    if !显式指定 && !args.lang_pack_path.exists() {
        // 从默认路径推断语言代码
        let lang_code = args
            .lang_pack_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("zh");
        args.lang_pack_path = find_lang_pack_fallback(&args.lang_pack_path, lang_code);
    }

    // 初始化全局界面消息（随语言包/系统语言变化）
    ui::init(&args.lang_pack_path);
    log::info!("{}", ui::global().t("lsp_log_start"));
    log::info!(
        "{}",
        ui::global().f(
            "lsp_log_lang_pack",
            &[&args.lang_pack_path.display().to_string()]
        )
    );

    // 创建代理服务器
    let (server, io_threads) = server::ProxyServer::new(&args.lang_pack_path, &args.extensions)?;

    // 运行服务器（阻塞直到退出）
    server.run(io_threads)?;

    log::info!("{}", ui::global().t("lsp_log_exit"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_extensions;

    /// 带点与不带点的扩展名统一补点
    #[test]
    fn test_parse_extensions_unifies_dot_prefix() {
        assert_eq!(parse_extensions(".zh,.en,.de"), vec![".zh", ".en", ".de"]);
        assert_eq!(parse_extensions("zh, en,de"), vec![".zh", ".en", ".de"]);
    }

    /// 空列表与单个扩展名
    #[test]
    fn test_parse_extensions_edge_cases() {
        assert!(parse_extensions("").is_empty());
        assert!(parse_extensions("  , ").is_empty());
        assert_eq!(parse_extensions("zh"), vec![".zh"]);
        assert_eq!(parse_extensions("zh,,en"), vec![".zh", ".en"]);
    }
}
