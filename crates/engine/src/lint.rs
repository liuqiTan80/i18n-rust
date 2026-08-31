// 教学 lint 模块
// 针对 Rust 初学者的启发式代码风格提示（教学价值驱动）：
// 与编译器错误不同，这些提示不阻断编译，只在代码位置出现常见
// 教学问题时给出建设性建议。规则刻意保守，避免噪音——教学工具的
// 提示必须每条都有价值：
// - 未标注类型：`让 x = 5;` 缺少类型注解（类型系统教学）
// - 魔法数字：非平凡数字字面量（命名常量教学）
// - 嵌套过深：代码行缩进过深（重构教学，每文件仅首个）
//
// 行注释含「教学忽略」时整行跳过（教师可标注故意不修的示例）：
// `让 x = 5;  // 教学忽略` 不产生任何教学警告。
//
// 扫描器与 fullwidth 模块同构：状态机跳过字符串/字符/原始字符串与
// 注释，仅在代码位置判定，避免把字符串内容误判为代码问题。

use std::collections::HashSet;

/// 教学 lint 规则种类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintKind {
    /// `让` 绑定缺少类型注解
    UntypedLet,
    /// 非平凡数字字面量（建议命名常量）
    MagicNumber,
    /// 代码行嵌套过深（建议拆分函数）
    DeepIndent,
}

/// 单个教学 lint 警告：位置（1 起行/列）+ 规则 + 相关文本
#[derive(Debug, Clone, PartialEq)]
pub struct LintWarning {
    /// 行号（从 1 开始）
    pub line: usize,
    /// 列号（从 1 开始，按字符计数）
    pub column: usize,
    /// 规则种类
    pub kind: LintKind,
    /// 相关文本（魔法数字的原文等；其他规则为空串）
    pub text: String,
}

impl LintWarning {
    /// 格式化为单行提示文本，模板随当前语言变化
    pub fn format(&self) -> String {
        match self.kind {
            LintKind::UntypedLet => crate::语言::f(
                "lint_untyped_let",
                &[&self.line.to_string(), &self.column.to_string()],
            ),
            LintKind::MagicNumber => crate::语言::f(
                "lint_magic_number",
                &[&self.line.to_string(), &self.column.to_string(), &self.text],
            ),
            LintKind::DeepIndent => {
                crate::语言::f("lint_deep_indent", &[&self.line.to_string(), &self.text])
            }
        }
    }
}

/// 进行中的 `让` 绑定扫描（等待语句结束判定是否有类型注解）
struct LetScan {
    line: usize,
    column: usize,
    /// 已见到 `=`（其后出现的 `:` 是路径分隔符而非类型注解）
    seen_eq: bool,
    /// 已见到 `:`（类型注解）
    annotated: bool,
}

/// 对源码执行教学 lint，返回全部警告（升序）
///
/// 仅报告代码位置的问题；字符串/注释内的同名文本被状态机跳过。
pub fn lint_teaching(source: &str) -> Vec<LintWarning> {
    let chars: Vec<char> = source.chars().collect();
    let mut warnings = Vec::new();
    let mut line = 1usize;
    let mut col = 1usize;
    // 字面量状态：0 = 代码位置；1 = 双引号串 2 = 字符 3 = 原始串
    let mut literal: u8 = 0;
    let mut raw_hashes = 0usize;
    let mut prev_plain = '\0';
    // 行首缩进追踪（仅代码行累计，用于嵌套深度提示）
    let mut line_indent = 0usize;
    let mut at_line_start = true;
    let mut first_nonspace_is_comment = false;
    let mut deep_indent_reported = false;
    // 未标注类型扫描状态
    let mut let_scan: Option<LetScan> = None;
    // 行注释含「教学忽略」的行号：整行跳过教学警告（教师标注故意不修的示例）
    let mut ignored_lines: HashSet<usize> = HashSet::new();

    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        // 行首缩进累计（独立于字面量状态：多行字符串内部行由 literal
        // 分支吞掉，不会走到这里；行首仅代码位置才累计）
        if at_line_start && literal == 0 {
            match ch {
                ' ' => {
                    line_indent += 1;
                    i += 1;
                    col += 1;
                    continue;
                }
                '\t' => {
                    line_indent += 4;
                    i += 1;
                    col += 1;
                    continue;
                }
                '\n' => {
                    line_indent = 0;
                    i += 1;
                    continue;
                }
                '/' if !first_nonspace_is_comment
                    && i + 1 < chars.len()
                    && (chars[i + 1] == '/' || chars[i + 1] == '*') =>
                {
                    // 注释行：不参与嵌套深度提示
                    first_nonspace_is_comment = true;
                    at_line_start = false;
                }
                _ => {
                    // 首个非空白字符（非注释）：代码行，判定嵌套深度
                    if line_indent >= DEEP_INDENT_THRESHOLD && !deep_indent_reported {
                        deep_indent_reported = true;
                        warnings.push(LintWarning {
                            line,
                            column: line_indent + 1,
                            kind: LintKind::DeepIndent,
                            text: line_indent.to_string(),
                        });
                    }
                    at_line_start = false;
                }
            }
        }
        if ch == '\n' {
            line += 1;
            col = 1;
            line_indent = 0;
            at_line_start = true;
            first_nonspace_is_comment = false;
            prev_plain = '\0';
            if literal == 2 || literal == 3 {
                // 防御：字符/原始串内出现未闭合引号时在行尾强制退出，
                // 避免状态污染后续行（编译器本就会报错，此处只为正确判定）
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
                    let comment_start = i;
                    col -= 1;
                    while i < chars.len() && chars[i] != '\n' {
                        i += 1;
                        col += 1;
                    }
                    // 注释含「教学忽略」：整行跳过教学警告
                    let comment: String = chars[comment_start..i].iter().collect();
                    if comment.contains(IGNORE_MARK) {
                        ignored_lines.insert(line);
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
                    let mut n_hashes = 0usize;
                    let mut j = i + 1;
                    while j < chars.len() && chars[j] == '#' {
                        n_hashes += 1;
                        j += 1;
                    }
                    if j < chars.len() && chars[j] == '"' {
                        raw_hashes = n_hashes;
                        literal = 3;
                        i = j + 1;
                        col += 1 + n_hashes + 1;
                        continue;
                    }
                }
                // `让` 关键字：开始未标注类型扫描（前后均须有标识符边界）
                '让' if is_ident_boundary(prev_plain)
                    && (i + 1 >= chars.len() || is_ident_boundary(chars[i + 1])) =>
                {
                    // 前一个 let 未闭合（上一语句缺分号）：先收尾
                    finish_let_scan(&mut let_scan, &mut warnings);
                    let_scan = Some(LetScan {
                        line,
                        column: col,
                        seen_eq: false,
                        annotated: false,
                    });
                }
                // 数字字面量（前一字不是标识符字符时才是独立字面量）
                c if c.is_ascii_digit() && is_ident_boundary(prev_plain) => {
                    i = scan_number(&chars, i, ch, &mut warnings, line, col, prev_plain);
                    continue;
                }
                _ => {}
            },
        }
        // let 扫描推进：仅在代码位置处理语句内符号
        if let Some(scan) = let_scan.as_mut() {
            match ch {
                ':' if !scan.seen_eq => scan.annotated = true,
                '=' => scan.seen_eq = true,
                ';' | '{' | '}' => finish_let_scan(&mut let_scan, &mut warnings),
                _ => {}
            }
        }
        i += 1;
        col += 1;
        if ch != ' ' && ch != '\t' && literal == 0 {
            prev_plain = ch;
        }
    }
    finish_let_scan(&mut let_scan, &mut warnings);
    // 过滤被「教学忽略」标记的行（三种规则统一按行过滤）
    warnings.retain(|w| !ignored_lines.contains(&w.line));
    warnings
}

/// 教学忽略标记：行注释含此文本时该行跳过全部教学警告
pub const IGNORE_MARK: &str = "教学忽略";

/// 嵌套深度阈值：前导缩进 ≥ 此空格数时提示（24 = 6 层，4 空格一层）
const DEEP_INDENT_THRESHOLD: usize = 24;

/// 判定字符是否可作为标识符边界（非字母/数字/下划线即边界）
fn is_ident_boundary(c: char) -> bool {
    !(c.is_alphanumeric() || c == '_')
}

/// 判定前一字符是否可作为原始字符串前缀（r#/br#：前一字为行首/空白/运算符）
fn is_raw_prefix(prev: char) -> bool {
    prev == '\0' || prev.is_whitespace() || "([{<,;:!?&|=+-*/%^".contains(prev)
}

/// 收尾进行中的 `让` 扫描：无类型注解时报告
fn finish_let_scan(let_scan: &mut Option<LetScan>, warnings: &mut Vec<LintWarning>) {
    if let Some(scan) = let_scan.take()
        && !scan.annotated
    {
        warnings.push(LintWarning {
            line: scan.line,
            column: scan.column,
            kind: LintKind::UntypedLet,
            text: String::new(),
        });
    }
}

/// 数字字面量扫描：返回结束位置（i 应前进到的下标）
///
/// 跳过 0x/0b/0o 前缀字面量；解析十进制（含小数点/科学计数法/下划线），
/// 值属于平凡集合（0/1/2 及其负数）时不报告。
fn scan_number(
    chars: &[char],
    start: usize,
    first: char,
    warnings: &mut Vec<LintWarning>,
    line: usize,
    column: usize,
    prev_plain: char,
) -> usize {
    let mut i = start;
    // 0x/0b/0o 前缀：跳过整个字面量（教学场景罕见，不提示）
    if first == '0' && i + 1 < chars.len() && matches!(chars[i + 1], 'x' | 'b' | 'o') {
        i += 2;
        while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
            i += 1;
        }
        return i;
    }
    let number_start = i;
    // 整数部分：[0-9_]+
    while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
        i += 1;
    }
    // 小数部分：. + [0-9_]+
    if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
        i += 1;
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
            i += 1;
        }
    }
    // 科学计数法：[eE][+-]?[0-9_]+
    if i < chars.len() && matches!(chars[i], 'e' | 'E') {
        let mut j = i + 1;
        if j < chars.len() && matches!(chars[j], '+' | '-') {
            j += 1;
        }
        if j < chars.len() && chars[j].is_ascii_digit() {
            i = j;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
                i += 1;
            }
        }
    }
    // 后随标识符字符（如 `42abc`）：非法代码，交给编译器，不提示
    if i < chars.len() && is_ident_continue(chars[i]) {
        return i;
    }
    let text: String = chars[number_start..i].iter().collect();
    // 平凡值排除：0/1/2（含浮点形式）与前导负号（-1/-2 等常见常量）
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    let is_trivial = cleaned
        .parse::<f64>()
        .is_ok_and(|v| v == 0.0 || v == 1.0 || v == 2.0);
    if is_trivial || prev_plain == '-' {
        return i;
    }
    warnings.push(LintWarning {
        line,
        column,
        kind: LintKind::MagicNumber,
        text,
    });
    i
}

/// 标识符续字符（数字后跟这些字符视为同一标识符）
fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_untyped_let_detected() {
        // 用平凡值 1 避免同时触发魔法数字提示
        let warnings = lint_teaching("函数 主函数() {\n    让 x = 1;\n}");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].kind, LintKind::UntypedLet);
        assert_eq!(warnings[0].line, 2);
        assert_eq!(warnings[0].column, 5);
    }

    #[test]
    fn test_annotated_let_ok() {
        let warnings = lint_teaching("函数 主函数() {\n    让 x: 整数 = 5;\n}");
        assert!(
            !warnings.iter().any(|w| w.kind == LintKind::UntypedLet),
            "{warnings:?}"
        );
    }

    /// 路径分隔符 `::` 出现在 `=` 之后时不是类型注解
    #[test]
    fn test_path_separator_after_eq_not_annotation() {
        let warnings = lint_teaching("函数 主函数() {\n    让 x = 向量::新建();\n}");
        assert!(
            warnings.iter().any(|w| w.kind == LintKind::UntypedLet),
            "{warnings:?}"
        );
    }

    /// 字符串/注释内的 `让` 与分号不参与判定
    #[test]
    fn test_strings_and_comments_ignored() {
        let source = "函数 主函数() {\n    // 让 y = 1; 注释里的代码\n    让 s = \"让 z = 2;\";\n}";
        let warnings = lint_teaching(source);
        assert!(
            !warnings
                .iter()
                .any(|w| w.kind == LintKind::UntypedLet && w.line == 2),
            "{warnings:?}"
        );
    }

    #[test]
    fn test_magic_number_reported() {
        let warnings = lint_teaching("函数 主函数() {\n    让 数量: 整数 = 42;\n}");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].kind, LintKind::MagicNumber);
        assert_eq!(warnings[0].text, "42");
        assert_eq!(warnings[0].line, 2);
    }

    /// 「教学忽略」标记行：三种规则均不报告（教师标注故意不修的示例）
    #[test]
    fn test_ignore_mark_skips_line() {
        let source = concat!(
            "函数 主函数() {\n",
            "    让 x = 1;  // 教学忽略\n",
            "    让 数量: 整数 = 42;  // 教学忽略：这行也跳过\n",
            "    让 y = 2;\n",
            "}"
        );
        let warnings = lint_teaching(source);
        assert!(
            !warnings.iter().any(|w| w.line == 2 || w.line == 3),
            "标记行不应有警告: {warnings:?}"
        );
        // 未标记行照常报告
        assert!(warnings.iter().any(|w| w.line == 4), "{warnings:?}");
    }

    /// 字符串/注释里的「教学忽略」文本不生效（仅在代码行注释中识别）
    #[test]
    fn test_ignore_mark_in_string_not_effective() {
        let source = "函数 主函数() {\n    让 s = \"教学忽略\";\n}";
        let warnings = lint_teaching(source);
        assert!(warnings.iter().any(|w| w.line == 2), "{warnings:?}");
    }

    #[test]
    fn test_trivial_numbers_ok() {
        // 0/1/2、负的 1/2、标识符内数字、字符串内数字均不提示
        let source = "函数 主函数() {\n    让 a = 0;\n    让 b = 1;\n    让 c = -2;\n    让 d = 数量2;\n    让 e = \"3\";\n}";
        let warnings = lint_teaching(source);
        assert!(
            !warnings.iter().any(|w| w.kind == LintKind::MagicNumber),
            "{warnings:?}"
        );
    }

    #[test]
    fn test_hex_and_underscore_literals() {
        let warnings = lint_teaching("函数 主函数() {\n    让 a = 0xFF;\n    让 b = 1_000;\n}");
        let magic: Vec<_> = warnings
            .iter()
            .filter(|w| w.kind == LintKind::MagicNumber)
            .map(|w| w.text.clone())
            .collect();
        assert_eq!(magic, vec!["1_000".to_string()], "{warnings:?}");
    }

    #[test]
    fn test_float_literal_reported() {
        let warnings = lint_teaching("函数 主函数() {\n    让 比例: 浮点数 = 0.5;\n}");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(warnings[0].kind, LintKind::MagicNumber);
        assert_eq!(warnings[0].text, "0.5");
    }

    #[test]
    fn test_deep_indent_reported_once() {
        // 两处深层嵌套：仅首个报告（避免刷屏）；缩进逐层递增
        let mut source = String::from("函数 主函数() {\n");
        for level in 1..=7 {
            source.push_str(&"    ".repeat(level));
            source.push_str("if 真 {\n");
        }
        source.push_str(&"    ".repeat(8));
        source.push_str("让 x = 1;\n");
        for level in (1..=7).rev() {
            source.push_str(&"    ".repeat(level));
            source.push_str("}\n");
        }
        source.push_str("    if 真 {\n        如果 真 {\n            循环 {\n");
        source.push_str(&"    ".repeat(5));
        source.push_str("让 y = 1;\n");
        source.push_str("            }\n        }\n    }\n}\n");
        let warnings = lint_teaching(&source);
        let deep: Vec<_> = warnings
            .iter()
            .filter(|w| w.kind == LintKind::DeepIndent)
            .collect();
        assert_eq!(deep.len(), 1, "{warnings:?}");
        // 第 7 层 if（第 7 行）缩进已达 24 空格阈值，是首个触发点
        assert_eq!(deep[0].line, 7);
    }

    #[test]
    fn test_comment_lines_not_deep() {
        let source = "函数 主函数() {\n                        // 长缩进的注释行\n    让 x = 1;\n}";
        let warnings = lint_teaching(source);
        assert!(
            !warnings.iter().any(|w| w.kind == LintKind::DeepIndent),
            "{warnings:?}"
        );
    }
}
