// 模块：词法处理
// 功能：将母语源码根据关键字映射转译为标准 Rust 源码

use rustc_lexer::{tokenize, TokenKind};
use std::collections::{HashMap, HashSet};

/// 将母语 Rust 源代码转换为标准 Rust 源代码字符串
/// 参数：
///   源码: 母语源代码（如 .zh 文件内容）
///   关键字映射: 母语关键字到英文关键字的映射表
/// 返回：标准 Rust 源代码
pub fn 转译源码(源码: &str, 关键字映射: &HashMap<String, String>) -> String {
    let 空集合 = HashSet::new();
    转译源码带宏集合(源码, 关键字映射, &空集合)
}

/// 将母语 Rust 源代码转换为标准 Rust 源代码字符串（支持宏感叹号自动补充）
/// 参数：
///   源码: 母语源代码（如 .zh 文件内容）
///   关键字映射: 母语关键字到英文关键字的映射表
///   宏名称集合: 所有中文宏名的集合（不含感叹号），用于自动补充 `!`
/// 返回：标准 Rust 源代码
pub fn 转译源码带宏集合(
    源码: &str,
    关键字映射: &HashMap<String, String>,
    宏名称集合: &HashSet<String>,
) -> String {
    // 收集所有 token 以便前瞻/后顾
    let 令牌流: Vec<_> = tokenize(源码).collect();
    let mut 输出 = String::new();
    let mut 当前偏移 = 0;

    for i in 0..令牌流.len() {
        let 令牌 = &令牌流[i];
        let 长度 = 令牌.len as usize;
        let 文本 = &源码[当前偏移..当前偏移 + 长度];

        match 令牌.kind {
            TokenKind::Ident | TokenKind::RawIdent => {
                // 处理标识符，检查是否命中关键字映射
                let 替换结果 = if 文本.starts_with("r#") {
                    let 内部 = &文本[2..];
                    关键字映射
                        .get(内部)
                        .map(|英文| format!("r#{}", 英文))
                        .unwrap_or_else(|| 文本.to_string())
                } else {
                    关键字映射
                        .get(文本)
                        .cloned()
                        .unwrap_or_else(|| 文本.to_string())
                };

                输出.push_str(&替换结果);

                // 宏感叹号自动补充：
                // 当标识符是宏名称、后面跟着 (/[/{ 且没有 !，且前面不是 :: 时，自动插入 !
                if !宏名称集合.is_empty() {
                    let 原始名称 = if 文本.starts_with("r#") {
                        &文本[2..]
                    } else {
                        文本
                    };

                    if 宏名称集合.contains(原始名称) && !前面是双冒号(&令牌流, i) {
                        if let Some(下一个kind) = 查找下一个非空白_kind(&令牌流, i + 1) {
                            if 是开括号(下一个kind) {
                                // 下一个有意义 token 是 ( [ {，需要补 !
                                输出.push('!');
                            }
                        }
                    }
                }
            }
            // 其他所有 token 直接原样输出
            _ => 输出.push_str(文本),
        }
        当前偏移 += 长度;
    }

    输出
}

/// 查找从指定位置开始的第一个非空白 token 的 kind
fn 查找下一个非空白_kind(
    令牌流: &[rustc_lexer::Token],
    起始: usize,
) -> Option<TokenKind> {
    for j in 起始..令牌流.len() {
        if !是空白(令牌流[j].kind) {
            return Some(令牌流[j].kind);
        }
    }
    None
}

/// 判断 token 是否为空白
fn 是空白(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::Whitespace | TokenKind::LineComment { .. } | TokenKind::BlockComment { .. })
}

/// 判断 token 是否为开括号（( [ {）
fn 是开括号(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::OpenParen | TokenKind::OpenBracket | TokenKind::OpenBrace
    )
}

/// 检查指定位置之前的非空白 token 是否构成 ::（双冒号）
fn 前面是双冒号(令牌流: &[rustc_lexer::Token], 当前位置: usize) -> bool {
    // 从当前位置往前找，跳过空白，找前两个非空白 token
    let mut 前一个 = None;
    let mut 前两个 = None;

    let mut j = 当前位置;
    while j > 0 {
        j -= 1;
        if !是空白(令牌流[j].kind) {
            if 前一个.is_none() {
                前一个 = Some(令牌流[j].kind);
            } else if 前两个.is_none() {
                前两个 = Some(令牌流[j].kind);
                break;
            }
        }
    }

    // 前一个和前两个都是冒号 → 前面是 ::
    matches!(前一个, Some(TokenKind::Colon))
        && matches!(前两个, Some(TokenKind::Colon))
}

#[cfg(test)]
mod 测试 {
    use super::*;
    use std::collections::HashMap;
    use std::collections::HashSet;

    fn 创建测试映射() -> HashMap<String, String> {
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

    fn 创建宏集合() -> HashSet<String> {
        HashSet::from([
            "打印行".to_string(),
            "打印".to_string(),
            "格式化".to_string(),
            "断言".to_string(),
            "断言相等".to_string(),
            "向量".to_string(),
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

    // ===== 宏感叹号自动补充测试 =====

    #[test]
    fn 测试宏自动补感叹号() {
        let 映射 = 创建测试映射();
        let 宏集合 = 创建宏集合();
        let 源码 = "打印行(\"你好\")";
        let 期望 = "println!(\"你好\")";
        assert_eq!(转译源码带宏集合(源码, &映射, &宏集合), 期望);
    }

    #[test]
    fn 测试宏已有感叹号不重复() {
        let 映射 = 创建测试映射();
        let 宏集合 = 创建宏集合();
        let 源码 = "打印行!(\"你好\")";
        let 期望 = "println!(\"你好\")";
        assert_eq!(转译源码带宏集合(源码, &映射, &宏集合), 期望);
    }

    #[test]
    fn 测试完整程序中宏自动补感叹号() {
        let 映射 = 创建测试映射();
        let 宏集合 = 创建宏集合();
        let 源码 = "函数 主函数() { 打印行(\"你好\") }";
        // 注意：主函数 不在映射中，所以保持原样
        // 但 函数 → fn，打印行 → println!
        let 实际 = 转译源码带宏集合(源码, &映射, &宏集合);
        assert!(实际.contains("fn"));
        assert!(实际.contains("println!(\"你好\")"));
    }

    #[test]
    fn 测试普通函数调用不误加感叹号() {
        let 映射 = 创建测试映射();
        let 宏集合 = 创建宏集合();
        // "从" 不在宏集合中，不应被加 !
        let 源码 = "字符串::从(\"x\")";
        let 期望 = "字符串::从(\"x\")";
        assert_eq!(转译源码带宏集合(源码, &映射, &宏集合), 期望);
    }

    #[test]
    fn 测试双冒号后的宏不加感叹号() {
        let 映射 = 创建测试映射();
        let 宏集合 = 创建宏集合();
        // 打印行 在宏集合中，但前面是 ::，不应加 !
        let 源码 = "std::打印行(\"你好\")";
        let 期望 = "std::println(\"你好\")";
        assert_eq!(转译源码带宏集合(源码, &映射, &宏集合), 期望);
    }

    #[test]
    fn 测试宏后跟方括号() {
        let 映射 = 创建测试映射();
        let 宏集合 = 创建宏集合();
        let 源码 = "向量![1, 2, 3]";
        let 期望 = "vec![1, 2, 3]";
        assert_eq!(转译源码带宏集合(源码, &映射, &宏集合), 期望);
    }

    #[test]
    fn 测试宏后跟方括号无感叹号() {
        let 映射 = 创建测试映射();
        let 宏集合 = 创建宏集合();
        let 源码 = "向量[1, 2, 3]";
        let 期望 = "vec![1, 2, 3]";
        assert_eq!(转译源码带宏集合(源码, &映射, &宏集合), 期望);
    }

    #[test]
    fn 测试空宏集合时不补感叹号() {
        let 映射 = 创建测试映射();
        let 空集合 = HashSet::new();
        let 源码 = "打印行(\"你好\")";
        let 期望 = "println(\"你好\")"; // 不补 !
        assert_eq!(转译源码带宏集合(源码, &映射, &空集合), 期望);
    }

    #[test]
    fn 测试多个宏调用() {
        let 映射 = 创建测试映射();
        let 宏集合 = 创建宏集合();
        let 源码 = "打印行(\"甲\"); 打印(\"乙\")";
        let 期望 = "println!(\"甲\"); print!(\"乙\")";
        assert_eq!(转译源码带宏集合(源码, &映射, &宏集合), 期望);
    }
}
