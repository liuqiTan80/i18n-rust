// 教学诊断结构与所有权叙事化详情

use serde::Serialize;

use super::诊断模型::{抽取源码文本, 编译器诊断, 诊断跨度};

/// 教学诊断：翻译后的完整诊断信息，包含错误码、翻译消息、教学提示、位置等
#[derive(Debug, Clone)]
pub struct 教学诊断 {
    pub 严重级别: 诊断级别,
    pub 错误码: Option<String>,
    pub 翻译消息: String,
    pub 原始消息: String,
    pub 教学提示: Vec<String>,
    pub 位置列表: Vec<诊断位置>,
    pub 子诊断: Vec<教学诊断>,
    pub 所有权详情: Option<所有权详情>,
}

/// 诊断级别
#[derive(Debug, Clone, PartialEq)]
pub enum 诊断级别 {
    错误级,
    警告级,
    注释级,
    帮助级,
    编译器崩溃级,
    未知级(String),
}

impl 诊断级别 {
    /// 从 rustc 的 level 字符串解析诊断级别
    pub(super) fn 从文本解析(级别文本: &str) -> Self {
        match 级别文本 {
            "error" => Self::错误级,
            "warning" => Self::警告级,
            "note" => Self::注释级,
            "help" => Self::帮助级,
            "ice" => Self::编译器崩溃级,
            其它 => Self::未知级(其它.to_string()),
        }
    }

    /// 返回当前语言下的诊断级别显示文字
    pub fn 显示文字(&self) -> String {
        let 键名 = match self {
            Self::错误级 => "diag_kind_error",
            Self::警告级 => "diag_kind_warning",
            Self::注释级 => "diag_kind_note",
            Self::帮助级 => "diag_kind_help",
            Self::编译器崩溃级 => "diag_kind_ice",
            Self::未知级(串) => return 串.clone(),
        };
        crate::语言::查句(键名)
    }
}

/// 诊断位置信息（翻译后的跨度）
#[derive(Debug, Clone, Serialize)]
pub struct 诊断位置 {
    pub 源文件名: String,
    #[serde(rename = "起始行")]
    pub 起始行: u32,
    #[serde(rename = "起始列")]
    pub 起始列: u32,
    #[serde(rename = "结束行")]
    pub 结束行: u32,
    #[serde(rename = "结束列")]
    pub 结束列: u32,
    pub 源码文本: Option<String>,
    pub 标签: Option<String>,
    pub 是主跨度: bool,
}

impl 诊断位置 {
    /// 从 rustc 的原始跨度构造诊断位置
    pub fn 从跨度构造(跨度: &诊断跨度) -> Self {
        Self {
            源文件名: 跨度.源文件名.clone(),
            起始行: 跨度.起始行,
            起始列: 跨度.起始列,
            结束行: 跨度.结束行,
            结束列: 跨度.结束列,
            源码文本: 抽取源码文本(&跨度.源码文本),
            标签: 跨度.标签.clone(),
            是主跨度: 跨度.是主跨度,
        }
    }
}

/// 所有权错误的叙事化详情
#[derive(Debug, Clone, Serialize)]
pub struct 所有权详情 {
    #[serde(rename = "变量名")]
    pub 变量名: String,
    #[serde(rename = "移动发生")]
    pub 移动发生: Option<诊断位置>,
    #[serde(rename = "借用发生")]
    pub 借用发生: Option<诊断位置>,
    #[serde(rename = "再次使用")]
    pub 再次使用: Option<诊断位置>,
}

impl 所有权详情 {
    /// 生成当前语言下的叙事性教学文本
    pub fn 叙事文本(&self) -> String {
        let 变量 = &self.变量名;
        match (&self.移动发生, &self.借用发生, &self.再次使用) {
            (Some(移动位), None, Some(复用)) => crate::语言::查译(
                "diag_ownership_moved_reused",
                &[变量, &移动位.起始行.to_string(), &复用.起始行.to_string()],
            ),
            (None, Some(借用), Some(复用)) => crate::语言::查译(
                "diag_ownership_borrowed_in_use",
                &[变量, &借用.起始行.to_string(), &复用.起始行.to_string()],
            ),
            (Some(移动位), _, _) => {
                crate::语言::查译("diag_ownership_moved", &[变量, &移动位.起始行.to_string()])
            }
            (None, Some(借用), _) => {
                crate::语言::查译("diag_ownership_borrowed", &[变量, &借用.起始行.to_string()])
            }
            _ => crate::语言::查译("diag_ownership_conflict", &[变量]),
        }
    }
}

/// 所有权相关错误码
pub const 所有权错误码: [&str; 3] = ["E0382", "E0502", "E0507"];

/// 从 rustc 诊断中提取所有权错误详情
pub fn 抽取所有权详情(错误码: &str, 诊断: &编译器诊断) -> Option<所有权详情> {
    if !所有权错误码.contains(&错误码) {
        return None;
    }
    let 变量名 =
        从消息抽取变量名(&诊断.诊断消息).or_else(|| 从跨度抽取变量名(&诊断.跨度列表))?;

    // 收集顶层与子诊断中的所有跨度
    let 全部跨度: Vec<&诊断跨度> = 诊断
        .跨度列表
        .iter()
        .chain(诊断.子诊断.iter().flat_map(|子| 子.跨度列表.iter()))
        .collect();

    let mut 移动发生 = None;
    let mut 借用发生 = None;
    let mut 再次使用 = None;
    for 跨度 in &全部跨度 {
        let 标签 = 跨度.标签.as_deref().unwrap_or("");
        // 注意顺序："borrow later used here" 同时含 borrow 与 used here，应归为再次使用
        if 标签.contains("used here") || 标签.contains("later used") || 标签.contains("after move")
        {
            再次使用.get_or_insert_with(|| 诊断位置::从跨度构造(跨度));
        } else if 标签.contains("move") {
            移动发生.get_or_insert_with(|| 诊断位置::从跨度构造(跨度));
        } else if 标签.contains("borrow") {
            借用发生.get_or_insert_with(|| 诊断位置::从跨度构造(跨度));
        }
    }

    // 主 span 兜底：对应错误类型的核心位置
    if let Some(主跨度) = 诊断.跨度列表.iter().find(|s| s.是主跨度) {
        match 错误码 {
            "E0382" => {
                再次使用.get_or_insert_with(|| 诊断位置::从跨度构造(主跨度));
            }
            "E0502" => {
                借用发生.get_or_insert_with(|| 诊断位置::从跨度构造(主跨度));
            }
            "E0507" => {
                移动发生.get_or_insert_with(|| 诊断位置::从跨度构造(主跨度));
            }
            _ => {}
        }
    }

    if 移动发生.is_none() && 借用发生.is_none() && 再次使用.is_none() {
        return None;
    }
    Some(所有权详情 {
        变量名,
        移动发生,
        借用发生,
        再次使用,
    })
}

/// 从诊断消息中提取反引号包裹的变量名
fn 从消息抽取变量名(消息文本: &str) -> Option<String> {
    let 起点位置 = 消息文本.find('`')?;
    let 剩余 = &消息文本[起点位置 + 1..];
    let 终点位置 = 剩余.find('`')?;
    Some(剩余[..终点位置].to_string())
}

/// 从 span 标签中提取反引号包裹的变量名
fn 从跨度抽取变量名(跨度列表: &[诊断跨度]) -> Option<String> {
    跨度列表.iter().find_map(|跨度| {
        let 标签 = 跨度.标签.as_deref()?;
        let 起点位置 = 标签.find('`')?;
        let 剩余 = &标签[起点位置 + 1..];
        let 终点位置 = 剩余.find('`')?;
        Some(剩余[..终点位置].to_string())
    })
}
