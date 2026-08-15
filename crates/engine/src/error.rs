// 错误类型模块
// 定义核心引擎的统一错误类型，实现 Display 输出友好的中文错误消息。
// 所有转译过程中可能出现的错误都通过 TranspileError 枚举统一表达，
// 每个变体携带足够的上下文信息（位置、名称、原因），便于诊断和调试。

use std::fmt;

/// 错误位置信息（1 起行/列，与 rustc 诊断的约定一致）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    /// 行号（从 1 开始）
    pub line: usize,
    /// 列号（从 1 开始）
    pub column: usize,
}

impl SourceLocation {
    /// 创建新的错误位置
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }

    /// 返回中文位置描述，如 `第 3 行第 5 列`
    pub fn describe(&self) -> String {
        format!("第 {} 行第 {} 列", self.line, self.column)
    }
}

/// 转译过程中的统一错误类型
///
/// 每个变体携带足够上下文（位置、名称、原因），
/// Display 输出可直接展示给用户的中文消息。
#[derive(Debug, Clone, PartialEq)]
pub enum TranspileError {
    /// 输入源码无效（空文件、非法编码等）
    InvalidInput { reason: String },
    /// 词法层面的错误（token 无法识别等）
    LexError {
        location: SourceLocation,
        detail: String,
    },
    /// 关键字/标识符映射缺失（找不到对应翻译）
    MappingMissing {
        name: String,
        location: Option<SourceLocation>,
    },
    /// 检测到可疑 Unicode 混淆字符（零宽/双向/同形）
    ConfusionChar {
        location: SourceLocation,
        character: char,
        detail: String,
    },
    /// 翻译缓存不可用（容量或状态异常）
    CacheUnavailable { reason: String },
    /// 暂不支持的语法构造
    UnsupportedConstruct {
        construct: String,
        location: Option<SourceLocation>,
    },
    /// 其他未分类错误
    Other { reason: String },
}

impl fmt::Display for TranspileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { reason } => write!(f, "输入无效：{}", reason),
            Self::LexError { location, detail } => {
                write!(f, "词法错误（{}）：{}", location.describe(), detail)
            }
            Self::MappingMissing {
                name,
                location: Some(loc),
            } => {
                write!(
                    f,
                    "映射缺失（{}）：找不到 `{}` 的翻译映射",
                    loc.describe(),
                    name
                )
            }
            Self::MappingMissing { name, location: None } => {
                write!(f, "映射缺失：找不到 `{}` 的翻译映射", name)
            }
            Self::ConfusionChar {
                location,
                character,
                detail,
            } => {
                write!(
                    f,
                    "检测到可疑 Unicode 字符（{}）：U+{:04X}（{}）",
                    location.describe(),
                    *character as u32,
                    detail
                )
            }
            Self::CacheUnavailable { reason } => write!(f, "翻译缓存不可用：{}", reason),
            Self::UnsupportedConstruct {
                construct,
                location: Some(loc),
            } => {
                write!(f, "暂不支持的语法构造（{}）：`{}`", loc.describe(), construct)
            }
            Self::UnsupportedConstruct {
                construct,
                location: None,
            } => {
                write!(f, "暂不支持的语法构造：`{}`", construct)
            }
            Self::Other { reason } => write!(f, "{}", reason),
        }
    }
}

impl std::error::Error for TranspileError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_location_describe() {
        let loc = SourceLocation::new(3, 5);
        assert_eq!(loc.describe(), "第 3 行第 5 列");
    }

    #[test]
    fn test_invalid_input_message() {
        let err = TranspileError::InvalidInput {
            reason: "源码为空".to_string(),
        };
        assert_eq!(err.to_string(), "输入无效：源码为空");
    }

    #[test]
    fn test_lex_error_message_with_location() {
        let err = TranspileError::LexError {
            location: SourceLocation::new(2, 10),
            detail: "无法识别的字符".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "词法错误（第 2 行第 10 列）：无法识别的字符"
        );
    }

    #[test]
    fn test_mapping_missing_with_and_without_location() {
        let with_loc = TranspileError::MappingMissing {
            name: "结构体".to_string(),
            location: Some(SourceLocation::new(1, 1)),
        };
        assert_eq!(
            with_loc.to_string(),
            "映射缺失（第 1 行第 1 列）：找不到 `结构体` 的翻译映射"
        );

        let without_loc = TranspileError::MappingMissing {
            name: "结构体".to_string(),
            location: None,
        };
        assert_eq!(
            without_loc.to_string(),
            "映射缺失：找不到 `结构体` 的翻译映射"
        );
    }

    #[test]
    fn test_confusion_char_message() {
        let err = TranspileError::ConfusionChar {
            location: SourceLocation::new(1, 4),
            character: '\u{200B}',
            detail: "零宽空格".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "检测到可疑 Unicode 字符（第 1 行第 4 列）：U+200B（零宽空格）"
        );
    }

    #[test]
    fn test_cache_unavailable_message() {
        let err = TranspileError::CacheUnavailable {
            reason: "容量为 0".to_string(),
        };
        assert_eq!(err.to_string(), "翻译缓存不可用：容量为 0");
    }

    #[test]
    fn test_unsupported_construct_message() {
        let err = TranspileError::UnsupportedConstruct {
            construct: "宏_rules".to_string(),
            location: None,
        };
        assert_eq!(err.to_string(), "暂不支持的语法构造：`宏_rules`");
    }

    #[test]
    fn test_std_error_trait_impl() {
        // 可向上转型为 Box<dyn std::error::Error>，供 anyhow 等错误链使用
        let err: Box<dyn std::error::Error> = Box::new(TranspileError::Other {
            reason: "未知故障".to_string(),
        });
        assert_eq!(err.to_string(), "未知故障");
    }
}
