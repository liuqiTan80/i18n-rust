//! i18n-rust LSP 代理服务器
//!
//! 作为 LSP 服务器接收编辑器的连接，将中文 Rust (.zh) 文件
//! 翻译为标准 Rust 后交给 rust-analyzer 处理，实现中文代码的
//! 智能补全、错误提示等功能。
//!
//! 用法：
//!   i18n-rust-lsp [--language-pack <路径>]
//!
//! 默认语言包路径：lang-packs/zh

/// LSP 代理服务器核心（握手、文档同步、请求转发、消息路由）
mod server;
/// rust-analyzer 子进程管理（启动、消息收发、关闭）
mod analyzer;
/// 响应位置映射（虚拟 .rs 坐标 → 原始 .zh 坐标 + 诊断翻译）
mod response_map;
/// 翻译缓存（虚拟文件系统，维护 .zh → .rs 翻译缓存与行号映射）
mod translation_cache;

use std::path::PathBuf;

/// 命令行参数
struct CliArgs {
    /// 语言包目录路径
    lang_pack_path: PathBuf,
}

/// 解析命令行参数
fn parse_args() -> CliArgs {
    let mut lang_pack_path = PathBuf::from("lang-packs/zh");

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--language-pack" | "-l" => {
                if i + 1 < args.len() {
                    lang_pack_path = PathBuf::from(&args[i + 1]);
                    i += 2;
                } else {
                    eprintln!("错误: --language-pack 需要指定路径");
                    std::process::exit(1);
                }
            }
            "--help" | "-h" => {
                println!("i18n-rust LSP 代理服务器");
                println!();
                println!("用法: i18n-rust-lsp [选项]");
                println!();
                println!("选项:");
                println!("  --language-pack, -l <路径>  语言包目录路径（默认: lang-packs/zh）");
                println!("  --help, -h                  显示帮助信息");
                std::process::exit(0);
            }
            _ => {
                eprintln!("未知参数: {}", args[i]);
                eprintln!("使用 --help 查看帮助");
                std::process::exit(1);
            }
        }
    }

    CliArgs { lang_pack_path }
}

fn main() -> anyhow::Result<()> {
    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    log::info!("i18n-rust LSP 代理服务器启动");

    // 解析命令行参数
    let args = parse_args();
    log::info!("语言包路径: {:?}", args.lang_pack_path);

    // 创建代理服务器
    let (server, io_threads) = server::ProxyServer::new(&args.lang_pack_path)?;

    // 运行服务器（阻塞直到退出）
    server.run(io_threads)?;

    log::info!("i18n-rust LSP 代理服务器已退出");
    Ok(())
}
