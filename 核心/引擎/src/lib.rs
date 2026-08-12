// i18n-rust 核心引擎
// 提供中文 Rust 方言的词法处理、映射管理、诊断翻译等功能

#[path = "词法处理.rs"]
pub mod 词法处理;
#[path = "映射管理.rs"]
pub mod 映射管理;
#[path = "诊断.rs"]
pub mod 诊断;
#[path = "映射源.rs"]
pub mod 映射源;
#[path = "模块路径替换.rs"]
pub mod 模块路径替换;
#[path = "别名替换.rs"]
pub mod 别名替换;