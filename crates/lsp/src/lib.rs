//! i18n-rust 语言服务器（库入口）
//!
//! 二进制入口在 `main.rs`（仅命令行解析与启动）；
//! 功能模块在此导出，供基准（`benches/hot_paths.rs`）等其它 target
//! 复用——二进制独占的模块无法被 bench/工具依赖，故统一走库 target。

/// LSP 代理服务器核心（握手、文档同步、请求转发、消息路由）
#[path = "代理.rs"]
pub mod 代理;
/// rust-analyzer 子进程管理（启动、消息收发、关闭）
#[path = "分析器.rs"]
pub mod 分析器;
/// 响应位置映射（虚拟 .rs 坐标 → 原始 .zh 坐标 + 诊断翻译）
#[path = "响应映射/mod.rs"]
pub mod 响应映射;
/// 界面消息本地化（帮助与错误提示随语言包/系统语言变化）
#[path = "本地化.rs"]
pub mod 本地化;
/// 翻译缓存（虚拟文件系统，维护 .zh → .rs 翻译缓存与行号映射）
#[path = "翻译缓存.rs"]
pub mod 翻译缓存;
/// 真实项目镜像 cargo check（权威诊断，镜像不可用时回退虚拟项目）
#[path = "镜像校验.rs"]
pub mod 镜像校验;
