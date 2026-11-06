//! 导出冲突提示：在导出前扫一遍「转译会静默改变你代码含义」的位置
//!
//! 与教学检查同一定位：只提示、不阻断（`--no-lint` 选项一并关闭）。规则刻意保守，
//! 只报**能确定会被改写**的三类位置，避免把正常代码当问题：
//!
//! 1. 结构体字段名命中关键字/宏词条：字段定义被词法层改写，序列化键与字段名随之变化
//!    （`类型: 字符串` → `type: String`）；
//! 2. 声明的名字命中关键字/宏词条：`让 自身 = 1` → `let Self = 1`、
//!    `结构体 配置` → `struct cfg!`（入口函数按惯例映射，不报）；
//! 3. 会恐慌的词条被当文本用：`打印行!("{}", 解包(甲))` 在格式化实参里
//!    直接触发恐慌，而不是产出文本。
//!
//! 位置一律按**母语源文件**的行列出（提示在写盘前扫描，不经产物）。
//! 别名词条（`长度`、`版本` 等）在声明位有豁免，不会被改写，故不提示。

use std::collections::{HashMap, HashSet};

use rustc_lexer::{TokenKind, tokenize};

use crate::映射管理器::映射管理器;

/// 提示种类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum 提示种类 {
    /// 结构体字段名命中词条：字段定义会被改写，序列化键随之变化
    字段遮蔽,
    /// 声明的名字（结构体/枚举/函数/变量）命中词条：声明本身被改写
    声明名改写,
    /// 会恐慌的词条的结果被当文本使用
    恐慌当文本,
}

/// 单条导出冲突提示：位置（1 起行/列）+ 种类 + 相关词条
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct 冲突提示 {
    /// 行号（从 1 开始）
    pub 行号: usize,
    /// 列号（从 1 开始，按字符计数）
    pub 列号: usize,
    /// 种类
    pub 种类: 提示种类,
    /// 触发的词条（源码里写的母语词）
    pub 词条: String,
    /// 该词条的转译目标（母语词 → 对应英文关键字）
    pub 目标: String,
}

impl 冲突提示 {
    /// 转成教学提示文本，模板随当前语言变化
    pub fn 转提示文本(&self) -> String {
        let 行号文本 = self.行号.to_string();
        let 列号文本 = self.列号.to_string();
        match self.种类 {
            提示种类::字段遮蔽 => crate::语言::取带参消息(
                "eject_hint_field_shadowed",
                &[&行号文本, &列号文本, &self.词条, &self.目标],
            ),
            提示种类::声明名改写 => crate::语言::取带参消息(
                "eject_hint_declared_name",
                &[&行号文本, &列号文本, &self.词条, &self.目标],
            ),
            提示种类::恐慌当文本 => crate::语言::取带参消息(
                "eject_hint_panic_as_text",
                &[&行号文本, &列号文本, &self.词条, &self.目标],
            ),
        }
    }
}

/// 扫描母语源码，返回全部导出冲突提示（按出现顺序）
///
/// 只依赖映射表与源码文本：不读文件、不联网、不看环境变量。
pub fn 扫描冲突(母语源: &str, 管理器: &映射管理器) -> Vec<冲突提示> {
    let 记号列表 = 切分记号(母语源);
    let 关键词表 = 管理器.取关键词表();
    let 别名表 = 管理器.取别名表();
    let 宏表 = 管理器.取宏表();
    let mut 汇聚 = 提示汇聚::default();

    // 方言拼写由词条表反查（语言无关）：转译目标为结构体关键字的那些键，
    // 就是「结构体」这个词的方言拼写
    let 反查词条 = |目标: &str| -> Vec<&str> {
        关键词表
            .iter()
            .filter(|(_, 词条值)| 词条值.as_str() == 目标)
            .map(|(词条, _)| 词条.as_str())
            .collect()
    };
    let 结构体词 = 反查词条("struct");
    let 枚举词 = 反查词条("enum");
    let 函数词 = 反查词条("fn");
    let 让词 = 反查词条("let");
    let 公开词 = 反查词条("pub");

    for (索引, 令牌) in 记号列表.iter().enumerate() {
        let 令牌文本 = 令牌.令牌文本;
        if 结构体词.contains(&令牌文本) || 枚举词.contains(&令牌文本) {
            if let Some(名字) = 记号列表.get(索引 + 1) {
                报告声明名(名字, 关键词表, &mut 汇聚);
            }
            if 结构体词.contains(&令牌文本) {
                检查结构体字段(&记号列表, 索引, 关键词表, &公开词, &mut 汇聚);
            }
        } else if 函数词.contains(&令牌文本) || 让词.contains(&令牌文本) {
            if let Some(名字) = 记号列表.get(索引 + 1) {
                报告声明名(名字, 关键词表, &mut 汇聚);
            }
        }
    }

    检查恐慌当文本(&记号列表, 管理器, 宏表, 别名表, &mut 汇聚);
    汇聚.提示集
}

/// 收集提示（同一位置的同一类提示只报一次）
#[derive(Default)]
struct 提示汇聚 {
    提示集: Vec<冲突提示>,
    已见: HashSet<(usize, usize, 提示种类)>,
}

impl 提示汇聚 {
    fn 记录(&mut self, 令牌: &记号<'_>, 种类: 提示种类, 词条: &str, 目标: &str) {
        if self.已见.insert((令牌.行号, 令牌.列号, 种类)) {
            self.提示集.push(冲突提示 {
                行号: 令牌.行号,
                列号: 令牌.列号,
                种类,
                词条: 词条.to_string(),
                目标: 目标.to_string(),
            });
        }
    }
}

/// 声明名命中关键字词条 → 声明本身会被改写
///
/// 入口函数按项目惯例映射，不提示。
fn 报告声明名(
    名字: &记号<'_>, 关键词表: &HashMap<String, String>, 汇聚: &mut 提示汇聚
) {
    if !名字.是否普通标识符 {
        return;
    }
    if let Some(目标) = 关键词表.get(名字.令牌文本) {
        if 目标 != "main" && 目标 != 名字.令牌文本 {
            汇聚.记录(名字, 提示种类::声明名改写, 名字.令牌文本, 目标);
        }
    }
}

/// 扫描结构体字段名，命中关键字/宏词条时报告（字段定义会被改写）
fn 检查结构体字段(
    记号列表: &[记号<'_>],
    结构体索引: usize,
    关键词表: &HashMap<String, String>,
    公开词: &[&str],
    汇聚: &mut 提示汇聚,
) {
    // 找结构体体的 `{`（跳过泛型约束子句；`;` 结尾的单元结构体没有字段）
    let mut 索引 = 结构体索引 + 1;
    while 索引 < 记号列表.len() && 记号列表[索引].令牌文本 != "{" && 记号列表[索引].令牌文本 != ";"
    {
        索引 += 1;
    }
    if 索引 >= 记号列表.len() || 记号列表[索引].令牌文本 != "{" {
        return;
    }
    let mut 深度 = 0usize;
    while 索引 < 记号列表.len() {
        match 记号列表[索引].令牌文本 {
            "{" => 深度 += 1,
            "}" => {
                深度 -= 1;
                if 深度 == 0 {
                    return;
                }
            }
            ":" if 深度 == 1 => {
                if let Some(名字) = 冒号前字段名(记号列表, 索引, 公开词) {
                    if let Some(目标) = 关键词表.get(名字.令牌文本) {
                        if 目标 != 名字.令牌文本 {
                            汇聚.记录(名字, 提示种类::字段遮蔽, 名字.令牌文本, 目标);
                        }
                    }
                }
            }
            _ => {}
        }
        索引 += 1;
    }
}

/// 判断 `:` 前的标识符是否字段名（字段以 `{`/`,`/`]`/可见性词开头，类型路径段不是）
fn 冒号前字段名<'甲, '乙>(
    记号列表: &'甲 [记号<'乙>],
    冒号索引: usize,
    公开词: &[&str],
) -> Option<&'甲 记号<'乙>> {
    let 名字 = 记号列表.get(冒号索引.checked_sub(1)?)?;
    if !名字.是否普通标识符 {
        return None;
    }
    let 前面 = 记号列表.get(冒号索引.checked_sub(2)?)?;
    let 字段起始 =
        matches!(前面.令牌文本, "{" | "," | "]" | "pub") || 公开词.contains(&前面.令牌文本);
    字段起始.then_some(名字)
}

/// 会恐慌的词条出现在文本语境：文本宏实参内，或结果上直接接文本方法
fn 检查恐慌当文本(
    记号列表: &[记号<'_>],
    管理器: &映射管理器,
    宏表: &HashMap<String, String>,
    别名表: &HashMap<String, String>,
    汇聚: &mut 提示汇聚,
) {
    // 文本宏：实参会被格式化，实参里恐慌等于直接崩
    let 是文本宏 = |词条: &str| -> bool {
        宏表.get(词条).is_some_and(|目标| {
            matches!(
                目标.as_str(),
                "format"
                    | "println"
                    | "print"
                    | "eprintln"
                    | "eprint"
                    | "panic"
                    | "write"
                    | "writeln"
                    | "assert"
                    | "assert_eq"
                    | "assert_ne"
                    | "debug_assert"
                    | "debug_assert_eq"
            )
        })
    };
    // 文本处理方法：作用在「结果」上等于把结果当文本用
    let 是文本方法 = |词条: &str| -> bool {
        别名表.get(词条).is_some_and(|目标| {
            matches!(
                目标.as_str(),
                "to_string"
                    | "to_lowercase"
                    | "to_uppercase"
                    | "to_ascii_lowercase"
                    | "to_ascii_uppercase"
                    | "trim"
                    | "trim_start"
                    | "trim_end"
                    | "len"
                    | "starts_with"
                    | "ends_with"
                    | "contains"
                    | "join"
                    | "replace"
                    | "split"
                    | "split_once"
                    | "push_str"
                    | "insert_str"
            )
        })
    };
    // 会恐慌的别名词条（宏词条排除：`恐慌!(…)` 本身就是显式恐慌）
    let 恐慌目标 = |词条: &str| -> Option<&str> {
        if 宏表.contains_key(词条) {
            return None;
        }
        let 目标 = 别名表.get(词条)?;
        管理器.取危险详情(词条).等级.会恐慌.then_some(目标.as_str())
    };

    // 每个 `(` 是否处在文本宏实参内（嵌套继承）
    let mut 在文本宏: Vec<bool> = Vec::new();
    for (索引, 令牌) in 记号列表.iter().enumerate() {
        match 令牌.令牌文本 {
            "(" => {
                // 宏调用写成 `词条 ! (`：`(` 前一个是 `!`，宏名再往前一个
                let mut 前面 = 索引.checked_sub(1).and_then(|令牌项| 记号列表.get(令牌项));
                if 前面.is_some_and(|令牌项| 令牌项.令牌文本 == "!") {
                    前面 = 索引.checked_sub(2).and_then(|令牌项| 记号列表.get(令牌项));
                }
                let 外层 = 在文本宏.last().copied().unwrap_or(false)
                    || 前面.is_some_and(|令牌项| 是文本宏(令牌项.令牌文本));
                在文本宏.push(外层);
                continue;
            }
            ")" => {
                在文本宏.pop();
                continue;
            }
            _ => {}
        }
        if !令牌.是否普通标识符 {
            continue;
        }
        // 宏调用（`恐慌!(`）是显式恐慌，不提示
        if 记号列表
            .get(索引 + 1)
            .is_some_and(|后续| 后续.令牌文本 == "!")
        {
            continue;
        }
        if let Some(目标) = 恐慌目标(令牌.令牌文本) {
            let 当文本 = 在文本宏.last().copied().unwrap_or(false)
                || 调用结果当文本(记号列表, 索引, &是文本方法);
            if 当文本 {
                汇聚.记录(令牌, 提示种类::恐慌当文本, 令牌.令牌文本, 目标);
            }
        }
    }
}

/// 调用结果（`词条(…)`）之后是否直接接文本方法（`.转字符串()` 等）
fn 调用结果当文本(
    记号列表: &[记号<'_>],
    词条索引: usize,
    是文本方法: &dyn Fn(&str) -> bool,
) -> bool {
    if 记号列表.get(词条索引 + 1).map(|令牌| 令牌.令牌文本) != Some("(") {
        return false;
    }
    let mut 深度 = 0usize;
    let mut 索引 = 词条索引 + 1;
    while 索引 < 记号列表.len() {
        match 记号列表[索引].令牌文本 {
            "(" => 深度 += 1,
            ")" => {
                深度 -= 1;
                if 深度 == 0 {
                    索引 += 1;
                    break;
                }
            }
            _ => {}
        }
        索引 += 1;
    }
    if 记号列表.get(索引).map(|令牌| 令牌.令牌文本) != Some(".") {
        return false;
    }
    记号列表
        .get(索引 + 1)
        .is_some_and(|方法名| 方法名.是否普通标识符 && 是文本方法(方法名.令牌文本))
}

/// 词法记号（仅保留代码位置的记号，附行列）
struct 记号<'甲> {
    令牌文本: &'甲 str,
    行号: usize,
    列号: usize,
    /// 普通标识符（`r#` 原始标识符不算：它不会被改写）
    是否普通标识符: bool,
}

/// 用词法解析器切分源码并换算行列（空白/注释之外的记号按序返回）
fn 切分记号(母语源: &str) -> Vec<记号<'_>> {
    let mut 行起点 = vec![0usize];
    行起点.extend(母语源.match_indices('\n').map(|(偏移, _)| 偏移 + 1));

    let mut 记号列表 = Vec::new();
    let mut 偏移 = 0usize;
    for 词元 in tokenize(母语源) {
        let 起始 = 偏移;
        偏移 += 词元.len;
        if matches!(
            词元.kind,
            TokenKind::Whitespace
                | TokenKind::LineComment
                | TokenKind::BlockComment { .. }
                | TokenKind::Unknown
        ) {
            continue;
        }
        let 行索引 = match 行起点.binary_search(&起始) {
            Ok(索引) => 索引,
            Err(索引) => 索引.saturating_sub(1),
        };
        let 列号 = 母语源[行起点[行索引]..起始].chars().count() + 1;
        记号列表.push(记号 {
            令牌文本: &母语源[起始..偏移],
            行号: 行索引 + 1,
            列号,
            是否普通标识符: matches!(词元.kind, TokenKind::Ident),
        });
    }
    记号列表
}

#[cfg(test)]
mod 母语测试 {
    use super::*;
    use std::path::Path;

    /// 真实中文内置语言包（与命令行同源）
    fn 内置管理器() -> 映射管理器 {
        let 语言目录 = Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh");
        映射管理器::从目录载入(&语言目录).expect("内置语言包应可加载")
    }

    fn 种类列表(提示: &[冲突提示]) -> Vec<提示种类> {
        提示.iter().map(|提示项| 提示项.种类).collect()
    }

    #[test]
    fn 字段名命中会被报告() {
        let 母语源 = "结构体 甲 {\n    类型: 字符串,\n    长度: 整数,\n}\n";
        let 提示 = 扫描冲突(母语源, &内置管理器());
        assert_eq!(种类列表(&提示), vec![提示种类::字段遮蔽]);
        assert_eq!(提示[0].词条, "类型");
        assert_eq!(提示[0].目标, "type");
        assert_eq!((提示[0].行号, 提示[0].列号), (2, 5));
    }

    #[test]
    fn 别名字段名静默() {
        // 别名类词条（长度/版本）在声明位有豁免，不会改写，不应提示
        let 母语源 = "结构体 甲 {\n    长度: 整数,\n    版本: 字符串,\n}\n";
        assert!(扫描冲突(母语源, &内置管理器()).is_empty());
    }

    #[test]
    fn 字段类型路径不报告() {
        // 字段类型里的路径段（`模块::类型`）不是字段名，不得误报
        let 母语源 = "结构体 甲 {\n    计数: 模块::类型,\n}\n";
        assert!(扫描冲突(母语源, &内置管理器()).is_empty());
    }

    #[test]
    fn 声明名命中关键字报告() {
        let 母语源 = "函数 主函数() {\n    让 自身 = 1;\n}\n";
        let 提示 = 扫描冲突(母语源, &内置管理器());
        assert_eq!(种类列表(&提示), vec![提示种类::声明名改写]);
        assert_eq!(提示[0].词条, "自身");
        assert_eq!(提示[0].目标, "Self");
        // 第 2 行 `    让 自身 = 1;`：四个空格 + 「让」+ 空格 → 「自身」在第 7 列
        assert_eq!((提示[0].行号, 提示[0].列号), (2, 7));
    }

    #[test]
    fn 入口函数不报告() {
        // 主函数 → 入口名 是项目惯例，不该当问题报出来
        let 母语源 = "函数 主函数() {\n    让 甲 = 1;\n}\n";
        assert!(扫描冲突(母语源, &内置管理器()).is_empty());
    }

    #[test]
    fn 结构体名命中关键字报告() {
        // 结构体 配置 → 该词在宏表里也有，词法层直接改写
        let 提示 = 扫描冲突("结构体 配置 {\n    甲: 整数,\n}\n", &内置管理器());
        assert_eq!(种类列表(&提示), vec![提示种类::声明名改写]);
        assert_eq!(提示[0].词条, "配置");
    }

    #[test]
    fn 恐慌词在格式化实参内报告() {
        let 母语源 = "函数 主函数() {\n    让 甲 = 有值(3);\n    打印行!(\"{}\", 解包(甲));\n}\n";
        let 提示 = 扫描冲突(母语源, &内置管理器());
        assert_eq!(种类列表(&提示), vec![提示种类::恐慌当文本]);
        assert_eq!(提示[0].词条, "解包");
        assert_eq!(提示[0].目标, "unwrap");
        assert_eq!(提示[0].行号, 3);
    }

    #[test]
    fn 恐慌词结果当文本报告() {
        let 母语源 = "函数 主函数() {\n    让 甲 = 有值(1).解包().转字符串();\n}\n";
        let 提示 = 扫描冲突(母语源, &内置管理器());
        assert_eq!(种类列表(&提示), vec![提示种类::恐慌当文本]);
        assert_eq!(提示[0].词条, "解包");
    }

    #[test]
    fn 恐慌词他处静默() {
        // 只当值用、不进文本语境：不提示（避免把正常代码刷成一片警告）
        let 母语源 = "函数 主函数() {\n    让 甲 = 有值(3).解包();\n    让 乙 = 甲 + 1;\n}\n";
        assert!(扫描冲突(母语源, &内置管理器()).is_empty());
    }

    #[test]
    fn 显式恐慌宏静默() {
        // `恐慌!(…)` 本身就是显式选择，不提示
        let 母语源 = "函数 主函数() {\n    恐慌!(\"{}\", 1);\n}\n";
        assert!(扫描冲突(母语源, &内置管理器()).is_empty());
    }

    #[test]
    fn 安全数据库列词静默() {
        // 「取列」已改为安全默认（可选取值），在文本位置也不再触发恐慌提示
        let 母语源 = "函数 主函数() {\n    打印行!(\"{}\", 取列(0));\n}\n";
        assert!(扫描冲突(母语源, &内置管理器()).is_empty());
    }
}
