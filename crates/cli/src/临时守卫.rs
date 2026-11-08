//! 共享的临时路径安全构建（与 LSP 侧虚拟临时目录同源的威胁模型）。
//!
//! 固定、可预测的临时目录路径在多用户机器上可被预先创建为符号链接：
//! 写文件会跟随链接覆写任意位置，递归删除目录会跟随链接删到任意目录。
//! 本模块提供两层防御：安全用户段按用户名隔离命名空间（特殊字符换成下划线）；
//! 安全临时路径拒绝已存在的符号链接。

use std::path::PathBuf;

/// 当前用户的路径安全段：仅保留字母、数字与下划线，其余替换为下划线
///
/// 与 LSP 侧虚拟临时目录的清洗规则一致；取不到用户环境变量时依次回退
/// 到 Windows 的用户名变量、再到默认值，保证多平台可用。
pub(crate) fn 安全用户段() -> String {
    let 用户名 = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "default".to_string());
    用户名
        .chars()
        .map(|本字| {
            if 本字.is_alphanumeric() || 本字 == '_' {
                本字
            } else {
                '_'
            }
        })
        .collect()
}

/// 构建经过符号链接校验的临时路径（文件或目录均可）
///
/// 传入的名字是系统临时目录下的相对名，调用方应先经安全用户段注入用户段，
/// 使路径不可预测。路径已存在且为符号链接时拒绝返回（防御对可预测名的预占位攻击）。
pub(crate) fn 安全临时路径(名称: &str) -> anyhow::Result<PathBuf> {
    let 目标路径 = std::env::temp_dir().join(名称);
    if 目标路径
        .symlink_metadata()
        .map(|本元| 本元.file_type().is_symlink())
        .unwrap_or(false)
    {
        anyhow::bail!(
            "临时路径安全检查失败：{} 已被符号链接占用，拒绝写入（疑似残留损坏或本地攻击）",
            目标路径.display()
        );
    }
    Ok(目标路径)
}
