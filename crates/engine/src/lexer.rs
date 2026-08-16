// 词法处理模块 - 将母语源码根据关键字映射转译为标准 Rust 源码
//
// 基于 rustc_lexer 的 token 级转译，保证注释与字符串字面量内容不被误改。
// 支持宏感叹号自动补充、原始标识符（r#）处理、反向转译（Rust → 母语）。

use crate::cache::SourceMapEntry;
use rustc_lexer::{TokenKind, tokenize};
use std::collections::{HashMap, HashSet};

/// 转译结果：标准 Rust 输出与源映射
#[derive(Debug, Clone)]
pub struct TranspileResult {
    /// 转译后的代码文本
    pub output: String,
    /// 源映射条目列表（记录被替换的标识符）
    pub source_map: Vec<SourceMapEntry>,
}

/// 将母语 Rust 源代码转换为标准 Rust 源代码字符串
///
/// 参数：
///   source: 母语源代码（如 .zh 文件内容）
///   keyword_map: 母语关键字到英文关键字的映射表
/// 返回：标准 Rust 源代码
pub fn transpile_source(source: &str, keyword_map: &HashMap<String, String>) -> String {
    let empty_map = HashMap::new();
    transpile_source_with_macro_map(source, keyword_map, &empty_map)
}

/// 将母语 Rust 源代码转换为标准 Rust 源代码字符串（支持宏感叹号自动补充）
///
/// 参数：
///   source: 母语源代码（如 .zh 文件内容）
///   keyword_map: 母语关键字到英文关键字的映射表
///   macro_names: 所有中文宏名的集合（不含感叹号），用于自动补充 `!`
/// 返回：标准 Rust 源代码
///
/// 注意：宏名的英文替换优先使用宏映射（见 [`transpile_source_with_macro_map`]）；
/// 本函数以 keyword_map 兜底（宏名在 keyword_map 中被类型节覆盖时值可能不准，
/// 如 `向量` 在类型节映射为 `Vec`、在宏节映射为 `vec`）。
pub fn transpile_source_with_macros(
    source: &str,
    keyword_map: &HashMap<String, String>,
    macro_names: &HashSet<String>,
) -> String {
    let macro_map: HashMap<String, String> = macro_names
        .iter()
        .map(|name| {
            (
                name.clone(),
                keyword_map
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| name.clone()),
            )
        })
        .collect();
    transpile_with_map(source, keyword_map, &macro_map).output
}

/// 同 [`transpile_source_with_macros`]，但宏名英文替换来自宏映射表
/// （宏名 → 英文宏名，如 `向量` → `vec`），可避免宏名在类型节与宏节
/// 重复时被类型值覆盖（如 `向量` 类型节为 `Vec`、宏节为 `vec`）。
pub fn transpile_source_with_macro_map(
    source: &str,
    keyword_map: &HashMap<String, String>,
    macro_map: &HashMap<String, String>,
) -> String {
    transpile_with_map(source, keyword_map, macro_map).output
}

/// 同 [`transpile_source_with_macros`]，同时产出源映射（被替换标识符的源偏移与翻译前后文本）
///
/// 源映射在标识符实际被替换时记录；宏感叹号自动补充不产生映射条目。
pub fn transpile_with_map(
    source: &str,
    keyword_map: &HashMap<String, String>,
    macro_map: &HashMap<String, String>,
) -> TranspileResult {
    // 收集所有 token 以便前瞻/后顾
    let token_stream: Vec<_> = tokenize(source).collect();
    let mut output = String::new();
    let mut current_offset = 0;
    let mut source_map = Vec::new();

    for i in 0..token_stream.len() {
        let token = &token_stream[i];
        let length = token.len as usize;
        let text = &source[current_offset..current_offset + length];

        match token.kind {
            TokenKind::Ident | TokenKind::RawIdent => {
                let raw_name = text.strip_prefix("r#").unwrap_or(text);

                // 宏调用上下文优先：宏名后跟 `!` 或 `(/[/{`（且前面不是 `::`）时，
                // 使用宏映射替换为英文宏名并确保感叹号，避免宏名在类型节与宏节
                // 重复时被类型值覆盖（如 `向量` 类型节为 `Vec`、宏节为 `vec`）
                let mut handled = false;
                if macro_map.contains_key(raw_name)
                    && !is_preceded_by_double_colon(&token_stream, i)
                    && let Some(next_kind) = find_next_non_ws_kind(&token_stream, i + 1)
                {
                    let is_bang = matches!(next_kind, TokenKind::Not);
                    if is_open_bracket(next_kind) || is_bang {
                        let en_macro = macro_map.get(raw_name).cloned().unwrap_or_else(|| {
                            keyword_map
                                .get(raw_name)
                                .cloned()
                                .unwrap_or_else(|| text.to_string())
                        });
                        let final_text = if text.starts_with("r#") && !en_macro.starts_with("r#") {
                            format!("r#{}", en_macro)
                        } else {
                            en_macro
                        };
                        if final_text != text {
                            source_map.push(SourceMapEntry::new(
                                current_offset,
                                length,
                                text,
                                &final_text,
                            ));
                        }
                        output.push_str(&final_text);
                        // 后跟开括号但无 `!` 时自动补 `!`
                        if is_open_bracket(next_kind) {
                            output.push('!');
                        }
                        handled = true;
                    }
                }

                if !handled {
                    // 普通关键字替换
                    let replacement = if let Some(inner) = text.strip_prefix("r#") {
                        keyword_map
                            .get(inner)
                            .map(|en| format!("r#{}", en))
                            .unwrap_or_else(|| text.to_string())
                    } else {
                        keyword_map
                            .get(text)
                            .cloned()
                            .unwrap_or_else(|| text.to_string())
                    };

                    // 记录实际发生替换的标识符映射
                    if replacement != text {
                        source_map.push(SourceMapEntry::new(
                            current_offset,
                            length,
                            text,
                            &replacement,
                        ));
                    }
                    output.push_str(&replacement);
                }
            }
            // 其他所有 token 直接原样输出
            _ => output.push_str(text),
        }
        current_offset += length;
    }

    TranspileResult { output, source_map }
}

/// 查找从指定位置开始的第一个非空白 token 的 kind
fn find_next_non_ws_kind(token_stream: &[rustc_lexer::Token], start: usize) -> Option<TokenKind> {
    for token in &token_stream[start..] {
        if !is_whitespace(token.kind) {
            return Some(token.kind);
        }
    }
    None
}

/// 将标准 Rust 源码反向转译为母语源码
///
/// 参数：
///   source: 标准 Rust 源代码（如 rustfmt 格式化后的虚拟 .rs 内容）
///   reverse_map: 英文关键字到母语关键字的映射表（由正向映射反转得到）
///   module_names: 当前已打开 .zh 文件的模块名集合。
///              代理为跨文件引用插入的 `crate::` 前缀在此被删除，
///              还原为母语中的裸路径（`辅助::`），与正向翻译的
///              `replace_module_paths` 互为逆操作。
/// 返回：母语源代码
///
/// 以 token 为单位匹配，天然避免子串误替换
/// （如 `i32` 不会被更短的 `i3` 错误替换），
/// 注释与字符串字面量内容保持原样，与正向翻译一一对应。
pub fn reverse_transpile(
    source: &str,
    reverse_map: &HashMap<String, String>,
    module_names: &HashSet<String>,
) -> String {
    // 收集 (token 种类, 文本) 对以便前瞻/后顾
    let token_stream: Vec<(TokenKind, &str)> = {
        let mut list = Vec::new();
        let mut offset = 0;
        for token in tokenize(source) {
            list.push((token.kind, &source[offset..offset + token.len]));
            offset += token.len;
        }
        list
    };
    let mut output = String::with_capacity(source.len());
    let mut skip_colons = 0usize; // 删除 crate:: 前缀时顺带跳过的两个 Colon

    for i in 0..token_stream.len() {
        if skip_colons > 0 {
            skip_colons -= 1;
            continue;
        }
        let (token, text) = &token_stream[i];
        match token {
            TokenKind::Ident | TokenKind::RawIdent => {
                let raw_name = if let Some(inner) = text.strip_prefix("r#") {
                    inner
                } else {
                    text
                };
                // 代理为跨文件引用插入的 `crate::` 前缀：整体删除
                if raw_name == "crate" && is_followed_by_module_name(&token_stream, i, module_names)
                {
                    skip_colons = 2;
                    continue;
                }
                let zh = reverse_map
                    .get(raw_name)
                    .map(|s| s.as_str())
                    .unwrap_or(raw_name);
                if text.starts_with("r#") && !zh.starts_with("r#") {
                    output.push_str("r#");
                }
                output.push_str(zh);
            }
            _ => output.push_str(text),
        }
    }
    output
}

/// 检查指定位置之后（跳过空白与 `::`）的第一个标识符是否为已知模块名
fn is_followed_by_module_name(
    token_stream: &[(TokenKind, &str)],
    current: usize,
    module_names: &HashSet<String>,
) -> bool {
    let mut colon_count = 0;
    for (token, text) in &token_stream[(current + 1)..] {
        if is_whitespace(*token) {
            continue;
        }
        if matches!(token, TokenKind::Colon) {
            colon_count += 1;
            if colon_count > 2 {
                return false;
            }
            continue;
        }
        return matches!(token, TokenKind::Ident)
            && colon_count == 2
            && module_names.contains(*text);
    }
    false
}

/// 判断 token 是否为空白
fn is_whitespace(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
    )
}

/// 判断 token 是否为开括号（( [ {）
fn is_open_bracket(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::OpenParen | TokenKind::OpenBracket | TokenKind::OpenBrace
    )
}

/// 检查指定位置之前的非空白 token 是否构成 ::（双冒号）
fn is_preceded_by_double_colon(token_stream: &[rustc_lexer::Token], current_pos: usize) -> bool {
    // 从当前位置往前找，跳过空白，找前两个非空白 token
    let mut prev_one = None;
    let mut prev_two = None;

    let mut j = current_pos;
    while j > 0 {
        j -= 1;
        if !is_whitespace(token_stream[j].kind) {
            if prev_one.is_none() {
                prev_one = Some(token_stream[j].kind);
            } else if prev_two.is_none() {
                prev_two = Some(token_stream[j].kind);
                break;
            }
        }
    }

    // 前一个和前两个都是冒号 → 前面是 ::
    matches!(prev_one, Some(TokenKind::Colon)) && matches!(prev_two, Some(TokenKind::Colon))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::collections::HashSet;

    fn create_test_map() -> HashMap<String, String> {
        HashMap::from([
            ("函数".to_string(), "fn".to_string()),
            ("让".to_string(), "let".to_string()),
            ("可变".to_string(), "mut".to_string()),
            ("如果".to_string(), "if".to_string()),
            ("否则".to_string(), "else".to_string()),
            ("打印行".to_string(), "println".to_string()),
            ("打印".to_string(), "print".to_string()),
            ("格式化".to_string(), "format".to_string()),
            ("断言".to_string(), "assert".to_string()),
            ("断言相等".to_string(), "assert_eq".to_string()),
            ("向量".to_string(), "vec".to_string()),
        ])
    }

    fn create_macro_map() -> HashMap<String, String> {
        HashMap::from([
            ("打印行".to_string(), "println".to_string()),
            ("打印".to_string(), "print".to_string()),
            ("格式化".to_string(), "format".to_string()),
            ("断言".to_string(), "assert".to_string()),
            ("断言相等".to_string(), "assert_eq".to_string()),
            ("向量".to_string(), "vec".to_string()),
        ])
    }

    #[test]
    fn test_simple_replacement() {
        let map = create_test_map();
        let source = "让 可变 x = 5;";
        let expected = "let mut x = 5;";
        assert_eq!(transpile_source(source, &map), expected);
    }

    #[test]
    fn test_non_mapped_identifiers_preserved() {
        let map = create_test_map();
        let source = "让 变量名 = 42;";
        let expected = "let 变量名 = 42;";
        assert_eq!(transpile_source(source, &map), expected);
    }

    #[test]
    fn test_comments_and_strings_preserved() {
        let map = create_test_map();
        let source = "// 这是注释 函数\n让 s = \"这是字符串 函数\";";
        let expected = "// 这是注释 函数\nlet s = \"这是字符串 函数\";";
        assert_eq!(transpile_source(source, &map), expected);
    }

    #[test]
    fn test_raw_identifier_handling() {
        // 原始标识符用于保留关键字（如 match 是 Rust 保留字）
        let mut map = create_test_map();
        map.insert("匹配".to_string(), "match".to_string());
        let source = "让 r#匹配 = 1;";
        let expected = "let r#match = 1;";
        assert_eq!(transpile_source(source, &map), expected);
    }

    #[test]
    fn test_non_mapped_identifiers_kept() {
        let map = create_test_map();
        let source = "函数 主函数() { }";
        let expected = "fn 主函数() { }";
        assert_eq!(transpile_source(source, &map), expected);
    }

    // ===== 宏感叹号自动补充测试 =====

    #[test]
    fn test_macro_auto_exclamation() {
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "打印行(\"你好\")";
        let expected = "println!(\"你好\")";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros),
            expected
        );
    }

    #[test]
    fn test_macro_existing_exclamation_not_duplicated() {
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "打印行!(\"你好\")";
        let expected = "println!(\"你好\")";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros),
            expected
        );
    }

    #[test]
    fn test_full_program_macro_auto_exclamation() {
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "函数 主函数() { 打印行(\"你好\") }";
        // 注意：主函数 不在映射中，所以保持原样
        // 但 函数 → fn，打印行 → println!
        let actual = transpile_source_with_macro_map(source, &map, &macros);
        assert!(actual.contains("fn"));
        assert!(actual.contains("println!(\"你好\")"));
    }

    #[test]
    fn test_regular_function_call_not_exclamation() {
        let map = create_test_map();
        let macros = create_macro_map();
        // "从" 不在宏集合中，不应被加 !
        let source = "字符串::从(\"x\")";
        let expected = "字符串::从(\"x\")";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros),
            expected
        );
    }

    #[test]
    fn test_macro_after_double_colon_no_exclamation() {
        let map = create_test_map();
        let macros = create_macro_map();
        // 打印行 在宏集合中，但前面是 ::，不应加 !
        let source = "std::打印行(\"你好\")";
        let expected = "std::println(\"你好\")";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros),
            expected
        );
    }

    #[test]
    fn test_macro_followed_by_brackets() {
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "向量![1, 2, 3]";
        let expected = "vec![1, 2, 3]";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros),
            expected
        );
    }

    #[test]
    fn test_macro_followed_by_brackets_no_exclamation() {
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "向量[1, 2, 3]";
        let expected = "vec![1, 2, 3]";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros),
            expected
        );
    }

    #[test]
    #[test]
    fn test_macro_map_overrides_type_value() {
        // 模拟真实语言包：宏名同时出现在类型节与宏节（如 向量→Vec 与 向量→vec），
        // keyword_map 被类型节覆盖为 Vec，宏映射应保证宏调用输出 vec!
        let mut map = create_test_map();
        map.insert("向量".to_string(), "Vec".to_string());
        let macros = create_macro_map();
        let source = "让 v = 向量![1, 2, 3];";
        let expected = "let v = vec![1, 2, 3];";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros),
            expected
        );
    }

    #[test]
    fn test_empty_macro_set_no_exclamation() {
        let map = create_test_map();
        let empty = HashMap::new();
        let source = "打印行(\"你好\")";
        let expected = "println(\"你好\")"; // 不补 !
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &empty),
            expected
        );
    }

    #[test]
    fn test_multiple_macro_calls() {
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "打印行(\"甲\"); 打印(\"乙\")";
        let expected = "println!(\"甲\"); print!(\"乙\")";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros),
            expected
        );
    }

    // ===== 源映射测试 =====

    #[test]
    fn test_transpile_with_map_output_consistent() {
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "函数 主函数() { 让 x = 5; 打印行(\"你好\") }";
        let result = transpile_with_map(source, &map, &macros);
        assert_eq!(
            result.output,
            transpile_source_with_macro_map(source, &map, &macros)
        );
    }

    #[test]
    fn test_transpile_with_map_records_keyword_replacements() {
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "函数 主函数() { 让 x = 5; 打印行(\"你好\") }";
        let result = transpile_with_map(source, &map, &macros);

        // 函数/让/打印行 被替换，主函数 未命中映射不记录
        let fn_entry = result
            .source_map
            .iter()
            .find(|m| m.original == "函数")
            .expect("应有 函数 映射");
        assert_eq!(fn_entry.replacement, "fn");
        assert_eq!(
            &source[fn_entry.source_offset..fn_entry.source_offset + fn_entry.length],
            "函数"
        );

        let let_entry = result
            .source_map
            .iter()
            .find(|m| m.original == "让")
            .expect("应有 让 映射");
        assert_eq!(let_entry.replacement, "let");
        assert_eq!(
            &source[let_entry.source_offset..let_entry.source_offset + let_entry.length],
            "让"
        );

        // 宏名映射记录翻译文本（println），感叹号补充不产生额外条目
        let println_entry = result
            .source_map
            .iter()
            .find(|m| m.original == "打印行")
            .expect("应有 打印行 映射");
        assert_eq!(println_entry.replacement, "println");

        assert!(!result.source_map.iter().any(|m| m.original == "主函数"));
    }

    #[test]
    fn test_transpile_with_map_raw_identifier() {
        let mut map = create_test_map();
        map.insert("匹配".to_string(), "match".to_string());
        let empty = HashMap::new();
        let source = "让 r#匹配 = 1;";
        let result = transpile_with_map(source, &map, &empty);
        assert_eq!(result.output, "let r#match = 1;");
        let entry = result
            .source_map
            .iter()
            .find(|m| m.original == "r#匹配")
            .expect("应有原始标识符映射");
        assert_eq!(entry.replacement, "r#match");
    }

    // ===== 反向转译测试 =====

    fn create_reverse_map(forward: &HashMap<String, String>) -> HashMap<String, String> {
        forward
            .iter()
            .map(|(k, v)| (v.clone(), k.clone()))
            .collect()
    }

    #[test]
    fn test_reverse_transpile_basic() {
        // 关键字、类型名、宏名均被还原为母语，宏感叹号保留
        let forward = HashMap::from([
            ("函数".to_string(), "fn".to_string()),
            ("主函数".to_string(), "main".to_string()),
            ("让".to_string(), "let".to_string()),
            ("可变".to_string(), "mut".to_string()),
            ("整数".to_string(), "i32".to_string()),
            ("打印行".to_string(), "println".to_string()),
        ]);
        let reverse = create_reverse_map(&forward);
        let empty = HashSet::new();
        let source = "fn main() { let mut x: i32 = 5; println!(\"你好\"); }";
        let expected = "函数 主函数() { 让 可变 x: 整数 = 5; 打印行!(\"你好\"); }";
        assert_eq!(reverse_transpile(source, &reverse, &empty), expected);
    }

    #[test]
    fn test_reverse_transpile_preserves_comments_strings_custom_idents() {
        // 注释、字符串字面量、中文/英文自定义标识符均保持原样
        let reverse = HashMap::from([
            ("fn".to_string(), "函数".to_string()),
            ("let".to_string(), "让".to_string()),
        ]);
        let empty = HashSet::new();
        let source = "// fn 是关键字\nlet s = \"let\";\nlet 计数 = fn_value;";
        let expected = "// fn 是关键字\n让 s = \"let\";\n让 计数 = fn_value;";
        assert_eq!(reverse_transpile(source, &reverse, &empty), expected);
    }

    #[test]
    fn test_reverse_transpile_longer_word_priority() {
        // token 级匹配：i32 与 i3x 是完整 token，互不干扰（无子串误替换）
        let reverse = HashMap::from([
            ("i32".to_string(), "整数".to_string()),
            ("i3".to_string(), "三".to_string()),
        ]);
        let empty = HashSet::new();
        let source = "let a: i32 = 1; let b: i3x = 2; let c = i3;";
        // let 不在反向表中保持原样；i32→整数、i3→三，i3x 是完整 token 不受影响
        let expected = "let a: 整数 = 1; let b: i3x = 2; let c = 三;";
        assert_eq!(reverse_transpile(source, &reverse, &empty), expected);
    }

    #[test]
    fn test_reverse_transpile_module_prefix() {
        let forward = HashMap::from([
            ("函数".to_string(), "fn".to_string()),
            ("包".to_string(), "crate".to_string()),
        ]);
        let reverse = create_reverse_map(&forward);
        // 代理插入的 crate:: 前缀：后跟模块名时整体删除，还原为裸路径
        let set = HashSet::from(["辅助".to_string()]);
        assert_eq!(
            reverse_transpile("fn main() { crate::辅助::辅助函数(); }", &reverse, &set),
            "函数 main() { 辅助::辅助函数(); }"
        );
        // 用户显式书写的 包::：后跟非模块名时还原
        assert_eq!(
            reverse_transpile("fn main() { crate::外部函数(); }", &reverse, &set),
            "函数 main() { 包::外部函数(); }"
        );
    }

    #[test]
    fn test_reverse_transpile_raw_identifier() {
        let reverse = HashMap::from([
            ("let".to_string(), "让".to_string()),
            ("match".to_string(), "匹配".to_string()),
        ]);
        let empty = HashSet::new();
        assert_eq!(
            reverse_transpile("let r#match = 1;", &reverse, &empty),
            "让 r#匹配 = 1;"
        );
    }
}
