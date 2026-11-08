//! 诊断模块 - 解析 rustc 的 JSON 错误输出，结合语言包翻译成中文教学诊断信息
//!
//! 将 rustc 编译器产生的 JSON 格式诊断信息（`--error-format=json`）解析为结构化数据，
//! 再根据语言包中的错误消息翻译表，生成面向中文教学场景的诊断输出，
//! 包含错误码解释、教学提示、所有权错误叙事化详情等。
//!
//! 子模块划分（与本文件此前的分段注释一致）：
//! - [`诊断模型`]：rustc JSON 诊断数据结构
//! - [`消息翻译`]：错误消息翻译结构（errors.toml）
//! - [`教学诊断记录`]：教学诊断结构与所有权叙事化详情
//! - [`诊断翻译`]：诊断翻译器（类型映射支持）
//! - [`输出格式`]：JSON 解析、格式化输出与未解析导入检测
//! - [`待译清单`]：英文残留兜底与待译清单（未译消息登记/落盘/读取）

#[path = "待译清单.rs"]
pub mod 待译清单;
#[path = "教学诊断记录.rs"]
mod 教学诊断记录;
#[path = "消息翻译.rs"]
mod 消息翻译;
#[path = "诊断模型.rs"]
mod 诊断模型;
#[path = "诊断翻译.rs"]
mod 诊断翻译;
#[path = "输出格式.rs"]
mod 输出格式;

pub use 待译清单::{
    主消息级, 取出缺口, 工具级, 建议级, 待译记录, 查看缺口, 清单目录, 清单项文件, 清空缺口,
    登记缺口, 编码缺口字段, 落盘缺口, 落盘缺口至目录, 解析清单行, 解码缺口字段, 读取缺口,
};
pub use 教学诊断记录::{
    借用冲突, 所有权详情, 所有权错误码表, 提取所有权详情, 教学诊断, 诊断位置, 诊断级别,
};
pub use 消息翻译::{
    填充动态占位, 审计消息, 消息审计, 消息残段, 错误消息管理器
};
pub use 诊断模型::{编译器诊断, 诊断代码, 诊断跨度};
pub use 诊断翻译::{已渲染消息, 建议包名, 构建类型映射, 诊断翻译器};
pub use 输出格式::{
    取反引号首段, 是否未解析导入消息, 未解析包候选, 解析诊断输出, 诊断文本格式,
};

// 内部辅助函数：仅测试直接引用（见下方测试模块）
#[cfg(test)]
pub(crate) use 诊断翻译::{
    引用本体类型, 数引用层数, 整词替换, 替换类型段, 本地化引用类型, 本地化类型段,
};

#[cfg(test)]
mod 母语测试 {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn 造消息表临时档(内容: &str) -> NamedTempFile {
        let mut 档 = NamedTempFile::new().expect("创建临时文件失败");
        write!(档, "{}", 内容).expect("写入临时文件失败");
        档
    }

    fn 造标准诊断() -> 编译器诊断 {
        编译器诊断 {
            诊断消息: "mismatched types".to_string(),
            诊断码: Some(诊断代码 { 码值: "E0308".to_string(), 释义: None }),
            级别: "error".to_string(),
            跨度集: vec![诊断跨度 {
                文件名称: "src/main.rs".to_string(),
                起始行号: 3,
                起始列号: 5,
                结束行号: 3,
                结束列号: 10,
                源码文本: Some(
                    serde_json::from_str(r#"[{"text": "let x: i32 = \"hello\";", "highlight_start": 1, "highlight_end": 22}]"#).expect("测试用 JSON 应能解析"),
                ),
                起始字节: Some(30),
                结束字节: Some(51),
                是否主因: true,
                标签文本: Some("expected `i32`, found `&str`".to_string()),
                建议替换: None,
            }],
            子诊断: vec![编译器诊断 {
                诊断消息: "consider using a conversion function".to_string(),
                诊断码: None,
                级别: "help".to_string(),
                跨度集: vec![],
                子诊断: vec![],
                已渲染: None,
            }],
            已渲染: None,
        }
    }

    fn 造标准类型表() -> HashMap<String, String> {
        HashMap::from([
            ("u32".into(), "整数32".into()),
            ("i32".into(), "有符号整数32".into()),
            ("f64".into(), "浮点64".into()),
            ("&str".into(), "字符串引用".into()),
            ("String".into(), "字符串".into()),
        ])
    }

    /// 构造指定标签的 E0308 诊断（基于标准测试诊断改标签）
    fn 造带标签(标签: &str) -> 编译器诊断 {
        let mut 诊断 = 造标准诊断();
        诊断.跨度集[0].标签文本 = Some(标签.to_string());
        诊断
    }

    #[test]
    fn 测_数引用层数() {
        assert_eq!(数引用层数("String"), 0);
        assert_eq!(数引用层数("&String"), 1);
        assert_eq!(数引用层数("&&String"), 2);
        assert_eq!(数引用层数("&mut String"), 1);
        assert_eq!(数引用层数("&str"), 1);
    }

    #[test]
    fn 测_本地化引用类型() {
        let 映射表 = 造标准类型表();
        // 整体命中类型映射表（`&str` 是特殊整体条目）
        assert_eq!(本地化引用类型("&str", &映射表), "字符串引用");
        // 拆引用符 + 本体映射
        assert_eq!(本地化引用类型("&String", &映射表), "&字符串");
        assert_eq!(本地化引用类型("&&i32", &映射表), "&&有符号整数32");
        assert_eq!(本地化引用类型("&mut String", &映射表), "&mut 字符串");
        // 无引用符时原样返回
        assert_eq!(本地化引用类型("String", &映射表), "字符串");
    }

    #[test]
    fn 测引用层数提示追加() {
        let _守卫 = crate::语言::钉住语言("zh");
        // `expected &String, found String`：引用层数 1 对 0，应追加提示
        let 诊断 = 造带标签("expected `&String`, found `String`");
        let 翻译器 =
            诊断翻译器::新建(错误消息管理器::从字符串载入("").unwrap(), 造标准类型表());
        let 教学 = 翻译器.翻译诊断(&诊断);
        assert!(
            教学
                .提示集
                .iter()
                .any(|每提示| 每提示.contains("引用层数不匹配")
                    && 每提示.contains("&字符串")
                    && 每提示.contains("字符串")),
            "应追加引用层数提示，实际提示：{:?}",
            教学.提示集
        );
    }

    #[test]
    fn 测同层数无提示() {
        let _守卫 = crate::语言::钉住语言("zh");
        // `expected &String, found &str`：层数 1 对 1，不应追加提示
        let 诊断 = 造带标签("expected `&String`, found `&str`");
        let 翻译器 =
            诊断翻译器::新建(错误消息管理器::从字符串载入("").unwrap(), 造标准类型表());
        let 教学 = 翻译器.翻译诊断(&诊断);
        assert!(
            !教学
                .提示集
                .iter()
                .any(|每提示| 每提示.contains("引用层数不匹配")),
            "层数相同时不应追加提示，实际提示：{:?}",
            教学.提示集
        );
    }

    #[test]
    fn 测无引用无提示() {
        let _守卫 = crate::语言::钉住语言("zh");
        // `expected String, found i32`：均非引用，不应追加提示
        let 诊断 = 造带标签("expected `String`, found `i32`");
        let 翻译器 =
            诊断翻译器::新建(错误消息管理器::从字符串载入("").unwrap(), 造标准类型表());
        let 教学 = 翻译器.翻译诊断(&诊断);
        assert!(
            !教学
                .提示集
                .iter()
                .any(|每提示| 每提示.contains("引用层数不匹配")),
            "无引用时不应追加提示，实际提示：{:?}",
            教学.提示集
        );
    }

    #[test]
    fn 测_引用本体类型() {
        assert_eq!(引用本体类型("String"), "String");
        assert_eq!(引用本体类型("&String"), "String");
        assert_eq!(引用本体类型("&&i32"), "i32");
        assert_eq!(引用本体类型("&mut String"), "String");
        assert_eq!(引用本体类型("&&mut String"), "String");
        // `&str` 剥引用后核心是切片，与「字符串」不同，不能靠增减引用符互换
        assert_eq!(引用本体类型("&str"), "str");
    }

    /// 核心类型不同（标量、字符串等）时：引用层数提示为假阳性，不应追加
    #[test]
    fn 测不同核心类型无引用提示() {
        let _守卫 = crate::语言::钉住语言("zh");
        for 标签 in [
            "expected `&str`, found `String`",
            "expected `i32`, found `&str`",
            "expected `&String`, found `i32`",
        ] {
            let 诊断 = 造带标签(标签);
            let 翻译器 =
                诊断翻译器::新建(错误消息管理器::从字符串载入("").unwrap(), 造标准类型表());
            let 教学 = 翻译器.翻译诊断(&诊断);
            assert!(
                !教学
                    .提示集
                    .iter()
                    .any(|每提示| 每提示.contains("引用层数不匹配")),
                "核心类型不同不应追加引用层数提示（{标签}）：{:?}",
                教学.提示集
            );
        }
    }

    #[test]
    fn 测_载入错误消息管理器() {
        let 内容 = r#"
[E0308]
"消息模板" = "类型不匹配：期望 `{期望}`，实际得到 `{实际}`"
"教学提示" = "请检查变量类型是否与上下文要求一致。"

[E0433]
"消息模板" = "未找到类型 `{名称}`"
"教学提示" = "请确认是否已导入所需的模块或类型。"
"#;
        let 档 = 造消息表临时档(内容);
        let 管理 = 错误消息管理器::从文件载入(档.path()).unwrap();
        assert_eq!(管理.覆盖数(), 2);
    }

    #[test]
    fn 测_带语言包翻译与类型替换() {
        let 内容 = r#"
[E0308]
"消息模板" = "类型不匹配：期望 `{期望}`，实际得到 `{实际}`"
"教学提示" = "请检查变量类型。"
"#;
        let 档 = 造消息表临时档(内容);
        let 管理 = 错误消息管理器::从文件载入(档.path()).unwrap();
        let 类型表 = 造标准类型表();
        let 翻译器 = 诊断翻译器::新建(管理, 类型表);

        let 诊断 = 造标准诊断();
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert_eq!(
            教学.译文消息,
            "类型不匹配：期望 `有符号整数32`，实际得到 `字符串引用`"
        );
    }

    /// 无错误码消息：命中 [消息翻译] 节精确条目，标题与教学提示均翻译
    #[test]
    fn 测_无码命中消息表() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 内容 = r#"
["消息翻译"."format argument must be a string literal"]
"消息模板" = "format 参数必须是字符串字面量"
"教学提示" = "第一个参数应带引号。"
"#;
        let 档 = 造消息表临时档(内容);
        let 管理 = 错误消息管理器::从文件载入(档.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建(管理, 造标准类型表());

        let 诊断 = 编译器诊断 {
            诊断消息: "format argument must be a string literal".to_string(),
            诊断码: None,
            级别: "error".to_string(),
            跨度集: vec![],
            子诊断: vec![],
            已渲染: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert_eq!(教学.译文消息, "format 参数必须是字符串字面量");
        assert_eq!(教学.提示集, vec!["第一个参数应带引号。"]);
    }

    /// 前缀匹配：建议短语保留动态后缀（「你是否想用」后的反引号名保留）
    #[test]
    fn 测_前缀保留动态后缀() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 内容 = r#"
["消息翻译"."did you mean "]
"消息模板" = "你是否想用 "
"#;
        let 档 = 造消息表临时档(内容);
        let 管理 = 错误消息管理器::从文件载入(档.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建(管理, 造标准类型表());

        let 诊断 = 编译器诊断 {
            诊断消息: "no method named `foo`".to_string(),
            诊断码: Some(诊断代码 {
                码值: "E0599".to_string(),
                释义: None,
            }),
            级别: "error".to_string(),
            跨度集: vec![],
            子诊断: vec![编译器诊断 {
                诊断消息: "did you mean `foo`?".to_string(),
                诊断码: None,
                级别: "help".to_string(),
                跨度集: vec![],
                子诊断: vec![],
                已渲染: None,
            }],
            已渲染: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        // 建议子诊断：前缀翻译 + 动态后缀保留，包上「修复建议：」前缀
        assert_eq!(教学.提示集, vec!["修复建议：你是否想用 `foo`?"]);
    }

    /// 混淆字符建议：两个占位符从单引号分段提取两个字符
    #[test]
    fn 测_混淆字符引号捕获() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 内容 = r#"
["消息翻译"."Unicode character '"]
"消息模板" = "Unicode 字符 '{q0}' 形似 '{q1}'，但它并不是它"
"#;
        let 档 = 造消息表临时档(内容);
        let 管理 = 错误消息管理器::从文件载入(档.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建(管理, 造标准类型表());

        let 诊断 = 编译器诊断 {
            诊断消息: "unknown start of token: \\u{ff0c}".to_string(),
            诊断码: None,
            级别: "error".to_string(),
            跨度集: vec![],
            子诊断: vec![编译器诊断 {
                诊断消息:
                    "Unicode character '，' (Fullwidth Comma) looks like ',' (Comma), but it is not"
                        .to_string(),
                诊断码: None,
                级别: "help".to_string(),
                跨度集: vec![],
                子诊断: vec![],
                已渲染: None,
            }],
            已渲染: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert_eq!(
            教学.提示集,
            vec!["修复建议：Unicode 字符 '，' 形似 ','，但它并不是它"]
        );
    }

    /// 「想添加某个第三方库」建议：非 ASCII 包名静默丢弃
    ///（回归：中文模块名误触发「添加包」建议，且原文直接透出）
    #[test]
    fn 测_添加包建议非可解析名丢弃() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 档 = 造消息表临时档("");
        let 管理 = 错误消息管理器::从文件载入(档.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建(管理, 造标准类型表());

        let 诊断 = 编译器诊断 {
            诊断消息: "unresolved import `不存在的中文模块`".to_string(),
            诊断码: Some(诊断代码 {
                码值: "E0432".to_string(),
                释义: None,
            }),
            级别: "error".to_string(),
            跨度集: vec![],
            子诊断: vec![编译器诊断 {
                诊断消息: "if you wanted to use a crate named `不存在的中文模块`, use \
                          `cargo add 不存在的中文模块` to add it to your `Cargo.toml`"
                    .to_string(),
                诊断码: None,
                级别: "help".to_string(),
                跨度集: vec![],
                子诊断: vec![],
                已渲染: None,
            }],
            已渲染: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        // 非可解析包名的建议必然无效，应整体丢弃而非原文透出
        assert!(教学.提示集.is_empty());
    }

    /// 「想添加某个第三方库」建议：可解析包名译为中文且重写为 `rzc add`
    #[test]
    fn 测_添加包建议可解析名() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 内容 = r#"
["消息翻译"."if you wanted to use a crate named "]
"消息模板" = "若要使用名为 {q0} 的第三方库，请运行 `rzc add {q0}` 添加依赖"
"#;
        let 档 = 造消息表临时档(内容);
        let 管理 = 错误消息管理器::从文件载入(档.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建(管理, 造标准类型表());

        let 诊断 = 编译器诊断 {
            诊断消息: "unresolved import `serde_json`".to_string(),
            诊断码: Some(诊断代码 {
                码值: "E0432".to_string(),
                释义: None,
            }),
            级别: "error".to_string(),
            跨度集: vec![],
            子诊断: vec![编译器诊断 {
                诊断消息: "if you wanted to use a crate named `serde_json`, use `cargo add \
                          serde_json` to add it to your `Cargo.toml`"
                    .to_string(),
                诊断码: None,
                级别: "help".to_string(),
                跨度集: vec![],
                子诊断: vec![],
                已渲染: None,
            }],
            已渲染: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert_eq!(
            教学.提示集,
            vec![
                "修复建议：若要使用名为 serde_json 的第三方库，请运行 `rzc add serde_json` 添加依赖"
            ]
        );
    }

    /// 内置中文包：顶层「让」的解析错误应完整中文化（消息 + 建议 + 教学提示）
    #[test]
    fn 测_内置顶层让完整翻译() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 中 = crate::语言::内置文件("zh", "errors.toml").expect("内置中文错误表应存在");
        let 管理 = 错误消息管理器::从字符串载入(中).unwrap();
        let 翻译器 = 诊断翻译器::新建(管理, 造标准类型表());
        let 诊断 = 编译器诊断 {
            诊断消息: "expected item, found keyword `let`".to_string(),
            诊断码: None,
            级别: "error".to_string(),
            跨度集: vec![],
            子诊断: vec![编译器诊断 {
                诊断消息: "consider using `static` or `const` instead of `let`".to_string(),
                诊断码: None,
                级别: "help".to_string(),
                跨度集: vec![],
                子诊断: vec![],
                已渲染: None,
            }],
            已渲染: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);
        assert_eq!(教学.译文消息, "此处应是一项声明，但遇到了 `让`");
        assert!(
            !教学.译文消息.contains("expected"),
            "消息不得残留英文：{}",
            教学.译文消息
        );
        assert_eq!(
            教学.提示集,
            vec![
                "模块（顶层）作用域不能直接写 `让`：全局变量请改用 `静态` 或 `常量`，代码逻辑请放进函数（如 `函数 主函数()`）。",
                "修复建议：考虑改用 `静态` 或 `常量` 代替 `让`"
            ]
        );
    }

    /// 带模块路径的类型名：最长后缀匹配
    #[test]
    fn 测_类型段最长后缀() {
        let mut 类型表 = 造标准类型表();
        类型表.insert("Display".into(), "显示".into());
        assert_eq!(
            替换类型段("std::fmt::Display", &类型表),
            Some("std::fmt::显示".to_string())
        );
        assert_eq!(替换类型段("std::io::Error", &类型表), None);
    }

    /// 三段式本地化：类型段 + 路径前缀 + 中间段全中文化
    #[test]
    fn 测_类型段三段式() {
        let mut 映射表 = 造标准类型表();
        映射表.insert("Display".into(), "可显示".into());
        映射表.insert("fmt".into(), "格式化".into());
        映射表.insert("std".into(), "标准库".into());
        // 类型 + 路径 + 中间段全部命中
        assert_eq!(
            本地化类型段("std::fmt::Display", &映射表),
            Some("标准库::格式化::可显示".to_string())
        );
        // 仅类型段命中：路径保持原样
        assert_eq!(
            本地化类型段("alloc::String", &映射表),
            Some("alloc::字符串".to_string())
        );
        // 无任何段命中时保持原样并返回「无」
        assert_eq!(本地化类型段("unknown::io::Error", &映射表), None);
        // 无路径分隔的类型名（`&str`）由上层整串命中处理，此处不处理属预期
        映射表.insert("&str".into(), "字符串引用".into());
        assert_eq!(本地化类型段("&str", &映射表), None);
        assert_eq!(映射表.get("&str"), Some(&"字符串引用".to_string()));
    }

    /// 占位符填充后的实际类型名也应中文化
    #[test]
    fn 测_占位符类型本地化() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 内容 = r#"
[E0277]
"消息模板" = "类型 `{类型}` 未实现特征 `{特征}`"
"教学提示" = "请为该类型实现所需的特征。"
"#;
        let 档 = 造消息表临时档(内容);
        let 管理 = 错误消息管理器::从文件载入(档.path()).unwrap();
        let mut 类型表 = 造标准类型表();
        类型表.insert("Display".into(), "显示".into());
        let 翻译器 = 诊断翻译器::新建(管理, 类型表);
        let 诊断 = 编译器诊断 {
            诊断消息: "`({integer}, {integer}, &str)` doesn't implement `std::fmt::Display`"
                .to_string(),
            诊断码: Some(诊断代码 {
                码值: "E0277".to_string(),
                释义: None,
            }),
            级别: "error".to_string(),
            跨度集: vec![],
            子诊断: vec![],
            已渲染: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);
        assert_eq!(
            教学.译文消息,
            "类型 `(整数, 整数, &str)` 未实现特征 `std::fmt::显示`"
        );
    }

    /// 后缀键（以「~」开头）：动态名在消息中间的告警
    #[test]
    fn 测_后缀键从未使用() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 内容 = r#"
["消息翻译"."function `"]
"消息模板" = "函数 `{q0}` 从未被使用"

["消息翻译"."~ is never used"]
"消息模板" = "`{q0}` 从未被使用"

["消息翻译"."~ should have an upper camel case name"]
"消息模板" = "`{q0}` 应使用大写驼峰命名"
"#;
        let 档 = 造消息表临时档(内容);
        let 管理 = 错误消息管理器::从文件载入(档.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建(管理, 造标准类型表());

        // 前缀键优先：含类型词「函数」，比通用后缀键更完整
        let 造诊断 = |原文: &str| 编译器诊断 {
            诊断消息: 原文.to_string(),
            诊断码: None,
            级别: "warning".to_string(),
            跨度集: vec![],
            子诊断: vec![],
            已渲染: None,
        };
        let 教学 = 翻译器.翻译诊断(&造诊断("function `foo` is never used"));
        assert_eq!(教学.译文消息, "函数 `foo` 从未被使用");

        // 无前缀键命中时走后缀键：动态名前缀保留在捕获中
        let 教学 = 翻译器.翻译诊断(&造诊断(
            "type `myStruct` should have an upper camel case name",
        ));
        assert_eq!(教学.译文消息, "`myStruct` 应使用大写驼峰命名");

        // 后缀键精确兜底：无专用前缀键时用通用模板
        let 教学 = 翻译器.翻译诊断(&造诊断("type `Foo` is never used"));
        assert_eq!(教学.译文消息, "`Foo` 从未被使用");
    }

    /// 较新工具的算术错误（E0369）无标签，类型嵌入消息，应回退提取并翻译字面量占位符
    #[test]
    fn 测_算术错误无标签回退消息() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 内容 = r#"
[E0369]
"消息模板" = "类型不匹配：无法对 `{期望}` 和 `{实际}` 执行运算"
"教学提示" = "运算符两侧的类型必须兼容。"
"#;
        let 档 = 造消息表临时档(内容);
        let 管理 = 错误消息管理器::从文件载入(档.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建(管理, 造标准类型表());

        let 诊断 = 编译器诊断 {
            诊断消息: "cannot add `{integer}` to `&str`".to_string(),
            诊断码: Some(诊断代码 {
                码值: "E0369".to_string(),
                释义: None,
            }),
            级别: "error".to_string(),
            跨度集: vec![诊断跨度 {
                文件名称: "src/main.rs".to_string(),
                起始行号: 3,
                起始列号: 5,
                结束行号: 3,
                结束列号: 10,
                源码文本: None,
                起始字节: None,
                结束字节: None,
                是否主因: true,
                标签文本: None,
                建议替换: None,
            }],
            子诊断: vec![],
            已渲染: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);
        assert_eq!(
            教学.译文消息,
            "类型不匹配：无法对 `整数` 和 `字符串引用` 执行运算"
        );
    }

    #[test]
    fn 测_解析诊断输出() {
        let 单行 = r#"{"message":"mismatched types","code":{"code":"E0308"},"level":"error","spans":[],"children":[],"rendered":null}"#;
        let 样例 = format!("{}\n{}", "    Compiling test v0.1.0", 单行);
        let 诊断集 = 解析诊断输出(&样例);
        assert_eq!(诊断集.len(), 1);
        assert_eq!(诊断集[0].诊断消息, "mismatched types");
    }

    #[test]
    fn 测_解析包装诊断输出() {
        // 构建工具的 JSON 包装行：诊断信息嵌套在消息字段
        let 包装行 = r#"{"reason":"compiler-message","package_id":"path+file:///test#e2e@0.1.0","manifest_path":"/test/Cargo.toml","target":{"kind":["bin"],"name":"e2e"},"message":{"message":"use of moved value: `数据`","code":{"code":"E0382"},"level":"error","spans":[],"children":[],"rendered":null}}"#;
        let 样例 = format!("{}\n{}", "    Checking e2e v0.1.0", 包装行);
        let 诊断集 = 解析诊断输出(&样例);
        assert_eq!(诊断集.len(), 1);
        assert_eq!(诊断集[0].诊断码.as_ref().unwrap().码值, "E0382");
        assert_eq!(诊断集[0].诊断消息, "use of moved value: `数据`");
    }

    #[test]
    fn 测_诊断级别转换() {
        assert_eq!(诊断级别::解析级别("error"), 诊断级别::诊断错误);
        assert_eq!(诊断级别::解析级别("warning"), 诊断级别::诊断告警);
        assert_eq!(诊断级别::解析级别("help"), 诊断级别::诊断建议);
    }

    /// 构造所有权错误诊断（rustc JSON 结构）
    fn 造所有权诊断(
        错误码文: &str,
        原文: &str,
        主跨度: 诊断跨度,
        其他跨度集: Vec<诊断跨度>,
    ) -> 编译器诊断 {
        编译器诊断 {
            诊断消息: 原文.to_string(),
            诊断码: Some(诊断代码 {
                码值: 错误码文.to_string(),
                释义: None,
            }),
            级别: "error".to_string(),
            跨度集: std::iter::once(主跨度).chain(其他跨度集).collect(),
            子诊断: vec![],
            已渲染: None,
        }
    }

    fn 造跨度(起始行号: u32, 标签: &str, 是否主因: bool) -> 诊断跨度 {
        诊断跨度 {
            文件名称: "src/main.rs".to_string(),
            起始行号,
            起始列号: 5,
            结束行号: 起始行号,
            结束列号: 10,
            源码文本: None,
            起始字节: None,
            结束字节: None,
            是否主因,
            标签文本: Some(标签.to_string()),
            建议替换: None,
        }
    }

    /// 教学提示须全部输出
    ///
    /// 回归：`作为文字输出` 曾只取首条提示，第二条及以后被静默丢弃。
    #[test]
    fn 测_作为文字输出全部提示() {
        let 样例 = 教学诊断 {
            级别: 诊断级别::诊断错误,
            错误码: Some("E0308".to_string()),
            译文消息: "类型不匹配".to_string(),
            原文消息: "mismatched types".to_string(),
            提示集: vec![
                "第一条提示".to_string(),
                "第二条提示".to_string(),
                "第三条提示".to_string(),
            ],
            位置集: vec![诊断位置 {
                文件名称: "src/main.zh".to_string(),
                起始行号: 3,
                起始列号: 5,
                结束行号: 3,
                结束列号: 10,
                源码文本: Some("    让 x = 1;".to_string()),
                标签文本: None,
                是否主因: true,
            }],
            子诊断: Vec::new(),
            所有权详情: None,
        };
        let 正文 = 样例.作为文字输出();
        assert_eq!(
            正文.matches("💡").count(),
            3,
            "教学提示应全部输出，实际输出：{正文}"
        );
        for 每提示 in ["第一条提示", "第二条提示", "第三条提示"] {
            assert!(正文.contains(每提示), "教学提示 `{每提示}` 丢失：{正文}");
        }
    }

    #[test]
    fn 测_提取所有权详情_移动() {
        // 叙事文本按当前语言取模板，需钉住中文并串行化
        let _守卫 = crate::语言::钉住语言("zh");
        // 主跨度是再次使用处，副跨度标记移动发生
        let 诊断 = 造所有权诊断(
            "E0382",
            "use of moved value: `数据`",
            造跨度(5, "value used here after move", true),
            vec![造跨度(
                3,
                "move occurs because `数据` has type `String`, which does not implement the `Copy` trait",
                false,
            )],
        );
        let 详情 = 提取所有权详情("E0382", &诊断).expect("应提取出所有权详情");
        assert_eq!(详情.变量名, "数据");
        assert_eq!(详情.移动发生.as_ref().unwrap().起始行号, 3);
        assert_eq!(详情.再次使用.as_ref().unwrap().起始行号, 5);
        assert!(详情.借用发生.is_none());
        assert_eq!(
            详情.叙事文本(),
            "变量 `数据` 在第 3 行被移动，第 5 行尝试再次使用。"
        );
    }

    #[test]
    fn 测_提取所有权详情_借用冲突() {
        let _守卫 = crate::语言::钉住语言("zh");
        // 主跨度是可变借用处，另有不可变借用处与「借用稍后仍在使用」
        let 诊断 = 造所有权诊断(
            "E0502",
            "cannot borrow `向量` as mutable because it is also borrowed as immutable",
            造跨度(6, "mutable borrow occurs here", true),
            vec![
                造跨度(4, "immutable borrow occurs here", false),
                造跨度(7, "borrow later used here", false),
            ],
        );
        let 详情 = 提取所有权详情("E0502", &诊断).expect("应提取出所有权详情");
        assert_eq!(详情.变量名, "向量");
        assert_eq!(详情.借用发生.as_ref().unwrap().起始行号, 6);
        // 「借用稍后仍在使用」应归为再次使用而非借用发生
        assert_eq!(详情.再次使用.as_ref().unwrap().起始行号, 7);
        assert!(详情.移动发生.is_none());
        // 冲突双方按标签分别记录：可变借用 6、不可变借用 4
        let 冲突 = 详情.借用冲突.as_ref().expect("应认出冲突双方");
        assert_eq!(冲突.可变方.起始行号, 6);
        assert_eq!(冲突.不可变方.起始行号, 4);
        assert_eq!(
            详情.叙事文本(),
            "变量 `向量` 的可变借用（第 6 行）与不可变借用（第 4 行）重叠：两者不能同时存在。"
        );
    }

    /// 可变 7、不可变 5 两个行号必须各归其位，不得互换（旧叙事会读反行号）
    #[test]
    fn 测_叙事不读反行号() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 诊断 = 造所有权诊断(
            "E0502",
            "cannot borrow `分数` as mutable because it is also borrowed as immutable",
            造跨度(7, "mutable borrow occurs here", true),
            vec![造跨度(5, "immutable borrow occurs here", false)],
        );
        let 详情 = 提取所有权详情("E0502", &诊断).expect("应提取出所有权详情");
        assert_eq!(
            详情.叙事文本(),
            "变量 `分数` 的可变借用（第 7 行）与不可变借用（第 5 行）重叠：两者不能同时存在。"
        );
        let 冲突 = 详情.借用冲突.as_ref().expect("应认出冲突双方");
        assert_eq!(冲突.可变方.起始行号, 7);
        assert_eq!(冲突.不可变方.起始行号, 5);
    }

    /// 只有一方借用标签时回退旧句式：宁可少说，不可编造另一方行号。
    #[test]
    fn 测_单一借用标签回退旧叙事() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 诊断 = 造所有权诊断(
            "E0502",
            "cannot borrow `向量` as mutable because it is also borrowed as immutable",
            造跨度(6, "mutable borrow occurs here", true),
            vec![],
        );
        let 详情 = 提取所有权详情("E0502", &诊断).expect("应提取出所有权详情");
        assert!(详情.借用冲突.is_none());
        // 只有一个借用位置，叙事回到「变量在某行被借用。」
        assert_eq!(详情.叙事文本(), "变量 `向量` 在第 6 行被借用。");
    }

    /// 冲突双方要进 JSON（供扩展可视化）；未识别时不得出现该键。
    #[test]
    fn 测_借用冲突仅在识别时序列化() {
        let 每0502 = 造所有权诊断(
            "E0502",
            "cannot borrow `向量` as mutable because it is also borrowed as immutable",
            造跨度(7, "mutable borrow occurs here", true),
            vec![造跨度(5, "immutable borrow occurs here", false)],
        );
        let 取值 = serde_json::to_value(提取所有权详情("E0502", &每0502).unwrap())
            .expect("所有权详情应可序列化");
        assert_eq!(取值["借用冲突"]["可变借用"]["起始行"], 7);
        assert_eq!(取值["借用冲突"]["不可变借用"]["起始行"], 5);

        let 每0382 = 造所有权诊断(
            "E0382",
            "use of moved value: `数据`",
            造跨度(5, "value used here after move", true),
            vec![造跨度(
                3,
                "move occurs because `数据` has type `String`",
                false,
            )],
        );
        let 取值 = serde_json::to_value(提取所有权详情("E0382", &每0382).unwrap())
            .expect("所有权详情应可序列化");
        assert!(
            取值.get("借用冲突").is_none(),
            "非 E0502 不应带借用冲突键：{取值}"
        );
    }

    #[test]
    fn 测_提取所有权详情_移动借用() {
        let _守卫 = crate::语言::钉住语言("zh");
        // 主跨度即移动发生处，无再次使用位置
        let 诊断 = 造所有权诊断(
            "E0507",
            "cannot move out of `数据` which is behind a shared reference",
            造跨度(
                3,
                "move occurs because `数据` has type `String`, which does not implement the `Copy` trait",
                true,
            ),
            vec![],
        );
        let 详情 = 提取所有权详情("E0507", &诊断).expect("应提取出所有权详情");
        assert_eq!(详情.变量名, "数据");
        assert_eq!(详情.移动发生.as_ref().unwrap().起始行号, 3);
        assert!(详情.再次使用.is_none());
        assert_eq!(详情.叙事文本(), "变量 `数据` 在第 3 行被移动。");
    }

    #[test]
    fn 测_非所有权错误返回无() {
        // E0308 类型不匹配不是所有权错误，不应提取详情
        let 诊断 = 造标准诊断();
        assert!(提取所有权详情("E0308", &诊断).is_none());
    }

    #[test]
    fn 测_所有权详情序列化_结构化() {
        let 诊断 = 造所有权诊断(
            "E0382",
            "use of moved value: `数据`",
            造跨度(5, "value used here after move", true),
            vec![造跨度(
                3,
                "move occurs because `数据` has type `String`",
                false,
            )],
        );
        let 详情 = 提取所有权详情("E0382", &诊断).unwrap();
        let 取值 = serde_json::to_value(&详情).expect("所有权详情应可序列化");
        assert_eq!(取值["变量名"], "数据");
        assert_eq!(取值["移动发生"]["起始行"], 3);
        assert_eq!(取值["再次使用"]["起始行"], 5);
        assert_eq!(取值["借用发生"], serde_json::Value::Null);
    }

    #[test]
    fn 测_诊断带所有权详情与叙事() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 表格文本 = r#"
[E0382]
"消息模板" = "值在移动后被使用：`{变量名}`"
"教学提示" = "Rust 中值被移动后不能再使用。"
"#;
        let 管理 = 错误消息管理器::从字符串载入(表格文本).unwrap();
        let 翻译器 = 诊断翻译器::新建(管理, 造标准类型表());

        let 诊断 = 造所有权诊断(
            "E0382",
            "use of moved value: `数据`",
            造跨度(5, "value used here after move", true),
            vec![造跨度(
                3,
                "move occurs because `数据` has type `String`",
                false,
            )],
        );
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert!(教学.所有权详情.is_some());
        let 详情 = 教学.所有权详情.as_ref().unwrap();
        assert_eq!(详情.变量名, "数据");

        let 正文 = 教学.作为文字输出();
        assert!(正文.contains("📌 变量 `数据` 在第 3 行被移动，第 5 行尝试再次使用。"));
        assert!(正文.contains("💡 Rust 中值被移动后不能再使用。"));
    }

    /// 反引号首段提取：路径取首段，含空格的自由文本不提取
    #[test]
    fn 测_取反引号首段() {
        assert_eq!(
            取反引号首段("unresolved imports `a`, `b::c`"),
            vec!["a", "b"]
        );
        assert_eq!(
            取反引号首段("unresolved import `serde_json::Value`"),
            vec!["serde_json"]
        );
        assert!(取反引号首段("expected type `i32 x`").is_empty());
        assert!(取反引号首段("无反引号消息").is_empty());
    }

    /// 整词替换不误伤标识符子串；边界（串首/串尾/空格）正常命中
    #[test]
    fn 测_整词替换边界() {
        assert_eq!(
            整词替换("expected integer, found &str", "integer", "整数"),
            "expected 整数, found &str"
        );
        // 标识符子串不替换
        assert_eq!(
            整词替换("no method named to_integer", "integer", "整数"),
            "no method named to_integer"
        );
        assert_eq!(
            整词替换("integer_count", "integer", "整数"),
            "integer_count"
        );
        // 串首/串尾边界
        assert_eq!(整词替换("integer", "integer", "整数"), "整数");
    }

    /// 未解析导入识别：E0432/E0433 两种消息格式命中，其他消息不命中
    #[test]
    fn 测_是否未解析导入消息() {
        assert!(是否未解析导入消息(
            "unresolved import `serde_json`"
        ));
        assert!(是否未解析导入消息(
            "failed to resolve: use of undeclared crate or module `tokio`"
        ));
        // 翻译后的母语消息不命中（由调用方按错误码兼容处理）
        assert!(!是否未解析导入消息("未解析的导入 `serde_json`"));
        assert!(!是否未解析导入消息("unused variable `x`"));
    }

    /// 候选第三方库提取：去重 + 排除标准库与保留路径；非目标消息返回空
    #[test]
    fn 测_未解析包候选() {
        assert_eq!(
            未解析包候选("unresolved import `serde_json`"),
            vec!["serde_json"]
        );
        assert_eq!(
            未解析包候选("unresolved imports `tokio`, `tokio::time`, `std::io`"),
            vec!["tokio"]
        );
        assert!(未解析包候选("unresolved import `self::inner`").is_empty());
        assert!(未解析包候选("mismatched types").is_empty());
    }

    /// 母语标识符不是第三方库名：非半角段被过滤，不再误提示 `rzc add 数据模型`
    #[test]
    fn 测_未解析包候选过滤非半角名() {
        assert!(未解析包候选("unresolved import `数据模型::规则类型`").is_empty());
        assert!(
            未解析包候选("unresolved imports `数据模型`, `serde`")
                .iter()
                .all(|串| 串 == "serde")
        );
    }

    /// 错误码条目是完整桩：消息表命中也不能回拼英文残段
    ///（回归：E0624 把消息名与「是私有的」英文残段拼在一起）
    #[test]
    fn 测_码表条目整桩不回拼英文残段() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 表格文本 = r#"
[E0624]
"消息模板" = "字段或方法 `{名称}` 是私有的"
"教学提示" = "结构体字段默认私有。"

["消息翻译"."method `"]
"消息模板" = "方法 `{q0}` 从未被使用"
"#;
        let 档 = 造消息表临时档(表格文本);
        let 管理 = 错误消息管理器::从文件载入(档.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建(管理, 造标准类型表());

        let 诊断 = 编译器诊断 {
            诊断消息: "method `year` is private".to_string(),
            诊断码: Some(诊断代码 {
                码值: "E0624".to_string(),
                释义: None,
            }),
            级别: "error".to_string(),
            跨度集: vec![],
            子诊断: vec![],
            已渲染: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        // {名称} 由消息反引号内容回填；不得粘上消息名之后的英文残段
        assert_eq!(教学.译文消息, "字段或方法 `year` 是私有的");
    }

    /// 前缀键 + 动态名后的附加从句：首个捕获槽取反引号前内容而非第二个引号对
    ///（回归：把从句引导词错当成动态名输出）
    #[test]
    fn 测_特征从句捕获() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 表格文本 = r#"
["消息翻译"."trait `"]
"消息模板" = "特征 `{q0}` 从未被使用"
"#;
        let 档 = 造消息表临时档(表格文本);
        let 管理 = 错误消息管理器::从文件载入(档.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建(管理, 造标准类型表());

        let 诊断 = 编译器诊断 {
            诊断消息: "trait `Datelike` which provides `year` is never used".to_string(),
            诊断码: None,
            级别: "warning".to_string(),
            跨度集: vec![],
            子诊断: vec![],
            已渲染: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert_eq!(教学.译文消息, "特征 `Datelike` 从未被使用");
    }

    /// 后缀链 + 双占位符（复数变体）：头段引号对完整，首末捕获槽分别取首个/末个
    #[test]
    fn 测_变体后缀双占位捕获() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 表格文本 = r#"
["消息翻译"."~ are never constructed"]
"消息模板" = "`{q0}` 和 `{q1}` 从未被构造"
"#;
        let 档 = 造消息表临时档(表格文本);
        let 管理 = 错误消息管理器::从文件载入(档.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建(管理, 造标准类型表());

        let 诊断 = 编译器诊断 {
            诊断消息: "variants `黄灯` and `绿灯` are never constructed".to_string(),
            诊断码: None,
            级别: "warning".to_string(),
            跨度集: vec![],
            子诊断: vec![],
            已渲染: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert_eq!(教学.译文消息, "`黄灯` 和 `绿灯` 从未被构造");
    }

    /// 构造带主跨度标签的诊断（模拟 rustc JSON：主消息 + 主标签）
    fn 造带标签诊断(
        码: Option<&str>, 消息: &str, 标签: Option<&str>
    ) -> 编译器诊断 {
        编译器诊断 {
            诊断消息: 消息.to_string(),
            诊断码: 码.map(|每码| 诊断代码 {
                码值: 每码.to_string(),
                释义: None,
            }),
            级别: "error".to_string(),
            跨度集: vec![诊断跨度 {
                文件名称: "src/main.rs".to_string(),
                起始行号: 3,
                起始列号: 5,
                结束行号: 3,
                结束列号: 10,
                源码文本: None,
                起始字节: None,
                结束字节: None,
                是否主因: true,
                标签文本: 标签.map(|串| 串.to_string()),
                建议替换: None,
            }],
            子诊断: vec![],
            已渲染: None,
        }
    }

    /// 命令行/服务端主消息译文对齐：同一（错误码, 消息, 主跨度标签）下，
    /// 服务端复用的渲染主消息与命令行的翻译诊断主消息必须逐字一致，
    /// 杜绝同一诊断两处一详一略的分叉。
    #[test]
    fn 测_命令行与服务端主消息对齐() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 表格文本 = r#"
[E0308]
"消息模板" = "类型不匹配：期望 `{期望}`，实际得到 `{实际}`"
[E0433]
"消息模板" = "未找到类型 `{名称}`"
[E0369]
"消息模板" = "类型不匹配：无法对 `{期望}` 和 `{实际}` 执行运算"
[E0382]
"消息模板" = "值在移动后被使用：`{变量名}`"

["消息翻译"."unused variable: `"]
"消息模板" = "未使用的变量：`{q0}`"

["消息翻译"."mismatched types"]
"消息模板" = "类型不匹配"
"#;
        let 管理 = 错误消息管理器::从字符串载入(表格文本).unwrap();
        let 翻译器 = 诊断翻译器::新建(管理, 造标准类型表());

        // 均为“可完整渲染命中”的代表性用例（占位符可从标签/消息回填）；
        // 错误码桩无法回填时命令行回英文、服务端回消息表属预期分叉，不纳入对齐。
        let 案例集: Vec<(Option<&str>, &str, Option<&str>)> = vec![
            (
                Some("E0308"),
                "mismatched types",
                Some("expected `i32`, found `String`"),
            ),
            (Some("E0433"), "cannot find type `Foo` in this scope", None),
            (Some("E0369"), "cannot add `{integer}` to `&str`", None),
            (
                Some("E0382"),
                "use of moved value: `数据`",
                Some("value used here after move"),
            ),
            (None, "unused variable: `x`", None),
            (None, "mismatched types", None),
        ];
        for (码, 消息, 标签) in 案例集 {
            let 命令行 = 翻译器.翻译诊断(&造带标签诊断(码, 消息, 标签)).译文消息;
            let 服务端 = 翻译器
                .渲染主消息(码, 消息, 标签)
                .unwrap_or_else(|| panic!("服务端应命中表并渲染：{:?}", (码, 消息, 标签)))
                .文字;
            assert_eq!(
                命令行,
                服务端,
                "命令行/服务端主消息译文不一致，案例={:?}",
                (码, 消息, 标签)
            );
        }
    }

    /// 错误码模板的 `{模块名}` 必须从消息反引号内容回填，
    /// 不得输出字面 `{模块名}`（多层模块断裂假红回归）
    #[test]
    fn 测_模块名占位符回填() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 表格文本 = r#"
[E0583]
"消息模板" = "找不到模块 `{模块名}` 对应的文件"
"教学提示" = "模块声明要求同级目录存在同名文件。"
"#;
        let 管理 = 错误消息管理器::从字符串载入(表格文本).unwrap();
        let 翻译器 = 诊断翻译器::新建(管理, 造标准类型表());
        let 码 = Some("E0583");
        let 消息 = "file not found for module `工具`";

        let 渲染 = 翻译器
            .渲染主消息(码, 消息, None)
            .expect("服务端路径应命中码表并渲染");
        assert!(
            渲染.文字.contains("工具") && !渲染.文字.contains("{模块名}"),
            "服务端路径应回填模块名：{}",
            渲染.文字
        );

        let 命令行 = 翻译器.翻译诊断(&造带标签诊断(码, 消息, None)).译文消息;
        assert!(
            命令行.contains("工具") && !命令行.contains("{模块名}"),
            "命令行路径应回填模块名：{命令行}"
        );
    }

    /// 门禁：英文包是恒等语言，它的错误码模板与消息表守卫键必须逐字复现 rustc 自己的原文。
    ///
    /// 起因：英文包的 `[E0597]` 抄了 `[E0596]` 的文案（将“引用比原值活得久”误写成
    /// 无关的可变借用报错），而生命周期章节正要把这条诊断原样印进书里。检索按错误码
    /// 命中，模板与原文不符不会让既有测试失败，因此用实机捕获的（错误码, 原文）
    /// 断言“渲染结果 == 输入原文”来锁住这类抄写错误。末两条走消息表的同文守卫键，
    /// 同样必须一字不改。
    #[test]
    fn 测_英文身份包码模板复现原文() {
        // 消息取自 rustc 实机输出；反引号内的变量名随源程序变化
        let 案例集 = [
            ("E0106", "missing lifetime specifier"),
            (
                "E0596",
                "cannot borrow `s` as mutable, as it is not declared as mutable",
            ),
            ("E0597", "`s` does not live long enough"),
            (
                "E0277",
                "the `?` operator can only be used in a function that returns `Result` or `Option` (or another type that implements `FromResidual`)",
            ),
            ("E0277", "`?` couldn't convert the error to `SignupError`"),
        ];
        let 档集 = crate::语言::内置语言包文件("en");
        let 错误表 = 档集
            .iter()
            .find(|(档名, _)| *档名 == "errors.toml")
            .map(|(_, 内容)| *内容)
            .expect("en 语言包缺少 errors.toml");
        let 管理 = 错误消息管理器::从字符串载入(错误表).expect("en errors.toml 应能解析");
        // 空类型表：英文包不做类型名中文化，渲染结果应与 rustc 原文逐字相同
        let 翻译器 = 诊断翻译器::新建(管理, HashMap::new());
        for (码, 原文消息) in 案例集 {
            let 渲染 = 翻译器
                .渲染主消息(Some(码), 原文消息, None)
                .unwrap_or_else(|| panic!("en 包应命中 [{码}] 错误码模板"));
            assert_eq!(
                渲染.文字, 原文消息,
                "[{码}] 模板与 rustc 英文原文不一致，英语读者会看到答非所问的诊断"
            );
            assert!(渲染.提示.is_some(), "[{码}] 缺少教学提示");
        }
    }

    /// 门禁：`?` 的两条 E0277 消息必须走语言包的整句译文，不能被码表桩套坏。
    ///
    /// 起因：这两句自带 E0277 码，而 `[E0277]` 桩是「类型 `{类型}` 未实现特征
    /// `{特征}`」，槽位值按消息里反引号的出现顺序取——`?` 被当成类型、返回类型被
    /// 当成特征，渲染成答非所问的句子，而错误处理章节正要讲 `?`。语言包为两句
    /// 收录整句键后由消息表交接；本测试对各语言逐个断言「渲染结果不等于桩的按序
    /// 回填」，并钉住英文包逐字复现、中文包的实际译文、命令行与服务端同口径。
    #[test]
    fn 测_问号运算符消息绕过逐词猜测桩() {
        const 运算符消息: &str = "the `?` operator can only be used in a function that returns `Result` or `Option` (or another type that implements `FromResidual`)";
        const 转换消息: &str = "`?` couldn't convert the error to `SignupError`";

        /// 旧行为：把消息里的反引号内容按出现顺序塞进 `[E0277]` 桩的槽位
        fn 桩猜测(管理: &错误消息管理器, 消息: &str) -> String {
            let mut 模板 = 管理
                .按码查询("E0277")
                .expect("各语言包都应有 [E0277] 桩")
                .消息模板
                .clone();
            let mut 剩余 = 消息;
            let mut 词集 = Vec::new();
            while let Some(首处) = 剩余.find('`') {
                let 后半 = &剩余[首处 + 1..];
                let Some(尾处) = 后半.find('`') else {
                    break;
                };
                词集.push(&后半[..尾处]);
                剩余 = &后半[尾处 + 1..];
            }
            let mut 词迭代 = 词集.into_iter();
            for 占位符 in super::诊断翻译::码模板占位符集 {
                if 模板.contains(占位符) {
                    let Some(词) = 词迭代.next() else { break };
                    模板 = 模板.replace(占位符, 词);
                }
            }
            模板
        }

        let mut 已检查 = 0;
        for 语言 in crate::语言::内置语言代码() {
            let 错误表 = crate::语言::内置语言包文件(语言)
                .into_iter()
                .find(|(档名, _)| *档名 == "errors.toml")
                .map(|(_, 内容)| 内容)
                .unwrap_or_else(|| panic!("{语言} 语言包缺少 errors.toml"));
            let 管理 = 错误消息管理器::从字符串载入(错误表)
                .unwrap_or_else(|错| panic!("{语言} errors.toml 解析失败：{错}"));
            已检查 += 1;

            for 消息 in [运算符消息, 转换消息] {
                let 猜测 = 桩猜测(&管理, 消息);
                let 翻译器 = 诊断翻译器::新建(管理.clone(), HashMap::new());
                let 服务端 = 翻译器
                    .渲染主消息(Some("E0277"), 消息, None)
                    .unwrap_or_else(|| panic!("[{语言}] {消息:?} 应命中表并渲染"));
                assert_ne!(
                    服务端.文字, 猜测,
                    "[{语言}] {消息:?} 又被套进 [E0277] 桩的猜法：{}",
                    服务端.文字
                );
                assert!(
                    !服务端.文字.contains('{'),
                    "[{语言}] {消息:?} 漏出未回填的占位符：{}",
                    服务端.文字
                );
                assert!(服务端.提示.is_some(), "[{语言}] {消息:?} 缺少教学提示");

                // 命令行通道必须与服务端同口径：第二条是通配段键，回填原先被
                // “码表有条目就跳过”的守卫挡掉，只有命令行会漏出裸占位符。
                let 命令行 = 翻译器
                    .翻译诊断(&造带标签诊断(Some("E0277"), 消息, None))
                    .译文消息;
                assert_eq!(
                    命令行, 服务端.文字,
                    "[{语言}] {消息:?} 命令行/服务端不同口径"
                );

                if 语言 == "en" {
                    // 恒等语言：整句键是同文守卫键，必须一字不改
                    assert_eq!(服务端.文字, 消息, "英文包只能收录原句照抄的守卫键");
                } else if 语言 == "zh" {
                    let 应有 = if 消息 == 运算符消息 {
                        "运算符 `?` 只能用在返回 `结果` 或 `选项`（或另一个实现了 `FromResidual` 的类型）的函数中"
                    } else {
                        "`?` 无法把错误转换为 `SignupError`"
                    };
                    assert_eq!(服务端.文字, 应有, "中文整句译文与语言包不符");
                }
            }
        }
        assert!(已检查 >= 11, "内置语言应全部覆盖，实际 {已检查} 个");
    }

    /// 反证：整句译文优先**只**接管「消息表有整句译文 且 码表桩靠词元猜槽位」。
    ///
    /// 收紧规则前逐条实测过：放开「任意形态命中都优先」会让中文语料多条翻转，
    /// 其中数条把半句前缀键（「特征约束」「无法借用」）之后的动态部分拼回
    /// 成英文残段；放开「整句命中就优先（不看桩形态）」则会让下面两条退化——
    /// 一丢了期望/实际类型，二丢了「存在多个可满足要求的实现」这段教学措辞。
    /// 故此处反向钉住：码表桩更精确时仍用桩。
    #[test]
    fn 测_精确码桩优先于粗略消息条目() {
        let _守卫 = crate::语言::钉住语言("zh");
        let 错误表 = crate::语言::内置语言包文件("zh")
            .into_iter()
            .find(|(档名, _)| *档名 == "errors.toml")
            .map(|(_, 内容)| 内容)
            .expect("中文语言包缺少 errors.toml");
        let 管理 = 错误消息管理器::从字符串载入(错误表).unwrap();
        let 翻译器 = 诊断翻译器::新建(管理, HashMap::new());

        // 一：`[E0308]` 桩有 `{期望}`/`{实际}` 两个精确槽位，消息表只有静态「类型不匹配」
        let 每0308 = 翻译器
            .渲染主消息(
                Some("E0308"),
                "mismatched types",
                Some("expected `integer`, found `&str`"),
            )
            .expect("E0308 应渲染");
        assert!(
            每0308.文字.contains("整数") && 每0308.文字.contains("&str"),
            "E0308 丢失了期望/实际类型（消息表静态句抢了精确桩）：{}",
            每0308.文字
        );

        // 二：`[E0283]` 桩是静态教学句，消息表只有「需要类型标注」
        let 每0283 = 翻译器
            .渲染主消息(Some("E0283"), "type annotations needed", None)
            .expect("E0283 应渲染");
        assert_eq!(
            每0283.文字, "类型推断歧义：存在多个可满足要求的实现",
            "E0283 退化成只说「需要类型标注」"
        );

        // 三：消息表只有半句前缀键（尾巴段，不算整句），码表静态桩优先
        let 每0596 = 翻译器
            .渲染主消息(
                Some("E0596"),
                "cannot borrow `s` as mutable, as it is not declared as mutable",
                None,
            )
            .expect("E0596 应渲染");
        assert_eq!(
            每0596.文字, "无法借用为可变",
            "前缀键被当成整句，中文模板后粘上了英文残段"
        );
    }

    /// 门禁：错误码模板不得替 rustc 断言「成员种类」。
    ///
    /// 起因：英文包 `[E0603]` 写死「函数是私有的」，而 rustc 按实际种类输出
    /// 模块 / 常量 / 函数——当某个模块少了公开标记时，读者看到的是「函数 某某
    /// 是私有的」，模块系统章节正好教「沿路径逐层补公开标记」，答错种类会把读者
    /// 引向错误的一层。`[E0425]` 同型：rustc 有「找不到 值/函数 ...」多种形式，
    /// 英/日/德的模板写死「值」，遇到函数名、结构体名一律说错。
    ///
    /// 一份模板对多种原文，种类名词无法从码表还原，故统一改为不点名词类的
    /// 中立措辞——丢一个名词好过给一个错名词。条目名必须保留，否则读者不知道
    /// 路径上哪一层断了。十一语言现已全部收口：各语曾写作「模块是私有的」且不带
    /// 条目名，同样会答错种类、也指不出断点，现与英文同形（不点种类 + 保留条目名）。
    #[test]
    fn 测_码模板不误报条目种类() {
        // (语言, 错误码, rustc 原文, 模板里写死的种类词)
        let 案例集 = [
            ("en", "E0603", "module `shelf` is private", "function"),
            ("en", "E0603", "constant `SECRET` is private", "function"),
            // 十语言用常量名反证：模板不得再说「模块」，且必须保留条目名
            ("zh", "E0603", "constant `SECRET` is private", "模块"),
            ("ja", "E0603", "constant `SECRET` is private", "モジュール"),
            ("ru", "E0603", "constant `SECRET` is private", "модул"),
            ("de", "E0603", "constant `SECRET` is private", "Modul"),
            ("es", "E0603", "constant `SECRET` is private", "módulo"),
            ("fr", "E0603", "constant `SECRET` is private", "module"),
            ("pt", "E0603", "constant `SECRET` is private", "modulo"),
            ("ko", "E0603", "constant `SECRET` is private", "모듈"),
            ("hi", "E0603", "constant `SECRET` is private", "मॉड्यूल"),
            ("ar", "E0603", "constant `SECRET` is private", "وحدة"),
            (
                "en",
                "E0425",
                "cannot find function `wash_veg` in this scope",
                "value",
            ),
            (
                "ja",
                "E0425",
                "cannot find function `wash_veg` in this scope",
                "値",
            ),
            (
                "de",
                "E0425",
                "cannot find function `wash_veg` in this scope",
                "Wert",
            ),
        ];
        for (语言, 码, 原文消息, 错误种类词) in 案例集 {
            let 档集 = crate::语言::内置语言包文件(语言);
            let 错误表 = 档集
                .iter()
                .find(|(档名, _)| *档名 == "errors.toml")
                .map(|(_, 内容)| *内容)
                .unwrap_or_else(|| panic!("{语言} 语言包缺少 errors.toml"));
            let 管理 = 错误消息管理器::从字符串载入(错误表)
                .unwrap_or_else(|错| panic!("{语言} errors.toml 解析失败：{错}"));
            let 翻译器 = 诊断翻译器::新建(管理, HashMap::new());
            let 渲染 = 翻译器
                .渲染主消息(Some(码), 原文消息, None)
                .unwrap_or_else(|| panic!("{语言} 包应命中 [{码}] 错误码模板"));
            assert!(
                !渲染.文字.contains(错误种类词),
                "{语言} 包 [{码}] 对 {原文消息:?} 输出了错误的成员种类 {错误种类词:?}：{}",
                渲染.文字
            );
            let 条目名 = 原文消息
                .split('`')
                .nth(1)
                .expect("样例消息应含反引号内的条目名");
            assert!(
                渲染.文字.contains(条目名),
                "{语言} 包 [{码}] 丢失了条目名 {条目名:?}：{}",
                渲染.文字
            );
        }
    }

    /// 门禁：rustc 的「`--extern` 收到非半角包名」硬错必须在十语言都有译文。
    ///
    /// 起因：同时含库与主程序的方言项目用中文包名时，构建工具要用 `--extern` 把
    /// 库交给主程序，rustc 立刻拒绝（实测触发条件只是「两种目标共存 + 非半角包名」，
    /// 主程序完全不引用库也撞；只有库或只有主程序都能编）。这条无错误码的消息此前
    /// 十一语言全都没条目：终端喷英文原句，还被刷进待译清单，新手读不出「该改包名」。
    ///
    /// 断言用实机采集的原文，锁住通配段键命中与首末捕获槽回填；顺带锁一条渲染陷阱：
    /// 主消息曾在渲染完成后对全文反引号词元查类型映射，模板若写「把某英文名改成
    /// 半角名」会被改写成对应中文词条。现在类型中文化只作用于 rustc 注入的值，模板
    /// 字面标识符原样保留；中文模板当年为规避旧机制写的不带反引号措辞保留不动，
    /// 断言继续钉住「读者不会被告知去改一个不存在的东西」。
    #[test]
    fn 测_非半角外部包名全语言本地化() {
        use crate::映射管理器::映射管理器;
        use std::path::Path;
        // `rzc check src/main.zh`（包名 `计算器`，同时有 src/lib.zh）实机原文
        let 原文 = "crate name `计算器` passed to `--extern` is not a valid ASCII identifier";
        for 语言 in ["zh", "ja", "ru", "de", "es", "fr", "pt", "hi", "ko", "ar"] {
            let 档集 = crate::语言::内置语言包文件(语言);
            let 错误表 = 档集
                .iter()
                .find(|(档名, _)| *档名 == "errors.toml")
                .map(|(_, 内容)| *内容)
                .unwrap_or_else(|| panic!("{语言} 语言包缺少 errors.toml"));
            let 管理 = 错误消息管理器::从字符串载入(错误表)
                .unwrap_or_else(|错| panic!("{语言} errors.toml 解析失败：{错}"));
            let 翻译器 = 诊断翻译器::新建(管理, HashMap::new());
            let 渲染 = 翻译器
                .渲染主消息(None, 原文, None)
                .unwrap_or_else(|| panic!("{语言} 包应命中非半角包名的消息表"));
            assert!(
                渲染.文字.contains("计算器"),
                "{语言} 译文丢了包名：{}",
                渲染.文字
            );
            assert!(
                !渲染.文字.contains("is not a valid ASCII identifier"),
                "{语言} 译文残留英文原句：{}",
                渲染.文字
            );
        }

        // 中文走命令行真实渲染口径（带类型映射）：译文不得被改成「线程名」
        let 语言目录 = Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh");
        let 类型表 =
            构建类型映射(&映射管理器::从目录载入(&语言目录).expect("内置中文语言包应可加载"));
        let 档集 = crate::语言::内置语言包文件("zh");
        let 错误表 = 档集
            .iter()
            .find(|(档名, _)| *档名 == "errors.toml")
            .map(|(_, 内容)| *内容)
            .expect("中文语言包缺少 errors.toml");
        let 管理 = 错误消息管理器::从字符串载入(错误表).expect("中文应可解析");
        let 渲染 = 诊断翻译器::新建(管理, 类型表)
            .渲染主消息(None, 原文, None)
            .expect("中文应命中消息表");
        assert!(
            !渲染.文字.contains("线程名"),
            "中文模板里的反引号英文词被类型映射改写了，读者会被告知去改一个不存在的东西：{}",
            渲染.文字
        );
    }

    /// 存量清单：用各语言**真实**类型表扫出的「消息模板字面里会被旧机制改写」的
    /// 反引号词元，共十七处（按语言计），涉词均为指代源代码语法的英文名。
    ///
    /// 每行为（语言, 错误码或无, 送进渲染的原文, 模板字面词元）：扫描口径与渲染同构
    /// （先整项命中、再走三段式路径），故清单外的模板字面本来就不会被改写，不需列入。
    const 间接引用文本: &str =
        "insert some indirection (e.g., a `Box`, `Rc`, or `&`) to break the cycle";
    const 克隆消费文本: &str =
        "you can `clone` the value and consume it, but this might not be your desired behavior";
    const 缺主函数文本: &str = "`main` function not found in crate `demo`";
    const 异步外等待文本: &str = "`await` is only allowed inside `async` functions and blocks";

    const 模板字面清单: &[(&str, Option<&str>, &str, &str)] = &[
        ("ar", None, 间接引用文本, "Box"),
        ("ar", None, 间接引用文本, "Rc"),
        ("hi", Some("E0728"), 异步外等待文本, "await"),
        ("hi", None, 间接引用文本, "Box"),
        ("hi", None, 间接引用文本, "Rc"),
        ("ja", Some("E0601"), 缺主函数文本, "main"),
        ("ja", None, 间接引用文本, "Box"),
        ("ja", None, 间接引用文本, "Rc"),
        ("ja", None, 克隆消费文本, "clone"),
        ("ko", None, 间接引用文本, "Box"),
        ("ko", None, 间接引用文本, "Rc"),
        ("ko", None, 克隆消费文本, "clone"),
        ("ru", None, 间接引用文本, "Box"),
        ("ru", None, 间接引用文本, "Rc"),
        ("zh", Some("E0601"), 缺主函数文本, "main"),
        ("zh", None, 间接引用文本, "Box"),
        ("zh", None, 间接引用文本, "Rc"),
    ];

    /// 用指定语言的**内置语言包**与真实类型表构建翻译器（两条注入点门禁共用）。
    ///
    /// 同时返回该语言的类型表：跨十一种语言的那半段需要逐语言反查「该词的
    /// 真表译名是什么」，不靠人背；也不造合成映射。
    fn 按真实类型表造翻译器(语言: &str) -> (诊断翻译器, HashMap<String, String>) {
        use crate::映射管理器::映射管理器;
        use std::path::Path;
        let 语言目录 = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("lang-packs/{语言}"));
        let 类型表 = 构建类型映射(
            &映射管理器::从目录载入(&语言目录)
                .unwrap_or_else(|错| panic!("{语言} 语言包应可加载：{错}")),
        );
        let 档集 = crate::语言::内置语言包文件(语言);
        let 错误表 = 档集
            .iter()
            .find(|(档名, _)| *档名 == "errors.toml")
            .map(|(_, 内容)| *内容)
            .unwrap_or_else(|| panic!("{语言} 语言包缺少 errors.toml"));
        let 管理 = 错误消息管理器::从字符串载入(错误表)
            .unwrap_or_else(|错| panic!("{语言} errors.toml 解析失败：{错}"));
        (诊断翻译器::新建(管理, 类型表.clone()), 类型表)
    }

    /// 门禁：同一英文值的多个母语同义词，胜出者必须与进程无关，且取语言包里写在
    /// 最前的主词条。
    ///
    /// 起因：构建类型映射早期直接遍历哈希表取条目，而语言包允许同一英文值配多个
    /// 中文写法并存（都合法）——胜出者取决于随机哈希种子，同一份代码两次校验的译文
    /// 会翻转（实机多轮会翻数轮），读者拿译文去搜 rustc 文档偶尔搜不到。现按「来源
    /// 优先级 + 书写序」取首条：特征区写在格式化区之前，故该英文值反查回特征义那条。
    #[test]
    fn 测_同义词胜出者确定且随包序() {
        use crate::映射管理器::映射管理器;
        use std::path::Path;
        /// 两张反查表的胜出者差异（只列不一致的键）
        fn 胜出者差异(
            表甲: &HashMap<String, String>,
            表乙: &HashMap<String, String>,
        ) -> Vec<String> {
            let mut 键集: Vec<&String> = 表甲.keys().chain(表乙.keys()).collect();
            键集.sort();
            键集.dedup();
            键集
                .into_iter()
                .filter(|每键| 表甲.get(*每键) != 表乙.get(*每键))
                .map(|每键| format!("{每键}: {:?} → {:?}", 表甲.get(每键), 表乙.get(每键)))
                .collect()
        }

        let _守卫 = crate::语言::钉住语言("zh");
        let 按目录加载 = |语言: &str| {
            let 目录 = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("lang-packs/{语言}"));
            映射管理器::从目录载入(&目录).unwrap_or_else(|错| panic!("{语言} 语言包应可加载：{错}"))
        };
        for (语言, 主词条) in [
            ("zh", "可显示"),
            ("ja", "表示可能"),
            ("ko", "표시_가능"),
            ("ru", "отображаемый"),
            ("hi", "प्रदर्शनीय"),
            ("ar", "قابل_للعرض"),
        ] {
            let 本次 = 构建类型映射(&按目录加载(语言));
            assert_eq!(
                本次["Display"], 主词条,
                "{语言} 的反查应取语言包特征区那条主词条（同义词胜出者必须确定）"
            );
            let 又一次 = 构建类型映射(&按目录加载(语言));
            let 跨进程 = 胜出者差异(&本次, &又一次);
            assert!(
                跨进程.is_empty(),
                "{语言} 反查表跨进程不稳定，差异 {} 项：{:?}",
                跨进程.len(),
                跨进程.iter().take(10).collect::<Vec<_>>()
            );

            let 档集 = crate::语言::内置语言包文件(语言);
            let 取内容 = |档名: &str| {
                档集
                    .iter()
                    .find(|(每项, _)| *每项 == 档名)
                    .map(|(_, 内容)| *内容)
                    .unwrap_or_default()
            };
            let 第三方集: Vec<(&str, &str)> = 档集
                .iter()
                .filter(|(每项, _)| 每项.starts_with("crates/"))
                .map(|(每项, 内容)| (*每项, *内容))
                .collect();
            let 内置 = 构建类型映射(
                &映射管理器::从内置载入(
                    取内容("keywords.toml"),
                    取内容("module_paths.toml"),
                    取内容("stdlib.toml"),
                    &第三方集,
                )
                .unwrap_or_else(|错| panic!("{语言} 内置语言包应可加载：{错}")),
            );
            let 两套 = 胜出者差异(&本次, &内置);
            assert!(
                两套.is_empty(),
                "{语言} 的目录加载与内置加载给出了两套诊断译文用词，差异 {} 项：{:?}",
                两套.len(),
                两套.iter().take(10).collect::<Vec<_>>()
            );
        }
    }

    /// 门禁：反查表的英文值集合必须覆盖全部正向条目（收窄同义词不能少译）。
    ///
    /// 胜出者改成「按优先序取首」后，最容易犯的错是候选汇总漏掉某个来源——
    /// 表现不是报错，而是译文里凭空多出英文。这里拿正向表自己算应有集合：
    /// 关键词的类型节 + 模块路径 + 标识符三张表里，母语键条目的英文值必须
    /// 全部出现在反查表键集里。
    #[test]
    fn 测_反查候选覆盖每个正向条目() {
        use crate::映射源::是母语词条;
        use crate::映射管理器::映射管理器;
        use std::collections::HashSet;
        use std::path::Path;
        let _守卫 = crate::语言::钉住语言("zh");
        for 语言 in ["ar", "de", "es", "fr", "hi", "ja", "ko", "pt", "ru", "zh"] {
            let 目录 = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("lang-packs/{语言}"));
            let Ok(管理) = 映射管理器::从目录载入(&目录) else {
                continue;
            };
            let 应有: HashSet<String> = [
                管理.取节映射("类型"),
                Some(管理.取模块路径表()),
                Some(管理.取别名表()),
            ]
            .into_iter()
            .flatten()
            .flat_map(|表格| 表格.iter())
            .filter(|(母语, _)| 是母语词条(母语))
            .map(|(_, 英文)| 英文.clone())
            .collect();
            let 实际: HashSet<String> = 构建类型映射(&管理).keys().cloned().collect();
            let 丢失: Vec<&String> = 应有.iter().filter(|每键| !实际.contains(*每键)).collect();
            assert!(
                丢失.is_empty(),
                "{语言} 有 {} 个英文值没进诊断反查表（首：{:?}），这些词在译文里会保持英文",
                丢失.len(),
                丢失.first()
            );
        }
    }

    /// 门禁：语言包模板里作者手写的反引号英文标识符不得被类型映射改写。
    ///
    /// 起因：早期渲染会对结果全文扫反引号词元并查类型表，于是中文 `[E0601]` 的模板
    /// 「找不到某入口函数」被错误显示成「找不到 主函数 函数」——后者是方言写法，而这里
    /// 指的是编译器原句里的入口函数名。双重翻译后与编译器原句对不上，读者拿去搜文档也搜
    /// 不到。同类模板字面按真实类型表扫出十七处（见下方清单），全部是「指认源代码语法」，
    /// 必须保持英文——清单逐行钉住，新增同类模板会红。
    #[test]
    fn 测_模板字面标识符不被本地化改写() {
        let _守卫 = crate::语言::钉住语言("zh");

        // 码表桩路径：模板字面原样送达终端（实机 `[E0601]` 的编译器原文）
        let (翻译器, _) = 按真实类型表造翻译器("zh");
        let 渲染 = 翻译器
            .渲染主消息(Some("E0601"), 缺主函数文本, None)
            .expect("zh 应命中 [E0601] 码表桩");
        assert_eq!(渲染.文字, "找不到 `main` 函数");

        // 消息表路径：间接引用类型名不被改写成中文别名（`&` 不在真表里，本来就不改）
        let 渲染 = 翻译器
            .渲染主消息(None, 间接引用文本, None)
            .expect("zh 应命中该消息表条目");
        assert_eq!(
            渲染.文字,
            "插入一层间接引用（如 `Box`、`Rc` 或 `&`）以打破递归环"
        );

        // 存量清单逐行：模板字面的英文词元保留原样，母语改写不得出现在译文里
        let mut 已检 = 0;
        for 语言 in ["ar", "hi", "ja", "ko", "ru", "zh"] {
            let (翻译器, 类型表) = 按真实类型表造翻译器(语言);
            for (行语言, 码, 消息, 词元) in 模板字面清单 {
                if *行语言 != 语言 {
                    continue;
                }
                let 渲染 = 翻译器
                    .渲染主消息(*码, 消息, None)
                    .unwrap_or_else(|| panic!("{语言} 应命中含模板字面 `{词元}` 的条目：{消息}"));
                let 本地化名 = &类型表[*词元];
                assert!(
                    渲染.文字.contains(&format!("`{词元}`")),
                    "{语言} 的模板字面 `{词元}` 丢失，读者按它去搜文档就找不到：{}",
                    渲染.文字
                );
                assert!(
                    !渲染.文字.contains(本地化名),
                    "{语言} 的模板字面 `{词元}` 又被改写成 `{本地化名}`，双重翻译会让读者被告知去搜一个编译器从不写出的词：{}",
                    渲染.文字
                );
                已检 += 1;
            }
        }
        assert_eq!(
            已检,
            模板字面清单.len(),
            "清单里的行没被执行到（改了清单要同步上面的语言列表）"
        );
    }

    /// 门禁（反向）：编译器注入模板的值仍必须被本地化。
    ///
    /// 与上一个门禁成对：本轮只收窄「全文改写」的作用域，不能把功能一并改没——类型表
    /// 对真实类型名（`std::fmt::Display`、`&str`、`{integer}`）的本地化必须照旧生效，
    /// 三类注入点（`{期望}`/`{实际}`、`码模板占位符集`、`{qN}`）任一漏做本地化，母语读者
    /// 就会在自己的句子里看到夹着的英文类型名。下半段拿存量十七处清单的同名词元反向跑
    /// 一遍：这些词「写在模板里」要保留英文，「由编译器填进来」则必须译成母语名——两侧
    /// 共用一张表，防的是只改一边。
    ///
    /// 期望值直接硬写中文字面名：同一个英文值可以有多个母语同义词，但胜出者已由类型表构建
    /// 的确定序固定下来（见同义词胜出者那个门禁），这里才能把「译成哪个词」钉死；改成从
    /// 类型表反查就会把这部分断言变成空跑。
    #[test]
    fn 测_注入类型仍被本地化() {
        let _守卫 = crate::语言::钉住语言("zh");
        // 期望值硬写中文字面名，不取类型表：见上方函数注释
        let (翻译器, _) = 按真实类型表造翻译器("zh");

        // `{期望}`/`{实际}`：取自跨度标签与消息原文的裸类型串
        let 渲染 = 翻译器
            .渲染主消息(Some("E0308"), "expected `String`, found `&str`", None)
            .expect("zh 应命中 [E0308]");
        assert_eq!(
            渲染.文字, "类型不匹配：期望 `字符串`，实际得到 `字符串引用`",
            "`{{期望}}`/`{{实际}}` 的注入值未本地化（或同义词胜出者变了）"
        );

        // 码模板占位符集：`{类型}`/`{特征}` 按序取消息反引号内容，
        // 带模块路径的类型走三段式本地化（逐段换成母语名）
        let 渲染 = 翻译器
            .渲染主消息(
                Some("E0277"),
                "`点` doesn't implement `std::fmt::Display`",
                None,
            )
            .expect("zh 应命中 [E0277]");
        assert_eq!(
            渲染.文字, "类型 `点` 未实现特征 `标准库::格式化::可显示`",
            "带路径的注入值未走三段式本地化（应取特征区那条主词条）"
        );

        // 编译器的字面量占位符 `{integer}`：注入值同样要走字面量显示名替换
        let 渲染 = 翻译器
            .渲染主消息(Some("E0369"), "cannot add `{integer}` to `&str`", None)
            .expect("zh 应命中 [E0369]");
        assert_eq!(
            渲染.文字,
            // 本用例已跑在钉住中文之下，模板与字面量显示名都取中文字面
            "类型不匹配：无法对 `整数` 和 `字符串引用` 执行运算",
            "`{{integer}}` 字面量或 `&str` 未本地化"
        );

        // `{qN}` 通配段捕获：注入的包名保留、参数标识符不被当成类型改写
        let 渲染 = 翻译器
            .渲染主消息(
                None,
                "crate name `计算器` passed to `--extern` is not a valid ASCII identifier",
                None,
            )
            .expect("zh 应命中非半角包名的消息表条目");
        assert!(
            渲染.文字.contains("计算器") && 渲染.文字.contains("--extern"),
            "`{{qN}}` 注入值渲染异常：{}",
            渲染.文字
        );

        // 存量清单的同一批词元，换成编译器注入值（填进 `{期望}`）后必须被本地化：
        // 「写在模板里」与「由编译器填进来」走的是两套口径，只改一边就算不上修好。
        // `[E0308]` 的十一语言模板都写 `{期望}`/`{实际}`，拿它做载体；另一端填一个
        // 表里没有的假类型名，保证只有被检词元这一侧发生替换。
        let mut 已检 = 0;
        for 语言 in ["ar", "hi", "ja", "ko", "ru", "zh"] {
            let (翻译器, 类型表) = 按真实类型表造翻译器(语言);
            for (行语言, _, _, 词元) in 模板字面清单 {
                if *行语言 != 语言 {
                    continue;
                }
                let 本地化名 = &类型表[*词元];
                let 消息 = format!("expected `{词元}`, found `ZzQq`");
                let 渲染 = 翻译器
                    .渲染主消息(Some("E0308"), &消息, None)
                    .unwrap_or_else(|| panic!("{语言} 应命中 [E0308] 码表桩"));
                assert!(
                    渲染.文字.contains(&format!("`{本地化名}`")),
                    "{语言} 下编译器注入的 `{词元}` 未本地化成 `{本地化名}`（注入点漏做中文化，读者会在母语句子里看到英文）：{}",
                    渲染.文字
                );
                已检 += 1;
            }
        }
        assert_eq!(
            已检,
            模板字面清单.len(),
            "清单里的行没被执行到（改了清单要同步上面的语言列表）"
        );
    }

    /// 门禁：恒等语言的语言包只能携带「同文守卫键」，不得做真翻译。
    ///
    /// 起因：待译清单对恒等语言不登记，它的前提就是「该语言没有可用的译文」（命中不了
    /// 就原句显示，而原句已是最终形态）。谁哪天给它补了真正的消息翻译条目，这条前提就
    /// 不再成立——英语用户会退回「漏译却不说明」的黑箱。
    ///
    /// 但「一条都不许有」过头了：问号运算符的两条 `[E0277]` 消息自带错误码，而桩靠消息
    /// 里反引号的出现顺序猜槽位，若不在消息表里「站住位置」，英语读者看到的也是句不通的
    /// 话。所以改为允许同文守卫键：模板把 `{qN}` 还原成问号后必须与键逐字相同，等于
    /// 「不译，只截住桩」。其余十语言反向钉住：消息表非空。
    #[test]
    fn 测_恒等语言包仅携带同文守卫键() {
        for 语言 in crate::语言::内置语言代码() {
            let 档集 = crate::语言::内置语言包文件(语言);
            let 错误表 = 档集
                .iter()
                .find(|(每项, _)| *每项 == "errors.toml")
                .unwrap_or_else(|| panic!("{语言} 语言包缺少 errors.toml"))
                .1;
            let 值对象: toml::Value = toml::from_str(错误表)
                .unwrap_or_else(|坏| panic!("{语言} errors.toml 解析失败：{坏}"));
            let 消息表 = 值对象
                .as_table()
                .and_then(|表格| 表格.get("消息翻译"))
                .and_then(|内| 内.as_table());
            let 条目数 = 消息表.map(|表格| 表格.len()).unwrap_or(0);
            if crate::语言::是恒等语言(语言) {
                for (键名, 条目) in 消息表.into_iter().flatten() {
                    let 模板 = 条目
                        .get("消息模板")
                        .and_then(|每值| 每值.as_str())
                        .unwrap_or_else(|| panic!("{语言} 消息键「{键名}」缺「消息模板」"));
                    // 通配段键的问号在模板里表现为 `{qN}`，还原后应与键逐字相同
                    let mut 还原 = 模板.to_string();
                    for 序号 in 0..10 {
                        还原 = 还原.replace(&format!("{{q{序号}}}"), "?");
                    }
                    assert_eq!(
                        还原, *键名,
                        "{语言} 是恒等语言（待译清单不登记），消息表只允许同文守卫键：键「{键名}」的模板「{模板}」做了真翻译，英语读者会失去待译反馈"
                    );
                }
            } else {
                assert!(
                    条目数 > 0,
                    "{语言} 不是恒等语言，却没有任何消息翻译条目：待译清单会把它当成无需补表，母语读者的英文残留再也得不到反馈"
                );
            }
        }
    }

    /// 门禁：语言包的教学提示不得残留引擎占位符。
    ///
    /// 占位符回填只作用于「消息模板」，「教学提示」是从 errors.toml 原样送到终端的——
    /// 提示里写 `{名称}` 这类占位符不会被替换，读者就会在自己的母语指引中看到一串花括号。
    /// 起因：`[E0596]` 的英文提示里带了待替换的变量名占位，多个非中文语言包的 `[E0309]`
    /// 含 `{特征}`；直到《集合类型》一章引用它做实机核对时才暴露。
    #[test]
    fn 测_教学提示不残留引擎占位符() {
        fn 收集提示<'甲>(值对象: &'甲 toml::Value, 输出: &mut Vec<&'甲 str>) {
            match 值对象 {
                toml::Value::Table(映射) => {
                    for (键名, 子) in 映射.iter() {
                        if 键名 == "教学提示" {
                            if let Some(文字) = 子.as_str() {
                                输出.push(文字);
                            }
                        } else {
                            收集提示(子, 输出);
                        }
                    }
                }
                toml::Value::Array(项集) => {
                    for 项 in 项集 {
                        收集提示(项, 输出);
                    }
                }
                _ => {}
            }
        }

        let mut 总计 = 0;
        for 语言 in crate::语言::内置语言代码() {
            let 档集 = crate::语言::内置语言包文件(语言);
            let 错误表 = 档集
                .iter()
                .find(|(每项, _)| *每项 == "errors.toml")
                .map(|(_, 内容)| *内容)
                .unwrap_or_else(|| panic!("{语言} 语言包缺少 errors.toml"));
            let 已解析: toml::Value = toml::from_str(错误表)
                .unwrap_or_else(|坏| panic!("{语言} errors.toml 解析失败：{坏}"));
            let mut 提示集 = Vec::new();
            收集提示(&已解析, &mut 提示集);
            assert!(
                提示集.len() >= 20,
                "{语言} 包只取到 {} 条教学提示，清单提取可能有误",
                提示集.len()
            );
            for 提示 in &提示集 {
                for 占位符 in super::诊断翻译::码模板占位符集 {
                    assert!(
                        !提示.contains(占位符),
                        "{语言} 包的教学提示残留占位符 {占位符}，会原样输出给读者：{提示}"
                    );
                }
            }
            总计 += 提示集.len();
        }
        assert!(总计 > 0, "应至少检查一条教学提示");
    }
}
