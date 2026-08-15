// Unicode 混淆检测模块
// 在词法处理前扫描源码，检测零宽字符、双向文本控制符与同形异义字符，
// 防范通过不可见或相似字符进行的代码伪装（隐藏恶意代码、标识符欺骗等）。

/// 混淆类别枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfusionCategory {
    /// 零宽字符：肉眼不可见的控制字符
    ZeroWidth,
    /// 双向文本控制符：可篡改显示顺序的 Unicode 字符
    BidiControl,
    /// 同形异义字符：形似拉丁字母的西里尔/希腊字母
    Homoglyph,
}

impl ConfusionCategory {
    /// 返回中文显示文字
    pub fn display_text(&self) -> &'static str {
        match self {
            Self::ZeroWidth => "零宽字符",
            Self::BidiControl => "双向文本控制符",
            Self::Homoglyph => "同形异义字符",
        }
    }
}

/// 单个混淆警告：位置（1 起行/列）+ 字符 + 类别 + 说明
#[derive(Debug, Clone, PartialEq)]
pub struct ConfusionWarning {
    /// 行号（从 1 开始）
    pub line: usize,
    /// 列号（从 1 开始）
    pub column: usize,
    /// 可疑字符
    pub character: char,
    /// 混淆类别
    pub category: ConfusionCategory,
    /// 中文说明
    pub detail: String,
}

impl ConfusionWarning {
    /// 格式化为单行中文警告文本
    /// 示例：`第 2 行第 5 列：检测到零宽字符 U+200B（零宽空格。此类字符肉眼不可见...）`
    pub fn format(&self) -> String {
        format!(
            "第 {} 行第 {} 列：检测到{} U+{:04X}（{}）",
            self.line,
            self.column,
            self.category.display_text(),
            self.character as u32,
            self.detail
        )
    }
}

/// 检查源码中的 Unicode 混淆字符
///
/// - 零宽字符：零宽空格、连接符、BOM（文件首部 BOM 除外）等不可见字符
/// - 双向文本控制符：U+202A~U+202E、U+2066~U+2069 等可篡改显示顺序的字符
/// - 同形异义字符：西里尔/希腊字母中形似拉丁字母的字符
///
/// 返回全部警告（不阻断翻译，由调用方决定如何呈现）。
pub fn check_unicode_confusion(source: &str) -> Vec<ConfusionWarning> {
    let mut warnings = Vec::new();
    let mut line = 1usize;
    let mut col = 1usize;

    for (offset, ch) in source.char_indices() {
        if ch == '\n' {
            line += 1;
            col = 1;
            continue;
        }
        // 文件开头的 BOM（U+FEFF）是合法编码标记，不视为混淆
        if ch == '\u{FEFF}' && offset == 0 {
            col += 1;
            continue;
        }
        if let Some(name) = zero_width_name(ch) {
            warnings.push(ConfusionWarning {
                line,
                column: col,
                character: ch,
                category: ConfusionCategory::ZeroWidth,
                detail: format!("{}。此类字符肉眼不可见，可能被用于隐藏代码或绕过检测", name),
            });
        } else if let Some(name) = bidi_control_name(ch) {
            warnings.push(ConfusionWarning {
                line,
                column: col,
                character: ch,
                category: ConfusionCategory::BidiControl,
                detail: format!(
                    "{}。双向控制符可篡改代码的显示顺序（bidi 攻击），可能隐藏恶意代码",
                    name
                ),
            });
        } else if let Some((similar, name)) = homoglyph_char(ch) {
            warnings.push(ConfusionWarning {
                line,
                column: col,
                character: ch,
                category: ConfusionCategory::Homoglyph,
                detail: format!(
                    "形似拉丁字母 '{}'（{}）。可能被用于同形异义攻击，伪装标识符",
                    similar, name
                ),
            });
        }
        col += 1;
    }
    warnings
}

/// 零宽字符名称查找
fn zero_width_name(ch: char) -> Option<&'static str> {
    match ch {
        '\u{200B}' => Some("零宽空格"),
        '\u{200C}' => Some("零宽非连接符"),
        '\u{200D}' => Some("零宽连接符"),
        '\u{FEFF}' => Some("零宽不换行空格"),
        '\u{2060}' => Some("单词连接符"),
        '\u{00AD}' => Some("软连字符"),
        '\u{180E}' => Some("蒙古语元音分隔符"),
        _ => None,
    }
}

/// 双向文本控制符名称查找
fn bidi_control_name(ch: char) -> Option<&'static str> {
    match ch {
        '\u{200E}' => Some("从左到右标记"),
        '\u{200F}' => Some("从右到左标记"),
        '\u{202A}' => Some("从左到右嵌入"),
        '\u{202B}' => Some("从右到左嵌入"),
        '\u{202C}' => Some("弹出方向格式化"),
        '\u{202D}' => Some("从左到右覆盖"),
        '\u{202E}' => Some("从右到左覆盖"),
        '\u{2066}' => Some("从左到右隔离"),
        '\u{2067}' => Some("从右到左隔离"),
        '\u{2068}' => Some("首项强隔离"),
        '\u{2069}' => Some("弹出方向隔离"),
        _ => None,
    }
}

/// 同形异义字符表：(可疑字符, 形似的拉丁字母, Unicode 名称)
const HOMOGLYPH_TABLE: &[(char, char, &str)] = &[
    // 西里尔字母（形似拉丁）
    ('\u{0410}', 'A', "西里尔大写字母 А"),
    ('\u{0412}', 'B', "西里尔大写字母 В"),
    ('\u{0415}', 'E', "西里尔大写字母 Е"),
    ('\u{041A}', 'K', "西里尔大写字母 К"),
    ('\u{041D}', 'H', "西里尔大写字母 Н"),
    ('\u{041E}', 'O', "西里尔大写字母 О"),
    ('\u{0420}', 'P', "西里尔大写字母 Р"),
    ('\u{0421}', 'C', "西里尔大写字母 С"),
    ('\u{0422}', 'T', "西里尔大写字母 Т"),
    ('\u{0423}', 'Y', "西里尔大写字母 У"),
    ('\u{0425}', 'X', "西里尔大写字母 Х"),
    ('\u{0430}', 'a', "西里尔小写字母 а"),
    ('\u{0435}', 'e', "西里尔小写字母 е"),
    ('\u{043E}', 'o', "西里尔小写字母 о"),
    ('\u{0440}', 'p', "西里尔小写字母 р"),
    ('\u{0441}', 'c', "西里尔小写字母 с"),
    ('\u{0445}', 'x', "西里尔小写字母 х"),
    // 希腊字母（形似拉丁）
    ('\u{0391}', 'A', "希腊大写字母 Α"),
    ('\u{0392}', 'B', "希腊大写字母 Β"),
    ('\u{0395}', 'E', "希腊大写字母 Ε"),
    ('\u{0397}', 'H', "希腊大写字母 Η"),
    ('\u{0399}', 'I', "希腊大写字母 Ι"),
    ('\u{039A}', 'K', "希腊大写字母 Κ"),
    ('\u{039C}', 'M', "希腊大写字母 Μ"),
    ('\u{039D}', 'N', "希腊大写字母 Ν"),
    ('\u{039F}', 'O', "希腊大写字母 Ο"),
    ('\u{03A1}', 'P', "希腊大写字母 Ρ"),
    ('\u{03A4}', 'T', "希腊大写字母 Τ"),
    ('\u{03A5}', 'Y', "希腊大写字母 Υ"),
    ('\u{03A7}', 'X', "希腊大写字母 Χ"),
    ('\u{03B1}', 'a', "希腊小写字母 α"),
    ('\u{03B5}', 'e', "希腊小写字母 ε"),
    ('\u{03B9}', 'i', "希腊小写字母 ι"),
    ('\u{03BA}', 'k', "希腊小写字母 κ"),
    ('\u{03BC}', 'u', "希腊小写字母 μ"),
    ('\u{03BD}', 'v', "希腊小写字母 ν"),
    ('\u{03BF}', 'o', "希腊小写字母 ο"),
    ('\u{03C1}', 'p', "希腊小写字母 ρ"),
    ('\u{03C4}', 't', "希腊小写字母 τ"),
    ('\u{03C5}', 'u', "希腊小写字母 υ"),
    ('\u{03C7}', 'x', "希腊小写字母 χ"),
    ('\u{03C9}', 'w', "希腊小写字母 ω"),
];

/// 查询字符是否为同形异义字符，返回 (形似的拉丁字母, Unicode 名称)
fn homoglyph_char(ch: char) -> Option<(char, &'static str)> {
    HOMOGLYPH_TABLE
        .iter()
        .find(|(suspicious, _, _)| *suspicious == ch)
        .map(|(_, similar, name)| (*similar, *name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_chinese_source_no_warnings() {
        let source = "// 注释\n函数 主函数() {\n    让 x = 5;\n    打印行(\"你好\");\n}";
        assert!(check_unicode_confusion(source).is_empty());
    }

    #[test]
    fn test_zero_width_space_detection_and_position() {
        let source = "函数 主函数() {\n\u{200B}让 x = 1;\n}";
        let warnings = check_unicode_confusion(source);
        assert_eq!(warnings.len(), 1);
        let w = &warnings[0];
        assert_eq!(w.character, '\u{200B}');
        assert_eq!(w.category, ConfusionCategory::ZeroWidth);
        assert_eq!(w.line, 2);
        assert_eq!(w.column, 1);
        assert!(w.detail.contains("零宽空格"));
    }

    #[test]
    fn test_zero_width_joiner_and_separator() {
        let source = "让 a\u{200D}b = 1; 让 c\u{200C}d = 2; 让 e\u{2060}f = 3;";
        let warnings = check_unicode_confusion(source);
        assert_eq!(warnings.len(), 3);
        assert!(warnings.iter().all(|w| w.category == ConfusionCategory::ZeroWidth));
    }

    #[test]
    fn test_bom_at_start_no_warning() {
        let source = "\u{FEFF}函数 主函数() {}";
        assert!(check_unicode_confusion(source).is_empty());
        // BOM in the middle is suspicious
        let source = "函数 主函数(\u{FEFF}) {}";
        let warnings = check_unicode_confusion(source);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].character, '\u{FEFF}');
    }

    #[test]
    fn test_bidi_text_controls() {
        let source = "// 注释 \u{202E} 反转显示\n让 x = \u{202A}1\u{202C};";
        let warnings = check_unicode_confusion(source);
        assert_eq!(warnings.len(), 3);
        assert!(warnings.iter().all(|w| w.category == ConfusionCategory::BidiControl));
        assert_eq!(warnings[0].line, 1);
        assert_eq!(warnings[0].column, 7);
        assert!(warnings[0].detail.contains("从右到左覆盖"));
    }

    #[test]
    fn test_bidi_isolates() {
        let source = "让 x = \u{2066}1\u{2069};";
        let warnings = check_unicode_confusion(source);
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].detail.contains("从左到右隔离"));
        assert!(warnings[1].detail.contains("弹出方向隔离"));
    }

    #[test]
    fn test_cyrillic_homoglyph() {
        let source = "让 а = 1;";
        let warnings = check_unicode_confusion(source);
        assert_eq!(warnings.len(), 1);
        let w = &warnings[0];
        assert_eq!(w.category, ConfusionCategory::Homoglyph);
        assert_eq!(w.character, '\u{0430}');
        assert!(w.detail.contains("形似拉丁字母 'a'"));
        assert!(w.detail.contains("西里尔小写字母"));
        assert_eq!(w.line, 1);
        assert_eq!(w.column, 3);
    }

    #[test]
    fn test_greek_homoglyph() {
        let source = "函数 主函数() { 让 ρ = 1; }";
        let warnings = check_unicode_confusion(source);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].character, '\u{03C1}');
        assert!(warnings[0].detail.contains("形似拉丁字母 'p'"));
    }

    #[test]
    fn test_multiline_position_counting() {
        let source = "函数 主函数() {\n    让 x = 1;\n    а = 2;\n}";
        let warnings = check_unicode_confusion(source);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 3);
        assert_eq!(warnings[0].column, 5);
    }

    #[test]
    fn test_warning_format_output() {
        let warning = ConfusionWarning {
            line: 2,
            column: 5,
            character: '\u{200B}',
            category: ConfusionCategory::ZeroWidth,
            detail: "零宽空格。此类字符肉眼不可见，可能被用于隐藏代码或绕过检测".to_string(),
        };
        let text = warning.format();
        assert!(text.contains("第 2 行第 5 列"));
        assert!(text.contains("零宽字符"));
        assert!(text.contains("U+200B"));
    }
}
