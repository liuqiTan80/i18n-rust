//! 共享的临时路径安全构建（与 LSP 虚拟目录 `virtual_temp_dir` 同源的威胁模型）。
//!
//! 固定/可预测的 /tmp 路径在多用户机器上可被预创建为符号链接：
//! 写文件会跟随链接覆写任意位置，`remove_dir_all` 会跟随链接删除
//! 任意目录。本模块提供两层防御：
//!
//! 1. [`safe_user_segment`]：按用户名隔离命名空间（特殊字符替换为 `_`）；
//! 2. [`secure_temp_path`]：拒绝已存在的符号链接路径。
//!
//! CLI 各临时产物（直调 rustc 的 exe、mapping 提取项目、语言包解压目录）
//! 统一经此构造，与 LSP 侧保持一致的安全水位。

use std::path::PathBuf;

/// 当前用户的路径安全段：仅保留字母/数字/下划线，其余替换为 `_`
///
/// 与 LSP 侧 `virtual_temp_dir` 的清洗规则一致；`USER` 缺失时回退
/// `USERNAME`（Windows）再到 `default`，保证多平台可用。
pub(crate) fn safe_user_segment() -> String {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "default".to_string());
    user.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// 构建经过符号链接校验的临时路径（文件或目录均可）
///
/// `name` 为 `std::env::temp_dir()` 下的相对名，调用方应经
/// [`safe_user_segment`] 注入用户段使路径不可预测。
/// 路径已存在且为符号链接时拒绝返回（防御对可预测名的预占位攻击）。
pub(crate) fn secure_temp_path(name: &str) -> anyhow::Result<PathBuf> {
    let path = std::env::temp_dir().join(name);
    if path
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        anyhow::bail!(
            "临时路径安全检查失败：{} 已被符号链接占用，拒绝写入（疑似残留损坏或本地攻击）",
            path.display()
        );
    }
    Ok(path)
}
