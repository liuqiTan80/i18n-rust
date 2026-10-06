//! 错误类型模块【zh 自举·批次1】
//! 本文件是源真相：由引导 rzc 转译为 错误类型.rs 后交 cargo 编译。
//! 再生成方式见 tools/zh-selfhost/regen.sh；请勿手改同目录下的 错误类型.rs。
//!
//! 定义核心引擎的统一错误类型，实现 Display 输出友好的中文错误消息。
//! 所有转译过程中可能出现的错误都通过 转译错误 枚举统一表达，
//! 每个变体携带足够的上下文信息（位置、名称、原因），便于诊断和调试。
//!
//! 注：`"err_*"`、`"load_*"` 系列字符串是与语言包 ui.toml 对齐的协议键
//! （数据，非代码），保持英文；`无符号机器整数`、`模式匹配!`、`标准库::错误模块::错误特征`
//! 路径暂无可靠中文写法，按英文透传（合法方言行为）。

use std::fmt::{Display, Formatter, Result};

/// 错误位置信息（1 起行/列，与 rustc 诊断的约定一致）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct 源码位置 {
    /// 行号（从 1 开始）
    pub 行号: usize,
    /// 列号（从 1 开始）
    pub 列号: usize,
}

impl 源码位置 {
    /// 创建新的错误位置
    pub fn 新建(行号: usize, 列号: usize) -> Self {
        Self { 行号, 列号 }
    }

    /// 返回当前语言下的位置描述，如 `第 3 行第 5 列`
    pub fn 描述位置(&self) -> String {
        crate::语言::查译(
            "err_line_col",
            &[&self.行号.to_string(), &self.列号.to_string()],
        )
    }
}

/// 转译过程中的统一错误类型
///
/// 每个变体携带足够上下文（位置、名称、原因），
/// Display 输出可直接展示给用户的中文消息。
#[derive(Debug, Clone, PartialEq)]
pub enum 转译错误 {
    /// 输入源码无效（空文件、非法编码等）
    输入无效 { 错误原因: String },
    /// 词法层面的错误（token 无法识别等）
    词法错误 {
        报错位置: 源码位置, 详情: String
    },
    /// 关键字/标识符映射缺失（找不到对应翻译）
    映射缺失 {
        词条名: String,
        报错位置: Option<源码位置>,
    },
    /// 检测到可疑 Unicode 混淆字符（零宽/双向/同形）
    混淆字符 {
        报错位置: 源码位置,
        可疑字符: char,
        详情: String,
    },
    /// 翻译缓存不可用（容量或状态异常）
    缓存不可用 { 错误原因: String },
    /// 暂不支持的语法构造
    不支持构造 {
        构造: String,
        报错位置: Option<源码位置>,
    },
    /// 其他未分类错误
    其他 { 错误原因: String },
}

impl Display for 转译错误 {
    fn fmt(&self, 格式器: &mut Formatter<'_>) -> Result {
        match self {
            Self::输入无效 { 错误原因 } => {
                write!(
                    格式器,
                    "{}",
                    crate::语言::查译("err_input_invalid", &[错误原因])
                )
            }
            Self::词法错误 {
                报错位置, 详情
            } => write!(
                格式器,
                "{}",
                crate::语言::查译("err_lex_error", &[&报错位置.描述位置(), 详情])
            ),
            Self::映射缺失 {
                词条名,
                报错位置: Some(位置值),
            } => write!(
                格式器,
                "{}",
                crate::语言::查译("err_mapping_missing_at", &[&位置值.描述位置(), 词条名])
            ),
            Self::映射缺失 {
                词条名,
                报错位置: None,
            } => {
                write!(
                    格式器,
                    "{}",
                    crate::语言::查译("err_mapping_missing", &[词条名])
                )
            }
            Self::混淆字符 {
                报错位置,
                可疑字符,
                详情,
            } => {
                // 格式占位符 {:04X} 先格式化再传入模板
                let 码位 = format!("{:04X}", *可疑字符 as u32);
                write!(
                    格式器,
                    "{}",
                    crate::语言::查译(
                        "err_confusion_char",
                        &[&报错位置.描述位置(), &码位, 详情]
                    )
                )
            }
            Self::缓存不可用 { 错误原因 } => {
                write!(
                    格式器,
                    "{}",
                    crate::语言::查译("err_cache_unavailable", &[错误原因])
                )
            }
            Self::不支持构造 {
                构造,
                报错位置: Some(位置值),
            } => write!(
                格式器,
                "{}",
                crate::语言::查译("err_unsupported_at", &[&位置值.描述位置(), 构造])
            ),
            Self::不支持构造 {
                构造,
                报错位置: None,
            } => write!(格式器, "{}", crate::语言::查译("err_unsupported", &[构造])),
            Self::其他 { 错误原因 } => write!(格式器, "{}", 错误原因),
        }
    }
}

impl std::error::Error for 转译错误 {}

/// 加载错误的目标数据源（选择本地化消息模板用）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 加载目标 {
    /// keywords.toml
    关键字表,
    /// module_paths.toml
    模块路径表,
    /// stdlib.toml
    标准库表,
    /// crates/*.toml 第三方库文件
    第三方库,
    /// 通用映射文件（映射加载器 单文件）
    映射表,
    /// 内置关键字数据（编译期嵌入）
    内置关键字,
    /// 内置模块路径数据（编译期嵌入）
    内置模块路径表,
    /// 内置标准库数据（编译期嵌入）
    内置标准库表,
    /// errors.toml 错误消息翻译表（诊断翻译层）
    错误消息表,
}

/// 语言包/映射表加载层的统一错误类型
///
/// 取代加载层旧有的 `IO结果<_, 字符串值>`：变体携带结构化上下文
/// （目标/路径/原因），调用方可用 `模式匹配!` 分类处理（如文件缺失走
/// 回退链、解析失败直接终止）；Display 输出与旧 字符串值 消息一致的
/// 本地化文本（复用同一批 ui.toml 键），消息不漂移。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum 加载错误 {
    /// 必需文件缺失（如 keywords.toml 不存在）
    文件缺失 {
        目标: 加载目标, 路径: String
    },
    /// 读取文件 IO 失败
    读取失败 {
        目标: 加载目标,
        路径: Option<String>,
        详情: String,
    },
    /// TOML 解析失败
    解析失败 {
        目标: 加载目标,
        路径: Option<String>,
        详情: String,
    },
    /// 第三方库目录（crates/）读取失败
    目录读取失败 { 详情: String },
}

// 构造助手：供其它 .zh 模块按位置参数构造带「路径」字段的变体。
// 原因：rzc 只在枚举定义的同一文件里保留字段名「路径」；跨文件用结构体
// 字面量写 `路径:` 会被词法器当作普通标识符映射成 `路径类型`（E0559）。
// 「目标」「详情」不是词典词，跨文件直接构造无碍；故仅含「路径」的三变体提供助手。
impl 加载错误 {
    pub fn 新建文件缺失(目标: 加载目标, 磁盘路径: String) -> Self {
        Self::文件缺失 {
            目标,
            路径: 磁盘路径,
        }
    }
    pub fn 新建读取失败(
        目标: 加载目标, 磁盘路径: Option<String>, 详情: String
    ) -> Self {
        Self::读取失败 {
            目标,
            路径: 磁盘路径,
            详情,
        }
    }
    pub fn 新建解析失败(
        目标: 加载目标, 磁盘路径: Option<String>, 详情: String
    ) -> Self {
        Self::解析失败 {
            目标,
            路径: 磁盘路径,
            详情,
        }
    }
}

impl Display for 加载错误 {
    fn fmt(&self, 格式器: &mut Formatter<'_>) -> Result {
        use crate::语言::查译 as 取消息;
        match self {
            Self::文件缺失 { 目标, 路径 } => {
                let 键名 = match 目标 {
                    加载目标::关键字表 => "load_keywords_missing",
                    _ => "load_map_file_missing",
                };
                write!(格式器, "{}", 取消息(键名, &[路径]))
            }
            Self::读取失败 {
                目标, 路径, 详情
            } => match 目标 {
                加载目标::关键字表 => {
                    write!(格式器, "{}", 取消息("load_read_keywords_failed", &[详情]))
                }
                加载目标::模块路径表 => {
                    write!(
                        格式器,
                        "{}",
                        取消息("load_read_module_paths_failed", &[详情])
                    )
                }
                加载目标::映射表 => {
                    write!(格式器, "{}", 取消息("load_read_map_failed", &[详情]))
                }
                // errors.toml：沿用诊断层原有的消息键（含路径 + 原因两个占位符）
                加载目标::错误消息表 => {
                    let 路径文本 = 路径.clone().unwrap_or_default();
                    write!(
                        格式器,
                        "{}",
                        取消息("err_read_error_messages", &[&路径文本, 详情])
                    )
                }
                // 第三方库文件：消息模板含路径占位符
                _ => {
                    let 路径文本 = 路径.clone().unwrap_or_default();
                    write!(
                        格式器,
                        "{}",
                        取消息("load_read_map_path_failed", &[&路径文本, 详情])
                    )
                }
            },
            Self::解析失败 {
                目标, 路径, 详情
            } => {
                let 输出 = match 目标 {
                    加载目标::关键字表 => 取消息("load_parse_keywords_failed", &[详情]),
                    加载目标::模块路径表 => {
                        取消息("load_parse_module_paths_failed", &[详情])
                    }
                    加载目标::映射表 => 取消息("load_parse_map_failed", &[详情]),
                    加载目标::内置关键字 => {
                        取消息("load_parse_builtin_keywords_failed", &[详情])
                    }
                    加载目标::内置模块路径表 => {
                        取消息("load_parse_builtin_paths_failed", &[详情])
                    }
                    加载目标::内置标准库表 => {
                        取消息("load_parse_builtin_stdlib_failed", &[详情])
                    }
                    // errors.toml：沿用诊断层原有的消息键
                    加载目标::错误消息表 => 取消息("err_parse_error_messages", &[详情]),
                    // stdlib/第三方库文件：消息模板含路径占位符
                    加载目标::标准库表 | 加载目标::第三方库 => {
                        let 路径文本 = 路径.clone().unwrap_or_default();
                        取消息("load_parse_map_path_failed", &[&路径文本, 详情])
                    }
                };
                write!(格式器, "{}", 输出)
            }
            Self::目录读取失败 { 详情 } => {
                write!(格式器, "{}", 取消息("load_read_dir_failed", &[详情]))
            }
        }
    }
}

impl std::error::Error for 加载错误 {}

#[cfg(test)]
mod 单元测试 {
    use super::*;

    // 本模块消息经 语言::f 按当前语言格式化，断言前需钉住 zh 并串行化
    fn 中文守卫() -> crate::语言::语言测试守卫 {
        crate::语言::测试语言("zh")
    }

    #[test]
    fn 源码位置描述() {
        let _守卫 = 中文守卫();
        let 位置 = 源码位置::新建(3, 5);
        assert_eq!(位置.描述位置(), "第 3 行第 5 列");
    }

    #[test]
    fn 输入无效消息() {
        let _守卫 = 中文守卫();
        let 错 = 转译错误::输入无效 {
            错误原因: "源码为空".to_string(),
        };
        assert_eq!(错.to_string(), "输入无效：源码为空");
    }

    #[test]
    fn 词法错误含位置消息() {
        let _守卫 = 中文守卫();
        let 错 = 转译错误::词法错误 {
            报错位置: 源码位置::新建(2, 10),
            详情: "无法识别的字符".to_string(),
        };
        assert_eq!(
            错.to_string(),
            "词法错误（第 2 行第 10 列）：无法识别的字符"
        );
    }

    #[test]
    fn 映射缺失有无位置消息() {
        let _守卫 = 中文守卫();
        let 有位置 = 转译错误::映射缺失 {
            词条名: "结构体".to_string(),
            报错位置: Some(源码位置::新建(1, 1)),
        };
        assert_eq!(
            有位置.to_string(),
            "映射缺失（第 1 行第 1 列）：找不到 `结构体` 的翻译映射"
        );

        let 无位置 = 转译错误::映射缺失 {
            词条名: "结构体".to_string(),
            报错位置: None,
        };
        assert_eq!(无位置.to_string(), "映射缺失：找不到 `结构体` 的翻译映射");
    }

    #[test]
    fn 混淆字符消息() {
        let _守卫 = 中文守卫();
        let 错 = 转译错误::混淆字符 {
            报错位置: 源码位置::新建(1, 4),
            可疑字符: '\u{200B}',
            详情: "零宽空格".to_string(),
        };
        assert_eq!(
            错.to_string(),
            "检测到可疑 Unicode 字符（第 1 行第 4 列）：U+200B（零宽空格）"
        );
    }

    #[test]
    fn 缓存不可用消息() {
        let _守卫 = 中文守卫();
        let 错 = 转译错误::缓存不可用 {
            错误原因: "容量为 0".to_string(),
        };
        assert_eq!(错.to_string(), "翻译缓存不可用：容量为 0");
    }

    #[test]
    fn 不支持构造消息() {
        let _守卫 = 中文守卫();
        let 错 = 转译错误::不支持构造 {
            构造: "宏_rules".to_string(),
            报错位置: None,
        };
        assert_eq!(错.to_string(), "暂不支持的语法构造：`宏_rules`");
    }

    #[test]
    fn 标准错误特征可实现() {
        // 可向上转型为 Box<动态 标准库::错误模块::错误特征>，供 anyhow 等错误链使用
        let 错 = 转译错误::其他 {
            错误原因: "未知故障".to_string(),
        };
        let 装箱: Box<dyn std::error::Error> = 错.into();
        assert_eq!(装箱.to_string(), "未知故障");
    }

    #[test]
    fn 加载错误消息对齐历史键() {
        let _守卫 = 中文守卫();
        // 枚举化后消息与旧 字符串值 错误一致（复用同一批 ui.toml 键）
        let 缺失 = 加载错误::文件缺失 {
            目标: 加载目标::关键字表,
            路径: "/lp/keywords.toml".into(),
        };
        assert!(缺失.to_string().contains("关键字文件不存在"));

        let 解析 = 加载错误::解析失败 {
            目标: 加载目标::内置标准库表,
            路径: None,
            详情: "expected value".into(),
        };
        assert_eq!(解析.to_string(), "解析内置标准库 TOML 失败: expected value");

        let 第三方 = 加载错误::解析失败 {
            目标: 加载目标::第三方库,
            路径: Some("\"crates/web.toml\"".into()),
            详情: "bad".into(),
        };
        assert_eq!(
            第三方.to_string(),
            "解析映射表 \"crates/web.toml\" 失败: bad"
        );

        // 可用于程序化分类：文件缺失可回退，解析失败应终止
        assert!(matches!(缺失, 加载错误::文件缺失 { .. }));
    }
}
