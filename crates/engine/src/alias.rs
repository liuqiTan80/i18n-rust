// 别名替换模块
// 将源代码中的中文标识符别名（如第三方库的中文名称）替换为英文标识符。
// 仅替换标识符类型的 token，不触碰字符串字面量和注释等内容。

use rustc_lexer::{TokenKind, tokenize};
use std::collections::HashMap;

/// 将源码中的中文别名替换为英文标识符（仅替换标识符 token）
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
    let token_stream = tokenize(source);
    let mut output = String::new();
    let mut current_offset = 0;

    for token in token_stream {
        let len = token.len;
        let text = &source[current_offset..current_offset + len];
        match token.kind {
            TokenKind::Ident => {
                // 如果是别名，替换为英文；否则保留原样
                if let Some(english) = alias_map.get(text) {
                    output.push_str(english);
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
