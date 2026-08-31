// 全角标点检测与修复模块
// 中文（及日/韩等）输入法下，用户常以全角标点书写代码（如 `打印行！（"你好"）；`），
// 这些字符在 Rust 语法中非法或会形成错误标识符，是初学者最常见的编译错误来源。
// 本模块在词法处理前扫描源码，仅报告代码位置（字符串字面量与注释内的全角标点
// 合法，不报告）的全角标点：
// - 有明确半角对应（，；：（）！？等）→ 提示对应字符，`rzc check --fix` 可自动转换
// - 无一对一映射的中文标点（、 与全角空格等）→ 仅提示，需人工修改

/// 全角标点 → 半角映射表（可自动修复的字符）
///
/// 覆盖中文输入法全角模式下可输入的所有常见标点；
/// 字符串/注释内的全角标点是合法内容，由调用方保证仅扫描代码位置。
const FIXABLE_PAIRS: &[(char, char)] = &[
    ('，', ','),
    ('。', '.'),
    ('；', ';'),
    ('：', ':'),
    ('！', '!'),
    ('？', '?'),
    ('（', '('),
    ('）', ')'),
    ('【', '['),
    ('】', ']'),
    ('｛', '{'),
    ('｝', '}'),
    ('「', '['),
    ('」', ']'),
    ('『', '['),
    ('』', ']'),
    ('〔', '('),
    ('〕', ')'),
    ('“', '"'),
    ('”', '"'),
    ('‘', '\''),
    ('’', '\''),
    ('～', '~'),
    ('＠', '@'),
    ('＃', '#'),
    ('％', '%'),
    ('＆', '&'),
    ('＊', '*'),
    ('＋', '+'),
    ('－', '-'),
    ('＝', '='),
    ('／', '/'),
    ('＼', '\\'),
    ('｜', '|'),
    ('＜', '<'),
    ('＞', '>'),
    ('＿', '_'),
    ('＄', '$'),
    ('＾', '^'),
];

/// 无一对一映射、仅提示不可自动修复的字符
const HINT_ONLY: &[char] = &['、', '　', '·'];

/// 单个全角标点警告：位置（1 起行/列）+ 字符 + 建议的半角字符
#[derive(Debug, Clone, PartialEq)]
pub struct FullwidthWarning {
    /// 行号（从 1 开始）
    pub line: usize,
    /// 列号（从 1 开始，按字符计数）
    pub column: usize,
    /// 全角字符
    pub character: char,
    /// 建议的半角字符；None 表示无对应（仅提示）
    pub replacement: Option<char>,
}

impl FullwidthWarning {
    /// 格式化为单行警告文本，模板随当前语言变化
    /// 示例（zh）：`第 1 行第 5 列：检测到全角标点「，」，应改为半角「,」`
    pub fn format(&self) -> String {
        let guidance = match self.replacement {
            Some(repl) => crate::语言::f("fullwidth_to_half", &[&repl.to_string()]),
            None => crate::语言::t("fullwidth_hint_only"),
        };
        crate::语言::f(
            "fullwidth_warning_at",
            &[
                &self.line.to_string(),
                &self.column.to_string(),
                &self.character.to_string(),
                &guidance,
            ],
        )
    }
}

/// 查找代码位置（非字符串/注释）的全角标点
///
/// 状态机跳过字符串字面量（含转义与原始字符串 r#""#）、字符字面量、
/// 行注释与块注释；其余位置出现的全角标点均报告。
/// 字符串/注释内的列号只保证大致正确（这些位置不产生警告）。
pub fn find_fullwidth_punct(source: &str) -> Vec<FullwidthWarning> {
    let chars: Vec<char> = source.chars().collect();
    let mut warnings = Vec::new();
    let mut line = 1usize;
    let mut col = 1usize;
    // 字面量状态：0 = 代码位置；1=双引号串 2=字符 3=原始串
    let mut literal: u8 = 0;
    let mut raw_hashes = 0usize; // 原始字符串的 # 数量
    // 保存上一个非空白字符（用于识别 r#"..."/b"..." 前缀与注释 `//`）
    let mut prev_plain = '\0';
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\n' {
            line += 1;
            col = 1;
            prev_plain = '\0';
            if literal == 2 || literal == 3 {
                // 防御：字符/原始串内出现未闭合引号时在行尾强制退出，
                // 避免状态污染后续行（编译器本就会报错，此处只为正确报告全角标点）
                literal = 0;
            }
            i += 1;
            continue;
        }
        match literal {
            1 | 2 => {
                // 字符串/字符字面量：转义跳过下一字符，闭合引号退出
                if ch == '\\' {
                    i += 2;
                    col += 1;
                    continue;
                }
                let closing = if literal == 1 { '"' } else { '\'' };
                if ch == closing {
                    literal = 0;
                }
            }
            3 => {
                // 原始字符串 r#"..."#：遇 " 后跟等量 #（或行尾/EOF）退出
                if ch == '"' {
                    let hashes = chars[i + 1..].iter().take_while(|c| **c == '#').count();
                    if hashes >= raw_hashes {
                        literal = 0;
                        raw_hashes = 0;
                    }
                }
            }
            _ => match ch {
                '"' => literal = 1,
                '\'' => literal = 2,
                // 行注释：吞到行尾（\n 留给主循环统一处理）
                '/' if prev_plain == '/' => {
                    col -= 1; // 回退第一个 '/' 已计的列
                    while i < chars.len() && chars[i] != '\n' {
                        i += 1;
                        col += 1;
                    }
                    prev_plain = '\0';
                    continue;
                }
                // 块注释（不处理嵌套，够用）
                '*' if prev_plain == '/' => {
                    col -= 1;
                    while i < chars.len() {
                        let c2 = chars[i];
                        i += 1;
                        if c2 == '\n' {
                            line += 1;
                            col = 1;
                        } else if c2 == '*' && i < chars.len() && chars[i] == '/' {
                            i += 1;
                            col += 2;
                            break;
                        } else {
                            col += 1;
                        }
                    }
                    prev_plain = '\0';
                    continue;
                }
                // 原始字符串前缀 r#"..."#（r 或 br 后跟 " 或 #）
                'r' | 'b' if is_raw_prefix(prev_plain) => {
                    // 前瞻后续字符：连续 # 后遇 " 才进入原始串
                    let mut n_hashes = 0usize;
                    let mut j = i + 1;
                    while j < chars.len() && chars[j] == '#' {
                        n_hashes += 1;
                        j += 1;
                    }
                    if j < chars.len() && chars[j] == '"' {
                        raw_hashes = n_hashes;
                        literal = 3;
                        // 消费前缀字符（r + # 序列 + 引号）
                        i = j + 1;
                        col += 1 + n_hashes + 1;
                        continue;
                    }
                }
                c if FIXABLE_PAIRS.iter().any(|(f, _)| *f == c) => {
                    let replacement = FIXABLE_PAIRS
                        .iter()
                        .find(|(f, _)| *f == c)
                        .map(|(_, half)| *half);
                    warnings.push(FullwidthWarning {
                        line,
                        column: col,
                        character: c,
                        replacement,
                    });
                }
                c if HINT_ONLY.contains(&c) => {
                    warnings.push(FullwidthWarning {
                        line,
                        column: col,
                        character: c,
                        replacement: None,
                    });
                }
                _ => {}
            },
        }
        i += 1;
        col += 1;
        if ch != ' ' && ch != '\t' && literal == 0 {
            prev_plain = ch;
        }
    }
    warnings
}

/// 判断前一字符是否可作为原始字符串前缀（r#/br#：前一字为 行首/空白/运算符）
fn is_raw_prefix(prev: char) -> bool {
    prev == '\0' || prev.is_whitespace() || "([{<,;:!?&|=+-*/%^".contains(prev)
}

/// 自动修复代码位置的全角标点，返回（修复后文本, 替换数量）
///
/// 仅替换 [`FIXABLE_PAIRS`] 中的字符；顿号/全角空格等仅提示字符保持原样。
/// 字符串与注释内容不受影响。
pub fn fix_fullwidth_punct(source: &str) -> (String, usize) {
    let chars: Vec<char> = source.chars().collect();
    let mut result = String::with_capacity(source.len());
    let mut count = 0usize;
    let mut literal: u8 = 0;
    let mut raw_hashes = 0usize;
    let mut prev_plain = '\0';
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\n' {
            prev_plain = '\0';
            if literal == 2 || literal == 3 {
                literal = 0;
            }
            result.push(ch);
            i += 1;
            continue;
        }
        match literal {
            1 | 2 => {
                result.push(ch);
                if ch == '\\' {
                    if let Some(next) = chars.get(i + 1) {
                        result.push(*next);
                    }
                    i += 2;
                    continue;
                }
                let closing = if literal == 1 { '"' } else { '\'' };
                if ch == closing {
                    literal = 0;
                }
            }
            3 => {
                result.push(ch);
                if ch == '"' {
                    let hashes = chars[i + 1..].iter().take_while(|c| **c == '#').count();
                    if hashes >= raw_hashes {
                        literal = 0;
                        raw_hashes = 0;
                    }
                }
            }
            _ => match ch {
                '"' => {
                    literal = 1;
                    result.push(ch);
                }
                '\'' => {
                    literal = 2;
                    result.push(ch);
                }
                '/' if prev_plain == '/' => {
                    result.push(ch);
                    i += 1;
                    while i < chars.len() && chars[i] != '\n' {
                        result.push(chars[i]);
                        i += 1;
                    }
                    prev_plain = '\0';
                    continue;
                }
                '*' if prev_plain == '/' => {
                    result.push(ch);
                    i += 1;
                    while i < chars.len() {
                        let c2 = chars[i];
                        result.push(c2);
                        i += 1;
                        if c2 == '\n' {
                            break;
                        }
                        if c2 == '*' && i < chars.len() && chars[i] == '/' {
                            result.push('/');
                            i += 1;
                            break;
                        }
                    }
                    prev_plain = '\0';
                    continue;
                }
                'r' | 'b' if is_raw_prefix(prev_plain) => {
                    let mut n_hashes = 0usize;
                    let mut j = i + 1;
                    while j < chars.len() && chars[j] == '#' {
                        n_hashes += 1;
                        j += 1;
                    }
                    if j < chars.len() && chars[j] == '"' {
                        raw_hashes = n_hashes;
                        literal = 3;
                        result.push(ch);
                        for c in &chars[i + 1..=j] {
                            result.push(*c);
                        }
                        i = j + 1;
                        continue;
                    }
                    result.push(ch);
                }
                c if FIXABLE_PAIRS.iter().any(|(f, _)| *f == c) => {
                    let replacement = FIXABLE_PAIRS
                        .iter()
                        .find(|(f, _)| *f == c)
                        .map(|(_, half)| *half)
                        .unwrap();
                    result.push(replacement);
                    count += 1;
                }
                _ => result.push(ch),
            },
        }
        i += 1;
        if ch != ' ' && ch != '\t' && literal == 0 {
            prev_plain = ch;
        }
    }
    (result, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_code_position_fullwidth() {
        let warnings = find_fullwidth_punct("函数 主函数（）{\n    打印行！（\"你好\"）；\n}");
        let chars: Vec<char> = warnings.iter().map(|w| w.character).collect();
        assert_eq!(chars, vec!['（', '）', '！', '（', '）', '；']);
        assert_eq!(warnings[0].line, 1);
        assert_eq!(warnings[0].column, 7);
        assert_eq!(warnings[0].replacement, Some('('));
    }

    #[test]
    fn ignores_strings_and_comments() {
        let source = "打印行!(\"你好，世界！？\"); // 注释：这里随便，。！\n/* 块注释：（）； */\n让 文本 = \"括号（）内的不算\";";
        let warnings = find_fullwidth_punct(source);
        assert!(
            warnings.is_empty(),
            "字符串/注释内全角标点不应报告：{warnings:?}"
        );
    }

    #[test]
    fn ignores_char_literal_and_raw_string() {
        let source = "让 字符 = '，';  // 字符字面量\n让 原始 = r#\"（全角）\"#;";
        let warnings = find_fullwidth_punct(source);
        assert!(warnings.is_empty(), "字符/原始字符串不应报告：{warnings:?}");
    }

    #[test]
    fn hint_only_chars_have_no_replacement() {
        let warnings = find_fullwidth_punct("让 顿号、= 1;");
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].character, '、');
        assert_eq!(warnings[0].replacement, None);
    }

    #[test]
    fn escaped_quote_inside_string() {
        let source = "让 文本 = \"包含\\\"引号（全角）\";";
        let warnings = find_fullwidth_punct(source);
        assert!(warnings.is_empty(), "转义串不应报告：{warnings:?}");
    }

    #[test]
    fn fix_replaces_only_code_position() {
        let source = "函数 主函数（）{\n    打印行！（\"你好，世界\"）；\n}";
        let (fixed, count) = fix_fullwidth_punct(source);
        assert_eq!(count, 6);
        assert_eq!(fixed, "函数 主函数(){\n    打印行!(\"你好，世界\");\n}");
        // 字符串内的全角逗号保留
        assert!(fixed.contains("你好，世界"));
    }

    #[test]
    fn fix_keeps_hint_only_chars() {
        let (fixed, count) = fix_fullwidth_punct("让 甲、乙 = 1；");
        assert_eq!(count, 1, "只有全角分号被替换");
        assert_eq!(fixed, "让 甲、乙 = 1;");
    }

    #[test]
    fn fix_keeps_raw_strings_untouched() {
        let source = "让 原始 = r#\"（全角）\"#;\n打印行！(\"x\");";
        let (fixed, count) = fix_fullwidth_punct(source);
        assert_eq!(count, 1);
        assert!(
            fixed.contains("r#\"（全角）\"#"),
            "原始字符串应保留：{fixed}"
        );
    }
}
