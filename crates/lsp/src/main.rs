//! i18n-rust LSP 代理服务器（二进制入口）
//!
//! 作为 LSP 服务器接收编辑器的连接，将方言 Rust 文件（.zh/.de 等）
//! 翻译为标准 Rust 后交给 rust-analyzer 处理，实现母语代码的
//! 智能补全、错误提示等功能。
//!
//! 用法：
//!   i18n-rust-lsp [--language-pack <路径>] [--extensions .zh,.de]
//!
//! 默认语言包路径：crates/engine/lang-packs/zh（主仓库单副本）；默认扩展名：全部内置语言包的扩展名
//!
//! 功能模块在库 target（[`i18n_rust_lsp`]）中实现，本文件仅做
//! 命令行解析与启动；模块清单与职责见 `src/lib.rs`。

use std::path::PathBuf;

use i18n_rust_lsp::{代理, 本地化};

/// 命令行参数
struct 命令行参数类型 {
    /// 语言包目录路径
    语言包路径: PathBuf,
    /// 支持的方言文件扩展名列表（如 `.zh`）
    扩展名列表: Vec<String>,
}

/// 解析逗号分隔的扩展名列表，自动补充 `.` 前缀
///
/// 空项（如 `"zh,,en"`）被忽略。
/// 示例：`"zh, .de,ja"` → `[".zh", ".de", ".ja"]`
fn 解析扩展名(逗号串: &str) -> Vec<String> {
    逗号串
        .split(',')
        .map(str::trim)
        .filter(|片段| !片段.is_empty())
        .map(|片段| {
            if 片段.starts_with('.') {
                片段.to_string()
            } else {
                format!(".{}", 片段)
            }
        })
        .collect()
}

/// 解析命令行参数
fn 解析命令行参数() -> 命令行参数类型 {
    let mut 语言包路径 = PathBuf::from("crates/engine/lang-packs/zh");
    let mut 扩展名列表: Vec<String> = Vec::new();

    let 参数表: Vec<String> = std::env::args().collect();
    let mut 游标 = 1;
    while 游标 < 参数表.len() {
        match 参数表[游标].as_str() {
            "--language-pack" | "-l" => {
                if 游标 + 1 < 参数表.len() {
                    语言包路径 = PathBuf::from(&参数表[游标 + 1]);
                    游标 += 2;
                } else {
                    let 界面实例 = 本地化::界面::创建(&语言包路径);
                    eprintln!("{}", 界面实例.取文("lsp_err_lang_pack"));
                    std::process::exit(1);
                }
            }
            "--extensions" | "-e" => {
                if 游标 + 1 < 参数表.len() {
                    扩展名列表 = 解析扩展名(&参数表[游标 + 1]);
                    游标 += 2;
                } else {
                    let 界面实例 = 本地化::界面::创建(&语言包路径);
                    eprintln!("{}", 界面实例.取文("lsp_err_extensions"));
                    std::process::exit(1);
                }
            }
            "--help" | "-h" => {
                let 界面实例 = 本地化::界面::创建(&语言包路径);
                打印帮助(&界面实例);
                std::process::exit(0);
            }
            // 输出版本号（纯版本号，供 rzc install 版本校验解析）
            "--version" | "-V" => {
                println!("{}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            // VSCode LanguageClient 会传递 --stdio 参数，我们默认就使用 stdio，直接忽略
            "--stdio" => {
                游标 += 1;
            }
            _ => {
                let 界面实例 = 本地化::界面::创建(&语言包路径);
                eprintln!("{}", 界面实例.取文带参("lsp_unknown_arg", &[&参数表[游标]]));
                eprintln!("{}", 界面实例.取文("lsp_use_help"));
                std::process::exit(1);
            }
        }
    }

    命令行参数类型 {
        语言包路径,
        扩展名列表,
    }
}

/// 打印帮助文本（提示语随语言包/系统语言本地化）
fn 打印帮助(界面实例: &本地化::界面) {
    println!("{}", 界面实例.取文("lsp_about"));
    println!();
    println!("{}", 界面实例.取文("lsp_usage"));
    println!();
    println!("{}", 界面实例.取文("lsp_options"));
    println!("{}", 界面实例.取文("lsp_lang_pack_opt"));
    println!("{}", 界面实例.取文("lsp_extensions_opt"));
    println!("{}", 界面实例.取文("lsp_version_opt"));
    println!("{}", 界面实例.取文("lsp_help_opt"));
}

/// 当默认语言包路径不存在时，自动搜索常见位置
///
/// 搜索顺序：
/// 1. 二进制所在目录向上搜索（最多 5 级）
/// 2. 当前工作目录向上搜索
/// 3. $HOME 下常见项目目录（code/zrRust、zrRust）
fn 查找语言包回退(默认目录: &std::path::Path, 语言码: &str) -> PathBuf {
    if 默认目录.exists() {
        return 默认目录.to_path_buf();
    }
    log::warn!("默认语言包路径 {} 不存在，正在搜索...", 默认目录.display());
    // 1. 二进制所在目录向上搜索
    if let Ok(可执行路径) = std::env::current_exe()
        && let Some(命中路径) = 向上搜索(&可执行路径, 语言码)
    {
        log::info!("在二进制目录找到语言包: {}", 命中路径.display());
        return 命中路径;
    }
    // 2. 当前工作目录向上搜索
    if let Ok(当前目录) = std::env::current_dir()
        && let Some(命中路径) = 向上搜索(&当前目录, 语言码)
    {
        log::info!("在工作目录找到语言包: {}", 命中路径.display());
        return 命中路径;
    }
    // 3. $HOME 下常见项目目录（两种布局：主仓库单副本与用户项目约定）
    if let Ok(主目录) = std::env::var("HOME") {
        for 项目名 in &["code/zrRust", "zrRust"] {
            for 布局名 in &["crates/engine/lang-packs", "lang-packs"] {
                let 候选路径 = PathBuf::from(&主目录)
                    .join(项目名)
                    .join(布局名)
                    .join(语言码);
                if 候选路径.exists() {
                    log::info!("在 HOME 目录找到语言包: {}", 候选路径.display());
                    return 候选路径;
                }
            }
        }
    }
    log::warn!("未找到语言包目录，使用内置映射");
    默认目录.to_path_buf()
}

/// 从指定路径向上搜索语言包目录（最多 5 级；两种布局均探测：
/// crates/engine/lang-packs/<码> 主仓库单副本与 lang-packs/<码> 用户项目约定）
fn 向上搜索(起始路径: &std::path::Path, 语言码: &str) -> Option<PathBuf> {
    let mut 上溯目录 = 起始路径.parent()?.to_path_buf();
    for _ in 0..5 {
        for 布局名 in &["crates/engine/lang-packs", "lang-packs"] {
            let 候选路径 = 上溯目录.join(布局名).join(语言码);
            if 候选路径.exists() {
                return Some(候选路径);
            }
        }
        if !上溯目录.pop() {
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
    let mut 参数表 = 解析命令行参数();

    // 如果语言包路径不存在，自动搜索（仅在未显式指定时）
    let 显式指定 = std::env::args().any(|参数项| 参数项 == "--language-pack" || 参数项 == "-l");
    if !显式指定 && !参数表.语言包路径.exists() {
        // 从默认路径推断语言代码
        let 语言码 = 参数表
            .语言包路径
            .file_name()
            .and_then(|片段| 片段.to_str())
            .unwrap_or("zh");
        参数表.语言包路径 = 查找语言包回退(&参数表.语言包路径, 语言码);
    }

    // 初始化全局界面消息（随语言包/系统语言变化）
    本地化::初始化全局(&参数表.语言包路径);
    log::info!("{}", 本地化::全局().取文("lsp_log_start"));
    log::info!(
        "{}",
        本地化::全局().取文带参(
            "lsp_log_lang_pack",
            &[&参数表.语言包路径.display().to_string()]
        )
    );

    // 创建代理服务器
    let (服务器实例, 输入线程) =
        代理::代理服务器::装配实例(&参数表.语言包路径, &参数表.扩展名列表)?;

    // 运行服务器（阻塞直到退出）
    服务器实例.服务循环(输入线程)?;

    log::info!("{}", 本地化::全局().取文("lsp_log_exit"));
    Ok(())
}

#[cfg(test)]
mod 单元测试 {
    use super::{向上搜索, 查找语言包回退, 解析扩展名};

    /// 带点与不带点的扩展名统一补点
    #[test]
    fn 测试解析扩展名统一补点前缀() {
        assert_eq!(解析扩展名(".zh,.de,.ja"), vec![".zh", ".de", ".ja"]);
        assert_eq!(解析扩展名("zh, de,ja"), vec![".zh", ".de", ".ja"]);
    }

    /// 空列表与单个扩展名
    #[test]
    fn 测试解析扩展名边界情形() {
        assert!(解析扩展名("").is_empty());
        assert!(解析扩展名("  , ").is_empty());
        assert_eq!(解析扩展名("zh"), vec![".zh"]);
        assert_eq!(解析扩展名("zh,,de"), vec![".zh", ".de"]);
    }

    /// 向上搜索：命中主仓库布局与平铺布局；超过 5 级的祖先命中不到
    #[test]
    fn 测试向上搜索布局与深度上限() {
        let 临时目录 = tempfile::tempdir().unwrap();

        // 布局一：crates/engine/lang-packs/<码>
        let 嵌套布局 = 临时目录.path().join("repo-nested");
        let 语言包目录 = 嵌套布局.join("crates/engine/lang-packs/zh");
        std::fs::create_dir_all(&语言包目录).unwrap();
        let 起始路径 = 嵌套布局.join("target/debug/deps/bin");
        assert_eq!(
            向上搜索(&起始路径, "zh").as_deref(),
            Some(语言包目录.as_path())
        );

        // 布局二：平铺 lang-packs/<码>
        let 平铺布局 = 临时目录.path().join("repo-flat");
        let 语言包目录二 = 平铺布局.join("lang-packs/ja");
        std::fs::create_dir_all(&语言包目录二).unwrap();
        let 起始路径二 = 平铺布局.join("a/b/bin");
        assert_eq!(
            向上搜索(&起始路径二, "ja").as_deref(),
            Some(语言包目录二.as_path())
        );

        // 语言码不存在 → None（不误中其他语言目录）
        assert!(向上搜索(&起始路径, "xx").is_none());

        // 包目录在 6 级祖先之外：向上最多 5 级，找不到
        let 过深目录 = 临时目录.path().join("d/l1/l2/l3/l4/l5/l6");
        std::fs::create_dir_all(&过深目录).unwrap();
        let 语言包目录三 = 临时目录.path().join("d/lang-packs/zh");
        std::fs::create_dir_all(&语言包目录三).unwrap();
        assert!(向上搜索(&过深目录.join("bin"), "zh").is_none());
    }

    /// 默认路径已存在时直接采用，不触发任何搜索（显式 --language-pack 路径）
    #[test]
    fn 测试查找语言包回退已存在短路() {
        let 临时目录 = tempfile::tempdir().unwrap();
        let 默认目录 = 临时目录.path().join("explicit/zh");
        std::fs::create_dir_all(&默认目录).unwrap();
        assert_eq!(查找语言包回退(&默认目录, "zh"), 默认目录);
    }
}
