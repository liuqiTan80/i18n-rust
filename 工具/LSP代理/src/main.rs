//! i18n-rust LSP 代理服务器
//!
//! 作为 LSP 服务器接收编辑器的连接，将中文 Rust (.zh) 文件
//! 翻译为标准 Rust 后交给 rust-analyzer 处理，实现中文代码的
//! 智能补全、错误提示等功能。
//!
//! 用法：
//!   i18n-rust-lsp [--language-pack <路径>]
//!
//! 默认语言包路径：语言包/中文

#[path = "翻译缓存.rs"]
mod 翻译缓存;
#[path = "分析器连接.rs"]
mod 分析器连接;
#[path = "响应映射.rs"]
mod 响应映射;
#[path = "代理服务器.rs"]
mod 代理服务器;

use std::path::PathBuf;

/// 命令行参数
struct 命令行参数 {
    语言包路径: PathBuf,
}

/// 解析命令行参数
fn 解析参数() -> 命令行参数 {
    let mut 语言包路径 = PathBuf::from("语言包/中文");

    let 参数列表: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < 参数列表.len() {
        match 参数列表[i].as_str() {
            "--language-pack" | "-l" => {
                if i + 1 < 参数列表.len() {
                    语言包路径 = PathBuf::from(&参数列表[i + 1]);
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
                println!("  --language-pack, -l <路径>  语言包目录路径（默认: 语言包/中文）");
                println!("  --help, -h                  显示帮助信息");
                std::process::exit(0);
            }
            _ => {
                eprintln!("未知参数: {}", 参数列表[i]);
                eprintln!("使用 --help 查看帮助");
                std::process::exit(1);
            }
        }
    }

    命令行参数 { 语言包路径 }
}

fn main() -> anyhow::Result<()> {
    // 初始化日志
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    )
    .format_timestamp_millis()
    .init();

    log::info!("i18n-rust LSP 代理服务器启动");

    // 解析命令行参数
    let 参数 = 解析参数();
    log::info!("语言包路径: {:?}", 参数.语言包路径);

    // 创建代理服务器
    let (服务器, io线程) = 代理服务器::代理服务器::新建(&参数.语言包路径)?;

    // 运行服务器（阻塞直到退出）
    服务器.运行(io线程)?;

    log::info!("i18n-rust LSP 代理服务器已退出");
    Ok(())
}
