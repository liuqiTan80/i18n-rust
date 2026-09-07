// i18n-rust 核心引擎
// 提供多语言 Rust 方言的词法处理、映射管理、诊断翻译、增量缓存、安全检测等功能

pub mod alias;
pub mod cache;
pub mod column_map;
pub mod diagnostic;
pub mod error;
pub mod fullwidth;
pub mod lexer;
pub mod lint;
pub mod logger;
pub mod mapping_manager;
pub mod mapping_source;
pub mod module_path;
pub mod toolchain;
pub mod unicode_confusion;
#[path = "语言.rs"]
pub mod 语言;

use std::collections::HashSet;
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

    let fingerprint = manager.context_fingerprint();

    let output = cache.get_or_transpile(source, fingerprint, || {
        Ok(transpile_pipeline(source, manager))
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

/// 无缓存的完整转译管线：Unicode 混淆检查 → 词法转译 → 模块路径替换 → 别名替换
///
/// CLI / LSP 等短命进程可直接复用此管线，无需维护增量缓存；
/// Unicode 告警经日志（warn 级）输出，不阻断转译。
/// 返回转译结果与源映射（被替换标识符的源偏移与翻译前后文本）。
pub fn transpile_pipeline(
    source: &str,
    manager: &mapping_manager::MappingManager,
) -> cache::TranspileOutput {
    transpile_pipeline_with_map(source, manager, None)
}

/// 同 [`transpile_pipeline`]，支持 LSP 虚拟项目的 `crate::` 前缀重写
///
/// `module_names` 为 Some 时，在模块路径替换后、别名替换前，为已知模块路径段
/// 添加 `crate::` 前缀（跨文件引用，见 [`module_path::qualify_module_paths_with_map`]）。
/// CLI 等非虚拟项目场景传 None 保持原有行为。
///
/// 返回的 `pipeline_map` 以母语源偏移升序记录全部替换（replacement 为最终输出文本），
/// 是列映射等消费方的唯一规则来源——消费方无需复刻任何转译判定。
pub fn transpile_pipeline_with_map(
    source: &str,
    manager: &mapping_manager::MappingManager,
    module_names: Option<&HashSet<String>>,
) -> cache::TranspileOutput {
    // 词法处理前的 Unicode 混淆安全检查（零宽/双向/同形字符，仅告警）
    for warning in unicode_confusion::check_unicode_confusion(source) {
        crate::log_warn!("unicode_confusion", "{}", warning.format());
    }
    // 全角标点教学检查（中文输入法常见错误：代码位置的全角标点，仅告警不阻断）；
    // 字符串/注释内的全角标点合法，由扫描器自动跳过
    for warning in fullwidth::find_fullwidth_punct(source) {
        crate::log_warn!("fullwidth", "{}", warning.format());
    }
    // 教学 lint（初学者代码风格提示，仅告警不阻断）：未标注类型/魔法数字/嵌套过深
    for warning in lint::lint_teaching(source) {
        crate::log_warn!("lint", "{}", warning.format());
    }

    let macro_map = manager.get_macro_map();
    let derive_map = manager.get_derive_map();
    let lex = lexer::transpile_with_map(source, manager.get_keyword_map(), &macro_map, &derive_map);

    // 阶段 2：use 语句路径替换
    let mp = if manager.module_path_map.is_empty() {
        module_path::ReplaceResult {
            output: lex.output.clone(),
            edits: Vec::new(),
        }
    } else {
        module_path::replace_module_paths_with_map(&lex.output, manager.get_module_path_map())
    };
    // 阶段 3（可选）：`crate::` 前缀重写（LSP 虚拟项目跨文件引用）
    let qual = if let Some(names) = module_names
        && !names.is_empty()
    {
        module_path::qualify_module_paths_with_map(&mp.output, names)
    } else {
        module_path::ReplaceResult {
            output: mp.output.clone(),
            edits: Vec::new(),
        }
    };
    // 阶段 4：标识符别名替换（声明位保护）
    let al = if manager.alias_map.is_empty() {
        alias::ReplaceResult {
            output: qual.output.clone(),
            edits: Vec::new(),
        }
    } else {
        alias::replace_aliases_with_map(&qual.output, manager.get_alias_map())
    };

    // 组合各阶段编辑表为母语源坐标的全管线地图（replacement 取最终输出文本）
    let stages: Vec<&[cache::SourceMapEntry]> =
        vec![&lex.final_edits, &mp.edits, &qual.edits, &al.edits];
    let pipeline_map = compose_pipeline_map(source, &stages);

    cache::TranspileOutput::with_full_map(al.output, lex.source_map, pipeline_map)
}

/// 合并各阶段编辑表为母语源坐标的最终编辑地图
///
/// `stages[0]`（词法）的编辑已直接以母语源偏移记录；后续阶段的编辑以各自
/// 输入文本偏移记录，需经逆映射链换算回母语源偏移。`original/length`
/// 一律以母语源文本为准（后续阶段替换的 token 未被前置阶段改写，长度一致）。
/// 同一源偏移出现多条编辑时保留最后阶段（后写覆盖先写，按阶段顺序稳定排序）。
fn compose_pipeline_map(
    source: &str,
    stages: &[&[cache::SourceMapEntry]],
) -> Vec<cache::SourceMapEntry> {
    let mut merged: Vec<cache::SourceMapEntry> = Vec::new();
    for (stage_idx, edits) in stages.iter().enumerate() {
        for e in edits.iter() {
            let zh_offset = if stage_idx == 0 {
                e.source_offset
            } else {
                // 逆映射链：从 stage_idx-1 阶段往回推到母语源坐标
                let mut off = e.source_offset;
                for prev in stages[..stage_idx].iter().rev() {
                    off = inverse_map_offset(prev, off);
                }
                off
            };
            // original/length 以母语源文本为准。
            // 注意：非第 0 阶段的 e.length 处于该阶段输入坐标，未必等于母语源坐标长度；
            // 这里把起止偏移都夹到母语源文本的合法字符边界，避免多字节字符（如日本語の
            // メソッド名 `から`）切片越界引发 panic。transpile 输出本身不受影响。
            let src_len = source.len();
            let start = zh_offset.min(src_len);
            let end_raw = (zh_offset + e.length).min(src_len);
            let start = {
                let mut s = start;
                while s < src_len && !source.is_char_boundary(s) {
                    s += 1;
                }
                s
            };
            let mut end = end_raw;
            while end > start && !source.is_char_boundary(end) {
                end -= 1;
            }
            let original = &source[start..end];
            merged.push(cache::SourceMapEntry::new(
                zh_offset,
                e.length,
                original,
                &e.replacement,
            ));
        }
    }
    // 稳定排序：同偏移多条编辑时，阶段靠后的保持在后（最终消费取后者）
    merged.sort_by_key(|e| e.source_offset);
    // 同偏移去重：保留最后一条（阶段靠后的替换即为最终输出文本）
    let mut deduped: Vec<cache::SourceMapEntry> = Vec::with_capacity(merged.len());
    for e in merged {
        if let Some(last) = deduped.last_mut()
            && last.source_offset == e.source_offset
        {
            *last = e;
        } else {
            deduped.push(e);
        }
    }
    deduped
}

/// 逆映射：把"上一阶段输出坐标"的偏移映射回"上一阶段输入坐标"
///
/// edits 按 input_offset 升序、互不重叠（每个被替换 token 一条）。
/// 落在替换区间内的偏移返回 token 起点（token 级边界近似——列映射在
/// token 起点记录分段点，区间内位置近似映射到起点即可）。
fn inverse_map_offset(edits: &[cache::SourceMapEntry], out_offset: usize) -> usize {
    let mut in_cursor = 0usize;
    let mut out_cursor = 0usize;
    for e in edits {
        let gap = e.source_offset - in_cursor; // 输入侧未替换区间
        if out_offset <= out_cursor + gap {
            return in_cursor + (out_offset - out_cursor);
        }
        out_cursor += gap;
        if out_offset < out_cursor + e.replacement.len() {
            return e.source_offset; // 替换区间内：token 起点近似
        }
        out_cursor += e.replacement.len();
        in_cursor = e.source_offset + e.length;
    }
    in_cursor + (out_offset - out_cursor)
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

    // ===== 全管线编辑地图（pipeline_map）测试 =====

    /// 把编辑地图应用到源文本（按 source_offset 拼接 replacement），
    /// 是"地图与真实输出一致"的权威校验——任何阶段改规则却不同步地图都会使此测试失败。
    fn apply_pipeline_map(source: &str, map: &[cache::SourceMapEntry]) -> String {
        let mut result = String::new();
        let mut pos = 0usize;
        for e in map {
            result.push_str(&source[pos..e.source_offset]);
            result.push_str(&e.replacement);
            pos = e.source_offset + e.length;
        }
        result.push_str(&source[pos..]);
        result
    }

    /// 完整管线（词法 + use 路径 + crate:: 前缀 + 别名）下，
    /// 地图回放结果必须与真实输出逐字符一致
    #[test]
    fn test_pipeline_map_replays_to_output() {
        let manager = create_manager();
        let module_names = HashSet::from(["辅助".to_string()]);
        let source = "使用 标准集合::哈希映射;\n\
                      函数 主函数() {\n\
                      \x20   辅助::辅助函数();\n\
                      \x20   让 数量: 整数 = 5;\n\
                      \x20   打印行(\"你好\");\n\
                      }";
        let output = transpile_pipeline_with_map(source, &manager, Some(&module_names));
        assert!(output.output.contains("crate::辅助::辅助函数()"));
        let rebuilt = apply_pipeline_map(source, &output.pipeline_map);
        assert_eq!(
            rebuilt, output.output,
            "pipeline_map 回放必须与真实输出一致\n实际输出：{}\n回放输出：{}",
            output.output, rebuilt
        );
    }

    /// 不带 module_names（CLI 场景）时地图同样与输出一致
    #[test]
    fn test_pipeline_map_replays_without_qualify() {
        let manager = create_manager();
        let source = "函数 主函数() { 让 数量: 整数 = 5; 打印行(\"你好\") }";
        let output = transpile_pipeline_with_map(source, &manager, None);
        let rebuilt = apply_pipeline_map(source, &output.pipeline_map);
        assert_eq!(rebuilt, output.output);
    }

    /// 宏自动补的 `!` 计入 replacement；模块路径段条目为 `crate::辅助`
    #[test]
    fn test_pipeline_map_records_macro_bang_and_qualify() {
        let manager = create_manager();
        let module_names = HashSet::from(["辅助".to_string()]);
        let source = "函数 主函数() {\n    辅助::辅助函数();\n    打印行(\"你好\");\n}";
        let output = transpile_pipeline_with_map(source, &manager, Some(&module_names));

        let macro_entry = output
            .pipeline_map
            .iter()
            .find(|e| e.original == "打印行")
            .expect("应有宏条目");
        assert_eq!(macro_entry.replacement, "println!");
        let qual_entry = output
            .pipeline_map
            .iter()
            .find(|e| e.original == "辅助")
            .expect("应有模块路径条目");
        assert_eq!(qual_entry.replacement, "crate::辅助");
    }

    /// use 语句路径段经模块路径阶段替换、末段经别名阶段替换（LSP 之前缺失的环节）
    #[test]
    fn test_pipeline_map_use_stmt_module_path_and_alias() {
        let manager = mapping_manager::MappingManager::load_from_builtin(
            r#"
["声明"]
"函数" = "fn"
"让" = "let"
"使用" = "use"
["宏"]
"打印行" = "println"
"#,
            r#"
["模块路径"]
"标准集合" = "std::collections"
"#,
            r#"
["标识符"]
"哈希映射" = "HashMap"
"字符串" = "String"
"#,
            &[],
        )
        .expect("创建管理器失败");
        let source = "使用 标准集合::哈希映射;\n函数 主函数() {\n    让 s: 字符串 = 哈希映射::新建();\n    打印行(\"你好\");\n}";
        let output = transpile_pipeline_with_map(source, &manager, None);
        assert!(
            output.output.contains("use std::collections::HashMap;"),
            "use 语句路径与末段都应被替换：{}",
            output.output
        );
        assert_eq!(
            apply_pipeline_map(source, &output.pipeline_map),
            output.output
        );
    }

    /// 属性测试：正向转译 → 反向转译应精确还原母语源（回译不变量）
    ///
    /// 约束：测试源避开模块路径（反向转译无模块路径表，`网络库::服务器` 之类
    /// 无法还原）；宏调用写带感叹号形式（正向自动补的 `!` 反向原样保留，
    /// 源写 `打印行(` 会还原成 `打印行!(` 造成差异）。
    #[test]
    fn test_property_roundtrip_preserves_source() {
        let manager = create_manager();
        let source = "函数 主函数() {\n    让 数量: 整数 = 5;\n    让 文本: 字符串 = \"世界\";\n    打印行!(\"你好\");\n}";
        let output = transpile_pipeline(source, &manager);
        assert!(
            output.output.contains("fn 主函数()") && output.output.contains("println!"),
            "正向转译应产出英文：{}",
            output.output
        );

        // 合并关键字 + 宏 + 别名三类映射并反转（英文 → 中文），与正向管线互为逆
        let mut reverse_map = std::collections::HashMap::new();
        for (zh, en) in manager.get_keyword_map() {
            reverse_map.insert(en.clone(), zh.clone());
        }
        for (zh, en) in manager.get_macro_map() {
            reverse_map.insert(en.clone(), zh.clone());
        }
        for (zh, en) in manager.get_alias_map() {
            reverse_map.insert(en.clone(), zh.clone());
        }

        let restored = lexer::reverse_transpile(
            &output.output,
            &reverse_map,
            &HashSet::new(),
            &HashSet::new(),
        );
        assert_eq!(restored, source, "回译应精确还原母语源");
    }

    /// 属性测试：英文输出再经转译管线应保持不变（幂等不变量）
    ///
    /// 若映射表中混入英文键、或宏感叹号自动补逻辑对英文输出不幂等，
    /// 连续转译会持续改变文本，本测试守护管线的收敛性。
    #[test]
    fn test_property_retranspile_idempotent() {
        let manager = create_manager();
        let source = "函数 主函数() {\n    让 数量: 整数 = 5;\n    打印行!(\"你好\");\n}";
        let en = transpile_pipeline(source, &manager).output;
        let again = transpile_pipeline(&en, &manager).output;
        assert_eq!(en, again, "对英文输出再转译应保持不变");
    }

    /// 属性测试：全部内置语言包的映射表不得含循环引用
    ///
    /// 循环（A→B 且 B→A）会使正向/反向转译来回抖动，属语言包配置错误；
    /// 本测试加载全部内置语言包，守护新增条目时不会引入互指。
    #[test]
    fn test_all_builtin_lang_packs_no_mapping_cycles() {
        let codes = 语言::builtin_language_codes();
        assert!(codes.len() >= 9, "应覆盖全部内置语言：{codes:?}");
        for code in codes {
            let files = 语言::builtin_lang_files(code);
            let get = |name: &str| {
                files
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, c)| *c)
                    .unwrap_or("")
            };
            let keywords = get("keywords.toml");
            assert!(!keywords.is_empty(), "{code} 语言包缺少 keywords.toml");
            let third_party: Vec<(&str, &str)> = files
                .iter()
                .filter(|(n, _)| n.starts_with("crates/"))
                .map(|(n, c)| (*n, *c))
                .collect();
            let manager = mapping_manager::MappingManager::load_from_builtin(
                keywords,
                get("module_paths.toml"),
                get("stdlib.toml"),
                &third_party,
            )
            .expect("语言包应能解析");
            let cycles = manager.find_mapping_cycles();
            assert!(cycles.is_empty(), "{code} 语言包存在循环映射：{cycles:?}");
        }
    }

    /// 门禁：附录C《常见错误信息字典》收录的错误码，在全部内置语言包中
    /// 都必须有教学提示（errors.toml 的 "教学提示" 字段）
    ///
    /// 附录C 是教程侧的高频错误清单，与教程章节同步维护；
    /// 教学提示缺失会让新手在常见错误上得不到母语指引，
    /// 因此该清单与语言包的覆盖关系由测试守护（附录C 新增错误码时同步更新本清单）。
    #[test]
    fn test_appendix_c_error_codes_have_teaching_hints_in_all_packs() {
        // 与 tutorials/附录C：常见错误信息字典.md 的 `### E0xxx` 标题同步维护
        let appendix_c_codes: &[&str] = &[
            "E0004", "E0063", "E0072", "E0106", "E0204", "E0261", "E0277", "E0308", "E0382",
            "E0405", "E0425", "E0432", "E0502", "E0507", "E0508", "E0531", "E0573", "E0596",
            "E0597", "E0599", "E0603",
        ];
        let codes = 语言::builtin_language_codes();
        assert!(codes.len() >= 9, "应覆盖全部内置语言：{codes:?}");
        for code in codes {
            let files = 语言::builtin_lang_files(code);
            let errors_toml = files
                .iter()
                .find(|(n, _)| *n == "errors.toml")
                .map(|(_, c)| *c)
                .expect("{code} 语言包缺少 errors.toml");
            let manager = diagnostic::ErrorTranslationManager::load_from_string(errors_toml)
                .expect("errors.toml 应能解析");
            for error_code in appendix_c_codes {
                let entry = manager
                    .translation_table
                    .get(*error_code)
                    .unwrap_or_else(|| {
                        panic!("{code} 语言包缺少附录C 错误码 [{error_code}] 的教学提示")
                    });
                assert!(
                    entry.teaching_hint.is_some(),
                    "{code} 语言包的 [{error_code}] 缺少教学提示（附录C 已收录）"
                );
            }
        }
    }
}
