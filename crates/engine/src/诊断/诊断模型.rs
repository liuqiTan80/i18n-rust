//! rustc 编译器诊断的 JSON 数据结构（对应其按 JSON 格式输出的诊断记录）。

use serde::Deserialize;

/// rustc 编译器诊断信息（对应一行 JSON 输出）
#[derive(Deserialize, Debug, Clone)]
pub struct 编译器诊断 {
    #[serde(rename = "message")]
    pub 诊断消息: String,
    #[serde(rename = "code")]
    pub 诊断码: Option<诊断代码>,
    #[serde(rename = "level")]
    pub 级别: String,
    #[serde(rename = "spans")]
    pub 跨度集: Vec<诊断跨度>,
    #[serde(rename = "children")]
    pub 子诊断: Vec<编译器诊断>,
    #[serde(rename = "rendered")]
    pub 已渲染: Option<String>,
}

/// 诊断错误码（如 E0308）
#[derive(Deserialize, Debug, Clone)]
pub struct 诊断代码 {
    #[serde(rename = "code")]
    pub 码值: String,
    #[serde(rename = "explanation")]
    pub 释义: Option<String>,
}

/// 诊断跨度（代码位置信息）
#[derive(Deserialize, Debug, Clone)]
pub struct 诊断跨度 {
    #[serde(rename = "file_name")]
    pub 文件名称: String,
    #[serde(rename = "line_start")]
    pub 起始行号: u32,
    #[serde(rename = "column_start")]
    pub 起始列号: u32,
    #[serde(rename = "line_end")]
    pub 结束行号: u32,
    #[serde(rename = "column_end")]
    pub 结束列号: u32,
    #[serde(rename = "text")]
    pub 源码文本: Option<serde_json::Value>,
    #[serde(rename = "byte_start")]
    pub 起始字节: Option<u64>,
    #[serde(rename = "byte_end")]
    pub 结束字节: Option<u64>,
    #[serde(rename = "is_primary")]
    pub 是否主因: bool,
    #[serde(rename = "label")]
    pub 标签文本: Option<String>,
    #[serde(rename = "suggested_replacement")]
    pub 建议替换: Option<String>,
}

/// 从序列化的 JSON 值中提取源码文本
pub(super) fn 取源码文本(文本值: &Option<serde_json::Value>) -> Option<String> {
    文本值.as_ref().and_then(|包装| {
        包装
            .as_array()
            .and_then(|数组| 数组.first())
            .and_then(|对象| 对象.get("text"))
            .and_then(|片段| 片段.as_str())
            .map(|取值| 取值.to_string())
    })
}
