//! 工具链定位模块【zh 自举·批次1】
//! 本文件是源真相：由引导 rzc 转译为 工具链.rs 后交 cargo 编译。
//! 再生成方式见 tools/zh-selfhost/regen.sh；请勿手改同目录下的 工具链.rs。
//!
//! 内置工具链：`rzc install toolchain` 将官方 standalone 工具链
//!（rustc/cargo/rust-analyzer）安装到 `~/.rz/toolchain/bin`，
//! 使发布包自包含、不依赖 rustup 与 PATH 配置。
//! 查找优先级：内置目录 → 环境变量 → PATH 扫描。
//!
//! 注：`切分路径`、`组件`、`作为系统串`、`转有损字符串`、
//! `有值且`、`平台常量::可执行后缀` 暂无中文词条，按英文透传（合法
//! 方言行为）；目录名 `".rz"`、`"toolchain"` 等是与磁盘布局对齐的字面量，
//! 属数据而非代码，保持原文。

use std::path::PathBuf;

/// 全局安装目录（~/.rz）：语言包（lang-packs/）与工具链（toolchain/）共用
pub fn 安装根目录() -> PathBuf {
    // `或用否则` / `解包或用否则` 无中文词条，按英文透传
    let 家目录 = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(家目录).join(".rz")
}

/// 内置工具链 bin 目录（~/.rz/toolchain/bin）
pub fn 工具链程序目录() -> PathBuf {
    安装根目录().join("toolchain").join("bin")
}

/// 查找工具链可执行文件的绝对路径
///
/// 优先级：
/// 1. 内置工具链目录（`rzc install toolchain` 安装的 standalone 版本）
/// 2. 同名环境变量（如 rust-analyzer 对应 RUST_ANALYZER_PATH）
/// 3. PATH 扫描（跨平台，Windows 追加 PATHEXT 后缀）
pub fn 查找工具链程序(名称: &str) -> Option<PathBuf> {
    let 程序名 = format!("{}{}", 名称, std::env::consts::EXE_SUFFIX);

    // 1. 内置工具链
    let 内置 = 工具链程序目录().join(&程序名);
    if 内置.is_file() {
        return Some(内置);
    }

    // 2. 环境变量（RUST_ANALYZER_PATH / RUSTC_PATH / CARGO_PATH 等）
    let 环境键 = format!("{}_PATH", 名称.to_uppercase().replace("-", "_"));
    if let Ok(路径) = std::env::var(环境键) {
        let 候选 = PathBuf::from(&路径);
        if 候选.is_file() {
            return Some(候选);
        }
    }

    // 3. PATH 扫描
    扫描路径(&程序名)
}

/// 在 PATH 中查找可执行文件（含 Windows PATHEXT 后缀探测）
fn 扫描路径(程序名: &str) -> Option<PathBuf> {
    let 路径变量 = std::env::var("PATH").ok()?;
    for 目录 in std::env::split_paths(&路径变量) {
        let 候选 = 目录.join(程序名);
        if 候选.is_file() {
            return Some(候选);
        }
        // Windows：无后缀名时按 PATHEXT 顺序探测（.EXE/.CMD/.BAT…）
        if std::env::consts::OS == "windows" {
            for 后缀 in ["exe", "cmd", "bat", "com"] {
                let 带后缀 = 目录.join(format!("{}.{}", 程序名, 后缀));
                if 带后缀.is_file() {
                    return Some(带后缀);
                }
            }
        }
    }
    None
}

/// 已安装的内置工具链版本目录名（如 `1.98.0`）；未安装返回 无
pub fn 已安装工具链版本() -> Option<String> {
    let 目录 = 安装根目录().join("toolchain").join("version.txt");
    std::fs::read_to_string(目录)
        .ok()
        .map(|str| str.trim().to_string())
        .filter(|str| !str.is_empty())
}

/// 当前锁定版本（与仓库 rust-toolchain.toml 对齐；后续随官方 stable 升级）
pub const 锁定工具链版本: &str = "1.98.0";

#[cfg(test)]
mod 单元测试 {
    use super::*;

    #[test]
    fn 工具链目录布局() {
        // 目录结构约定：~/.rz/toolchain/bin（组件级检查）
        let 目录 = 工具链程序目录();
        let 组件列表: Vec<String> = 目录
            .components()
            .map(|字符值| 字符值.as_os_str().to_string_lossy().to_string())
            .collect();
        assert!(组件列表.iter().any(|字符值| 字符值 == ".rz"));
        assert!(组件列表.iter().any(|字符值| 字符值 == "toolchain"));
        assert!(组件列表.last().is_some_and(|字符值| 字符值 == "bin"));
    }

    #[test]
    fn 查找缺失程序返回无() {
        // 不存在的二进制名应返回 无（内置与 PATH 都无）
        let 名称 = format!("__rzc_nonexistent__{}", std::process::id());
        assert!(查找工具链程序(&名称).is_none());
    }
}
