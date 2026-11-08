//! 错误消息翻译结构：从语言包 errors.toml 加载错误码表与消息表。

use crate::故障::{载入故障, 载入目标};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// 错误消息翻译条目（从 errors.toml 加载）
#[derive(Deserialize, Debug, Clone)]
pub struct 错误消息条目 {
    pub 消息模板: String,
    pub 教学提示: Option<String>,
}

/// 消息键的通配符：`?` 表示一段动态内容（非贪婪，按后续字面量定位）
///
/// 带通配的键称「通配段键」：一次匹配即可拿到中间与末尾两个动态名（`{q0}`/`{q1}`），
/// 解决「两端动态」的消息无法用单一前缀/后缀键完整翻译的问题。
pub const 通配符: char = '?';

/// 消息表匹配的动态部分来源
///
/// 前缀键与后缀键命中的残段形态不同，占位符提取方式也不同：
/// - 前缀键：残段是键之后的尾段，开引号已被键剥离；
/// - 后缀键（键以 `~` 开头）：残段是键之前的头段，引号对完整；
/// - 通配段键（含 `?`）：逐个捕获每个 `?` 之间的原文。
#[derive(Debug, Clone)]
pub enum 消息残段<'乙> {
    /// 前缀键命中的尾段
    尾段(&'乙 str),
    /// 后缀键命中的头段
    头段(&'乙 str),
    /// 通配段键命中的逐个捕获（第 n 个 `?` 之间的原文）
    片段集(Vec<&'乙 str>),
}

impl<'乙> 消息残段<'乙> {
    /// 未翻译原文残段（模板无占位符命中时按原文拼接到模板后）
    ///
    /// 通配段键不适用：其动态部分已被逐个 `{qN}` 完整接管，残段为空。
    pub fn 文字(&self) -> &'乙 str {
        match self {
            Self::尾段(串) | Self::头段(串) => 串,
            Self::片段集(_) => "",
        }
    }
}

/// 错误消息翻译管理器：按错误码或消息文本查询翻译条目
#[derive(Debug, Clone)]
pub struct 错误消息管理器 {
    /// 错误码表：错误码 → 翻译条目（`errors.toml` 顶层 `[E0xxx]` 节）
    pub 翻译表: HashMap<String, 错误消息条目>,
    /// 消息表：英文消息原文 → 翻译条目（errors.toml [消息翻译] 节）
    消息映射: HashMap<String, 错误消息条目>,
}

impl 错误消息管理器 {
    /// 从文件加载错误翻译表
    pub fn 从文件载入(目标路径: &Path) -> Result<Self, 载入故障> {
        let 内容 = fs::read_to_string(目标路径).map_err(|坏| 载入故障::读取失败 {
            目标: 载入目标::错误消息表,
            文件路径: Some(目标路径.display().to_string()),
            详情: 坏.to_string(),
        })?;
        Self::从字符串载入(&内容)
    }

    /// 从 TOML 字符串加载错误翻译表
    ///
    /// 顶层表分为两类：错误码表（含模板与提示）与消息表（键为英文消息原文）。
    pub fn 从字符串载入(内容: &str) -> Result<Self, 载入故障> {
        let 值对象: toml::Value =
            toml::from_str(内容).map_err(|坏| 载入故障::解析失败 {
                目标: 载入目标::错误消息表,
                文件路径: None,
                详情: 坏.to_string(),
            })?;
        let mut 翻译表 = HashMap::new();
        let mut 消息映射 = HashMap::new();
        if let Some(顶层表) = 值对象.as_table() {
            for (键名, 映射值) in 顶层表 {
                if 键名 == "消息翻译" {
                    if let Some(子表) = 映射值.as_table() {
                        for (消息文, 翻译条目) in 子表 {
                            消息映射.insert(消息文.clone(), 条目自值(翻译条目));
                        }
                    }
                } else {
                    翻译表.insert(键名.clone(), 条目自值(映射值));
                }
            }
        }
        Ok(Self {
            翻译表, 消息映射
        })
    }

    /// 按错误码查询翻译条目
    pub fn 按码查询(&self, 错误码: &str) -> Option<&错误消息条目> {
        self.翻译表.get(错误码)
    }

    /// 按消息原文查询翻译条目
    ///
    /// 匹配顺序：精确匹配 → 最长通配段键 → 最长前缀匹配 → 最长后缀匹配。
    /// 返回未匹配的动态部分（前缀键为尾段、后缀键为头段、通配段键为逐个捕获），
    /// 供调用方填充模板占位符或拼接到模板后。
    pub fn 按消息查询<'甲, '乙>(
        &'甲 self,
        原文: &'乙 str,
    ) -> Option<(&'甲 错误消息条目, Option<消息残段<'乙>>)> {
        // 1. 精确匹配
        if let Some(翻译条目) = self.消息映射.get(原文) {
            return Some((翻译条目, None));
        }
        // 2. 通配段键（字面量总长最长者胜：越具体越优先）
        let mut 最佳段: Option<(usize, Vec<&'乙 str>, &'甲 错误消息条目)> = None;
        // 3. 最长前缀 / 最长后缀候选（前缀优先，其模板通常更完整、含类型词）
        let mut 最佳前缀: Option<(usize, &'乙 str, &'甲 错误消息条目)> = None;
        let mut 最佳后缀: Option<(usize, &'乙 str, &'甲 错误消息条目)> = None;
        for (键名, 翻译条目) in &self.消息映射 {
            if let Some(后缀键) = 键名.strip_prefix('~') {
                if let Some(前缀) = 原文.strip_suffix(后缀键)
                    && 最佳后缀.is_none_or(|(段长, _, _)| 后缀键.len() > 段长)
                {
                    最佳后缀 = Some((后缀键.len(), 前缀, 翻译条目));
                }
            } else if 键名.contains(通配符) {
                if let Some(捕获集) = 匹配段(键名, 原文)
                    && 最佳段
                        .as_ref()
                        .is_none_or(|(段长, _, _)| 字面量总长(键名) > *段长)
                {
                    最佳段 = Some((字面量总长(键名), 捕获集, 翻译条目));
                }
            } else if 原文.starts_with(键名.as_str())
                && 最佳前缀.is_none_or(|(段长, _, _)| 键名.len() > 段长)
            {
                最佳前缀 = Some((键名.len(), &原文[键名.len()..], 翻译条目));
            }
        }
        if let Some((_, 捕获集, 翻译条目)) = 最佳段 {
            // 通配段捕获已借自原文（'乙），直接归入片段集随返回值存活；
            // 仅当模板确实引用占位符时保留捕获，否则丢弃空片段集。
            let 残段值 = if 翻译条目.消息模板.contains("{q") {
                消息残段::片段集(捕获集)
            } else {
                消息残段::片段集(Vec::new())
            };
            return Some((翻译条目, Some(残段值)));
        }
        if let Some((_, 尾段, 翻译条目)) = 最佳前缀 {
            return Some((翻译条目, Some(消息残段::尾段(尾段))));
        }
        最佳后缀.map(|(_, 头段, 翻译条目)| (翻译条目, Some(消息残段::头段(头段))))
    }

    /// 已覆盖的错误码数量
    pub fn 覆盖数(&self) -> usize {
        self.翻译表.len()
    }
}

/// 将通配段键拆为字面量段（以 `?` 为分界）
fn 分割通配(键名: &str) -> Vec<&str> {
    键名.split(通配符).collect()
}

/// 通配段键的字面量总长度（多键命中同一消息时的特异度排序依据）
fn 字面量总长(键名: &str) -> usize {
    分割通配(键名).iter().map(|段| 段.chars().count()).sum()
}

/// 用通配段键匹配消息，成功返回每个 `?` 捕获的原文（按出现顺序）
///
/// 语义为非贪婪：每个字面量段从上一段之后首次出现的位置开始找。首段为前缀锚
/// （须非空），尾段为后缀锚（可空），中间为捕获间的分隔锚，要求依次不重叠出现。
fn 匹配段<'乙>(键名: &str, 原文: &'乙 str) -> Option<Vec<&'乙 str>> {
    let 字面量段集 = 分割通配(键名);
    if 字面量段集.is_empty() {
        return None;
    }
    let 首段 = 字面量段集[0];
    let 其余字面量 = &字面量段集[1..];
    if 首段.is_empty() {
        return None;
    }
    let 尾锚 = *其余字面量.last()?;
    let 正文 = 原文.strip_prefix(首段)?;
    if !正文.ends_with(尾锚) {
        return None;
    }
    let 分隔锚 = &其余字面量[..其余字面量.len() - 1];
    let mut 捕获集 = Vec::new();
    let mut 剩余 = 正文;
    for 隔符 in 分隔锚 {
        let 命中处 = 剩余.find(隔符)?;
        捕获集.push(&剩余[..命中处]);
        剩余 = &剩余[命中处 + 隔符.len()..];
    }
    捕获集.push(剩余.strip_suffix(尾锚)?);
    Some(捕获集)
}

/// 用动态部分填充模板的 `{q0}`/`{q1}`（恒等本地化）
pub fn 填充动态占位(译文模板: &str, 残段: &消息残段<'_>) -> (String, bool) {
    填充动态占位定制(译文模板, 残段, &mut |词元| 词元.to_string())
}

/// 可注入本地化版本的动态占位填充
///
/// 本地化作用在每个即将写进模板的动态捕获值上，一次一处：调用方因此能区分
/// 「编译器注入的类型名」与「作者写在模板字面里的英文标识符」。主消息走此入口，
/// 建议通道与审计/工具通道继续用恒等闭包（填充动态占位），行为不变。
pub fn 填充动态占位定制(
    译文模板: &str,
    残段: &消息残段<'_>,
    本地化: &mut dyn FnMut(&str) -> String,
) -> (String, bool) {
    let mut 成品 = 译文模板.to_string();
    let mut 已消费 = false;
    // 占位符编号上界：前缀/后缀键的启发式只区分 `{q0}`/`{q1}`；`{q2}` 仅对通配段键有意义。
    for 序号 in 0..3 {
        let 占位符 = format!("{{q{序号}}}");
        if 成品.contains(&占位符) {
            let 内容 = match 残段 {
                消息残段::片段集(捕获集) => 捕获集.get(序号).copied(),
                消息残段::头段(动态段) => {
                    if 动态段.contains('`') {
                        if 序号 == 1 || 动态段.matches('`').count() == 1 {
                            动态段.rsplit('`').nth(1)
                        } else {
                            动态段.split('`').nth(1)
                        }
                    } else {
                        动态段.split('\'').nth(序号 * 2 + 1)
                    }
                }
                消息残段::尾段(动态段) => {
                    if 动态段.contains('`') {
                        if 序号 == 1 {
                            动态段.rsplit('`').nth(1)
                        } else if let Some(后) = 动态段.strip_prefix('`') {
                            后.split('`').next()
                        } else {
                            动态段.split('`').next()
                        }
                    } else {
                        动态段.split('\'').nth(序号 * 2)
                    }
                }
            };
            match 内容 {
                Some(内容) if !内容.is_empty() => {
                    成品 = 成品.replace(&占位符, &本地化(内容));
                    已消费 = true;
                }
                // 捕获为空或失败：原样回退，交由调用方拼接原文
                _ => return (译文模板.to_string(), false),
            }
        }
    }
    (成品, 已消费)
}

/// 从 TOML 值构建翻译条目（模板缺失时回退空串，避免解析失败丢失整表）
fn 条目自值(值对象: &toml::Value) -> 错误消息条目 {
    错误消息条目 {
        消息模板: 值对象
            .get("消息模板")
            .and_then(|映射值| 映射值.as_str())
            .unwrap_or_default()
            .to_string(),
        教学提示: 值对象
            .get("教学提示")
            .and_then(|映射值| 映射值.as_str())
            .map(|串| 串.to_string()),
    }
}

/// 单条 rustc 消息在给定语言包下的译文审计结果（供审计示例与门禁测试共用）
#[derive(Debug, Clone)]
pub struct 消息审计 {
    /// 命中形态：CODE/EXACT/PREFIX/SUFFIX/SEGMENT/MISS
    pub 种类: String,
    /// 渲染后的译文（含未消费残段回拼）
    pub 已渲染: String,
    /// 渲染后仍残留的英文词串（空表示已译全）
    pub 残留: Vec<String>,
}

/// 连续多少个英文单词即视为「未翻译残留」（单个标识符关键字之类不报）
const 最少英文词数: usize = 4;

/// 从文本中提取长度≥最少英文词数的连续 ASCII 英文词串
///
/// 分隔符仅用于切词：连续的标点/反引号/逗号会切出空串，但空串不算英文、
/// 也不算译文，必须跳过而非中断词串，否则整句未译的英文会被拆成碎串而漏报。
fn 英文词串(正文: &str) -> Vec<String> {
    let mut 词串集 = Vec::new();
    let mut 当前: Vec<&str> = Vec::new();
    for 词元 in 正文.split(|每字: char| !每字.is_ascii_alphanumeric()) {
        if 词元.is_empty() {
            continue;
        }
        if 词元.chars().all(|每字: char| 每字.is_alphanumeric())
            && 词元.contains(|每字: char| 每字.is_ascii_alphabetic())
        {
            当前.push(词元);
        } else {
            if 当前.len() >= 最少英文词数 {
                词串集.push(当前.join(" "));
            }
            当前.clear();
        }
    }
    if 当前.len() >= 最少英文词数 {
        词串集.push(当前.join(" "));
    }
    词串集
}

/// 码表桩是否会按「反引号记号出现顺序」猜句子结构
///
/// 桩里的类型/特征等占位符没有语义来源，渲染时把消息里的反引号内容按先后顺序
/// 逐个塞进去；同一错误码下编译器会输出结构完全不同的句子，猜错就答非所问。
/// 只用由跨度标签可靠提取的期望/实际、或不含占位符的桩不属于猜结构。
pub(crate) fn 桩猜测标记形态(翻译条目: &错误消息条目) -> bool {
    super::诊断翻译::码模板占位符集
        .iter()
        .any(|占位| 翻译条目.消息模板.contains(占位))
}

/// 消息表命中结果里的「整句条目」（精确键或通配段键）
pub(crate) fn 整句命中<'甲>(
    查询结果: &Option<(&'甲 错误消息条目, Option<消息残段<'_>>)>,
) -> Option<&'甲 错误消息条目> {
    let Some((翻译条目, 残段)) = 查询结果 else {
        return None;
    };
    match 残段 {
        None | Some(消息残段::片段集(_)) => Some(*翻译条目),
        _ => None,
    }
}

/// 主消息取哪张表：消息表的整句译文，还是会猜槽位的码表桩
///
/// 规约：码表桩要靠记号顺序猜句子结构、而消息表对该句有整条译文时，用消息表；
/// 其余情形维持既有的「码表桩优先」。
pub(crate) fn 消息表胜出(
    码表条目: Option<&错误消息条目>,
    整句条目: Option<&错误消息条目>,
) -> bool {
    let (Some(桩), Some(_)) = (码表条目, 整句条目) else {
        return false;
    };
    桩猜测标记形态(桩)
}

/// 用引擎自身的匹配器审计一条消息在语言包中的译文残留
///
/// 渲染路径与翻译器完全一致，因此这里报出的残留就是用户在终端里实际看到的文字。
pub fn 审计消息(
    译表: &错误消息管理器, 级别: &str, 码: &str, 原文: &str
) -> 消息审计 {
    // 主消息优先看错误码表（完整桩）：若该码已有模板，主消息就不会残留英文。
    // 例外：桩要靠记号顺序猜结构而消息表有整句译文时，渲染走的是消息表。
    if 级别 == "main"
        && !码.is_empty()
        && let Some(翻译条目) = 译表.按码查询(码)
        && !翻译条目.消息模板.is_empty()
        && !消息表胜出(Some(翻译条目), 整句命中(&译表.按消息查询(原文)))
    {
        return 消息审计 {
            种类: "CODE".to_string(),
            已渲染: 翻译条目.消息模板.clone(),
            残留: Vec::new(),
        };
    }
    match 译表.按消息查询(原文) {
        // 完全未命中：整条英文原文都是残留。
        None => 消息审计 {
            种类: "MISS".to_string(),
            已渲染: String::new(),
            残留: 英文词串(原文),
        },
        // 精确命中：译文即我方模板，不再度量模板。
        Some((翻译条目, None)) => 消息审计 {
            种类: "EXACT".to_string(),
            已渲染: 翻译条目.消息模板.clone(),
            残留: Vec::new(),
        },
        Some((翻译条目, Some(残段))) => {
            let 种类标签 = match &残段 {
                消息残段::尾段(_) => "PREFIX",
                消息残段::头段(_) => "SUFFIX",
                消息残段::片段集(_) => "SEGMENT",
            };
            let (mut 已渲染, 已消费) = 填充动态占位(&翻译条目.消息模板, &残段);
            if !已消费 {
                已渲染.push_str(残段.文字());
                // 未消费＝把英文残段原样回拼，只对残段判残留。
                return 消息审计 {
                    种类: 种类标签.to_string(),
                    已渲染,
                    残留: 英文词串(残段.文字()),
                };
            }
            消息审计 {
                种类: 种类标签.to_string(),
                已渲染,
                残留: Vec::new(),
            }
        }
    }
}

#[cfg(test)]
mod 母语测试 {
    use super::*;

    /// 从消息表点号表头片段构建消息管理器（测试辅助）
    fn 建表(片段文字: &str) -> 错误消息管理器 {
        错误消息管理器::从字符串载入(片段文字).expect("测试用 TOML 应能解析")
    }

    /// 渲染消息表命中结果：填充占位符，未消费时拼回残段（与调用方同逻辑）
    fn 渲染(译表: &错误消息管理器, 原文: &str) -> Option<String> {
        let (翻译条目, 残段) = 译表.按消息查询(原文)?;
        let mut 文字 = 翻译条目.消息模板.clone();
        if let Some(残段) = 残段 {
            let (已填充, 已消费) = 填充动态占位(&文字, &残段);
            文字 = 已填充;
            if !已消费 {
                文字.push_str(残段.文字());
            }
        }
        Some(文字)
    }

    /// 单 `?` 通配段键（回归）：仅两段时也必须能匹配
    #[test]
    fn 通配段单捕获() {
        let 表 = 建表(
            "[\"消息翻译\".\"consider specifying a concrete type for the type parameter `?`\"]\n\"消息模板\" = \"考虑为类型参数 `{q0}` 指定具体类型\"",
        );
        assert_eq!(
            渲染(
                &表,
                "consider specifying a concrete type for the type parameter `T`"
            )
            .as_deref(),
            Some("考虑为类型参数 `T` 指定具体类型")
        );
    }

    /// 英文词串跨标点连续计数（回归）：反引号/逗号拆出的空串不得中断词串
    #[test]
    fn 英文词串跨标点() {
        let 串集 = 英文词串("variable `x` is assigned to, but never used");
        assert!(
            串集
                .iter()
                .any(|串| 串.contains("assigned") && 串.contains("never")),
            "整句英文应作为连续词串报出，实际: {串集:?}"
        );
        assert!(英文词串("赋值给 `x` 的值从未被读取").is_empty());
    }

    /// 「未使用变量」告警的「赋值但从未使用」通配段键：动态变量名回填、无英文残留
    #[test]
    fn 通配段赋值未使用() {
        let 表 = 建表(
            "[\"消息翻译\".\"variable `?` is assigned to, but never used\"]\n\"消息模板\" = \"`{q0}` 被赋值但从未使用\"",
        );
        assert_eq!(
            审计消息(
                &表,
                "main",
                "unused_variables",
                "variable `x` is assigned to, but never used"
            )
            .残留,
            Vec::<String>::new(),
            "补键后应无英文残留"
        );
    }

    /// 两端动态：通配段键一次拿齐两个动态名
    #[test]
    fn 通配段两端动态() {
        let 表 = 建表(
            "[\"消息翻译\".\"the trait `?` is not implemented for `?`\"]\n\"消息模板\" = \"特征 `{q0}` 未对 `{q1}` 实现\"",
        );
        assert_eq!(
            渲染(
                &表,
                "the trait `std::fmt::Display` is not implemented for `HashMap<&str, i32>`"
            )
            .as_deref(),
            Some("特征 `std::fmt::Display` 未对 `HashMap<&str, i32>` 实现")
        );
    }

    /// 多通配：非贪婪逐个切分
    #[test]
    fn 通配段三捕获非贪婪() {
        let 表 = 建表(
            "[\"消息翻译\".\"the lifetime `'a` may only live as long as one of `?`'s ? or one of `?`'s ?\"]\n\"消息模板\" = \"`a` 只能与 `{q0}` 或 `{q2}` 的引用同长存活\"",
        );
        let 原文 = "the lifetime `'a` may only live as long as one of `a' and `b' and `c`'s 2 lifetimes or one of `d`'s 2 lifetimes";
        assert_eq!(
            渲染(&表, 原文).as_deref(),
            Some("`a` 只能与 `a' and `b' and `c` 或 `d` 的引用同长存活")
        );
    }

    /// 特异度：通配段键优先于更长的前缀/后缀键
    #[test]
    fn 通配段优先前后缀() {
        let 表 = 建表(
            "[\"消息翻译\".\"the trait `\"]\n\"消息模板\" = \"特征 `{q0}`\"\n\
             [\"消息翻译\".\"~ is not implemented for `\"]\n\"消息模板\" = \"未实现尾巴\"\n\
             [\"消息翻译\".\"the trait `?` is not implemented for `?`\"]\n\"消息模板\" = \"特征 `{q0}` 未对 `{q1}` 实现\"",
        );
        assert_eq!(
            渲染(&表, "the trait `Debug` is not implemented for `Foo`").as_deref(),
            Some("特征 `Debug` 未对 `Foo` 实现")
        );
    }

    /// 多通配段键同时命中时取字面量更长者
    #[test]
    fn 更具体通配键胜出() {
        let 表 = 建表(
            "[\"消息翻译\".\"`?` and `?`\"]\n\"消息模板\" = \"宽松\"\n\
             [\"消息翻译\".\"variants `?` and `?` are never constructed\"]\n\"消息模板\" = \"变体 `{q0}` 与 `{q1}` 从未被构造\"",
        );
        assert_eq!(
            渲染(&表, "variants `Red` and `Green` are never constructed").as_deref(),
            Some("变体 `Red` 与 `Green` 从未被构造")
        );
    }

    /// 无占位符的通配键：整句改写，不粘回英文残段
    #[test]
    fn 无占位通配整句改写() {
        let 表 = 建表(
            "[\"消息翻译\".\"this trait has no implementations, `?` adding `?`\"]\n\"消息模板\" = \"该特征无任何实现\"",
        );
        assert_eq!(
            渲染(&表, "this trait has no implementations, `X` adding `Y`").as_deref(),
            Some("该特征无任何实现")
        );
    }

    /// 精确匹配仍优先于通配段键
    #[test]
    fn 精确优先通配() {
        let 表 = 建表(
            "[\"消息翻译\".\"the trait `?` is not implemented for `?`\"]\n\"消息模板\" = \"通配\"\n\
             [\"消息翻译\".\"the trait `A` is not implemented for `B`\"]\n\"消息模板\" = \"精确\"",
        );
        assert_eq!(
            渲染(&表, "the trait `A` is not implemented for `B`").as_deref(),
            Some("精确")
        );
        assert_eq!(
            渲染(&表, "the trait `C` is not implemented for `D`").as_deref(),
            Some("通配")
        );
    }

    /// 锚点不唯一时必须拒绝
    #[test]
    fn 锚点不唯一拒绝() {
        let 表 = 建表(
            "[\"消息翻译\".\"this function's return type contains a borrowed value, but the signature does not say whether it is borrowed from `?` or `?`\"]\n\"消息模板\" = \"未说明借自 `{q0}` 还是 `{q1}`\"",
        );
        let 原文 = "this function's return type contains a borrowed value, but the signature does not say whether it is borrowed from `a` or `b`";
        assert_eq!(渲染(&表, 原文).as_deref(), Some("未说明借自 `a` 还是 `b`"));
        let 坏 =
            建表("[\"消息翻译\".\"borrowed from `?` or `?`\"]\n\"消息模板\" = \"误抓 `{q0}`\"");
        assert!(坏.按消息查询(原文).is_none());
    }

    /// 字面量不匹配时不得误命中
    #[test]
    fn 字面量失配不误命中() {
        let 表 = 建表(
            "[\"消息翻译\".\"the trait `?` is not implemented for `?`\"]\n\"消息模板\" = \"特征 `{q0}`\"",
        );
        assert!(表.按消息查询("totally different message").is_none());
    }

    /// 捕获为空串时不假装消费
    #[test]
    fn 空捕获回退() {
        let 表 = 建表("[\"消息翻译\".\"a?b?c\"]\n\"消息模板\" = \"{q0}/{q1}\"");
        let (翻译条目, 残段) = 表.按消息查询("abbc").expect("通配段键应命中空捕获");
        let 残段 = 残段.expect("通配段键应携带捕获");
        let (已填充, 已消费) = 填充动态占位(&翻译条目.消息模板, &残段);
        assert_eq!(已填充, "{q0}/{q1}");
        assert!(!已消费);
    }

    /// 既有前缀键行为不变（残段回拼，零回归）
    #[test]
    fn 前缀键行为不变() {
        let 表 = 建表("[\"消息翻译\".\"consider \"]\n\"消息模板\" = \"考虑 \"");
        assert_eq!(
            渲染(&表, "consider making this binding mutable").as_deref(),
            Some("考虑 making this binding mutable")
        );
    }

    /// 既有后缀键行为不变
    #[test]
    fn 后缀键行为不变() {
        let 表 = 建表("[\"消息翻译\".\"~ is never used\"]\n\"消息模板\" = \"`{q0}` 从未被使用\"");
        assert_eq!(
            渲染(&表, "function `helper` is never used").as_deref(),
            Some("`helper` 从未被使用")
        );
    }

    /// 字面量切分正确性
    #[test]
    fn 切分与字面量总长() {
        assert_eq!(分割通配("a?b?c"), vec!["a", "b", "c"]);
        assert_eq!(分割通配("no wildcard"), vec!["no wildcard"]);
        assert_eq!(字面量总长("a?b?c"), 3);
        assert_eq!(字面量总长("中文?"), 2);
    }

    /// 匹配段直接对抗：单/双/三捕获、非贪婪切分、空尾锚、各类失配返回无
    #[test]
    fn 匹配段对抗() {
        assert_eq!(
            匹配段("before `?` after", "before `std::io::Error` after"),
            Some(vec!["std::io::Error"])
        );
        assert_eq!(
            匹配段("`?` and `?`", "`x` and `y` and `z`"),
            Some(vec!["x", "y` and `z"])
        );
        assert_eq!(匹配段("[?][?][?]", "[a][b][c]"), Some(vec!["a", "b", "c"]));
        assert_eq!(
            匹配段("prefix ?", "prefix tail goes to end"),
            Some(vec!["tail goes to end"])
        );
        assert_eq!(匹配段("? suffix", "x suffix"), None);
        assert_eq!(匹配段("abc?def", "abXdef"), None);
        assert_eq!(匹配段("abc?def", "abcdefXX"), None);
        assert_eq!(匹配段("a?x?b", "ayyb"), None);
        assert_eq!(匹配段("abc?def", "ab"), None);
        assert_eq!(匹配段("a?b", "ab"), Some(vec![""]));
    }

    /// 英文词串阈值与切分对抗
    #[test]
    fn 英文词串阈值() {
        assert!(英文词串("").is_empty());
        assert!(英文词串("``, ，。").is_empty());
        assert!(英文词串("one two three").is_empty());
        assert_eq!(英文词串("one two three four"), vec!["one two three four"]);
        assert_eq!(
            英文词串("aaa bbb 123 ccc ddd eee fff"),
            vec!["ccc ddd eee fff"]
        );
        assert_eq!(英文词串("foo1 bar2 baz3 qux4"), vec!["foo1 bar2 baz3 qux4"]);
        assert_eq!(
            英文词串("word `a` word2, word3 word4"),
            vec!["word a word2 word3 word4"]
        );
        assert_eq!(
            英文词串("let x 赋值 alpha beta gamma"),
            vec!["let x alpha beta gamma"]
        );
        assert!(英文词串("let x = 1;").is_empty());
    }
}
