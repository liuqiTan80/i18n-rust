//! 工具链定位：内置目录 → 环境变量 → PATH 的三源查找
//!
//! 内置工具链：`rzc install toolchain` 将官方独立发行版工具链
//!（rustc/cargo/rust-analyzer）安装到 `~/.rz/toolchain/bin`，使发布包自包含、
//! 不依赖 rustup 与 PATH 配置。
//!
//! 设计（中文原生重构，非英文版的逐句对译）：
//! - 三个来源先**折算成一条候选路径序列**，存在性判定只留一处 `是否为文件`。
//!   英文版把「是否为文件」判定抄了四遍（内置、环境变量、PATH 目录、PATHEXT 后缀各一
//!   遍），每加一个来源就多一个判定分支；改成数据驱动后，新增来源只是往序列里
//!   多推一条路径，优先级由序列次序表达，不再由嵌套层次表达。
//! - Windows 后缀**读系统真实登记的 `PATHEXT`**，取代硬编码的
//!   硬编码的「.exe / .cmd / .bat / .com」后缀表与「操作系统名 == Windows」字符串比较：
//!   非 Windows 上该环境变量不存在，直接得到空序列，既不空转四轮探测，也尊重
//!   用户自定义的 PATHEXT（原实现会漏 `.MSI` 一类，又无条件试 `.COM`）。

use std::env;
use std::path::{Path, PathBuf};

/// 全局安装目录（`~/.rz`）：语言包与工具链共用一根
pub fn 安装根目录() -> PathBuf {
    let 主目录 = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(主目录).join(".rz")
}

/// 内置工具链的二进制目录（`~/.rz/toolchain/bin`）
pub fn 工具链程序目录() -> PathBuf {
    安装根目录().join("toolchain").join("bin")
}

/// 收紧目录权限：Unix 0700（仅属主可读写进入），其他平台无操作
///
/// 安装根目录与缓存目录含转译源码/工具链，多用户机器上不应被同机用户读取。
pub fn 收紧目录权限(路径值: &Path) {
    #[cfg(unix)]
    {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(路径值, Permissions::from_mode(0o700));
    }
}

/// 收紧文件权限：Unix 0600（仅属主可读写），其他平台无操作
pub fn 收紧文件权限(路径值: &Path) {
    #[cfg(unix)]
    {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(路径值, Permissions::from_mode(0o600));
    }
}

/// 创建安装根目录 `~/.rz` 并收紧为 0700（Unix）
pub fn 确保安装根目录() -> std::io::Result<()> {
    let 根目录 = 安装根目录();
    std::fs::create_dir_all(&根目录)?;
    收紧目录权限(&根目录);
    Ok(())
}

/// 补上平台可执行后缀的裸文件名（Unix 上后缀为空串，结果就是原名）
fn 拼程序名(名字: &str) -> String {
    let mut 程序名 = 名字.to_string();
    程序名.push_str(env::consts::EXE_SUFFIX);
    程序名
}

/// 组件名转「同名环境变量名」：`rust-analyzer` → `RUST_ANALYZER_PATH`
fn 环境变量名(名字: &str) -> String {
    let mut 键名 = 名字.to_uppercase().replace("-", "_");
    键名.push_str("_PATH");
    键名
}

/// PATH 扩展名序列（PATHEXT 原值按 `;` 切分，项自带前导点，如 `.EXE`）
///
/// 非 Windows 上没有该环境变量，`.转选项()` 后即空序列——平台差异由数据
/// 存在与否表达，不用 `操作系统名` 分支。
fn 路径扩展名序列() -> Vec<String> {
    env::var("PATHEXT")
        .ok()
        .map(|串| {
            串.split(';')
                .map(|项| 项.to_string())
                .collect::<Vec<String>>()
        })
        .unwrap_or_default()
}

/// 按优先级排出的候选路径：内置目录 → 同名环境变量 → PATH 逐目录（含后缀展开）
fn 候选序列(名字: &str) -> Vec<PathBuf> {
    let 程序名 = 拼程序名(名字);
    let mut 候选 = Vec::new();
    // 1. 内置工具链（`rzc install toolchain` 的落点）
    候选.push(工具链程序目录().join(&程序名));
    // 2. 同名环境变量指路（如 RUST_ANALYZER_PATH），值原样当路径用
    if let Some(指路路径) = env::var(环境变量名(名字)).ok().map(PathBuf::from) {
        候选.push(指路路径);
    }
    // 3. PATH 扫描：每个目录先试裸名，再试 PATHEXT 各后缀（后缀序列只取一次）
    if let Ok(路径串) = env::var("PATH") {
        let 后缀序列 = 路径扩展名序列();
        for 目录 in env::split_paths(&路径串) {
            候选.push(目录.join(&程序名));
            for 后缀 in &后缀序列 {
                let mut 带后缀名 = 程序名.to_string();
                带后缀名.push_str(后缀);
                候选.push(目录.join(&带后缀名));
            }
        }
    }
    候选
}

/// 查找工具链可执行文件的绝对路径：按候选序列取第一个真实存在的普通文件
///
/// 内置目录、环境变量指路、PATH 扫描三级优先级由 `候选序列` 的次序表达；
/// 全都不存在时返回无。
pub fn 查找工具链程序(名字: &str) -> Option<PathBuf> {
    候选序列(名字)
        .into_iter()
        .find(|候选路径| 候选路径.is_file())
}

/// 已安装的内置工具链版本号（如 `1.98.0`）；未安装或记录为空时返回无
pub fn 已安装版本() -> Option<String> {
    std::fs::read_to_string(安装根目录().join("toolchain").join("version.txt"))
        .ok()
        .map(|串| 串.trim().to_string())
        .filter(|串| !串.is_empty())
}

/// 当前锁定版本（与仓库 rust-toolchain.toml 对齐；后续随官方稳定版升级）
pub const 锁定版本: &str = "1.98.0";

#[cfg(test)]
mod 母语测试 {
    use super::*;
    use std::path::Path;

    /// 把路径拆成组件字符串序列，便于断言目录约定
    fn 组件串序列(待拆路径: &Path) -> Vec<String> {
        待拆路径
            .components()
            .map(|组件| 组件.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<String>>()
    }

    #[test]
    fn 目录布局符合约定() {
        // 目录结构约定：~/.rz/toolchain/bin（组件级检查，不依赖文件系统真实状态）
        let 组件们 = 组件串序列(&工具链程序目录());
        assert!(组件们.iter().any(|组件| 组件 == ".rz"));
        assert!(组件们.iter().any(|组件| 组件 == "toolchain"));
        assert!(组件们.last().is_some_and(|尾| 尾 == "bin"));
    }

    #[test]
    fn 环境变量名按组件名派生() {
        // 连字符转下划线、字母大写——三级查找的第二级全靠这个环境变量名
        assert_eq!(环境变量名("rust-analyzer"), "RUST_ANALYZER_PATH");
        assert_eq!(环境变量名("cargo"), "CARGO_PATH");
    }

    #[test]
    fn 程序名带平台后缀() {
        // Unix 上「可执行后缀」为空串，Windows 上为 .exe：拼接结果随平台一致变化
        let 期望名 = 拼程序名("rustc");
        assert!(期望名.starts_with("rustc"));
        assert_eq!(期望名.len(), "rustc".len() + env::consts::EXE_SUFFIX.len());
    }

    #[test]
    fn 候选序列首位是内置目录() {
        // 优先级是这三级查找的核心约定：内置目录必须排在最前
        let 候选们 = 候选序列("rustc");
        assert_eq!(候选们[0], 工具链程序目录().join(拼程序名("rustc")));
    }

    #[test]
    fn 扩展名序列的项自带前导点() {
        // Windows 的 PATHEXT 每项以 `.` 起头；非 Windows 得到空序列
        for 后缀 in 路径扩展名序列() {
            assert!(后缀.starts_with("."), "PATHEXT 项应自带前导点，实得 {后缀}");
        }
    }

    #[test]
    fn 不存在的程序名查不出路径() {
        // 内置、环境变量指路、PATH 扫描三级都落空：名字带 __ 前缀，不会与真实工具链撞名
        assert!(查找工具链程序("__rzc_absent_bin__").is_none());
    }
}
