//! 配套组件安装模块
//!
//! `rzc install` 安装 rzc 所需的配套组件。当前组件为语言服务器
//! i18n-rust-lsp（编辑器扩展的补全/诊断后端），与 rzc 属于不同
//! 包，常规安装不会自动带上，需单独安装。
//!
//! 安装来源优先级：
//! 1. 与 rzc 同目录的可执行文件（离线发布包自带，免网络直接复制到工具目录）
//! 2. 远端包仓库（安装语言服务器，版本与 rzc 严格一致，
//!    保证协议与语言包版本兼容）

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::界面::界面消息表;
use std::io::Read;

use crate::语言管理::{
    全局语言包, 全局语言包子目录名, 全局语言目录, 全部可用扩展名, 扫描全局语言包, 获取内置元数据,
    静态扩展名映射,
};

/// 语言服务器可执行文件名（对应语言服务包的包名）
const 语言服务程序名: &str = "i18n-rust-lsp";

/// 当前平台可执行文件后缀（Windows 为可执行后缀，其余为空）
const 程序后缀: &str = std::env::consts::EXE_SUFFIX;

/// 工具程序目录：工具主目录下的程序目录，未设置时回退用户主目录下的默认位置
fn 货物程序目录() -> PathBuf {
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
/// 优先使用与 rzc 同目录的可执行文件（离线包场景）；否则通过
/// 包管理器从远端包仓库安装，保证与 rzc 版本严格一致。
///
/// 已安装时先校验版本：与 rzc 一致则提示已安装；不一致或无法确认时
/// 引导用户执行 `--force` 重装（不自动覆盖，避免打断正在使用的语言服务进程）。
pub fn 安装语言服务(界面表: &界面消息表, 强制: bool) -> anyhow::Result<()> {
    let 目标路径 = 货物程序目录().join(format!("{语言服务程序名}{程序后缀}"));
    if 目标路径.is_file() && !强制 {
        match 核对语言服务版本(读取已装语言服务版本(&目标路径), env!("CARGO_PKG_VERSION"))
        {
            版本核对::一致 => {
                println!(
                    "{}",
                    界面表
                        .取带参消息("lsp_install_already", &[&目标路径.display().to_string()])
                );
            }
            版本核对::不一致(已装版本) => {
                println!(
                    "{}",
                    界面表.取带参消息(
                        "lsp_install_version_mismatch",
                        &[&已装版本, env!("CARGO_PKG_VERSION")]
                    )
                );
            }
            版本核对::未知 => {
                println!(
                    "{}",
                    界面表.取带参消息(
                        "lsp_install_version_unknown",
                        &[&目标路径.display().to_string()]
                    )
                );
            }
        }
        return Ok(());
    }

    // 1. 离线发布包：与 rzc 同目录附带同名可执行文件，直接复制（无需网络）
    if let Ok(可执行路径) = std::env::current_exe()
        && let Some(所在目录) = 可执行路径.parent()
    {
        let 本地路径 = 所在目录.join(format!("{语言服务程序名}{程序后缀}"));
        if 本地路径.is_file() {
            从本地安装(界面表, &本地路径, &目标路径)?;
            return Ok(());
        }
    }

    // 2. 远端包仓库：版本与 rzc 精确一致
    从包仓库安装(界面表, &目标路径)?;
    Ok(())
}

/// 从本地路径复制到工具程序目录
fn 从本地安装(
    界面表: &界面消息表, 本地路径: &Path, 目标路径: &Path
) -> anyhow::Result<()> {
    let 程序目录 = 目标路径.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "{}",
            界面表.取带参消息("lsp_install_failed", &["无有效安装目录"])
        )
    })?;
    std::fs::create_dir_all(程序目录).map_err(|错误值| {
        anyhow::anyhow!(
            "{}",
            界面表.取带参消息("lsp_install_failed", &[&错误值.to_string()])
        )
    })?;
    std::fs::copy(本地路径, 目标路径).map_err(|错误值| {
        anyhow::anyhow!(
            "{}",
            界面表.取带参消息("lsp_install_failed", &[&错误值.to_string()])
        )
    })?;
    println!(
        "{}",
        界面表.取带参消息("lsp_install_local", &[&目标路径.display().to_string()])
    );
    Ok(())
}

/// 通过包管理器从远端包仓库安装
fn 从包仓库安装(界面表: &界面消息表, 目标路径: &Path) -> anyhow::Result<()> {
    let 版本值 = env!("CARGO_PKG_VERSION");
    println!("{}", 界面表.取带参消息("lsp_install_cargo", &[版本值]));
    let 状态值 = Command::new(crate::解析cargo())
        .arg("install")
        .arg(语言服务程序名)
        .arg("--version")
        .arg(format!("={版本值}"))
        .status()
        .map_err(|错误值| {
            anyhow::anyhow!(
                "{}",
                界面表.取带参消息("lsp_install_no_cargo", &[&错误值.to_string()])
            )
        })?;
    if !状态值.success() {
        anyhow::bail!(
            "{}",
            界面表.取带参消息("lsp_install_failed", &[&状态值.to_string()])
        );
    }
    // 包管理器默认输出到工具主目录下的程序目录，与目标路径一致
    println!("{}", 界面表.取带参消息("lsp_install_done", &[版本值]));
    println!(
        "{}",
        界面表.取带参消息("lsp_install_hint", &[&目标路径.display().to_string()])
    );
    Ok(())
}

/// 已安装版本与期望版本的比较结果
#[derive(Debug, PartialEq, Eq)]
enum 版本核对 {
    /// 版本一致
    一致,
    /// 版本不一致（附已安装版本号）
    不一致(String),
    /// 无法确定已安装版本
    未知,
}

/// 比较已安装语言服务器版本与期望版本（纯函数，便于测试）
fn 核对语言服务版本(已装版本: Option<String>, 期望版本: &str) -> 版本核对 {
    match 已装版本 {
        Some(版本值) if 版本值 == 期望版本 => 版本核对::一致,
        Some(版本值) => 版本核对::不一致(版本值),
        None => 版本核对::未知,
    }
}

/// 读取已安装语言服务器的版本号（`i18n-rust-lsp --version` 输出纯版本号）
///
/// 可执行文件不存在、执行失败（如被占用）或输出为空时返回空值。
fn 读取已装语言服务版本(目标路径: &Path) -> Option<String> {
    let 输出值 = Command::new(目标路径).arg("--version").output().ok()?;
    if !crate::界面::命令成功(&输出值) {
        return None;
    }
    let 版本值 = String::from_utf8_lossy(&输出值.stdout).trim().to_string();
    if 版本值.is_empty() {
        None
    } else {
        Some(版本值)
    }
}

// ============================================================
// 内置工具链（独立编译器/包管理器/语言分析器）
// ============================================================

/// 语言分析器官方发布标签（随官方发布升级；可用 `--ra-tag` 覆盖）
pub const 语言服务发布标签: &str = "2026-08-24";

/// 当前平台的目标三元组（官方分发与语言分析器发布资产名用）
///
/// 不支持的平台返回错误而非恐慌，由调用方以本地化提示终止（避免崩溃式退出）。
fn 目标三元组() -> anyhow::Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        (系统值, 架构值) => {
            anyhow::bail!("不支持的平台 {系统值}/{架构值}（内置工具链暂未覆盖）")
        }
    }
}

/// 安装内置工具链（独立编译器/包管理器/语言分析器）到用户工具链目录
///
/// 编译器与包管理器来自官方分发（含全套组件）；
/// 语言分析器来自官方发布（下载失败不阻塞——可稍后补装）。
/// `仅分析器` 为真时仅升级语言分析器（跳过编译器与包管理器的三百兆重下）。
/// 安装后 rzc 与语言服务优先使用内置工具链，不再依赖工具链管理器与 PATH 配置。
pub fn 安装工具链(
    界面表: &界面消息表,
    版本值: &str,
    分析器标签: &str,
    强制: bool,
    仅分析器: bool,
) -> anyhow::Result<()> {
    use i18n_rust_engine::工具链::{安装根目录, 工具链程序目录};

    let 程序目录 = 工具链程序目录();
    if 仅分析器 {
        return 仅安装语言分析器(界面表, 分析器标签, &程序目录);
    }

    let 编译器路径 = 程序目录.join(format!("rustc{程序后缀}"));
    if 编译器路径.is_file() && !强制 {
        println!(
            "{}",
            界面表.取带参消息("tc_install_already", &[&程序目录.display().to_string()])
        );
        return Ok(());
    }

    let 三元组 = 目标三元组()?;
    let 临时 = tempfile::tempdir()?;

    // 1. 编译器与包管理器独立包（官方分发单包含全部组件）
    let 地址 = format!("https://static.rust-lang.org/dist/rust-{版本值}-{三元组}.tar.gz");
    println!("{}", 界面表.取带参消息("tc_download_rustc", &[&地址]));
    let 归档路径 = 临时.path().join("rust.tar.gz");
    下载到(&地址, &归档路径)?;
    // 校验官方摘要（官方分发提供摘要文件）
    // 缺失即停：校验和缺失/格式非法时直接中止，绝不静默放行
    let 摘要地址 = format!("{地址}.sha256");
    let 期望值 = 下载文本(&摘要地址)?
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    let 摘要有效 = 期望值.len() == 64
        && 期望值
            .chars()
            .all(|本字| "0123456789abcdef".contains(本字.to_ascii_lowercase()));
    if !摘要有效 {
        anyhow::bail!("编译器包摘要校验和不可用（{摘要地址} 返回异常内容），已中止下载");
    }
    let 实际值 = 文件摘要(&归档路径)?;
    if 实际值 != 期望值 {
        anyhow::bail!("编译器包摘要校验失败（下载可能被篡改，请重试）");
    }
    println!("{}", 界面表.取消息("tc_extracting"));
    解压归档包(&归档路径, 临时.path())?;
    let 根目录 = 临时.path().join(format!("rust-{版本值}-{三元组}"));
    std::fs::create_dir_all(&程序目录)?;
    复制程序目录(&根目录.join("rustc").join("bin"), &程序目录, 强制)?;
    复制程序目录(&根目录.join("cargo").join("bin"), &程序目录, 强制)?;
    // 覆盖安装后做一次冒烟校验：文件在但不可执行（半截覆盖/平台不匹配）
    // 不能写版本号冒充成功
    let 版本可用 = Command::new(&编译器路径)
        .arg("--version")
        .output()
        .map(|输出值| 输出值.status.success())
        .unwrap_or(false);
    if !版本可用 {
        anyhow::bail!(
            "工具链安装后校验失败：{} --version 未成功（若为覆盖安装，请关闭正在使用该工具链的进程后重试）",
            编译器路径.display()
        );
    }

    // 2. 语言分析器（官方发布的压缩资产：类 Unix 为压缩流、Windows 为压缩包；
    //    失败不阻塞主流程，但已下载的可执行文件必须通过摘要校验，未经验证绝不落盘）
    match 下载语言分析器(分析器标签, 三元组, &程序目录, 临时.path()) {
        Ok(()) => println!("{}", 界面表.取带参消息("tc_ra_installed", &[分析器标签])),
        Err(错误值) => println!(
            "{}",
            界面表.取带参消息("tc_ra_skipped", &[&错误值.to_string()])
        ),
    }

    // 3. 记录版本
    let 工具链目录 = 安装根目录().join("toolchain");
    std::fs::create_dir_all(&工具链目录)?;
    std::fs::write(工具链目录.join("version.txt"), 版本值)?;
    println!(
        "{}",
        界面表.取带参消息("tc_install_done", &[&程序目录.display().to_string()])
    );
    Ok(())
}

/// 仅升级语言分析器（跳过编译器与包管理器的三百兆重下）
fn 仅安装语言分析器(
    界面表: &界面消息表,
    分析器标签: &str,
    程序目录: &Path,
) -> anyhow::Result<()> {
    // 首次运行时工具链程序目录可能不存在，须先创建（否则解压后重命名报错）
    std::fs::create_dir_all(程序目录)?;
    let 三元组 = 目标三元组()?;
    let 临时 = tempfile::tempdir()?;
    下载语言分析器(分析器标签, 三元组, 程序目录, 临时.path())?;
    println!("{}", 界面表.取带参消息("tc_ra_installed", &[分析器标签]));
    Ok(())
}

/// 下载并安装语言分析器（官方发布的压缩资产，带摘要校验）
///
/// 资产命名：类 Unix 为压缩流、Windows 为压缩包，
/// 官方不发布裸可执行资产（旧代码直接下载裸名会失败，从未安装成功）。
/// 完整性校验：官方未发布独立摘要文件，摘要由远端接口的
/// 资产列表提供；校验失败（篡改）或摘要不可用（接口限流）均抛错中止，
/// 不做「未验证降级安装」——后者会让阻断接口请求成为绕过完整性校验的手段。
fn 下载语言分析器(
    分析器标签: &str,
    三元组: &str,
    程序目录: &Path,
    临时目录: &Path,
) -> anyhow::Result<()> {
    let 是视窗 = std::env::consts::OS == "windows";
    let 资产名 = if 是视窗 {
        format!("rust-analyzer-{三元组}.zip")
    } else {
        format!("rust-analyzer-{三元组}.gz")
    };
    let 地址 = format!(
        "https://github.com/rust-lang/rust-analyzer/releases/download/{分析器标签}/{资产名}"
    );
    let 分析器目标 = 程序目录.join(format!("rust-analyzer{程序后缀}"));

    // 下载压缩资产
    let 压缩路径 = 临时目录.join("rust-analyzer.asset");
    下载到(&地址, &压缩路径)?;

    // 完整性校验：官方接口的摘要字段（无独立摘要文件）。
    // 摘要不可用时**中止安装**：跳过校验等于放弃完整性保证，而让接口
    // 不可达（限流、域名系统或中间人劫持）比篡改发布资产容易得多，若此时仍安装，
    // 攻击者只需阻断一次接口请求即可让任意可执行文件被装入并随后运行。
    let 期望值 = 分析器资产摘要(分析器标签, &资产名).ok_or_else(|| {
        anyhow::anyhow!(
            "无法获取语言分析器官方摘要（资产 {资产名}，标签 {分析器标签}）：\
             远端接口可能限流或网络不可达。为避免安装未经验证的可执行文件已中止，请稍后重试"
        )
    })?;
    let 实际值 = 文件摘要(&压缩路径)?;
    if 实际值 != 期望值 {
        let _ = std::fs::remove_file(&压缩路径);
        anyhow::bail!("语言分析器包摘要校验失败（下载可能被篡改）");
    }

    // 解压到目标目录
    if 是视窗 {
        提取压缩包条目(&压缩路径, &分析器目标)?;
    } else {
        let 压缩数据 = std::fs::read(&压缩路径)?;
        let 解压流 = flate2::read::GzDecoder::new(std::io::Cursor::new(压缩数据));
        let mut 解压流变量 = 解压流;
        let mut 解压内容 = Vec::new();
        解压流变量.read_to_end(&mut 解压内容)?;
        std::fs::write(&分析器目标, &解压内容)?;
    }
    // 类 Unix 平台确保可执行权限
    #[cfg(unix)]
    {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&分析器目标, Permissions::from_mode(0o755));
    }
    Ok(())
}

/// 从远端接口获取指定发布资产的摘要值（官方未发布独立摘要文件）
fn 分析器资产摘要(标签: &str, 资产名: &str) -> Option<String> {
    let 地址 = format!("https://api.github.com/repos/rust-lang/rust-analyzer/releases/tags/{标签}");
    let 响应值 = ureq::get(&地址)
        .header("User-Agent", "rzc-install")
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .call()
        .ok()?;
    let 响应文本 = 响应值.into_body().read_to_string().ok()?;
    let 值项: serde_json::Value = serde_json::from_str(&响应文本).ok()?;
    let 资产们 = 值项.get("assets")?.as_array()?;
    for 资产项 in 资产们 {
        if 资产项.get("name").and_then(|名称值| 名称值.as_str()) == Some(资产名) {
            return 资产项
                .get("digest")
                .and_then(|摘要值| 摘要值.as_str())
                .and_then(|摘要文本| 摘要文本.strip_prefix("sha256:"))
                .map(|摘要文本| 摘要文本.to_string());
        }
    }
    None
}

/// 解压压缩包中的第一个条目到目标文件（语言分析器的 Windows 资产为单文件压缩包）
fn 提取压缩包条目(压缩包路径: &Path, 目标文件: &Path) -> anyhow::Result<()> {
    let 压缩数据 = std::fs::read(压缩包路径)?;
    let 文件游标 = std::io::Cursor::new(压缩数据);
    let mut 归档 = zip::ZipArchive::new(文件游标)?;
    let mut 条目值 = 归档.by_index(0)?;
    let mut 条目内容 = Vec::new();
    条目值.read_to_end(&mut 条目内容)?;
    std::fs::write(目标文件, &条目内容)?;
    Ok(())
}

/// 轻量客户端流式下载到文件（大文件不驻留内存）
fn 下载到(地址: &str, 目标文件: &Path) -> anyhow::Result<()> {
    let 响应值 = ureq::get(地址)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(600)))
        .build()
        .call()
        .map_err(|错误值| anyhow::anyhow!("下载失败: {错误值}"))?;
    let mut 读取器 = 响应值.into_body().into_reader();
    let mut 内容 = Vec::new();
    读取器.read_to_end(&mut 内容)?;
    std::fs::write(目标文件, &内容)?;
    Ok(())
}

/// 轻量客户端下载文本内容（如官方摘要校验文件）
fn 下载文本(地址: &str) -> anyhow::Result<String> {
    let 响应值 = ureq::get(地址)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build()
        .call()
        .map_err(|错误值| anyhow::anyhow!("下载失败: {错误值}"))?;
    响应值
        .into_body()
        .read_to_string()
        .map_err(|错误值| anyhow::anyhow!("读取失败: {错误值}"))
}

/// 计算文件摘要（十六进制小写）
fn 文件摘要(路径值: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let 数据项 = std::fs::read(路径值)?;
    let mut 摘要器 = Sha256::new();
    摘要器.update(&数据项);
    Ok(format!("{:x}", 摘要器.finalize()))
}

/// 解压归档包到目标目录（归档库默认拒绝 `..` 与绝对路径，路径安全）
fn 解压归档包(归档路径: &Path, 目标目录: &Path) -> anyhow::Result<()> {
    let 压缩数据 = std::fs::read(归档路径)?;
    let 解压流 = flate2::read::GzDecoder::new(std::io::Cursor::new(压缩数据));
    let mut 归档 = tar::Archive::new(解压流);
    归档.unpack(目标目录)?;
    Ok(())
}

/// 复制目录内全部文件到目标目录（`覆盖` 为假时跳过已存在文件，避免覆盖冲突）
fn 复制程序目录(
    来源目录: &Path, 目标目录: &Path, 覆盖: bool
) -> anyhow::Result<()> {
    for 条目值 in std::fs::read_dir(来源目录)? {
        let 条目值 = 条目值?;
        let 目标路径 = 目标目录.join(条目值.file_name());
        if 目标路径.is_file() && !覆盖 {
            continue;
        }
        std::fs::copy(条目值.path(), &目标路径)?;
    }
    Ok(())
}

/// 环境诊断：rzc / 内置工具链 / 环境工具链 / 版本对比
pub fn 诊断环境() -> anyhow::Result<()> {
    use i18n_rust_engine::工具链::{工具链程序目录, 已安装版本, 锁定版本};

    println!("=== rzc doctor ===");
    println!("rzc: {}", env!("CARGO_PKG_VERSION"));

    let 程序目录 = 工具链程序目录();
    let 内置版本 = 已安装版本();
    match &内置版本 {
        Some(版本值) => println!("内置工具链: {}（{}）", 版本值, 程序目录.display()),
        None => {
            println!(
                "内置工具链: 未安装（可执行 rzc install toolchain 一键安装，脱离工具链管理器）"
            )
        }
    }

    for 行值 in 组件状态行(&程序目录) {
        println!("{行值}");
    }

    // 编辑器链路：语言服务器版本可能落后于 rzc（旧包仓库版本或旧拷贝）
    if let Some(来源项) = 定位组件(语言服务程序名, &程序目录) {
        let 版本值 = Command::new(来源项.取路径())
            .arg("--version")
            .output()
            .ok()
            .map(|输出值| String::from_utf8_lossy(&输出值.stdout).to_string());
        if let Some(行值) = 语言服务版本行(版本值.as_deref(), env!("CARGO_PKG_VERSION"))
        {
            println!("{行值}");
        }
    }

    // 语言包健康检查：遮蔽/备份遗留/身份不符只在用户环境发生，静态门检查不出
    let 语言根 = 全局语言目录();
    let 健康行们 = 语言包健康行(&扫描全局语言包(), &全局语言包子目录名());
    println!();
    println!("语言包: {}", 语言根.display());
    if 健康行们.is_empty() {
        println!("  全局语言包: 无遮蔽风险（未安装用户级语言包，或均与内置不冲突）");
    } else {
        for 行值 in 健康行们 {
            println!("{行值}");
        }
    }

    if let Some(版本值) = 内置版本.as_deref()
        && 版本值 != 锁定版本
    {
        println!(
            "提示: 内置工具链 {} 与锁定版本 {} 不一致（rzc install toolchain --force 更新）",
            版本值, 锁定版本
        );
    }
    println!(
        "升级内置工具链：rzc install toolchain --version <新版本> --force（当前锁定 {}）",
        锁定版本
    );

    // 项目入口自检：在项目里跑诊断命令时，把「直接构建撞无目标」
    // 这条墙提前看得见（判定与 rzc run/check 的阻断同源）
    let 项目根 = std::env::current_dir()
        .ok()
        .and_then(|当前值| crate::向上查找项目根(&当前值));
    println!();
    match 项目根 {
        Some(根路径) => {
            println!("项目: {}", 根路径.display());
            let 健康行们 = 项目入口健康行(&根路径, &全部可用扩展名());
            if 健康行们.is_empty() {
                println!("  包管理器可读入口: 正常");
            } else {
                for 行值 in 健康行们 {
                    println!("{行值}");
                }
            }
        }
        None => println!("项目: 当前目录不在包管理项目内（未找到 Cargo.toml，跳过入口自检）"),
    }
    Ok(())
}

/// 项目入口自检状态行（纯函数，便于测试）
///
/// 把「直接构建撞清单无目标」这条墙从「读得懂编译器原文」降级为「提前看得见」：
/// 在项目里跑 `rzc doctor` 就能看见缺哪个入口产物、该跑哪条命令补上。
/// 构建目标判定沿用 [`包::有货物目标`]，与 `rzc run`/`rzc check` 的阻断同一口径，不许分叉。
///
/// 报三类：
/// - 包管理器读不到任何构建目标：点名源码目录下的方言源文件，给出该先跑的
///   `rzc check <入口候选>`；源码目录里连方言文件都没有时提示 `rzc init`；
/// - 主程序与库同项目只生成了主程序产物：库源在、库产物不在、
///   清单又没声明库目标，提示用库之前先检查一次库；
/// - 工作区根：虚拟清单本就没有构建目标，跳过而不报「缺入口」。
fn 项目入口健康行(项目根: &Path, 扩展名们: &[String]) -> Vec<String> {
    let mut 行们 = Vec::new();
    if !项目根.join("Cargo.toml").is_file() {
        return 行们;
    }
    if 清单为工作区(项目根) {
        行们.push(
            "  这是工作区根（虚拟清单，本身没有构建目标）：进成员目录再跑 doctor".to_string(),
        );
        return 行们;
    }
    let 源文件们 = 源目录方言源(项目根, 扩展名们);
    if !crate::有货物目标(项目根) {
        if 源文件们.is_empty() {
            行们.push("  ⚠ 包管理器读不到任何构建目标，源码目录下也没有方言源文件".to_string());
            行们.push("    → rzc init <项目名> 建骨架，或写一个 src/main.<语言>".to_string());
            return 行们;
        }
        行们.push(format!(
            "  ⚠ 项目有 {} 个方言源文件，但包管理器读不到任何构建目标（src/main.rs、src/lib.rs 都不存在）",
            源文件们.len()
        ));
        行们.push(format!(
            "    方言源文件: {}",
            源文件们
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
        let 入口值 = 入口候选(&源文件们);
        行们.push(format!(
            "    → 先跑 `rzc check {入口值}` 生成入口产物；此刻直接构建会报 `no targets specified in the manifest`"
        ));
        return 行们;
    }
    // 库源在、库产物不在，且清单没声明库目标（含库路径时包管理器直接认库目标）
    let Some(库源) = 源文件们.iter().find(|相对值| {
        Path::new(相对值)
            .file_stem()
            .is_some_and(|干值| 干值 == "lib")
    }) else {
        return 行们;
    };
    if !项目根.join("src/lib.rs").is_file() && !清单声明库(项目根) {
        行们.push(format!(
            "  ⚠ {库源} 还没有入口产物 src/lib.rs（入口产物只由被请求的文件写）"
        ));
        行们.push(format!(
            "    → 主程序 `使用` 这个库之前，先跑一次 `rzc check {库源}`"
        ));
    }
    行们
}

/// 源码目录下的方言源文件（相对项目根路径，字典序；只认已装语言包的扩展名）
fn 源目录方言源(项目根: &Path, 扩展名们: &[String]) -> Vec<String> {
    let mut 已找到 = Vec::new();
    收集方言源(&项目根.join("src"), 项目根, 扩展名们, &mut 已找到);
    已找到.sort();
    已找到
}

fn 收集方言源(
    目录值: &Path, 根路径: &Path, 扩展名们: &[String], 已找到: &mut Vec<String>
) {
    let Ok(目录读) = std::fs::read_dir(目录值) else {
        return;
    };
    for 条目值 in 目录读.flatten() {
        let 路径值 = 条目值.path();
        if 路径值.is_dir() {
            收集方言源(&路径值, 根路径, 扩展名们, 已找到);
            continue;
        }
        let 是方言 = 路径值
            .extension()
            .and_then(|后缀| 后缀.to_str())
            .is_some_and(|后缀| 扩展名们.iter().any(|已知值| 已知值 == 后缀));
        if !是方言 {
            continue;
        }
        if let Ok(相对值) = 路径值.strip_prefix(根路径) {
            已找到.push(相对值.display().to_string().replace('\\', "/"));
        }
    }
}

/// 无构建目标时建议先跑的入口候选：主程序 → 库 → 源码目录直属的第一个
/// 方言文件（母语词干入口如 `src/主函数.zh` 走最后一条，也正好是该先跑的那条命令）
fn 入口候选(源文件们: &[String]) -> String {
    let 取词干 = |相对值: &str| {
        Path::new(相对值)
            .file_stem()
            .map(|干值| 干值.to_string_lossy().to_string())
    };
    ["main", "lib"]
        .iter()
        .find_map(|目标值| {
            源文件们
                .iter()
                .find(|相对值| 取词干(相对值).is_some_and(|干值| 干值 == *目标值))
                .cloned()
        })
        .or_else(|| {
            源文件们
                .iter()
                .find(|相对值| 相对值.matches('/').count() == 1)
                .cloned()
        })
        .or_else(|| 源文件们.first().cloned())
        .unwrap_or_else(|| "src/main.rs".to_string())
}

/// 清单是否为工作区根（虚拟清单含工作区节）
fn 清单为工作区(项目根: &Path) -> bool {
    清单值(项目根).is_some_and(|值项| 值项.get("workspace").is_some())
}

/// 清单是否声明了库目标（含库路径：此时库目标由包管理器直接认，不要求产物名）
fn 清单声明库(项目根: &Path) -> bool {
    清单值(项目根).is_some_and(|值项| 值项.get("lib").is_some())
}

fn 清单值(项目根: &Path) -> Option<toml::Value> {
    let 文本串 = std::fs::read_to_string(项目根.join("Cargo.toml")).ok()?;
    toml::from_str::<toml::Value>(&文本串).ok()
}

/// 语言包健康检查状态行（纯函数，便于测试）
///
/// `包列表` 为全局目录下的语言包候选（含 keywords.toml 的子目录），
/// `子目录们` 为该目录下全部子目录名。返回空表示无异常。
///
/// 报三类问题（均只在用户环境发生，静态门检查不出）：
/// - 目录名与内置代码相同 → 用户级副本遮蔽内置表（内置表优先级最低）
/// - 目录名形如备份（以备份后缀结尾或含过期标记）→ 仍被当语言包吃掉并接管扩展名
/// - 正常包（非备份）目录名与声明的扩展名不一致 → 提示确认是否有意为之
///
/// 第三方自研包（目录名与扩展名相同）不属异常，不报警。
fn 语言包健康行(包列表: &[全局语言包], 子目录们: &[String]) -> Vec<String> {
    let mut 行们 = Vec::new();
    // 1. 遮蔽内置表（同名的正常目录）
    for 包项 in 包列表 {
        if !crate::内置语言::有内置语言包(&包项.目录名) || 是备份名(&包项.目录名)
        {
            continue;
        }
        let 内置版本 = 获取内置元数据(&包项.目录名)
            .map(|元项| 元项.版本)
            .unwrap_or_else(|| "?".to_string());
        let 用户版本 = 包项
            .元数据
            .as_ref()
            .map(|元项| 元项.版本.clone())
            .unwrap_or_else(|| "?".to_string());
        行们.push(format!(
            "  ⚠ 全局包 {}（v{}）遮蔽内置语言包（v{}），当前 .{} 源码走全局副本",
            包项.目录名,
            用户版本,
            内置版本,
            语言包扩展名(包项).unwrap_or_else(|| 包项.目录名.clone())
        ));
        行们.push(
            "    → 升级 rzc 后诊断文案仍是旧的，多半是此因：rzc lang remove <码>，或把该目录移出 lang-packs/"
                .to_string(),
        );
    }
    // 2. 备份/改名遗留：无论是否含 keywords.toml，留在 lang-packs/ 内都可能被吃掉
    for 名称值 in 子目录们 {
        if !是备份名(名称值) {
            continue;
        }
        if 包列表.iter().any(|包项| &包项.目录名 == 名称值) {
            行们.push(format!(
                "  ⚠ 备份目录 {} 仍被当作语言包吃掉（语言代码取目录名）：请移出 {}",
                名称值,
                全局语言目录().display()
            ));
        } else {
            行们.push(format!(
                "  ⚠ 子目录 {} 不具备 keywords.toml（未作为语言包加载）：备份建议移出 lang-packs/",
                名称值
            ));
        }
    }
    // 3. 目录名与声明扩展名不符（排除备份与自研同名包）
    for 包项 in 包列表 {
        if crate::内置语言::有内置语言包(&包项.目录名) || 是备份名(&包项.目录名)
        {
            continue;
        }
        if let Some(后缀值) = 语言包扩展名(包项)
            && 后缀值 != 包项.目录名
        {
            行们.push(format!(
                "  ⚠ 全局包目录名 {} 与其声明的扩展名 {} 不一致（语言代码取目录名，当前 .{} 源码按声明接管）",
                包项.目录名,
                后缀值,
                后缀值
            ));
        }
    }
    行们
}

/// 备份/改名遗留目录名特征：以备份后缀结尾或含过期标记
fn 是备份名(名称值: &str) -> bool {
    名称值.ends_with(".bak") || 名称值.contains(".stale")
}

/// 取全局包生效的扩展名：语言信息声明优先，缺失时回退静态映射推断
fn 语言包扩展名(包项: &全局语言包) -> Option<String> {
    包项
        .元数据
        .as_ref()
        .map(|元项| 元项.扩展名.clone())
        .or_else(|| {
            静态扩展名映射()
                .into_iter()
                .find(|(_, 代码值)| 代码值 == &包项.目录名)
                .map(|(后缀, _)| 后缀)
        })
}

/// 组件来源：内置工具链目录优先，其次环境变量路径
enum 组件来源 {
    内置(PathBuf),
    路径来源(PathBuf),
}

impl 组件来源 {
    fn 取路径(&self) -> &Path {
        match self {
            Self::内置(值项) | Self::路径来源(值项) => 值项,
        }
    }
}

/// 定位组件实际使用的可执行文件（内置目录优先，其次环境变量路径）
fn 定位组件(名称值: &str, 程序目录: &Path) -> Option<组件来源> {
    let 内置路径 = 程序目录.join(format!("{名称值}{程序后缀}"));
    if 内置路径.is_file() {
        return Some(组件来源::内置(内置路径));
    }
    i18n_rust_engine::工具链::查找工具链程序(名称值).map(组件来源::路径来源)
}

/// 各组件来源状态行（纯函数，便于测试）：编译器内置、包管理器来自环境变量路径等
///
/// 除编译链路外，还覆盖编辑器链路的两个必需件：语言分析器（语言服务代理
/// 转发目标）与语言服务器（编辑器扩展启动的语言服务器）。两者缺失
/// 在编辑器里只表现为服务初始化失败而拿不到原因，
/// `rzc doctor` 是唯一的自查入口，因此缺失时必须给出可直接复制的安装命令。
fn 组件状态行(程序目录: &Path) -> Vec<String> {
    const 组件清单: [(&str, &str); 4] = [
        ("rustc", "rzc install toolchain（或 rustup default stable）"),
        ("cargo", "rzc install toolchain（或 rustup default stable）"),
        (
            "rust-analyzer",
            "rustup component add rust-analyzer（或 rzc install toolchain 装内置工具链）",
        ),
        (
            "i18n-rust-lsp",
            "rzc install lsp（或 cargo install i18n-rust-lsp）",
        ),
    ];
    let mut 行们 = Vec::new();
    for (组件名, 修复命令) in 组件清单 {
        let 状态文本 = match 定位组件(组件名, 程序目录) {
            Some(组件来源::内置(路径值)) => format!("内置（{}）", 路径值.display()),
            Some(组件来源::路径来源(路径值)) => {
                format!("环境路径（{}）", 路径值.display())
            }
            None => {
                行们.push(format!("{}: 未找到", 组件名));
                行们.push(format!("  → 安装：{}", 修复命令));
                continue;
            }
        };
        行们.push(format!("{}: {}", 组件名, 状态文本));
    }
    行们
}

/// 语言服务器与 rzc 的版本一致性行（纯函数，便于测试）
///
/// 两者是两条独立安装链路：服务器可执行文件可能来自更早的包仓库版本或
/// 手动拷贝的旧文件，转译规则与诊断文案都是旧的。版本拿不到时不给结论
/// （返回空值，避免在旧版无版本输出时误报）。
fn 语言服务版本行(语言服务版本: Option<&str>, 程序版本: &str) -> Option<String> {
    // 命令行风格输出可能是「包名 版本号」，取末个字段作版本号
    let 版本值 = 语言服务版本
        .map(|值项| 值项.trim())
        .filter(|值项| !值项.is_empty())?
        .split_whitespace()
        .last()?;
    if 版本值 == 程序版本 {
        Some(format!("  ✓ 语言服务器与 rzc 版本一致（{}）", 版本值))
    } else {
        Some(format!(
            "  ⚠ 语言服务器 {} 与 rzc {} 不一致：转译规则与诊断文案仍是旧的 → rzc install lsp --force 更新",
            版本值, 程序版本
        ))
    }
}

/// 双击可执行文件（或终端无参数运行）时的环境安装向导：
/// 逐项检查组件状态，标注「已就绪」或「缺失 + 解决命令」。
pub fn 显示安装向导() {
    use i18n_rust_engine::工具链::{工具链程序目录, 查找工具链程序};
    let 后缀值 = std::env::consts::EXE_SUFFIX;

    println!("════════════ i18n-rust 环境安装向导 ════════════");
    println!("rzc 本体          v{}（已就绪）", env!("CARGO_PKG_VERSION"));
    println!();
    println!("  💡 也可下载「离线发布包」：压缩包已含语言分析器 + 语言服务器 +");
    println!("     语言包与教程，适合无网络环境（离线包由本地构建，手动分发）。");
    println!();

    // 1. 内置工具链（编译器 / 包管理器 / 语言分析器）
    println!("【第 1 步】编译器与工具链（rzc install toolchain 一键安装）");
    for (组件名, 安装命令) in [
        ("rustc", "rzc install toolchain"),
        ("cargo", "rzc install toolchain"),
        ("rust-analyzer", "rzc install toolchain --force"),
    ] {
        let 内置路径 = 工具链程序目录().join(format!("{组件名}{后缀值}"));
        let (标记, 详情) = if 内置路径.is_file() {
            ("✓ 已就绪", format!("内置（{}）", 内置路径.display()))
        } else if let Some(路径项) = 查找工具链程序(组件名) {
            ("✓ 已就绪", format!("环境路径（{}）", 路径项.display()))
        } else {
            (
                "✗ 缺失",
                format!("未找到 → 运行: {}（需联网，约 300MB）", 安装命令),
            )
        };
        println!("  [{}] {:<14} {}", 标记, 组件名, 详情);
    }
    println!();

    // 2. 语言服务器（编辑器补全/诊断后端）
    println!("【第 2 步】语言服务器 i18n-rust-lsp（rzc install lsp 一键安装）");
    if let Some(路径项) = 查找工具链程序("i18n-rust-lsp") {
        println!("  [✓ 已就绪] {}", 路径项.display());
        if let Ok(输出值) = std::process::Command::new(&路径项)
            .arg("--version")
            .output()
        {
            let 版本值 = String::from_utf8_lossy(&输出值.stdout).trim().to_string();
            if !版本值.is_empty() {
                println!("              版本 {版本值}");
            }
        }
    } else {
        println!(
            "  [✗ 缺失] 未找到 i18n-rust-lsp{} → 运行: rzc install lsp",
            后缀值
        );
    }
    println!();

    // 3. 编辑器扩展
    println!("【第 3 步】编辑器扩展（安装包手动安装，未发布到扩展商城）");
    println!("  [? 手动] 从发布页下载 i18n-rust-<版本> 安装包");
    println!("           → 编辑器命令面板（Ctrl+Shift+P）→ 从安装包安装并选择该文件");
    println!();

    // 4. 环境变量（可选）
    println!("【第 4 步】环境变量（可选，一般无需配置）");
    println!("  RZ_LANG_DIR        指定语言包目录（默认内置）");
    println!("  RUST_ANALYZER_PATH 指定语言分析器路径（自动检测失败时）");
    println!("  编辑器设置 i18n-rust.serverPath 可指定语言服务器路径");
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
    println!("详细教程见《开篇：这本书怎么用》与《第一章：用好编辑器扩展》。");
}

#[cfg(test)]
mod 母语测试 {
    use super::*;
    use std::fs;

    #[test]
    fn 语言服务版本一致() {
        assert_eq!(
            核对语言服务版本(Some("0.5.5".to_string()), "0.5.5"),
            版本核对::一致
        );
    }

    #[test]
    fn 语言服务版本不一致() {
        assert_eq!(
            核对语言服务版本(Some("0.5.3".to_string()), "0.5.5"),
            版本核对::不一致("0.5.3".to_string())
        );
    }

    #[test]
    fn 语言服务版本未知() {
        assert_eq!(核对语言服务版本(None, "0.5.5"), 版本核对::未知);
    }

    #[test]
    fn 目标三元组支持当前平台() {
        // 当前平台必须能被识别（不恐慌）
        let 三元组 = 目标三元组().expect("当前平台应受支持");
        assert!(!三元组.is_empty());
    }

    /// 摘要计算正确性（空文件与已知内容）
    #[test]
    fn 文件摘要已知内容() {
        let 目录值 = tempfile::tempdir().unwrap();
        let 文件路径 = 目录值.path().join("a.txt");
        std::fs::write(&文件路径, b"hello").unwrap();
        // 该短串的摘要（标准已知值）
        assert_eq!(
            文件摘要(&文件路径).unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    /// 内置工具链存在时状态行标记为内置
    #[test]
    fn 组件状态行内置标记() {
        let 目录值 = tempfile::tempdir().unwrap();
        let 程序目录 = 目录值.path();
        std::fs::write(程序目录.join(format!("rustc{程序后缀}")), b"x").unwrap();
        let 行们 = 组件状态行(程序目录);
        assert!(行们.iter().any(|行值| 行值.starts_with("rustc: 内置")));
        // 其余组件未内置时标记环境路径或未找到（不恐慌）
        assert!(行们.iter().any(|行值| 行值.starts_with("cargo:")));
        assert!(行们.iter().any(|行值| 行值.starts_with("rust-analyzer:")));
    }

    /// 诊断覆盖编辑器链路：语言服务器必须有状态行，
    /// 且任何「未找到」行后面紧跟一条可复制的安装命令
    #[test]
    fn 组件状态行覆盖编辑器链路() {
        let 目录值 = tempfile::tempdir().unwrap();
        let 行们 = 组件状态行(目录值.path());
        assert!(
            行们.iter().any(|行值| 行值.starts_with("i18n-rust-lsp:")),
            "{行们:?}"
        );
        for (序号, 行值) in 行们.iter().enumerate() {
            if 行值.contains("未找到") {
                // 下一行必须是可直接复制的安装命令（三种来源）
                let 下一行 = 行们
                    .get(序号 + 1)
                    .map(|值项| 值项.as_str())
                    .unwrap_or_default();
                assert!(
                    下一行.starts_with("  → 安装：") && 下一行.contains("install"),
                    "「{}」后缺少安装提示：{:?}",
                    行值,
                    下一行
                );
            }
        }
    }

    /// 语言服务器版本漂移提示：一致时无警告，落后时给更新命令
    #[test]
    fn 语言服务版本行标记漂移() {
        let 相同行 = 语言服务版本行(Some("0.8.11"), "0.8.11").unwrap();
        assert_eq!(相同行, "  ✓ 语言服务器与 rzc 版本一致（0.8.11）");
        // 带包名前缀的输出同样能比对
        let 漂移行 = 语言服务版本行(Some("i18n-rust-lsp 0.8.2"), "0.8.11").unwrap();
        assert!(漂移行.contains("0.8.2"), "{漂移行}");
        assert!(漂移行.contains("rzc install lsp --force"), "{漂移行}");
        // 旧版无版本输出：不给结论，避免误报
        assert!(语言服务版本行(None, "0.8.11").is_none());
        assert!(语言服务版本行(Some("  \n"), "0.8.11").is_none());
    }

    /// 构造全局语言包候选条目（测试辅助）
    fn 全局包(
        目录名值: &str, 后缀: Option<&str>, 版本值: Option<&str>
    ) -> 全局语言包 {
        use crate::语言管理::语言元数据;
        全局语言包 {
            目录名: 目录名值.to_string(),
            元数据: 后缀.map(|后缀值| 语言元数据 {
                名称: "测试".to_string(),
                扩展名: 后缀值.to_string(),
                版本: 版本值.unwrap_or("0.1").to_string(),
            }),
        }
    }

    /// 全局同名包遮蔽内置表：须报警并给出解除命令
    #[test]
    fn 语言包健康遮蔽() {
        let 包列表 = vec![全局包("zh", Some("zh"), Some("1.0"))];
        let 行们 = 语言包健康行(&包列表, &["zh".to_string()]);
        assert!(
            行们.iter().any(|行值| 行值.contains("遮蔽内置语言包")),
            "{行们:?}"
        );
        assert!(
            行们.iter().any(|行值| 行值.contains("rzc lang remove")),
            "应给出解除命令: {行们:?}"
        );
    }

    /// 备份目录改名后仍被当语言包吃掉：须报警（即上一步实际踩到的坑）
    #[test]
    fn 语言包健康备份遗留() {
        let 名称值 = "zh.stale-20260919.bak";
        let 包列表 = vec![全局包(名称值, Some("zh"), None)];
        let 行们 = 语言包健康行(&包列表, &[名称值.to_string()]);
        assert!(
            行们.iter().any(|行值| 行值.contains("仍被当作语言包吃掉")),
            "{行们:?}"
        );
        // 遮蔽行只针对目录名与内置代码完全相同的包，备份不当作遮蔽报
        assert!(
            !行们.iter().any(|行值| 行值.contains("遮蔽内置语言包")),
            "{行们:?}"
        );
    }

    /// 备份目录不具备 keywords.toml（未被加载）也须提示移出
    #[test]
    fn 语言包健康未加载备份() {
        let 名称值 = "ja.backup-20260919.stale";
        let 行们 = 语言包健康行(&[], &[名称值.to_string()]);
        assert!(
            行们
                .iter()
                .any(|行值| 行值.contains(名称值) && 行值.contains("keywords.toml")),
            "{行们:?}"
        );
    }

    /// 第三方自研包（目录名与扩展名一致）不应被误报
    #[test]
    fn 语言包健康自研包安静() {
        let 包列表 = vec![全局包("vi", Some("vi"), Some("1.0"))];
        assert!(
            语言包健康行(&包列表, &["vi".to_string()]).is_empty(),
            "自研包不应报警"
        );
    }

    /// 自研包目录名与声明扩展名不符：提示身份不一致
    #[test]
    fn 语言包健康身份不符() {
        let 包列表 = vec![全局包("mypack", Some("vi"), None)];
        let 行们 = 语言包健康行(&包列表, &["mypack".to_string()]);
        assert!(行们.iter().any(|行值| 行值.contains("不一致")), "{行们:?}");
    }

    /// 无全局包时不报任何异常
    #[test]
    fn 语言包健康空清单() {
        assert!(语言包健康行(&[], &[]).is_empty());
    }

    /// 写一个全局语言包目录（keywords.toml + 可选语言信息），供端到端注入。
    /// 后缀为空时不写语言信息（模拟扩展名靠静态映射推断的旧包）。
    fn 写全局包(
        根路径: &Path,
        目录名值: &str,
        后缀: Option<&str>,
        版本值: Option<&str>,
        带关键词: bool,
    ) {
        let 目录值 = 根路径.join(目录名值);
        std::fs::create_dir_all(&目录值).unwrap();
        if 带关键词 {
            std::fs::write(目录值.join("keywords.toml"), "# 占位\n").unwrap();
        }
        if let Some(后缀值) = 后缀 {
            let 版本文本 = 版本值.unwrap_or("1.0");
            std::fs::write(
                目录值.join("lang_info.toml"),
                format!("[\"语言包\"]\n\"名称\" = \"{目录名值}\"\n\"扩展名\" = \"{后缀值}\"\n\"版本\" = \"{版本文本}\"\n"),
            )
            .unwrap();
        }
    }

    /// 端到端故障注入：真实文件系统扫描 → 健康行，逐场景核应报/不误报。
    ///
    /// 上面几例只喂合成切片，验不出 [`扫描全局语言包`]/[`全局语言包子目录名`]
    /// 对真实目录（含/不含 keywords.toml、语言信息解析）的处理是否与判定假设一致；
    /// 这里把六类只在用户环境发生的坑一次性注入语言目录指向的临时目录，
    /// 走真实的扫描→健康行链路验证诊断命令能一次查全且不误报。
    #[test]
    fn 诊断语言包端到端注入() {
        let _锁 = crate::语言管理::母语测试::环境锁();
        let 临时根 = tempfile::tempdir().unwrap();
        let 全局目录 = 临时根.path().join("global");
        std::fs::create_dir_all(&全局目录).unwrap();
        // ① 同名遮蔽内置表（版本不同）
        写全局包(&全局目录, "zh", Some("zh"), Some("9.9"), true);
        // ② 改名遗留的备份且仍被当包吃掉（含 keywords.toml）
        写全局包(&全局目录, "de.stale-20260101.bak", Some("de"), None, true);
        // ③ 不具备 keywords.toml 的备份（未被加载，仍须提示移出）
        写全局包(&全局目录, "ja.backup.stale", Some("ja"), None, false);
        // ④ 合法自研包（目录名即扩展名）：不得误报
        写全局包(&全局目录, "vi", Some("vi"), Some("1.0"), true);
        // ⑤ 目录名与声明扩展名不符的自研包：应报身份不一致
        写全局包(&全局目录, "mypack", Some("fr"), None, true);

        unsafe { std::env::set_var("RZ_LANG_DIR", &全局目录) };
        let 包列表 = 扫描全局语言包();
        let 子目录们 = 全局语言包子目录名();
        let 健康行们 = 语言包健康行(&包列表, &子目录们);
        unsafe { std::env::remove_var("RZ_LANG_DIR") };

        let 合并文本 = 健康行们.join("\n");
        // ① 同名遮蔽：报警 + 给出解除命令
        assert!(
            合并文本.contains("遮蔽内置语言包") && 合并文本.contains("rzc lang remove"),
            "{合并文本}"
        );
        // ② 备份仍被吃掉
        assert!(
            健康行们
                .iter()
                .any(|行值| 行值.contains("de.stale-20260101.bak")
                    && 行值.contains("仍被当作语言包吃掉")),
            "{合并文本}"
        );
        // ③ 未加载备份：提示 keywords.toml 缺失
        assert!(
            健康行们
                .iter()
                .any(|行值| 行值.contains("ja.backup.stale") && 行值.contains("keywords.toml")),
            "{合并文本}"
        );
        // ⑤ 身份不一致
        assert!(
            健康行们
                .iter()
                .any(|行值| 行值.contains("mypack") && 行值.contains("不一致")),
            "{合并文本}"
        );
        // ④ 合法自研包不得出现在任何报警行
        assert!(
            !健康行们.iter().any(|行值| 行值.contains("vi（")
                || 行值.contains("  vi ")
                || (行值.contains("vi") && 行值.contains("不一致"))),
            "合法自研包 vi 不应被误报：{合并文本}"
        );
    }

    /// 文件摘要：已知向量（空串/短串）；文件缺失报错
    #[test]
    fn 文件摘要已知向量() {
        let 临时 = tempfile::tempdir().unwrap();
        let 空路径 = 临时.path().join("empty");
        fs::write(&空路径, b"").unwrap();
        assert_eq!(
            文件摘要(&空路径).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let 短串路径 = 临时.path().join("abc");
        fs::write(&短串路径, b"abc").unwrap();
        assert_eq!(
            文件摘要(&短串路径).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(文件摘要(&临时.path().join("absent")).is_err());
    }

    /// 复制程序目录：默认跳过已存在；覆盖模式替换旧文件
    #[test]
    fn 复制程序目录可覆盖已存在() {
        let 临时 = tempfile::tempdir().unwrap();
        let 来源目录 = 临时.path().join("from");
        let 目标目录 = 临时.path().join("to");
        fs::create_dir_all(&来源目录).unwrap();
        fs::create_dir_all(&目标目录).unwrap();
        fs::write(来源目录.join("a"), b"new-a").unwrap();
        fs::write(来源目录.join("b"), b"new-b").unwrap();
        fs::write(目标目录.join("a"), b"keep-old").unwrap();
        复制程序目录(&来源目录, &目标目录, false).unwrap();
        assert_eq!(fs::read(目标目录.join("a")).unwrap(), b"keep-old");
        assert_eq!(fs::read(目标目录.join("b")).unwrap(), b"new-b");
        // 覆盖模式：旧文件被替换（强制安装修复被篡改的工具链）
        复制程序目录(&来源目录, &目标目录, true).unwrap();
        assert_eq!(fs::read(目标目录.join("a")).unwrap(), b"new-a");
        // 源目录缺失报错（调用方负责确保前置条件）
        assert!(复制程序目录(&临时.path().join("absent"), &目标目录, false).is_err());
    }

    /// 提取压缩包条目：取压缩包第一个条目原样落盘（Windows 单文件资产路径）
    #[test]
    fn 提取压缩包条目往返() {
        use std::io::Write as _;
        use zip::write::FileOptions;
        let 临时 = tempfile::tempdir().unwrap();
        let 压缩包路径 = 临时.path().join("asset.zip");
        let 负载 = b"rust-analyzer binary \x00\x01\x02";
        {
            let mut 写入器 = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            写入器
                .start_file("rust-analyzer", FileOptions::<()>::default())
                .unwrap();
            写入器.write_all(负载).unwrap();
            fs::write(压缩包路径.as_path(), 写入器.finish().unwrap().into_inner()).unwrap();
        }
        let 目标路径 = 临时.path().join("out/rust-analyzer");
        fs::create_dir_all(目标路径.parent().unwrap()).unwrap();
        提取压缩包条目(&压缩包路径, &目标路径).unwrap();
        assert_eq!(fs::read(&目标路径).unwrap(), 负载);
        // 损坏/缺失压缩包 → 报错而非写出半截文件
        let 坏路径 = 临时.path().join("bad.zip");
        fs::write(&坏路径, b"not a zip").unwrap();
        assert!(提取压缩包条目(&坏路径, &临时.path().join("out2")).is_err());
    }

    /// 解压归档包：归档内文件按相对路径还原到目标目录
    #[test]
    fn 解压归档包往返() {
        let 临时 = tempfile::tempdir().unwrap();
        let 归档路径 = 临时.path().join("bin.tar.gz");
        let 负载 = b"#!/bin/sh\necho hi\n";
        {
            let 编码器 = flate2::write::GzEncoder::new(
                std::io::Cursor::new(Vec::new()),
                flate2::Compression::default(),
            );
            let mut 构建器 = tar::Builder::new(编码器);
            let mut 头部 = tar::Header::new_gnu();
            头部.set_path("bin/rzc").unwrap();
            头部.set_size(负载.len() as u64);
            头部.set_mode(0o755);
            头部.set_cksum();
            构建器.append(&头部, &负载[..]).unwrap();
            let 压缩数据 = 构建器.into_inner().unwrap().finish().unwrap().into_inner();
            fs::write(&归档路径, &压缩数据).unwrap();
        }
        let 目标目录 = 临时.path().join("dest");
        解压归档包(&归档路径, &目标目录).unwrap();
        assert_eq!(fs::read(目标目录.join("bin/rzc")).unwrap(), 负载);
    }

    /// 项目自检用例的公共脚手架：写清单 + 源码目录下的文件，返回项目根
    fn 建项目(根路径: &Path, 清单文本: &str, 文件们: &[&str]) {
        std::fs::write(根路径.join("Cargo.toml"), 清单文本).unwrap();
        std::fs::create_dir_all(根路径.join("src")).unwrap();
        for 相对值 in 文件们 {
            let 路径值 = 根路径.join(相对值);
            std::fs::create_dir_all(路径值.parent().unwrap()).unwrap();
            std::fs::write(&路径值, "// 占位\n").unwrap();
        }
    }

    const 中英扩展名: &[&str] = &["zh", "ja"];

    fn 健康(根路径: &Path) -> Vec<String> {
        项目入口健康行(
            根路径,
            &中英扩展名
                .iter()
                .map(|文本值| 文本值.to_string())
                .collect::<Vec<_>>(),
        )
    }

    /// 有方言源文件却没有入口产物：点名缺构建目标，并给出能走通的那条检查命令
    #[test]
    fn 项目入口健康指向检查命令() {
        let 目录值 = tempfile::tempdir().unwrap();
        建项目(
            目录值.path(),
            "[package]\nname=\"p\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
            &["src/main.zh", "src/餐具库.zh"],
        );
        let 行们 = 健康(目录值.path());
        let 文本串 = 行们.join("\n");
        assert!(文本串.contains("包管理器读不到任何构建目标"), "{文本串}");
        assert!(文本串.contains("2 个方言源文件"), "{文本串}");
        assert!(文本串.contains("rzc check src/main.zh"), "{文本串}");
        assert!(
            文本串.contains("no targets specified in the manifest"),
            "须写明直接构建会撞的那句原文：{文本串}"
        );
    }

    /// 入口产物齐备：一行都不报（正常项目不得被自检噪音打扰）
    #[test]
    fn 项目入口健康产物齐备安静() {
        let 目录值 = tempfile::tempdir().unwrap();
        建项目(
            目录值.path(),
            "[package]\nname=\"p\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
            &["src/main.zh", "src/main.rs"],
        );
        assert!(健康(目录值.path()).is_empty(), "{:?}", 健康(目录值.path()));
    }

    /// 主程序与库的第 ③ 条墙：库源在、库产物不在 → 提示先检查一次库
    #[test]
    fn 项目入口健康标记未建库() {
        let 目录值 = tempfile::tempdir().unwrap();
        建项目(
            目录值.path(),
            "[package]\nname=\"p\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
            &["src/main.zh", "src/main.rs", "src/lib.zh"],
        );
        let 文本串 = 健康(目录值.path()).join("\n");
        assert!(
            文本串.contains("src/lib.zh 还没有入口产物 src/lib.rs"),
            "{文本串}"
        );
        assert!(文本串.contains("rzc check src/lib.zh"), "{文本串}");
    }

    /// 清单声明了库路径时不报库产物：库目标由包管理器直接认
    #[test]
    fn 项目入口健康尊重声明库() {
        let 目录值 = tempfile::tempdir().unwrap();
        建项目(
            目录值.path(),
            "[package]\nname=\"p\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[lib]\npath=\"src/库.rs\"\n",
            &["src/main.rs", "src/lib.zh"],
        );
        assert!(健康(目录值.path()).is_empty(), "{:?}", 健康(目录值.path()));
    }

    /// 母语词干入口（`src/主函数.zh`）：建议的命令就是它，不是凭空造一个主程序产物
    #[test]
    fn 项目入口健康回退直属入口() {
        let 目录值 = tempfile::tempdir().unwrap();
        建项目(
            目录值.path(),
            "[package]\nname=\"p\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
            &["src/领域/工具.zh", "src/主函数.zh"],
        );
        let 文本串 = 健康(目录值.path()).join("\n");
        assert!(文本串.contains("rzc check src/主函数.zh"), "{文本串}");
    }

    /// 工作区根（虚拟清单）本就没有构建目标：跳过，不报「缺入口」
    #[test]
    fn 项目入口健康跳工作区根() {
        let 目录值 = tempfile::tempdir().unwrap();
        建项目(
            目录值.path(),
            "[workspace]\nmembers = [\"crates/a\"]\n",
            &[],
        );
        let 文本串 = 健康(目录值.path()).join("\n");
        assert!(文本串.contains("工作区根"), "{文本串}");
        assert!(!文本串.contains("包管理器读不到任何构建目标"), "{文本串}");
    }

    /// 没有清单的目录：整段自检沉默（诊断另有「不在项目内」一行）
    #[test]
    fn 项目入口健康项目外沉默() {
        let 目录值 = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(目录值.path().join("src/main.rs")).unwrap();
        assert!(健康(目录值.path()).is_empty());
    }

    /// 源码目录下连方言文件都没有：给出初始化命令，而不是编不出来的检查命令
    #[test]
    fn 项目入口健康无源提示初始化() {
        let 目录值 = tempfile::tempdir().unwrap();
        建项目(
            目录值.path(),
            "[package]\nname=\"p\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
            &["src/说明.txt"],
        );
        let 文本串 = 健康(目录值.path()).join("\n");
        assert!(文本串.contains("rzc init"), "{文本串}");
        assert!(!文本串.contains("rzc check"), "{文本串}");
    }

    /// 备份名识别：以备份后缀结尾或含过期段；正常目录名不误判
    #[test]
    fn 备份名规则() {
        assert!(是备份名("ja.bak"));
        assert!(是备份名("de.stale-20260101.bak"));
        assert!(是备份名("ja.backup.stale"));
        assert!(!是备份名("ja"));
        assert!(!是备份名("backup-dir"));
        assert!(!是备份名("stale"));
    }
}
