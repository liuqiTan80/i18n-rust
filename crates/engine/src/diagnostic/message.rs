//! 错误消息翻译结构：从语言包 errors.toml 加载错误码表与消息表。

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
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| {
            crate::语言::f(
                "err_read_error_messages",
                &[&path.display().to_string(), &e.to_string()],
            )
        })?;
        Self::load_from_string(&content)
    }

    /// 从 TOML 字符串加载错误翻译表
    ///
    /// 顶层表分为两类：`[E0xxx]` 等错误码表（含 "消息模板"/"教学提示"）
    /// 与 `[消息翻译]` 消息表（键为英文消息原文，同样含模板与提示）。
    pub fn load_from_string(content: &str) -> Result<Self, String> {
        let value: toml::Value = toml::from_str(content)
            .map_err(|e| crate::语言::f("err_parse_error_messages", &[&e.to_string()]))?;
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
    /// 匹配顺序：精确匹配 → 最长前缀匹配 → 最长后缀匹配。
    /// 返回未匹配的动态部分（前缀匹配为后缀原文，后缀匹配为前缀原文），
    /// 供调用方拼接到模板后，保留 `did you mean \`x\``、`function \`foo\` is never used` 等动态内容。
    /// 后缀键以 `~` 开头（如 `~ is never used`），解决动态名位于消息中间的
    /// lint 警告（dead_code/non_snake_case 族）无法用前缀键覆盖的问题。
    pub fn query_by_message<'a, 'b>(
        &'a self,
        message: &'b str,
    ) -> Option<(&'a ErrorMessageEntry, Option<&'b str>)> {
        // 1. 精确匹配
        if let Some(entry) = self.message_map.get(message) {
            return Some((entry, None));
        }
        // 2. 最长前缀 / 最长后缀候选（前缀优先，其模板通常更完整、含类型词）
        let mut best_prefix: Option<(usize, &str, &ErrorMessageEntry)> = None;
        let mut best_suffix: Option<(usize, &str, &ErrorMessageEntry)> = None;
        for (key, entry) in &self.message_map {
            if let Some(suffix_key) = key.strip_prefix('~') {
                if let Some(prefix) = message.strip_suffix(suffix_key)
                    && best_suffix.is_none_or(|(len, _, _)| suffix_key.len() > len)
                {
                    best_suffix = Some((suffix_key.len(), prefix, entry));
                }
            } else if message.starts_with(key.as_str())
                && best_prefix.is_none_or(|(len, _, _)| key.len() > len)
            {
                best_prefix = Some((key.len(), &message[key.len()..], entry));
            }
        }
        if let Some((_, rest, entry)) = best_prefix {
            return Some((entry, Some(rest)));
        }
        best_suffix.map(|(_, prefix, entry)| (entry, Some(prefix)))
    }

    /// 已覆盖的错误码数量
    pub fn coverage_count(&self) -> usize {
        self.translation_table.len()
    }
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
