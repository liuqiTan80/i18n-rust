//! 诊断模块 - 解析 rustc 的 JSON 错误输出，结合语言包翻译成中文教学诊断信息
//!
//! 将 rustc 编译器产生的 JSON 格式诊断信息（--error-format=json）解析为结构化数据，
//! 再根据语言包中的错误消息翻译表，生成面向中文教学场景的诊断输出，
//! 包含错误码解释、教学提示、所有权错误叙事化详情等。
//!
//! 子模块划分（与本文件此前的分段注释一致）：
//! - [`model`]：rustc JSON 诊断数据结构
//! - [`message`]：错误消息翻译结构（errors.toml）
//! - [`teaching`]：教学诊断结构与所有权叙事化详情
//! - [`translator`]：诊断翻译器（类型映射支持）
//! - [`output`]：JSON 解析、格式化输出与未解析导入检测
//! - [`gaps`]：英文残留兜底与待译清单（未译消息登记/落盘/读取）

pub mod gaps;
mod message;
mod model;
mod output;
mod teaching;
mod translator;

pub use gaps::{
    LEVEL_HELP, LEVEL_MAIN, LEVEL_TOOL, TranslationGap, clear_gaps, drain_gaps, flush_gaps,
    flush_gaps_to_dir, gap_file, gaps_dir, parse_gap_lines, peek_gaps, read_gaps, record_gap,
};
pub use message::{
    ErrorTranslationManager, MessageAudit, MessageRest, audit_message, fill_dynamic_placeholders,
};
pub use model::{CompilerDiagnostic, DiagnosticCode, DiagnosticSpan};
pub use output::{
    FormattedDiagnostic, extract_backtick_first_segments, is_unresolved_import_message,
    parse_diagnostic_output, unresolved_crate_candidates,
};
pub use teaching::{
    DiagnosticLevel, DiagnosticLocation, OWNERSHIP_ERROR_CODES, OwnershipDetails,
    TeachingDiagnostic, extract_ownership_details,
};
pub use translator::{DiagnosticTranslator, RenderedMessage, build_type_map, suggested_crate_name};

// 内部辅助函数：仅测试直接引用（见下方 tests 模块）
#[cfg(test)]
pub(crate) use translator::{
    count_ref_prefix, localize_ref_type, localize_type_token, ref_base_type, replace_type_token,
    replace_whole_word,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_error_message_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("创建临时文件失败");
        write!(file, "{}", content).expect("写入临时文件失败");
        file
    }

    fn create_test_diagnostic() -> CompilerDiagnostic {
        CompilerDiagnostic {
            message: "mismatched types".to_string(),
            code: Some(DiagnosticCode {
                code: "E0308".to_string(),
                explanation: None,
            }),
            level: "error".to_string(),
            spans: vec![DiagnosticSpan {
                file_name: "src/main.rs".to_string(),
                line_start: 3,
                column_start: 5,
                line_end: 3,
                column_end: 10,
                source_text: Some(
                    serde_json::json!([{"text": "let x: i32 = \"hello\";", "highlight_start": 1, "highlight_end": 22}]),
                ),
                byte_start: Some(30),
                byte_end: Some(51),
                is_primary: true,
                label: Some("expected `i32`, found `&str`".to_string()),
                suggested_replacement: None,
            }],
            children: vec![CompilerDiagnostic {
                message: "consider using a conversion function".to_string(),
                code: None,
                level: "help".to_string(),
                spans: vec![],
                children: vec![],
                rendered: None,
            }],
            rendered: None,
        }
    }

    fn create_test_type_map() -> HashMap<String, String> {
        HashMap::from([
            ("u32".into(), "整数32".into()),
            ("i32".into(), "有符号整数32".into()),
            ("f64".into(), "浮点64".into()),
            ("&str".into(), "字符串引用".into()),
            ("String".into(), "字符串".into()),
        ])
    }

    /// 构造指定 label 的 E0308 诊断（基于标准测试诊断修改 label）
    fn create_e0308_with_label(label: &str) -> CompilerDiagnostic {
        let mut diagnostic = create_test_diagnostic();
        diagnostic.spans[0].label = Some(label.to_string());
        diagnostic
    }

    #[test]
    fn test_count_ref_prefix() {
        assert_eq!(count_ref_prefix("String"), 0);
        assert_eq!(count_ref_prefix("&String"), 1);
        assert_eq!(count_ref_prefix("&&String"), 2);
        // &mut 是单层可变引用，不算两层
        assert_eq!(count_ref_prefix("&mut String"), 1);
        assert_eq!(count_ref_prefix("&str"), 1);
    }

    #[test]
    fn test_localize_ref_type() {
        let map = create_test_type_map();
        // 整体命中 type_map（&str 是特殊整体条目）
        assert_eq!(localize_ref_type("&str", &map), "字符串引用");
        // 拆 & 前缀 + 本体映射
        assert_eq!(localize_ref_type("&String", &map), "&字符串");
        assert_eq!(localize_ref_type("&&i32", &map), "&&有符号整数32");
        assert_eq!(localize_ref_type("&mut String", &map), "&mut 字符串");
        // 无 & 前缀时原样返回
        assert_eq!(localize_ref_type("String", &map), "字符串");
    }

    #[test]
    fn test_e0308_ref_depth_hint_appended() {
        let _guard = crate::语言::test_language("zh");
        // expected `&String`, found `String`：引用层数 1 vs 0，应追加提示
        let diagnostic = create_e0308_with_label("expected `&String`, found `String`");
        let translator = DiagnosticTranslator::new(
            ErrorTranslationManager::load_from_string("").unwrap(),
            create_test_type_map(),
        );
        let teaching = translator.translate_diagnostic(&diagnostic);
        assert!(
            teaching
                .teaching_hints
                .iter()
                .any(|h| h.contains("引用层数不匹配")
                    && h.contains("&字符串")
                    && h.contains("字符串")),
            "应追加引用层数提示，实际提示：{:?}",
            teaching.teaching_hints
        );
    }

    #[test]
    fn test_e0308_same_ref_depth_no_hint() {
        let _guard = crate::语言::test_language("zh");
        // expected `&String`, found `&str`：层数 1 vs 1，不应追加提示
        let diagnostic = create_e0308_with_label("expected `&String`, found `&str`");
        let translator = DiagnosticTranslator::new(
            ErrorTranslationManager::load_from_string("").unwrap(),
            create_test_type_map(),
        );
        let teaching = translator.translate_diagnostic(&diagnostic);
        assert!(
            !teaching
                .teaching_hints
                .iter()
                .any(|h| h.contains("引用层数不匹配")),
            "层数相同时不应追加提示，实际提示：{:?}",
            teaching.teaching_hints
        );
    }

    #[test]
    fn test_e0308_no_ref_no_hint() {
        let _guard = crate::语言::test_language("zh");
        // expected `String`, found `i32`：均非引用，不应追加提示
        let diagnostic = create_e0308_with_label("expected `String`, found `i32`");
        let translator = DiagnosticTranslator::new(
            ErrorTranslationManager::load_from_string("").unwrap(),
            create_test_type_map(),
        );
        let teaching = translator.translate_diagnostic(&diagnostic);
        assert!(
            !teaching
                .teaching_hints
                .iter()
                .any(|h| h.contains("引用层数不匹配")),
            "无引用时不应追加提示，实际提示：{:?}",
            teaching.teaching_hints
        );
    }

    #[test]
    fn test_ref_base_type() {
        assert_eq!(ref_base_type("String"), "String");
        assert_eq!(ref_base_type("&String"), "String");
        assert_eq!(ref_base_type("&&i32"), "i32");
        // &mut 是单层可变引用，剥后为 String
        assert_eq!(ref_base_type("&mut String"), "String");
        assert_eq!(ref_base_type("&&mut String"), "String");
        // &str 的核心类型是 str（与 String 不同，不可相互以增减 & 得到）
        assert_eq!(ref_base_type("&str"), "str");
    }

    /// 核心类型不同（标量 vs 字符串等）时：引用层数提示为假阳性，不应追加
    #[test]
    fn test_e0308_different_base_types_no_ref_hint() {
        let _guard = crate::语言::test_language("zh");
        for label in [
            "expected `&str`, found `String`",
            "expected `i32`, found `&str`",
            "expected `&String`, found `i32`",
        ] {
            let diagnostic = create_e0308_with_label(label);
            let translator = DiagnosticTranslator::new(
                ErrorTranslationManager::load_from_string("").unwrap(),
                create_test_type_map(),
            );
            let teaching = translator.translate_diagnostic(&diagnostic);
            assert!(
                !teaching
                    .teaching_hints
                    .iter()
                    .any(|h| h.contains("引用层数不匹配")),
                "核心类型不同不应追加引用层数提示（{label}）：{:?}",
                teaching.teaching_hints
            );
        }
    }

    #[test]
    fn test_load_error_translation_manager() {
        let toml_content = r#"
[E0308]
"消息模板" = "类型不匹配：期望 `{期望}`，实际得到 `{实际}`"
"教学提示" = "请检查变量类型是否与上下文要求一致。"

[E0433]
"消息模板" = "未找到类型 `{名称}`"
"教学提示" = "请确认是否已导入所需的模块或类型。"
"#;
        let file = create_error_message_file(toml_content);
        let manager = ErrorTranslationManager::load_from_file(file.path()).unwrap();
        assert_eq!(manager.coverage_count(), 2);
    }

    #[test]
    fn test_translate_with_lang_pack_and_type_replacement() {
        let toml_content = r#"
[E0308]
"消息模板" = "类型不匹配：期望 `{期望}`，实际得到 `{实际}`"
"教学提示" = "请检查变量类型。"
"#;
        let file = create_error_message_file(toml_content);
        let manager = ErrorTranslationManager::load_from_file(file.path()).unwrap();
        let type_map = create_test_type_map();
        let translator = DiagnosticTranslator::new(manager, type_map);

        let diagnostic = create_test_diagnostic();
        let teaching = translator.translate_diagnostic(&diagnostic);

        assert_eq!(
            teaching.translated_message,
            "类型不匹配：期望 `有符号整数32`，实际得到 `字符串引用`"
        );
    }

    /// 无错误码消息：命中 [消息翻译] 节精确条目，标题与教学提示均翻译
    #[test]
    fn test_translate_without_code_matches_message_map() {
        let _guard = crate::语言::test_language("zh");
        let toml_content = r#"
["消息翻译"."format argument must be a string literal"]
"消息模板" = "format 参数必须是字符串字面量"
"教学提示" = "第一个参数应带引号。"
"#;
        let file = create_error_message_file(toml_content);
        let manager = ErrorTranslationManager::load_from_file(file.path()).unwrap();
        let translator = DiagnosticTranslator::new(manager, create_test_type_map());

        let diagnostic = CompilerDiagnostic {
            message: "format argument must be a string literal".to_string(),
            code: None,
            level: "error".to_string(),
            spans: vec![],
            children: vec![],
            rendered: None,
        };
        let teaching = translator.translate_diagnostic(&diagnostic);

        assert_eq!(teaching.translated_message, "format 参数必须是字符串字面量");
        assert_eq!(teaching.teaching_hints, vec!["第一个参数应带引号。"]);
    }

    /// 前缀匹配：help 短语 "did you mean " 保留动态后缀（`foo`?）
    #[test]
    fn test_translate_help_prefix_keeps_dynamic_rest() {
        let _guard = crate::语言::test_language("zh");
        let toml_content = r#"
["消息翻译"."did you mean "]
"消息模板" = "你是否想用 "
"#;
        let file = create_error_message_file(toml_content);
        let manager = ErrorTranslationManager::load_from_file(file.path()).unwrap();
        let translator = DiagnosticTranslator::new(manager, create_test_type_map());

        let diagnostic = CompilerDiagnostic {
            message: "no method named `foo`".to_string(),
            code: Some(DiagnosticCode {
                code: "E0599".to_string(),
                explanation: None,
            }),
            level: "error".to_string(),
            spans: vec![],
            children: vec![CompilerDiagnostic {
                message: "did you mean `foo`?".to_string(),
                code: None,
                level: "help".to_string(),
                spans: vec![],
                children: vec![],
                rendered: None,
            }],
            rendered: None,
        };
        let teaching = translator.translate_diagnostic(&diagnostic);

        // help 子诊断：前缀翻译 + 动态后缀保留，包上"修复建议："前缀
        assert_eq!(teaching.teaching_hints, vec!["修复建议：你是否想用 `foo`?"]);
    }

    /// Unicode 混淆 help：{q0}/{q1} 捕获占位符从单引号分段提取两个字符
    #[test]
    fn test_translate_help_unicode_quote_captures() {
        let _guard = crate::语言::test_language("zh");
        let toml_content = r#"
["消息翻译"."Unicode character '"]
"消息模板" = "Unicode 字符 '{q0}' 形似 '{q1}'，但它并不是它"
"#;
        let file = create_error_message_file(toml_content);
        let manager = ErrorTranslationManager::load_from_file(file.path()).unwrap();
        let translator = DiagnosticTranslator::new(manager, create_test_type_map());

        let diagnostic = CompilerDiagnostic {
            message: "unknown start of token: \\u{ff0c}".to_string(),
            code: None,
            level: "error".to_string(),
            spans: vec![],
            children: vec![CompilerDiagnostic {
                message:
                    "Unicode character '，' (Fullwidth Comma) looks like ',' (Comma), but it is not"
                        .to_string(),
                code: None,
                level: "help".to_string(),
                spans: vec![],
                children: vec![],
                rendered: None,
            }],
            rendered: None,
        };
        let teaching = translator.translate_diagnostic(&diagnostic);

        assert_eq!(
            teaching.teaching_hints,
            vec!["修复建议：Unicode 字符 '，' 形似 ','，但它并不是它"]
        );
    }

    /// "if you wanted to use a crate named" 建议：中文/非 ASCII 名静默丢弃
    ///（回归：中文模块名误触发 `add 规则类型` 建议，且英文原文直接透出）
    #[test]
    fn test_translate_help_crate_add_suggestion_non_ascii_dropped() {
        let _guard = crate::语言::test_language("zh");
        let file = create_error_message_file("");
        let manager = ErrorTranslationManager::load_from_file(file.path()).unwrap();
        let translator = DiagnosticTranslator::new(manager, create_test_type_map());

        let diagnostic = CompilerDiagnostic {
            message: "unresolved import `不存在的中文模块`".to_string(),
            code: Some(DiagnosticCode {
                code: "E0432".to_string(),
                explanation: None,
            }),
            level: "error".to_string(),
            spans: vec![],
            children: vec![CompilerDiagnostic {
                message: "if you wanted to use a crate named `不存在的中文模块`, use \
                          `cargo add 不存在的中文模块` to add it to your `Cargo.toml`"
                    .to_string(),
                code: None,
                level: "help".to_string(),
                spans: vec![],
                children: vec![],
                rendered: None,
            }],
            rendered: None,
        };
        let teaching = translator.translate_diagnostic(&diagnostic);

        // 非 ASCII “crate 名”的建议必然无效，应整体丢弃而非英文透出
        assert!(teaching.teaching_hints.is_empty());
    }

    /// "if you wanted to use a crate named" 建议：ASCII crate 名翻译为中文
    /// 且重写为 `rzc add`（教学引导走方言工具链）
    #[test]
    fn test_translate_help_crate_add_suggestion_ascii() {
        let _guard = crate::语言::test_language("zh");
        let toml_content = r#"
["消息翻译"."if you wanted to use a crate named "]
"消息模板" = "若要使用名为 {q0} 的第三方库，请运行 `rzc add {q0}` 添加依赖"
"#;
        let file = create_error_message_file(toml_content);
        let manager = ErrorTranslationManager::load_from_file(file.path()).unwrap();
        let translator = DiagnosticTranslator::new(manager, create_test_type_map());

        let diagnostic = CompilerDiagnostic {
            message: "unresolved import `serde_json`".to_string(),
            code: Some(DiagnosticCode {
                code: "E0432".to_string(),
                explanation: None,
            }),
            level: "error".to_string(),
            spans: vec![],
            children: vec![CompilerDiagnostic {
                message: "if you wanted to use a crate named `serde_json`, use `cargo add \
                          serde_json` to add it to your `Cargo.toml`"
                    .to_string(),
                code: None,
                level: "help".to_string(),
                spans: vec![],
                children: vec![],
                rendered: None,
            }],
            rendered: None,
        };
        let teaching = translator.translate_diagnostic(&diagnostic);

        assert_eq!(
            teaching.teaching_hints,
            vec![
                "修复建议：若要使用名为 serde_json 的第三方库，请运行 `rzc add serde_json` 添加依赖"
            ]
        );
    }

    /// 内置中文包：顶层 `让` 的解析错误应完整中文化（消息 + 建议 + 教学提示）
    ///
    /// 回归：'expected item, found keyword `let`' 只译出 '期望 ' 前缀、
    /// 'consider using `static`...' 只译出 '考虑 ' 前缀，中英混杂。
    #[test]
    fn test_builtin_zh_top_level_let_fully_translated() {
        let _guard = crate::语言::test_language("zh");
        let zh = crate::语言::builtin_file("zh", "errors.toml").expect("内置中文错误表应存在");
        let manager = ErrorTranslationManager::load_from_string(zh).unwrap();
        let translator = DiagnosticTranslator::new(manager, create_test_type_map());

        let diagnostic = CompilerDiagnostic {
            message: "expected item, found keyword `let`".to_string(),
            code: None,
            level: "error".to_string(),
            spans: vec![],
            children: vec![CompilerDiagnostic {
                message: "consider using `static` or `const` instead of `let`".to_string(),
                code: None,
                level: "help".to_string(),
                spans: vec![],
                children: vec![],
                rendered: None,
            }],
            rendered: None,
        };
        let teaching = translator.translate_diagnostic(&diagnostic);

        assert_eq!(
            teaching.translated_message,
            "此处应是一项声明，但遇到了 `让`"
        );
        assert!(
            !teaching.translated_message.contains("expected"),
            "消息不得残留英文：{}",
            teaching.translated_message
        );
        assert_eq!(
            teaching.teaching_hints,
            vec![
                "模块（顶层）作用域不能直接写 `让`：全局变量请改用 `静态` 或 `常量`，代码逻辑请放进函数（如 `函数 主函数()`）。",
                "修复建议：考虑改用 `静态` 或 `常量` 代替 `让`"
            ]
        );
    }

    /// 带模块路径的类型名：最长后缀匹配（std::fmt::Display → std::fmt::显示）
    #[test]
    fn test_replace_type_token_longest_suffix() {
        let mut type_map = create_test_type_map();
        type_map.insert("Display".into(), "显示".into());
        assert_eq!(
            replace_type_token("std::fmt::Display", &type_map),
            Some("std::fmt::显示".to_string())
        );
        assert_eq!(replace_type_token("std::io::Error", &type_map), None);
    }

    /// 三段式本地化：类型段 + 路径前缀 + 中间段全中文化
    #[test]
    fn test_localize_type_token_full_path() {
        let mut map = create_test_type_map();
        map.insert("Display".into(), "可显示".into());
        map.insert("fmt".into(), "格式化".into());
        map.insert("std".into(), "标准库".into());
        // 类型 + 路径 + 中间段全部命中
        assert_eq!(
            localize_type_token("std::fmt::Display", &map),
            Some("标准库::格式化::可显示".to_string())
        );
        // 仅类型段命中：路径保持原样
        assert_eq!(
            localize_type_token("alloc::String", &map),
            Some("alloc::字符串".to_string())
        );
        // 无任何段命中：保持原样返回 None（unknown 段不在映射中）
        assert_eq!(localize_type_token("unknown::io::Error", &map), None);
        // 无 :: 路径的类型名（如 &str）由上层整串命中处理，此处不处理属预期
        map.insert("&str".into(), "字符串引用".into());
        assert_eq!(localize_type_token("&str", &map), None);
        assert_eq!(map.get("&str"), Some(&"字符串引用".to_string()));
    }

    /// E0277 占位符填充后的实际类型名也应中文化（{类型}/{特征} → 中文 + 后缀匹配）
    #[test]
    fn test_translate_e0277_placeholder_types_localized() {
        let _guard = crate::语言::test_language("zh");
        let toml_content = r#"
[E0277]
"消息模板" = "类型 `{类型}` 未实现特征 `{特征}`"
"教学提示" = "请为该类型实现所需的特征。"
"#;
        let file = create_error_message_file(toml_content);
        let manager = ErrorTranslationManager::load_from_file(file.path()).unwrap();
        let mut type_map = create_test_type_map();
        type_map.insert("Display".into(), "显示".into());
        let translator = DiagnosticTranslator::new(manager, type_map);

        let diagnostic = CompilerDiagnostic {
            message: "`({integer}, {integer}, &str)` doesn't implement `std::fmt::Display`"
                .to_string(),
            code: Some(DiagnosticCode {
                code: "E0277".to_string(),
                explanation: None,
            }),
            level: "error".to_string(),
            spans: vec![],
            children: vec![],
            rendered: None,
        };
        let teaching = translator.translate_diagnostic(&diagnostic);

        assert_eq!(
            teaching.translated_message,
            "类型 `(整数, 整数, &str)` 未实现特征 `std::fmt::显示`"
        );
    }

    /// 后缀键（~ 开头）：动态名在消息中间的 lint 警告（dead_code/non_snake_case）
    #[test]
    fn test_translate_suffix_key_never_used() {
        let _guard = crate::语言::test_language("zh");
        let toml_content = r#"
["消息翻译"."function `"]
"消息模板" = "函数 `{q0}` 从未被使用"

["消息翻译"."~ is never used"]
"消息模板" = "`{q0}` 从未被使用"

["消息翻译"."~ should have an upper camel case name"]
"消息模板" = "`{q0}` 应使用大写驼峰命名"
"#;
        let file = create_error_message_file(toml_content);
        let manager = ErrorTranslationManager::load_from_file(file.path()).unwrap();
        let translator = DiagnosticTranslator::new(manager, create_test_type_map());

        // 前缀键优先：含类型词 "函数"，比通用后缀键更完整
        let diag = |message: &str| CompilerDiagnostic {
            message: message.to_string(),
            code: None,
            level: "warning".to_string(),
            spans: vec![],
            children: vec![],
            rendered: None,
        };
        let teaching = translator.translate_diagnostic(&diag("function `foo` is never used"));
        assert_eq!(teaching.translated_message, "函数 `foo` 从未被使用");

        // 无前缀键命中时走后缀键：动态名前缀保留在 {q0} 捕获中
        let teaching = translator.translate_diagnostic(&diag(
            "type `myStruct` should have an upper camel case name",
        ));
        assert_eq!(teaching.translated_message, "`myStruct` 应使用大写驼峰命名");

        // 后缀键精确兜底：enum 无专用前缀键时用通用模板
        let teaching = translator.translate_diagnostic(&diag("type `Foo` is never used"));
        assert_eq!(teaching.translated_message, "`Foo` 从未被使用");
    }

    /// rustc 1.97+ 算术错误（E0369）无 label，类型嵌入 message，
    /// 应回退提取并翻译 rustc 字面量占位符 `{integer}`
    #[test]
    fn test_translate_e0369_no_label_fallback_to_message() {
        let _guard = crate::语言::test_language("zh");
        let toml_content = r#"
[E0369]
"消息模板" = "类型不匹配：无法对 `{期望}` 和 `{实际}` 执行运算"
"教学提示" = "运算符两侧的类型必须兼容。"
"#;
        let file = create_error_message_file(toml_content);
        let manager = ErrorTranslationManager::load_from_file(file.path()).unwrap();
        let translator = DiagnosticTranslator::new(manager, create_test_type_map());

        let diagnostic = CompilerDiagnostic {
            message: "cannot add `{integer}` to `&str`".to_string(),
            code: Some(DiagnosticCode {
                code: "E0369".to_string(),
                explanation: None,
            }),
            level: "error".to_string(),
            spans: vec![DiagnosticSpan {
                file_name: "src/main.rs".to_string(),
                line_start: 3,
                column_start: 5,
                line_end: 3,
                column_end: 10,
                source_text: None,
                byte_start: None,
                byte_end: None,
                is_primary: true,
                label: None,
                suggested_replacement: None,
            }],
            children: vec![],
            rendered: None,
        };

        let teaching = translator.translate_diagnostic(&diagnostic);
        assert_eq!(
            teaching.translated_message,
            "类型不匹配：无法对 `整数` 和 `字符串引用` 执行运算"
        );
    }

    #[test]
    fn test_parse_json_diagnostic_output() {
        let json_line = r#"{"message":"mismatched types","code":{"code":"E0308"},"level":"error","spans":[],"children":[],"rendered":null}"#;
        let output = format!("{}\n{}", "    Compiling test v0.1.0", json_line);
        let diagnostics = parse_diagnostic_output(&output);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].message, "mismatched types");
    }

    #[test]
    fn test_parse_cargo_wrapped_diagnostic_output() {
        // cargo check --message-format=json 的 compiler-message 包装行：诊断嵌套在 message 字段
        let wrapped_line = r#"{"reason":"compiler-message","package_id":"path+file:///test#e2e@0.1.0","manifest_path":"/test/Cargo.toml","target":{"kind":["bin"],"name":"e2e"},"message":{"message":"use of moved value: `数据`","code":{"code":"E0382"},"level":"error","spans":[],"children":[],"rendered":null}}"#;
        let output = format!("{}\n{}", "    Checking e2e v0.1.0", wrapped_line);
        let diagnostics = parse_diagnostic_output(&output);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code.as_ref().unwrap().code, "E0382");
        assert_eq!(diagnostics[0].message, "use of moved value: `数据`");
    }

    #[test]
    fn test_diagnostic_level_conversion() {
        assert_eq!(DiagnosticLevel::from_str("error"), DiagnosticLevel::Error);
        assert_eq!(
            DiagnosticLevel::from_str("warning"),
            DiagnosticLevel::Warning
        );
        assert_eq!(DiagnosticLevel::from_str("help"), DiagnosticLevel::Help);
    }

    /// 构造所有权错误诊断（rustc JSON 结构）
    fn create_ownership_diagnostic(
        error_code: &str,
        message: &str,
        primary_span: DiagnosticSpan,
        other_spans: Vec<DiagnosticSpan>,
    ) -> CompilerDiagnostic {
        CompilerDiagnostic {
            message: message.to_string(),
            code: Some(DiagnosticCode {
                code: error_code.to_string(),
                explanation: None,
            }),
            level: "error".to_string(),
            spans: std::iter::once(primary_span).chain(other_spans).collect(),
            children: vec![],
            rendered: None,
        }
    }

    fn create_span(line_start: u32, label: &str, is_primary: bool) -> DiagnosticSpan {
        DiagnosticSpan {
            file_name: "src/main.rs".to_string(),
            line_start,
            column_start: 5,
            line_end: line_start,
            column_end: 10,
            source_text: None,
            byte_start: None,
            byte_end: None,
            is_primary,
            label: Some(label.to_string()),
            suggested_replacement: None,
        }
    }

    /// 教学提示须全部输出
    ///
    /// 回归：`format_as_text` 曾只用 `teaching_hints.first()`，第二条及以后的
    /// 教学提示被静默丢弃，而多处诊断（修复建议 + 所有权说明等）会产出多条。
    #[test]
    fn test_format_as_text_outputs_all_teaching_hints() {
        let diag = TeachingDiagnostic {
            level: DiagnosticLevel::Error,
            error_code: Some("E0308".to_string()),
            translated_message: "类型不匹配".to_string(),
            original_message: "mismatched types".to_string(),
            teaching_hints: vec![
                "第一条提示".to_string(),
                "第二条提示".to_string(),
                "第三条提示".to_string(),
            ],
            locations: vec![DiagnosticLocation {
                file_name: "src/main.zh".to_string(),
                line_start: 3,
                column_start: 5,
                line_end: 3,
                column_end: 10,
                source_text: Some("    让 x = 1;".to_string()),
                label: None,
                is_primary: true,
            }],
            children: Vec::new(),
            ownership_details: None,
        };

        let text = diag.format_as_text();
        assert_eq!(
            text.matches("💡").count(),
            3,
            "教学提示应全部输出，实际输出：{text}"
        );
        for hint in ["第一条提示", "第二条提示", "第三条提示"] {
            assert!(text.contains(hint), "教学提示 `{hint}` 丢失：{text}");
        }
    }

    #[test]
    fn test_extract_ownership_details_e0382() {
        // narrative_text 按当前语言取模板，需钉住 zh 并串行化
        let _guard = crate::语言::test_language("zh");
        // rustc 对 E0382 输出：主 span 是再次使用处，副 span 标记移动发生
        let diagnostic = create_ownership_diagnostic(
            "E0382",
            "use of moved value: `数据`",
            create_span(5, "value used here after move", true),
            vec![create_span(
                3,
                "move occurs because `数据` has type `String`, which does not implement the `Copy` trait",
                false,
            )],
        );

        let details = extract_ownership_details("E0382", &diagnostic).expect("应提取出所有权详情");
        assert_eq!(details.var_name, "数据");
        assert_eq!(details.move_location.as_ref().unwrap().line_start, 3);
        assert_eq!(details.reuse_location.as_ref().unwrap().line_start, 5);
        assert!(details.borrow_location.is_none());
        assert_eq!(
            details.narrative_text(),
            "变量 `数据` 在第 3 行被移动，第 5 行尝试再次使用。"
        );
    }

    #[test]
    fn test_extract_ownership_details_e0502() {
        let _guard = crate::语言::test_language("zh");
        // E0502：主 span 是可变借用处，另有不可变借用处与 borrow later used here
        let diagnostic = create_ownership_diagnostic(
            "E0502",
            "cannot borrow `向量` as mutable because it is also borrowed as immutable",
            create_span(6, "mutable borrow occurs here", true),
            vec![
                create_span(4, "immutable borrow occurs here", false),
                create_span(7, "borrow later used here", false),
            ],
        );

        let details = extract_ownership_details("E0502", &diagnostic).expect("应提取出所有权详情");
        assert_eq!(details.var_name, "向量");
        assert_eq!(details.borrow_location.as_ref().unwrap().line_start, 6);
        // "borrow later used here" 应归为再次使用而非借用发生
        assert_eq!(details.reuse_location.as_ref().unwrap().line_start, 7);
        assert!(details.move_location.is_none());
        assert_eq!(
            details.narrative_text(),
            "变量 `向量` 在第 6 行被借用，第 7 行仍在被使用。"
        );
    }

    #[test]
    fn test_extract_ownership_details_e0507() {
        let _guard = crate::语言::test_language("zh");
        // E0507：主 span 即移动发生处，无再次使用位置
        let diagnostic = create_ownership_diagnostic(
            "E0507",
            "cannot move out of `数据` which is behind a shared reference",
            create_span(
                3,
                "move occurs because `数据` has type `String`, which does not implement the `Copy` trait",
                true,
            ),
            vec![],
        );

        let details = extract_ownership_details("E0507", &diagnostic).expect("应提取出所有权详情");
        assert_eq!(details.var_name, "数据");
        assert_eq!(details.move_location.as_ref().unwrap().line_start, 3);
        assert!(details.reuse_location.is_none());
        assert_eq!(details.narrative_text(), "变量 `数据` 在第 3 行被移动。");
    }

    #[test]
    fn test_extract_ownership_details_non_ownership_error_returns_none() {
        // E0308 类型不匹配不是所有权错误，不应提取详情
        let diagnostic = create_test_diagnostic();
        assert!(extract_ownership_details("E0308", &diagnostic).is_none());
    }

    #[test]
    fn test_ownership_details_serialize_to_json() {
        let diagnostic = create_ownership_diagnostic(
            "E0382",
            "use of moved value: `数据`",
            create_span(5, "value used here after move", true),
            vec![create_span(
                3,
                "move occurs because `数据` has type `String`",
                false,
            )],
        );
        let details = extract_ownership_details("E0382", &diagnostic).unwrap();

        let value = serde_json::to_value(&details).expect("所有权详情应可序列化");
        assert_eq!(value["变量名"], "数据");
        assert_eq!(value["移动发生"]["起始行"], 3);
        assert_eq!(value["再次使用"]["起始行"], 5);
        assert_eq!(value["借用发生"], serde_json::Value::Null);
    }

    #[test]
    fn test_translate_diagnostic_with_ownership_details_and_narrative() {
        let _guard = crate::语言::test_language("zh");
        let toml_content = r#"
[E0382]
"消息模板" = "值在移动后被使用：`{变量名}`"
"教学提示" = "Rust 中值被移动后不能再使用。"
"#;
        let manager = ErrorTranslationManager::load_from_string(toml_content).unwrap();
        let translator = DiagnosticTranslator::new(manager, create_test_type_map());

        let diagnostic = create_ownership_diagnostic(
            "E0382",
            "use of moved value: `数据`",
            create_span(5, "value used here after move", true),
            vec![create_span(
                3,
                "move occurs because `数据` has type `String`",
                false,
            )],
        );
        let teaching = translator.translate_diagnostic(&diagnostic);

        assert!(teaching.ownership_details.is_some());
        let details = teaching.ownership_details.as_ref().unwrap();
        assert_eq!(details.var_name, "数据");

        let text = teaching.format_as_text();
        assert!(text.contains("📌 变量 `数据` 在第 3 行被移动，第 5 行尝试再次使用。"));
        assert!(text.contains("💡 Rust 中值被移动后不能再使用。"));
    }

    /// 反引号首段提取：路径取首段，含空格的自由文本不提取
    #[test]
    fn test_extract_backtick_first_segments() {
        assert_eq!(
            extract_backtick_first_segments("unresolved imports `a`, `b::c`"),
            vec!["a", "b"]
        );
        assert_eq!(
            extract_backtick_first_segments("unresolved import `serde_json::Value`"),
            vec!["serde_json"]
        );
        assert!(extract_backtick_first_segments("expected type `i32 x`").is_empty());
        assert!(extract_backtick_first_segments("无反引号消息").is_empty());
    }

    /// 整词替换不误伤标识符子串；边界（串首/串尾/空格）正常命中
    #[test]
    fn test_replace_whole_word_boundary() {
        assert_eq!(
            replace_whole_word("expected integer, found &str", "integer", "整数"),
            "expected 整数, found &str"
        );
        // 标识符子串不替换
        assert_eq!(
            replace_whole_word("no method named to_integer", "integer", "整数"),
            "no method named to_integer"
        );
        assert_eq!(
            replace_whole_word("integer_count", "integer", "整数"),
            "integer_count"
        );
        // 串首/串尾边界
        assert_eq!(replace_whole_word("integer", "integer", "整数"), "整数");
    }

    /// 未解析导入识别：E0432/E0433 两种消息格式命中，其他消息不命中
    #[test]
    fn test_is_unresolved_import_message() {
        assert!(is_unresolved_import_message(
            "unresolved import `serde_json`"
        ));
        assert!(is_unresolved_import_message(
            "failed to resolve: use of undeclared crate or module `tokio`"
        ));
        // 翻译后的母语消息不命中（由调用方按错误码兼容处理）
        assert!(!is_unresolved_import_message("未解析的导入 `serde_json`"));
        assert!(!is_unresolved_import_message("unused variable `x`"));
    }

    /// 候选 crate 提取：去重 + 排除标准库与保留路径；非目标消息返回空
    #[test]
    fn test_unresolved_crate_candidates() {
        assert_eq!(
            unresolved_crate_candidates("unresolved import `serde_json`"),
            vec!["serde_json"]
        );
        assert_eq!(
            unresolved_crate_candidates("unresolved imports `tokio`, `tokio::time`, `std::io`"),
            vec!["tokio"]
        );
        assert!(unresolved_crate_candidates("unresolved import `self::inner`").is_empty());
        assert!(unresolved_crate_candidates("mismatched types").is_empty());
    }

    /// 母语标识符不是 crate 名：非 ASCII 段被过滤，不再误提示 `rzc add 数据模型`
    #[test]
    fn test_unresolved_crate_candidates_filters_non_ascii() {
        assert!(unresolved_crate_candidates("unresolved import `数据模型::规则类型`").is_empty());
        assert!(
            unresolved_crate_candidates("unresolved imports `数据模型`, `serde`")
                .iter()
                .all(|s| s == "serde")
        );
    }

    /// 错误码条目是完整桩：消息表命中也不能回拼英文残段
    ///（回归：E0624 输出 "字段或方法是私有的year` is private"）
    #[test]
    fn test_translate_code_entry_no_message_rest_concat() {
        let _guard = crate::语言::test_language("zh");
        let toml_content = r#"
[E0624]
"消息模板" = "字段或方法 `{名称}` 是私有的"
"教学提示" = "结构体字段默认私有。"

["消息翻译"."method `"]
"消息模板" = "方法 `{q0}` 从未被使用"
"#;
        let file = create_error_message_file(toml_content);
        let manager = ErrorTranslationManager::load_from_file(file.path()).unwrap();
        let translator = DiagnosticTranslator::new(manager, create_test_type_map());

        let diagnostic = CompilerDiagnostic {
            message: "method `year` is private".to_string(),
            code: Some(DiagnosticCode {
                code: "E0624".to_string(),
                explanation: None,
            }),
            level: "error".to_string(),
            spans: vec![],
            children: vec![],
            rendered: None,
        };
        let teaching = translator.translate_diagnostic(&diagnostic);

        // {名称} 由消息反引号内容回填；不得粘上 "year` is private" 英文残段
        assert_eq!(teaching.translated_message, "字段或方法 `year` 是私有的");
    }

    /// 前缀键 + 动态名后的附加短语：{q0} 取首个反引号前内容而非第二个引号对
    ///（回归：输出 "特征 ` which provides ` 从未被使用"）
    #[test]
    fn test_translate_trait_which_provides_capture() {
        let _guard = crate::语言::test_language("zh");
        let toml_content = r#"
["消息翻译"."trait `"]
"消息模板" = "特征 `{q0}` 从未被使用"
"#;
        let file = create_error_message_file(toml_content);
        let manager = ErrorTranslationManager::load_from_file(file.path()).unwrap();
        let translator = DiagnosticTranslator::new(manager, create_test_type_map());

        let diagnostic = CompilerDiagnostic {
            message: "trait `Datelike` which provides `year` is never used".to_string(),
            code: None,
            level: "warning".to_string(),
            spans: vec![],
            children: vec![],
            rendered: None,
        };
        let teaching = translator.translate_diagnostic(&diagnostic);

        assert_eq!(teaching.translated_message, "特征 `Datelike` 从未被使用");
    }

    /// 后缀链 + 双占位符（复数变体）：头段引号对完整，{q0}/{q1} 分别取首个/末个
    #[test]
    fn test_translate_variants_suffix_captures() {
        let _guard = crate::语言::test_language("zh");
        let toml_content = r#"
["消息翻译"."~ are never constructed"]
"消息模板" = "`{q0}` 和 `{q1}` 从未被构造"
"#;
        let file = create_error_message_file(toml_content);
        let manager = ErrorTranslationManager::load_from_file(file.path()).unwrap();
        let translator = DiagnosticTranslator::new(manager, create_test_type_map());

        let diagnostic = CompilerDiagnostic {
            message: "variants `黄灯` and `绿灯` are never constructed".to_string(),
            code: None,
            level: "warning".to_string(),
            spans: vec![],
            children: vec![],
            rendered: None,
        };
        let teaching = translator.translate_diagnostic(&diagnostic);

        assert_eq!(teaching.translated_message, "`黄灯` 和 `绿灯` 从未被构造");
    }

    /// 构造带主 span 标签的诊断（模拟 rustc JSON：主消息 + primary label）
    fn diag_with(code: Option<&str>, message: &str, label: Option<&str>) -> CompilerDiagnostic {
        CompilerDiagnostic {
            message: message.to_string(),
            code: code.map(|c| DiagnosticCode {
                code: c.to_string(),
                explanation: None,
            }),
            level: "error".to_string(),
            spans: vec![DiagnosticSpan {
                file_name: "src/main.rs".to_string(),
                line_start: 3,
                column_start: 5,
                line_end: 3,
                column_end: 10,
                source_text: None,
                byte_start: None,
                byte_end: None,
                is_primary: true,
                label: label.map(|s| s.to_string()),
                suggested_replacement: None,
            }],
            children: vec![],
            rendered: None,
        }
    }

    /// CLI/LSP 主消息译文 parity（P1-1）：同一 (code, message, 主 span 标签) 下，
    /// LSP 复用的 `render_main_message` 与 CLI 的 `translate_diagnostic` 主消息
    /// 必须逐字一致，杜绝“同诊断 CLI 详、LSP 略”的分叉。
    #[test]
    fn cli_lsp_main_message_parity() {
        let _guard = crate::语言::test_language("zh");
        let toml_content = r#"
[E0308]
"消息模板" = "类型不匹配：期望 `{期望}`，实际得到 `{实际}`"
[E0433]
"消息模板" = "未找到类型 `{名称}`"
[E0369]
"消息模板" = "类型不匹配：无法对 `{期望}` 和 `{实际}` 执行运算"
[E0382]
"消息模板" = "值在移动后被使用：`{变量名}`"

["消息翻译"."unused variable: `"]
"消息模板" = "未使用的变量：`{q0}`"

["消息翻译"."mismatched types"]
"消息模板" = "类型不匹配"
"#;
        let manager = ErrorTranslationManager::load_from_string(toml_content).unwrap();
        let translator = DiagnosticTranslator::new(manager, create_test_type_map());

        // 均为“可完整渲染命中”的代表性用例（占位符可从 label/消息回填）；
        // 错误码桩无法回填时 CLI 回英文、LSP 回消息表属预期分叉，不纳入 parity。
        let cases: Vec<(Option<&str>, &str, Option<&str>)> = vec![
            (
                Some("E0308"),
                "mismatched types",
                Some("expected `i32`, found `String`"),
            ),
            (Some("E0433"), "cannot find type `Foo` in this scope", None),
            (Some("E0369"), "cannot add `{integer}` to `&str`", None),
            (
                Some("E0382"),
                "use of moved value: `数据`",
                Some("value used here after move"),
            ),
            (None, "unused variable: `x`", None),
            (None, "mismatched types", None),
        ];
        for (code, message, label) in cases {
            let cli = translator
                .translate_diagnostic(&diag_with(code, message, label))
                .translated_message;
            let lsp = translator
                .render_main_message(code, message, label)
                .unwrap_or_else(|| panic!("LSP 应命中表并渲染：{:?}", (code, message, label)))
                .text;
            assert_eq!(
                cli,
                lsp,
                "CLI/LSP 主消息译文不一致，case={:?}",
                (code, message, label)
            );
        }
    }

    /// E0583 码模板的 `{模块名}` 必须从消息反引号内容回填，
    /// 不得输出字面 `{模块名}`（多层模块断裂假红回归）
    #[test]
    fn e0583_module_name_placeholder_filled() {
        let _guard = crate::语言::test_language("zh");
        let toml_content = r#"
[E0583]
"消息模板" = "找不到模块 `{模块名}` 对应的文件"
"教学提示" = "模块声明要求同级目录存在同名文件。"
"#;
        let manager = ErrorTranslationManager::load_from_string(toml_content).unwrap();
        let translator = DiagnosticTranslator::new(manager, create_test_type_map());
        let code = Some("E0583");
        let message = "file not found for module `工具`";

        let rendered = translator
            .render_main_message(code, message, None)
            .expect("E0583 应命中码表并渲染");
        assert!(
            rendered.text.contains("工具") && !rendered.text.contains("{模块名}"),
            "LSP 路径应回填模块名：{}",
            rendered.text
        );

        let cli = translator
            .translate_diagnostic(&diag_with(code, message, None))
            .translated_message;
        assert!(
            cli.contains("工具") && !cli.contains("{模块名}"),
            "CLI 路径应回填模块名：{cli}"
        );
    }

    /// 门禁：en 是身份语言，它的错误码模板必须逐字复现 rustc 自己的英文消息。
    ///
    /// 起因：en 包的 `[E0597]` 抄了 `[E0596]` 的文案（"cannot borrow `{名称}` as
    /// mutable"），于是"引用比原值活得久"在英语读者面前显示成一句无关的可变借用
    /// 报错，而第十四章《生命周期》正要把这条诊断原样印进书里。检索按错误码命中，
    /// 模板与原文不符不会让任何既有测试失败，因此用实机捕获的
    /// (错误码, rustc 原文) 断言"渲染结果 == 输入原文"来锁住这类抄写错误。
    #[test]
    fn en_pack_code_templates_reproduce_rustc_own_wording() {
        // 消息取自 `rustc --edition 2021` 实机输出；反引号内的变量名随源程序变化
        let cases = [
            ("E0106", "missing lifetime specifier"),
            (
                "E0596",
                "cannot borrow `s` as mutable, as it is not declared as mutable",
            ),
            ("E0597", "`s` does not live long enough"),
        ];
        let files = crate::语言::builtin_lang_files("en");
        let errors_toml = files
            .iter()
            .find(|(name, _)| *name == "errors.toml")
            .map(|(_, content)| *content)
            .expect("en 语言包缺少 errors.toml");
        let manager = ErrorTranslationManager::load_from_string(errors_toml)
            .expect("en errors.toml 应能解析");
        // 空类型表：en 不做类型名中文化，渲染结果应与 rustc 原文逐字相同
        let translator = DiagnosticTranslator::new(manager, HashMap::new());
        for (code, rustc_message) in cases {
            let rendered = translator
                .render_main_message(Some(code), rustc_message, None)
                .unwrap_or_else(|| panic!("en 包应命中 [{code}] 错误码模板"));
            assert_eq!(
                rendered.text, rustc_message,
                "[{code}] 模板与 rustc 英文原文不一致，英语读者会看到答非所问的诊断"
            );
            assert!(rendered.hint.is_some(), "[{code}] 缺少教学提示");
        }
    }

    /// 门禁：错误码模板不得替 rustc 断言「成员种类」。
    ///
    /// 起因：en 包 `[E0603]` 写死 "function `{名称}` is private"，而 rustc 按实际
    /// 种类输出 module / constant / function——`mod shelf` 少了 `pub` 时读者看到的是
    /// 「函数 shelf 是私有的」，第十七章《模块系统》正好教「沿路径逐层补 `pub`」，
    /// 答错种类会把读者往错误的一层引。`[E0425]` 同型：rustc 有
    /// "cannot find value/function ..." 多种形式，en/ja/de 的模板写死「值」，
    /// 遇到函数名、结构体名一律说错。
    ///
    /// 一份模板对多种原文，种类名词无法从码表还原，故统一改为**不点名种类**的
    /// 中立措辞——丢一个名词好过给一个错名词。条目名（`{名称}`）必须保留，
    /// 否则读者不知道路径上哪一层断了。
    #[test]
    fn code_templates_do_not_misname_item_kind() {
        // (语言, 错误码, rustc 原文, 模板里写死的种类词)
        let cases = [
            ("en", "E0603", "module `shelf` is private", "function"),
            ("en", "E0603", "constant `SECRET` is private", "function"),
            (
                "en",
                "E0425",
                "cannot find function `wash_veg` in this scope",
                "value",
            ),
            (
                "ja",
                "E0425",
                "cannot find function `wash_veg` in this scope",
                "値",
            ),
            (
                "de",
                "E0425",
                "cannot find function `wash_veg` in this scope",
                "Wert",
            ),
        ];
        for (lang, code, rustc_message, wrong_noun) in cases {
            let files = crate::语言::builtin_lang_files(lang);
            let errors_toml = files
                .iter()
                .find(|(name, _)| *name == "errors.toml")
                .map(|(_, content)| *content)
                .unwrap_or_else(|| panic!("{lang} 语言包缺少 errors.toml"));
            let manager = ErrorTranslationManager::load_from_string(errors_toml)
                .unwrap_or_else(|e| panic!("{lang} errors.toml 解析失败：{e}"));
            let translator = DiagnosticTranslator::new(manager, HashMap::new());
            let rendered = translator
                .render_main_message(Some(code), rustc_message, None)
                .unwrap_or_else(|| panic!("{lang} 包应命中 [{code}] 错误码模板"));
            assert!(
                !rendered.text.contains(wrong_noun),
                "{lang} 包 [{code}] 对 {rustc_message:?} 输出了错误的成员种类 {wrong_noun:?}：{}",
                rendered.text
            );
            let name = rustc_message
                .split('`')
                .nth(1)
                .expect("样例消息应含反引号内的条目名");
            assert!(
                rendered.text.contains(name),
                "{lang} 包 [{code}] 丢失了条目名 {name:?}：{}",
                rendered.text
            );
        }
    }

    /// 门禁：rustc 的「`--extern` 收到非 ASCII 包名」硬错必须在 10 语言都有译文。
    ///
    /// 起因：bin+lib 方言项目（`src/lib.zh` + `src/main.zh`）用中文包名时，Cargo 要用
    /// `--extern 计算器=…` 把库交给主程序，rustc 立刻拒绝（实测触发条件只是「两种目标
    /// 共存 + 非 ASCII 包名」，主程序**完全不引用库**也撞；只有库或只有主程序都能编）。
    /// 这条无错误码的消息此前 11 语言全都没有条目：终端喷英文原句，还被刷进待译清单，
    /// 新手从 `is not a valid ASCII identifier` 读不出「该改 Cargo.toml 的包名」。
    ///
    /// 断言用实机采集的原文，锁住通配段键命中与 `{q0}`/`{q1}` 回填；顺带锁一条渲染陷阱：
    /// `replace_type_names` 会按类型映射改写译文里**任何**反引号英文 token（zh 的
    /// stdlib 表里 `线程名` = `name`），模板若写「把 `name` 改成 ASCII 名」会被改成
    /// 「把 `线程名` 改成 ASCII 名」——故 zh 模板不加反引号，末尾按 CLI 同口径复现一次。
    #[test]
    fn non_ascii_extern_crate_name_is_localized_in_all_lang_packs() {
        use crate::mapping_manager::MappingManager;
        // `rzc check src/main.zh`（包名 `计算器`，同时有 src/lib.zh）实机原文
        let raw = "crate name `计算器` passed to `--extern` is not a valid ASCII identifier";
        for lang in ["zh", "ja", "ru", "de", "es", "fr", "pt", "hi", "ko", "ar"] {
            let files = crate::语言::builtin_lang_files(lang);
            let errors_toml = files
                .iter()
                .find(|(name, _)| *name == "errors.toml")
                .map(|(_, content)| *content)
                .unwrap_or_else(|| panic!("{lang} 语言包缺少 errors.toml"));
            let manager = ErrorTranslationManager::load_from_string(errors_toml)
                .unwrap_or_else(|e| panic!("{lang} errors.toml 解析失败：{e}"));
            let translator = DiagnosticTranslator::new(manager, HashMap::new());
            let rendered = translator
                .render_main_message(None, raw, None)
                .unwrap_or_else(|| panic!("{lang} 包应命中 `--extern` 非 ASCII 包名的消息表"));
            assert!(
                rendered.text.contains("计算器"),
                "{lang} 译文丢了包名：{}",
                rendered.text
            );
            assert!(
                !rendered.text.contains("is not a valid ASCII identifier"),
                "{lang} 译文残留英文原句：{}",
                rendered.text
            );
        }

        // zh 走 CLI 真实渲染口径（带类型映射）：译文不得被改成「`线程名`」
        let lang_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh");
        let type_map = build_type_map(
            &MappingManager::load_from_dir(&lang_dir).expect("内置 zh 语言包应可加载"),
        );
        let files = crate::语言::builtin_lang_files("zh");
        let errors_toml = files
            .iter()
            .find(|(name, _)| *name == "errors.toml")
            .map(|(_, content)| *content)
            .expect("zh 语言包缺少 errors.toml");
        let manager = ErrorTranslationManager::load_from_string(errors_toml).expect("zh 应可解析");
        let rendered = DiagnosticTranslator::new(manager, type_map)
            .render_main_message(None, raw, None)
            .expect("zh 应命中消息表");
        assert!(
            !rendered.text.contains("线程名"),
            "zh 模板里的反引号英文词被类型映射改写了，读者会被告知去改一个不存在的东西：{}",
            rendered.text
        );
    }

    /// 门禁：语言包的教学提示不得残留引擎占位符。
    ///
    /// 占位符回填（`translator::CODE_TEMPLATE_PLACEHOLDERS`）只作用于 `消息模板`，
    /// `教学提示` 是从 errors.toml 原样送到终端的——提示里写 `{名称}` 这类占位符
    /// 不会被替换，读者就会在自己的母语指引中看到一串中文花括号。
    /// 起因：en 包 [E0596] 的提示是 "Add `mut` to the variable binding:
    /// `let mut {名称} = ...;`"，ar/es/fr/hi/ko/pt/ru 的 [E0309] 含 `{特征}`；
    /// 第十五章《集合类型》引用 E0596 的英文提示做实机核对时才暴露。
    #[test]
    fn pack_teaching_hints_never_leak_engine_placeholders() {
        fn collect_hints<'a>(value: &'a toml::Value, out: &mut Vec<&'a str>) {
            match value {
                toml::Value::Table(map) => {
                    for (key, child) in map.iter() {
                        if key == "教学提示" {
                            if let toml::Value::String(text) = child {
                                out.push(text);
                            }
                        } else {
                            collect_hints(child, out);
                        }
                    }
                }
                toml::Value::Array(items) => {
                    for item in items {
                        collect_hints(item, out);
                    }
                }
                _ => {}
            }
        }

        let mut total = 0;
        for lang in crate::语言::builtin_language_codes() {
            let files = crate::语言::builtin_lang_files(lang);
            let errors_toml = files
                .iter()
                .find(|(name, _)| *name == "errors.toml")
                .map(|(_, content)| *content)
                .unwrap_or_else(|| panic!("{lang} 语言包缺少 errors.toml"));
            let parsed: toml::Value = toml::from_str(errors_toml)
                .unwrap_or_else(|e| panic!("{lang} errors.toml 解析失败：{e}"));
            let mut hints = Vec::new();
            collect_hints(&parsed, &mut hints);
            assert!(
                hints.len() >= 20,
                "{lang} 包只取到 {} 条教学提示，清单提取可能有误",
                hints.len()
            );
            for hint in &hints {
                for placeholder in super::translator::CODE_TEMPLATE_PLACEHOLDERS {
                    assert!(
                        !hint.contains(placeholder),
                        "{lang} 包的教学提示残留占位符 {placeholder}，会原样输出给读者：{hint}"
                    );
                }
            }
            total += hints.len();
        }
        assert!(total > 0, "应至少检查一条教学提示");
    }
}
