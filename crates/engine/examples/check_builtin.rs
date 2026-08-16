use i18n_rust_engine::mapping_manager::MappingManager;
use i18n_rust_engine::语言;

fn main() {
    let keywords = 语言::builtin_file("zh", "keywords.toml").unwrap();
    println!(
        "内置 keywords.toml 包含 哪里: {}",
        keywords.contains("哪里")
    );
    let m = MappingManager::load_from_builtin(
        keywords,
        语言::builtin_file("zh", "module_paths.toml").unwrap(),
        语言::builtin_file("zh", "stdlib.toml").unwrap(),
        &[],
    )
    .unwrap();
    let km = m.get_keyword_map();
    println!("keyword_map 长度: {}", km.len());
    println!("哪里 => {:?}", km.get("哪里"));
    println!("函数 => {:?}", km.get("函数"));
    let src = "函数 主函数() { 让 甲 = 5; }";
    let out = i18n_rust_engine::lexer::transpile_source(src, km);
    println!("转译: {}", out);
}
