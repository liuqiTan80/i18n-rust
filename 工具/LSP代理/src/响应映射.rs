//! 响应映射模块
//!
//! 将 rust-analyzer 响应中的位置信息（指向虚拟 .rs 文件）
//! 还原为原始 .zh 文件的位置。同时处理诊断信息的中文翻译。

use std::sync::Arc;

use serde_json::{json, Value};

use crate::翻译缓存::{翻译缓存, 翻译条目};

/// 响应映射器
///
/// 持有翻译缓存的引用，负责将 rust-analyzer 的各种响应
/// 中的位置/URI 从虚拟文件还原到原始中文文件。
pub struct 响应映射器 {
    缓存: Arc<翻译缓存>,
}

impl 响应映射器 {
    /// 创建新的响应映射器
    pub fn 新建(缓存: Arc<翻译缓存>) -> Self {
        Self { 缓存 }
    }

    /// 判断一个 URI 是否指向我们的虚拟文件
    pub fn 是虚拟URI(&self, URI: &str) -> bool {
        self.缓存.从虚拟URI查询(URI).is_some()
    }

    /// 将虚拟 URI 替换为原始 URI
    pub fn 还原URI(&self, URI: &str) -> String {
        if let Some(条目) = self.缓存.从虚拟URI查询(URI) {
            条目.原始URI
        } else {
            URI.to_string()
        }
    }

    /// 将英文（虚拟文件）行号映射回中文（原始文件）行号
    pub fn 还原行号(&self, URI: &str, 英文行: u32) -> u32 {
        if let Some(条目) = self.缓存.从虚拟URI查询(URI) {
            还原行号_单条(&条目, 英文行)
        } else {
            英文行
        }
    }

    /// 映射一条 LSP 位置（行、列）从虚拟文件到原始文件
    pub fn 还原位置(&self, URI: &str, 行: u32, 列: u32) -> (u32, u32) {
        let 中文行 = self.还原行号(URI, 行);
        // 列号保持不变（当前简化实现：关键字替换不改变列偏移的语义）
        (中文行, 列)
    }

    /// 映射一个 LSP Range
    pub fn 还原范围(&self, URI: &str, 范围: &Value) -> Value {
        let 起始行 = 范围["start"]["line"].as_u64().unwrap_or(0) as u32;
        let 起始列 = 范围["start"]["character"].as_u64().unwrap_or(0) as u32;
        let 结束行 = 范围["end"]["line"].as_u64().unwrap_or(0) as u32;
        let 结束列 = 范围["end"]["character"].as_u64().unwrap_or(0) as u32;

        let (中文起始行, 中文起始列) = self.还原位置(URI, 起始行, 起始列);
        let (中文结束行, 中文结束列) = self.还原位置(URI, 结束行, 结束列);

        json!({
            "start": { "line": 中文起始行, "character": 中文起始列 },
            "end": { "line": 中文结束行, "character": 中文结束列 }
        })
    }

    /// 映射一个 LSP Location（URI + Range）
    pub fn 还原位置对象(&self, 位置: &Value) -> Value {
        let 虚拟URI = 位置["uri"].as_str().unwrap_or("");
        let 原始URI = self.还原URI(虚拟URI);
        let 原始范围 = self.还原范围(虚拟URI, &位置["range"]);

        json!({
            "uri": 原始URI,
            "range": 原始范围
        })
    }

    /// 映射 rust-analyzer 的 publishDiagnostics 通知
    ///
    /// 将诊断信息中的 URI 和位置还原为原始 .zh 文件，
    /// 并尝试翻译诊断消息为中文。
    pub fn 映射诊断(&self, 参数: &Value) -> Value {
        let 虚拟URI = 参数["uri"].as_str().unwrap_or("");
        let 原始URI = self.还原URI(虚拟URI);
        let 诊断列表 = 参数["diagnostics"].as_array();

        let mut 映射后诊断 = Vec::new();

        if let Some(诊断数组) = 诊断列表 {
            for 诊断 in 诊断数组 {
                let mut 映射后 = 诊断.clone();

                // 映射范围
                if 诊断.get("range").is_some() {
                    映射后["range"] = self.还原范围(虚拟URI, &诊断["range"]);
                }

                // 映射 relatedInformation 中的位置
                if let Some(相关信息) = 诊断.get("relatedInformation").and_then(|v| v.as_array()) {
                    let mut 映射后相关 = Vec::new();
                    for 条目 in 相关信息 {
                        let mut 映射后条目 = 条目.clone();
                        if let Some(location) = 条目.get("location") {
                            映射后条目["location"] = self.还原位置对象(location);
                        }
                        映射后相关.push(映射后条目);
                    }
                    映射后["relatedInformation"] = Value::Array(映射后相关);
                }

                // 翻译诊断消息
                映射后["message"] = Value::String(
                    翻译诊断消息(诊断["message"].as_str().unwrap_or(""))
                );

                // 添加教学提示标记
                映射后["source"] = Value::String("i18n-rust".to_string());

                映射后诊断.push(映射后);
            }
        }

        json!({
            "uri": 原始URI,
            "diagnostics": 映射后诊断,
            "version": 参数.get("version").cloned().unwrap_or(Value::Null)
        })
    }

    /// 映射补全响应中的位置信息
    pub fn 映射补全响应(&self, 响应: &Value) -> Value {
        let mut 结果 = 响应.clone();

        // 处理 items 中的 textEdit
        if let Some(条目列表) = 结果.get("items").and_then(|v| v.as_array()) {
            let mut 映射后条目 = Vec::new();
            for item in 条目列表 {
                let mut 映射后 = item.clone();
                if let Some(text_edit) = item.get("textEdit") {
                    // 补全的 textEdit 通常指向当前编辑文件
                    // 此处不改变 URI，只保留原始位置
                    映射后["textEdit"] = text_edit.clone();
                }
                映射后条目.push(映射后);
            }
            结果["items"] = Value::Array(映射后条目);
        }

        结果
    }

    /// 映射定义跳转响应（Location 或 Location[]）
    pub fn 映射定义响应(&self, 响应: &Value) -> Value {
        match 响应 {
            Value::Null => Value::Null,
            Value::Array(数组) => {
                let 映射后: Vec<Value> = 数组.iter()
                    .map(|item| self.还原位置对象(item))
                    .collect();
                Value::Array(映射后)
            }
            Value::Object(_) => {
                // 单个 Location
                self.还原位置对象(响应)
            }
            _ => 响应.clone(),
        }
    }

    /// 映射悬停响应中的位置信息
    pub fn 映射悬停响应(&self, 响应: &Value) -> Value {
        // 悬停响应的 contents 是文本/markdown，不包含位置
        // 但 range 字段需要映射
        let mut 结果 = 响应.clone();
        if let Some(范围) = 响应.get("range") {
            // 悬停的 range 指向触发悬停的位置，使用请求中的 URI
            // 此处保持原样，因为悬停请求的 URI 已经是原始 URI
            结果["range"] = 范围.clone();
        }
        结果
    }

    /// 映射引用响应
    pub fn 映射引用响应(&self, 响应: &Value) -> Value {
        self.映射定义响应(响应) // 与定义跳转格式相同
    }
}

/// 根据翻译条目的行映射还原行号
fn 还原行号_单条(条目: &翻译条目, 英文行: u32) -> u32 {
    let 索引 = 英文行 as usize;
    if 索引 < 条目.行映射.len() {
        条目.行映射[索引]
    } else if let Some(&最后) = 条目.行映射.last() {
        最后
    } else {
        英文行
    }
}

/// 翻译诊断消息为中文
///
/// 尝试匹配常见的 rustc 错误模式并翻译。
/// 完整翻译由核心引擎的 `诊断翻译器` 处理，
/// 此处提供轻量级的关键字替换。
fn 翻译诊断消息(消息: &str) -> String {
    let mut 结果 = 消息.to_string();

    // 常见错误模式翻译
    let 替换表 = [
        ("cannot find value", "找不到变量"),
        ("cannot find type", "找不到类型"),
        ("cannot find function", "找不到函数"),
        ("cannot find module", "找不到模块"),
        ("mismatched types", "类型不匹配"),
        ("type mismatch", "类型不匹配"),
        ("expected", "期望"),
        ("found", "实际为"),
        ("unused variable", "未使用的变量"),
        ("unused import", "未使用的导入"),
        ("unused variable", "未使用的变量"),
        ("cannot borrow", "无法借用"),
        ("borrowed as immutable", "被不可变借用"),
        ("borrowed as mutable", "被可变借用"),
        ("no method named", "没有名为"),
        ("method not found", "方法未找到"),
        ("field", "字段"),
        ("does not implement", "未实现"),
        ("the trait", "特征"),
        ("is not satisfied", "未被满足"),
        ("unresolved import", "未解析的导入"),
        ("file not found", "文件未找到"),
        ("aborting due to", "中止，原因："),
        ("previous error", "前一个错误"),
    ];

    for (英文, 中文) in 替换表 {
        结果 = 结果.replace(英文, 中文);
    }

    // 添加教学提示
    if 消息.contains("mismatched types") || 消息.contains("type mismatch") {
        结果.push_str("\n\n💡 教学提示：Rust 是强类型语言，请确保赋值和函数参数的类型一致。");
    } else if 消息.contains("cannot find") {
        结果.push_str("\n\n💡 教学提示：请检查名称拼写是否正确，以及是否已通过 use 导入。");
    } else if 消息.contains("unused") {
        结果.push_str("\n\n💡 教学提示：未使用的变量可以用下划线 _ 前缀标记，如 _变量名。");
    }

    结果
}

#[cfg(test)]
mod 测试 {
    use super::*;
    use std::collections::HashMap;

    fn 创建测试缓存() -> Arc<翻译缓存> {
        let 映射 = HashMap::from([
            ("函数".into(), "fn".into()),
            ("让".into(), "let".into()),
        ]);
        let 临时 = tempfile::tempdir().unwrap();
        翻译缓存::新建(映射, 临时.into_path())
    }

    #[test]
    fn 测试_还原URI() {
        let 缓存 = 创建测试缓存();
        let 映射器 = 响应映射器::新建(缓存.clone());

        let 条目 = 缓存.更新文档("file:///test/main.zh", "让 x = 1;", 1).unwrap();

        assert_eq!(映射器.还原URI(&条目.虚拟URI), "file:///test/main.zh");
    }

    #[test]
    fn 测试_还原行号() {
        let 缓存 = 创建测试缓存();
        let 映射器 = 响应映射器::新建(缓存.clone());

        let 条目 = 缓存.更新文档("file:///test/main.zh", "让 x = 1;\n让 y = 2;", 1).unwrap();

        assert_eq!(映射器.还原行号(&条目.虚拟URI, 0), 0);
        assert_eq!(映射器.还原行号(&条目.虚拟URI, 1), 1);
    }

    #[test]
    fn 测试_翻译诊断消息() {
        let 结果 = 翻译诊断消息("mismatched types: expected i32, found String");
        assert!(结果.contains("类型不匹配"));
        assert!(结果.contains("教学提示"));
    }

    #[test]
    fn 测试_映射诊断() {
        let 缓存 = 创建测试缓存();
        let 映射器 = 响应映射器::新建(缓存.clone());

        let 条目 = 缓存.更新文档("file:///test/main.zh", "让 x = 1;", 1).unwrap();

        let 诊断参数 = json!({
            "uri": 条目.虚拟URI,
            "diagnostics": [{
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 5 }
                },
                "message": "unused variable",
                "severity": 1
            }],
            "version": null
        });

        let 映射后 = 映射器.映射诊断(&诊断参数);
        assert_eq!(映射后["uri"].as_str().unwrap(), "file:///test/main.zh");
        assert!(映射后["diagnostics"][0]["message"].as_str().unwrap().contains("未使用"));
    }
}
