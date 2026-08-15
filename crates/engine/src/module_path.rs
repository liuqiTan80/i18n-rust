// 模块路径替换模块
// 将源代码中 `use` 语句的中文模块路径段替换为英文路径段。
// 例如将 `使用 标准集合::哈希映射` 替换为 `使用 std::collections::HashMap`。

use rustc_lexer::{TokenKind, tokenize};
use std::collections::HashMap;

/// 将源代码中的中文模块路径段替换为英文
///
/// # 参数
/// - `source`: 待替换的源代码字符串
/// - `path_map`: 中文路径段 → 英文路径段 的映射表
///
/// # 返回
/// 替换后的源代码字符串
///
/// # 注意
/// 采用 token 级替换（与 `replace_aliases` 一致）：
/// - 仅替换**完整标识符 token**，避免破坏组合词
///   （如 `读取全部字符串` 中的 `字符串` 不会被误替换）；
/// - 仅替换**后跟 `::` 的路径段**，避免误替换普通中文变量名；
/// - 路径段与 `::` 之间可能有空白，替换时仅替换路径段本身。
pub fn replace_module_paths(source: &str, path_map: &HashMap<String, String>) -> String {
    if path_map.is_empty() {
        return source.to_string();
    }
    let token_stream = tokenize(source);
    let tokens: Vec<_> = token_stream.collect();
    let mut output = String::new();
    let mut current_offset = 0;

    for (i, token) in tokens.iter().enumerate() {
        let len = token.len;
        let text = &source[current_offset..current_offset + len];
        match token.kind {
            TokenKind::Ident => {
                // 仅当该路径段后紧接 `::`（两个连续的 Colon token）时
                // 才视为模块路径并替换
                let is_path_segment = tokens
                    .get(i + 1)
                    .is_some_and(|t| matches!(t.kind, TokenKind::Colon))
                    && tokens
                        .get(i + 2)
                        .is_some_and(|t| matches!(t.kind, TokenKind::Colon));
                if is_path_segment {
                    if let Some(english) = path_map.get(text) {
                        output.push_str(english);
                    } else {
                        output.push_str(text);
                    }
                } else {
                    output.push_str(text);
                }
            }
            _ => output.push_str(text),
        }
        current_offset += len;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_path_map() -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("标准集合".to_string(), "std::collections".to_string());
        map.insert("文件系统".to_string(), "std::fs".to_string());
        map.insert("字符串".to_string(), "string".to_string());
        map
    }

    /// 模块路径段后跟 `::` 时被替换
    #[test]
    fn test_replace_module_path_segment() {
        let map = sample_path_map();
        let result = replace_module_paths("使用 标准集合::哈希映射;", &map);
        assert_eq!(result, "使用 std::collections::哈希映射;");
    }

    /// 组合词内部的中文不被破坏（token 级替换）
    #[test]
    fn test_compound_word_not_broken() {
        let map = sample_path_map();
        // `读取全部字符串` 是完整 token，其中的 `字符串` 不应被替换
        let result = replace_module_paths("读取全部字符串(\"a.txt\")", &map);
        assert_eq!(result, "读取全部字符串(\"a.txt\")");
    }

    /// 普通中文变量名（后无 `::`）不被替换
    #[test]
    fn test_plain_ident_not_replaced() {
        let map = sample_path_map();
        let result = replace_module_paths("让 字符串 = 1;", &map);
        assert_eq!(result, "让 字符串 = 1;");
    }

    /// 空映射表直接返回原文
    #[test]
    fn test_empty_map() {
        let result = replace_module_paths("让 x = 1;", &HashMap::new());
        assert_eq!(result, "让 x = 1;");
    }
}
