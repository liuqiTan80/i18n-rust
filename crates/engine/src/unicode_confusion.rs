// Unicode 混淆检测模块
// 在词法处理前扫描源码，检测零宽字符、双向文本控制符与同形异义字符，
// 防范通过不可见或相似字符进行的代码伪装（隐藏恶意代码、标识符欺骗等）。
// 语言感知：当前方言合法使用某文字系统时（如 ru 方言的西里尔标识符），
// 不报告该文字的同形异义告警，避免对合法代码的误报。

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
    /// 返回当前语言下的类别显示文字
    pub fn display_text(&self) -> String {
        let key = match self {
            Self::ZeroWidth => "unicode_cat_zero_width",
            Self::BidiControl => "unicode_cat_bidi",
            Self::Homoglyph => "unicode_cat_homoglyph",
        };
        crate::语言::t(key)
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
    /// 格式化为单行警告文本，模板随当前语言变化
    /// 示例（zh）：`第 2 行第 5 列：检测到零宽字符 U+200B（零宽空格。此类字符肉眼不可见...）`
    pub fn format(&self) -> String {
        // 格式占位符 {:04X} 先格式化再传入模板
        let codepoint = format!("{:04X}", self.character as u32);
        crate::语言::f(
            "unicode_warning_at",
            &[
                &self.line.to_string(),
                &self.column.to_string(),
                &self.category.display_text(),
                &codepoint,
                &self.detail,
            ],
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
                detail: crate::语言::f(
                    "unicode_zero_width_hint",
                    &[&localized_char_name(ch, name)],
                ),
            });
        } else if let Some(name) = bidi_control_name(ch) {
            warnings.push(ConfusionWarning {
                line,
                column: col,
                character: ch,
                category: ConfusionCategory::BidiControl,
                detail: crate::语言::f("unicode_bidi_hint", &[&localized_char_name(ch, name)]),
            });
        } else if let Some((similar, name)) = homoglyph_char(ch) {
            // 当前方言合法使用该文字时（如 ru 方言的西里尔字母）不构成混淆，跳过
            if !is_native_script_char(ch) {
                warnings.push(ConfusionWarning {
                    line,
                    column: col,
                    character: ch,
                    category: ConfusionCategory::Homoglyph,
                    detail: crate::语言::f(
                        "unicode_homoglyph_hint",
                        &[&similar.to_string(), &localized_char_name(ch, name)],
                    ),
                });
            }
        }
        col += 1;
    }
    warnings
}

/// 零宽字符名称查找（英文 Unicode 官方名）
fn zero_width_name(ch: char) -> Option<&'static str> {
    match ch {
        '\u{200B}' => Some("ZERO WIDTH SPACE"),
        '\u{200C}' => Some("ZERO WIDTH NON-JOINER"),
        '\u{200D}' => Some("ZERO WIDTH JOINER"),
        '\u{FEFF}' => Some("ZERO WIDTH NO-BREAK SPACE"),
        '\u{2060}' => Some("WORD JOINER"),
        '\u{00AD}' => Some("SOFT HYPHEN"),
        '\u{180E}' => Some("MONGOLIAN VOWEL SEPARATOR"),
        _ => None,
    }
}

/// 双向文本控制符名称查找（英文 Unicode 官方名）
fn bidi_control_name(ch: char) -> Option<&'static str> {
    match ch {
        '\u{200E}' => Some("LEFT-TO-RIGHT MARK"),
        '\u{200F}' => Some("RIGHT-TO-LEFT MARK"),
        '\u{202A}' => Some("LEFT-TO-RIGHT EMBEDDING"),
        '\u{202B}' => Some("RIGHT-TO-LEFT EMBEDDING"),
        '\u{202C}' => Some("POP DIRECTIONAL FORMATTING"),
        '\u{202D}' => Some("LEFT-TO-RIGHT OVERRIDE"),
        '\u{202E}' => Some("RIGHT-TO-LEFT OVERRIDE"),
        '\u{2066}' => Some("LEFT-TO-RIGHT ISOLATE"),
        '\u{2067}' => Some("RIGHT-TO-LEFT ISOLATE"),
        '\u{2068}' => Some("FIRST STRONG ISOLATE"),
        '\u{2069}' => Some("POP DIRECTIONAL ISOLATE"),
        _ => None,
    }
}

/// 获取当前语言下的字符名：zh 语言包提供中文名（unicode_name_{codepoint:X}），
/// 其他语言直接用英文 Unicode 官方名（语言包不重复翻译字符名）。
fn localized_char_name(ch: char, english_name: &str) -> String {
    if crate::语言::current_language() == "zh" {
        // 键名为 4 位大写十六进制（如 unicode_name_200B），与语言包一致
        crate::语言::t(&format!("unicode_name_{:04X}", ch as u32))
    } else {
        english_name.to_string()
    }
}

/// 同形异义字符表：(可疑字符, 形似的拉丁字母, Unicode 官方名)
const HOMOGLYPH_TABLE: &[(char, char, &str)] = &[
    // 西里尔字母（形似拉丁）
    ('\u{0410}', 'A', "CYRILLIC CAPITAL LETTER A"),
    ('\u{0412}', 'B', "CYRILLIC CAPITAL LETTER VE"),
    ('\u{0415}', 'E', "CYRILLIC CAPITAL LETTER IE"),
    ('\u{041A}', 'K', "CYRILLIC CAPITAL LETTER KA"),
    ('\u{041D}', 'H', "CYRILLIC CAPITAL LETTER EN"),
    ('\u{041E}', 'O', "CYRILLIC CAPITAL LETTER O"),
    ('\u{0420}', 'P', "CYRILLIC CAPITAL LETTER ER"),
    ('\u{0421}', 'C', "CYRILLIC CAPITAL LETTER ES"),
    ('\u{0422}', 'T', "CYRILLIC CAPITAL LETTER TE"),
    ('\u{0423}', 'Y', "CYRILLIC CAPITAL LETTER U"),
    ('\u{0425}', 'X', "CYRILLIC CAPITAL LETTER HA"),
    ('\u{0430}', 'a', "CYRILLIC SMALL LETTER A"),
    ('\u{0435}', 'e', "CYRILLIC SMALL LETTER IE"),
    ('\u{043E}', 'o', "CYRILLIC SMALL LETTER O"),
    ('\u{0440}', 'p', "CYRILLIC SMALL LETTER ER"),
    ('\u{0441}', 'c', "CYRILLIC SMALL LETTER ES"),
    ('\u{0445}', 'x', "CYRILLIC SMALL LETTER HA"),
    // 希腊字母（形似拉丁）
    ('\u{0391}', 'A', "GREEK CAPITAL LETTER ALPHA"),
    ('\u{0392}', 'B', "GREEK CAPITAL LETTER BETA"),
    ('\u{0395}', 'E', "GREEK CAPITAL LETTER EPSILON"),
    ('\u{0397}', 'H', "GREEK CAPITAL LETTER ETA"),
    ('\u{0399}', 'I', "GREEK CAPITAL LETTER IOTA"),
    ('\u{039A}', 'K', "GREEK CAPITAL LETTER KAPPA"),
    ('\u{039C}', 'M', "GREEK CAPITAL LETTER MU"),
    ('\u{039D}', 'N', "GREEK CAPITAL LETTER NU"),
    ('\u{039F}', 'O', "GREEK CAPITAL LETTER OMICRON"),
    ('\u{03A1}', 'P', "GREEK CAPITAL LETTER RHO"),
    ('\u{03A4}', 'T', "GREEK CAPITAL LETTER TAU"),
    ('\u{03A5}', 'Y', "GREEK CAPITAL LETTER UPSILON"),
    ('\u{03A7}', 'X', "GREEK CAPITAL LETTER CHI"),
    ('\u{03B1}', 'a', "GREEK SMALL LETTER ALPHA"),
    ('\u{03B5}', 'e', "GREEK SMALL LETTER EPSILON"),
    ('\u{03B9}', 'i', "GREEK SMALL LETTER IOTA"),
    ('\u{03BA}', 'k', "GREEK SMALL LETTER KAPPA"),
    ('\u{03BC}', 'u', "GREEK SMALL LETTER MU"),
    ('\u{03BD}', 'v', "GREEK SMALL LETTER NU"),
    ('\u{03BF}', 'o', "GREEK SMALL LETTER OMICRON"),
    ('\u{03C1}', 'p', "GREEK SMALL LETTER RHO"),
    ('\u{03C4}', 't', "GREEK SMALL LETTER TAU"),
    ('\u{03C5}', 'u', "GREEK SMALL LETTER UPSILON"),
    ('\u{03C7}', 'x', "GREEK SMALL LETTER CHI"),
    ('\u{03C9}', 'w', "GREEK SMALL LETTER OMEGA"),
];

/// 查询字符是否为同形异义字符，返回 (形似的拉丁字母, Unicode 名称)
fn homoglyph_char(ch: char) -> Option<(char, &'static str)> {
    HOMOGLYPH_TABLE
        .iter()
        .find(|(suspicious, _, _)| *suspicious == ch)
        .map(|(_, similar, name)| (*similar, *name))
}

/// 字符是否属于当前方言合法使用的文字系统
///
/// ru 方言用西里尔字母书写标识符，形似拉丁字母的西里尔字符是合法字符而非伪装；
/// 希腊字母等其他同形字符与零宽/双向控制符不受豁免，仍然告警。
fn is_native_script_char(ch: char) -> bool {
    let code = ch as u32;
    matches!(
        (crate::语言::current_language().as_str(), code),
        ("ru", 0x0400..=0x04FF) // 西里尔字母块
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 语言测试守卫：复用 语言::test_language（持全局测试锁串行化 +
    /// RAII 恢复语言 + 抗毒化；unicode 字符名仅 zh 语言包提供中文映射）
    fn zh_guard() -> crate::语言::LangTestGuard {
        crate::语言::test_language("zh")
    }

    #[test]
    fn test_normal_chinese_source_no_warnings() {
        let source = "// 注释\n函数 主函数() {\n    让 x = 5;\n    打印行(\"你好\");\n}";
        assert!(check_unicode_confusion(source).is_empty());
    }

    #[test]
    fn test_zero_width_space_detection_and_position() {
        let _guard = zh_guard();
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
        assert!(
            warnings
                .iter()
                .all(|w| w.category == ConfusionCategory::ZeroWidth)
        );
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
        let _guard = zh_guard();
        let source = "// 注释 \u{202E} 反转显示\n让 x = \u{202A}1\u{202C};";
        let warnings = check_unicode_confusion(source);
        assert_eq!(warnings.len(), 3);
        assert!(
            warnings
                .iter()
                .all(|w| w.category == ConfusionCategory::BidiControl)
        );
        assert_eq!(warnings[0].line, 1);
        assert_eq!(warnings[0].column, 7);
        assert!(warnings[0].detail.contains("从右到左覆盖"));
    }

    #[test]
    fn test_bidi_isolates() {
        let _guard = zh_guard();
        let source = "让 x = \u{2066}1\u{2069};";
        let warnings = check_unicode_confusion(source);
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].detail.contains("从左到右隔离"));
        assert!(warnings[1].detail.contains("弹出方向隔离"));
    }

    #[test]
    fn test_cyrillic_homoglyph() {
        let _guard = zh_guard();
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
        let _guard = zh_guard();
        let source = "函数 主函数() { 让 ρ = 1; }";
        let warnings = check_unicode_confusion(source);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].character, '\u{03C1}');
        assert!(warnings[0].detail.contains("形似拉丁字母 'p'"));
    }

    #[test]
    fn test_multiline_position_counting() {
        // 西里尔字符告警依赖当前语言为 zh（ru 下方豁免），需持锁串行
        let _guard = zh_guard();
        let source = "函数 主函数() {\n    让 x = 1;\n    а = 2;\n}";
        let warnings = check_unicode_confusion(source);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 3);
        assert_eq!(warnings[0].column, 5);
    }

    #[test]
    fn test_russian_dialect_skips_cyrillic_homoglyph() {
        // RAII 守卫：离开作用域（含断言 panic）自动恢复 zh，无需手工还原
        let _guard = crate::语言::test_language("ru");
        // 西里尔字符在 ru 方言中是合法标识符字符，不再报同形异义告警
        let warnings = check_unicode_confusion("пусть а = 1;");
        assert!(warnings.is_empty(), "ru 方言不应误报西里尔字符");
        // 希腊字母与零宽字符在 ru 方言下仍然告警
        let warnings = check_unicode_confusion("пусть ρ\u{200B} = 1;");
        assert_eq!(warnings.len(), 2);
        assert!(
            warnings
                .iter()
                .any(|w| w.category == ConfusionCategory::Homoglyph)
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.category == ConfusionCategory::ZeroWidth)
        );
    }

    #[test]
    fn test_warning_format_output() {
        let _guard = zh_guard();
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
