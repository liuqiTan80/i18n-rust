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
    let status = Command::new(crate::resolve_cargo())
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

// ============================================================
// 内置工具链（standalone rustc/cargo/rust-analyzer）
// ============================================================

/// rust-analyzer 官方 Release tag（随官方发布升级；可用 --ra-tag 覆盖）
pub const RA_RELEASE_TAG: &str = "2026-08-24";

/// 当前平台的目标三元组（官方 dist 与 rust-analyzer Release 资产名用）
fn target_triple() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        (os, arch) => {
            panic!("不支持的平台 {os}/{arch}（内置工具链暂未覆盖）")
        }
    }
}

/// 安装内置工具链（standalone rustc/cargo/rust-analyzer）到 ~/.rz/toolchain
///
/// rustc/cargo 来自官方 dist（static.rust-lang.org，含全套组件）；
/// rust-analyzer 来自官方 GitHub Release（下载失败不阻塞——可稍后补装）。
/// 安装后 rzc 与 LSP 优先使用内置工具链，不再依赖 rustup 与 PATH 配置。
pub fn install_toolchain(ui: &Ui, version: &str, ra_tag: &str, force: bool) -> anyhow::Result<()> {
    use i18n_rust_engine::toolchain::{rz_home, toolchain_bin_dir};

    let bin_dir = toolchain_bin_dir();
    let rustc_exe = bin_dir.join(format!("rustc{EXE_SUFFIX}"));
    if rustc_exe.is_file() && !force {
        println!(
            "{}",
            ui.f("tc_install_already", &[&bin_dir.display().to_string()])
        );
        return Ok(());
    }

    let triple = target_triple();
    let tmp = tempfile::tempdir()?;

    // 1. rustc/cargo standalone（官方 dist 单包含全部组件）
    let url = format!("https://static.rust-lang.org/dist/rust-{version}-{triple}.tar.gz");
    println!("{}", ui.f("tc_download_rustc", &[&url]));
    let archive = tmp.path().join("rust.tar.gz");
    download_to(&url, &archive)?;
    // 校验官方 SHA-256（官方 dist 提供 .sha256 文件；不匹配视为下载被篡改）
    let sha_url = format!("{url}.sha256");
    let expected = download_text(&sha_url)?
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    let actual = sha256_file(&archive)?;
    if expected.len() == 64 && actual != expected {
        anyhow::bail!("rustc 包 SHA-256 校验失败（下载可能被篡改，请重试）");
    }
    println!("{}", ui.t("tc_extracting"));
    extract_tar_gz(&archive, tmp.path())?;
    let root = tmp.path().join(format!("rust-{version}-{triple}"));
    std::fs::create_dir_all(&bin_dir)?;
    copy_bin_dir(&root.join("rustc").join("bin"), &bin_dir)?;
    copy_bin_dir(&root.join("cargo").join("bin"), &bin_dir)?;

    // 2. rust-analyzer（官方 GitHub Release；失败不阻塞主流程）
    let ra_name = format!(
        "rust-analyzer-{triple}{}",
        if std::env::consts::OS == "windows" {
            ".exe"
        } else {
            ""
        }
    );
    let ra_url =
        format!("https://github.com/rust-lang/rust-analyzer/releases/download/{ra_tag}/{ra_name}");
    let ra_dest = bin_dir.join(format!("rust-analyzer{EXE_SUFFIX}"));
    match download_to(&ra_url, &ra_dest) {
        Ok(()) => println!("{}", ui.f("tc_ra_installed", &[ra_tag])),
        Err(e) => println!("{}", ui.f("tc_ra_skipped", &[&e.to_string()])),
    }

    // 3. 记录版本
    let toolchain_dir = rz_home().join("toolchain");
    std::fs::create_dir_all(&toolchain_dir)?;
    std::fs::write(toolchain_dir.join("version.txt"), version)?;
    println!(
        "{}",
        ui.f("tc_install_done", &[&bin_dir.display().to_string()])
    );
    Ok(())
}

/// ureq 流式下载到文件（大文件不驻留内存）
fn download_to(url: &str, dest: &Path) -> anyhow::Result<()> {
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(600))
        .call()
        .map_err(|e| anyhow::anyhow!("下载失败: {e}"))?;
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(dest)?;
    std::io::copy(&mut reader, &mut file)?;
    Ok(())
}

/// ureq 下载文本内容（如官方 .sha256 校验文件）
fn download_text(url: &str) -> anyhow::Result<String> {
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|e| anyhow::anyhow!("下载失败: {e}"))?;
    let mut text = String::new();
    resp.into_reader()
        .read_to_string(&mut text)
        .map_err(|e| anyhow::anyhow!("读取失败: {e}"))?;
    Ok(text)
}

/// 计算文件 SHA-256（十六进制小写）
fn sha256_file(path: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut file = std::fs::File::open(path)?;
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// 解压 tar.gz 到目标目录（tar crate 默认拒绝 `..` 与绝对路径，路径安全）
fn extract_tar_gz(archive: &Path, dest: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::open(archive)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);
    tar.unpack(dest)?;
    Ok(())
}

/// 复制目录内全部文件到目标目录（跳过已存在，避免覆盖冲突）
fn copy_bin_dir(from: &Path, to: &Path) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if dest.is_file() {
            continue;
        }
        std::fs::copy(entry.path(), &dest)?;
    }
    Ok(())
}

/// 环境诊断：rzc / 内置工具链 / PATH 工具链 / 版本对比
pub fn doctor() -> anyhow::Result<()> {
    use i18n_rust_engine::toolchain::{
        LOCKED_TOOLCHAIN_VERSION, installed_toolchain_version, toolchain_bin_dir,
    };

    println!("=== rzc doctor ===");
    println!("rzc: {}", env!("CARGO_PKG_VERSION"));

    let bin_dir = toolchain_bin_dir();
    let builtin_version = installed_toolchain_version();
    match &builtin_version {
        Some(v) => println!("内置工具链: {}（{}）", v, bin_dir.display()),
        None => {
            println!("内置工具链: 未安装（可执行 rzc install toolchain 一键安装，脱离 rustup）")
        }
    }

    for line in component_status_lines(&bin_dir) {
        println!("{line}");
    }

    if let Some(v) = builtin_version.as_deref()
        && v != LOCKED_TOOLCHAIN_VERSION
    {
        println!(
            "提示: 内置工具链 {} 与锁定版本 {} 不一致（rzc install toolchain --force 更新）",
            v, LOCKED_TOOLCHAIN_VERSION
        );
    }
    println!(
        "升级内置工具链：rzc install toolchain --version <新版本> --force（当前锁定 {}）",
        LOCKED_TOOLCHAIN_VERSION
    );
    Ok(())
}

/// 各组件来源状态行（纯函数，便于测试）：`rustc: 内置（路径）` / `cargo: PATH（路径）` 等
fn component_status_lines(bin_dir: &std::path::Path) -> Vec<String> {
    use i18n_rust_engine::toolchain::find_toolchain_bin;

    ["rustc", "cargo", "rust-analyzer"]
        .iter()
        .map(|name| {
            let builtin = bin_dir.join(format!("{name}{EXE_SUFFIX}"));
            let via = if builtin.is_file() {
                format!("内置（{}）", builtin.display())
            } else {
                match find_toolchain_bin(name) {
                    Some(p) => format!("PATH（{}）", p.display()),
                    None => "未找到".to_string(),
                }
            };
            format!("{name}: {via}")
        })
        .collect()
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

    #[test]
    fn test_target_triple_supported() {
        // 当前平台必须能被识别（不 panic）
        let triple = target_triple();
        assert!(!triple.is_empty());
    }

    /// SHA-256 计算正确性（空文件与已知内容）
    #[test]
    fn test_sha256_file_known_content() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.txt");
        std::fs::write(&f, b"hello").unwrap();
        // `hello` 的 SHA-256（标准已知值）
        assert_eq!(
            sha256_file(&f).unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    /// 内置工具链存在时状态行标记为内置
    #[test]
    fn test_component_status_lines_builtin() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path();
        std::fs::write(bin.join(format!("rustc{EXE_SUFFIX}")), b"x").unwrap();
        let lines = component_status_lines(bin);
        assert!(lines.iter().any(|l| l.starts_with("rustc: 内置")));
        // 其余组件未内置时标记 PATH 或未找到（不 panic）
        assert!(lines.iter().any(|l| l.starts_with("cargo:")));
        assert!(lines.iter().any(|l| l.starts_with("rust-analyzer:")));
    }
}

/// 双击 rzc.exe（或终端无参数运行）时的环境安装向导：
/// 逐项检查组件状态，标注「已就绪 ✓」或「缺失 ✗ + 解决命令」。
pub fn show_setup_wizard() {
    use i18n_rust_engine::toolchain::{find_toolchain_bin, toolchain_bin_dir};
    let exe = std::env::consts::EXE_SUFFIX;

    println!("════════════ i18n-rust 环境安装向导 ════════════");
    println!("rzc 本体          v{}（已就绪）", env!("CARGO_PKG_VERSION"));
    println!();

    // 1. 内置工具链（rustc / cargo / rust-analyzer）
    println!("【第 1 步】编译器与工具链（rzc install toolchain 一键安装）");
    for (name, install_cmd) in [
        ("rustc", "rzc install toolchain"),
        ("cargo", "rzc install toolchain"),
        ("rust-analyzer", "rzc install toolchain --force"),
    ] {
        let builtin = toolchain_bin_dir().join(format!("{name}{exe}"));
        let (mark, detail) = if builtin.is_file() {
            ("✓ 已就绪", format!("内置（{}）", builtin.display()))
        } else if let Some(p) = find_toolchain_bin(name) {
            ("✓ 已就绪", format!("PATH（{}）", p.display()))
        } else {
            (
                "✗ 缺失",
                format!("未找到 → 运行: {install_cmd}（需联网，约 300MB）"),
            )
        };
        println!("  [{mark}] {name:<14} {detail}");
    }
    println!();

    // 2. 语言服务器（VS Code 补全/诊断后端）
    println!("【第 2 步】语言服务器 i18n-rust-lsp（rzc install lsp 一键安装）");
    if let Some(p) = find_toolchain_bin("i18n-rust-lsp") {
        println!("  [✓ 已就绪] {}", p.display());
        if let Ok(output) = std::process::Command::new(&p).arg("--version").output() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !version.is_empty() {
                println!("              版本 {version}");
            }
        }
    } else {
        println!("  [✗ 缺失] 未找到 i18n-rust-lsp{exe} → 运行: rzc install lsp");
    }
    println!();

    // 3. VS Code 扩展
    println!("【第 3 步】VS Code 扩展（.vsix 手动安装，未发布到扩展商城）");
    println!("  [? 手动] 从网盘或 GitHub Releases 下载 i18n-rust-<版本>.vsix");
    println!("           → VS Code 命令面板（Ctrl+Shift+P）→ Install from VSIX 选择该文件");
    println!();

    // 4. 环境变量（可选）
    println!("【第 4 步】环境变量（可选，一般无需配置）");
    println!("  RZ_LANG_DIR        指定语言包目录（默认内置）");
    println!("  RUST_ANALYZER_PATH 指定 rust-analyzer 路径（自动检测失败时）");
    println!("  VS Code 设置 i18n-rust.serverPath 可指定语言服务器路径");
    println!();

    // 常用命令
    println!("────────── 常用命令速查 ──────────");
    println!("  rzc init 我的项目        创建新项目");
    println!("  rzc run src/main.zh      运行中文代码");
    println!("  rzc check src/main.zh    类型检查（中文错误提示）");
    println!("  rzc install toolchain    一键安装内置工具链");
    println!("  rzc install lsp          安装语言服务器");
    println!("  rzc doctor               查看工具链环境状态");
    println!("  rzc --help               查看全部命令");
    println!();
    println!("详细教程见《开篇：这本书怎么用》与《第一章：用好 VS Code 扩展》。");
}
