// 工具链定位模块
//
// 内置工具链：`rzc install toolchain` 将官方 standalone 工具链
//（rustc/cargo/rust-analyzer）安装到 `~/.rz/toolchain/bin`，
// 使发布包自包含、不依赖 rustup 与 PATH 配置。
// 查找优先级：内置目录 → 环境变量 → PATH 扫描。

use std::path::PathBuf;

/// 全局安装目录（~/.rz）：语言包（lang-packs/）与工具链（toolchain/）共用
pub fn rz_home() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".rz")
}

/// 内置工具链 bin 目录（~/.rz/toolchain/bin）
pub fn toolchain_bin_dir() -> PathBuf {
    rz_home().join("toolchain").join("bin")
}

/// 查找工具链可执行文件的绝对路径
///
/// 优先级：
/// 1. 内置工具链目录（`rzc install toolchain` 安装的 standalone 版本）
/// 2. 同名环境变量（如 rust-analyzer 对应 RUST_ANALYZER_PATH）
/// 3. PATH 扫描（跨平台，Windows 追加 PATHEXT 后缀）
pub fn find_toolchain_bin(name: &str) -> Option<PathBuf> {
    let exe = format!("{name}{}", std::env::consts::EXE_SUFFIX);

    // 1. 内置工具链
    let builtin = toolchain_bin_dir().join(&exe);
    if builtin.is_file() {
        return Some(builtin);
    }

    // 2. 环境变量（RUST_ANALYZER_PATH / RUSTC_PATH / CARGO_PATH 等）
    let env_key = format!("{}_PATH", name.to_uppercase().replace('-', "_"));
    if let Ok(path) = std::env::var(env_key) {
        let p = PathBuf::from(&path);
        if p.is_file() {
            return Some(p);
        }
    }

    // 3. PATH 扫描
    scan_path(&exe)
}

/// 在 PATH 中查找可执行文件（含 Windows PATHEXT 后缀探测）
fn scan_path(exe: &str) -> Option<PathBuf> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(exe);
        if candidate.is_file() {
            return Some(candidate);
        }
        // Windows：无后缀名时按 PATHEXT 顺序探测（.EXE/.CMD/.BAT…）
        if std::env::consts::OS == "windows" {
            for ext in ["exe", "cmd", "bat", "com"] {
                let with_ext = dir.join(format!("{exe}.{ext}"));
                if with_ext.is_file() {
                    return Some(with_ext);
                }
            }
        }
    }
    None
}

/// 已安装的内置工具链版本目录名（如 `1.98.0`）；未安装返回 None
pub fn installed_toolchain_version() -> Option<String> {
    let dir = rz_home().join("toolchain").join("version.txt");
    std::fs::read_to_string(dir)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 当前锁定版本（与仓库 rust-toolchain.toml 对齐；后续随官方 stable 升级）
pub const LOCKED_TOOLCHAIN_VERSION: &str = "1.98.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toolchain_dir_layout() {
        // 目录结构约定：~/.rz/toolchain/bin（组件级检查）
        let dir = toolchain_bin_dir();
        let components: Vec<String> = dir
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        assert!(components.iter().any(|c| c == ".rz"));
        assert!(components.iter().any(|c| c == "toolchain"));
        assert!(components.last().is_some_and(|c| c == "bin"));
    }

    #[test]
    fn test_find_toolchain_bin_missing_returns_none() {
        // 不存在的二进制名应返回 None（内置与 PATH 都无）
        let name = format!("__rzc_nonexistent__{}", std::process::id());
        assert!(find_toolchain_bin(&name).is_none());
    }
}
