//! 响应映射模块
//!
//! 将 rust-analyzer 响应中的位置信息（指向虚拟 .rs 文件）
//! 还原为原始 .zh 文件的位置。同时处理诊断信息的中文翻译。
//!
//! 子模块划分：
//! - [`响应`]：各 LSP 请求类型的响应映射（悬停/补全/符号/编辑等）
//! - [`诊断文本`]：诊断消息翻译与所有权详情提取（CLI 同源消息表）
//!
//! 本文件保留映射基础设施：资源标识/行/列还原、编辑映射与反向转译。

#[path = "响应.rs"]
mod 响应;
#[path = "诊断文本.rs"]
pub(crate) mod 诊断文本;

#[cfg(test)]
#[path = "测试.rs"]
mod 母语测试;

pub use 诊断文本::初始化诊断翻译器;

use std::sync::Arc;

use serde_json::Value;

use crate::翻译缓存::{翻译条目, 英文列转母语列, 转译缓存};

/// 响应映射器
///
/// 持有翻译缓存的引用，负责将 rust-analyzer 的各种响应
/// 中的位置/资源标识从虚拟文件还原到原始中文文件。
pub struct 响应映射器 {
    缓存: Arc<转译缓存>,
    /// 预构建的反向映射表（英文 → 母语），常数时间查找且结果确定
    反向映射: std::collections::HashMap<String, String>,
    /// 是否启用补全语言过滤（禁止串语言）
    ///
    /// 方言关键字与英文存在差异时启用（zh/ja/ru 等）；
    /// 英文包为恒等映射（关键字与英文相同），不过滤。
    严格母语过滤: bool,
}

impl 响应映射器 {
    /// 创建新的响应映射器
    ///
    /// 反向表直接复用 转译缓存 构造时预构建的合并表
    /// （关键字反转后合并别名反转，多对一冲突保留排序最小者，
    /// 结果确定），避免每次构造映射器重复构建。
    pub fn 新建(缓存: Arc<转译缓存>) -> Self {
        // 英文语言包为恒等映射（键等于值），无需也无法做语言过滤
        let 严格母语过滤 = 缓存
            .关键词映射()
            .iter()
            .any(|(母语词形, 英文词形)| 母语词形 != 英文词形);
        Self {
            反向映射: 缓存.反向映射引用().clone(),
            缓存,
            严格母语过滤,
        }
    }

    /// 判断一个资源标识是否指向我们的虚拟文件
    pub fn 是虚拟资源标识(&self, 资源标识: &str) -> bool {
        self.缓存.按虚拟资源标识查询(资源标识).is_some()
    }

    /// 按方言资源标识查询翻译条目（全角标点教学诊断合并等场景）
    pub fn 原始条目(&self, 资源标识: &str) -> Option<Arc<翻译条目>> {
        self.缓存.查询原始(资源标识)
    }

    /// 获取教学检查已知词表（转发 转译缓存，易混方法名提示用）
    pub fn 教学词表(&self) -> &std::collections::HashSet<String> {
        self.缓存.校验词表()
    }

    /// 获取教学检查的分桶索引（转发 转译缓存，
    /// 教学诊断发布路径的一次扫描入口用，索引随语言包缓存一次）
    pub fn 教学词分桶(&self) -> &i18n_rust_engine::校验::词形分桶 {
        self.缓存.校验词分桶()
    }

    /// 获取映射管理器的共享句柄（cargo 检查线程跨线程取词表用）
    pub fn 管理器共享(&self) -> Arc<i18n_rust_engine::映射管理器::映射管理器> {
        self.缓存.管理器共享句柄()
    }

    /// 获取教学检查歧义构造器词集（转发 转译缓存，
    /// 「未标注类型」仅提示无法自行推导的新建/缺省/收集形态）
    pub fn 歧义构造器词集(&self) -> &std::collections::HashSet<String> {
        self.缓存.歧义构造器词()
    }

    /// 获取英文回落检查反向索引（转发 转译缓存，
    /// 方言源码里写成英文且有母语词条的结构词提示用）
    pub fn 英文回落映射(&self) -> &std::collections::HashMap<String, String> {
        self.缓存.英文回落映射()
    }

    /// 获取关键字映射表（转发 转译缓存，
    /// 保留字撞名提示：声明名字转译后是否为 Rust 保留字）
    pub fn 关键词映射(&self) -> &std::collections::HashMap<String, String> {
        self.缓存.关键词映射()
    }

    /// 将虚拟资源标识替换为原始资源标识
    pub fn 还原资源标识(&self, 资源标识: &str) -> String {
        if let Some(条目值) = self.缓存.按虚拟资源标识查询(资源标识) {
            条目值.原始资源标识.clone()
        } else {
            资源标识.to_string()
        }
    }

    /// 预取条目后映射一个 LSP 范围值（无锁纯函数）
    ///
    /// 循环场景（诊断/定义/引用等批量映射）由调用方预取条目一次，
    /// 复用同一文档的列映射，避免每条范围值重复加锁查询缓存。
    pub(super) fn 带条目还原范围(
        条目值: Option<&翻译条目>, 范围值: &Value
    ) -> Value {
        let 映射位置 = |行号: u32, 列号: u32| -> (u32, u32) {
            match 条目值 {
                Some(条目) => (还原单行(条目, 行号), 英文列转母语列(条目, 行号, 列号)),
                None => (行号, 列号),
            }
        };
        let 起始行 = 范围值["start"]["line"].as_u64().unwrap_or(0) as u32;
        let 起始列 = 范围值["start"]["character"].as_u64().unwrap_or(0) as u32;
        let 结束行 = 范围值["end"]["line"].as_u64().unwrap_or(0) as u32;
        let 结束列 = 范围值["end"]["character"].as_u64().unwrap_or(0) as u32;

        let (母语起始行, 母语起始列) = 映射位置(起始行, 起始列);
        let (母语结束行, 母语结束列) = 映射位置(结束行, 结束列);

        serde_json::json!({
            "start": { "line": 母语起始行, "character": 母语起始列 },
            "end": { "line": 母语结束行, "character": 母语结束列 }
        })
    }

    /// 映射一个 LSP 范围值
    pub fn 还原范围(&self, 资源标识: &str, 范围值: &Value) -> Value {
        // 预取翻译条目一次：起点/终点列映射基于同一文档，一次查询后
        // 走无锁纯函数，避免每处范围值重复锁（诊断/高亮/符号/编辑
        // 每处 2 次还原位置，每次内部 2-4 次锁获取）
        let 条目值 = self
            .缓存
            .按虚拟资源标识查询(资源标识)
            .or_else(|| self.缓存.查询原始(资源标识));
        Self::带条目还原范围(条目值.as_deref(), 范围值)
    }

    /// 预取条目后映射一个 LSP 位置值（无锁纯函数）
    ///
    /// 资源标识还原与范围值映射共用同一预取条目；循环场景（诊断的
    /// 关联信息、定义/引用批量映射）由调用方预取避免重复加锁。
    pub(super) fn 带条目还原位置(
        条目值: Option<&翻译条目>,
        虚拟资源标识: &str,
        位置值: &Value,
    ) -> Value {
        let 原始资源标识 = match 条目值 {
            Some(条目) => 条目.原始资源标识.clone(),
            None => 虚拟资源标识.to_string(),
        };
        let 原始范围 = Self::带条目还原范围(条目值, &位置值["range"]);

        serde_json::json!({
            "uri": 原始资源标识,
            "range": 原始范围
        })
    }

    /// 映射一个 LSP 位置值（资源标识 + 范围值）
    pub fn 还原位置(&self, 位置值: &Value) -> Value {
        let 虚拟资源标识 = 位置值["uri"].as_str().unwrap_or("");
        let 条目值 = self
            .缓存
            .按虚拟资源标识查询(虚拟资源标识)
            .or_else(|| self.缓存.查询原始(虚拟资源标识));
        Self::带条目还原位置(条目值.as_deref(), 虚拟资源标识, 位置值)
    }

    /// 从关键字映射中反向查找：英文 → 中文（预构建表，常数时间）
    pub(super) fn 反向查找(&self, 英文名: &str) -> Option<String> {
        self.反向映射.get(英文名).cloned()
    }

    /// 将英文代码片段反向翻译为母语
    ///
    /// 精确命中关键字时直接替换；含代码片段占位符（`${}`）的片段
    /// 保持原样（避免破坏占位符结构）；其余交给引擎词法级反向转译。
    pub(super) fn 翻译代码(&self, 文本串: &str) -> String {
        if let Some(母语词形) = self.反向查找(文本串) {
            return 母语词形;
        }
        if 文本串.contains("${") {
            return 文本串.to_string();
        }
        // 补全片段无文档上下文：不删除任何包路径前缀（宁可保留代理
        // 前缀，也不误删用户手写的前缀——见 转译缓存::反向转译）
        self.缓存.反向转译(None, 文本串)
    }

    /// 映射一个工作区编辑（变更表与文档变更）
    ///
    /// 编辑目标不是已打开 .zh 的虚拟文件时直接丢弃。
    pub(super) fn 映射编辑(&self, 编辑值: &Value, 翻译新文本: bool) -> Value {
        let mut 映射结果 = 编辑值.clone();

        // 变更表：{ 资源标识 → [文本编辑] }
        if let Some(变更表) = 编辑值.get("changes").and_then(|值| 值.as_object()) {
            let mut 映射后变更 = serde_json::Map::new();
            for (资源标识, 编辑列表) in 变更表 {
                if !self.是虚拟资源标识(资源标识) {
                    continue;
                }
                let 目标资源标识 = self.还原资源标识(资源标识);
                let 映射后编辑 = self.映射编辑列表(编辑列表, 资源标识, 翻译新文本);
                映射后变更.insert(目标资源标识, 映射后编辑);
            }
            映射结果["changes"] = Value::Object(映射后变更);
        }

        // 文档变更： [文本编辑]
        if let Some(文档变更) = 编辑值.get("documentChanges").and_then(|值| 值.as_array())
        {
            let 映射后文档变更: Vec<Value> = 文档变更
                .iter()
                .filter_map(|项| {
                    let 资源标识 = 项
                        .get("textDocument")
                        .and_then(|文档| 文档.get("uri"))
                        .and_then(|值| 值.as_str())
                        .unwrap_or("");
                    if !self.是虚拟资源标识(资源标识) {
                        return None;
                    }
                    let mut 映射项 = 项.clone();
                    映射项["textDocument"]["uri"] = Value::String(self.还原资源标识(资源标识));
                    if let Some(编辑) = 项.get("edits") {
                        映射项["edits"] = self.映射编辑列表(编辑, 资源标识, 翻译新文本);
                    }
                    Some(映射项)
                })
                .collect();
            映射结果["documentChanges"] = Value::Array(映射后文档变更);
        }

        映射结果
    }

    /// 映射一组文本编辑的位置
    ///
    /// 当 `翻译新文本` 为真时（重命名/代码操作/补全），
    /// 将新文本反向翻译为母语；为假时新文本保持英文。
    pub(super) fn 映射编辑列表(
        &self,
        编辑列表: &Value,
        虚拟资源标识: &str,
        翻译新文本: bool,
    ) -> Value {
        let 映射结果: Vec<Value> = 编辑列表
            .as_array()
            .map(|数组| {
                数组
                    .iter()
                    .map(|编辑| {
                        let mut 映射项 = 编辑.clone();
                        if let Some(范围值) = 编辑.get("range") {
                            映射项["range"] = self.还原范围(虚拟资源标识, 范围值);
                        }
                        if 翻译新文本
                            && let Some(新文本) = 编辑.get("newText").and_then(|值| 值.as_str())
                        {
                            映射项["newText"] = Value::String(self.翻译代码(新文本));
                        }
                        映射项
                    })
                    .collect()
            })
            .unwrap_or_default();
        Value::Array(映射结果)
    }
}

/// 根据翻译条目的行映射还原行号
pub(super) fn 还原单行(条目值: &翻译条目, 英文行号: u32) -> u32 {
    let 下标 = 英文行号 as usize;
    if 下标 < 条目值.行映射.len() {
        条目值.行映射[下标]
    } else if let Some(&末行) = 条目值.行映射.last() {
        末行
    } else {
        英文行号
    }
}
