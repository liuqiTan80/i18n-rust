// 诊断模块 - 解析 rustc 的 JSON 错误输出，结合语言包翻译成中文教学诊断信息
//
// 将 rustc 编译器产生的 JSON 格式诊断信息（--error-format=json）解析为结构化数据，
// 再根据语言包中的错误消息翻译表，生成面向中文教学场景的诊断输出，
// 包含错误码解释、教学提示、所有权错误叙事化详情等。
//
// 子模块划分（与本文件此前的分段注释一致）：
// - [`model`]：rustc JSON 诊断数据结构
// - [`message`]：错误消息翻译结构（errors.toml）
// - [`teaching`]：教学诊断结构与所有权叙事化详情
// - [`translator`]：诊断翻译器（类型映射支持）
// - [`output`]：JSON 解析、格式化输出与未解析导入检测

mod message;
mod model;
mod output;
mod teaching;
mod translator;

pub use message::ErrorTranslationManager;
pub use model::{CompilerDiagnostic, DiagnosticCode, DiagnosticSpan};
pub use output::{
    FormattedDiagnostic, extract_backtick_first_segments, is_unresolved_import_message,
    parse_diagnostic_output, unresolved_crate_candidates,
};
pub use teaching::{
    DiagnosticLevel, DiagnosticLocation, OWNERSHIP_ERROR_CODES, OwnershipDetails,
    TeachingDiagnostic, extract_ownership_details,
};
pub use translator::DiagnosticTranslator;

// 内部辅助函数：仅测试直接引用（见下方 tests 模块）
#[cfg(test)]
pub(crate) use translator::{
    count_ref_prefix, localize_ref_type, localize_type_token, replace_type_token,
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
}
