// 诊断翻译器：结合错误消息翻译表与类型映射，将 rustc 的英文诊断
// 转化为面向母语学习者的教学诊断信息。

use std::collections::HashMap;

use super::教学::{抽取所有权详情, 教学诊断, 诊断位置, 诊断级别};
use super::消息翻译::{填充动态占位符, 错误翻译管理器};
use super::诊断模型::编译器诊断;
use crate::映射管理::映射管理器;

/// 诊断翻译器：将 rustc 诊断翻译为教学诊断
///
/// 结合错误消息翻译表和类型映射，将 rustc 的英文诊断转化为
/// 面向中文学习者的教学诊断信息。
pub struct 诊断翻译器 {
    翻译管理器: 错误翻译管理器,
    类型映射表: HashMap<String, String>, // 来自关键字映射表，用于替换消息中的英文类型
}

/// 对带模块路径的类型名做最长后缀匹配
///
/// `std::fmt::Display` 逐段尝试（fmt::Display → Display），命中映射段后
/// 保留原路径前缀，仅替换命中段：`std::fmt::Display` → `std::fmt::显示`。
pub(crate) fn 替换类型段(
    标记: &str, 类型映射表: &HashMap<String, String>
) -> Option<String> {
    let 段列表: Vec<&str> = 标记.split("::").collect();
    for 序号 in 1..段列表.len() {
        let 后缀 = 段列表[序号..].join("::");
        if let Some(母语) = 类型映射表.get(&后缀) {
            let mut 转换结果 = 段列表[..序号].join("::");
            if !转换结果.is_empty() {
                转换结果.push_str("::");
            }
            转换结果.push_str(母语);
            return Some(转换结果);
        }
    }
    None
}

/// 本地化带模块路径的类型名（三段式）
///
/// 1. 类型后缀段映射：`std::fmt::Display` → `std::fmt::可显示`
/// 2. 路径前缀完整匹配（从长到短）：`std::fmt` → `标准库::格式化`（或 `std` → `标准库`）
/// 3. 剩余中间段单段映射：`fmt` → `格式化`
///    最终：`std::fmt::Display` → `标准库::格式化::可显示`；
///    仅含路径（如 `std::io::Error`）未命中任何段时返回 None 保持原样。
pub(crate) fn 本地化类型段(标记: &str, 映射: &HashMap<String, String>) -> Option<String> {
    let 原文 = 标记.to_string();
    // 1. 类型后缀段映射
    let 转换结果 = 替换类型段(标记, 映射).unwrap_or_else(|| 标记.to_string());
    // 2. 路径前缀完整匹配（从长到短）
    let mut 段列表: Vec<String> = 转换结果.split("::").map(|s| s.to_string()).collect();
    if 段列表.len() >= 2 {
        for 序号 in (1..段列表.len()).rev() {
            let 前缀 = 段列表[..序号].join("::");
            if let Some(母语) = 映射.get(&前缀) {
                let 其余 = 段列表[序号..].join("::");
                段列表 = vec![母语.clone()];
                段列表.extend(其余.split("::").map(|s| s.to_string()));
                break;
            }
        }
        // 3. 剩余中间段单段映射（跳过首尾，避免误伤已替换的路径与类型段）
        for 序号 in 1..段列表.len().saturating_sub(1) {
            if let Some(母语) = 映射.get(&段列表[序号]) {
                段列表[序号] = 母语.clone();
            }
        }
    }
    let 转换结果 = 段列表.join("::");
    (转换结果 != 原文).then_some(转换结果)
}

/// 统计类型字符串开头的引用层数（`&` 前缀个数）
///
/// 规则：连续 strip `&` 前缀，遇 `mut`（含 `mut ` / `mut` 结尾）即停止
/// —— `&mut T` 是单层可变引用，不算两层。
/// 示例：`&String` → 1，`&&String` → 2，`&mut String` → 1，`String` → 0。
pub(crate) fn 计数引用前缀(类型串: &str) -> usize {
    let mut 剩余 = 类型串;
    let mut 计数 = 0;
    while let Some(之后) = 剩余.strip_prefix('&') {
        计数 += 1;
        if 之后.starts_with("mut ") || 之后 == "mut" || 之后.starts_with("mut\t") {
            break;
        }
        剩余 = 之后;
    }
    计数
}

/// 本地化引用类型：先整体查 type_map（如 `&str` → `字符串引用`），
/// 未命中时拆开 `&` 前缀，对本体做类型本地化后拼回
/// （如 `&String` → `&字符串`、`&&i32` → `&&整数`）。
pub(crate) fn 本地化引用类型(类型串: &str, 映射: &HashMap<String, String>) -> String {
    if let Some(母语) = 映射.get(类型串) {
        return 母语.clone();
    }
    let mut 前缀 = String::new();
    let mut 剩余 = 类型串;
    while let Some(之后) = 剩余.strip_prefix('&') {
        前缀.push('&');
        剩余 = 之后;
    }
    if 前缀.is_empty() {
        return 类型串.to_string();
    }
    let (可变部分, 本体) = match 剩余.strip_prefix("mut ") {
        Some(本体) => ("mut ", 本体),
        None => ("", 剩余),
    };
    let 本体母语 = 映射
        .get(本体)
        .cloned()
        .or_else(|| 本地化类型段(本体, 映射))
        .unwrap_or_else(|| 本体.to_string());
    format!("{前缀}{可变部分}{本体母语}")
}

/// 提取 rustc "if you wanted to use a crate named `X`" 建议中的 crate 名
fn 建议包名(消息: &str) -> Option<&str> {
    消息
        .strip_prefix("if you wanted to use a crate named `")
        .and_then(|剩余| 剩余.split('`').next())
        .filter(|名字| !名字.is_empty())
}

/// 从 `expected `X`, found `Y`` 形式的文本（rustc label）中提取期望/实际类型
fn 抽取期望实际(内容: &str) -> Option<(String, String)> {
    let 位置 = 内容.find("expected ")?;
    let 之后 = &内容[位置 + "expected ".len()..];
    let 逗号位置 = 之后.find(", found ")?;
    let 期望 = 之后[..逗号位置].trim().trim_matches('`').to_string();
    let 实际 = 之后[逗号位置 + ", found ".len()..]
        .trim()
        .trim_matches('`')
        .to_string();
    Some((期望, 实际))
}

/// 整词替换：仅当目标前后字符均非标识符字符（字母/数字/下划线）时替换，
/// 避免裸词模式（如 integer）误伤 to_integer/integer_count 等标识符子串
pub(crate) fn 整词替换(内容: &str, 来源: &str, 目标: &str) -> String {
    let 是标识符字符 = |c: char| c.is_alphanumeric() || c == '_';
    let mut 转换结果 = String::with_capacity(内容.len());
    let mut 剩余 = 内容;
    while let Some(位置) = 剩余.find(来源) {
        let 结束 = 位置 + 来源.len();
        let 前合规 = 剩余[..位置]
            .chars()
            .next_back()
            .is_none_or(|c| !是标识符字符(c));
        let 后合规 = 剩余[结束..].chars().next().is_none_or(|c| !是标识符字符(c));
        转换结果.push_str(&剩余[..位置]);
        转换结果.push_str(if 前合规 && 后合规 {
            目标
        } else {
            来源
        });
        剩余 = &剩余[结束..];
    }
    转换结果.push_str(剩余);
    转换结果
}

/// 提取文本中所有反引号包裹的内容（按出现顺序）
fn 抽取反引号段(内容: &str) -> Vec<String> {
    let mut 段列表 = Vec::new();
    let mut 剩余 = 内容;
    while let Some(起点) = 剩余.find('`') {
        let 之后 = &剩余[起点 + 1..];
        match 之后.find('`') {
            Some(终点) => {
                段列表.push(之后[..终点].to_string());
                剩余 = &之后[终点 + 1..];
            }
            None => break,
        }
    }
    段列表
}

/// 从诊断消息中提取前两个反引号包裹的类型
///
/// rustc 1.97+ 的算术错误不再输出 label，类型信息嵌入 message，
/// 如 E0369：`cannot add `{integer}` to `&str``。
fn 从消息抽取类型(消息: &str) -> Option<(String, String)> {
    let 段列表 = 抽取反引号段(消息);
    if 段列表.len() >= 2 {
        Some((段列表[0].clone(), 段列表[1].clone()))
    } else {
        None
    }
}

/// 由映射管理器构建诊断类型反查表（英文类型名 → 母语名），CLI 与 LSP 共用。
///
/// 数据来源与优先级（与历史 CLI 内联逻辑完全一致，抽出以消除两处漂移）：
/// keywords `[类型]` 节反转为主，stdlib/别名表仅补充缺失条目（不覆盖），
/// 模块路径映射（std → 标准库 等）覆盖同名第三方条目以稳定标准库路径翻译。
/// 仅收录「母语键」（含非 ASCII 字符）的反向项，避免把 `format`→`fmt` 之类
/// 纯英文修正项引入诊断译文（会让译文出现英文值）。
pub fn 构建类型映射(管理器: &映射管理器) -> HashMap<String, String> {
    let 是母语键 = |键: &str| {
        !键.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    let mut 反向映射: HashMap<String, String> = 管理器
        .取节映射("类型")
        .map(|节| {
            节.iter()
                .filter(|(键, _)| 是母语键(键))
                .map(|(键, values)| (values.clone(), 键.clone()))
                .collect()
        })
        .unwrap_or_default();
    for (母语, 英文) in 管理器.取别名映射表() {
        if 是母语键(母语) {
            反向映射.entry(英文.clone()).or_insert_with(|| 母语.clone());
        }
    }
    for (母语, 英文) in 管理器.取模块路径映射表() {
        if 是母语键(母语) {
            反向映射.insert(英文.clone(), 母语.clone());
        }
    }
    反向映射
}

/// 错误码模板（`[E0xxx]` 节）支持的动态占位符：按此顺序从消息原文反引号
/// 内容中依次回填。顺序须与各模板内占位符的出现先后相容（如 E0609 的
/// `{类型}` 先于 `{字段名}`、E0027 的 `{结构名}` 先于 `{字段名}`）；
/// 模板不含某占位符时不消费 token，故跨模板共用一份列表不会串位。
/// `{期望}`/`{实际}` 另有专门的 expected/found 提取逻辑，不在此列。
const 码模板占位符: &[&str] = &[
    "{变量名}",
    "{名称}",
    "{类型}",
    "{特征}",
    "{模块名}",
    "{字段名}",
    "{方法名}",
    "{函数名}",
    "{结构名}",
    "{关联类型}",
    "{特征名}",
    "{生命周期}",
];

/// [`诊断翻译器::渲染主消息`] 的返回：主消息译文与其教学提示。
#[derive(Debug, Clone)]
pub struct 渲染消息 {
    /// 已回填占位符并完成类型中文化的主消息译文
    pub 主消息文本: String,
    /// 命中条目的教学提示（若有），供调用方按需追加
    pub 教学提示: Option<String>,
}

impl 诊断翻译器 {
    /// 创建诊断翻译器
    pub fn 新建翻译器(
        翻译管理器: 错误翻译管理器,
        类型映射表: HashMap<String, String>,
    ) -> Self {
        Self {
            翻译管理器,
            类型映射表,
        }
    }

    /// 仅凭 (错误码, 消息原文, 主 span 标签) 渲染主消息——CLI/LSP 同口径共享入口。
    ///
    /// 渲染链与 [`诊断翻译器::翻译诊断`] 的主消息分支一致：
    /// 错误码表优先（完整桩/静态模板）→ 消息表（精确/前缀/后缀/通配段 + `{qN}`
    /// 回填）→ 占位符按序回填 → `类型映射表` 类型中文化。区别在于不依赖完整
    /// `编译器诊断`：期望/实际取自 `主要标签`（rustc 主 span 标签），
    /// 其次从消息原文 `expected X, found Y`/前两个反引号类型提取；
    /// `{变量名}`/`{名称}`/`{类型}`/`{特征}` 按序取消息反引号内容。
    ///
    /// 安全兵：错误码模板存在无法回填的占位符时（如 rust-analyzer 把
    /// `expected/found` 放在 relatedInformation 而非主消息的 E0308）
    /// **回退消息表**而非输出英文或裸占位符——保证译文不劣于旧行为。
    /// 命中任一表返回 `Some`，两表皆未命中返回 `None`（调用方走轻量兵底）。
    pub fn 渲染主消息(
        &self,
        码: Option<&str>,
        消息: &str,
        主要标签: Option<&str>,
    ) -> Option<渲染消息> {
        let 码条目 = 码
            .filter(|c| !c.is_empty())
            .and_then(|c| self.翻译管理器.码查询(c));
        let 消息查询 = self.翻译管理器.按消息查询(消息);

        // 1. 错误码表优先（与 CLI `码条目.or_else(消息查询)` 对齐）
        if let Some(条目) = 码条目 {
            let mut 模板 = 条目.消息模板.clone();
            let 期望实际 = 主要标签
                .and_then(抽取期望实际)
                .or_else(|| 抽取期望实际(消息))
                .or_else(|| 从消息抽取类型(消息));
            if let Some((期望, 实际)) = 期望实际
                && !期望.is_empty()
                && !实际.is_empty()
            {
                模板 = 模板.replace("{期望}", &期望).replace("{实际}", &实际);
            }
            // 其余占位符按出现顺序从消息反引号内容回退填充
            let mut 段迭代器 = 抽取反引号段(消息).into_iter();
            for 占位符 in 码模板占位符 {
                if 模板.contains(占位符) {
                    match 段迭代器.next() {
                        Some(标记) => 模板 = 模板.replace(占位符, &标记),
                        None => break,
                    }
                }
            }
            // 仍有未回填占位符 → 码表桩无法从可用信息渲染 → 回退消息表（不出英文/裸占位符）
            let 未解决 = ["{期望}", "{实际}"]
                .iter()
                .chain(码模板占位符.iter())
                .any(|p| 模板.contains(p));
            if !未解决 {
                return Some(渲染消息 {
                    主消息文本: self.替换类型名(模板),
                    教学提示: 条目.教学提示.clone(),
                });
            }
        }

        // 2. 消息表（精确/前缀/后缀/通配段）+ `{qN}` 回填，额外补 类型映射表 类型中文化
        if let Some((条目, 残段)) = 消息查询 {
            let mut 消息文本 = 条目.消息模板.clone();
            if let Some(残段) = 残段 {
                let (已填充, 已消费) = 填充动态占位符(&消息文本, &残段);
                消息文本 = 已填充;
                if !已消费 {
                    消息文本.push_str(残段.文字());
                }
            }
            return Some(渲染消息 {
                主消息文本: self.替换类型名(消息文本),
                教学提示: 条目.教学提示.clone(),
            });
        }

        None
    }

    /// 翻译单条诊断信息
    pub fn 翻译诊断(&self, 诊断: &编译器诊断) -> 教学诊断 {
        let 错误码 = 诊断.诊断码.as_ref().map(|c| c.码值.clone());

        let 码条目 = 错误码.as_deref().and_then(|码| self.翻译管理器.码查询(码));
        // 无错误码或错误码未收录时，按消息原文匹配（[消息翻译] 节）；
        // 消息表查询所得保留匹配来源（前缀/后缀键），供动态部分回填
        let 消息查询 = self.翻译管理器.按消息查询(&诊断.诊断消息);
        let 命中条目 = 码条目.or_else(|| 消息查询.map(|(条目, _)| 条目));

        // 从主要 span 的 label 中提取 expected 和 found；
        // rustc 1.97+ 的算术错误（E0369 等）不再输出 label，类型信息
        // 直接嵌入 message（如 `cannot add `{integer}` to `&str``），
        // 因此提取失败时回退从 message 中解析反引号包裹的类型。
        let 主要标签 = 诊断
            .跨度列表
            .iter()
            .find(|s| s.是主跨度)
            .and_then(|s| s.标签.as_deref());

        let (期望, 实际) = 主要标签
            .and_then(抽取期望实际)
            .or_else(|| 从消息抽取类型(&诊断.诊断消息))
            .unwrap_or_default();

        // 构建翻译消息
        let 翻译消息 = if let Some(条目) = 命中条目 {
            let mut 模板 = 条目.消息模板.clone();
            if !期望.is_empty() && !实际.is_empty() {
                模板 = 模板.replace("{期望}", &期望).replace("{实际}", &实际);
            } else if 模板.contains("{期望}") || 模板.contains("{实际}") {
                // 无法提取期望/实际类型时回退 rustc 原文，避免输出裸占位符
                模板 = 诊断.诊断消息.clone();
            }
            // 动态部分回填仅对消息表条目生效（前缀/后缀键匹配时保留动态名，
            // 如 "did you mean " → "你是否想用 `foo`?"）；错误码条目是完整桩，
            // 回拼会在中文模板后粘上英文残段（如 E0624 的
            // "字段或方法是私有的year` is private"）。错误码条目需要展示动态
            // 名称时用 {名称} 等占位符（从消息反引号内容回退填充，见下方）。
            if 码条目.is_none()
                && let Some((_, Some(残段))) = 消息查询
            {
                let (已填充, 已消费) = 填充动态占位符(&模板, &残段);
                模板 = 已填充;
                if !已消费 {
                    模板.push_str(残段.文字());
                }
            }
            // 对模板中的类型名进行中文化替换
            self.替换类型名(模板)
        } else {
            self.替换类型名(诊断.诊断消息.clone())
        };

        let mut 教学提示列表 = Vec::new();
        if let Some(条目) = 命中条目
            && let Some(提示) = &条目.教学提示
        {
            教学提示列表.push(提示.clone());
        }
        for 子诊断 in &诊断.子诊断 {
            if 子诊断.诊断级别 == "help" {
                // "if you wanted to use a crate named `X`" 建议：crate 名规则不允许
                // 非 ASCII，中文名实为拼写错误的母语模块，建议 `cargo add 中文名`
                // 必然无效（如 `add 规则类型`）；静默丢弃，避免误导初学者。
                if 建议包名(&子诊断.诊断消息).is_some_and(|名字| !名字.is_ascii()) {
                    continue;
                }
                // help 短语优先查消息表翻译（前缀匹配保留动态后缀），未命中保留原文
                let 提示 = self
                    .翻译管理器
                    .按消息查询(&子诊断.诊断消息)
                    .map(|(条目, 残段)| {
                        let mut 消息文本 = 条目.消息模板.clone();
                        if let Some(残段) = 残段 {
                            // 模板含 {q0}/{q1} 捕获占位符时，从动态部分提取引号内容填充
                            //（如 "trait `Datelike` which provides `year` is never used" →
                            //  "特征 `Datelike` 从未被使用"）；
                            // 捕获完整覆盖时不再追加英文残段。
                            let (已填充, 已消费) = 填充动态占位符(&消息文本, &残段);
                            消息文本 = 已填充;
                            if !已消费 {
                                消息文本.push_str(残段.文字());
                            }
                        }
                        消息文本
                    })
                    .unwrap_or_else(|| 子诊断.诊断消息.clone());
                教学提示列表.push(crate::语言::查译("diag_fix_suggestion", &[&提示]));
            }
        }

        // E0308 引用层数教学提示：期望/实际类型均为引用或其中之一为引用、
        // 但引用层数不同（如 `expected `&String`, found `String``）时，
        // 追加一条检查 `&` 数量的提示，覆盖初学者最常见的借用错误之一。
        if 错误码.as_deref() == Some("E0308") && !期望.is_empty() && !实际.is_empty() {
            let 期望引用数 = 计数引用前缀(&期望);
            let 实际引用数 = 计数引用前缀(&实际);
            if 期望引用数 != 实际引用数 && (期望引用数 > 0 || 实际引用数 > 0) {
                教学提示列表.push(crate::语言::查译(
                    "diag_ref_depth_hint",
                    &[
                        &本地化引用类型(&期望, &self.类型映射表),
                        &本地化引用类型(&实际, &self.类型映射表),
                    ],
                ));
            }
        }

        let 位置列表 = 诊断.跨度列表.iter().map(诊断位置::从跨度构造).collect();

        // 所有权错误：提取叙事化详情（变量名、移动/借用、再次使用位置）
        let 所有权详情 = 错误码.as_deref().and_then(|码| 抽取所有权详情(码, 诊断));

        // 翻译消息中含 {变量名} 占位符时，用提取到的变量名填充
        let mut 翻译消息 = 翻译消息;
        if let Some(详情) = &所有权详情 {
            翻译消息 = 翻译消息.replace("{变量名}", &详情.变量名);
        }

        // 其余占位符（{变量名}/{名称}/{类型}/{特征}/{模块名}/{字段名} 等）
        // 按出现顺序从消息反引号内容回退填充，覆盖无 label 的错误
        //（E0384 重复赋值、E0433 未找到类型、E0583 模块文件缺失等）；
        // 提取不到时回退 rustc 原文，避免输出裸占位符。
        let mut 段迭代器 = 抽取反引号段(&诊断.诊断消息).into_iter();
        for 占位符 in 码模板占位符 {
            if 翻译消息.contains(占位符) {
                match 段迭代器.next() {
                    Some(标记) => {
                        翻译消息 = 翻译消息.replace(占位符, &标记);
                    }
                    None => {
                        翻译消息 = 诊断.诊断消息.clone();
                        break;
                    }
                }
            }
        }
        // 占位符填充的才是运行时实际类型名，需再次中文化
        //（如 E0277 的 `std::fmt::Display` → `std::fmt::显示`、`{integer}` → `整数`）
        翻译消息 = self.替换类型名(翻译消息);

        let 子诊断列表 = 诊断.子诊断.iter().map(|子| self.翻译诊断(子)).collect();

        教学诊断 {
            严重级别: 诊断级别::从文本解析(&诊断.诊断级别),
            错误码,
            翻译消息,
            原始消息: 诊断.诊断消息.clone(),
            教学提示: 教学提示列表,
            位置列表,
            子诊断: 子诊断列表,
            所有权详情,
        }
    }

    /// 使用类型映射替换消息中的英文类型名
    fn 替换类型名(&self, 消息: String) -> String {
        let mut 转换结果 = 消息;
        // rustc 未推断字面量占位符（`{integer}`/`{float}`）及 1.97+ 裸显示名
        // （`integer`/`floating-point number`）按全局语言翻译；
        // 裸 integer 用整词匹配，避免误伤 to_integer/integer_count 等标识符
        转换结果 = 转换结果
            .replace("{integer}", &crate::语言::查句("diag_rustc_integer"))
            .replace("{float}", &crate::语言::查句("diag_rustc_float"))
            .replace(
                "floating-point number",
                &crate::语言::查句("diag_rustc_float"),
            );
        转换结果 = 整词替换(
            &转换结果,
            "integer",
            &crate::语言::查句("diag_rustc_integer"),
        );
        // 类型映射：仅替换反引号包裹的完整类型名（rustc 诊断中的类型均在反引号内），
        // 避免 "str"→"文本" 等短条目把消息中的 "string" 部分替换成 "文本ing"
        for 标记 in 抽取反引号段(&转换结果) {
            if let Some(母语) = self.类型映射表.get(&标记) {
                转换结果 = 转换结果.replace(&format!("`{}`", 标记), &format!("`{}`", 母语));
            } else if let Some(已替换) = 本地化类型段(&标记, &self.类型映射表) {
                // 带模块路径的类型名：三段式本地化
                //（std::fmt::Display → 标准库::格式化::可显示）
                转换结果 = 转换结果.replace(&format!("`{}`", 标记), &format!("`{}`", 已替换));
            }
        }
        转换结果
    }

    /// 批量翻译诊断列表
    pub fn 批量翻译(&self, 诊断列表: &[编译器诊断]) -> Vec<教学诊断> {
        诊断列表.iter().map(|d| self.翻译诊断(d)).collect()
    }
}
