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

    /// 返回当前语言下的位置描述，如 `第 3 行第 5 列`
    pub fn describe(&self) -> String {
        crate::语言::f(
            "err_line_col",
            &[&self.line.to_string(), &self.column.to_string()],
        )
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
            Self::InvalidInput { reason } => {
                write!(f, "{}", crate::语言::f("err_input_invalid", &[reason]))
            }
            Self::LexError { location, detail } => write!(
                f,
                "{}",
                crate::语言::f("err_lex_error", &[&location.describe(), detail])
            ),
            Self::MappingMissing {
                name,
                location: Some(loc),
            } => write!(
                f,
                "{}",
                crate::语言::f("err_mapping_missing_at", &[&loc.describe(), name])
            ),
            Self::MappingMissing {
                name,
                location: None,
            } => {
                write!(f, "{}", crate::语言::f("err_mapping_missing", &[name]))
            }
            Self::ConfusionChar {
                location,
                character,
                detail,
            } => {
                // 格式占位符 {:04X} 先格式化再传入模板
                let codepoint = format!("{:04X}", *character as u32);
                write!(
                    f,
                    "{}",
                    crate::语言::f(
                        "err_confusion_char",
                        &[&location.describe(), &codepoint, detail]
                    )
                )
            }
            Self::CacheUnavailable { reason } => {
                write!(f, "{}", crate::语言::f("err_cache_unavailable", &[reason]))
            }
            Self::UnsupportedConstruct {
                construct,
                location: Some(loc),
            } => write!(
                f,
                "{}",
                crate::语言::f("err_unsupported_at", &[&loc.describe(), construct])
            ),
            Self::UnsupportedConstruct {
                construct,
                location: None,
            } => write!(f, "{}", crate::语言::f("err_unsupported", &[construct])),
            Self::Other { reason } => write!(f, "{}", reason),
        }
    }
}

impl std::error::Error for TranspileError {}

/// 加载错误的目标数据源（选择本地化消息模板用）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadTarget {
    /// keywords.toml
    Keywords,
    /// module_paths.toml
    ModulePaths,
    /// stdlib.toml
    Stdlib,
    /// crates/*.toml 第三方库文件
    ThirdParty,
    /// 通用映射文件（MappingLoader 单文件）
    Mapping,
    /// 内置关键字数据（编译期嵌入）
    BuiltinKeywords,
    /// 内置模块路径数据（编译期嵌入）
    BuiltinModulePaths,
    /// 内置标准库数据（编译期嵌入）
    BuiltinStdlib,
}

/// 语言包/映射表加载层的统一错误类型
///
/// 取代加载层旧有的 `Result<_, String>`：变体携带结构化上下文
/// （目标/路径/原因），调用方可用 `matches!` 分类处理（如文件缺失走
/// 回退链、解析失败直接终止）；Display 输出与旧 String 消息一致的
/// 本地化文本（复用同一批 ui.toml 键），消息不漂移。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// 必需文件缺失（如 keywords.toml 不存在）
    FileMissing { target: LoadTarget, path: String },
    /// 读取文件 IO 失败
    ReadFailed {
        target: LoadTarget,
        path: Option<String>,
        detail: String,
    },
    /// TOML 解析失败
    ParseFailed {
        target: LoadTarget,
        path: Option<String>,
        detail: String,
    },
    /// 第三方库目录（crates/）读取失败
    DirReadFailed { detail: String },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::语言::f as msg;
        match self {
            Self::FileMissing { target, path } => {
                let key = match target {
                    LoadTarget::Keywords => "load_keywords_missing",
                    _ => "load_map_file_missing",
                };
                write!(f, "{}", msg(key, &[path]))
            }
            Self::ReadFailed {
                target,
                path,
                detail,
            } => match target {
                LoadTarget::Keywords => {
                    write!(f, "{}", msg("load_read_keywords_failed", &[detail]))
                }
                LoadTarget::ModulePaths => {
                    write!(f, "{}", msg("load_read_module_paths_failed", &[detail]))
                }
                LoadTarget::Mapping => {
                    write!(f, "{}", msg("load_read_map_failed", &[detail]))
                }
                // 第三方库文件：消息模板含路径占位符
                _ => {
                    let path_str = path.clone().unwrap_or_default();
                    write!(
                        f,
                        "{}",
                        msg("load_read_map_path_failed", &[&path_str, detail])
                    )
                }
            },
            Self::ParseFailed {
                target,
                path,
                detail,
            } => {
                let out = match target {
                    LoadTarget::Keywords => msg("load_parse_keywords_failed", &[detail]),
                    LoadTarget::ModulePaths => msg("load_parse_module_paths_failed", &[detail]),
                    LoadTarget::Mapping => msg("load_parse_map_failed", &[detail]),
                    LoadTarget::BuiltinKeywords => {
                        msg("load_parse_builtin_keywords_failed", &[detail])
                    }
                    LoadTarget::BuiltinModulePaths => {
                        msg("load_parse_builtin_paths_failed", &[detail])
                    }
                    LoadTarget::BuiltinStdlib => msg("load_parse_builtin_stdlib_failed", &[detail]),
                    // stdlib/第三方库文件：消息模板含路径占位符
                    LoadTarget::Stdlib | LoadTarget::ThirdParty => {
                        let path_str = path.clone().unwrap_or_default();
                        msg("load_parse_map_path_failed", &[&path_str, detail])
                    }
                };
                write!(f, "{}", out)
            }
            Self::DirReadFailed { detail } => {
                write!(f, "{}", msg("load_read_dir_failed", &[detail]))
            }
        }
    }
}

impl std::error::Error for LoadError {}

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

    #[test]
    fn test_load_error_messages_match_legacy_keys() {
        // 枚举化后消息与旧 String 错误一致（复用同一批 ui.toml 键）
        let missing = LoadError::FileMissing {
            target: LoadTarget::Keywords,
            path: "/lp/keywords.toml".into(),
        };
        assert!(missing.to_string().contains("关键字文件不存在"));

        let parse = LoadError::ParseFailed {
            target: LoadTarget::BuiltinStdlib,
            path: None,
            detail: "expected value".into(),
        };
        assert_eq!(
            parse.to_string(),
            "解析内置标准库 TOML 失败: expected value"
        );

        let third = LoadError::ParseFailed {
            target: LoadTarget::ThirdParty,
            path: Some("\"crates/web.toml\"".into()),
            detail: "bad".into(),
        };
        assert_eq!(
            third.to_string(),
            "解析映射表 \"crates/web.toml\" 失败: bad"
        );

        // 可用于程序化分类：文件缺失可回退，解析失败应终止
        assert!(matches!(missing, LoadError::FileMissing { .. }));
    }
}
