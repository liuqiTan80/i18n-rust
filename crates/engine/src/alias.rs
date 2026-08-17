// 别名替换模块
// 将源代码中的中文标识符别名（如第三方库的中文名称）替换为英文标识符。
// 仅替换标识符类型的 token，不触碰字符串字面量和注释等内容。
// 声明位保护：紧跟在声明关键字（fn/struct/let 等）后的标识符是用户自己的定义，
// 不是库 API 引用，不参与别名替换；且用户声明的名字在整个文件内的
// 裸使用处都豁免（两遍扫描：先收集声明名，再逐 token 替换），
// 避免 `let 新建 = 5` 声明位受保护而后续使用处被误替换成 new。
// 例外：`::` 限定后的路径段（如 `字符串::新建`）是库 API 的限定访问，
// 用户变量/类型不可能经 `::` 访问，仍照常替换。

use rustc_lexer::{TokenKind, tokenize};
use std::collections::{HashMap, HashSet};

/// 声明位关键字（转译后的英文形式）：其后紧跟的标识符为用户定义
///
/// 不含 `mut`：仅 `让 mut 名称` 场景下透明传递声明状态（见替换循环），
/// `&mut 类型` 等非声明位的 mut 不传递，避免误保护库类型引用。
/// 不含 `impl`：`impl 特征 for 类型` 中的特征/类型多为库 API，仍需替换。
/// LSP 列映射模拟别名替换时复用同一张表，保证两侧行为一致。
pub const DECL_KEYWORDS: &[&str] = &[
    "fn", "struct", "enum", "trait", "type", "mod", "let", "const", "static",
];

/// 收集用户在声明位定义的标识符名（第一遍扫描）
///
/// 状态机与替换主循环一致：声明关键字后紧跟的标识符计入集合；
/// `mut` 在声明态内透明传递；空白/注释不打断声明态；符号终结声明位。
/// 集合内的名字在整个文件的所有出现处都豁免别名替换（保守近似：
/// 不做作用域分析，同名遮蔽场景同样豁免，与声明位保护的设计意图一致）。
pub fn collect_declared_names(source: &str) -> HashSet<String> {
    let mut declared = HashSet::new();
    let mut prev_is_decl = false;
    let mut offset = 0;
    for token in tokenize(source) {
        let text = &source[offset..offset + token.len];
        match token.kind {
            TokenKind::Ident => {
                if prev_is_decl {
                    declared.insert(text.to_string());
                }
                prev_is_decl = if text == "mut" {
                    prev_is_decl
                } else {
                    DECL_KEYWORDS.contains(&text)
                };
            }
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. } => {}
            _ => prev_is_decl = false,
        }
        offset += token.len;
    }
    declared
}

/// 将源码中的中文别名替换为英文标识符（仅替换标识符 token）
///
/// 输入应为已完成关键字转译的代码（声明关键字已是英文形式）。
/// 声明位的标识符（如 `fn 绝对值()`、`让 字符串 = 1`）保留原样，
/// 且这些用户声明名在文件内的裸使用处也不替换；但 `::` 限定后的
/// 路径段（库 API 限定访问，如 `字符串::新建`）不受豁免，照常替换。
///
/// # 参数
/// - `source`: 待替换的源代码字符串
/// - `alias_map`: 中文别名 → 英文原名 的映射表
///
/// # 返回
/// 替换后的源代码字符串
pub fn replace_aliases(source: &str, alias_map: &HashMap<String, String>) -> String {
    // 映射表为空时直接返回，避免不必要的词法分析开销
    if alias_map.is_empty() {
        return source.to_string();
    }
    // 第一遍：收集用户声明名，裸使用处豁免
    let declared = collect_declared_names(source);
    // 单个 Colon 跟踪：rustc_lexer 把 `::` 拆成两个 Colon token
    let mut last_was_colon = false;
    let token_stream = tokenize(source);
    let mut output = String::new();
    let mut current_offset = 0;
    // 上一个有意义 token 是否为声明关键字；空白/注释不重置该状态
    let mut prev_is_decl = false;
    // 上一个有意义 token 是否为 `::`（两个连续 Colon token 的第二个）；
    // `::` 后的路径段是库 API 限定访问，不受用户声明名豁免
    let mut prev_is_path_sep = false;

    for token in token_stream {
        let len = token.len;
        let text = &source[current_offset..current_offset + len];
        match token.kind {
            TokenKind::Ident => {
                let exempt = prev_is_decl || (declared.contains(text) && !prev_is_path_sep);
                if exempt {
                    // 声明位标识符或用户声明名的裸使用处：保留原样
                    output.push_str(text);
                } else if let Some(english) = alias_map.get(text) {
                    output.push_str(english);
                } else {
                    output.push_str(text);
                }
                // `让 mut 名称`：声明位内的 mut 透明传递声明状态；
                // `&mut 类型` 等非声明位的 mut 不传递，库类型引用仍被替换
                prev_is_decl = if text == "mut" {
                    prev_is_decl
                } else {
                    DECL_KEYWORDS.contains(&text)
                };
                prev_is_path_sep = false;
            }
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. } => {
                output.push_str(text);
                // 空白与注释不打断声明状态与路径限定状态
            }
            _ => {
                output.push_str(text);
                // 符号终结声明位；连续两个 Colon 构成 `::` 路径限定
                prev_is_decl = false;
                prev_is_path_sep = if matches!(token.kind, TokenKind::Colon) {
                    prev_is_path_sep || last_was_colon
                } else {
                    false
                };
                last_was_colon = matches!(token.kind, TokenKind::Colon);
            }
        }
        current_offset += len;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alias_map() -> HashMap<String, String> {
        HashMap::from([
            ("绝对值".to_string(), "abs".to_string()),
            ("字符串".to_string(), "String".to_string()),
        ])
    }

    #[test]
    fn test_method_call_and_type_position_replaced() {
        let out = replace_aliases("let s: 字符串 = x.绝对值();", &alias_map());
        assert_eq!(out, "let s: String = x.abs();");
    }

    #[test]
    fn test_declaration_sites_preserved() {
        let out = replace_aliases("fn 绝对值() {}\nlet 字符串 = 1;", &alias_map());
        assert_eq!(out, "fn 绝对值() {}\nlet 字符串 = 1;");
    }

    #[test]
    fn test_declared_name_usage_sites_preserved() {
        // 用户声明名在使用处同样豁免：声明位保护不能只保护声明点
        let out = replace_aliases("let 字符串 = 1;\nlet y = 字符串 + 2;", &alias_map());
        assert_eq!(out, "let 字符串 = 1;\nlet y = 字符串 + 2;");
    }

    #[test]
    fn test_undeclared_alias_usage_still_replaced() {
        // 无用户声明撞名时，使用处照常替换
        let out = replace_aliases("let y = 字符串::from(x);", &alias_map());
        assert_eq!(out, "let y = String::from(x);");
    }

    #[test]
    fn test_qualified_path_segment_not_exempt() {
        // `::` 限定后的路径段是库 API 访问，不受用户声明名豁免；
        // 裸使用处（第一行声明与第三行变量引用）仍豁免
        let map = HashMap::from([
            ("字符串".to_string(), "String".to_string()),
            ("新建".to_string(), "new".to_string()),
        ]);
        let out = replace_aliases(
            "let 新建 = 5;\nlet s = 字符串::新建();\nlet y = 新建;",
            &map,
        );
        assert_eq!(out, "let 新建 = 5;\nlet s = String::new();\nlet y = 新建;");
    }

    #[test]
    fn test_struct_and_const_declaration_preserved() {
        let out = replace_aliases("struct 字符串;\nconst 绝对值: i32 = 1;", &alias_map());
        assert_eq!(out, "struct 字符串;\nconst 绝对值: i32 = 1;");
    }

    #[test]
    fn test_let_mut_declaration_preserved() {
        // mut 透明传递声明状态，变量名仍受保护
        let out = replace_aliases("let mut 绝对值 = 1;", &alias_map());
        assert_eq!(out, "let mut 绝对值 = 1;");
    }

    #[test]
    fn test_ref_mut_type_still_replaced() {
        // `&mut 类型` 是非声明位，库类型引用仍应替换
        let out = replace_aliases("fn f(x: &mut 字符串) {}", &alias_map());
        assert_eq!(out, "fn f(x: &mut String) {}");
    }

    #[test]
    fn test_comment_between_decl_keyword_and_name() {
        let out = replace_aliases("fn /* 说明 */ 绝对值() {}", &alias_map());
        assert_eq!(out, "fn /* 说明 */ 绝对值() {}");
    }

    #[test]
    fn test_impl_target_still_replaced() {
        // impl 后的特征/类型多为库 API，不在保护名单内
        let out = replace_aliases("impl 字符串 for 客户端 {}", &alias_map());
        assert_eq!(out, "impl String for 客户端 {}");
    }

    #[test]
    fn test_string_literal_and_comment_untouched() {
        let out = replace_aliases("println!(\"绝对值\"); // 绝对值", &alias_map());
        assert_eq!(out, "println!(\"绝对值\"); // 绝对值");
    }

    #[test]
    fn test_empty_map_returns_original() {
        assert_eq!(replace_aliases("任意内容", &HashMap::new()), "任意内容");
    }
}
