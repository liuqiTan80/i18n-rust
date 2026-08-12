// 模块：词法处理
// 功能：将母语源码根据关键字映射转译为标准 Rust 源码

use rustc_lexer::{tokenize, TokenKind};
use std::collections::HashMap;

/// 将母语 Rust 源代码转换为标准 Rust 源代码字符串
/// 参数：
///   源码: 母语源代码（如 .zh 文件内容）
///   关键字映射: 母语关键字到英文关键字的映射表
/// 返回：标准 Rust 源代码
pub fn 转译源码(源码: &str, 关键字映射: &HashMap<String, String>) -> String {
    let 令牌流 = tokenize(源码);
    let mut 输出 = String::new();
    let mut 当前偏移 = 0;

    for 令牌 in 令牌流 {
        let 长度 = 令牌.len as usize;
        let 文本 = &源码[当前偏移..当前偏移 + 长度];
        match 令牌.kind {
            TokenKind::Ident => {
                // 处理标识符，检查是否命中关键字映射
                if let Some(替换文本) = 处理标识符(文本, 关键字映射) {
                    输出.push_str(&替换文本);
                } else {
                    输出.push_str(文本);
                }
            }
            TokenKind::RawIdent => {
                // 处理原始标识符 r#xxx
                if let Some(替换文本) = 处理标识符(文本, 关键字映射) {
                    输出.push_str(&替换文本);
                } else {
                    输出.push_str(文本);
                }
            }
            // 其他所有 token 直接原样输出
            _ => 输出.push_str(文本),
        }
        当前偏移 += 长度;
    }

    输出
}

/// 对单个标识符文本进行关键字替换逻辑
fn 处理标识符(文本: &str, 关键字映射: &HashMap<String, String>) -> Option<String> {
    // 如果是 r# 开头的原始标识符
    if 文本.starts_with("r#") {
        let 内部 = &文本[2..];
        if 关键字映射.contains_key(内部) {
            let 英文关键字 = &关键字映射[内部];
            return Some(format!("r#{}", 英文关键字));
        }
    } else if 关键字映射.contains_key(文本) {
        // 普通中文关键字直接替换为英文
        return Some(关键字映射[文本].clone());
    }
    // 不是关键字，保留原样
    None
}

#[cfg(test)]
mod 测试 {
    use super::*;
    use std::collections::HashMap;

    fn 创建测试映射() -> HashMap<String, String> {
        HashMap::from([
            ("函数".to_string(), "fn".to_string()),
            ("让".to_string(), "let".to_string()),
            ("可变".to_string(), "mut".to_string()),
            ("如果".to_string(), "if".to_string()),
            ("否则".to_string(), "else".to_string()),
        ])
    }

    #[test]
    fn 测试简单替换() {
        let 映射 = 创建测试映射();
        let 源码 = "让 可变 x = 5;";
        let 期望 = "let mut x = 5;";
        assert_eq!(转译源码(源码, &映射), 期望);
    }

    #[test]
    fn 测试不改变普通标识符() {
        let 映射 = 创建测试映射();
        let 源码 = "让 变量名 = 42;";
        let 期望 = "let 变量名 = 42;";
        assert_eq!(转译源码(源码, &映射), 期望);
    }

    #[test]
    fn 测试保留注释和字符串() {
        let 映射 = 创建测试映射();
        let 源码 = "// 这是注释 函数\n让 s = \"这是字符串 函数\";";
        let 期望 = "// 这是注释 函数\nlet s = \"这是字符串 函数\";";
        assert_eq!(转译源码(源码, &映射), 期望);
    }

    #[test]
    fn 测试原始标识符处理() {
        // 原始标识符用于保留关键字（如 match 是 Rust 保留字）
        let mut 映射 = 创建测试映射();
        映射.insert("匹配".to_string(), "match".to_string());
        let 源码 = "让 r#匹配 = 1;";
        let 期望 = "let r#match = 1;";
        assert_eq!(转译源码(源码, &映射), 期望);
    }

    #[test]
    fn 测试保留非映射标识符() {
        let 映射 = 创建测试映射();
        let 源码 = "函数 主函数() { }";
        let 期望 = "fn 主函数() { }";
        assert_eq!(转译源码(源码, &映射), 期望);
    }
}