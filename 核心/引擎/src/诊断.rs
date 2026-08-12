// 模块：诊断
// 功能：解析 rustc 的 JSON 错误输出，结合语言包翻译成中文教学诊断信息

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

// ============================================================
// rustc JSON 诊断数据结构（对应 --error-format=json 输出）
// ============================================================

#[derive(Deserialize, Debug, Clone)]
pub struct 编译器诊断 {
    #[serde(rename = "message")]
    pub 消息: String,
    #[serde(rename = "code")]
    pub 代码: Option<诊断代码>,
    #[serde(rename = "level")]
    pub 级别: String,
    #[serde(rename = "spans")]
    pub 跨度: Vec<诊断跨度>,
    #[serde(rename = "children")]
    pub 子诊断: Vec<编译器诊断>,
    #[serde(rename = "rendered")]
    pub 渲染后: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct 诊断代码 {
    #[serde(rename = "code")]
    pub 代码: String,
    #[serde(rename = "explanation")]
    pub 说明: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct 诊断跨度 {
    #[serde(rename = "file_name")]
    pub 文件名: String,
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
    pub 起始偏移: Option<u64>,
    #[serde(rename = "byte_end")]
    pub 结束偏移: Option<u64>,
    #[serde(rename = "is_primary")]
    pub 是否主要: bool,
    #[serde(rename = "label")]
    pub 标签: Option<String>,
    #[serde(rename = "suggested_replacement")]
    pub 建议修复: Option<String>,
}

// ============================================================
// 错误消息翻译结构
// ============================================================

#[derive(Deserialize, Debug, Clone)]
pub struct 错误消息条目 {
    #[serde(rename = "消息模板")]
    pub 消息模板: String,
    #[serde(rename = "教学提示")]
    pub 教学提示: Option<String>,
}

#[derive(Debug, Clone)]
pub struct 错误翻译管理器 {
    pub 翻译表: HashMap<String, 错误消息条目>,
}

impl 错误翻译管理器 {
    pub fn 从文件加载(路径: &Path) -> Result<Self, String> {
        let 内容 = fs::read_to_string(路径)
            .map_err(|e| format!("无法读取错误消息文件 {}: {}", 路径.display(), e))?;
        let 翻译表: HashMap<String, 错误消息条目> = toml::from_str(&内容)
            .map_err(|e| format!("解析错误消息 TOML 失败: {}", e))?;
        Ok(Self { 翻译表 })
    }

    pub fn 查询(&self, 错误码: &str) -> Option<&错误消息条目> {
        self.翻译表.get(错误码)
    }

    pub fn 覆盖数量(&self) -> usize {
        self.翻译表.len()
    }
}

// ============================================================
// 教学诊断结构（翻译后的输出）
// ============================================================

#[derive(Debug, Clone)]
pub struct 教学诊断 {
    pub 级别: 诊断级别,
    pub 错误码: Option<String>,
    pub 翻译消息: String,
    pub 原始消息: String,
    pub 教学提示: Vec<String>,
    pub 位置: Vec<诊断位置>,
    pub 子诊断: Vec<教学诊断>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum 诊断级别 {
    错误,
    警告,
    注释,
    帮助,
    内部编译器错误,
    未知(String),
}

impl 诊断级别 {
    fn 从字符串(级别: &str) -> Self {
        match 级别 {
            "error" => Self::错误,
            "warning" => Self::警告,
            "note" => Self::注释,
            "help" => Self::帮助,
            "ice" => Self::内部编译器错误,
            其他 => Self::未知(其他.to_string()),
        }
    }

    pub fn 显示文字(&self) -> &str {
        match self {
            Self::错误 => "错误",
            Self::警告 => "警告",
            Self::注释 => "注释",
            Self::帮助 => "帮助",
            Self::内部编译器错误 => "内部编译器错误",
            Self::未知(s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct 诊断位置 {
    pub 文件名: String,
    pub 起始行: u32,
    pub 起始列: u32,
    pub 结束行: u32,
    pub 结束列: u32,
    pub 源码文本: Option<String>,
    pub 标签: Option<String>,
    pub 是否主要: bool,
}

// ============================================================
// 诊断翻译器（增加类型映射支持）
// ============================================================

pub struct 诊断翻译器 {
    翻译管理器: 错误翻译管理器,
    类型映射: HashMap<String, String>, // 来自关键字映射表，用于替换消息中的英文类型
}

impl 诊断翻译器 {
    pub fn 新建(翻译管理器: 错误翻译管理器, 类型映射: HashMap<String, String>) -> Self {
        Self {
            翻译管理器,
            类型映射,
        }
    }

    pub fn 翻译诊断(&self, 诊断: &编译器诊断) -> 教学诊断 {
        let 错误码 = 诊断.代码.as_ref().map(|c| c.代码.clone());

        let 翻译条目 = 错误码
            .as_deref()
            .and_then(|码| self.翻译管理器.查询(码));

        // 从主要 span 的 label 中提取 expected 和 found
        let 主要标签 = 诊断
            .跨度
            .iter()
            .find(|s| s.是否主要)
            .and_then(|s| s.标签.as_deref());

        let (期望值, 实际值) = 主要标签
            .and_then(|标签| {
                if let Some(pos) = 标签.find("expected ") {
                    let 后半段 = &标签[pos + "expected ".len()..];
                    if let Some(逗号位置) = 后半段.find(", found ") {
                        let 期望 = 后半段[..逗号位置].trim().trim_matches('`').to_string();
                        let 实际 = 后半段[逗号位置 + ", found ".len()..].trim().trim_matches('`').to_string();
                        return Some((期望, 实际));
                    }
                }
                None
            })
            .unwrap_or_default();

        // 构建翻译消息
        let 翻译消息 = if let Some(条目) = 翻译条目 {
            let mut 模板 = 条目.消息模板.clone();
            if !期望值.is_empty() && !实际值.is_empty() {
                模板 = 模板.replace("{期望}", &期望值).replace("{实际}", &实际值);
            }
            // 对模板中的类型名进行中文化替换
            self.替换类型名称(模板)
        } else {
            self.替换类型名称(诊断.消息.clone())
        };

        let mut 教学提示 = Vec::new();
        if let Some(条目) = 翻译条目 {
            if let Some(提示) = &条目.教学提示 {
                教学提示.push(提示.clone());
            }
        }
        for 子 in &诊断.子诊断 {
            if 子.级别 == "help" {
                教学提示.push(format!("修复建议：{}", 子.消息));
            }
        }

        let 位置 = 诊断
            .跨度
            .iter()
            .map(|跨度| 诊断位置 {
                文件名: 跨度.文件名.clone(),
                起始行: 跨度.起始行,
                起始列: 跨度.起始列,
                结束行: 跨度.结束行,
                结束列: 跨度.结束列,
                源码文本: 提取源码文本(&跨度.源码文本),
                标签: 跨度.标签.clone(),
                是否主要: 跨度.是否主要,
            })
            .collect();

        let 子诊断 = 诊断
            .子诊断
            .iter()
            .map(|子| self.翻译诊断(子))
            .collect();

        教学诊断 {
            级别: 诊断级别::从字符串(&诊断.级别),
            错误码,
            翻译消息,
            原始消息: 诊断.消息.clone(),
            教学提示,
            位置,
            子诊断,
        }
    }

    /// 使用类型映射替换消息中的英文类型名
    fn 替换类型名称(&self, 消息: String) -> String {
        let mut 结果 = 消息;
        // 按长度降序替换，避免短词错误匹配
        let mut 映射条目: Vec<(&String, &String)> = self.类型映射.iter().collect();
        映射条目.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        for (英文, 中文) in 映射条目 {
            结果 = 结果.replace(英文.as_str(), 中文.as_str());
        }
        结果
    }

    pub fn 批量翻译(&self, 诊断列表: &[编译器诊断]) -> Vec<教学诊断> {
        诊断列表.iter().map(|d| self.翻译诊断(d)).collect()
    }
}

// ============================================================
// JSON 解析与格式化输出
// ============================================================

fn 提取源码文本(text_value: &Option<serde_json::Value>) -> Option<String> {
    text_value.as_ref().and_then(|v| {
        v.as_array()
            .and_then(|arr| arr.first())
            .and_then(|obj| obj.get("text"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
    })
}

pub fn 解析诊断输出(输出: &str) -> Vec<编译器诊断> {
    let mut 结果 = Vec::new();
    for 行 in 输出.lines() {
        let 行 = 行.trim();
        if 行.is_empty() || !行.starts_with('{') {
            continue;
        }
        if let Ok(诊断) = serde_json::from_str::<编译器诊断>(行) {
            结果.push(诊断);
        }
    }
    结果
}

pub struct 格式化诊断 {
    pub 级别文字: String,
    pub 错误码文字: String,
    pub 消息: String,
    pub 位置描述: Vec<String>,
    pub 教学提示: Vec<String>,
}

impl 教学诊断 {
    pub fn 格式化(&self) -> 格式化诊断 {
        let 级别文字 = self.级别.显示文字().to_string();
        let 错误码文字 = self
            .错误码
            .as_ref()
            .map(|码| format!("[{}]", 码))
            .unwrap_or_default();

        let 位置描述 = self
            .位置
            .iter()
            .filter(|p| p.是否主要)
            .map(|p| {
                let mut 描述 = format!("  --> {}:{}:{}", p.文件名, p.起始行, p.起始列);
                if let Some(标签) = &p.标签 {
                    描述 = format!("{}\n      {}", 描述, 标签);
                }
                描述
            })
            .collect();

        格式化诊断 {
            级别文字,
            错误码文字,
            消息: self.翻译消息.clone(),
            位置描述,
            教学提示: self.教学提示.clone(),
        }
    }

    pub fn 格式化为文本(&self) -> String {
        let 格式化 = self.格式化();
        let mut 输出 = String::new();

        // 第一行：错误级别 + 错误码 + 消息
        输出.push_str(&format!(
            "{}{}: {}\n",
            格式化.级别文字, 格式化.错误码文字, 格式化.消息
        ));

        // 位置信息（只显示第一个主要位置）
        if let Some(位置) = self.位置.iter().find(|p| p.是否主要) {
            输出.push_str(&format!("  --> {}:{}:{}\n", 位置.文件名, 位置.起始行, 位置.起始列));
            if let Some(源码) = &位置.源码文本 {
                输出.push_str(&format!("   | {}\n", 源码));
            }
        }

        // 第一条教学提示
        if let Some(提示) = self.教学提示.first() {
            输出.push_str(&format!("💡 {}\n", 提示));
        }

        输出
    }
    
    pub fn 批量格式化为文本(诊断列表: &[教学诊断]) -> String {
        let mut 输出 = String::new();
        for (i, 诊断) in 诊断列表.iter().enumerate() {
            if i > 0 {
                输出.push_str("\n---\n\n");
            }
            输出.push_str(&诊断.格式化为文本());
        }
        输出
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod 测试 {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn 创建错误消息文件(内容: &str) -> NamedTempFile {
        let mut 文件 = NamedTempFile::new().expect("创建临时文件失败");
        write!(文件, "{}", 内容).expect("写入临时文件失败");
        文件
    }

    fn 创建测试诊断() -> 编译器诊断 {
        编译器诊断 {
            消息: "mismatched types".to_string(),
            代码: Some(诊断代码 {
                代码: "E0308".to_string(),
                说明: None,
            }),
            级别: "error".to_string(),
            跨度: vec![诊断跨度 {
                文件名: "src/main.rs".to_string(),
                起始行: 3,
                起始列: 5,
                结束行: 3,
                结束列: 10,
                源码文本: Some(serde_json::json!([{"text": "let x: i32 = \"hello\";", "highlight_start": 1, "highlight_end": 22}])),
                起始偏移: Some(30),
                结束偏移: Some(51),
                是否主要: true,
                标签: Some("expected `i32`, found `&str`".to_string()),
                建议修复: None,
            }],
            子诊断: vec![编译器诊断 {
                消息: "consider using a conversion function".to_string(),
                代码: None,
                级别: "help".to_string(),
                跨度: vec![],
                子诊断: vec![],
                渲染后: None,
            }],
            渲染后: None,
        }
    }

    fn 创建测试类型映射() -> HashMap<String, String> {
        HashMap::from([
            ("u32".into(), "整数32".into()),
            ("i32".into(), "有符号整数32".into()),
            ("f64".into(), "浮点64".into()),
            ("&str".into(), "字符串引用".into()),
            ("String".into(), "字符串".into()),
        ])
    }

    #[test]
    fn 测试加载错误翻译管理器() {
        let toml内容 = r#"
[E0308]
"消息模板" = "类型不匹配：期望 `{期望}`，实际得到 `{实际}`"
"教学提示" = "请检查变量类型是否与上下文要求一致。"

[E0433]
"消息模板" = "未找到类型 `{名称}`"
"教学提示" = "请确认是否已导入所需的模块或类型。"
"#;
        let 文件 = 创建错误消息文件(toml内容);
        let 管理器 = 错误翻译管理器::从文件加载(文件.path()).unwrap();
        assert_eq!(管理器.覆盖数量(), 2);
    }

    #[test]
    fn 测试翻译有语言包并替换类型名() {
        let toml内容 = r#"
[E0308]
"消息模板" = "类型不匹配：期望 `{期望}`，实际得到 `{实际}`"
"教学提示" = "请检查变量类型。"
"#;
        let 文件 = 创建错误消息文件(toml内容);
        let 管理器 = 错误翻译管理器::从文件加载(文件.path()).unwrap();
        let 类型映射 = 创建测试类型映射();
        let 翻译器 = 诊断翻译器::新建(管理器, 类型映射);

        let 诊断 = 创建测试诊断();
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert_eq!(教学.翻译消息, "类型不匹配：期望 `有符号整数32`，实际得到 `字符串引用`");
    }

    #[test]
    fn 测试解析_json诊断输出() {
        let json行 = r#"{"message":"mismatched types","code":{"code":"E0308"},"level":"error","spans":[],"children":[],"rendered":null}"#;
        let 输出 = format!("{}\n{}", "    Compiling test v0.1.0", json行);
        let 诊断列表 = 解析诊断输出(&输出);
        assert_eq!(诊断列表.len(), 1);
        assert_eq!(诊断列表[0].消息, "mismatched types");
    }

    #[test]
    fn 测试诊断级别转换() {
        assert_eq!(诊断级别::从字符串("error"), 诊断级别::错误);
        assert_eq!(诊断级别::从字符串("warning"), 诊断级别::警告);
        assert_eq!(诊断级别::从字符串("help"), 诊断级别::帮助);
    }
}