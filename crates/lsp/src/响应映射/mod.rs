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

pub(crate) mod diag_text;
mod responses;

#[cfg(test)]
mod tests;

pub use diag_text::初始化诊断翻译器;

use std::sync::Arc;

use serde_json::{Value, json};

use crate::翻译缓存::{TranslationEntry, 英文列转中文列, 转译缓存};

/// 响应映射器
///
/// 持有翻译缓存的引用，负责将 rust-analyzer 的各种响应
/// 中的位置/URI 从虚拟文件还原到原始中文文件。
pub struct 响应映射器 {
    缓存: Arc<转译缓存>,
    /// 预构建的反向映射表（英文 → 母语），O(1) 查找且结果确定
    反向映射表: std::collections::HashMap<String, String>,
    /// 是否启用补全语言过滤（禁止串语言）
    ///
    /// 方言关键字与英文存在差异时启用（zh/ja/ru 等）；
    /// 英文包为恒等映射（fn=fn），不过滤。
    严格母语过滤: bool,
}

impl 响应映射器 {
    /// 创建新的响应映射器
    ///
    /// 反向表直接复用 转译缓存 构造时预构建的合并表
    /// （关键字反转后合并别名反转，多对一冲突保留排序最小者，
    /// 结果确定），避免每次构造映射器重复构建。
    pub fn 新建映射器(缓存: Arc<转译缓存>) -> Self {
        // 英文语言包为恒等映射（键=值），无需也无法做语言过滤
        let 严格母语过滤 = 缓存.关键词映射表().iter().any(|(母语, 英文)| 母语 != 英文);
        Self {
            反向映射表: 缓存.反向映射表().clone(),
            缓存,
            严格母语过滤,
        }
    }

    /// 判断一个 URI 是否指向我们的虚拟文件
    pub fn 是虚拟资源定位(&self, 资源定位: &str) -> bool {
        self.缓存.按虚拟资源定位查询(资源定位).is_some()
    }

    /// 按方言 URI 查询翻译条目（全角标点教学诊断合并等场景）
    pub fn 按原始取条目(
        &self, 资源定位: &str
    ) -> Option<std::sync::Arc<TranslationEntry>> {
        self.缓存.查询原文(资源定位)
    }

    /// 获取教学 lint 已知词表（转发 转译缓存，易混方法名提示用）
    pub fn lint词表(&self) -> &std::collections::HashSet<String> {
        self.缓存.校验词表()
    }

    /// 获取教学 lint 歧义构造器词集（转发 转译缓存，
    /// 「未标注类型」仅提示无法自行推导的 new/default/collect 形态）
    pub fn 歧义构造词集(&self) -> std::collections::HashSet<String> {
        self.缓存.歧义构造词集()
    }

    /// 将虚拟 URI 替换为原始 URI
    pub fn 还原资源定位(&self, 资源定位: &str) -> String {
        if let Some(翻译条目) = self.缓存.按虚拟资源定位查询(资源定位) {
            翻译条目.原始资源定位.clone()
        } else {
            资源定位.to_string()
        }
    }

    /// 预取条目后映射一个 LSP Range（无锁纯函数）
    ///
    /// 循环场景（诊断/定义/引用等批量映射）由调用方预取条目一次，
    /// 复用同一文档的列映射，避免每条 range 重复加锁查询缓存。
    pub(super) fn 带条目还原范围(
        翻译条目: Option<&TranslationEntry>,
        跨度: &Value,
    ) -> Value {
        let 映射位置 = |行号: u32, 列号: u32| -> (u32, u32) {
            match 翻译条目 {
                Some(e) => (还原单行(e, 行号), 英文列转中文列(e, 行号, 列号)),
                None => (行号, 列号),
            }
        };
        let 起始行 = 跨度["start"]["line"].as_u64().unwrap_or(0) as u32;
        let 起始列 = 跨度["start"]["character"].as_u64().unwrap_or(0) as u32;
        let 结束行 = 跨度["end"]["line"].as_u64().unwrap_or(0) as u32;
        let 结束列 = 跨度["end"]["character"].as_u64().unwrap_or(0) as u32;

        let (中文起始行, 中文起始列) = 映射位置(起始行, 起始列);
        let (中文结束行, 中文结束列) = 映射位置(结束行, 结束列);

        json!({
            "start": { "line": 中文起始行, "character": 中文起始列 },
            "end": { "line": 中文结束行, "character": 中文结束列 }
        })
    }

    /// 映射一个 LSP Range
    pub fn 还原范围(&self, 资源定位: &str, 跨度: &Value) -> Value {
        // 预取翻译条目一次：起点/终点列映射基于同一文档，一次查询后
        // 走无锁纯函数，避免每处 range 重复锁（诊断/高亮/符号/编辑
        // 每处 2 次 restore_position，每次内部 2-4 次锁获取）
        let 翻译条目 = self
            .缓存
            .按虚拟资源定位查询(资源定位)
            .or_else(|| self.缓存.查询原文(资源定位));
        Self::带条目还原范围(翻译条目.as_deref(), 跨度)
    }

    /// 预取条目后映射一个 LSP Location（无锁纯函数）
    ///
    /// URI 还原与 range 映射共用同一预取条目；循环场景（诊断的
    /// relatedInformation、定义/引用批量映射）由调用方预取避免重复加锁。
    pub(super) fn 带条目还原位置(
        翻译条目: Option<&TranslationEntry>,
        虚拟资源定位: &str,
        目标位置: &Value,
    ) -> Value {
        let 原始资源定位 = match 翻译条目 {
            Some(e) => e.原始资源定位.clone(),
            None => 虚拟资源定位.to_string(),
        };
        let 原始跨度 = Self::带条目还原范围(翻译条目, &目标位置["range"]);

        json!({
            "uri": 原始资源定位,
            "range": 原始跨度
        })
    }

    /// 映射一个 LSP Location（URI + Range）
    pub fn 还原位置(&self, 目标位置: &Value) -> Value {
        let 虚拟资源定位 = 目标位置["uri"].as_str().unwrap_or("");
        let 翻译条目 = self
            .缓存
            .按虚拟资源定位查询(虚拟资源定位)
            .or_else(|| self.缓存.查询原文(虚拟资源定位));
        Self::带条目还原位置(翻译条目.as_deref(), 虚拟资源定位, 目标位置)
    }

    /// 从关键字映射中反向查找：英文 → 中文（预构建表，O(1)）
    pub(super) fn 反向查词(&self, 英文名: &str) -> Option<String> {
        self.反向映射表.get(英文名).cloned()
    }

    /// 将英文代码片段反向翻译为母语
    ///
    /// 精确命中关键字时直接替换；含 snippet 占位符（`${}`）的片段
    /// 保持原样（避免破坏占位符结构）；其余交给引擎词法级反向转译。
    pub(super) fn 翻译代码(&self, 相关文本: &str) -> String {
        if let Some(中文) = self.反向查词(相关文本) {
            return 中文;
        }
        if 相关文本.contains("${") {
            return 相关文本.to_string();
        }
        // 补全片段无文档上下文：不删除任何 crate:: 前缀（宁可保留代理
        // 前缀，也不误删用户手写的前缀——见 转译缓存::逆向转译）
        self.缓存.逆向转译(None, 相关文本)
    }

    /// 映射一个 WorkspaceEdit（changes + documentChanges）
    ///
    /// 编辑目标不是已打开 .zh 的虚拟文件时直接丢弃。
    pub(super) fn 映射编辑(&self, 编辑: &Value, 翻译新文本: bool) -> Value {
        let mut 映射结果 = 编辑.clone();

        // changes: { uri → [TextEdit] }
        if let Some(变更) = 编辑.get("changes").and_then(|v| v.as_object()) {
            let mut 映射变更 = serde_json::Map::new();
            for (资源定位, 编辑列表) in 变更 {
                if !self.是虚拟资源定位(资源定位) {
                    continue;
                }
                let 目标资源定位 = self.还原资源定位(资源定位);
                let 映射编辑结果 = self.映射编辑列表(编辑列表, 资源定位, 翻译新文本);
                映射变更.insert(目标资源定位, 映射编辑结果);
            }
            映射结果["changes"] = Value::Object(映射变更);
        }

        // documentChanges: [TextDocumentEdit]
        if let Some(文档变更) = 编辑.get("documentChanges").and_then(|v| v.as_array()) {
            let 映射文档变更: Vec<Value> = 文档变更
                .iter()
                .filter_map(|项| {
                    let 资源定位 = 项
                        .get("textDocument")
                        .and_then(|文本文档| 文本文档.get("uri"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !self.是虚拟资源定位(资源定位) {
                        return None;
                    }
                    let mut 项映射 = 项.clone();
                    项映射["textDocument"]["uri"] = Value::String(self.还原资源定位(资源定位));
                    if let Some(编辑数组) = 项.get("edits") {
                        项映射["edits"] = self.映射编辑列表(编辑数组, 资源定位, 翻译新文本);
                    }
                    Some(项映射)
                })
                .collect();
            映射结果["documentChanges"] = Value::Array(映射文档变更);
        }

        映射结果
    }

    /// 映射一组 TextEdit 的位置
    ///
    /// 当 `翻译新文本` 为 true 时（重命名/代码操作/补全），
    /// 将 newText 反向翻译为母语；为 false 时 newText 保持英文。
    pub(super) fn 映射编辑列表(
        &self,
        编辑列表: &Value,
        虚拟资源定位: &str,
        翻译新文本: bool,
    ) -> Value {
        let 映射结果: Vec<Value> = 编辑列表
            .as_array()
            .map(|数组| {
                数组
                    .iter()
                    .map(|编辑| {
                        let mut 映射编辑 = 编辑.clone();
                        if let Some(跨度) = 编辑.get("range") {
                            映射编辑["range"] = self.还原范围(虚拟资源定位, 跨度);
                        }
                        if 翻译新文本
                            && let Some(新文本) = 编辑.get("newText").and_then(|v| v.as_str())
                        {
                            映射编辑["newText"] = Value::String(self.翻译代码(新文本));
                        }
                        映射编辑
                    })
                    .collect()
            })
            .unwrap_or_default();
        Value::Array(映射结果)
    }
}

/// 根据翻译条目的行映射还原行号
pub(super) fn 还原单行(翻译条目: &TranslationEntry, 英文行号: u32) -> u32 {
    let 索引 = 英文行号 as usize;
    if 索引 < 翻译条目.行号映射.len() {
        翻译条目.行号映射[索引]
    } else if let Some(&最后) = 翻译条目.行号映射.last() {
        最后
    } else {
        英文行号
    }
}
