//! 响应映射模块
//!
//! 将 rust-analyzer 响应中的位置信息（指向虚拟 .rs 文件）
//! 还原为原始 .zh 文件的位置。同时处理诊断信息的中文翻译。
//!
//! 子模块划分：
//! - [`responses`]：各 LSP 请求类型的响应映射（hover/补全/符号/编辑等）
//! - [`diag_text`]：诊断消息翻译与所有权详情提取（CLI 同源消息表）
//!
//! 本文件保留映射基础设施：URI/行/列还原、编辑映射与反向转译。

mod diag_text;
mod responses;

#[cfg(test)]
mod tests;

pub use diag_text::init_diagnostic_translator;

use std::sync::Arc;

use serde_json::{Value, json};

use crate::translation_cache::{TranslationCache, TranslationEntry, en_col_to_zh_col_single};

/// 响应映射器
///
/// 持有翻译缓存的引用，负责将 rust-analyzer 的各种响应
/// 中的位置/URI 从虚拟文件还原到原始中文文件。
pub struct ResponseMapper {
    cache: Arc<TranslationCache>,
    /// 预构建的反向映射表（英文 → 母语），O(1) 查找且结果确定
    reverse_map: std::collections::HashMap<String, String>,
    /// 是否启用补全语言过滤（禁止串语言）
    ///
    /// 方言关键字与英文存在差异时启用（zh/ja/ru 等）；
    /// 英文包为恒等映射（fn=fn），不过滤。
    strict_native_filter: bool,
}

impl ResponseMapper {
    /// 创建新的响应映射器
    ///
    /// 反向表直接复用 TranslationCache 构造时预构建的合并表
    /// （关键字反转后合并别名反转，多对一冲突保留排序最小者，
    /// 结果确定），避免每次构造映射器重复构建。
    pub fn new(cache: Arc<TranslationCache>) -> Self {
        // 英文语言包为恒等映射（键=值），无需也无法做语言过滤
        let strict_native_filter = cache
            .keyword_map()
            .iter()
            .any(|(native, english)| native != english);
        Self {
            reverse_map: cache.reverse_map().clone(),
            cache,
            strict_native_filter,
        }
    }

    /// 判断一个 URI 是否指向我们的虚拟文件
    pub fn is_virtual_uri(&self, uri: &str) -> bool {
        self.cache.query_by_virtual_uri(uri).is_some()
    }

    /// 按方言 URI 查询翻译条目（全角标点教学诊断合并等场景）
    pub fn entry_for_original(&self, uri: &str) -> Option<std::sync::Arc<TranslationEntry>> {
        self.cache.query_original(uri)
    }

    /// 将虚拟 URI 替换为原始 URI
    pub fn restore_uri(&self, uri: &str) -> String {
        if let Some(entry) = self.cache.query_by_virtual_uri(uri) {
            entry.original_uri.clone()
        } else {
            uri.to_string()
        }
    }

    /// 预取条目后映射一个 LSP Range（无锁纯函数）
    ///
    /// 循环场景（诊断/定义/引用等批量映射）由调用方预取条目一次，
    /// 复用同一文档的列映射，避免每条 range 重复加锁查询缓存。
    pub(super) fn restore_range_with_entry(
        entry: Option<&TranslationEntry>,
        range: &Value,
    ) -> Value {
        let map_position = |line: u32, col: u32| -> (u32, u32) {
            match entry {
                Some(e) => (
                    restore_line_single(e, line),
                    en_col_to_zh_col_single(e, line, col),
                ),
                None => (line, col),
            }
        };
        let start_line = range["start"]["line"].as_u64().unwrap_or(0) as u32;
        let start_col = range["start"]["character"].as_u64().unwrap_or(0) as u32;
        let end_line = range["end"]["line"].as_u64().unwrap_or(0) as u32;
        let end_col = range["end"]["character"].as_u64().unwrap_or(0) as u32;

        let (zh_start_line, zh_start_col) = map_position(start_line, start_col);
        let (zh_end_line, zh_end_col) = map_position(end_line, end_col);

        json!({
            "start": { "line": zh_start_line, "character": zh_start_col },
            "end": { "line": zh_end_line, "character": zh_end_col }
        })
    }

    /// 映射一个 LSP Range
    pub fn restore_range(&self, uri: &str, range: &Value) -> Value {
        // 预取翻译条目一次：起点/终点列映射基于同一文档，一次查询后
        // 走无锁纯函数，避免每处 range 重复锁（诊断/高亮/符号/编辑
        // 每处 2 次 restore_position，每次内部 2-4 次锁获取）
        let entry = self
            .cache
            .query_by_virtual_uri(uri)
            .or_else(|| self.cache.query_original(uri));
        Self::restore_range_with_entry(entry.as_deref(), range)
    }

    /// 预取条目后映射一个 LSP Location（无锁纯函数）
    ///
    /// URI 还原与 range 映射共用同一预取条目；循环场景（诊断的
    /// relatedInformation、定义/引用批量映射）由调用方预取避免重复加锁。
    pub(super) fn restore_location_with_entry(
        entry: Option<&TranslationEntry>,
        virtual_uri: &str,
        location: &Value,
    ) -> Value {
        let original_uri = match entry {
            Some(e) => e.original_uri.clone(),
            None => virtual_uri.to_string(),
        };
        let original_range = Self::restore_range_with_entry(entry, &location["range"]);

        json!({
            "uri": original_uri,
            "range": original_range
        })
    }

    /// 映射一个 LSP Location（URI + Range）
    pub fn restore_location(&self, location: &Value) -> Value {
        let virtual_uri = location["uri"].as_str().unwrap_or("");
        let entry = self
            .cache
            .query_by_virtual_uri(virtual_uri)
            .or_else(|| self.cache.query_original(virtual_uri));
        Self::restore_location_with_entry(entry.as_deref(), virtual_uri, location)
    }

    /// 从关键字映射中反向查找：英文 → 中文（预构建表，O(1)）
    pub(super) fn reverse_lookup(&self, en_name: &str) -> Option<String> {
        self.reverse_map.get(en_name).cloned()
    }

    /// 将英文代码片段反向翻译为母语
    ///
    /// 精确命中关键字时直接替换；含 snippet 占位符（`${}`）的片段
    /// 保持原样（避免破坏占位符结构）；其余交给引擎词法级反向转译。
    pub(super) fn translate_code(&self, text: &str) -> String {
        if let Some(zh) = self.reverse_lookup(text) {
            return zh;
        }
        if text.contains("${") {
            return text.to_string();
        }
        // 补全片段无文档上下文：不删除任何 crate:: 前缀（宁可保留代理
        // 前缀，也不误删用户手写的前缀——见 TranslationCache::reverse_transpile）
        self.cache.reverse_transpile(None, text)
    }

    /// 映射一个 WorkspaceEdit（changes + documentChanges）
    ///
    /// 编辑目标不是已打开 .zh 的虚拟文件时直接丢弃。
    pub(super) fn map_edit(&self, edit: &Value, translate_new_text: bool) -> Value {
        let mut mapped = edit.clone();

        // changes: { uri → [TextEdit] }
        if let Some(changes) = edit.get("changes").and_then(|v| v.as_object()) {
            let mut mapped_changes = serde_json::Map::new();
            for (uri, edits_list) in changes {
                if !self.is_virtual_uri(uri) {
                    continue;
                }
                let target_uri = self.restore_uri(uri);
                let mapped_edits = self.map_edit_list(edits_list, uri, translate_new_text);
                mapped_changes.insert(target_uri, mapped_edits);
            }
            mapped["changes"] = Value::Object(mapped_changes);
        }

        // documentChanges: [TextDocumentEdit]
        if let Some(doc_changes) = edit.get("documentChanges").and_then(|v| v.as_array()) {
            let mapped_doc_changes: Vec<Value> = doc_changes
                .iter()
                .filter_map(|item| {
                    let uri = item
                        .get("textDocument")
                        .and_then(|td| td.get("uri"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !self.is_virtual_uri(uri) {
                        return None;
                    }
                    let mut mapped = item.clone();
                    mapped["textDocument"]["uri"] = Value::String(self.restore_uri(uri));
                    if let Some(edits) = item.get("edits") {
                        mapped["edits"] = self.map_edit_list(edits, uri, translate_new_text);
                    }
                    Some(mapped)
                })
                .collect();
            mapped["documentChanges"] = Value::Array(mapped_doc_changes);
        }

        mapped
    }

    /// 映射一组 TextEdit 的位置
    ///
    /// 当 `translate_new_text` 为 true 时（重命名/代码操作/补全），
    /// 将 newText 反向翻译为母语；为 false 时 newText 保持英文。
    pub(super) fn map_edit_list(
        &self,
        edits_list: &Value,
        virtual_uri: &str,
        translate_new_text: bool,
    ) -> Value {
        let mapped: Vec<Value> = edits_list
            .as_array()
            .map(|array| {
                array
                    .iter()
                    .map(|edit| {
                        let mut mapped_edit = edit.clone();
                        if let Some(range) = edit.get("range") {
                            mapped_edit["range"] = self.restore_range(virtual_uri, range);
                        }
                        if translate_new_text
                            && let Some(new_text) = edit.get("newText").and_then(|v| v.as_str())
                        {
                            mapped_edit["newText"] = Value::String(self.translate_code(new_text));
                        }
                        mapped_edit
                    })
                    .collect()
            })
            .unwrap_or_default();
        Value::Array(mapped)
    }
}

/// 根据翻译条目的行映射还原行号
pub(super) fn restore_line_single(entry: &TranslationEntry, en_line: u32) -> u32 {
    let idx = en_line as usize;
    if idx < entry.line_map.len() {
        entry.line_map[idx]
    } else if let Some(&last) = entry.line_map.last() {
        last
    } else {
        en_line
    }
}
