// i18n-rust 核心引擎
// 提供多语言 Rust 方言的词法处理、映射管理、诊断翻译、增量缓存、安全检测等功能

pub mod alias;
pub mod cache;
pub mod diagnostic;
pub mod error;
pub mod lexer;
pub mod logger;
#[path = "语言.rs"]
pub mod 语言;
pub mod mapping_manager;
pub mod mapping_source;
pub mod module_path;
pub mod unicode_confusion;

use std::time::Instant;

/// 生产级翻译入口：完整转译管线（词法转译 → 模块路径替换 → 别名替换）
///
/// 流程：
/// 1. 查询增量缓存（内容哈希 + 语境指纹），命中直接复用翻译结果与源映射；
/// 2. 未命中时先执行 Unicode 混淆安全检查（零宽/双向/同形字符），再执行转译；
/// 3. 转译结果写入缓存供后续复用。
///
/// 与命令行工具的管线顺序保持一致；日志级别由 `logger::init()` 读取
/// `RZ_LOG` 环境变量（debug/info/warn/error）控制。
pub fn transpile_source(
    source: &str,
    manager: &mapping_manager::MappingManager,
    cache: &mut cache::TranslationCache,
) -> Result<String, error::TranspileError> {
    Ok(transpile_source_with_map(source, manager, cache)?.output)
}

/// 同 [`transpile_source`]，同时返回源映射（被替换标识符的源偏移与翻译前后文本）
pub fn transpile_source_with_map(
    source: &str,
    manager: &mapping_manager::MappingManager,
    cache: &mut cache::TranslationCache,
) -> Result<cache::TranspileOutput, error::TranspileError> {
    logger::init();
    let start = Instant::now();
    crate::log_info!(
        "transpile_engine",
        "{}",
        crate::语言::f("log_transpile_start", &[&source.len().to_string()])
    );

    let fingerprint = cache::TranslationCache::generate_context_fingerprint(
        manager.get_keyword_map(),
        manager.get_module_path_map(),
        manager.get_alias_map(),
    );

    let output = cache.get_or_transpile(source, fingerprint, || {
        // 词法处理前的 Unicode 混淆安全检查（仅未命中缓存时执行）
        for warning in unicode_confusion::check_unicode_confusion(source) {
            crate::log_warn!("unicode_confusion", "{}", warning.format());
        }

        let macro_set = manager.get_macro_names();
        let result = lexer::transpile_with_map(source, manager.get_keyword_map(), &macro_set);
        let mut translated = result.output;
        if !manager.module_path_map.is_empty() {
            translated =
                module_path::replace_module_paths(&translated, manager.get_module_path_map());
        }
        if !manager.alias_map.is_empty() {
            translated = alias::replace_aliases(&translated, manager.get_alias_map());
        }
        Ok(cache::TranspileOutput::with_map(
            translated,
            result.source_map,
        ))
    })?;

    let elapsed = format!("{:?}", start.elapsed());
    crate::log_info!(
        "transpile_engine",
        "{}",
        crate::语言::f(
            "log_transpile_done",
            &[
                &source.len().to_string(),
                &output.output.len().to_string(),
                &elapsed,
            ]
        )
    );
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_manager() -> mapping_manager::MappingManager {
        let keywords_toml = r#"
["声明"]
"函数" = "fn"
"让" = "let"
["类型"]
"整数" = "i32"
["宏"]
"打印行" = "println"
"#;
        let module_paths_toml = r#"
["模块路径"]
"标准集合" = "std::collections"
"#;
        let stdlib_toml = r#"
["模块路径"]
"线程" = "std::thread"
["标识符"]
"字符串" = "String"
"#;
        let third_party_data = [(
            "测试库.toml",
            r#"
["模块路径"]
"网络库" = "netlib"
["标识符"]
"服务器" = "Server"
"#,
        )];
        mapping_manager::MappingManager::load_from_builtin(
            keywords_toml,
            module_paths_toml,
            stdlib_toml,
            &third_party_data,
        )
        .expect("创建测试管理器失败")
    }

    fn new_cache() -> cache::TranslationCache {
        // 测试中抑制日志输出（默认级别为警告，信息级会被过滤）
        logger::set_log_level(logger::LogLevel::Error);
        cache::TranslationCache::with_default_capacity()
    }

    #[test]
    fn test_full_pipeline_and_incremental_cache() {
        let manager = create_manager();
        let mut cache = new_cache();
        let source = "函数 主函数() { 让 数量: 整数 = 5; 打印行(\"你好\") }";

        let first = transpile_source(source, &manager, &mut cache).expect("翻译失败");
        let second = transpile_source(source, &manager, &mut cache).expect("翻译失败");

        // 输出一致且包含各阶段替换结果（词法 + 模块路径 + 别名）
        assert_eq!(first, second);
        assert!(
            first.contains("fn 主函数() { let 数量: i32 = 5; println!(\"你好\") }"),
            "实际输出：{}",
            first
        );

        // 第二次调用命中缓存：条数 1、未命中 1 次、命中 1 次
        assert_eq!(cache.current_count(), 1);
        assert_eq!(cache.miss_count(), 1);
        assert_eq!(cache.hit_count(), 1);
        assert_eq!(cache.hit_rate(), 0.5);
    }

    #[test]
    fn test_content_change_triggers_retranslate() {
        let manager = create_manager();
        let mut cache = new_cache();

        transpile_source("函数 主函数() { 让 x = 1; }", &manager, &mut cache).expect("翻译失败");
        transpile_source("函数 主函数() { 让 x = 2; }", &manager, &mut cache).expect("翻译失败");

        assert_eq!(cache.current_count(), 2);
        assert_eq!(cache.miss_count(), 2);
    }

    #[test]
    fn test_transpile_with_map_records_keyword_replacements() {
        let manager = create_manager();
        let mut cache = new_cache();
        let source = "函数 主函数() { 让 x = 1; }";

        let output = transpile_source_with_map(source, &manager, &mut cache).expect("翻译失败");

        // 函数 与 让 被替换，主函数 保持原样不产生映射
        let fn_map = output
            .source_map
            .iter()
            .find(|m| m.original == "函数")
            .expect("应有 函数 映射");
        assert_eq!(fn_map.replacement, "fn");
        assert_eq!(
            &source[fn_map.source_offset..fn_map.source_offset + fn_map.length],
            "函数"
        );

        let let_map = output
            .source_map
            .iter()
            .find(|m| m.original == "让")
            .expect("应有 让 映射");
        assert_eq!(let_map.replacement, "let");
        assert!(!output.source_map.iter().any(|m| m.original == "主函数"));
    }

    #[test]
    fn test_language_pack_update_invalidates_cache() {
        let manager = create_manager();
        let mut cache = new_cache();
        let source = "函数 主函数() { 让 x = 1; }";

        transpile_source(source, &manager, &mut cache).expect("翻译失败");
        // 语言包变化（新增 可变→mut 映射）→ 语境指纹变化 → 缓存失效重新翻译
        let new_manager = mapping_manager::MappingManager::load_from_builtin(
            r#"
["声明"]
"函数" = "fn"
"让" = "let"
"可变" = "mut"
"#,
            "[\"模块路径\"]\n",
            "[\"模块路径\"]\n[\"标识符\"]\n",
            &[],
        )
        .expect("创建新管理器失败");
        transpile_source(source, &new_manager, &mut cache).expect("翻译失败");

        // 同内容哈希 → 覆盖原条目（条数不变）；语境变化 → 未命中计数增加
        assert_eq!(cache.current_count(), 1);
        assert_eq!(cache.miss_count(), 2);
        // 新映射生效：可变→mut
        let result = transpile_source("函数 主函数() { 让 可变 x = 1; }", &new_manager, &mut cache)
            .expect("翻译失败");
        assert!(result.contains("let mut x"));
    }

    #[test]
    fn test_zero_width_char_warns_but_does_not_block() {
        let manager = create_manager();
        let mut cache = new_cache();
        // 源码含零宽空格（token 之间，Rust 视为空白），翻译应正常完成
        let source = "函数 主函数() {\u{200B} 让 x = 1; }";
        let result = transpile_source(source, &manager, &mut cache);
        assert!(result.is_ok(), "零宽字符不应阻断翻译：{:?}", result);
        assert!(result.unwrap().contains("fn 主函数()"));
    }
}
