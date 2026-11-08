//! 构建脚本：把 `tests-zh/*.zh` 方言测试转译为标准 Rust，写入 `$OUT_DIR`
//!
//! cargo 只识别 `.rs` 测试文件，因此这里用项目自身引擎做一次转译，
//! 生成物由 `tests/方言测试.zh` 转译出的测试入口以 `include!` 引入——方言里的
//! `#[测试]` 转译后成为 `#[test]`，`cargo test` 自动发现并执行。
//!
//! 映射只取自**编译期嵌入**的内置 zh 语言包（不读磁盘语言包、不联网、
//! 不读环境变量），保证测试结果与运行环境无关。

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// 存放方言测试的目录（与 `.rs` 测试入口分离，专放纯函数用例）
const 源目录: &str = "tests-zh";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={源目录}");

    let 管理器值 = 加载映射();
    let mut 产物文本 = String::from("// 由 build.rs 从 tests-zh/ 转译生成，请勿手改\n");

    let mut 源文件列表: Vec<PathBuf> = Vec::new();
    for 目录条目 in fs::read_dir(源目录)
        .unwrap_or_else(|故障值| panic!("读取 {源目录} 目录失败: {故障值}"))
        .flatten()
    {
        let 路径值 = 目录条目.path();
        if 路径值.extension().and_then(|后缀值| 后缀值.to_str()) == Some("zh") {
            源文件列表.push(路径值);
        }
    }
    源文件列表.sort();
    if 源文件列表.is_empty() {
        panic!("{源目录}/ 下没有 .zh 测试文件");
    }

    for 路径值 in &源文件列表 {
        let 源码文本 = fs::read_to_string(路径值)
            .unwrap_or_else(|故障值| panic!("读取 {} 失败: {故障值}", 路径值.display()));
        let 转译结果 = i18n_rust_engine::静默转译管线(&源码文本, &管理器值);
        // 产物必须是标准 Rust：方言测试标记没被替换说明语言包没生效，
        // 此时产出的 #[测试] 会被 rustc 当成未知属性丢弃、测试静默消失，
        // 故直接让构建失败（构建期恐慌是 cargo 的既定失败方式）。
        // 注释里提到标记的说明文字不算残留，故按行跳过注释行。
        let 残留标记 = 转译结果.输出.lines().any(|代码行| {
            let 去空白 = 代码行.trim_start();
            !去空白.starts_with("//") && 去空白.contains("#[测试]")
        });
        assert!(
            !残留标记,
            "{} 未被转译（语言包映射未生效）",
            路径值.display()
        );
        // 反向保险：转译后的用例必须真的带上 `#[test]`，否则 cargo 永远发现不到
        assert!(
            转译结果.输出.contains("#[test]"),
            "{} 没有产出任何 #[test] 用例",
            路径值.display()
        );
        writeln!(产物文本, "// ===== 源文件：{} =====", 路径值.display()).expect("写入产物失败");
        产物文本.push_str(&转译结果.输出);
        产物文本.push('\n');
    }

    let 输出路径 = Path::new(&env::var("OUT_DIR").expect("缺少 OUT_DIR")).join("zh_tests.rs");
    fs::write(&输出路径, 产物文本)
        .unwrap_or_else(|故障值| panic!("写入 {} 失败: {故障值}", 输出路径.display()));
}

/// 从编译期嵌入的 zh 语言包构造映射管理器（与 CLI 内置包同源）
fn 加载映射() -> i18n_rust_engine::映射管理器::映射管理器 {
    let 文件列表 = i18n_rust_engine::语言::内置语言包文件("zh");
    let 取内容 = |名称值: &str| {
        文件列表
            .iter()
            .find(|(路径名, _)| *路径名 == 名称值)
            .map(|(_, 内容值)| *内容值)
            .unwrap_or("")
    };
    let mut 第三方: Vec<(&str, &str)> = Vec::new();
    for (路径名, 内容值) in 文件列表.iter() {
        if 路径名.starts_with("crates/") {
            第三方.push((*路径名, *内容值));
        }
    }
    i18n_rust_engine::映射管理器::映射管理器::从内置载入(
        取内容("keywords.toml"),
        取内容("module_paths.toml"),
        取内容("stdlib.toml"),
        &第三方,
    )
    .expect("内置 zh 语言包应可加载")
}
