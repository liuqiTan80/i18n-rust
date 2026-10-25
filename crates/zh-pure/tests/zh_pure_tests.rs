//! 方言纯函数测试入口：把 `build.rs` 转译出的标准 Rust 测试代码展开进来
//!
//! 测试用例用项目方言写在 `tests-zh/纯函数.zh`，`#[测试]` 经转译后成为
//! `#[test]`，由 cargo 自动发现并执行；本文件不手写任何用例。

include!(concat!(env!("OUT_DIR"), "/zh_tests.rs"));
