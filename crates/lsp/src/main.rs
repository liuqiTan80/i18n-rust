//! i18n-rust 语言服务代理服务器（二进制入口）
//!
//! 作为语言服务器接收编辑器的连接，将方言 Rust 文件（.zh/.de 等）
//! 翻译为标准 Rust 后交给分析器处理，实现母语代码的
//! 智能补全、错误提示等功能。
//!
//! 用法：
//!   i18n-rust-lsp [语言包路径选项] [扩展名清单选项]
//!
//! 默认语言包路径：crates/engine/lang-packs/zh（主仓库单副本）；默认扩展名：全部内置语言包的扩展名
//!
//! 功能模块在库目标（[`语言服务库`]）中实现，本文件仅做
//! 命令行解析与启动；模块清单与职责见 `src/lib.rs`。

use std::path::PathBuf;

use i18n_rust_lsp::{服务器, 界面消息};

/// 命令行参数
struct 命令行参数 {
    /// 语言包目录路径
    语言包路径: PathBuf,
    /// 支持的方言文件扩展名列表（如 `.zh`）
    扩展名表: Vec<String>,
}

/// 解析逗号分隔的扩展名列表，自动补充点号前缀
///
/// 空项（如 `zh,,en`）被忽略。
/// 示例：`zh, .de,ja` → `.zh`、`.de`、`.ja`
fn 解析扩展名表(清单文本: &str) -> Vec<String> {
    清单文本
        .split(',')
        .map(|单项| 单项.trim())
        .filter(|单项| !单项.is_empty())
        .map(|单项| {
            if 单项.starts_with('.') {
                单项.to_string()
            } else {
                format!(".{}", 单项)
            }
        })
        .collect()
}

/// 解析命令行参数
fn 解析参数() -> 命令行参数 {
    let mut 语言包路径 = PathBuf::from("crates/engine/lang-packs/zh");
    let mut 扩展名表: Vec<String> = Vec::new();

    let 参数表: Vec<String> = std::env::args().collect();
    let mut 下标 = 1;
    while 下标 < 参数表.len() {
        match 参数表[下标].as_str() {
            "--language-pack" | "-l" => {
                if 下标 + 1 < 参数表.len() {
                    语言包路径 = PathBuf::from(&参数表[下标 + 1]);
                    下标 += 2;
                } else {
                    let 界面表 = 界面消息::界面::载入(&语言包路径);
                    eprintln!("{}", 界面表.取消息("lsp_err_lang_pack"));
                    std::process::exit(1);
                }
            }
            "--extensions" | "-e" => {
                if 下标 + 1 < 参数表.len() {
                    扩展名表 = 解析扩展名表(&参数表[下标 + 1]);
                    下标 += 2;
                } else {
                    let 界面表 = 界面消息::界面::载入(&语言包路径);
                    eprintln!("{}", 界面表.取消息("lsp_err_extensions"));
                    std::process::exit(1);
                }
            }
            "--help" | "-h" => {
                let 界面表 = 界面消息::界面::载入(&语言包路径);
                打印帮助(&界面表);
                std::process::exit(0);
            }
            // 输出版本号（纯版本号，供安装命令的版本校验解析）
            "--version" | "-V" => {
                println!("{}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            // 编辑器语言客户端会传递标准输入输出模式参数，本程序默认就用标准输入输出，直接忽略
            "--stdio" => {
                下标 += 1;
            }
            _ => {
                let 界面表 = 界面消息::界面::载入(&语言包路径);
                eprintln!("{}", 界面表.取带参消息("lsp_unknown_arg", &[&参数表[下标]]));
                eprintln!("{}", 界面表.取消息("lsp_use_help"));
                std::process::exit(1);
            }
        }
    }

    命令行参数 {
        语言包路径,
        扩展名表,
    }
}

/// 打印帮助文本（提示语随语言包/系统语言本地化）
fn 打印帮助(界面表: &界面消息::界面) {
    println!("{}", 界面表.取消息("lsp_about"));
    println!();
    println!("{}", 界面表.取消息("lsp_usage"));
    println!();
    println!("{}", 界面表.取消息("lsp_options"));
    println!("{}", 界面表.取消息("lsp_lang_pack_opt"));
    println!("{}", 界面表.取消息("lsp_extensions_opt"));
    println!("{}", 界面表.取消息("lsp_version_opt"));
    println!("{}", 界面表.取消息("lsp_help_opt"));
}

/// 当默认语言包路径不存在时，自动搜索常见位置
///
/// 搜索顺序：
/// 1. 二进制所在目录向上搜索（最多五级）
/// 2. 当前工作目录向上搜索
/// 3. 主目录下常见项目目录
fn 查找语言包回退(默认路径: &std::path::Path, 语言代码: &str) -> PathBuf {
    if 默认路径.exists() {
        return 默认路径.to_path_buf();
    }
    log::warn!("默认语言包路径 {} 不存在，正在搜索...", 默认路径.display());
    // 1. 二进制所在目录向上搜索
    if let Ok(程序路径) = std::env::current_exe()
        && let Some(命中路径) = 向上搜索(&程序路径, 语言代码)
    {
        log::info!("在二进制目录找到语言包: {}", 命中路径.display());
        return 命中路径;
    }
    // 2. 当前工作目录向上搜索
    if let Ok(工作目录) = std::env::current_dir()
        && let Some(命中路径) = 向上搜索(&工作目录, 语言代码)
    {
        log::info!("在工作目录找到语言包: {}", 命中路径.display());
        return 命中路径;
    }
    // 3. 主目录下常见项目目录（两种布局：主仓库单副本与用户项目约定）
    if let Ok(主目录) = std::env::var("HOME") {
        for 项目名 in &["code/zrRust", "zrRust"] {
            for 布局名 in &["crates/engine/lang-packs", "lang-packs"] {
                let 候选路径 = PathBuf::from(&主目录)
                    .join(项目名)
                    .join(布局名)
                    .join(语言代码);
                if 候选路径.exists() {
                    log::info!("在主目录找到语言包: {}", 候选路径.display());
                    return 候选路径;
                }
            }
        }
    }
    log::warn!("未找到语言包目录，使用内置映射");
    默认路径.to_path_buf()
}

/// 从指定路径向上搜索语言包目录（最多五级；两种布局均探测：
/// 主仓库单副本与用户项目约定）
fn 向上搜索(起点: &std::path::Path, 语言代码: &str) -> Option<PathBuf> {
    let mut 当前目录 = 起点.parent()?.to_path_buf();
    for _ in 0..5 {
        for 布局名 in &["crates/engine/lang-packs", "lang-packs"] {
            let 候选路径 = 当前目录.join(布局名).join(语言代码);
            if 候选路径.exists() {
                return Some(候选路径);
            }
        }
        if !当前目录.pop() {
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
    let mut 参数 = 解析参数();

    // 如果语言包路径不存在，自动搜索（仅在未显式指定时）
    let 显式指定 =
        std::env::args().any(|单个参数| 单个参数 == "--language-pack" || 单个参数 == "-l");
    if !显式指定 && !参数.语言包路径.exists() {
        // 从默认路径推断语言代码
        let 语言代码 = 参数
            .语言包路径
            .file_name()
            .and_then(|名称值| 名称值.to_str())
            .unwrap_or("zh");
        参数.语言包路径 = 查找语言包回退(&参数.语言包路径, 语言代码);
    }

    // 初始化全局界面消息（随语言包/系统语言变化）
    界面消息::设定全局(&参数.语言包路径);
    log::info!("{}", 界面消息::全局界面().取消息("lsp_log_start"));
    log::info!(
        "{}",
        界面消息::全局界面().取带参消息(
            "lsp_log_lang_pack",
            &[&参数.语言包路径.display().to_string()]
        )
    );

    // 创建代理服务器
    let (服务器实例, 输入输出线程) =
        服务器::代理服务器::新建(&参数.语言包路径, &参数.扩展名表)?;

    // 运行服务器（阻塞直到退出）
    服务器实例.运行(输入输出线程)?;

    log::info!("{}", 界面消息::全局界面().取消息("lsp_log_exit"));
    Ok(())
}

#[cfg(test)]
mod 母语测试 {
    use super::{向上搜索, 查找语言包回退, 解析扩展名表};

    /// 带点与不带点的扩展名统一补点
    #[test]
    fn 扩展名补点() {
        assert_eq!(解析扩展名表(".zh,.de,.ja"), vec![".zh", ".de", ".ja"]);
        assert_eq!(解析扩展名表("zh, de,ja"), vec![".zh", ".de", ".ja"]);
    }

    /// 空列表与单个扩展名
    #[test]
    fn 扩展名边界情形() {
        assert!(解析扩展名表("").is_empty());
        assert!(解析扩展名表("  , ").is_empty());
        assert_eq!(解析扩展名表("zh"), vec![".zh"]);
        assert_eq!(解析扩展名表("zh,,de"), vec![".zh", ".de"]);
    }

    /// 向上搜索：命中主仓库布局与平铺布局；超过五级的祖先命中不到
    #[test]
    fn 向上搜索布局与深度上限() {
        let 临时 = tempfile::tempdir().unwrap();

        // 布局一：主仓库单副本
        let 嵌套目录 = 临时.path().join("repo-nested");
        let 语言包 = 嵌套目录.join("crates/engine/lang-packs/zh");
        std::fs::create_dir_all(&语言包).unwrap();
        let 起点 = 嵌套目录.join("target/debug/deps/bin");
        assert_eq!(向上搜索(&起点, "zh").as_deref(), Some(语言包.as_path()));

        // 布局二：平铺语言包目录
        let 平铺目录 = 临时.path().join("repo-flat");
        let 语言包二 = 平铺目录.join("lang-packs/ja");
        std::fs::create_dir_all(&语言包二).unwrap();
        let 起点二 = 平铺目录.join("a/b/bin");
        assert_eq!(向上搜索(&起点二, "ja").as_deref(), Some(语言包二.as_path()));

        // 语言代码不存在 → 空值（不误中其他语言目录）
        assert!(向上搜索(&起点, "xx").is_none());

        // 包目录在六级祖先之外：向上最多五级，找不到
        let 深层 = 临时.path().join("d/l1/l2/l3/l4/l5/l6");
        std::fs::create_dir_all(&深层).unwrap();
        let 语言包三 = 临时.path().join("d/lang-packs/zh");
        std::fs::create_dir_all(&语言包三).unwrap();
        assert!(向上搜索(&深层.join("bin"), "zh").is_none());
    }

    /// 默认路径已存在时直接采用，不触发任何搜索（显式指定语言包路径）
    #[test]
    fn 默认路径存在直接采用() {
        let 临时 = tempfile::tempdir().unwrap();
        let 默认路径 = 临时.path().join("explicit/zh");
        std::fs::create_dir_all(&默认路径).unwrap();
        assert_eq!(查找语言包回退(&默认路径, "zh"), 默认路径);
    }
}
