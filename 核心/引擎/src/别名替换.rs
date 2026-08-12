// 模块：别名替换
// 功能：将源代码中的中文标识符（如第三方库的别名）替换为英文

use std::collections::HashMap;
use rustc_lexer::{tokenize, TokenKind};

/// 将源码中的中文别名替换为英文标识符（仅替换标识符 token）
pub fn 替换别名(源码: &str, 别名映射: &HashMap<String, String>) -> String {
    if 别名映射.is_empty() {
        return 源码.to_string();
    }
    let 令牌流 = tokenize(源码);
    let mut 输出 = String::new();
    let mut 当前偏移 = 0;

    for 令牌 in 令牌流 {
        let 长度 = 令牌.len as usize;
        let 文本 = &源码[当前偏移..当前偏移 + 长度];
        match 令牌.kind {
            TokenKind::Ident => {
                // 如果是别名，替换；否则保留原样
                if let Some(英文) = 别名映射.get(文本) {
                    输出.push_str(英文);
                } else {
                    输出.push_str(文本);
                }
            }
            _ => 输出.push_str(文本),
        }
        当前偏移 += 长度;
    }
    输出
}