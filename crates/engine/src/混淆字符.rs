//! Unicode 混淆检测模块【zh 自举·批次1】
//! 本文件是源真相：由引导 rzc 转译为 混淆字符.rs 后交 cargo 编译。
//! 再生成方式见 tools/zh-selfhost/regen.sh；请勿手改同目录下的 混淆字符.rs。
//!
//! 在词法处理前扫描源码，检测零宽字符、双向文本控制符与同形异义字符，
//! 防范通过不可见或相似字符进行的代码伪装（隐藏恶意代码、标识符欺骗等）。
//! 语言感知：当前方言合法使用某文字系统时（如 ru 方言的西里尔标识符），
//! 不报告该文字的同形异义告警，避免对合法代码的误报。
//!
//! 注：`"unicode_*"` 系列字符串是与各语言包 ui.toml 对齐的协议键（数据，
//! 非代码），按约定保持英文，不得本地化；`无符号机器整数`、`模式匹配!`、生命周期名
//! `'static` 及宿主侧 `语言` 模块 API 名暂无中文词条，按英文透传（合法方言行为）。

/// 混淆类别枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 混淆类别 {
    /// 零宽字符：肉眼不可见的控制字符
    零宽,
    /// 双向文本控制符：可篡改显示顺序的 Unicode 字符
    双向,
    /// 同形异义字符：形似拉丁字母的西里尔/希腊字母
    同形,
}

impl 混淆类别 {
    /// 返回当前语言下的类别显示文字
    pub fn 显示文字(&self) -> String {
        let 键 = match self {
            Self::零宽 => "unicode_cat_zero_width",
            Self::双向 => "unicode_cat_bidi",
            Self::同形 => "unicode_cat_homoglyph",
        };
        crate::语言::查句(键)
    }
}

/// 单个混淆警告：位置（1 起行/列）+ 字符 + 类别 + 说明
#[derive(Debug, Clone, PartialEq)]
pub struct 混淆警告 {
    /// 行号（从 1 开始）
    pub 行号: usize,
    /// 列号（从 1 开始）
    pub 列号: usize,
    /// 可疑字符
    pub 可疑字符: char,
    /// 混淆类别
    pub 类别: 混淆类别,
    /// 中文说明
    pub 说明: String,
}

impl 混淆警告 {
    /// 格式化为单行警告文本，模板随当前语言变化
    /// 示例（zh）：`第 2 行第 5 列：检测到零宽字符 U+200B（零宽空格。此类字符肉眼不可见...）`
    pub fn 格式化输出(&self) -> String {
        // 格式占位符 {:04X} 先格式化再传入模板
        let 码位 = format!("{:04X}", self.可疑字符 as u32);
        crate::语言::查译(
            "unicode_warning_at",
            &[
                &self.行号.to_string(),
                &self.列号.to_string(),
                &self.类别.显示文字(),
                &码位,
                &self.说明,
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
pub fn 检查混淆字符(源码: &str) -> Vec<混淆警告> {
    let mut 警告列表 = Vec::new();
    let mut line = 1usize;
    let mut column = 1usize;

    for (偏移, 字) in 源码.char_indices() {
        if 字 == '\n' {
            line += 1;
            column = 1;
            continue;
        }
        // 文件开头的 BOM（U+FEFF）是合法编码标记，不视为混淆
        if 字 == '\u{FEFF}' && 偏移 == 0 {
            column += 1;
            continue;
        }
        if let Some(名称) = 零宽名(字) {
            警告列表.push(混淆警告 {
                行号: line,
                列号: column,
                可疑字符: 字,
                类别: 混淆类别::零宽,
                说明: crate::语言::查译(
                    "unicode_zero_width_hint",
                    &[&本地化字符名(字, 名称)],
                ),
            });
        } else if let Some(名称) = 双向名(字) {
            警告列表.push(混淆警告 {
                行号: line,
                列号: column,
                可疑字符: 字,
                类别: 混淆类别::双向,
                说明: crate::语言::查译("unicode_bidi_hint", &[&本地化字符名(字, 名称)]),
            });
        } else if let Some((形似拉丁, 名称)) = 同形字查询(字) {
            // 当前方言合法使用该文字时（如 ru 方言的西里尔字母）不构成混淆，跳过
            if !本域文字(字) {
                警告列表.push(混淆警告 {
                    行号: line,
                    列号: column,
                    可疑字符: 字,
                    类别: 混淆类别::同形,
                    说明: crate::语言::查译(
                        "unicode_homoglyph_hint",
                        &[&形似拉丁.to_string(), &本地化字符名(字, 名称)],
                    ),
                });
            }
        }
        column += 1;
    }
    警告列表
}

/// 零宽字符名称查找（英文 Unicode 官方名）
fn 零宽名(字: char) -> Option<&'static str> {
    match 字 {
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
fn 双向名(字: char) -> Option<&'static str> {
    match 字 {
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
fn 本地化字符名(字: char, 英文名: &str) -> String {
    if crate::语言::当前语言() == "zh" {
        // 键名为 4 位大写十六进制（如 unicode_name_200B），与语言包一致
        crate::语言::查句(&format!("unicode_name_{:04X}", 字 as u32))
    } else {
        英文名.to_string()
    }
}

/// 同形异义字符表：(可疑字符, 形似的拉丁字母, Unicode 官方名)
const 同形字表: &[(char, char, &str)] = &[
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
fn 同形字查询(字: char) -> Option<(char, &'static str)> {
    同形字表
        .iter()
        .find(|(可疑, _, _)| *可疑 == 字)
        .map(|&(_, 形似, 名称)| (形似, 名称))
}

/// 字符是否属于当前方言合法使用的文字系统
///
/// 使用西里尔字母的方言（ru/uk/bg/sr 等）中，形似拉丁字母的西里尔字符
/// （а/е/о/р/с/у/х 等）是合法字符而非伪装；
/// 覆盖完整西里尔字母 Unicode 块（U+0400–U+04FF 基本块 +
/// U+0500–U+052F 扩展块），确保乌克兰语（є/і/ї/ґ）、
/// 白俄罗斯语等扩展字符不被误报为可疑伪装。
/// 希腊字母等其他同形字符与零宽/双向控制符不受豁免，仍然告警。
fn 本域文字(字: char) -> bool {
    let 码位 = 字 as u32;
    // 任何使用西里尔字母的方言包：整个西里尔字母块均为合法字符
    if crate::语言::使用西里尔文字(&crate::语言::当前语言()) {
        return matches!(码位, 0x0400..=0x052F);
    }
    false
}

#[cfg(test)]
mod 单元测试 {
    use super::*;

    /// 语言测试守卫：复用 语言::测试语言（持全局测试锁串行化 +
    /// RAII 恢复语言 + 抗毒化；unicode 字符名仅 zh 语言包提供中文映射）
    fn 中文守卫() -> crate::语言::语言测试守卫 {
        crate::语言::测试语言("zh")
    }

    #[test]
    fn 正常中文源码无警告() {
        let 源码 = "// 注释\n函数 主函数() {\n    让 x = 5;\n    打印行(\"你好\");\n}";
        assert!(检查混淆字符(源码).is_empty());
    }

    #[test]
    fn 零宽空格检测与定位() {
        let _守卫 = 中文守卫();
        let 源码 = "函数 主函数() {\n\u{200B}让 x = 1;\n}";
        let 警告列表 = 检查混淆字符(源码);
        assert_eq!(警告列表.len(), 1);
        let 警告项 = &警告列表[0];
        assert_eq!(警告项.可疑字符, '\u{200B}');
        assert_eq!(警告项.类别, 混淆类别::零宽);
        assert_eq!(警告项.行号, 2);
        assert_eq!(警告项.列号, 1);
        assert!(警告项.说明.contains("零宽空格"));
    }

    #[test]
    fn 零宽连接符与分隔符() {
        let 源码 = "让 a\u{200D}b = 1; 让 c\u{200C}d = 2; 让 e\u{2060}f = 3;";
        let 警告列表 = 检查混淆字符(源码);
        assert_eq!(警告列表.len(), 3);
        assert!(警告列表.iter().all(|警告项| 警告项.类别 == 混淆类别::零宽));
    }

    #[test]
    fn 文件头不告警() {
        let 源码 = "\u{FEFF}函数 主函数() {}";
        assert!(检查混淆字符(源码).is_empty());
        // BOM 在 the middle is suspicious
        let 源码 = "函数 主函数(\u{FEFF}) {}";
        let 警告列表 = 检查混淆字符(源码);
        assert_eq!(警告列表.len(), 1);
        assert_eq!(警告列表[0].可疑字符, '\u{FEFF}');
    }

    #[test]
    fn 双向文本控制符() {
        let _守卫 = 中文守卫();
        let 源码 = "// 注释 \u{202E} 反转显示\n让 x = \u{202A}1\u{202C};";
        let 警告列表 = 检查混淆字符(源码);
        assert_eq!(警告列表.len(), 3);
        assert!(警告列表.iter().all(|警告项| 警告项.类别 == 混淆类别::双向));
        assert_eq!(警告列表[0].行号, 1);
        assert_eq!(警告列表[0].列号, 7);
        assert!(警告列表[0].说明.contains("从右到左覆盖"));
    }

    #[test]
    fn 双向隔离符() {
        let _守卫 = 中文守卫();
        let 源码 = "让 x = \u{2066}1\u{2069};";
        let 警告列表 = 检查混淆字符(源码);
        assert_eq!(警告列表.len(), 2);
        assert!(警告列表[0].说明.contains("从左到右隔离"));
        assert!(警告列表[1].说明.contains("弹出方向隔离"));
    }

    #[test]
    fn 西里尔同形字符() {
        let _守卫 = 中文守卫();
        let 源码 = "让 а = 1;";
        let 警告列表 = 检查混淆字符(源码);
        assert_eq!(警告列表.len(), 1);
        let 警告项 = &警告列表[0];
        assert_eq!(警告项.类别, 混淆类别::同形);
        assert_eq!(警告项.可疑字符, '\u{0430}');
        assert!(警告项.说明.contains("形似拉丁字母 'a'"));
        assert!(警告项.说明.contains("西里尔小写字母"));
        assert_eq!(警告项.行号, 1);
        assert_eq!(警告项.列号, 3);
    }

    #[test]
    fn 希腊同形字符() {
        let _守卫 = 中文守卫();
        let 源码 = "函数 主函数() { 让 ρ = 1; }";
        let 警告列表 = 检查混淆字符(源码);
        assert_eq!(警告列表.len(), 1);
        assert_eq!(警告列表[0].可疑字符, '\u{03C1}');
        assert!(警告列表[0].说明.contains("形似拉丁字母 'p'"));
    }

    #[test]
    fn 多行位置计数() {
        // 西里尔字符告警依赖当前语言为 zh（ru 下方豁免），需持锁串行
        let _守卫 = 中文守卫();
        let 源码 = "函数 主函数() {\n    让 x = 1;\n    а = 2;\n}";
        let 警告列表 = 检查混淆字符(源码);
        assert_eq!(警告列表.len(), 1);
        assert_eq!(警告列表[0].行号, 3);
        assert_eq!(警告列表[0].列号, 5);
    }

    #[test]
    fn 俄语方言豁免西里尔同形字符() {
        // RAII 守卫：离开作用域（含断言 崩溃）自动恢复 zh，无需手工还原
        let _守卫 = crate::语言::测试语言("ru");
        // 西里尔字符在 ru 方言中是合法标识符字符，不再报同形异义告警
        let 警告列表 = 检查混淆字符("пусть а = 1;");
        assert!(警告列表.is_empty(), "ru 方言不应误报西里尔字符");
        // 希腊字母与零宽字符在 ru 方言下仍然告警
        let 警告列表 = 检查混淆字符("пусть ρ\u{200B} = 1;");
        assert_eq!(警告列表.len(), 2);
        assert!(警告列表.iter().any(|警告项| 警告项.类别 == 混淆类别::同形));
        assert!(警告列表.iter().any(|警告项| 警告项.类别 == 混淆类别::零宽));
    }

    #[test]
    fn 警告格式化输出() {
        let _守卫 = 中文守卫();
        let 警告 = 混淆警告 {
            行号: 2,
            列号: 5,
            可疑字符: '\u{200B}',
            类别: 混淆类别::零宽,
            说明: "零宽空格。此类字符肉眼不可见，可能被用于隐藏代码或绕过检测".to_string(),
        };
        let str = 警告.格式化输出();
        assert!(str.contains("第 2 行第 5 列"));
        assert!(str.contains("零宽字符"));
        assert!(str.contains("U+200B"));
    }
}
