use i18n_rust_engine::映射管理器::映射管理器;
use i18n_rust_engine::语言;

fn main() {
    let keywords = 语言::内置文件("zh", "keywords.toml").unwrap();
    println!(
        "内置 keywords.toml 包含 哪里: {}",
        keywords.contains("哪里")
    );
    let m = 映射管理器::从内置载入(
        keywords,
        语言::内置文件("zh", "module_paths.toml").unwrap(),
        语言::内置文件("zh", "stdlib.toml").unwrap(),
        &[],
    )
    .unwrap();
    let km = m.取关键词表();
    println!("keyword_map 长度: {}", km.len());
    println!("哪里 => {:?}", km.get("哪里"));
    println!("函数 => {:?}", km.get("函数"));
    let src = "函数 主函数() { 让 甲 = 5; }";
    let out = i18n_rust_engine::词法器::transpile_source(src, km);
    println!("转译: {}", out);
}
