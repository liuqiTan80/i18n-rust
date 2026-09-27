//! 错误消息翻译结构：从语言包 errors.toml 加载错误码表与消息表。

use crate::error::{LoadError, LoadTarget};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// 错误消息翻译条目（从 errors.toml 加载）
#[derive(Deserialize, Debug, Clone)]
pub struct ErrorMessageEntry {
    #[serde(rename = "消息模板")]
    pub message_template: String,
    #[serde(rename = "教学提示")]
    pub teaching_hint: Option<String>,
}

/// 消息键的通配符：`?` 表示一段动态内容（非贪婪，按后续字面量定位）
///
/// 带通配的键称「通配段键」，如 ``the trait `?` is not implemented for `?` ``：
/// 一次匹配即可拿到中间与末尾两个动态名（{q0}/{q1}），解决「两端动态」
/// 的消息无法用单一前缀/后缀键完整翻译的问题。
pub const WILDCARD: char = '?';

/// 消息表匹配的动态部分来源
///
/// 前缀键与后缀键命中的 rest 形态不同，占位符提取方式也不同：
/// - 前缀键（如 "trait `"）：rest 是键之后的**尾段**，开引号已被键
///   剥离（"Datelike` which provides `year` is never used"）；
/// - 后缀键（键以 `~` 开头，如 "~ is never used"）：rest 是键之前的
///   **头段**，引号对完整（"variants `黄灯` and `绿灯` "）。
#[derive(Debug, Clone, Copy)]
pub enum MessageRest<'b> {
    /// 前缀键命中的尾段
    Tail(&'b str),
    /// 后缀键命中的头段
    Head(&'b str),
    /// 通配段键命中的逐个捕获（第 n 个 `?` 之间的原文）
    Parts(&'b [&'b str]),
}

impl<'b> MessageRest<'b> {
    /// 未翻译原文残段（模板无占位符命中时按原文拼接到模板后）
    ///
    /// 通配段键不适用：其动态部分已被逐个 `{qN}` 完整接管，残段为空
    /// （拼接会把已捕获的英文名再粘一遍）。
    pub fn text(&self) -> &'b str {
        match self {
            Self::Tail(s) | Self::Head(s) => s,
            Self::Parts(_) => "",
        }
    }
}

/// 错误消息翻译管理器：按错误码或消息文本查询翻译条目
#[derive(Debug, Clone)]
pub struct ErrorTranslationManager {
    /// 错误码表：错误码 → 翻译条目（errors.toml 顶层 [E0xxx] 节）
    pub translation_table: HashMap<String, ErrorMessageEntry>,
    /// 消息表：英文消息原文 → 翻译条目（errors.toml [消息翻译] 节）
    ///
    /// 覆盖无错误码的 rustc 消息（如 format 参数检查）与常见 help 短语；
    /// 支持精确匹配与最长前缀匹配（如 "did you mean " 可保留动态后缀）。
    message_map: HashMap<String, ErrorMessageEntry>,
}

impl ErrorTranslationManager {
    /// 从文件加载错误翻译表
    pub fn load_from_file(path: &Path) -> Result<Self, LoadError> {
        let content = fs::read_to_string(path).map_err(|e| LoadError::ReadFailed {
            target: LoadTarget::ErrorMessages,
            path: Some(path.display().to_string()),
            detail: e.to_string(),
        })?;
        Self::load_from_string(&content)
    }

    /// 从 TOML 字符串加载错误翻译表
    ///
    /// 顶层表分为两类：`[E0xxx]` 等错误码表（含 "消息模板"/"教学提示"）
    /// 与 `[消息翻译]` 消息表（键为英文消息原文，同样含模板与提示）。
    pub fn load_from_string(content: &str) -> Result<Self, LoadError> {
        let value: toml::Value = toml::from_str(content).map_err(|e| LoadError::ParseFailed {
            target: LoadTarget::ErrorMessages,
            path: None,
            detail: e.to_string(),
        })?;
        let mut translation_table = HashMap::new();
        let mut message_map = HashMap::new();
        if let Some(table) = value.as_table() {
            for (key, val) in table {
                if key == "消息翻译" {
                    if let Some(entries) = val.as_table() {
                        for (msg, entry) in entries {
                            message_map.insert(msg.clone(), entry_from_value(entry));
                        }
                    }
                } else {
                    translation_table.insert(key.clone(), entry_from_value(val));
                }
            }
        }
        Ok(Self {
            translation_table,
            message_map,
        })
    }

    /// 按错误码查询翻译条目
    pub fn query(&self, error_code: &str) -> Option<&ErrorMessageEntry> {
        self.translation_table.get(error_code)
    }

    /// 按消息原文查询翻译条目
    ///
    /// 匹配顺序：精确匹配 → 最长通配段键 → 最长前缀匹配 → 最长后缀匹配。
    /// 返回未匹配的动态部分（前缀匹配为尾段原文，后缀匹配为头段原文，
    /// 通配段匹配为逐个 `?` 的捕获，见 [`MessageRest`]），供调用方填充模板
    /// 占位符或拼接到模板后，保留 `did you mean \`x\``、`function \`foo\` is never
    /// used` 等动态内容。
    /// 后缀键以 `~` 开头（如 `~ is never used`），解决动态名位于消息中间的
    /// lint 警告（dead_code/non_snake_case 族）无法用前缀键覆盖的问题；
    /// 通配段键（含 `?`，如 ``the trait `?` is not implemented for `?` ``）则解决
    /// **两端以上动态**的消息，单一头/尾段捕获无法覆盖的情形。
    pub fn query_by_message<'a, 'b>(
        &'a self,
        message: &'b str,
    ) -> Option<(&'a ErrorMessageEntry, Option<MessageRest<'b>>)> {
        // 1. 精确匹配
        if let Some(entry) = self.message_map.get(message) {
            return Some((entry, None));
        }
        // 2. 通配段键（字面量总长最长者胜：越具体越优先）
        let mut best_segment: Option<(usize, Vec<&'b str>, &'a ErrorMessageEntry)> = None;
        // 3. 最长前缀 / 最长后缀候选（前缀优先，其模板通常更完整、含类型词）
        let mut best_prefix: Option<(usize, &'b str, &'a ErrorMessageEntry)> = None;
        let mut best_suffix: Option<(usize, &'b str, &'a ErrorMessageEntry)> = None;
        for (key, entry) in &self.message_map {
            if let Some(suffix_key) = key.strip_prefix('~') {
                if let Some(prefix) = message.strip_suffix(suffix_key)
                    && best_suffix.is_none_or(|(len, _, _)| suffix_key.len() > len)
                {
                    best_suffix = Some((suffix_key.len(), prefix, entry));
                }
            } else if key.contains(WILDCARD) {
                if let Some(captures) = match_segments(key, message)
                    && best_segment
                        .as_ref()
                        .is_none_or(|(len, _, _)| segment_literals_len(key) > *len)
                {
                    best_segment = Some((segment_literals_len(key), captures, entry));
                }
            } else if message.starts_with(key.as_str())
                && best_prefix.is_none_or(|(len, _, _)| key.len() > len)
            {
                best_prefix = Some((key.len(), &message[key.len()..], entry));
            }
        }
        if let Some((_, captures, entry)) = best_segment {
            // `captures` 里的切片虽借自 `message`（'b），但作为本地 Vec 的元素无法
            // 直接随返回值活过本函数：只在模板确实引用占位符时把它们提升为
            // 'static（泄漏量级 = 命中词数 × 捕获数，与错误码表加载同模式），
            // 未引用时直接丢弃，不做无谓分配。
            let rest = if entry.message_template.contains("{q") {
                let owned: Box<[&'static str]> = captures
                    .into_iter()
                    .map(|c| -> &'static str {
                        let b: &'static mut str = Box::leak(c.to_owned().into_boxed_str());
                        &*b
                    })
                    .collect();
                // 整体泄漏为 &'static [_]（不能借本地 `owned` 返回），再收缩到 'b
                MessageRest::Parts(Box::leak(owned))
            } else {
                MessageRest::Parts(&[])
            };
            return Some((entry, Some(rest)));
        }
        if let Some((_, rest, entry)) = best_prefix {
            return Some((entry, Some(MessageRest::Tail(rest))));
        }
        best_suffix.map(|(_, prefix, entry)| (entry, Some(MessageRest::Head(prefix))))
    }

    /// 已覆盖的错误码数量
    pub fn coverage_count(&self) -> usize {
        self.translation_table.len()
    }
}

/// 将通配段键拆为字面量段（以 `?` 为分界）
///
/// 例：``"the trait `?` is not implemented for `?`"`` → ["the trait `", "` is not
/// implemented for `", "`"]（n 个通配对应 n+1 个字面量段）。
fn split_wildcard(key: &str) -> Vec<&str> {
    key.split(WILDCARD).collect()
}

/// 通配段键的字面量总长度（多键命中同一消息时的特异度排序依据）
fn segment_literals_len(key: &str) -> usize {
    split_wildcard(key).iter().map(|s| s.chars().count()).sum()
}

/// 用通配段键匹配消息，成功返回每个 `?` 捕获的原文（按出现顺序）
///
/// 语义为**非贪婪**：每个字面量段从上一段之后首次出现的位置开始找，
/// 因此同一消息内多个同名占位能逐个切分（如 借自 `a` 还是 `b`）。
/// 要求消息以首段开头、以尾段结尾，且各段依次不重叠出现。
fn match_segments<'b>(key: &str, message: &'b str) -> Option<Vec<&'b str>> {
    let literals = split_wildcard(key);
    let (first, rest_literals) = literals.split_first()?;
    let tail = rest_literals.last()?;
    let body = message.strip_prefix(first)?;
    if !body.ends_with(tail) || rest_literals.len() < 2 {
        return None;
    }
    let mut captures = Vec::with_capacity(rest_literals.len());
    let mut remain = body;
    for lit in &rest_literals[..rest_literals.len() - 1] {
        let at = remain.find(lit)?;
        captures.push(&remain[..at]);
        remain = &remain[at + lit.len()..];
    }
    captures.push(remain.strip_suffix(tail)?);
    Some(captures)
}

/// 用动态部分（前缀/后缀键命中的原文残段，或通配段键的逐个捕获）填充模板的 {q0}/{q1}
///
/// 通配段场景（Parts）：`{qN}` 直接取第 N 个 `?` 捕获的原文（不再做引号启发式切分）。
///
/// 反引号场景（dead_code 等）：
/// - 头段（后缀键，引号对完整）：{q0} 取第一个引号对内容
///   （"variants `黄灯` and `绿灯` " → "黄灯"），{q1} 取最后一个引号对内容
///   （rsplit）；引号数不足时回退 rsplit（兼容奇数引号的残段）；
/// - 尾段（前缀键，开引号已被键剥离）：{q0} 取第一个反引号前的内容
///   （"foo` is never used" → "foo"、"Datelike` which provides `year` is
///   never used" → "Datelike"）；残段以反引号开头时（键未含开引号）
///   退化取引号对内容；{q1} 取最后一个引号对内容（rsplit）。
///
/// 单引号场景（Unicode 混淆 help）按段隔取：头段取第 1/3 段，
/// 尾段取第 0/2 段（开引号已剥离，段 0 即第一个字符）。
///
/// 返回 (填充后的模板, 是否发生了填充)；捕获不足时原样返回模板，
/// 由调用方决定是否拼接动态原文，避免中英文混排或信息凭空丢失。
pub fn fill_dynamic_placeholders(template: &str, rest: &MessageRest<'_>) -> (String, bool) {
    let mut result = template.to_string();
    let mut consumed_any = false;
    // 占位符编号上界：前缀/后缀键的启发式只区分 q0/q1；q2 仅对通配段键
    //（捕获逐个对应）有意义，如 E0106 的三个动态名。
    for i in 0..3 {
        let placeholder = format!("{{q{i}}}");
        if result.contains(&placeholder) {
            let content = match rest {
                MessageRest::Parts(captures) => captures.get(i).copied(),
                MessageRest::Head(dynamic) => {
                    if dynamic.contains('`') {
                        if i == 1 || dynamic.matches('`').count() == 1 {
                            dynamic.rsplit('`').nth(1)
                        } else {
                            dynamic.split('`').nth(1)
                        }
                    } else {
                        dynamic.split('\'').nth(i * 2 + 1)
                    }
                }
                MessageRest::Tail(dynamic) => {
                    if dynamic.contains('`') {
                        if i == 1 {
                            dynamic.rsplit('`').nth(1)
                        } else if let Some(after) = dynamic.strip_prefix('`') {
                            // 键未含开引号：残段以引号对开始，取引号对内容
                            after.split('`').next()
                        } else {
                            dynamic.split('`').next()
                        }
                    } else {
                        dynamic.split('\'').nth(i * 2)
                    }
                }
            };
            match content {
                Some(content) if !content.is_empty() => {
                    result = result.replace(&placeholder, content);
                    consumed_any = true;
                }
                // 捕获为空或失败：原样回退，交由调用方拼接原文
                _ => return (template.to_string(), false),
            }
        }
    }
    (result, consumed_any)
}

/// 从 TOML 值构建翻译条目（消息模板缺失时回退空串，避免解析失败丢失整表）
fn entry_from_value(val: &toml::Value) -> ErrorMessageEntry {
    ErrorMessageEntry {
        message_template: val
            .get("消息模板")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        teaching_hint: val
            .get("教学提示")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 从 `[["消息翻译"."键"]]` 点号表头片段构建消息表（测试辅助）
    ///
    /// 不包 `[消息翻译]` 头：中文裸键在 TOML 中非法（裸键仅允许 ASCII），
    /// 真实 errors.toml 亦采用 `["消息翻译"."键"]` 引号点号写法。
    fn manager(toml_fragment: &str) -> ErrorTranslationManager {
        ErrorTranslationManager::load_from_string(toml_fragment).expect("测试用 TOML 应能解析")
    }

    /// 渲染消息表命中结果：填充占位符，未消费时拼回残段（与 CLI/LSP 调用方同逻辑）
    fn render(m: &ErrorTranslationManager, message: &str) -> Option<String> {
        let (entry, rest) = m.query_by_message(message)?;
        let mut text = entry.message_template.clone();
        if let Some(rest) = rest {
            let (filled, consumed) = fill_dynamic_placeholders(&text, &rest);
            text = filled;
            if !consumed {
                text.push_str(rest.text());
            }
        }
        Some(text)
    }

    /// 两端动态（E0277 族）：通配段键一次拿齐两个动态名
    #[test]
    fn wildcard_segment_captures_two_dynamic_names() {
        let m = manager(
            "[\"消息翻译\".\"the trait `?` is not implemented for `?`\"]\n\"消息模板\" = \"特征 `{q0}` 未对 `{q1}` 实现\"",
        );
        assert_eq!(
            render(
                &m,
                "the trait `std::fmt::Display` is not implemented for `HashMap<&str, i32>`"
            )
            .as_deref(),
            Some("特征 `std::fmt::Display` 未对 `HashMap<&str, i32>` 实现")
        );
    }

    /// 多通配（E0106 族）：非贪婪逐个切分，能分开两个参数名与其生命周期数
    ///
    /// 键取自 rustc 的 help 原文（整句两端动态，单一前缀/后缀键只能翻到头或尾）；
    /// 捕获按 `?` 出现顺序编号：q0=首个参数名列表、q1=其生命周期数、q2=次个参数名；
    /// 模板只引用 q0/q2，未引用的 q1 被丢弃（不粘回原文）。
    #[test]
    fn wildcard_segment_is_non_greedy_for_three_captures() {
        let m = manager(
            "[\"消息翻译\".\"the lifetime `'a` may only live as long as one of `?`'s ? or one of `?`'s ?\"]\n\"消息模板\" = \"`a` 只能与 `{q0}` 或 `{q2}` 的引用同长存活\"",
        );
        let message = "the lifetime `'a` may only live as long as one of `a' and `b' and `c`'s 2 lifetimes or one of `d`'s 2 lifetimes";
        assert_eq!(
            render(&m, message).as_deref(),
            Some("`a` 只能与 `a' and `b' and `c` 或 `d` 的引用同长存活")
        );
    }

    /// 特异度：通配段键优先于更长的前缀/后缀键（避免前缀键吞掉整句）
    #[test]
    fn wildcard_segment_beats_prefix_and_suffix_keys() {
        let m = manager(
            "[\"消息翻译\".\"the trait `\"]\n\"消息模板\" = \"特征 `{q0}`\"\n\
             [\"消息翻译\".\"~ is not implemented for `\"]\n\"消息模板\" = \"未实现尾巴\"\n\
             [\"消息翻译\".\"the trait `?` is not implemented for `?`\"]\n\"消息模板\" = \"特征 `{q0}` 未对 `{q1}` 实现\"",
        );
        assert_eq!(
            render(&m, "the trait `Debug` is not implemented for `Foo`").as_deref(),
            Some("特征 `Debug` 未对 `Foo` 实现")
        );
    }

    /// 多通配段键同时命中时取字面量更长者（与最长前缀同口径）
    #[test]
    fn more_specific_segment_key_wins() {
        let m = manager(
            "[\"消息翻译\".\"`?` and `?`\"]\n\"消息模板\" = \"宽松\"\n\
             [\"消息翻译\".\"variants `?` and `?` are never constructed\"]\n\"消息模板\" = \"变体 `{q0}` 与 `{q1}` 从未被构造\"",
        );
        assert_eq!(
            render(&m, "variants `Red` and `Green` are never constructed").as_deref(),
            Some("变体 `Red` 与 `Green` 从未被构造")
        );
    }

    /// 无占位符的通配键：整句改写，不粘回英文残段
    #[test]
    fn segment_without_placeholder_yields_full_translation() {
        let m = manager(
            "[\"消息翻译\".\"this trait has no implementations, `?` adding `?`\"]\n\"消息模板\" = \"该特征无任何实现\"",
        );
        assert_eq!(
            render(&m, "this trait has no implementations, `X` adding `Y`").as_deref(),
            Some("该特征无任何实现")
        );
    }

    /// 精确匹配仍优先于通配段键
    #[test]
    fn exact_match_still_wins_over_segment() {
        let m = manager(
            "[\"消息翻译\".\"the trait `?` is not implemented for `?`\"]\n\"消息模板\" = \"通配\"\n\
             [\"消息翻译\".\"the trait `A` is not implemented for `B`\"]\n\"消息模板\" = \"精确\"",
        );
        assert_eq!(
            render(&m, "the trait `A` is not implemented for `B`").as_deref(),
            Some("精确")
        );
        assert_eq!(
            render(&m, "the trait `C` is not implemented for `D`").as_deref(),
            Some("通配")
        );
    }

    /// 锚点不唯一时必须拒绝（而非吞掉整段前缀）
    ///
    /// 反例踩坑：消息前半句 `...a borrowed value, but the signature does not
    /// say...` 本身含有 `borrowed from `，若键只写成句中片段，首个 `?` 会定位到
    /// 第一次出现的错误位置、把整段前缀当成参数名抓走。
    #[test]
    fn segment_with_ambiguous_anchor_rejects() {
        let m = manager(
            "[\"消息翻译\".\"this function's return type contains a borrowed value, but the signature does not say whether it is borrowed from `?` or `?`\"]\n\"消息模板\" = \"未说明借自 `{q0}` 还是 `{q1}`\"",
        );
        let message = "this function's return type contains a borrowed value, but the signature does not say whether it is borrowed from `a` or `b`";
        assert_eq!(
            render(&m, message).as_deref(),
            Some("未说明借自 `a` 还是 `b`")
        );
        // 同一消息不得被片段式键误抓（首段不是消息前缀 → 不命中）
        let bad =
            manager("[\"消息翻译\".\"borrowed from `?` or `?`\"]\n\"消息模板\" = \"误抓 `{q0}`\"");
        assert!(bad.query_by_message(message).is_none());
    }

    /// 字面量不匹配时不得误命中（回落原行为）
    #[test]
    fn segment_rejects_non_matching_message() {
        let m = manager(
            "[\"消息翻译\".\"the trait `?` is not implemented for `?`\"]\n\"消息模板\" = \"特征 `{q0}`\"",
        );
        assert!(m.query_by_message("totally different message").is_none());
    }

    /// 捕获为空串时不假装消费（交回调用方回退，避免信息凭空丢失）
    #[test]
    fn empty_capture_falls_back() {
        let m = manager("[\"消息翻译\".\"a?b?c\"]\n\"消息模板\" = \"{q0}/{q1}\"");
        let (entry, rest) = m.query_by_message("abbc").expect("通配段键应命中空捕获");
        let rest = rest.expect("通配段键应携带捕获");
        let (filled, consumed) = fill_dynamic_placeholders(&entry.message_template, &rest);
        assert_eq!(filled, "{q0}/{q1}");
        assert!(!consumed);
    }

    /// 既有前缀键行为不变（残段回拼，零回归）
    #[test]
    fn prefix_key_behavior_unchanged() {
        let m = manager("[\"消息翻译\".\"consider \"]\n\"消息模板\" = \"考虑 \"");
        assert_eq!(
            render(&m, "consider making this binding mutable").as_deref(),
            Some("考虑 making this binding mutable")
        );
    }

    /// 既有后缀键行为不变（头段取引号对内容，反引号由模板自带）
    #[test]
    fn suffix_key_behavior_unchanged() {
        let m = manager("[\"消息翻译\".\"~ is never used\"]\n\"消息模板\" = \"`{q0}` 从未被使用\"");
        assert_eq!(
            render(&m, "function `helper` is never used").as_deref(),
            Some("`helper` 从未被使用")
        );
    }

    /// 字面量切分正确性（n 个通配 → n+1 段）
    #[test]
    fn split_and_literals_len() {
        assert_eq!(split_wildcard("a?b?c"), vec!["a", "b", "c"]);
        assert_eq!(split_wildcard("no wildcard"), vec!["no wildcard"]);
        assert_eq!(segment_literals_len("a?b?c"), 3);
        assert_eq!(segment_literals_len("中文?"), 2);
    }
}
