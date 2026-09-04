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
        crate::语言::t(key)
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
}

impl OwnershipDetails {
    /// 生成当前语言下的叙事性教学文本
    ///
    /// 示例（中文）：变量 `数据` 在第 3 行被移动，第 5 行尝试再次使用。
    pub fn narrative_text(&self) -> String {
        let var = &self.var_name;
        match (
            &self.move_location,
            &self.borrow_location,
            &self.reuse_location,
        ) {
            (Some(mv), None, Some(reuse)) => crate::语言::f(
                "diag_ownership_moved_reused",
                &[
                    var,
                    &mv.line_start.to_string(),
                    &reuse.line_start.to_string(),
                ],
            ),
            (None, Some(borrow), Some(reuse)) => crate::语言::f(
                "diag_ownership_borrowed_in_use",
                &[
                    var,
                    &borrow.line_start.to_string(),
                    &reuse.line_start.to_string(),
                ],
            ),
            (Some(mv), _, _) => {
                crate::语言::f("diag_ownership_moved", &[var, &mv.line_start.to_string()])
            }
            (None, Some(borrow), _) => crate::语言::f(
                "diag_ownership_borrowed",
                &[var, &borrow.line_start.to_string()],
            ),
            _ => crate::语言::f("diag_ownership_conflict", &[var]),
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
    Some(OwnershipDetails {
        var_name,
        move_location,
        borrow_location,
        reuse_location,
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
