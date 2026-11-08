//! 教学诊断结构（翻译后的输出）与所有权错误叙事化详情。

use serde::Serialize;

use super::诊断模型::{取源码文本, 编译器诊断, 诊断跨度};

/// 教学诊断：翻译后的完整诊断信息，包含错误码、翻译消息、教学提示、位置等
#[derive(Debug, Clone)]
pub struct 教学诊断 {
    pub 级别: 诊断级别,
    pub 错误码: Option<String>,
    pub 译文消息: String,
    pub 原文消息: String,
    pub 提示集: Vec<String>,
    pub 位置集: Vec<诊断位置>,
    pub 子诊断: Vec<教学诊断>,
    pub 所有权详情: Option<所有权详情>,
}

/// 诊断级别
#[derive(Debug, Clone, PartialEq)]
pub enum 诊断级别 {
    诊断错误,
    诊断告警,
    诊断注记,
    诊断建议,
    诊断崩溃,
    未知级别(String),
}

impl 诊断级别 {
    /// 把 rustc 输出的严重程度字符串解析成诊断级别
    pub(super) fn 解析级别(级别: &str) -> Self {
        match 级别 {
            "error" => Self::诊断错误,
            "warning" => Self::诊断告警,
            "note" => Self::诊断注记,
            "help" => Self::诊断建议,
            "ice" => Self::诊断崩溃,
            其余 => Self::未知级别(其余.to_string()),
        }
    }

    /// 返回当前语言下的诊断级别显示文字
    pub fn 显示文字(&self) -> String {
        let 键 = match self {
            Self::诊断错误 => "diag_kind_error",
            Self::诊断告警 => "diag_kind_warning",
            Self::诊断注记 => "diag_kind_note",
            Self::诊断建议 => "diag_kind_help",
            Self::诊断崩溃 => "diag_kind_ice",
            Self::未知级别(串) => return 串.clone(),
        };
        crate::语言::取消息(键)
    }
}

/// 诊断位置信息（翻译后的跨度）
#[derive(Debug, Clone, Serialize)]
pub struct 诊断位置 {
    #[serde(rename = "file_name")]
    pub 文件名称: String,
    #[serde(rename = "起始行")]
    pub 起始行号: u32,
    #[serde(rename = "起始列")]
    pub 起始列号: u32,
    #[serde(rename = "结束行")]
    pub 结束行号: u32,
    #[serde(rename = "结束列")]
    pub 结束列号: u32,
    #[serde(rename = "source_text")]
    pub 源码文本: Option<String>,
    #[serde(rename = "label")]
    pub 标签文本: Option<String>,
    #[serde(rename = "is_primary")]
    pub 是否主因: bool,
}

impl 诊断位置 {
    /// 从 rustc 的原始跨度构造诊断位置
    pub fn 自跨度(跨度: &诊断跨度) -> Self {
        Self {
            文件名称: 跨度.文件名称.clone(),
            起始行号: 跨度.起始行号,
            起始列号: 跨度.起始列号,
            结束行号: 跨度.结束行号,
            结束列号: 跨度.结束列号,
            源码文本: 取源码文本(&跨度.源码文本),
            标签文本: 跨度.标签文本.clone(),
            是否主因: 跨度.是否主因,
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
    /// E0502 的冲突双方（仅当两个借用标签都在跨度中时才有值）
    #[serde(rename = "借用冲突", skip_serializing_if = "Option::is_none")]
    pub 借用冲突: Option<借用冲突>,
}

/// E0502 的借用冲突双方
#[derive(Debug, Clone, Serialize)]
pub struct 借用冲突 {
    #[serde(rename = "可变借用")]
    pub 可变方: 诊断位置,
    #[serde(rename = "不可变借用")]
    pub 不可变方: 诊断位置,
}

impl 借用冲突 {
    /// 按标签把两端位置分开收：返回 `(可变借用, 不可变借用)`
    pub fn 归类带标签跨度<'甲, 项流>(
        跨度: 项流,
    ) -> (Option<诊断位置>, Option<诊断位置>)
    where
        项流: IntoIterator<Item = (&'甲 str, 诊断位置)>,
    {
        let mut 可变方 = None;
        let mut 不可变方 = None;
        for (标签, 位置) in 跨度 {
            if 不可变方.is_none() && 标签.contains("immutable borrow") {
                不可变方 = Some(位置);
            } else if 可变方.is_none()
                && 标签.contains("mutable borrow")
                && !标签.contains("immutable")
            {
                可变方 = Some(位置);
            }
        }
        (可变方, 不可变方)
    }

    /// 两端都认出来时才算得上「借用冲突」
    pub fn 自两端(
        可变方: Option<诊断位置>, 不可变方: Option<诊断位置>
    ) -> Option<Self> {
        match (可变方, 不可变方) {
            (Some(可变方), Some(不可变方)) => Some(Self {
                可变方, 不可变方
            }),
            _ => None,
        }
    }

    /// 从「标签 + 位置」序列里认出冲突双方
    pub fn 自带标签跨度<'甲, 项流>(跨度: 项流) -> Option<Self>
    where
        项流: IntoIterator<Item = (&'甲 str, 诊断位置)>,
    {
        let (可变方, 不可变方) = Self::归类带标签跨度(跨度);
        Self::自两端(可变方, 不可变方)
    }
}

impl 所有权详情 {
    /// 生成当前语言下的叙事性教学文本
    pub fn 叙事文本(&self) -> String {
        let 变量 = &self.变量名;
        if let Some(冲突) = &self.借用冲突 {
            return crate::语言::取带参消息(
                "diag_ownership_borrow_conflict",
                &[
                    变量,
                    &冲突.可变方.起始行号.to_string(),
                    &冲突.不可变方.起始行号.to_string(),
                ],
            );
        }
        match (&self.移动发生, &self.借用发生, &self.再次使用) {
            (Some(移动处), None, Some(复用处)) => crate::语言::取带参消息(
                "diag_ownership_moved_reused",
                &[
                    变量,
                    &移动处.起始行号.to_string(),
                    &复用处.起始行号.to_string(),
                ],
            ),
            (None, Some(借用处), Some(复用处)) => crate::语言::取带参消息(
                "diag_ownership_borrowed_in_use",
                &[
                    变量,
                    &借用处.起始行号.to_string(),
                    &复用处.起始行号.to_string(),
                ],
            ),
            (Some(移动处), _, _) => crate::语言::取带参消息(
                "diag_ownership_moved",
                &[变量, &移动处.起始行号.to_string()],
            ),
            (None, Some(借用处), _) => crate::语言::取带参消息(
                "diag_ownership_borrowed",
                &[变量, &借用处.起始行号.to_string()],
            ),
            _ => crate::语言::取带参消息("diag_ownership_conflict", &[变量]),
        }
    }
}

/// 所有权相关错误码（E0382 / E0502 / E0507）
pub const 所有权错误码表: [&str; 3] = ["E0382", "E0502", "E0507"];

/// 从 rustc 诊断中提取所有权错误详情
pub fn 提取所有权详情(错误码: &str, 诊断: &编译器诊断) -> Option<所有权详情> {
    if !所有权错误码表.contains(&错误码) {
        return None;
    }
    let 变量名 = 从消息取变量名(&诊断.诊断消息).or_else(|| 从跨度取变量名(&诊断.跨度集))?;

    let 全部跨度: Vec<&诊断跨度> = 诊断
        .跨度集
        .iter()
        .chain(诊断.子诊断.iter().flat_map(|子| 子.跨度集.iter()))
        .collect();

    let mut 移动发生 = None;
    let mut 借用发生 = None;
    let mut 再次使用 = None;
    for 跨度 in &全部跨度 {
        let 标签 = 跨度.标签文本.as_deref().unwrap_or("");
        if 标签.contains("used here") || 标签.contains("later used") || 标签.contains("after move")
        {
            再次使用.get_or_insert_with(|| 诊断位置::自跨度(跨度));
        } else if 标签.contains("move") {
            移动发生.get_or_insert_with(|| 诊断位置::自跨度(跨度));
        } else if 标签.contains("borrow") {
            借用发生.get_or_insert_with(|| 诊断位置::自跨度(跨度));
        }
    }

    if let Some(主因) = 诊断.跨度集.iter().find(|跨度| 跨度.是否主因) {
        match 错误码 {
            "E0382" => {
                再次使用.get_or_insert_with(|| 诊断位置::自跨度(主因));
            }
            "E0502" => {
                借用发生.get_or_insert_with(|| 诊断位置::自跨度(主因));
            }
            "E0507" => {
                移动发生.get_or_insert_with(|| 诊断位置::自跨度(主因));
            }
            _ => {}
        }
    }

    if 移动发生.is_none() && 借用发生.is_none() && 再次使用.is_none() {
        return None;
    }
    let 借用冲突 = if 错误码 == "E0502" {
        借用冲突::自带标签跨度(全部跨度.iter().filter_map(|跨度| {
            跨度
                .标签文本
                .as_deref()
                .map(|标签| (标签, 诊断位置::自跨度(跨度)))
        }))
    } else {
        None
    };
    Some(所有权详情 {
        变量名,
        移动发生,
        借用发生,
        再次使用,
        借用冲突,
    })
}

/// 从诊断消息中提取反引号包裹的变量名
fn 从消息取变量名(消息: &str) -> Option<String> {
    let 首 = 消息.find('`')?;
    let 剩余 = &消息[首 + 1..];
    let 尾 = 剩余.find('`')?;
    Some(剩余[..尾].to_string())
}

/// 从跨度标签中提取反引号包裹的变量名
fn 从跨度取变量名(跨度: &[诊断跨度]) -> Option<String> {
    跨度.iter().find_map(|跨度| {
        let 标签 = 跨度.标签文本.as_deref()?;
        let 首 = 标签.find('`')?;
        let 剩余 = &标签[首 + 1..];
        let 尾 = 剩余.find('`')?;
        Some(剩余[..尾].to_string())
    })
}

#[cfg(test)]
mod 母语测试 {
    use super::*;

    fn 定位(起始行号: u32) -> 诊断位置 {
        诊断位置 {
            文件名称: "main.zh".to_string(),
            起始行号,
            起始列号: 1,
            结束行号: 起始行号,
            结束列号: 2,
            源码文本: None,
            标签文本: None,
            是否主因: false,
        }
    }

    #[test]
    fn 借用冲突模板在每个语言包都在() {
        let 详情 = 所有权详情 {
            变量名: "x".to_string(),
            移动发生: None,
            借用发生: None,
            再次使用: None,
            借用冲突: Some(借用冲突 {
                可变方: 定位(7),
                不可变方: 定位(5),
            }),
        };
        for 语言 in [
            "zh", "en", "ja", "ko", "ru", "de", "es", "fr", "pt", "hi", "ar",
        ] {
            let _守卫 = crate::语言::钉住语言(语言);
            let 正文 = 详情.叙事文本();
            assert!(
                !正文.contains("diag_ownership_borrow_conflict"),
                "{语言} 语言包缺少 diag_ownership_borrow_conflict"
            );
            assert!(
                正文.contains('x') && 正文.contains('7') && 正文.contains('5'),
                "{语言} 的冲突叙事没有回填全部三个槽位：{正文}"
            );
        }
    }

    #[test]
    fn 不可变标签不被误判为可变() {
        let 可变在前 = 借用冲突::自带标签跨度(vec![
            ("mutable borrow occurs here", 定位(7)),
            ("immutable borrow occurs here", 定位(5)),
            ("immutable borrow later used here", 定位(5)),
        ])
        .expect("应认出冲突双方");
        assert_eq!(可变在前.可变方.起始行号, 7);
        assert_eq!(可变在前.不可变方.起始行号, 5);

        let (可变方, 不可变方) = 借用冲突::归类带标签跨度(vec![
            ("immutable borrow occurs here", 定位(5)),
            ("immutable borrow later used here", 定位(5)),
        ]);
        assert!(可变方.is_none(), "不可变标签不得冒充可变借用");
        assert_eq!(不可变方.unwrap().起始行号, 5);
    }
}
