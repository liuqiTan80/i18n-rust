use i18n_rust_engine::映射管理器::映射管理器;
use i18n_rust_engine::语言;

fn main() {
    let 关键词源码 = 语言::内置文件("zh", "keywords.toml").unwrap();
    println!(
        "内置 keywords.toml 包含 哪里: {}",
        关键词源码.contains("哪里")
    );
    let 管理器 = 映射管理器::从内置载入(
        关键词源码,
        语言::内置文件("zh", "module_paths.toml").unwrap(),
        语言::内置文件("zh", "stdlib.toml").unwrap(),
        &[],
    )
    .unwrap();
    let 关键词表 = 管理器.取关键词表();
    println!("keyword_map 长度: {}", 关键词表.len());
    println!("哪里 => {:?}", 关键词表.get("哪里"));
    println!("函数 => {:?}", 关键词表.get("函数"));
    let 源码 = "函数 主函数() { 让 甲 = 5; }";
    let 输出 = i18n_rust_engine::词法器::基础转译(源码, 关键词表);
    println!("转译: {}", 输出);
}
