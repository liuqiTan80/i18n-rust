//! 教学诊断结构（翻译后的输出）与所有权错误叙事化详情。

use serde::Serialize;

use super::model::{CompilerDiagnostic, DiagnosticSpan, extract_source_text};

/// 教学诊断：翻译后的完整诊断信息，包含错误码、翻译消息、教学提示、位置等
#[derive(Debug, Clone)]
pub struct TeachingDiagnostic {
    pub level: DiagnosticLevel,
    pub error_code: Option<String>,
    pub translated_message: String,
    pub original_message: String,
    pub teaching_hints: Vec<String>,
    pub locations: Vec<DiagnosticLocation>,
    pub children: Vec<TeachingDiagnostic>,
    pub ownership_details: Option<OwnershipDetails>,
}

/// 诊断级别
#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
    Help,
    ICE,
    Unknown(String),
}

impl DiagnosticLevel {
    /// 从 rustc 的 level 字符串解析诊断级别
    pub(super) fn from_str(level: &str) -> Self {
        match level {
            "error" => Self::Error,
            "warning" => Self::Warning,
            "note" => Self::Note,
            "help" => Self::Help,
            "ice" => Self::ICE,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// 返回当前语言下的诊断级别显示文字
    pub fn display_text(&self) -> String {
        let key = match self {
            Self::Error => "diag_kind_error",
            Self::Warning => "diag_kind_warning",
            Self::Note => "diag_kind_note",
            Self::Help => "diag_kind_help",
            Self::ICE => "diag_kind_ice",
            Self::Unknown(s) => return s.clone(),
        };
        crate::语言::取消息(key)
    }
}

/// 诊断位置信息（翻译后的跨度）
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticLocation {
    pub file_name: String,
    #[serde(rename = "起始行")]
    pub line_start: u32,
    #[serde(rename = "起始列")]
    pub column_start: u32,
    #[serde(rename = "结束行")]
    pub line_end: u32,
    #[serde(rename = "结束列")]
    pub column_end: u32,
    pub source_text: Option<String>,
    pub label: Option<String>,
    pub is_primary: bool,
}

impl DiagnosticLocation {
    /// 从 rustc 的原始跨度构造诊断位置
    pub fn from_span(span: &DiagnosticSpan) -> Self {
        Self {
            file_name: span.file_name.clone(),
            line_start: span.line_start,
            column_start: span.column_start,
            line_end: span.line_end,
            column_end: span.column_end,
            source_text: extract_source_text(&span.source_text),
            label: span.label.clone(),
            is_primary: span.is_primary,
        }
    }
}

/// 所有权错误的叙事化详情
///
/// 用于 E0382（使用已移动的值）、E0502（同时可变与不可变借用）、
/// E0507（不能移出借用的内容）等错误的增强提示：
/// 指出具体变量名、移动/借用发生的位置与再次使用的位置。
/// 可序列化为 JSON，供 LSP 代理存入诊断的 data 字段。
#[derive(Debug, Clone, Serialize)]
pub struct OwnershipDetails {
    #[serde(rename = "变量名")]
    pub var_name: String,
    #[serde(rename = "移动发生")]
    pub move_location: Option<DiagnosticLocation>,
    #[serde(rename = "借用发生")]
    pub borrow_location: Option<DiagnosticLocation>,
    #[serde(rename = "再次使用")]
    pub reuse_location: Option<DiagnosticLocation>,
    /// E0502 的冲突双方（仅当两个借用标签都在跨度中时才有值），
    /// 有值时叙事改走 `diag_ownership_borrow_conflict`
    #[serde(rename = "借用冲突", skip_serializing_if = "Option::is_none")]
    pub borrow_conflict: Option<BorrowConflict>,
}

/// E0502 的借用冲突双方
///
/// rustc 对 E0502 给出的两个标签是「mutable borrow occurs here」与
/// 「immutable borrow occurs here」（另可能附加「immutable borrow later used here」）。
/// 旧实现把「借用发生」取主 span（总是那个被拒绝的可变借用）、「仍在用」取
/// later-used 行，于是渲染出「在第 7 行被借用，第 5 行仍在被使用」——行号读反，
/// 初学者会当成时间倒流。这里按标签分别记下两方，叙事不再依赖先后猜。
#[derive(Debug, Clone, Serialize)]
pub struct BorrowConflict {
    #[serde(rename = "可变借用")]
    pub mutable: DiagnosticLocation,
    #[serde(rename = "不可变借用")]
    pub immutable: DiagnosticLocation,
}

impl BorrowConflict {
    /// 按标签把两端位置分开收：返回 `(可变借用, 不可变借用)`
    ///
    /// 两个坑都要躲：①「immutable borrow occurs here」以「mutable borrow」结尾
    /// （im**mutable**），必须先判 immutable；② 已经收到 immutable 后，
    /// 「immutable borrow later used here」仍会命中 mutable 分支，故可变分支还得排除
    /// label 里的 immutable。LSP 侧也用它：rust-analyzer 的主 range 不带标签，
    /// 可变侧的兜底由调用方补。
    pub fn classify_labelled_spans<'a, I>(
        spans: I,
    ) -> (Option<DiagnosticLocation>, Option<DiagnosticLocation>)
    where
        I: IntoIterator<Item = (&'a str, DiagnosticLocation)>,
    {
        let mut mutable = None;
        let mut immutable = None;
        for (label, location) in spans {
            if immutable.is_none() && label.contains("immutable borrow") {
                immutable = Some(location);
            } else if mutable.is_none()
                && label.contains("mutable borrow")
                && !label.contains("immutable")
            {
                mutable = Some(location);
            }
        }
        (mutable, immutable)
    }

    /// 两端都认出来时才算得上「借用冲突」
    pub fn from_sides(
        mutable: Option<DiagnosticLocation>,
        immutable: Option<DiagnosticLocation>,
    ) -> Option<Self> {
        match (mutable, immutable) {
            (Some(mutable), Some(immutable)) => Some(Self { mutable, immutable }),
            _ => None,
        }
    }

    /// 从「标签 + 位置」序列里认出冲突双方
    pub fn from_labelled_spans<'a, I>(spans: I) -> Option<Self>
    where
        I: IntoIterator<Item = (&'a str, DiagnosticLocation)>,
    {
        let (mutable, immutable) = Self::classify_labelled_spans(spans);
        Self::from_sides(mutable, immutable)
    }
}

impl OwnershipDetails {
    /// 生成当前语言下的叙事性教学文本
    ///
    /// 示例（中文）：变量 `数据` 在第 3 行被移动，第 5 行尝试再次使用。
    /// E0502 另走 `diag_ownership_borrow_conflict`：只陈述「两方在第几行」
    /// 与「不能重叠」，不暗示谁先谁后（两种形状下都能读对）。
    pub fn narrative_text(&self) -> String {
        let var = &self.var_name;
        if let Some(conflict) = &self.borrow_conflict {
            return crate::语言::取带参消息(
                "diag_ownership_borrow_conflict",
                &[
                    var,
                    &conflict.mutable.line_start.to_string(),
                    &conflict.immutable.line_start.to_string(),
                ],
            );
        }
        match (
            &self.move_location,
            &self.borrow_location,
            &self.reuse_location,
        ) {
            (Some(mv), None, Some(reuse)) => crate::语言::取带参消息(
                "diag_ownership_moved_reused",
                &[
                    var,
                    &mv.line_start.to_string(),
                    &reuse.line_start.to_string(),
                ],
            ),
            (None, Some(borrow), Some(reuse)) => crate::语言::取带参消息(
                "diag_ownership_borrowed_in_use",
                &[
                    var,
                    &borrow.line_start.to_string(),
                    &reuse.line_start.to_string(),
                ],
            ),
            (Some(mv), _, _) => crate::语言::取带参消息(
                "diag_ownership_moved",
                &[var, &mv.line_start.to_string()],
            ),
            (None, Some(borrow), _) => crate::语言::取带参消息(
                "diag_ownership_borrowed",
                &[var, &borrow.line_start.to_string()],
            ),
            _ => crate::语言::取带参消息("diag_ownership_conflict", &[var]),
        }
    }
}

/// 所有权相关错误码（E0382 使用已移动的值 / E0502 同时可变与不可变借用 / E0507 不能移出借用的内容）
pub const OWNERSHIP_ERROR_CODES: [&str; 3] = ["E0382", "E0502", "E0507"];

/// 从 rustc 诊断中提取所有权错误详情
///
/// 解析主 span 与子 span 的标签：
/// - 变量名：消息或 span 标签中的反引号内容（`x`）；
/// - 移动发生：标签含 "move"（value moved here / move occurs because...）；
/// - 借用发生：标签含 "borrow"（immutable/mutable borrow occurs here）；
/// - 再次使用：标签含 "used here"（value used here after move / borrow later used here）。
///
/// 主 span 作为对应错误类型的兜底位置（E0382→再次使用，E0502→借用发生，E0507→移动发生）。
/// 变量名缺失或没有任何位置信息时返回 None。
pub fn extract_ownership_details(
    error_code: &str,
    diagnostic: &CompilerDiagnostic,
) -> Option<OwnershipDetails> {
    if !OWNERSHIP_ERROR_CODES.contains(&error_code) {
        return None;
    }
    let var_name = extract_var_name_from_message(&diagnostic.message)
        .or_else(|| extract_var_name_from_spans(&diagnostic.spans))?;

    // 收集顶层与子诊断中的所有跨度
    let all_spans: Vec<&DiagnosticSpan> = diagnostic
        .spans
        .iter()
        .chain(
            diagnostic
                .children
                .iter()
                .flat_map(|child| child.spans.iter()),
        )
        .collect();

    let mut move_location = None;
    let mut borrow_location = None;
    let mut reuse_location = None;
    for span in &all_spans {
        let label = span.label.as_deref().unwrap_or("");
        // 注意顺序："borrow later used here" 同时含 borrow 与 used here，应归为再次使用
        if label.contains("used here")
            || label.contains("later used")
            || label.contains("after move")
        {
            reuse_location.get_or_insert_with(|| DiagnosticLocation::from_span(span));
        } else if label.contains("move") {
            move_location.get_or_insert_with(|| DiagnosticLocation::from_span(span));
        } else if label.contains("borrow") {
            borrow_location.get_or_insert_with(|| DiagnosticLocation::from_span(span));
        }
    }

    // 主 span 兜底：对应错误类型的核心位置
    if let Some(primary) = diagnostic.spans.iter().find(|s| s.is_primary) {
        match error_code {
            "E0382" => {
                reuse_location.get_or_insert_with(|| DiagnosticLocation::from_span(primary));
            }
            "E0502" => {
                borrow_location.get_or_insert_with(|| DiagnosticLocation::from_span(primary));
            }
            "E0507" => {
                move_location.get_or_insert_with(|| DiagnosticLocation::from_span(primary));
            }
            _ => {}
        }
    }

    if move_location.is_none() && borrow_location.is_none() && reuse_location.is_none() {
        return None;
    }
    // E0502 单独记下冲突双方：上方「借用发生 / 仍在用」的粗分类会
    // 把被拒绝的可变借用当成前者（主 span 兜底），行号因此读反
    let borrow_conflict = if error_code == "E0502" {
        BorrowConflict::from_labelled_spans(all_spans.iter().filter_map(|span| {
            span.label
                .as_deref()
                .map(|label| (label, DiagnosticLocation::from_span(span)))
        }))
    } else {
        None
    };
    Some(OwnershipDetails {
        var_name,
        move_location,
        borrow_location,
        reuse_location,
        borrow_conflict,
    })
}

/// 从诊断消息中提取反引号包裹的变量名
///
/// 示例："use of moved value: `数据`" → "数据"
fn extract_var_name_from_message(message: &str) -> Option<String> {
    let start = message.find('`')?;
    let rest = &message[start + 1..];
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
}

/// 从 span 标签中提取反引号包裹的变量名
///
/// 示例："move occurs because `数据` has type `String`..." → "数据"
fn extract_var_name_from_spans(spans: &[DiagnosticSpan]) -> Option<String> {
    spans.iter().find_map(|span| {
        let label = span.label.as_deref()?;
        let start = label.find('`')?;
        let rest = &label[start + 1..];
        let end = rest.find('`')?;
        Some(rest[..end].to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用位置（只关心行号）
    fn loc(line: u32) -> DiagnosticLocation {
        DiagnosticLocation {
            file_name: "main.zh".to_string(),
            line_start: line,
            column_start: 1,
            line_end: line,
            column_end: 2,
            source_text: None,
            label: None,
            is_primary: false,
        }
    }

    /// 冲突叙事的模板必须在 11 个语言包里都在，且三个槽位都能回填。
    /// 键缺失时 `f()` 会原样返回键名，占位符写错则行号丢失——两者都实机可见。
    /// 走 `narrative_text()` 而非直接调 `f()`：与 CLI 实际入口同一条路径。
    #[test]
    fn borrow_conflict_template_present_in_every_language_pack() {
        let details = OwnershipDetails {
            var_name: "x".to_string(),
            move_location: None,
            borrow_location: None,
            reuse_location: None,
            borrow_conflict: Some(BorrowConflict {
                mutable: loc(7),
                immutable: loc(5),
            }),
        };
        for lang in [
            "zh", "en", "ja", "ko", "ru", "de", "es", "fr", "pt", "hi", "ar",
        ] {
            let _guard = crate::语言::钉住语言(lang);
            let text = details.narrative_text();
            assert!(
                !text.contains("diag_ownership_borrow_conflict"),
                "{lang} 语言包缺少 diag_ownership_borrow_conflict"
            );
            assert!(
                text.contains('x') && text.contains('7') && text.contains('5'),
                "{lang} 的冲突叙事没有回填全部三个槽位：{text}"
            );
        }
    }

    /// 标签判定次序：“immutable borrow occurs here” 以 “mutable borrow” 结尾，
    /// 先认 mutable 会把不可变借用当成可变借用；且第二处 immutable 标签
    /// （later used）也不得冒充可变借用。两种顺序都要稳。
    #[test]
    fn immutable_label_is_not_mistaken_for_mutable() {
        let mutable_first = BorrowConflict::from_labelled_spans(vec![
            ("mutable borrow occurs here", loc(7)),
            ("immutable borrow occurs here", loc(5)),
            ("immutable borrow later used here", loc(5)),
        ])
        .expect("应认出冲突双方");
        assert_eq!(mutable_first.mutable.line_start, 7);
        assert_eq!(mutable_first.immutable.line_start, 5);

        // 顺序颠倒（LSP 的 relatedInformation 里没有主 range 的标签）：
        // 可变侧必须缺席，交给调用方兜底，不能被 later-used 那条顶上
        let (mutable, immutable) = BorrowConflict::classify_labelled_spans(vec![
            ("immutable borrow occurs here", loc(5)),
            ("immutable borrow later used here", loc(5)),
        ]);
        assert!(mutable.is_none(), "不可变标签不得冒充可变借用");
        assert_eq!(immutable.unwrap().line_start, 5);
    }
}
