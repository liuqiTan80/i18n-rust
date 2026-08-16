// 诊断模块 - 解析 rustc 的 JSON 错误输出，结合语言包翻译成中文教学诊断信息
//
// 将 rustc 编译器产生的 JSON 格式诊断信息（--error-format=json）解析为结构化数据，
// 再根据语言包中的错误消息翻译表，生成面向中文教学场景的诊断输出，
// 包含错误码解释、教学提示、所有权错误叙事化详情等。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

// ============================================================
// rustc JSON 诊断数据结构（对应 --error-format=json 输出）
// ============================================================

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

// ============================================================
// 错误消息翻译结构
// ============================================================

/// 错误消息翻译条目（从 errors.toml 加载）
#[derive(Deserialize, Debug, Clone)]
pub struct ErrorMessageEntry {
    #[serde(rename = "消息模板")]
    pub message_template: String,
    #[serde(rename = "教学提示")]
    pub teaching_hint: Option<String>,
}

/// 错误消息翻译管理器：按错误码查询翻译条目
#[derive(Debug, Clone)]
pub struct ErrorTranslationManager {
    pub translation_table: HashMap<String, ErrorMessageEntry>,
}

impl ErrorTranslationManager {
    /// 从文件加载错误翻译表
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| {
            crate::语言::f(
                "err_read_error_messages",
                &[&path.display().to_string(), &e.to_string()],
            )
        })?;
        Self::load_from_string(&content)
    }

    /// 从 TOML 字符串加载错误翻译表（用于内置数据）
    pub fn load_from_string(content: &str) -> Result<Self, String> {
        let translation_table: HashMap<String, ErrorMessageEntry> = toml::from_str(content)
            .map_err(|e| crate::语言::f("err_parse_error_messages", &[&e.to_string()]))?;
        Ok(Self { translation_table })
    }

    /// 按错误码查询翻译条目
    pub fn query(&self, error_code: &str) -> Option<&ErrorMessageEntry> {
        self.translation_table.get(error_code)
    }

    /// 已覆盖的错误码数量
    pub fn coverage_count(&self) -> usize {
        self.translation_table.len()
    }
}

// ============================================================
// 教学诊断结构（翻译后的输出）
// ============================================================

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
    fn from_str(level: &str) -> Self {
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

// ============================================================
// 所有权错误叙事化详情
// ============================================================

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

// ============================================================
// 诊断翻译器（增加类型映射支持）
// ============================================================

/// 诊断翻译器：将 rustc 诊断翻译为教学诊断
///
/// 结合错误消息翻译表和类型映射，将 rustc 的英文诊断转化为
/// 面向中文学习者的教学诊断信息。
pub struct DiagnosticTranslator {
    translation_manager: ErrorTranslationManager,
    type_map: HashMap<String, String>, // 来自关键字映射表，用于替换消息中的英文类型
}

/// 从 `expected `X`, found `Y`` 形式的文本（rustc label）中提取期望/实际类型
fn extract_expected_found(text: &str) -> Option<(String, String)> {
    let pos = text.find("expected ")?;
    let after = &text[pos + "expected ".len()..];
    let comma_pos = after.find(", found ")?;
    let exp = after[..comma_pos].trim().trim_matches('`').to_string();
    let fnd = after[comma_pos + ", found ".len()..]
        .trim()
        .trim_matches('`')
        .to_string();
    Some((exp, fnd))
}

/// 提取文本中所有反引号包裹的内容（按出现顺序）
fn extract_backtick_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        match after.find('`') {
            Some(end) => {
                tokens.push(after[..end].to_string());
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    tokens
}

/// 从诊断消息中提取前两个反引号包裹的类型
///
/// rustc 1.97+ 的算术错误不再输出 label，类型信息嵌入 message，
/// 如 E0369：`cannot add `{integer}` to `&str``。
fn extract_types_from_message(message: &str) -> Option<(String, String)> {
    let tokens = extract_backtick_tokens(message);
    if tokens.len() >= 2 {
        Some((tokens[0].clone(), tokens[1].clone()))
    } else {
        None
    }
}

impl DiagnosticTranslator {
    /// 创建诊断翻译器
    pub fn new(
        translation_manager: ErrorTranslationManager,
        type_map: HashMap<String, String>,
    ) -> Self {
        Self {
            translation_manager,
            type_map,
        }
    }

    /// 翻译单条诊断信息
    pub fn translate_diagnostic(&self, diagnostic: &CompilerDiagnostic) -> TeachingDiagnostic {
        let error_code = diagnostic.code.as_ref().map(|c| c.code.clone());

        let matched_entry = error_code
            .as_deref()
            .and_then(|code| self.translation_manager.query(code));

        // 从主要 span 的 label 中提取 expected 和 found；
        // rustc 1.97+ 的算术错误（E0369 等）不再输出 label，类型信息
        // 直接嵌入 message（如 `cannot add `{integer}` to `&str``），
        // 因此提取失败时回退从 message 中解析反引号包裹的类型。
        let primary_label = diagnostic
            .spans
            .iter()
            .find(|s| s.is_primary)
            .and_then(|s| s.label.as_deref());

        let (expected, found) = primary_label
            .and_then(extract_expected_found)
            .or_else(|| extract_types_from_message(&diagnostic.message))
            .unwrap_or_default();

        // 构建翻译消息
        let translated_message = if let Some(entry) = matched_entry {
            let mut template = entry.message_template.clone();
            if !expected.is_empty() && !found.is_empty() {
                template = template
                    .replace("{期望}", &expected)
                    .replace("{实际}", &found);
            } else if template.contains("{期望}") || template.contains("{实际}") {
                // 无法提取期望/实际类型时回退 rustc 原文，避免输出裸占位符
                template = diagnostic.message.clone();
            }
            // 对模板中的类型名进行中文化替换
            self.replace_type_names(template)
        } else {
            self.replace_type_names(diagnostic.message.clone())
        };

        let mut teaching_hints = Vec::new();
        if let Some(entry) = matched_entry
            && let Some(hint) = &entry.teaching_hint
        {
            teaching_hints.push(hint.clone());
        }
        for child in &diagnostic.children {
            if child.level == "help" {
                teaching_hints.push(crate::语言::f("diag_fix_suggestion", &[&child.message]));
            }
        }

        let locations = diagnostic
            .spans
            .iter()
            .map(DiagnosticLocation::from_span)
            .collect();

        // 所有权错误：提取叙事化详情（变量名、移动/借用、再次使用位置）
        let ownership_details = error_code
            .as_deref()
            .and_then(|code| extract_ownership_details(code, diagnostic));

        // 翻译消息中含 {变量名} 占位符时，用提取到的变量名填充
        let mut translated_message = translated_message;
        if let Some(details) = &ownership_details {
            translated_message = translated_message.replace("{变量名}", &details.var_name);
        }

        // 其余占位符（{变量名}/{名称}/{类型}/{特征}）按出现顺序从消息反引号内容
        // 回退填充，覆盖无 label 的错误（E0384 重复赋值、E0433 未找到类型等）；
        // 提取不到时回退 rustc 原文，避免输出裸占位符。
        let mut tokens = extract_backtick_tokens(&diagnostic.message).into_iter();
        for placeholder in ["{变量名}", "{名称}", "{类型}", "{特征}"] {
            if translated_message.contains(placeholder) {
                match tokens.next() {
                    Some(token) => {
                        translated_message = translated_message.replace(placeholder, &token);
                    }
                    None => {
                        translated_message = diagnostic.message.clone();
                        break;
                    }
                }
            }
        }

        let children = diagnostic
            .children
            .iter()
            .map(|child| self.translate_diagnostic(child))
            .collect();

        TeachingDiagnostic {
            level: DiagnosticLevel::from_str(&diagnostic.level),
            error_code,
            translated_message,
            original_message: diagnostic.message.clone(),
            teaching_hints,
            locations,
            children,
            ownership_details,
        }
    }

    /// 使用类型映射替换消息中的英文类型名
    fn replace_type_names(&self, message: String) -> String {
        let mut result = message;
        // rustc 未推断字面量占位符（`{integer}`/`{float}`）及 1.97+ 裸显示名
        // （`integer`/`floating-point number`）按全局语言翻译
        result = result
            .replace("{integer}", &crate::语言::t("diag_rustc_integer"))
            .replace("{float}", &crate::语言::t("diag_rustc_float"))
            .replace("floating-point number", &crate::语言::t("diag_rustc_float"))
            .replace("integer", &crate::语言::t("diag_rustc_integer"));
        // 类型映射：仅替换反引号包裹的完整类型名（rustc 诊断中的类型均在反引号内），
        // 避免 "str"→"文本" 等短条目把消息中的 "string" 部分替换成 "文本ing"
        for token in extract_backtick_tokens(&result) {
            if let Some(zh) = self.type_map.get(&token) {
                result = result.replace(&format!("`{}`", token), &format!("`{}`", zh));
            }
        }
        result
    }

    /// 批量翻译诊断列表
    pub fn batch_translate(&self, diagnostics: &[CompilerDiagnostic]) -> Vec<TeachingDiagnostic> {
        diagnostics
            .iter()
            .map(|d| self.translate_diagnostic(d))
            .collect()
    }
}

// ============================================================
// JSON 解析与格式化输出
// ============================================================

/// 从 serde_json::Value 中提取源码文本
fn extract_source_text(text_value: &Option<serde_json::Value>) -> Option<String> {
    text_value.as_ref().and_then(|v| {
        v.as_array()
            .and_then(|arr| arr.first())
            .and_then(|obj| obj.get("text"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
    })
}

/// 解析 rustc/cargo 的 JSON 诊断输出（逐行 JSON 对象）
///
/// 支持两种格式：
/// - rustc 直接输出：每行一个完整的诊断 JSON 对象
/// - cargo --message-format=json 包装：诊断嵌套在 `message` 字段中
pub fn parse_diagnostic_output(output: &str) -> Vec<CompilerDiagnostic> {
    let mut result = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with('{') {
            continue;
        }
        if let Ok(diagnostic) = serde_json::from_str::<CompilerDiagnostic>(line) {
            result.push(diagnostic);
        } else if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            // cargo --message-format=json 的 compiler-message 包装行：
            // 完整诊断嵌套在顶层 message 字段中（build-finished 等行无 message 对象，自动跳过）
            if let Ok(diagnostic) =
                serde_json::from_value::<CompilerDiagnostic>(value["message"].clone())
            {
                result.push(diagnostic);
            }
        }
    }
    result
}

/// 格式化后的诊断信息（用于文本输出）
pub struct FormattedDiagnostic {
    pub level_text: String,
    pub code_text: String,
    pub message: String,
    pub location_descriptions: Vec<String>,
    pub teaching_hints: Vec<String>,
}

impl TeachingDiagnostic {
    /// 格式化为结构化诊断
    pub fn format(&self) -> FormattedDiagnostic {
        let level_text = self.level.display_text().to_string();
        let code_text = self
            .error_code
            .as_ref()
            .map(|code| format!("[{}]", code))
            .unwrap_or_default();

        let location_descriptions = self
            .locations
            .iter()
            .filter(|p| p.is_primary)
            .map(|p| {
                let mut desc = format!("  --> {}:{}:{}", p.file_name, p.line_start, p.column_start);
                if let Some(label) = &p.label {
                    desc = format!("{}\n      {}", desc, label);
                }
                desc
            })
            .collect();

        FormattedDiagnostic {
            level_text,
            code_text,
            message: self.translated_message.clone(),
            location_descriptions,
            teaching_hints: self.teaching_hints.clone(),
        }
    }

    /// 格式化为文本（可直接输出到终端）
    pub fn format_as_text(&self) -> String {
        let formatted = self.format();
        let mut output = String::new();

        // 第一行：错误级别 + 错误码 + 消息
        output.push_str(&format!(
            "{}{}: {}\n",
            formatted.level_text, formatted.code_text, formatted.message
        ));

        // 位置信息（只显示第一个主要位置）
        if let Some(location) = self.locations.iter().find(|p| p.is_primary) {
            output.push_str(&format!(
                "  --> {}:{}:{}\n",
                location.file_name, location.line_start, location.column_start
            ));
            if let Some(source) = &location.source_text {
                output.push_str(&format!("   | {}\n", source));
            }
        }

        // 所有权错误叙事提示
        if let Some(details) = &self.ownership_details {
            output.push_str(&format!("📌 {}\n", details.narrative_text()));
        }

        // 第一条教学提示
        if let Some(hint) = self.teaching_hints.first() {
            output.push_str(&format!("💡 {}\n", hint));
        }

        output
    }

    /// 批量格式化为文本
    pub fn batch_format_as_text(diagnostics: &[TeachingDiagnostic]) -> String {
        let mut output = String::new();
        for (i, diag) in diagnostics.iter().enumerate() {
            if i > 0 {
                output.push_str("\n---\n\n");
            }
            output.push_str(&diag.format_as_text());
        }
        output
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_error_message_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("创建临时文件失败");
        write!(file, "{}", content).expect("写入临时文件失败");
        file
    }

    fn create_test_diagnostic() -> CompilerDiagnostic {
        CompilerDiagnostic {
            message: "mismatched types".to_string(),
            code: Some(DiagnosticCode {
                code: "E0308".to_string(),
                explanation: None,
            }),
            level: "error".to_string(),
            spans: vec![DiagnosticSpan {
                file_name: "src/main.rs".to_string(),
                line_start: 3,
                column_start: 5,
                line_end: 3,
                column_end: 10,
                source_text: Some(
                    serde_json::json!([{"text": "let x: i32 = \"hello\";", "highlight_start": 1, "highlight_end": 22}]),
                ),
                byte_start: Some(30),
                byte_end: Some(51),
                is_primary: true,
                label: Some("expected `i32`, found `&str`".to_string()),
                suggested_replacement: None,
            }],
            children: vec![CompilerDiagnostic {
                message: "consider using a conversion function".to_string(),
                code: None,
                level: "help".to_string(),
                spans: vec![],
                children: vec![],
                rendered: None,
            }],
            rendered: None,
        }
    }

    fn create_test_type_map() -> HashMap<String, String> {
        HashMap::from([
            ("u32".into(), "整数32".into()),
            ("i32".into(), "有符号整数32".into()),
            ("f64".into(), "浮点64".into()),
            ("&str".into(), "字符串引用".into()),
            ("String".into(), "字符串".into()),
        ])
    }

    #[test]
    fn test_load_error_translation_manager() {
        let toml_content = r#"
[E0308]
"消息模板" = "类型不匹配：期望 `{期望}`，实际得到 `{实际}`"
"教学提示" = "请检查变量类型是否与上下文要求一致。"

[E0433]
"消息模板" = "未找到类型 `{名称}`"
"教学提示" = "请确认是否已导入所需的模块或类型。"
"#;
        let file = create_error_message_file(toml_content);
        let manager = ErrorTranslationManager::load_from_file(file.path()).unwrap();
        assert_eq!(manager.coverage_count(), 2);
    }

    #[test]
    fn test_translate_with_lang_pack_and_type_replacement() {
        let toml_content = r#"
[E0308]
"消息模板" = "类型不匹配：期望 `{期望}`，实际得到 `{实际}`"
"教学提示" = "请检查变量类型。"
"#;
        let file = create_error_message_file(toml_content);
        let manager = ErrorTranslationManager::load_from_file(file.path()).unwrap();
        let type_map = create_test_type_map();
        let translator = DiagnosticTranslator::new(manager, type_map);

        let diagnostic = create_test_diagnostic();
        let teaching = translator.translate_diagnostic(&diagnostic);

        assert_eq!(
            teaching.translated_message,
            "类型不匹配：期望 `有符号整数32`，实际得到 `字符串引用`"
        );
    }

    /// rustc 1.97+ 算术错误（E0369）无 label，类型嵌入 message，
    /// 应回退提取并翻译 rustc 字面量占位符 `{integer}`
    #[test]
    fn test_translate_e0369_no_label_fallback_to_message() {
        let _guard = crate::语言::LANG_TEST_LOCK.lock().unwrap();
        let toml_content = r#"
[E0369]
"消息模板" = "类型不匹配：无法对 `{期望}` 和 `{实际}` 执行运算"
"教学提示" = "运算符两侧的类型必须兼容。"
"#;
        let file = create_error_message_file(toml_content);
        let manager = ErrorTranslationManager::load_from_file(file.path()).unwrap();
        let translator = DiagnosticTranslator::new(manager, create_test_type_map());
        crate::语言::set_language("zh");

        let diagnostic = CompilerDiagnostic {
            message: "cannot add `{integer}` to `&str`".to_string(),
            code: Some(DiagnosticCode {
                code: "E0369".to_string(),
                explanation: None,
            }),
            level: "error".to_string(),
            spans: vec![DiagnosticSpan {
                file_name: "src/main.rs".to_string(),
                line_start: 3,
                column_start: 5,
                line_end: 3,
                column_end: 10,
                source_text: None,
                byte_start: None,
                byte_end: None,
                is_primary: true,
                label: None,
                suggested_replacement: None,
            }],
            children: vec![],
            rendered: None,
        };

        let teaching = translator.translate_diagnostic(&diagnostic);
        assert_eq!(
            teaching.translated_message,
            "类型不匹配：无法对 `整数` 和 `字符串引用` 执行运算"
        );
    }

    #[test]
    fn test_parse_json_diagnostic_output() {
        let json_line = r#"{"message":"mismatched types","code":{"code":"E0308"},"level":"error","spans":[],"children":[],"rendered":null}"#;
        let output = format!("{}\n{}", "    Compiling test v0.1.0", json_line);
        let diagnostics = parse_diagnostic_output(&output);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].message, "mismatched types");
    }

    #[test]
    fn test_parse_cargo_wrapped_diagnostic_output() {
        // cargo check --message-format=json 的 compiler-message 包装行：诊断嵌套在 message 字段
        let wrapped_line = r#"{"reason":"compiler-message","package_id":"path+file:///test#e2e@0.1.0","manifest_path":"/test/Cargo.toml","target":{"kind":["bin"],"name":"e2e"},"message":{"message":"use of moved value: `数据`","code":{"code":"E0382"},"level":"error","spans":[],"children":[],"rendered":null}}"#;
        let output = format!("{}\n{}", "    Checking e2e v0.1.0", wrapped_line);
        let diagnostics = parse_diagnostic_output(&output);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code.as_ref().unwrap().code, "E0382");
        assert_eq!(diagnostics[0].message, "use of moved value: `数据`");
    }

    #[test]
    fn test_diagnostic_level_conversion() {
        assert_eq!(DiagnosticLevel::from_str("error"), DiagnosticLevel::Error);
        assert_eq!(
            DiagnosticLevel::from_str("warning"),
            DiagnosticLevel::Warning
        );
        assert_eq!(DiagnosticLevel::from_str("help"), DiagnosticLevel::Help);
    }

    /// 构造所有权错误诊断（rustc JSON 结构）
    fn create_ownership_diagnostic(
        error_code: &str,
        message: &str,
        primary_span: DiagnosticSpan,
        other_spans: Vec<DiagnosticSpan>,
    ) -> CompilerDiagnostic {
        CompilerDiagnostic {
            message: message.to_string(),
            code: Some(DiagnosticCode {
                code: error_code.to_string(),
                explanation: None,
            }),
            level: "error".to_string(),
            spans: std::iter::once(primary_span).chain(other_spans).collect(),
            children: vec![],
            rendered: None,
        }
    }

    fn create_span(line_start: u32, label: &str, is_primary: bool) -> DiagnosticSpan {
        DiagnosticSpan {
            file_name: "src/main.rs".to_string(),
            line_start,
            column_start: 5,
            line_end: line_start,
            column_end: 10,
            source_text: None,
            byte_start: None,
            byte_end: None,
            is_primary,
            label: Some(label.to_string()),
            suggested_replacement: None,
        }
    }

    #[test]
    fn test_extract_ownership_details_e0382() {
        // rustc 对 E0382 输出：主 span 是再次使用处，副 span 标记移动发生
        let diagnostic = create_ownership_diagnostic(
            "E0382",
            "use of moved value: `数据`",
            create_span(5, "value used here after move", true),
            vec![create_span(
                3,
                "move occurs because `数据` has type `String`, which does not implement the `Copy` trait",
                false,
            )],
        );

        let details = extract_ownership_details("E0382", &diagnostic).expect("应提取出所有权详情");
        assert_eq!(details.var_name, "数据");
        assert_eq!(details.move_location.as_ref().unwrap().line_start, 3);
        assert_eq!(details.reuse_location.as_ref().unwrap().line_start, 5);
        assert!(details.borrow_location.is_none());
        assert_eq!(
            details.narrative_text(),
            "变量 `数据` 在第 3 行被移动，第 5 行尝试再次使用。"
        );
    }

    #[test]
    fn test_extract_ownership_details_e0502() {
        // E0502：主 span 是可变借用处，另有不可变借用处与 borrow later used here
        let diagnostic = create_ownership_diagnostic(
            "E0502",
            "cannot borrow `向量` as mutable because it is also borrowed as immutable",
            create_span(6, "mutable borrow occurs here", true),
            vec![
                create_span(4, "immutable borrow occurs here", false),
                create_span(7, "borrow later used here", false),
            ],
        );

        let details = extract_ownership_details("E0502", &diagnostic).expect("应提取出所有权详情");
        assert_eq!(details.var_name, "向量");
        assert_eq!(details.borrow_location.as_ref().unwrap().line_start, 6);
        // "borrow later used here" 应归为再次使用而非借用发生
        assert_eq!(details.reuse_location.as_ref().unwrap().line_start, 7);
        assert!(details.move_location.is_none());
        assert_eq!(
            details.narrative_text(),
            "变量 `向量` 在第 6 行被借用，第 7 行仍在被使用。"
        );
    }

    #[test]
    fn test_extract_ownership_details_e0507() {
        // E0507：主 span 即移动发生处，无再次使用位置
        let diagnostic = create_ownership_diagnostic(
            "E0507",
            "cannot move out of `数据` which is behind a shared reference",
            create_span(
                3,
                "move occurs because `数据` has type `String`, which does not implement the `Copy` trait",
                true,
            ),
            vec![],
        );

        let details = extract_ownership_details("E0507", &diagnostic).expect("应提取出所有权详情");
        assert_eq!(details.var_name, "数据");
        assert_eq!(details.move_location.as_ref().unwrap().line_start, 3);
        assert!(details.reuse_location.is_none());
        assert_eq!(details.narrative_text(), "变量 `数据` 在第 3 行被移动。");
    }

    #[test]
    fn test_extract_ownership_details_non_ownership_error_returns_none() {
        // E0308 类型不匹配不是所有权错误，不应提取详情
        let diagnostic = create_test_diagnostic();
        assert!(extract_ownership_details("E0308", &diagnostic).is_none());
    }

    #[test]
    fn test_ownership_details_serialize_to_json() {
        let diagnostic = create_ownership_diagnostic(
            "E0382",
            "use of moved value: `数据`",
            create_span(5, "value used here after move", true),
            vec![create_span(
                3,
                "move occurs because `数据` has type `String`",
                false,
            )],
        );
        let details = extract_ownership_details("E0382", &diagnostic).unwrap();

        let value = serde_json::to_value(&details).expect("所有权详情应可序列化");
        assert_eq!(value["变量名"], "数据");
        assert_eq!(value["移动发生"]["起始行"], 3);
        assert_eq!(value["再次使用"]["起始行"], 5);
        assert_eq!(value["借用发生"], serde_json::Value::Null);
    }

    #[test]
    fn test_translate_diagnostic_with_ownership_details_and_narrative() {
        let _guard = crate::语言::LANG_TEST_LOCK.lock().unwrap();
        let toml_content = r#"
[E0382]
"消息模板" = "值在移动后被使用：`{变量名}`"
"教学提示" = "Rust 中值被移动后不能再使用。"
"#;
        let manager = ErrorTranslationManager::load_from_string(toml_content).unwrap();
        let translator = DiagnosticTranslator::new(manager, create_test_type_map());

        let diagnostic = create_ownership_diagnostic(
            "E0382",
            "use of moved value: `数据`",
            create_span(5, "value used here after move", true),
            vec![create_span(
                3,
                "move occurs because `数据` has type `String`",
                false,
            )],
        );
        let teaching = translator.translate_diagnostic(&diagnostic);

        assert!(teaching.ownership_details.is_some());
        let details = teaching.ownership_details.as_ref().unwrap();
        assert_eq!(details.var_name, "数据");

        let text = teaching.format_as_text();
        assert!(text.contains("📌 变量 `数据` 在第 3 行被移动，第 5 行尝试再次使用。"));
        assert!(text.contains("💡 Rust 中值被移动后不能再使用。"));
    }
}
