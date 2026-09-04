//! rustc JSON 诊断数据结构（对应 --error-format=json 输出）。

use serde::Deserialize;

/// rustc 编译器诊断信息（对应一行 JSON 输出）
#[derive(Deserialize, Debug, Clone)]
pub struct CompilerDiagnostic {
    #[serde(rename = "message")]
    pub message: String,
    #[serde(rename = "code")]
    pub code: Option<DiagnosticCode>,
    #[serde(rename = "level")]
    pub level: String,
    #[serde(rename = "spans")]
    pub spans: Vec<DiagnosticSpan>,
    #[serde(rename = "children")]
    pub children: Vec<CompilerDiagnostic>,
    #[serde(rename = "rendered")]
    pub rendered: Option<String>,
}

/// 诊断错误码（如 E0308）
#[derive(Deserialize, Debug, Clone)]
pub struct DiagnosticCode {
    #[serde(rename = "code")]
    pub code: String,
    #[serde(rename = "explanation")]
    pub explanation: Option<String>,
}

/// 诊断跨度（代码位置信息）
#[derive(Deserialize, Debug, Clone)]
pub struct DiagnosticSpan {
    #[serde(rename = "file_name")]
    pub file_name: String,
    #[serde(rename = "line_start")]
    pub line_start: u32,
    #[serde(rename = "column_start")]
    pub column_start: u32,
    #[serde(rename = "line_end")]
    pub line_end: u32,
    #[serde(rename = "column_end")]
    pub column_end: u32,
    #[serde(rename = "text")]
    pub source_text: Option<serde_json::Value>,
    #[serde(rename = "byte_start")]
    pub byte_start: Option<u64>,
    #[serde(rename = "byte_end")]
    pub byte_end: Option<u64>,
    #[serde(rename = "is_primary")]
    pub is_primary: bool,
    #[serde(rename = "label")]
    pub label: Option<String>,
    #[serde(rename = "suggested_replacement")]
    pub suggested_replacement: Option<String>,
}

/// 从 serde_json::Value 中提取源码文本
pub(super) fn extract_source_text(text_value: &Option<serde_json::Value>) -> Option<String> {
    text_value.as_ref().and_then(|v| {
        v.as_array()
            .and_then(|arr| arr.first())
            .and_then(|obj| obj.get("text"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
    })
}
