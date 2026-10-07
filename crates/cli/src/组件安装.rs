//! 配套组件安装模块
//!
//! `rzc install` 安装 rzc 所需的配套组件。当前组件为语言服务器
//! i18n-rust-lsp（编辑器的补全/诊断后端），与 rzc 属于不同
//! crate，cargo install rzc 不会自动带上，需单独安装。
//!
//! 安装来源优先级：
//! 1. 与 rzc 同目录的二进制（离线发布包自带，免网络直接复制到 cargo bin）
//! 2. crates.io（cargo install i18n-rust-lsp，版本与 rzc 严格一致，
//!    保证协议与语言包版本兼容）
//!
//! 注：外部 std/三方 ABI 走英文透传——crate 根 `crate::解析cargo路径`（`main.zh` 已翻转）、
//! `std`/`tempfile`/`flate2`/`zip`/`tar`/`sha2`/`ureq`/`serde_json` 及其方法链与枚举构造；
//! 已翻转中文 ABI：`crate::本地化::界面::全局/取文/取文带参`（类型 `crate::本地化::界面`）、
//! `i18n_rust_engine::工具链::{...}`、`crate::内置语言::拥有内置语言`、
//! `crate::语言包工具::{全局语言包, 语言元数据, 取内置元数据, 全局语言目录,
//! 扫描全局包, 全局包子目录名, 静态扩展名映射}` 及其字段
//! `目录名/包元数据/语言名称/文件扩展名/版本号` 为已翻转的中文 ABI。
//! 所有面向用户的 `println!`/`bail!`/`format!` 显示文案（含 `⚠ ✓ ✗`、中文提示、命令速查、
//! 路径与 URL）逐字保真；`format!` 内联占位符随变量改名而输出文本不变。

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::本地化::界面;

/// 语言服务器二进制名（对应 crates/lsp 的包名）
const 语言服务器二进制名: &str = "i18n-rust-lsp";

/// 当前平台可执行文件后缀（Windows 为 .exe，其余为空）
const 可执行后缀: &str = std::env::consts::EXE_SUFFIX;

/// cargo 二进制目录：$CARGO_HOME/bin，未设置时回退 ~/.cargo/bin
fn cargo程序目录() -> PathBuf {
    if let Ok(主目录) = std::env::var("CARGO_HOME")
        && !主目录.is_empty()
    {
        return PathBuf::from(主目录).join("bin");
    }
    // HOME（Unix）优先，USERPROFILE（Windows）回退，保证跨平台目录正确
    let 主目录 = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(主目录).join(".cargo").join("bin")
}

/// 安装语言服务器 i18n-rust-lsp
///
/// 优先使用与 rzc 同目录的二进制（离线包场景）；否则通过
/// `cargo install i18n-rust-lsp --version =<当前版本>` 从 crates.io 安装，
/// 保证与 rzc 版本严格一致。
///
/// 已安装时先校验版本：与 rzc 一致则提示已安装；不一致或无法确认时
/// 引导用户执行 `--force` 重装（不自动覆盖，避免打断正在使用的 LSP 进程）。
pub fn 安装语言服务器(界面: &界面, 强制: bool) -> anyhow::Result<()> {
    let 目标路径 = cargo程序目录().join(format!("{语言服务器二进制名}{可执行后缀}"));
    if 目标路径.is_file() && !强制 {
        match 校验语言服务器版本(
            已安装语言服务器版本(&目标路径),
            env!("CARGO_PKG_VERSION"),
        ) {
            版本校验::相符 => {
                println!(
                    "{}",
                    界面.取文带参("lsp_install_already", &[&目标路径.display().to_string()])
                );
            }
            版本校验::不符(已装) => {
                println!(
                    "{}",
                    界面.取文带参(
                        "lsp_install_version_mismatch",
                        &[&已装, env!("CARGO_PKG_VERSION")]
                    )
                );
            }
            版本校验::未知 => {
                println!(
                    "{}",
                    界面.取文带参(
                        "lsp_install_version_unknown",
                        &[&目标路径.display().to_string()]
                    )
                );
            }
        }
        return Ok(());
    }

    // 1. 离线发布包：与 rzc 同目录附带同名二进制，直接复制（无需网络）
    if let Ok(自身程序) = std::env::current_exe()
        && let Some(所在目录) = 自身程序.parent()
    {
        let 本地二进制 = 所在目录.join(format!("{语言服务器二进制名}{可执行后缀}"));
        if 本地二进制.is_file() {
            从本地安装(界面, &本地二进制, &目标路径)?;
            return Ok(());
        }
    }

    // 2. crates.io：cargo install（版本与 rzc 精确一致）
    从crates安装(界面, &目标路径)?;
    Ok(())
}

/// 从本地路径复制到 cargo bin 目录
fn 从本地安装(
    界面: &界面, 本地二进制: &Path, 目标路径: &Path
) -> anyhow::Result<()> {
    let 安装目录 = 目标路径.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "{}",
            界面.取文带参("lsp_install_failed", &["无有效安装目录"])
        )
    })?;
    std::fs::create_dir_all(安装目录).map_err(|错| {
        anyhow::anyhow!(
            "{}",
            界面.取文带参("lsp_install_failed", &[&错.to_string()])
        )
    })?;
    std::fs::copy(本地二进制, 目标路径).map_err(|错| {
        anyhow::anyhow!(
            "{}",
            界面.取文带参("lsp_install_failed", &[&错.to_string()])
        )
    })?;
    println!(
        "{}",
        界面.取文带参("lsp_install_local", &[&目标路径.display().to_string()])
    );
    Ok(())
}

/// 通过 cargo install 从 crates.io 安装
fn 从crates安装(界面: &界面, 目标路径: &Path) -> anyhow::Result<()> {
    let 目标版本 = env!("CARGO_PKG_VERSION");
    println!("{}", 界面.取文带参("lsp_install_cargo", &[目标版本]));
    let 退出状态 = Command::new(crate::解析cargo路径())
        .arg("install")
        .arg(语言服务器二进制名)
        .arg("--version")
        .arg(format!("={目标版本}"))
        .status()
        .map_err(|错| {
            anyhow::anyhow!(
                "{}",
                界面.取文带参("lsp_install_no_cargo", &[&错.to_string()])
            )
        })?;
    if !退出状态.success() {
        anyhow::bail!(
            "{}",
            界面.取文带参("lsp_install_failed", &[&退出状态.to_string()])
        );
    }
    // cargo install 默认输出到 $CARGO_HOME/bin，与目标路径一致
    println!("{}", 界面.取文带参("lsp_install_done", &[目标版本]));
    println!(
        "{}",
        界面.取文带参("lsp_install_hint", &[&目标路径.display().to_string()])
    );
    Ok(())
}

/// 已安装版本与期望版本的比较结果
#[derive(Debug, PartialEq, Eq)]
enum 版本校验 {
    /// 版本一致
    相符,
    /// 版本不一致（附已安装版本号）
    不符(String),
    /// 无法确定已安装版本
    未知,
}

/// 比较已安装语言服务器版本与期望版本（纯函数，便于测试）
fn 校验语言服务器版本(已装: Option<String>, 期望: &str) -> 版本校验 {
    match 已装 {
        Some(版本号) if 版本号 == 期望 => 版本校验::相符,
        Some(版本号) => 版本校验::不符(版本号),
        None => 版本校验::未知,
    }
}

/// 读取已安装语言服务器的版本号（`i18n-rust-lsp --version` 输出纯版本号）
///
/// 二进制不存在、执行失败（如被占用）或输出为空时返回 None。
fn 已安装语言服务器版本(目标路径: &Path) -> Option<String> {
    let 命令输出 = Command::new(目标路径).arg("--version").output().ok()?;
    if !命令输出.status.success() {
        return None;
    }
    let 目标版本 = String::from_utf8_lossy(&命令输出.stdout).trim().to_string();
    if 目标版本.is_empty() {
        None
    } else {
        Some(目标版本)
    }
}

// ============================================================
// 内置工具链（standalone rustc/cargo/rust-analyzer）
// ============================================================

/// rust-analyzer 官方 Release tag（随官方发布升级；可用 --ra-tag 覆盖）
pub const 分析器发布标签: &str = "2026-08-24";

/// 当前平台的目标三元组（官方 dist 与 rust-analyzer Release 资产名用）
///
/// 不支持的平台返回错误而非 panic，由调用方以本地化提示终止（避免崩溃式退出）。
fn 计算目标三元组() -> anyhow::Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        (系统, 架构) => anyhow::bail!("不支持的平台 {系统}/{架构}（内置工具链暂未覆盖）"),
    }
}

/// 安装内置工具链（standalone rustc/cargo/rust-analyzer）到 ~/.rz/toolchain
///
/// rustc/cargo 来自官方 dist（static.rust-lang.org，含全套组件）；
/// rust-analyzer 来自官方 GitHub Release（下载失败不阻塞——可稍后补装）。
/// `ra_only` 为 true 时仅升级 rust-analyzer（跳过 rustc/cargo 的 300MB 重下）。
/// 安装后 rzc 与 LSP 优先使用内置工具链，不再依赖 rustup 与 PATH 配置。
pub fn 安装工具链(
    界面: &界面,
    目标版本: &str,
    分析器标签: &str,
    强制: bool,
    仅分析器: bool,
) -> anyhow::Result<()> {
    use i18n_rust_engine::工具链::{安装根目录, 工具链程序目录};

    let 程序目录 = 工具链程序目录();
    if 仅分析器 {
        return 仅升级分析器(界面, 分析器标签, &程序目录);
    }

    let 编译器程序 = 程序目录.join(format!("rustc{可执行后缀}"));
    if 编译器程序.is_file() && !强制 {
        println!(
            "{}",
            界面.取文带参("tc_install_already", &[&程序目录.display().to_string()])
        );
        return Ok(());
    }

    let 三元组 = 计算目标三元组()?;
    let 临时区 = tempfile::tempdir()?;

    // 1. rustc/cargo standalone（官方 dist 单包含全部组件）
    let 下载地址 = format!("https://static.rust-lang.org/dist/rust-{目标版本}-{三元组}.tar.gz");
    println!("{}", 界面.取文带参("tc_download_rustc", &[&下载地址]));
    let 归档文件 = 临时区.path().join("rust.tar.gz");
    下载到文件(&下载地址, &归档文件)?;
    // 校验官方 SHA-256（官方 dist 提供 .sha256 文件）
    // fail-closed：校验和缺失/格式非法时直接中止，绝不静默放行
    let 校验地址 = format!("{下载地址}.sha256");
    let 期望校验 = 下载文本(&校验地址)?
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    let 校验和合法 =
        期望校验.len() == 64 && 期望校验.chars().all(|每字符| 每字符.is_ascii_hexdigit());
    if !校验和合法 {
        anyhow::bail!("rustc 包 SHA-256 校验和不可用（{校验地址} 返回异常内容），已中止下载");
    }
    let 实际校验 = 计算文件校验和(&归档文件)?;
    if 实际校验 != 期望校验 {
        anyhow::bail!("rustc 包 SHA-256 校验失败（下载可能被篡改，请重试）");
    }
    println!("{}", 界面.取文("tc_extracting"));
    解压归档(&归档文件, 临时区.path())?;
    let 解压根 = 临时区.path().join(format!("rust-{目标版本}-{三元组}"));
    std::fs::create_dir_all(&程序目录)?;
    复制程序目录(&解压根.join("rustc").join("bin"), &程序目录)?;
    复制程序目录(&解压根.join("cargo").join("bin"), &程序目录)?;

    // 2. rust-analyzer（官方 Release 的压缩资产：linux/mac 为 .gz、windows 为 .zip；
    //    失败不阻塞主流程，但已下载的二进制必须通过 SHA-256 校验，未经验证绝不落盘）
    match 下载并安装分析器(分析器标签, 三元组, &程序目录, 临时区.path()) {
        Ok(()) => println!("{}", 界面.取文带参("tc_ra_installed", &[分析器标签])),
        Err(错) => println!("{}", 界面.取文带参("tc_ra_skipped", &[&错.to_string()])),
    }

    // 3. 记录版本
    let 工具链目录 = 安装根目录().join("toolchain");
    std::fs::create_dir_all(&工具链目录)?;
    std::fs::write(工具链目录.join("version.txt"), 目标版本)?;
    println!(
        "{}",
        界面.取文带参("tc_install_done", &[&程序目录.display().to_string()])
    );
    Ok(())
}

/// 仅升级 rust-analyzer（跳过 rustc/cargo 的 300MB 重下）
fn 仅升级分析器(
    界面: &界面, 分析器标签: &str, 程序目录: &Path
) -> anyhow::Result<()> {
    // 首次运行时 ~/.rz/toolchain/bin 可能不存在，须先创建（否则解压后 rename 报 ENOENT）
    std::fs::create_dir_all(程序目录)?;
    let 三元组 = 计算目标三元组()?;
    let 临时区 = tempfile::tempdir()?;
    下载并安装分析器(分析器标签, 三元组, 程序目录, 临时区.path())?;
    println!("{}", 界面.取文带参("tc_ra_installed", &[分析器标签]));
    Ok(())
}

/// 下载并安装 rust-analyzer（官方 Release 的压缩资产，带 SHA-256 校验）
///
/// 资产命名：linux/macos 为 `rust-analyzer-<triple>.gz`、windows 为 `.zip`，
/// 官方不发布裸二进制资产（旧代码直接下载裸名 404，从未安装成功）。
/// 完整性校验：官方未发布独立 `.sha256` 文件，digest 由 GitHub API 的
/// assets 列表提供；校验失败（篡改）或 digest 不可用（API 限流）均抛错中止，
/// 不做「未验证降级安装」——后者会让阻断 API 请求成为绕过完整性校验的手段。
fn 下载并安装分析器(
    分析器标签: &str,
    三元组: &str,
    程序目录: &Path,
    临时区: &Path,
) -> anyhow::Result<()> {
    let 是窗口平台 = std::env::consts::OS == "windows";
    let 资产名 = if 是窗口平台 {
        format!("rust-analyzer-{三元组}.zip")
    } else {
        format!("rust-analyzer-{三元组}.gz")
    };
    let 下载地址 = format!(
        "https://github.com/rust-lang/rust-analyzer/releases/download/{分析器标签}/{资产名}"
    );
    let 分析器目标 = 程序目录.join(format!("rust-analyzer{可执行后缀}"));

    // 下载压缩资产
    let 压缩资产 = 临时区.join("rust-analyzer.asset");
    下载到文件(&下载地址, &压缩资产)?;

    // 完整性校验：官方 API 的 digest 字段（无独立 .sha256 文件）。
    // digest 不可用时**中止安装**：跳过校验等于放弃完整性保证，而让 API
    // 不可达（限流、DNS/中间人劫持）比篡改发布资产容易得多，若此时仍安装，
    // 攻击者只需阻断一次 API 请求即可让任意二进制被装入并随后运行。
    let 期望校验 = 取资产校验和(分析器标签, &资产名).ok_or_else(|| {
        anyhow::anyhow!(
            "无法获取 rust-analyzer 官方 SHA-256（资产 {资产名}，标签 {分析器标签}）：\
             GitHub API 可能限流或网络不可达。为避免安装未经验证的二进制已中止，请稍后重试"
        )
    })?;
    let 实际校验 = 计算文件校验和(&压缩资产)?;
    if 实际校验 != 期望校验 {
        let _ = std::fs::remove_file(&压缩资产);
        anyhow::bail!("rust-analyzer 包 SHA-256 校验失败（下载可能被篡改）");
    }

    // 解压到目标目录
    if 是窗口平台 {
        解压压缩条目(&压缩资产, &分析器目标)?;
    } else {
        let 文件句柄 = std::fs::File::open(&压缩资产)?;
        let mut 解码器 = flate2::read::GzDecoder::new(文件句柄);
        let mut 输出文件 = std::fs::File::create(&分析器目标)?;
        std::io::copy(&mut 解码器, &mut 输出文件)?;
    }
    // Unix 平台确保可执行权限
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&分析器目标, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

/// 从 GitHub API 获取指定发布资产的 SHA-256 digest（官方未发布独立 .sha256 文件）
fn 取资产校验和(标签: &str, 资产名: &str) -> Option<String> {
    let 下载地址 =
        format!("https://api.github.com/repos/rust-lang/rust-analyzer/releases/tags/{标签}");
    let 响应 = ureq::get(&下载地址)
        .header("User-Agent", "rzc-install")
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .call()
        .ok()?;
    let 内容 = 响应.into_body().read_to_string().ok()?;
    let 文档: serde_json::Value = serde_json::from_str(&内容).ok()?;
    let 资产列表 = 文档.get("assets")?.as_array()?;
    for 资产 in 资产列表 {
        if 资产.get("name").and_then(|名称项| 名称项.as_str()) == Some(资产名) {
            return 资产
                .get("digest")
                .and_then(|摘要项| 摘要项.as_str())
                .and_then(|摘要项| 摘要项.strip_prefix("sha256:"))
                .map(str::to_string);
        }
    }
    None
}

/// 解压 zip 中的第一个条目到目标文件（rust-analyzer 的 windows 资产为单文件 zip）
fn 解压压缩条目(压缩文件: &Path, 目标文件: &Path) -> anyhow::Result<()> {
    let 文件句柄 = std::fs::File::open(压缩文件)?;
    let mut 归档 = zip::ZipArchive::new(文件句柄)?;
    let mut 条目 = 归档.by_index(0)?;
    let mut 输出文件 = std::fs::File::create(目标文件)?;
    std::io::copy(&mut 条目, &mut 输出文件)?;
    Ok(())
}

/// ureq 流式下载到文件（大文件不驻留内存）
fn 下载到文件(下载地址: &str, 目标文件: &Path) -> anyhow::Result<()> {
    let 响应 = ureq::get(下载地址)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(600)))
        .build()
        .call()
        .map_err(|错| anyhow::anyhow!("下载失败: {错}"))?;
    let mut 读取器 = 响应.into_body().into_reader();
    let mut 文件句柄 = std::fs::File::create(目标文件)?;
    std::io::copy(&mut 读取器, &mut 文件句柄)?;
    Ok(())
}

/// ureq 下载文本内容（如官方 .sha256 校验文件）
fn 下载文本(下载地址: &str) -> anyhow::Result<String> {
    let 响应 = ureq::get(下载地址)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build()
        .call()
        .map_err(|错| anyhow::anyhow!("下载失败: {错}"))?;
    响应
        .into_body()
        .read_to_string()
        .map_err(|错| anyhow::anyhow!("读取失败: {错}"))
}

/// 计算文件 SHA-256（十六进制小写）
fn 计算文件校验和(目标路径: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let mut 摘要器 = Sha256::new();
    let mut 文件句柄 = std::fs::File::open(目标路径)?;
    std::io::copy(&mut 文件句柄, &mut 摘要器)?;
    Ok(format!("{:x}", 摘要器.finalize()))
}

/// 解压 tar.gz 到目标目录（tar crate 默认拒绝 `..` 与绝对路径，路径安全）
fn 解压归档(归档文件: &Path, 目标目录: &Path) -> anyhow::Result<()> {
    let 文件句柄 = std::fs::File::open(归档文件)?;
    let 压缩流 = flate2::read::GzDecoder::new(文件句柄);
    let mut 归档器 = tar::Archive::new(压缩流);
    归档器.unpack(目标目录)?;
    Ok(())
}

/// 复制目录内全部文件到目标目录（跳过已存在，避免覆盖冲突）
fn 复制程序目录(来源: &Path, 目标目录: &Path) -> anyhow::Result<()> {
    for 条目 in std::fs::read_dir(来源)? {
        let 条目 = 条目?;
        let 目标文件 = 目标目录.join(条目.file_name());
        if 目标文件.is_file() {
            continue;
        }
        std::fs::copy(条目.path(), &目标文件)?;
    }
    Ok(())
}

/// 环境诊断：rzc / 内置工具链 / PATH 工具链 / 版本对比
pub fn 诊断环境() -> anyhow::Result<()> {
    use i18n_rust_engine::工具链::{
        工具链程序目录, 已安装工具链版本, 锁定工具链版本
    };

    println!("=== rzc doctor ===");
    println!("rzc: {}", env!("CARGO_PKG_VERSION"));

    let 程序目录 = 工具链程序目录();
    let 内置版本 = 已安装工具链版本();
    match &内置版本 {
        Some(版本串) => println!("内置工具链: {}（{}）", 版本串, 程序目录.display()),
        None => {
            println!("内置工具链: 未安装（可执行 rzc install toolchain 一键安装，脱离 rustup）")
        }
    }

    for 每行 in 组件状态行(&程序目录) {
        println!("{每行}");
    }

    // 语言包健康检查：遮蔽/备份遗留/身份不符只在用户环境发生，静态门检查不出
    let 语言根 = crate::语言包工具::全局语言目录();
    let 健康行 = 语言包健康状态行(
        &crate::语言包工具::扫描全局包(),
        &crate::语言包工具::全局包子目录名(),
    );
    println!();
    println!("语言包: {}", 语言根.display());
    if 健康行.is_empty() {
        println!("  全局语言包: 无遮蔽风险（未安装用户级语言包，或均与内置不冲突）");
    } else {
        for 每行 in 健康行 {
            println!("{每行}");
        }
    }

    if let Some(版本串) = 内置版本.as_deref()
        && 版本串 != 锁定工具链版本
    {
        println!(
            "提示: 内置工具链 {} 与锁定版本 {} 不一致（rzc install toolchain --force 更新）",
            版本串, 锁定工具链版本
        );
    }
    println!(
        "升级内置工具链：rzc install toolchain --version <新版本> --force（当前锁定 {}）",
        锁定工具链版本
    );
    Ok(())
}

/// 语言包健康检查状态行（纯函数，便于测试）
///
/// `packs` 为全局目录下的语言包候选（含 keywords.toml 的子目录），
/// `subdirs` 为该目录下全部子目录名。返回空表示无异常。
///
/// 报三类问题（均只在用户环境发生，静态门检查不出）：
/// - 目录名与内置 code 相同 → 用户级副本遮蔽内置表（内置表优先级最低）
/// - 目录名形如备份（以 `.bak` 结尾或含 `.stale`）→ 仍被当语言包吃掉并接管扩展名
/// - 正常包（非备份）目录名与声明的扩展名不一致 → 提示确认是否有意为之
///
/// 第三方自研包（如 `vi` 的扩展名也是 `vi`）不属异常，不报警。
fn 语言包健康状态行(
    包候选: &[crate::语言包工具::全局语言包],
    子目录列表: &[String],
) -> Vec<String> {
    let mut 状态行 = Vec::new();
    // 1. 遮蔽内置表（同名的正常目录）
    for 包项 in 包候选 {
        if !crate::内置语言::拥有内置语言(&包项.目录名) || 是备份名(&包项.目录名)
        {
            continue;
        }
        let 内置版本 = crate::语言包工具::取内置元数据(&包项.目录名)
            .map(|元数据项| 元数据项.版本号)
            .unwrap_or_else(|| "?".to_string());
        let 用户版本 = 包项
            .包元数据
            .as_ref()
            .map(|元数据项| 元数据项.版本号.clone())
            .unwrap_or_else(|| "?".to_string());
        状态行.push(format!(
            "  ⚠ 全局包 {}（v{}）遮蔽内置语言包（v{}），当前 .{} 源码走全局副本",
            包项.目录名,
            用户版本,
            内置版本,
            取生效扩展名(包项).unwrap_or_else(|| 包项.目录名.clone())
        ));
        状态行.push(
            "    → 升级 rzc 后诊断文案仍是旧的，多半是此因：rzc lang remove <码>，或把该目录移出 lang-packs/"
                .to_string(),
        );
    }
    // 2. 备份/改名遗留：无论是否含 keywords.toml，留在 lang-packs/ 内都可能被吃掉
    for 目录名 in 子目录列表 {
        if !是备份名(目录名) {
            continue;
        }
        if 包候选.iter().any(|候选包项| &候选包项.目录名 == 目录名) {
            状态行.push(format!(
                "  ⚠ 备份目录 {目录名} 仍被当作语言包吃掉（语言 code 取目录名）：请移出 {}",
                crate::语言包工具::全局语言目录().display()
            ));
        } else {
            状态行.push(format!(
                "  ⚠ 子目录 {目录名} 不具备 keywords.toml（未作为语言包加载）：备份建议移出 lang-packs/"
            ));
        }
    }
    // 3. 目录名与声明扩展名不符（排除备份与自研同名包）
    for 包项 in 包候选 {
        if crate::内置语言::拥有内置语言(&包项.目录名) || 是备份名(&包项.目录名)
        {
            continue;
        }
        if let Some(后缀名) = 取生效扩展名(包项)
            && 后缀名 != 包项.目录名
        {
            状态行.push(format!(
                "  ⚠ 全局包目录名 {} 与其声明的扩展名 {后缀名} 不一致（语言 code 取目录名，当前 .{后缀名} 源码按声明接管）",
                包项.目录名
            ));
        }
    }
    状态行
}

/// 备份/改名遗留目录名特征：以 .bak 结尾或含 .stale
fn 是备份名(目录名: &str) -> bool {
    目录名.ends_with(".bak") || 目录名.contains(".stale")
}

/// 取全局包生效的扩展名：lang_info 声明优先，缺失时回退静态映射推断
fn 取生效扩展名(包项: &crate::语言包工具::全局语言包) -> Option<String> {
    包项
        .包元数据
        .as_ref()
        .map(|元数据项| 元数据项.文件扩展名.clone())
        .or_else(|| {
            crate::语言包工具::静态扩展名映射()
                .into_iter()
                .find(|(_, 编码项)| 编码项 == &包项.目录名)
                .map(|(扩展名项, _)| 扩展名项)
        })
}

/// 各组件来源状态行（纯函数，便于测试）：`rustc: 内置（路径）` / `cargo: PATH（路径）` 等
fn 组件状态行(程序目录: &std::path::Path) -> Vec<String> {
    use i18n_rust_engine::工具链::查找工具链程序;

    ["rustc", "cargo", "rust-analyzer"]
        .iter()
        .map(|组件名| {
            let 内置路径 = 程序目录.join(format!("{组件名}{可执行后缀}"));
            let 来源 = if 内置路径.is_file() {
                format!("内置（{}）", 内置路径.display())
            } else {
                match 查找工具链程序(组件名) {
                    Some(程序路径) => format!("PATH（{}）", 程序路径.display()),
                    None => "未找到".to_string(),
                }
            };
            format!("{组件名}: {来源}")
        })
        .collect()
}

/// 双击 rzc.exe（或终端无参数运行）时的环境安装向导：
/// 逐项检查组件状态，标注「已就绪 ✓」或「缺失 ✗ + 解决命令」。
pub fn 显示安装向导() {
    use i18n_rust_engine::工具链::{工具链程序目录, 查找工具链程序};
    let 后缀 = std::env::consts::EXE_SUFFIX;

    println!("════════════ i18n-rust 环境安装向导 ════════════");
    println!("rzc 本体          v{}（已就绪）", env!("CARGO_PKG_VERSION"));
    println!();
    println!("  💡 也可下载「离线发布包」：压缩包已含 rust-analyzer + 语言服务器 +");
    println!("     语言包与教程，适合无网络环境（离线包由本地构建，手动分发）。");
    println!();

    // 1. 内置工具链（rustc / cargo / rust-analyzer）
    println!("【第 1 步】编译器与工具链（rzc install toolchain 一键安装）");
    for (组件名, 安装命令) in [
        ("rustc", "rzc install toolchain"),
        ("cargo", "rzc install toolchain"),
        ("rust-analyzer", "rzc install toolchain --force"),
    ] {
        let 内置路径 = 工具链程序目录().join(format!("{组件名}{后缀}"));
        let (标记, 详情) = if 内置路径.is_file() {
            ("✓ 已就绪", format!("内置（{}）", 内置路径.display()))
        } else if let Some(程序路径) = 查找工具链程序(组件名) {
            ("✓ 已就绪", format!("PATH（{}）", 程序路径.display()))
        } else {
            (
                "✗ 缺失",
                format!("未找到 → 运行: {安装命令}（需联网，约 300MB）"),
            )
        };
        println!("  [{标记}] {组件名:<14} {详情}");
    }
    println!();

    // 2. 语言服务器（编辑器补全/诊断后端）
    println!("【第 2 步】语言服务器 i18n-rust-lsp（rzc install lsp 一键安装）");
    if let Some(程序路径) = 查找工具链程序("i18n-rust-lsp") {
        println!("  [✓ 已就绪] {}", 程序路径.display());
        if let Ok(命令输出) = std::process::Command::new(&程序路径)
            .arg("--version")
            .output()
        {
            let 目标版本 = String::from_utf8_lossy(&命令输出.stdout).trim().to_string();
            if !目标版本.is_empty() {
                println!("              版本 {目标版本}");
            }
        }
    } else {
        println!("  [✗ 缺失] 未找到 i18n-rust-lsp{后缀} → 运行: rzc install lsp");
    }
    println!();

    // 3. 环境变量（可选）
    println!("【第 3 步】环境变量（可选，一般无需配置）");
    println!("  RZ_LANG_DIR        指定语言包目录（默认内置）");
    println!("  RUST_ANALYZER_PATH 指定 rust-analyzer 路径（自动检测失败时）");
    println!();

    // 常用命令
    println!("────────── 常用命令速查 ──────────");
    println!("  rzc init 我的项目        创建新项目");
    println!("  rzc run src/main.zh      运行中文代码");
    println!("  rzc check src/main.zh    类型检查（中文错误提示）");
    println!("  rzc install toolchain    一键安装内置工具链");
    println!("  rzc install lsp          安装语言服务器");
    println!("  rzc doctor               查看工具链环境状态与语言包遮蔽");
    println!("  rzc --help               查看全部命令");
    println!();
    println!("详细教程见《开篇：这本书怎么用》与《第一章：把程序跑起来》。");
}

#[cfg(test)]
mod 单元测试 {
    use super::*;
    use std::fs;

    #[test]
    fn 测试语言服务器版本校验相符() {
        assert_eq!(
            校验语言服务器版本(Some("0.5.5".to_string()), "0.5.5"),
            版本校验::相符
        );
    }

    #[test]
    fn 测试语言服务器版本校验不符() {
        assert_eq!(
            校验语言服务器版本(Some("0.5.3".to_string()), "0.5.5"),
            版本校验::不符("0.5.3".to_string())
        );
    }

    #[test]
    fn 测试语言服务器版本校验未知() {
        assert_eq!(校验语言服务器版本(None, "0.5.5"), 版本校验::未知);
    }

    #[test]
    fn 测试目标三元组受支持() {
        // 当前平台必须能被识别（不 panic）
        let 三元组 = 计算目标三元组().expect("当前平台应受支持");
        assert!(!三元组.is_empty());
    }

    /// SHA-256 计算正确性（空文件与已知内容）
    #[test]
    fn 测试文件校验和已知内容() {
        let 目录 = tempfile::tempdir().unwrap();
        let file = 目录.path().join("a.txt");
        std::fs::write(&file, b"hello").unwrap();
        // `hello` 的 SHA-256（标准已知值）
        assert_eq!(
            计算文件校验和(&file).unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    /// 内置工具链存在时状态行标记为内置
    #[test]
    fn 测试组件状态行标记内置() {
        let 目录 = tempfile::tempdir().unwrap();
        let 程序目录 = 目录.path();
        std::fs::write(程序目录.join(format!("rustc{可执行后缀}")), b"x").unwrap();
        let 状态行 = 组件状态行(程序目录);
        assert!(状态行.iter().any(|每行| 每行.starts_with("rustc: 内置")));
        // 其余组件未内置时标记 PATH 或未找到（不 panic）
        assert!(状态行.iter().any(|每行| 每行.starts_with("cargo:")));
        assert!(状态行.iter().any(|每行| 每行.starts_with("rust-analyzer:")));
    }

    /// 构造全局语言包候选条目（测试辅助）
    fn 构造全局包(
        目录名: &str,
        后缀名: Option<&str>,
        目标版本: Option<&str>,
    ) -> crate::语言包工具::全局语言包 {
        use crate::语言包工具::语言元数据;
        crate::语言包工具::全局语言包 {
            目录名: 目录名.to_string(),
            包元数据: 后缀名.map(|扩展名项| 语言元数据 {
                语言名称: "测试".to_string(),
                文件扩展名: 扩展名项.to_string(),
                版本号: 目标版本.unwrap_or("0.1").to_string(),
            }),
        }
    }

    /// 全局同名包遮蔽内置表：须报警并给出解除命令
    #[test]
    fn 测试语言包健康报遮蔽() {
        let 包候选 = vec![构造全局包("zh", Some("zh"), Some("1.0"))];
        let 状态行 = 语言包健康状态行(&包候选, &["zh".to_string()]);
        assert!(
            状态行.iter().any(|每行| 每行.contains("遮蔽内置语言包")),
            "{状态行:?}"
        );
        assert!(
            状态行.iter().any(|每行| 每行.contains("rzc lang remove")),
            "应给出解除命令: {状态行:?}"
        );
    }

    /// 备份目录改名后仍被当语言包吃掉：须报警（即上一步实际踩到的坑）
    #[test]
    fn 测试语言包健康报备份遗留() {
        let 目录名 = "zh.stale-20260919.bak";
        let 包候选 = vec![构造全局包(目录名, Some("zh"), None)];
        let 状态行 = 语言包健康状态行(&包候选, &[目录名.to_string()]);
        assert!(
            状态行
                .iter()
                .any(|每行| 每行.contains("仍被当作语言包吃掉")),
            "{状态行:?}"
        );
        // 遮蔽行只针对目录名与内置 code 完全相同的包，备份不当作遮蔽报
        assert!(
            !状态行.iter().any(|每行| 每行.contains("遮蔽内置语言包")),
            "{状态行:?}"
        );
    }

    /// 备份目录不具备 keywords.toml（未被加载）也须提示移出
    #[test]
    fn 测试语言包健康报未加载备份() {
        let 目录名 = "ja.backup-20260919.stale";
        let 状态行 = 语言包健康状态行(&[], &[目录名.to_string()]);
        assert!(
            状态行
                .iter()
                .any(|每行| 每行.contains(目录名) && 每行.contains("keywords.toml")),
            "{状态行:?}"
        );
    }

    /// 第三方自研包（目录名与扩展名一致）不应被误报
    #[test]
    fn 测试语言包健康对自研包静默() {
        let 包候选 = vec![构造全局包("vi", Some("vi"), Some("1.0"))];
        assert!(
            语言包健康状态行(&包候选, &["vi".to_string()]).is_empty(),
            "自研包不应报警"
        );
    }

    /// 自研包目录名与声明扩展名不符：提示身份不一致
    #[test]
    fn 测试语言包健康报身份不符() {
        let 包候选 = vec![构造全局包("mypack", Some("vi"), None)];
        let 状态行 = 语言包健康状态行(&包候选, &["mypack".to_string()]);
        assert!(
            状态行.iter().any(|每行| 每行.contains("不一致")),
            "{状态行:?}"
        );
    }

    /// 无全局包时不报任何异常
    #[test]
    fn 测试语言包健康空即静默() {
        assert!(语言包健康状态行(&[], &[]).is_empty());
    }

    /// 写一个全局语言包目录（keywords.toml + 可选 lang_info.toml），供端到端注入。
    /// `lang_info` 为 None 时不写 lang_info.toml（模拟扩展名靠静态映射推断的旧包）。
    fn 写入全局包(
        根: &std::path::Path,
        目录名: &str,
        后缀名: Option<&str>,
        目标版本: Option<&str>,
        含关键词: bool,
    ) {
        let 目录 = 根.join(目录名);
        std::fs::create_dir_all(&目录).unwrap();
        if 含关键词 {
            std::fs::write(目录.join("keywords.toml"), "# 占位\n").unwrap();
        }
        if let Some(后缀名) = 后缀名 {
            let 目标版本 = 目标版本.unwrap_or("1.0");
            std::fs::write(
                目录.join("lang_info.toml"),
                format!("[\"语言包\"]\n\"名称\" = \"{目录名}\"\n\"扩展名\" = \"{后缀名}\"\n\"版本\" = \"{目标版本}\"\n"),
            )
            .unwrap();
        }
    }

    /// 端到端故障注入：真实文件系统扫描 → health 行，逐场景核应报/不误报。
    ///
    /// 上面几例只喂合成切片，验不出 `scan_global_packs`/`global_pack_subdir_names`
    /// 对真实目录（含/不含 keywords.toml、lang_info 解析）的处理是否与判定假设一致；
    /// 这里把六类只在用户环境发生的坑一次性注入 `RZ_LANG_DIR` 指向的临时目录，
    /// 走真实的 scan→health 链路验证 doctor 能一次查全且不误报。
    #[test]
    fn 测试诊断环境语言健康端到端注入() {
        let _锁 = crate::语言包工具::单元测试::环境锁();
        let 临时根 = tempfile::tempdir().unwrap();
        let 全局目录 = 临时根.path().join("global");
        std::fs::create_dir_all(&全局目录).unwrap();
        // ① 同名遮蔽内置表（版本不同）
        写入全局包(&全局目录, "zh", Some("zh"), Some("9.9"), true);
        // ② 改名遗留的备份且仍被当包吃掉（含 keywords.toml）
        写入全局包(&全局目录, "de.stale-20260101.bak", Some("de"), None, true);
        // ③ 不具备 keywords.toml 的备份（未被加载，仍须提示移出）
        写入全局包(&全局目录, "ja.backup.stale", Some("ja"), None, false);
        // ④ 合法自研包（目录名即扩展名）：不得误报
        写入全局包(&全局目录, "vi", Some("vi"), Some("1.0"), true);
        // ⑤ 目录名与声明扩展名不符的自研包：应报身份不一致
        写入全局包(&全局目录, "mypack", Some("fr"), None, true);

        unsafe { std::env::set_var("RZ_LANG_DIR", &全局目录) };
        let 包候选 = crate::语言包工具::扫描全局包();
        let 子目录列表 = crate::语言包工具::全局包子目录名();
        let 健康行 = 语言包健康状态行(&包候选, &子目录列表);
        unsafe { std::env::remove_var("RZ_LANG_DIR") };

        let 合并文本 = 健康行.join("\n");
        // ① 同名遮蔽：报警 + 给出解除命令
        assert!(
            合并文本.contains("遮蔽内置语言包") && 合并文本.contains("rzc lang remove"),
            "{合并文本}"
        );
        // ② 备份仍被吃掉
        assert!(
            健康行
                .iter()
                .any(|每行| 每行.contains("de.stale-20260101.bak")
                    && 每行.contains("仍被当作语言包吃掉")),
            "{合并文本}"
        );
        // ③ 未加载备份：提示 keywords.toml 缺失
        assert!(
            健康行
                .iter()
                .any(|每行| 每行.contains("ja.backup.stale") && 每行.contains("keywords.toml")),
            "{合并文本}"
        );
        // ⑤ 身份不一致
        assert!(
            健康行
                .iter()
                .any(|每行| 每行.contains("mypack") && 每行.contains("不一致")),
            "{合并文本}"
        );
        // ④ 合法自研包 vi 不得出现在任何报警行
        assert!(
            !健康行.iter().any(|每行| 每行.contains("vi（")
                || 每行.contains("  vi ")
                || (每行.contains("vi") && 每行.contains("不一致"))),
            "合法自研包 vi 不应被误报：{合并文本}"
        );
    }

    /// sha256_file：已知向量（空串/abc）；文件缺失报错
    #[test]
    fn 测试文件校验和已知向量() {
        let 临时 = tempfile::tempdir().unwrap();
        let 空文件 = 临时.path().join("empty");
        fs::write(&空文件, b"").unwrap();
        assert_eq!(
            计算文件校验和(&空文件).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let abc文件 = 临时.path().join("abc");
        fs::write(&abc文件, b"abc").unwrap();
        assert_eq!(
            计算文件校验和(&abc文件).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(计算文件校验和(&临时.path().join("absent")).is_err());
    }

    /// copy_bin_dir：逐文件复制；目标已存在的文件跳过（不覆盖）
    #[test]
    fn 测试复制程序目录跳过已存在() {
        let 临时 = tempfile::tempdir().unwrap();
        let 来源 = 临时.path().join("from");
        let 目标 = 临时.path().join("to");
        fs::create_dir_all(&来源).unwrap();
        fs::create_dir_all(&目标).unwrap();
        fs::write(来源.join("a"), b"new-a").unwrap();
        fs::write(来源.join("b"), b"new-b").unwrap();
        fs::write(目标.join("a"), b"keep-old").unwrap();
        复制程序目录(&来源, &目标).unwrap();
        assert_eq!(fs::read(目标.join("a")).unwrap(), b"keep-old");
        assert_eq!(fs::read(目标.join("b")).unwrap(), b"new-b");
        // 源目录缺失报错（调用方负责确保前置条件）
        assert!(复制程序目录(&临时.path().join("absent"), &目标).is_err());
    }

    /// extract_zip_entry：取 zip 第一个条目原样落盘（Windows 单文件资产路径）
    #[test]
    fn 测试解压压缩条目往返() {
        use std::io::Write as _;
        use zip::write::FileOptions;
        let 临时 = tempfile::tempdir().unwrap();
        let 压缩文件 = 临时.path().join("asset.zip");
        let 有效载荷 = b"rust-analyzer binary \x00\x01\x02";
        {
            let 文件句柄 = fs::File::create(&压缩文件).unwrap();
            let mut 归档 = zip::ZipWriter::new(文件句柄);
            归档
                .start_file("rust-analyzer", FileOptions::<()>::default())
                .unwrap();
            归档.write_all(有效载荷).unwrap();
            归档.finish().unwrap();
        }
        let 目标 = 临时.path().join("out/rust-analyzer");
        fs::create_dir_all(目标.parent().unwrap()).unwrap();
        解压压缩条目(&压缩文件, &目标).unwrap();
        assert_eq!(fs::read(&目标).unwrap(), 有效载荷);
        // 损坏/缺失 zip → 报错而非写出半截文件
        let 坏文件 = 临时.path().join("bad.zip");
        fs::write(&坏文件, b"not a zip").unwrap();
        assert!(解压压缩条目(&坏文件, &临时.path().join("out2")).is_err());
    }

    /// extract_tar_gz：归档内文件按相对路径还原到目标目录
    #[test]
    fn 测试解压归档往返() {
        let 临时 = tempfile::tempdir().unwrap();
        let 归档路径 = 临时.path().join("bin.tar.gz");
        let 有效载荷 = b"#!/bin/sh\necho hi\n";
        {
            let 文件句柄 = fs::File::create(&归档路径).unwrap();
            let 编码器 = flate2::write::GzEncoder::new(文件句柄, flate2::Compression::default());
            let mut 构建器 = tar::Builder::new(编码器);
            let mut 表头 = tar::Header::new_gnu();
            表头.set_path("bin/rzc").unwrap();
            表头.set_size(有效载荷.len() as u64);
            表头.set_mode(0o755);
            表头.set_cksum();
            构建器.append(&表头, &有效载荷[..]).unwrap();
            构建器.into_inner().unwrap().finish().unwrap();
        }
        let 目标 = 临时.path().join("dest");
        解压归档(&归档路径, &目标).unwrap();
        assert_eq!(fs::read(目标.join("bin/rzc")).unwrap(), 有效载荷);
    }

    /// 备份名识别：.bak 后缀或含 .stale 段；正常目录名不误判
    #[test]
    fn 测试备份名识别规则() {
        assert!(是备份名("ja.bak"));
        assert!(是备份名("de.stale-20260101.bak"));
        assert!(是备份名("ja.backup.stale"));
        assert!(!是备份名("ja"));
        assert!(!是备份名("backup-dir"));
        assert!(!是备份名("stale"));
    }
}
