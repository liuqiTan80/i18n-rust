// rustc JSON 诊断数据结构（对应 --error-format=json 输出）。

use serde::Deserialize;

/// rustc 编译器诊断信息（对应一行 JSON 输出）
#[derive(Deserialize, Debug, Clone)]
pub struct 编译器诊断 {
    #[serde(rename = "message")]
    pub 诊断消息: String,
    #[serde(rename = "code")]
    pub 诊断码: Option<诊断码>,
    #[serde(rename = "level")]
    pub 诊断级别: String,
    #[serde(rename = "spans")]
    pub 跨度列表: Vec<诊断跨度>,
    #[serde(rename = "children")]
    pub 子诊断: Vec<Self>,
    #[serde(rename = "rendered")]
    pub 预渲染文本: Option<String>,
}

/// 诊断错误码（如 E0308）
#[derive(Deserialize, Debug, Clone)]
pub struct 诊断码 {
    #[serde(rename = "code")]
    pub 码值: String,
    #[serde(rename = "explanation")]
    pub 解释: Option<String>,
}

/// 诊断跨度（代码位置信息）
#[derive(Deserialize, Debug, Clone)]
pub struct 诊断跨度 {
    #[serde(rename = "file_name")]
    pub 源文件名: String,
    #[serde(rename = "line_start")]
    pub 起始行: u32,
    #[serde(rename = "column_start")]
    pub 起始列: u32,
    #[serde(rename = "line_end")]
    pub 结束行: u32,
    #[serde(rename = "column_end")]
    pub 结束列: u32,
    #[serde(rename = "text")]
    pub 源码文本: Option<serde_json::Value>,
    #[serde(rename = "byte_start")]
    pub 起始字节: Option<u64>,
    #[serde(rename = "byte_end")]
    pub 结束字节: Option<u64>,
    #[serde(rename = "is_primary")]
    pub 是主跨度: bool,
    #[serde(rename = "label")]
    pub 标签: Option<String>,
    #[serde(rename = "suggested_replacement")]
    pub 建议替换: Option<String>,
}

/// 从 serde_json::Value 中提取源码文本
pub(super) fn 抽取源码文本(文本值: &Option<serde_json::Value>) -> Option<String> {
    文本值.as_ref().and_then(|数据| {
        数据
            .as_array()
            .and_then(|数组| 数组.first())
            .and_then(|对象| 对象.get("text"))
            .and_then(|文本段| 文本段.as_str())
            .map(|串| 串.to_string())
    })
}
