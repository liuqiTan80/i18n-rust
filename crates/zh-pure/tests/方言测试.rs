//! 方言纯函数测试入口：把 `build.rs` 转译出的标准 Rust 测试代码展开进来
//!
//! 测试用例用项目方言写在 `tests-zh/纯函数.zh`，`#[测试]` 经转译后成为
//! `#[test]`，由 cargo 自动发现并执行；本文件不手写任何用例。
//!
//! 方言源码区不允许出现英文，而被测 crate 的名字**必须**是 ASCII
//! （rustc：`crate name ... passed to --extern is not a valid ASCII identifier`），
//! 所以这里在测试 crate 根把全部公开项再导出一次，产物里的 `使用 超级::*;`
//! 因此拿到被测项——crate 名只出现在这条英文侧的薄胶水里。
pub use zh_pure::*;

/// 产物展开的位置：`super` 即本 crate 根，上一条再导出在这里生效
mod 方言用例 {
    include!(concat!(env!("OUT_DIR"), "/zh_tests.rs"));
}
