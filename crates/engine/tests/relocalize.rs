//! 反译重生（relocalize）引擎级测试：跨母语阅读的可编译保真
//!
//! 固定 zh 样例经「正向转译到标准 Rust → relocalize 到 ja」产出含日文词条的
//! 可编译方言，验证：核心词 `main` 取关键字表词条（`主関数`）而非第三方 crate
//! 别名（tokio 的 `メイン関数`）；反译产物可再转译回 `fn main`；往返幂等；纯函数。

use std::path::Path;

use i18n_rust_engine::mapping_manager::MappingManager;
use i18n_rust_engine::relocalize::relocalize;
use i18n_rust_engine::transpile_pipeline;

fn manager(lang: &str) -> MappingManager {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("lang-packs/{lang}"));
    MappingManager::load_from_dir(&dir).unwrap_or_else(|e| panic!("加载 {lang} 语言包失败: {e}"))
}

/// zh 样例源码（声明/控制流/宏/字符串，字符串内含词条探针）
const ZH_SOURCE: &str = r#"函数 主函数() {
    让 数字 = 42;
    打印行!("答案是 {}", 数字);
    如果 数字 > 10 {
        打印行!("很大 打印行! 不该被改");
    }
    返回 ();
}
"#;

#[test]
fn relocalize_zh_to_ja_uses_core_main_keyword_not_crate_alias() {
    let zh = manager("zh");
    let ja = manager("ja");
    let english = transpile_pipeline(ZH_SOURCE, &zh).output;
    let ja_source = relocalize(&english, &ja);
    assert!(ja_source.contains("関数"), "应含 ja `関数`: {ja_source}");
    assert!(ja_source.contains("もし"), "应含 ja `もし`: {ja_source}");
    assert!(
        ja_source.contains("表示行!"),
        "应含 ja `表示行!`: {ja_source}"
    );
    assert!(
        ja_source.contains("主関数"),
        "入口应为 `主関数`: {ja_source}"
    );
    assert!(
        !ja_source.contains("メイン関数"),
        "不应把 main 反查为 tokio crate 别名 `メイン関数`: {ja_source}"
    );
    assert!(
        ja_source.contains("答案是"),
        "字符串应保持原样: {ja_source}"
    );
}

#[test]
fn relocalize_output_transpiles_back_to_same_english() {
    let zh = manager("zh");
    let ja = manager("ja");
    let english = transpile_pipeline(ZH_SOURCE, &zh).output;
    let ja_source = relocalize(&english, &ja);
    let back = transpile_pipeline(&ja_source, &ja).output;
    assert!(
        back.contains("fn main"),
        "反译产物应可转译回 `fn main`: {back}"
    );
    assert!(
        back.contains("println!"),
        "反译产物应可转译回 `println!`: {back}"
    );
}

#[test]
fn relocalize_is_idempotent_on_relocalize() {
    let ja = manager("ja");
    let zh = manager("zh");
    let english = transpile_pipeline(ZH_SOURCE, &zh).output;
    let first = relocalize(&english, &ja);
    let english_again = transpile_pipeline(&first, &ja).output;
    let second = relocalize(&english_again, &ja);
    assert_eq!(first, second, "relocalize 经正向/反译往返应幂等");
}

#[test]
fn relocalize_pure_no_panic_on_empty_and_standard_input() {
    let ja = manager("ja");
    assert_eq!(relocalize("", &ja), "", "空输入应得空输出");
    let _ = relocalize("fn main() { let x = std::mem::size_of::<u8>(); }", &ja);
}
