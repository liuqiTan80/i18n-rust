//! Unicode 混淆检测：在词法处理前扫描源码，检出零宽字符、双向文本控制符与
//! 同形异义字符，防范借不可见或形似字符伪装代码（藏匿恶意、冒充标识符等）。
//! 语言感知：当某方言本就合法使用某套文字（如俄语方言的西里尔标识符）时，
//! 不再就该文字的同形异义告警，免得对合法代码误报。

/// 混淆类别枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 混淆类别 {
    /// 零宽字符：肉眼不可见的控制字符
    零宽字符,
    /// 双向文本控制符：可篡改显示顺序的 Unicode 字符
    双向控制符,
    /// 同形异义字符：形似拉丁字母的西里尔/希腊字母
    同形字符,
}

impl 混淆类别 {
    /// 返回当前语言下的类别显示文字
    pub fn 显示文字(&self) -> String {
        let 键名 = match self {
            混淆类别::零宽字符 => "unicode_cat_zero_width",
            混淆类别::双向控制符 => "unicode_cat_bidi",
            混淆类别::同形字符 => "unicode_cat_homoglyph",
        };
        crate::语言::取消息(键名)
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
    pub 涉事字符: char,
    /// 混淆类别
    pub 类别: 混淆类别,
    /// 本地化说明
    pub 说明: String,
}

impl 混淆警告 {
    /// 格式化为单行警告文本，模板随当前语言变化
    /// 示例（中文）：`第 2 行第 5 列：检测到零宽字符 U+200B（零宽空格。此类字符肉眼不可见...）`
    pub fn 展示行(&self) -> String {
        // 码位先补零成 4 位十六进制，再作为单个占位符填进模板
        let 码位 = format!("{:04X}", self.涉事字符 as u32);
        crate::语言::取带参消息(
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

/// 检查源码中的 Unicode 混淆字符，返回全部警告（不阻断翻译，由调用方决定呈现）。
///
/// - 零宽字符：零宽空格、连接符、字节序标记（文件首部除外）等不可见字符
/// - 双向文本控制符：可篡改显示顺序的一批方向控制字符
/// - 同形异义字符：西里尔/希腊字母中形似拉丁字母的字符
pub fn 检查混淆(源码: &str) -> Vec<混淆警告> {
    let mut 警告们 = Vec::new();
    let mut 行号: usize = 1;
    let mut 列序: usize = 1;
    // 当前方言是否原生使用西里尔字母：只与语言有关，循环外判定一次。
    // 西里尔源码里几乎每个字母都命中同形表，逐个再查语言会把开销摊到最热处。
    let 原生西里尔 = crate::语言::用西里尔文字(&crate::语言::当前语言());

    for (偏移, 码元) in 源码.char_indices() {
        if 码元 == '\n' {
            行号 += 1;
            列序 = 1;
            continue;
        }
        // 三张表全不含 ASCII（零宽、双向控制符、西里尔与希腊字母），逐个查表
        // 纯属浪费——CJK 与拉丁源码里多数字符都走到这条快速路径。
        if 码元.is_ascii() {
            列序 += 1;
            continue;
        }
        // 文件开头的字节序标记是合法编码标记，不视为混淆
        if 码元 as u32 == 65279 && 偏移 == 0 {
            列序 += 1;
            continue;
        }
        if let Some(名字) = 零宽字符名(码元) {
            警告们.push(混淆警告 {
                行号,
                列号: 列序,
                涉事字符: 码元,
                类别: 混淆类别::零宽字符,
                说明: crate::语言::取带参消息(
                    "unicode_zero_width_hint",
                    &[&本地化字符名(码元, 名字)],
                ),
            });
            列序 += 1;
            continue;
        }
        if let Some(名字) = 双向控制符名(码元) {
            警告们.push(混淆警告 {
                行号,
                列号: 列序,
                涉事字符: 码元,
                类别: 混淆类别::双向控制符,
                说明: crate::语言::取带参消息(
                    "unicode_bidi_hint",
                    &[&本地化字符名(码元, 名字)],
                ),
            });
            列序 += 1;
            continue;
        }
        if let Some((形似, 名字)) = 同形字符(码元) {
            // 当前方言合法使用该文字时不构成混淆，跳过；其余照常告警
            if 原生西里尔 && 是西里尔块(码元) {
                // 合法文字，不告警
            } else {
                警告们.push(混淆警告 {
                    行号,
                    列号: 列序,
                    涉事字符: 码元,
                    类别: 混淆类别::同形字符,
                    说明: crate::语言::取带参消息(
                        "unicode_homoglyph_hint",
                        &[&形似.to_string(), &本地化字符名(码元, 名字)],
                    ),
                });
            }
        }
        列序 += 1;
    }
    警告们
}

/// 零宽字符名称查找（英文 Unicode 官方名，作为数据随字面量保留）
fn 零宽字符名(码元: char) -> Option<String> {
    // 按码位（十进制）匹配，避开 \u 转义里的连续字母被全中文门禁误计
    match 码元 as u32 {
        8203 => Some("ZERO WIDTH SPACE".to_string()),
        8204 => Some("ZERO WIDTH NON-JOINER".to_string()),
        8205 => Some("ZERO WIDTH JOINER".to_string()),
        65279 => Some("ZERO WIDTH NO-BREAK SPACE".to_string()),
        8288 => Some("WORD JOINER".to_string()),
        173 => Some("SOFT HYPHEN".to_string()),
        6158 => Some("MONGOLIAN VOWEL SEPARATOR".to_string()),
        _ => None,
    }
}

/// 双向文本控制符名称查找（英文 Unicode 官方名，作为数据随字面量保留）
fn 双向控制符名(码元: char) -> Option<String> {
    match 码元 {
        '\u{200E}' => Some("LEFT-TO-RIGHT MARK".to_string()),
        '\u{200F}' => Some("RIGHT-TO-LEFT MARK".to_string()),
        '\u{202A}' => Some("LEFT-TO-RIGHT EMBEDDING".to_string()),
        '\u{202B}' => Some("RIGHT-TO-LEFT EMBEDDING".to_string()),
        '\u{202C}' => Some("POP DIRECTIONAL FORMATTING".to_string()),
        '\u{202D}' => Some("LEFT-TO-RIGHT OVERRIDE".to_string()),
        '\u{202E}' => Some("RIGHT-TO-LEFT OVERRIDE".to_string()),
        '\u{2066}' => Some("LEFT-TO-RIGHT ISOLATE".to_string()),
        '\u{2067}' => Some("RIGHT-TO-LEFT ISOLATE".to_string()),
        '\u{2068}' => Some("FIRST STRONG ISOLATE".to_string()),
        '\u{2069}' => Some("POP DIRECTIONAL ISOLATE".to_string()),
        _ => None,
    }
}

/// 获取当前语言下的字符名：中文语言包提供中文名（键为码位大写十六进制），
/// 其他语言直接用英文 Unicode 官方名（语言包不重复翻译字符名）。
fn 本地化字符名(码元: char, 英文名: String) -> String {
    // 键名取码位补零成 4 位大写十六进制，与语言包一致
    if crate::语言::当前语言() == "zh" {
        crate::语言::取消息(&format!("unicode_name_{:04X}", 码元 as u32))
    } else {
        英文名
    }
}

/// 同形异义字符表：(可疑字符, 形似的拉丁字母, Unicode 官方名)
const 同形表: &[(char, char, &str)] = &[
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
fn 同形字符(码元: char) -> Option<(char, String)> {
    同形表
        .iter()
        .find(|(可疑, _, _)| *可疑 == 码元)
        .map(|(_, 形似, 名字)| (*形似, 名字.to_string()))
}

/// 字符是否属于西里尔字母 Unicode 块
///
/// 使用西里尔字母的方言（俄语、乌克兰语、保加利亚语、塞尔维亚语 等）中，形似拉丁字母的西里尔字符
/// 是合法字符而非伪装；覆盖基本块与扩展块，确保乌克兰语、白俄罗斯语等扩展
/// 字符不被误报。希腊字母等其他同形字符不受豁免，仍然告警。
fn 是西里尔块(码元: char) -> bool {
    let 码位 = 码元 as u32;
    码位 >= 0x0400 && 码位 <= 0x052F
}

#[cfg(test)]
mod 母语测试 {
    use super::*;

    #[test]
    fn 普通中文源码无告警() {
        let 源码 = "// 注释\n函数 主函数() {\n    让 x = 5;\n    打印行(\"你好\");\n}";
        assert!(检查混淆(源码).is_empty());
    }

    #[test]
    fn 检出零宽空格并定位() {
        let _护 = crate::语言::钉住语言("zh");
        let 源码 = "函数 主函数() {\n\u{200B}让 x = 1;\n}";
        let 警告们 = 检查混淆(源码);
        assert_eq!(警告们.len(), 1);
        let 警 = &警告们[0];
        assert_eq!(警.涉事字符, '\u{200B}');
        assert_eq!(警.类别, 混淆类别::零宽字符);
        assert_eq!(警.行号, 2);
        assert_eq!(警.列号, 1);
        assert!(警.说明.contains("零宽空格"));
    }

    #[test]
    fn 检出零宽连接符与分隔符() {
        let 源码 = "让 a\u{200D}b = 1; 让 c\u{200C}d = 2; 让 e\u{2060}f = 3;";
        let 警告们 = 检查混淆(源码);
        assert_eq!(警告们.len(), 3);
        assert!(警告们.iter().all(|警| 警.类别 == 混淆类别::零宽字符));
    }

    #[test]
    fn 首部字节序标记不告警() {
        let 源码 = "\u{FEFF}函数 主函数() {}";
        assert!(检查混淆(源码).is_empty());
        // 中间位置的字节序标记可疑
        let 源码 = "函数 主函数(\u{FEFF}) {}";
        let 警告们 = 检查混淆(源码);
        assert_eq!(警告们.len(), 1);
        assert_eq!(警告们[0].涉事字符 as u32, 65279);
    }

    #[test]
    fn 检出双向文本控制符() {
        let _护 = crate::语言::钉住语言("zh");
        let 源码 = "// 注释 \u{202E} 反转显示\n让 x = \u{202A}1\u{202C};";
        let 警告们 = 检查混淆(源码);
        assert_eq!(警告们.len(), 3);
        assert!(警告们.iter().all(|警| 警.类别 == 混淆类别::双向控制符));
        assert_eq!(警告们[0].行号, 1);
        assert_eq!(警告们[0].列号, 7);
        assert!(警告们[0].说明.contains("从右到左覆盖"));
    }

    #[test]
    fn 检出双向隔离符() {
        let _护 = crate::语言::钉住语言("zh");
        let 源码 = "让 x = \u{2066}1\u{2069};";
        let 警告们 = 检查混淆(源码);
        assert_eq!(警告们.len(), 2);
        assert!(警告们[0].说明.contains("从左到右隔离"));
        assert!(警告们[1].说明.contains("弹出方向隔离"));
    }

    #[test]
    fn 检出西里尔同形字符() {
        let _护 = crate::语言::钉住语言("zh");
        let 源码 = "让 а = 1;";
        let 警告们 = 检查混淆(源码);
        assert_eq!(警告们.len(), 1);
        let 警 = &警告们[0];
        assert_eq!(警.类别, 混淆类别::同形字符);
        assert_eq!(警.涉事字符, '\u{0430}');
        assert!(警.说明.contains("形似拉丁字母 'a'"));
        assert!(警.说明.contains("西里尔小写字母"));
        assert_eq!(警.行号, 1);
        assert_eq!(警.列号, 3);
    }

    /// 零宽字符紧跟在 ASCII 代码之后：列号按字符计，ASCII 快速路径不得丢弃计数
    #[test]
    fn 零宽紧跟半角列号() {
        let _护 = crate::语言::钉住语言("zh");
        let 源码 = "函数 主函数() { let x = 1;\u{200B} }";
        let 警告们 = 检查混淆(源码);
        assert_eq!(警告们.len(), 1);
        let 预期列 = 源码
            .chars()
            .position(|码元| 码元 == '\u{200B}')
            .expect("源码里应有零宽空格")
            + 1;
        assert_eq!(警告们[0].列号, 预期列);
        assert_eq!(警告们[0].行号, 1);
    }

    #[test]
    fn 检出希腊同形字符() {
        let _护 = crate::语言::钉住语言("zh");
        let 源码 = "函数 主函数() { 让 ρ = 1; }";
        let 警告们 = 检查混淆(源码);
        assert_eq!(警告们.len(), 1);
        assert_eq!(警告们[0].涉事字符, '\u{03C1}');
        assert!(警告们[0].说明.contains("形似拉丁字母 'p'"));
    }

    #[test]
    fn 跨行位置计数() {
        // 西里尔字符告警依赖当前语言为中文（俄语下方豁免），需持锁串行
        let _护 = crate::语言::钉住语言("zh");
        let 源码 = "函数 主函数() {\n    让 x = 1;\n    а = 2;\n}";
        let 警告们 = 检查混淆(源码);
        assert_eq!(警告们.len(), 1);
        assert_eq!(警告们[0].行号, 3);
        assert_eq!(警告们[0].列号, 5);
    }

    #[test]
    fn 俄语方言跳过西里尔同形() {
        // 作用域守卫：离开作用域（含断言失败）自动恢复中文，无需手工还原
        let _护 = crate::语言::钉住语言("ru");
        // 西里尔字符在俄语方言中是合法标识符字符，不再报同形异义告警
        let 警告们 = 检查混淆("пусть а = 1;");
        assert!(警告们.is_empty(), "ru 方言不应误报西里尔字符");
        // 希腊字母与零宽字符在俄语方言下仍然告警
        let 警告们 = 检查混淆("пусть ρ\u{200B} = 1;");
        assert_eq!(警告们.len(), 2);
        assert!(警告们.iter().any(|警| 警.类别 == 混淆类别::同形字符));
        assert!(警告们.iter().any(|警| 警.类别 == 混淆类别::零宽字符));
    }

    #[test]
    fn 警告格式化输出() {
        let _护 = crate::语言::钉住语言("zh");
        let 警告 = 混淆警告 {
            行号: 2,
            列号: 5,
            涉事字符: '\u{200B}',
            类别: 混淆类别::零宽字符,
            说明: "零宽空格。此类字符肉眼不可见，可能被用于隐藏代码或绕过检测".to_string(),
        };
        let 文字 = 警告.展示行();
        assert!(文字.contains("第 2 行第 5 列"));
        assert!(文字.contains("零宽字符"));
        assert!(文字.contains("U+200B"));
    }
}
