//! JSON 解析与格式化输出、未解析导入检测（CLI / LSP 共用）。

use super::model::CompilerDiagnostic;
use super::teaching::TeachingDiagnostic;

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

        // 教学提示（全部输出；此前仅取首条，其余被静默丢弃）
        for hint in &self.teaching_hints {
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

/// 判断诊断消息是否为未解析导入类错误（英文原文匹配）
///
/// 覆盖 rustc E0432（unresolved import）与 E0433（failed to resolve:
/// use of undeclared crate or module）两种消息格式。
pub fn is_unresolved_import_message(message: &str) -> bool {
    message.contains("unresolved import") || message.contains("use of undeclared crate or module")
}

/// 提取消息中所有反引号包裹路径的首段（:: 分隔）
///
/// 仅取形如路径的内容（标识符字符与 ::），过滤含空格的自由文本；
/// 翻译后的诊断同样保留反引号内容，故母语/英文消息均可提取。
pub fn extract_backtick_first_segments(text: &str) -> Vec<String> {
    let mut segs = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('`') else { break };
        let inner = &after[..end];
        rest = &after[end + 1..];
        if !inner.is_empty()
            && inner
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
        {
            let first = inner.split("::").next().unwrap_or(inner);
            if !first.is_empty() {
                segs.push(first.to_string());
            }
        }
    }
    segs
}

/// 从未解析导入消息提取候选 crate 名（去重，排除标准库与保留路径）
///
/// 非未解析导入消息返回空列表。供 CLI 编译诊断提示与 LSP
/// 快捷修复代码动作共用：提示用户通过 `rzc add <crate>` 添加依赖。
pub fn unresolved_crate_candidates(message: &str) -> Vec<String> {
    if !is_unresolved_import_message(message) {
        return Vec::new();
    }
    let mut result = Vec::new();
    for seg in extract_backtick_first_segments(message) {
        if matches!(
            seg.as_str(),
            "std" | "core" | "alloc" | "self" | "super" | "crate" | "proc_macro"
        ) || seg.chars().next().is_some_and(|c| c.is_ascii_digit())
        {
            continue;
        }
        if !result.contains(&seg) {
            result.push(seg);
        }
    }
    result
}
