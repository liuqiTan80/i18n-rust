// 配套组件安装模块
//
// `rzc install` 安装 rzc 所需的配套组件。当前组件为语言服务器
// i18n-rust-lsp（VS Code 扩展的补全/诊断后端），与 rzc 属于不同
// crate，cargo install rzc 不会自动带上，需单独安装。
//
// 安装来源优先级：
// 1. 与 rzc 同目录的二进制（离线发布包自带，免网络直接复制到 cargo bin）
// 2. crates.io（cargo install i18n-rust-lsp，版本与 rzc 严格一致，
//    保证协议与语言包版本兼容）

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ui::Ui;

/// 语言服务器二进制名（对应 crates/lsp 的包名）
const LSP_BIN: &str = "i18n-rust-lsp";

/// 当前平台可执行文件后缀（Windows 为 .exe，其余为空）
const EXE_SUFFIX: &str = std::env::consts::EXE_SUFFIX;

/// cargo 二进制目录：$CARGO_HOME/bin，未设置时回退 ~/.cargo/bin
fn cargo_bin_dir() -> PathBuf {
    if let Ok(home) = std::env::var("CARGO_HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home).join("bin");
    }
    // HOME（Unix）优先，USERPROFILE（Windows）回退，保证跨平台目录正确
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".cargo").join("bin")
}

/// 安装语言服务器 i18n-rust-lsp
///
/// 优先使用与 rzc 同目录的二进制（离线包场景）；否则通过
/// `cargo install i18n-rust-lsp --version =<当前版本>` 从 crates.io 安装，
/// 保证与 rzc 版本严格一致。
///
/// 已安装时先校验版本：与 rzc 一致则提示已安装；不一致或无法确认时
/// 引导用户执行 `--force` 重装（不自动覆盖，避免打断正在使用的 LSP 进程）。
pub fn install_lsp(ui: &Ui, force: bool) -> anyhow::Result<()> {
    let target = cargo_bin_dir().join(format!("{LSP_BIN}{EXE_SUFFIX}"));
    if target.is_file() && !force {
        match check_lsp_version(installed_lsp_version(&target), env!("CARGO_PKG_VERSION")) {
            VersionCheck::Match => {
                println!(
                    "{}",
                    ui.f("lsp_install_already", &[&target.display().to_string()])
                );
            }
            VersionCheck::Mismatch(installed) => {
                println!(
                    "{}",
                    ui.f(
                        "lsp_install_version_mismatch",
                        &[&installed, env!("CARGO_PKG_VERSION")]
                    )
                );
            }
            VersionCheck::Unknown => {
                println!(
                    "{}",
                    ui.f(
                        "lsp_install_version_unknown",
                        &[&target.display().to_string()]
                    )
                );
            }
        }
        return Ok(());
    }

    // 1. 离线发布包：与 rzc 同目录附带同名二进制，直接复制（无需网络）
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let local = dir.join(format!("{LSP_BIN}{EXE_SUFFIX}"));
        if local.is_file() {
            install_from_local(ui, &local, &target)?;
            return Ok(());
        }
    }

    // 2. crates.io：cargo install（版本与 rzc 精确一致）
    install_from_crates(ui, &target)?;
    Ok(())
}

/// 从本地路径复制到 cargo bin 目录
fn install_from_local(ui: &Ui, local: &Path, target: &Path) -> anyhow::Result<()> {
    let bin_dir = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{}", ui.f("lsp_install_failed", &["无有效安装目录"])))?;
    std::fs::create_dir_all(bin_dir)
        .map_err(|e| anyhow::anyhow!("{}", ui.f("lsp_install_failed", &[&e.to_string()])))?;
    std::fs::copy(local, target)
        .map_err(|e| anyhow::anyhow!("{}", ui.f("lsp_install_failed", &[&e.to_string()])))?;
    println!(
        "{}",
        ui.f("lsp_install_local", &[&target.display().to_string()])
    );
    Ok(())
}

/// 通过 cargo install 从 crates.io 安装
fn install_from_crates(ui: &Ui, target: &Path) -> anyhow::Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    println!("{}", ui.f("lsp_install_cargo", &[version]));
    let status = Command::new("cargo")
        .arg("install")
        .arg(LSP_BIN)
        .arg("--version")
        .arg(format!("={version}"))
        .status()
        .map_err(|e| anyhow::anyhow!("{}", ui.f("lsp_install_no_cargo", &[&e.to_string()])))?;
    if !status.success() {
        anyhow::bail!("{}", ui.f("lsp_install_failed", &[&status.to_string()]));
    }
    // cargo install 默认输出到 $CARGO_HOME/bin，与目标路径一致
    println!("{}", ui.f("lsp_install_done", &[version]));
    println!(
        "{}",
        ui.f("lsp_install_hint", &[&target.display().to_string()])
    );
    Ok(())
}

/// 已安装版本与期望版本的比较结果
#[derive(Debug, PartialEq, Eq)]
enum VersionCheck {
    /// 版本一致
    Match,
    /// 版本不一致（附已安装版本号）
    Mismatch(String),
    /// 无法确定已安装版本
    Unknown,
}

/// 比较已安装语言服务器版本与期望版本（纯函数，便于测试）
fn check_lsp_version(installed: Option<String>, expected: &str) -> VersionCheck {
    match installed {
        Some(version) if version == expected => VersionCheck::Match,
        Some(version) => VersionCheck::Mismatch(version),
        None => VersionCheck::Unknown,
    }
}

/// 读取已安装语言服务器的版本号（`i18n-rust-lsp --version` 输出纯版本号）
///
/// 二进制不存在、执行失败（如被占用）或输出为空时返回 None。
fn installed_lsp_version(target: &Path) -> Option<String> {
    let output = Command::new(target).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_lsp_version_match() {
        assert_eq!(
            check_lsp_version(Some("0.5.5".to_string()), "0.5.5"),
            VersionCheck::Match
        );
    }

    #[test]
    fn test_check_lsp_version_mismatch() {
        assert_eq!(
            check_lsp_version(Some("0.5.3".to_string()), "0.5.5"),
            VersionCheck::Mismatch("0.5.3".to_string())
        );
    }

    #[test]
    fn test_check_lsp_version_unknown() {
        assert_eq!(check_lsp_version(None, "0.5.5"), VersionCheck::Unknown);
    }
}
