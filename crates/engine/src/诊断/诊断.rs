// 诊断模块 - 解析 rustc 的 JSON 错误输出，结合语言包翻译成中文教学诊断信息
//
// 将 rustc 编译器产生的 JSON 格式诊断信息（--error-format=json）解析为结构化数据，
// 再根据语言包中的错误消息翻译表，生成面向中文教学场景的诊断输出，
// 包含错误码解释、教学提示、所有权错误叙事化详情等。
//
// 子模块划分（与本文件此前的分段注释一致）：
// - [`诊断模型`]：rustc JSON 诊断数据结构
// - [`消息翻译`]：错误消息翻译结构（errors.toml）
// - [`教学`]：教学诊断结构与所有权叙事化详情
// - [`翻译器`]：诊断翻译器（类型映射支持）
// - [`诊断输出`]：JSON 解析、格式化输出与未解析导入检测

#[path = "教学.rs"]
mod 教学;
#[path = "消息翻译.rs"]
mod 消息翻译;
#[path = "翻译器.rs"]
mod 翻译器;
#[path = "诊断模型.rs"]
mod 诊断模型;
#[path = "诊断输出.rs"]
mod 诊断输出;

pub use 教学::{
    所有权详情, 所有权错误码, 抽取所有权详情, 教学诊断, 诊断位置, 诊断级别
};
pub use 消息翻译::{
    填充动态占位符, 审计消息, 消息审计, 消息残段, 错误翻译管理器
};
pub use 翻译器::{构建类型映射, 渲染消息, 诊断翻译器};
pub use 诊断模型::{编译器诊断, 诊断码, 诊断跨度};
pub use 诊断输出::{
    抽取反引号首段, 是未解析导入消息, 未解析包候选, 格式化诊断, 解析诊断输出
};

// 内部辅助函数：仅测试直接引用（见下方 单元测试 模块）
#[cfg(test)]
pub(crate) use 翻译器::{
    整词替换, 替换类型段, 本地化引用类型, 本地化类型段, 计数引用前缀
};

#[cfg(test)]
#[allow(non_snake_case)]
mod 单元测试 {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn 创建错误消息文件(内容: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("创建临时文件失败");
        write!(file, "{}", 内容).expect("写入临时文件失败");
        file
    }

    fn 创建测试诊断() -> 编译器诊断 {
        编译器诊断 {
            诊断消息: "mismatched types".to_string(),
            诊断码: Some(诊断码 {
                码值: "E0308".to_string(),
                解释: None,
            }),
            诊断级别: "error".to_string(),
            跨度列表: vec![诊断跨度 {
                源文件名: "src/main.rs".to_string(),
                起始行: 3,
                起始列: 5,
                结束行: 3,
                结束列: 10,
                源码文本: Some(
                    serde_json::json!([{"text": "let x: i32 = \"hello\";", "highlight_start": 1, "highlight_end": 22}]),
                ),
                起始字节: Some(30),
                结束字节: Some(51),
                是主跨度: true,
                标签: Some("expected `i32`, found `&str`".to_string()),
                建议替换: None,
            }],
            子诊断: vec![编译器诊断 {
                诊断消息: "consider using a conversion function".to_string(),
                诊断码: None,
                诊断级别: "help".to_string(),
                跨度列表: vec![],
                子诊断: vec![],
                预渲染文本: None,
            }],
            预渲染文本: None,
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

    /// 构造指定标签的 E0308 诊断（基于标准测试诊断修改 标签）
    fn 创建带标签E0308(标签: &str) -> 编译器诊断 {
        let mut 诊断 = 创建测试诊断();
        诊断.跨度列表[0].标签 = Some(标签.to_string());
        诊断
    }

    #[test]
    fn 测试计数引用前缀() {
        assert_eq!(计数引用前缀("String"), 0);
        assert_eq!(计数引用前缀("&String"), 1);
        assert_eq!(计数引用前缀("&&String"), 2);
        // &mut 是单层可变引用，不算两层
        assert_eq!(计数引用前缀("&mut String"), 1);
        assert_eq!(计数引用前缀("&str"), 1);
    }

    #[test]
    fn 测试本地化引用类型() {
        let 映射 = 创建测试类型映射();
        // 整体命中 类型映射表（&str 是特殊整体条目）
        assert_eq!(本地化引用类型("&str", &映射), "字符串引用");
        // 拆 & 前缀 + 本体映射
        assert_eq!(本地化引用类型("&String", &映射), "&字符串");
        assert_eq!(本地化引用类型("&&i32", &映射), "&&有符号整数32");
        assert_eq!(本地化引用类型("&mut String", &映射), "&mut 字符串");
        // 无 & 前缀时原样返回
        assert_eq!(本地化引用类型("String", &映射), "字符串");
    }

    #[test]
    fn 测试E0308引用层数提示追加() {
        let _守卫 = crate::语言::测试语言("zh");
        // expected `&String`, found `String`：引用层数 1 vs 0，应追加提示
        let 诊断 = 创建带标签E0308("expected `&String`, found `String`");
        let 翻译器 = 诊断翻译器::新建翻译器(
            错误翻译管理器::从字符串载入("").unwrap(),
            创建测试类型映射(),
        );
        let 教学 = 翻译器.翻译诊断(&诊断);
        assert!(
            教学
                .教学提示
                .iter()
                .any(|提示| 提示.contains("引用层数不匹配")
                    && 提示.contains("&字符串")
                    && 提示.contains("字符串")),
            "应追加引用层数提示，实际提示：{:?}",
            教学.教学提示
        );
    }

    #[test]
    fn 测试E0308同层数不提示() {
        let _守卫 = crate::语言::测试语言("zh");
        // expected `&String`, found `&str`：层数 1 vs 1，不应追加提示
        let 诊断 = 创建带标签E0308("expected `&String`, found `&str`");
        let 翻译器 = 诊断翻译器::新建翻译器(
            错误翻译管理器::从字符串载入("").unwrap(),
            创建测试类型映射(),
        );
        let 教学 = 翻译器.翻译诊断(&诊断);
        assert!(
            !教学
                .教学提示
                .iter()
                .any(|提示| 提示.contains("引用层数不匹配")),
            "层数相同时不应追加提示，实际提示：{:?}",
            教学.教学提示
        );
    }

    #[test]
    fn 测试E0308无引用不提示() {
        let _守卫 = crate::语言::测试语言("zh");
        // expected `String`, found `i32`：均非引用，不应追加提示
        let 诊断 = 创建带标签E0308("expected `String`, found `i32`");
        let 翻译器 = 诊断翻译器::新建翻译器(
            错误翻译管理器::从字符串载入("").unwrap(),
            创建测试类型映射(),
        );
        let 教学 = 翻译器.翻译诊断(&诊断);
        assert!(
            !教学
                .教学提示
                .iter()
                .any(|提示| 提示.contains("引用层数不匹配")),
            "无引用时不应追加提示，实际提示：{:?}",
            教学.教学提示
        );
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
        let file = 创建错误消息文件(toml内容);
        let 管理器 = 错误翻译管理器::自文件加载(file.path()).unwrap();
        assert_eq!(管理器.覆盖数量(), 2);
    }

    #[test]
    fn 测试语言包加类型替换翻译() {
        let toml内容 = r#"
[E0308]
"消息模板" = "类型不匹配：期望 `{期望}`，实际得到 `{实际}`"
"教学提示" = "请检查变量类型。"
"#;
        let file = 创建错误消息文件(toml内容);
        let 管理器 = 错误翻译管理器::自文件加载(file.path()).unwrap();
        let 类型映射表 = 创建测试类型映射();
        let 翻译器 = 诊断翻译器::新建翻译器(管理器, 类型映射表);

        let 诊断 = 创建测试诊断();
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert_eq!(
            教学.翻译消息,
            "类型不匹配：期望 `有符号整数32`，实际得到 `字符串引用`"
        );
    }

    /// 无错误码消息：命中 [消息翻译] 节精确条目，标题与教学提示均翻译
    #[test]
    fn 测试无错误码命中消息表() {
        let _守卫 = crate::语言::测试语言("zh");
        let toml内容 = r#"
["消息翻译"."format argument must be a string literal"]
"消息模板" = "format 参数必须是字符串字面量"
"教学提示" = "第一个参数应带引号。"
"#;
        let file = 创建错误消息文件(toml内容);
        let 管理器 = 错误翻译管理器::自文件加载(file.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建翻译器(管理器, 创建测试类型映射());

        let 诊断 = 编译器诊断 {
            诊断消息: "format argument must be a string literal".to_string(),
            诊断码: None,
            诊断级别: "error".to_string(),
            跨度列表: vec![],
            子诊断: vec![],
            预渲染文本: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert_eq!(教学.翻译消息, "format 参数必须是字符串字面量");
        assert_eq!(教学.教学提示, vec!["第一个参数应带引号。"]);
    }

    /// 前缀匹配：help 短语 "did you mean " 保留动态后缀（`foo`?）
    #[test]
    fn 测试帮助前缀保留动态后缀() {
        let _守卫 = crate::语言::测试语言("zh");
        let toml内容 = r#"
["消息翻译"."did you mean "]
"消息模板" = "你是否想用 "
"#;
        let file = 创建错误消息文件(toml内容);
        let 管理器 = 错误翻译管理器::自文件加载(file.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建翻译器(管理器, 创建测试类型映射());

        let 诊断 = 编译器诊断 {
            诊断消息: "no method named `foo`".to_string(),
            诊断码: Some(诊断码 {
                码值: "E0599".to_string(),
                解释: None,
            }),
            诊断级别: "error".to_string(),
            跨度列表: vec![],
            子诊断: vec![编译器诊断 {
                诊断消息: "did you mean `foo`?".to_string(),
                诊断码: None,
                诊断级别: "help".to_string(),
                跨度列表: vec![],
                子诊断: vec![],
                预渲染文本: None,
            }],
            预渲染文本: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        // help 子诊断：前缀翻译 + 动态后缀保留，包上"修复建议："前缀
        assert_eq!(教学.教学提示, vec!["修复建议：你是否想用 `foo`?"]);
    }

    /// Unicode 混淆 help：{q0}/{q1} 捕获占位符从单引号分段提取两个字符
    #[test]
    fn 测试帮助Unicode引号捕获() {
        let _守卫 = crate::语言::测试语言("zh");
        let toml内容 = r#"
["消息翻译"."Unicode character '"]
"消息模板" = "Unicode 字符 '{q0}' 形似 '{q1}'，但它并不是它"
"#;
        let file = 创建错误消息文件(toml内容);
        let 管理器 = 错误翻译管理器::自文件加载(file.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建翻译器(管理器, 创建测试类型映射());

        let 诊断 = 编译器诊断 {
            诊断消息: "unknown start of token: \\u{ff0c}".to_string(),
            诊断码: None,
            诊断级别: "error".to_string(),
            跨度列表: vec![],
            子诊断: vec![编译器诊断 {
                诊断消息:
                    "Unicode character '，' (Fullwidth Comma) looks like ',' (Comma), but it is not"
                        .to_string(),
                诊断码: None,
                诊断级别: "help".to_string(),
                跨度列表: vec![],
                子诊断: vec![],
                预渲染文本: None,
            }],
            预渲染文本: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert_eq!(
            教学.教学提示,
            vec!["修复建议：Unicode 字符 '，' 形似 ','，但它并不是它"]
        );
    }

    /// "if you wanted to use a crate named" 建议：中文/非 ASCII 名静默丢弃
    ///（回归：中文模块名误触发 `add 规则类型` 建议，且英文原文直接透出）
    #[test]
    fn 测试帮助crate建议非ASCII丢弃() {
        let _守卫 = crate::语言::测试语言("zh");
        let file = 创建错误消息文件("");
        let 管理器 = 错误翻译管理器::自文件加载(file.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建翻译器(管理器, 创建测试类型映射());

        let 诊断 = 编译器诊断 {
            诊断消息: "unresolved import `不存在的中文模块`".to_string(),
            诊断码: Some(诊断码 {
                码值: "E0432".to_string(),
                解释: None,
            }),
            诊断级别: "error".to_string(),
            跨度列表: vec![],
            子诊断: vec![编译器诊断 {
                诊断消息: "if you wanted to use a crate named `不存在的中文模块`, use \
                          `cargo add 不存在的中文模块` to add it to your `Cargo.toml`"
                    .to_string(),
                诊断码: None,
                诊断级别: "help".to_string(),
                跨度列表: vec![],
                子诊断: vec![],
                预渲染文本: None,
            }],
            预渲染文本: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        // 非 ASCII “crate 名”的建议必然无效，应整体丢弃而非英文透出
        assert!(教学.教学提示.is_empty());
    }

    /// "if you wanted to use a crate named" 建议：ASCII crate 名翻译为中文
    /// 且重写为 `rzc add`（教学引导走方言工具链）
    #[test]
    fn 测试帮助crate建议ASCII() {
        let _守卫 = crate::语言::测试语言("zh");
        let toml内容 = r#"
["消息翻译"."if you wanted to use a crate named "]
"消息模板" = "若要使用名为 {q0} 的第三方库，请运行 `rzc add {q0}` 添加依赖"
"#;
        let file = 创建错误消息文件(toml内容);
        let 管理器 = 错误翻译管理器::自文件加载(file.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建翻译器(管理器, 创建测试类型映射());

        let 诊断 = 编译器诊断 {
            诊断消息: "unresolved import `serde_json`".to_string(),
            诊断码: Some(诊断码 {
                码值: "E0432".to_string(),
                解释: None,
            }),
            诊断级别: "error".to_string(),
            跨度列表: vec![],
            子诊断: vec![编译器诊断 {
                诊断消息: "if you wanted to use a crate named `serde_json`, use `cargo add \
                          serde_json` to add it to your `Cargo.toml`"
                    .to_string(),
                诊断码: None,
                诊断级别: "help".to_string(),
                跨度列表: vec![],
                子诊断: vec![],
                预渲染文本: None,
            }],
            预渲染文本: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert_eq!(
            教学.教学提示,
            vec![
                "修复建议：若要使用名为 serde_json 的第三方库，请运行 `rzc add serde_json` 添加依赖"
            ]
        );
    }

    /// 内置中文包：顶层 `让` 的解析错误应完整中文化（消息 + 建议 + 教学提示）
    ///
    /// 回归：'expected item, found keyword `let`' 只译出 '期望 ' 前缀、
    /// 'consider using `static`...' 只译出 '考虑 ' 前缀，中英混杂。
    #[test]
    fn 测试内置中文顶层let全译() {
        let _守卫 = crate::语言::测试语言("zh");
        let zh = crate::语言::内置文件("zh", "errors.toml").expect("内置中文错误表应存在");
        let 管理器 = 错误翻译管理器::从字符串载入(zh).unwrap();
        let 翻译器 = 诊断翻译器::新建翻译器(管理器, 创建测试类型映射());

        let 诊断 = 编译器诊断 {
            诊断消息: "expected item, found keyword `let`".to_string(),
            诊断码: None,
            诊断级别: "error".to_string(),
            跨度列表: vec![],
            子诊断: vec![编译器诊断 {
                诊断消息: "consider using `static` or `const` instead of `let`".to_string(),
                诊断码: None,
                诊断级别: "help".to_string(),
                跨度列表: vec![],
                子诊断: vec![],
                预渲染文本: None,
            }],
            预渲染文本: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert_eq!(教学.翻译消息, "此处应是一项声明，但遇到了 `让`");
        assert!(
            !教学.翻译消息.contains("expected"),
            "消息不得残留英文：{}",
            教学.翻译消息
        );
        assert_eq!(
            教学.教学提示,
            vec![
                "模块（顶层）作用域不能直接写 `让`：全局变量请改用 `静态` 或 `常量`，代码逻辑请放进函数（如 `函数 主函数()`）。",
                "修复建议：考虑改用 `静态` 或 `常量` 代替 `让`"
            ]
        );
    }

    /// 带模块路径的类型名：最长后缀匹配（std::fmt::Display → std::fmt::显示）
    #[test]
    fn 测试替换类型段最长后缀() {
        let mut 类型映射表 = 创建测试类型映射();
        类型映射表.insert("Display".into(), "显示".into());
        assert_eq!(
            替换类型段("std::fmt::Display", &类型映射表),
            Some("std::fmt::显示".to_string())
        );
        assert_eq!(替换类型段("std::io::Error", &类型映射表), None);
    }

    /// 三段式本地化：类型段 + 路径前缀 + 中间段全中文化
    #[test]
    fn 测试本地化类型段全路径() {
        let mut 映射 = 创建测试类型映射();
        映射.insert("Display".into(), "可显示".into());
        映射.insert("fmt".into(), "格式化".into());
        映射.insert("std".into(), "标准库".into());
        // 类型 + 路径 + 中间段全部命中
        assert_eq!(
            本地化类型段("std::fmt::Display", &映射),
            Some("标准库::格式化::可显示".to_string())
        );
        // 仅类型段命中：路径保持原样
        assert_eq!(
            本地化类型段("alloc::String", &映射),
            Some("alloc::字符串".to_string())
        );
        // 无任何段命中：保持原样返回 无（unknown 段不在映射中）
        assert_eq!(本地化类型段("unknown::io::Error", &映射), None);
        // 无 :: 路径的类型名（如 &str）由上层整串命中处理，此处不处理属预期
        映射.insert("&str".into(), "字符串引用".into());
        assert_eq!(本地化类型段("&str", &映射), None);
        assert_eq!(映射.get("&str"), Some(&"字符串引用".to_string()));
    }

    /// E0277 占位符填充后的实际类型名也应中文化（{类型}/{特征} → 中文 + 后缀匹配）
    #[test]
    fn 测试E0277占位符类型本地化() {
        let _守卫 = crate::语言::测试语言("zh");
        let toml内容 = r#"
[E0277]
"消息模板" = "类型 `{类型}` 未实现特征 `{特征}`"
"教学提示" = "请为该类型实现所需的特征。"
"#;
        let file = 创建错误消息文件(toml内容);
        let 管理器 = 错误翻译管理器::自文件加载(file.path()).unwrap();
        let mut 类型映射表 = 创建测试类型映射();
        类型映射表.insert("Display".into(), "显示".into());
        let 翻译器 = 诊断翻译器::新建翻译器(管理器, 类型映射表);

        let 诊断 = 编译器诊断 {
            诊断消息: "`({integer}, {integer}, &str)` doesn't implement `std::fmt::Display`"
                .to_string(),
            诊断码: Some(诊断码 {
                码值: "E0277".to_string(),
                解释: None,
            }),
            诊断级别: "error".to_string(),
            跨度列表: vec![],
            子诊断: vec![],
            预渲染文本: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert_eq!(
            教学.翻译消息,
            "类型 `(整数, 整数, &str)` 未实现特征 `std::fmt::显示`"
        );
    }

    /// 后缀键（~ 开头）：动态名在消息中间的 lint 警告（dead_code/non_snake_case）
    #[test]
    fn 测试后缀键从未使用() {
        let _守卫 = crate::语言::测试语言("zh");
        let toml内容 = r#"
["消息翻译"."function `"]
"消息模板" = "函数 `{q0}` 从未被使用"

["消息翻译"."~ is never used"]
"消息模板" = "`{q0}` 从未被使用"

["消息翻译"."~ should have an upper camel case name"]
"消息模板" = "`{q0}` 应使用大写驼峰命名"
"#;
        let file = 创建错误消息文件(toml内容);
        let 管理器 = 错误翻译管理器::自文件加载(file.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建翻译器(管理器, 创建测试类型映射());

        // 前缀键优先：含类型词 "函数"，比通用后缀键更完整
        let 诊断构造 = |消息: &str| 编译器诊断 {
            诊断消息: 消息.to_string(),
            诊断码: None,
            诊断级别: "warning".to_string(),
            跨度列表: vec![],
            子诊断: vec![],
            预渲染文本: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断构造("function `foo` is never used"));
        assert_eq!(教学.翻译消息, "函数 `foo` 从未被使用");

        // 无前缀键命中时走后缀键：动态名前缀保留在 {q0} 捕获中
        let 教学 = 翻译器.翻译诊断(&诊断构造(
            "type `myStruct` should have an upper camel case name",
        ));
        assert_eq!(教学.翻译消息, "`myStruct` 应使用大写驼峰命名");

        // 后缀键精确兜底：enum 无专用前缀键时用通用模板
        let 教学 = 翻译器.翻译诊断(&诊断构造("type `Foo` is never used"));
        assert_eq!(教学.翻译消息, "`Foo` 从未被使用");
    }

    /// rustc 1.97+ 算术错误（E0369）无 标签，类型嵌入 消息，
    /// 应回退提取并翻译 rustc 字面量占位符 `{integer}`
    #[test]
    fn 测试E0369无标签回退消息() {
        let _守卫 = crate::语言::测试语言("zh");
        let toml内容 = r#"
[E0369]
"消息模板" = "类型不匹配：无法对 `{期望}` 和 `{实际}` 执行运算"
"教学提示" = "运算符两侧的类型必须兼容。"
"#;
        let file = 创建错误消息文件(toml内容);
        let 管理器 = 错误翻译管理器::自文件加载(file.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建翻译器(管理器, 创建测试类型映射());

        let 诊断 = 编译器诊断 {
            诊断消息: "cannot add `{integer}` to `&str`".to_string(),
            诊断码: Some(诊断码 {
                码值: "E0369".to_string(),
                解释: None,
            }),
            诊断级别: "error".to_string(),
            跨度列表: vec![诊断跨度 {
                源文件名: "src/main.rs".to_string(),
                起始行: 3,
                起始列: 5,
                结束行: 3,
                结束列: 10,
                源码文本: None,
                起始字节: None,
                结束字节: None,
                是主跨度: true,
                标签: None,
                建议替换: None,
            }],
            子诊断: vec![],
            预渲染文本: None,
        };

        let 教学 = 翻译器.翻译诊断(&诊断);
        assert_eq!(
            教学.翻译消息,
            "类型不匹配：无法对 `整数` 和 `字符串引用` 执行运算"
        );
    }

    #[test]
    fn 测试解析JSON诊断输出() {
        let json行 = r#"{"message":"mismatched types","code":{"code":"E0308"},"level":"error","spans":[],"children":[],"rendered":null}"#;
        let 输出 = format!("{}\n{}", "    Compiling test v0.1.0", json行);
        let 诊断列表 = 解析诊断输出(&输出);
        assert_eq!(诊断列表.len(), 1);
        assert_eq!(诊断列表[0].诊断消息, "mismatched types");
    }

    #[test]
    fn 测试解析cargo包装诊断输出() {
        // cargo check --message-format=json 的 compiler-message 包装行：诊断嵌套在 message 字段
        let 包装行 = r#"{"reason":"compiler-message","package_id":"path+file:///test#e2e@0.1.0","manifest_path":"/test/Cargo.toml","target":{"kind":["bin"],"name":"e2e"},"message":{"message":"use of moved value: `数据`","code":{"code":"E0382"},"level":"error","spans":[],"children":[],"rendered":null}}"#;
        let 输出 = format!("{}\n{}", "    Checking e2e v0.1.0", 包装行);
        let 诊断列表 = 解析诊断输出(&输出);
        assert_eq!(诊断列表.len(), 1);
        assert_eq!(诊断列表[0].诊断码.as_ref().unwrap().码值, "E0382");
        assert_eq!(诊断列表[0].诊断消息, "use of moved value: `数据`");
    }

    #[test]
    fn 测试诊断级别转换() {
        assert_eq!(诊断级别::从文本解析("error"), 诊断级别::错误级);
        assert_eq!(诊断级别::从文本解析("warning"), 诊断级别::警告级);
        assert_eq!(诊断级别::从文本解析("help"), 诊断级别::帮助级);
    }

    /// 构造所有权错误诊断（rustc JSON 结构）
    fn 创建所有权诊断(
        错误码: &str,
        消息: &str,
        主跨度: 诊断跨度,
        其余跨度: Vec<诊断跨度>,
    ) -> 编译器诊断 {
        编译器诊断 {
            诊断消息: 消息.to_string(),
            诊断码: Some(诊断码 {
                码值: 错误码.to_string(),
                解释: None,
            }),
            诊断级别: "error".to_string(),
            跨度列表: std::iter::once(主跨度).chain(其余跨度).collect(),
            子诊断: vec![],
            预渲染文本: None,
        }
    }

    fn 创建跨度(起始行: u32, 标签: &str, 是主跨度: bool) -> 诊断跨度 {
        诊断跨度 {
            源文件名: "src/main.rs".to_string(),
            起始行,
            起始列: 5,
            结束行: 起始行,
            结束列: 10,
            源码文本: None,
            起始字节: None,
            结束字节: None,
            是主跨度,
            标签: Some(标签.to_string()),
            建议替换: None,
        }
    }

    /// 教学提示须全部输出
    ///
    /// 回归：`格式化为文本` 曾只用 `教学提示.first()`，第二条及以后的
    /// 教学提示被静默丢弃，而多处诊断（修复建议 + 所有权说明等）会产出多条。
    #[test]
    fn 测试格式化文本输出全部教学提示() {
        let 诊断 = 教学诊断 {
            严重级别: 诊断级别::错误级,
            错误码: Some("E0308".to_string()),
            翻译消息: "类型不匹配".to_string(),
            原始消息: "mismatched types".to_string(),
            教学提示: vec![
                "第一条提示".to_string(),
                "第二条提示".to_string(),
                "第三条提示".to_string(),
            ],
            位置列表: vec![诊断位置 {
                源文件名: "src/main.zh".to_string(),
                起始行: 3,
                起始列: 5,
                结束行: 3,
                结束列: 10,
                源码文本: Some("    让 x = 1;".to_string()),
                标签: None,
                是主跨度: true,
            }],
            子诊断: Vec::new(),
            所有权详情: None,
        };

        let 渲染全文 = 诊断.格式化为文本();
        assert_eq!(
            渲染全文.matches("💡").count(),
            3,
            "教学提示应全部输出，实际输出：{渲染全文}"
        );
        for 提示 in ["第一条提示", "第二条提示", "第三条提示"] {
            assert!(
                渲染全文.contains(提示),
                "教学提示 `{提示}` 丢失：{渲染全文}"
            );
        }
    }

    #[test]
    fn 测试抽取所有权详情E0382() {
        // 叙事文本 按当前语言取模板，需钉住 zh 并串行化
        let _守卫 = crate::语言::测试语言("zh");
        // rustc 对 E0382 输出：主 span 是再次使用处，副 span 标记移动发生
        let 诊断 = 创建所有权诊断(
            "E0382",
            "use of moved value: `数据`",
            创建跨度(5, "value used here after move", true),
            vec![创建跨度(
                3,
                "move occurs because `数据` has type `String`, which does not implement the `Copy` trait",
                false,
            )],
        );

        let 详情 = 抽取所有权详情("E0382", &诊断).expect("应提取出所有权详情");
        assert_eq!(详情.变量名, "数据");
        assert_eq!(详情.移动发生.as_ref().unwrap().起始行, 3);
        assert_eq!(详情.再次使用.as_ref().unwrap().起始行, 5);
        assert!(详情.借用发生.is_none());
        assert_eq!(
            详情.叙事文本(),
            "变量 `数据` 在第 3 行被移动，第 5 行尝试再次使用。"
        );
    }

    #[test]
    fn 测试抽取所有权详情E0502() {
        let _守卫 = crate::语言::测试语言("zh");
        // E0502：主 span 是可变借用处，另有不可变借用处与 borrow later used here
        let 诊断 = 创建所有权诊断(
            "E0502",
            "cannot borrow `向量` as mutable because it is also borrowed as immutable",
            创建跨度(6, "mutable borrow occurs here", true),
            vec![
                创建跨度(4, "immutable borrow occurs here", false),
                创建跨度(7, "borrow later used here", false),
            ],
        );

        let 详情 = 抽取所有权详情("E0502", &诊断).expect("应提取出所有权详情");
        assert_eq!(详情.变量名, "向量");
        assert_eq!(详情.借用发生.as_ref().unwrap().起始行, 6);
        // "borrow later used here" 应归为再次使用而非借用发生
        assert_eq!(详情.再次使用.as_ref().unwrap().起始行, 7);
        assert!(详情.移动发生.is_none());
        assert_eq!(
            详情.叙事文本(),
            "变量 `向量` 在第 6 行被借用，第 7 行仍在被使用。"
        );
    }

    #[test]
    fn 测试抽取所有权详情E0507() {
        let _守卫 = crate::语言::测试语言("zh");
        // E0507：主 span 即移动发生处，无再次使用位置
        let 诊断 = 创建所有权诊断(
            "E0507",
            "cannot move out of `数据` which is behind a shared reference",
            创建跨度(
                3,
                "move occurs because `数据` has type `String`, which does not implement the `Copy` trait",
                true,
            ),
            vec![],
        );

        let 详情 = 抽取所有权详情("E0507", &诊断).expect("应提取出所有权详情");
        assert_eq!(详情.变量名, "数据");
        assert_eq!(详情.移动发生.as_ref().unwrap().起始行, 3);
        assert!(详情.再次使用.is_none());
        assert_eq!(详情.叙事文本(), "变量 `数据` 在第 3 行被移动。");
    }

    #[test]
    fn 测试非所有权错误返回无() {
        // E0308 类型不匹配不是所有权错误，不应提取详情
        let 诊断 = 创建测试诊断();
        assert!(抽取所有权详情("E0308", &诊断).is_none());
    }

    #[test]
    fn 测试所有权详情序列化为JSON() {
        let 诊断 = 创建所有权诊断(
            "E0382",
            "use of moved value: `数据`",
            创建跨度(5, "value used here after move", true),
            vec![创建跨度(
                3,
                "move occurs because `数据` has type `String`",
                false,
            )],
        );
        let 详情 = 抽取所有权详情("E0382", &诊断).unwrap();

        let 数值 = serde_json::to_value(&详情).expect("所有权详情应可序列化");
        assert_eq!(数值["变量名"], "数据");
        assert_eq!(数值["移动发生"]["起始行"], 3);
        assert_eq!(数值["再次使用"]["起始行"], 5);
        assert_eq!(数值["借用发生"], serde_json::Value::Null);
    }

    #[test]
    fn 测试翻译诊断含所有权叙事() {
        let _守卫 = crate::语言::测试语言("zh");
        let toml内容 = r#"
[E0382]
"消息模板" = "值在移动后被使用：`{变量名}`"
"教学提示" = "Rust 中值被移动后不能再使用。"
"#;
        let 管理器 = 错误翻译管理器::从字符串载入(toml内容).unwrap();
        let 翻译器 = 诊断翻译器::新建翻译器(管理器, 创建测试类型映射());

        let 诊断 = 创建所有权诊断(
            "E0382",
            "use of moved value: `数据`",
            创建跨度(5, "value used here after move", true),
            vec![创建跨度(
                3,
                "move occurs because `数据` has type `String`",
                false,
            )],
        );
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert!(教学.所有权详情.is_some());
        let 详情 = 教学.所有权详情.as_ref().unwrap();
        assert_eq!(详情.变量名, "数据");

        let str = 教学.格式化为文本();
        assert!(str.contains("📌 变量 `数据` 在第 3 行被移动，第 5 行尝试再次使用。"));
        assert!(str.contains("💡 Rust 中值被移动后不能再使用。"));
    }

    /// 反引号首段提取：路径取首段，含空格的自由文本不提取
    #[test]
    fn 测试抽取反引号首段() {
        assert_eq!(
            抽取反引号首段("unresolved imports `a`, `b::c`"),
            vec!["a", "b"]
        );
        assert_eq!(
            抽取反引号首段("unresolved import `serde_json::Value`"),
            vec!["serde_json"]
        );
        assert!(抽取反引号首段("expected type `i32 x`").is_empty());
        assert!(抽取反引号首段("无反引号消息").is_empty());
    }

    /// 整词替换不误伤标识符子串；边界（串首/串尾/空格）正常命中
    #[test]
    fn 测试整词替换边界() {
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
    fn 测试是未解析导入消息() {
        assert!(是未解析导入消息("unresolved import `serde_json`"));
        assert!(是未解析导入消息(
            "failed to resolve: use of undeclared crate or module `tokio`"
        ));
        // 翻译后的母语消息不命中（由调用方按错误码兼容处理）
        assert!(!是未解析导入消息("未解析的导入 `serde_json`"));
        assert!(!是未解析导入消息("unused variable `x`"));
    }

    /// 候选 crate 提取：去重 + 排除标准库与保留路径；非目标消息返回空
    #[test]
    fn 测试未解析包候选() {
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

    /// 母语标识符不是 crate 名：非 ASCII 段被过滤，不再误提示 `rzc add 数据模型`
    #[test]
    fn 测试未解析包候选过滤非ASCII() {
        assert!(未解析包候选("unresolved import `数据模型::规则类型`").is_empty());
        assert!(
            未解析包候选("unresolved imports `数据模型`, `serde`")
                .iter()
                .all(|串| 串 == "serde")
        );
    }

    /// 错误码条目是完整桩：消息表命中也不能回拼英文残段
    ///（回归：E0624 输出 "字段或方法是私有的year` is private"）
    #[test]
    fn 测试码条目不回拼残段() {
        let _守卫 = crate::语言::测试语言("zh");
        let toml内容 = r#"
[E0624]
"消息模板" = "字段或方法 `{名称}` 是私有的"
"教学提示" = "结构体字段默认私有。"

["消息翻译"."method `"]
"消息模板" = "方法 `{q0}` 从未被使用"
"#;
        let file = 创建错误消息文件(toml内容);
        let 管理器 = 错误翻译管理器::自文件加载(file.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建翻译器(管理器, 创建测试类型映射());

        let 诊断 = 编译器诊断 {
            诊断消息: "method `year` is private".to_string(),
            诊断码: Some(诊断码 {
                码值: "E0624".to_string(),
                解释: None,
            }),
            诊断级别: "error".to_string(),
            跨度列表: vec![],
            子诊断: vec![],
            预渲染文本: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        // {名称} 由消息反引号内容回填；不得粘上 "year` is private" 英文残段
        assert_eq!(教学.翻译消息, "字段或方法 `year` 是私有的");
    }

    /// 前缀键 + 动态名后的附加短语：{q0} 取首个反引号前内容而非第二个引号对
    ///（回归：输出 "特征 ` which provides ` 从未被使用"）
    #[test]
    fn 测试特征which提供捕获() {
        let _守卫 = crate::语言::测试语言("zh");
        let toml内容 = r#"
["消息翻译"."trait `"]
"消息模板" = "特征 `{q0}` 从未被使用"
"#;
        let file = 创建错误消息文件(toml内容);
        let 管理器 = 错误翻译管理器::自文件加载(file.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建翻译器(管理器, 创建测试类型映射());

        let 诊断 = 编译器诊断 {
            诊断消息: "trait `Datelike` which provides `year` is never used".to_string(),
            诊断码: None,
            诊断级别: "warning".to_string(),
            跨度列表: vec![],
            子诊断: vec![],
            预渲染文本: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert_eq!(教学.翻译消息, "特征 `Datelike` 从未被使用");
    }

    /// 后缀链 + 双占位符（复数变体）：头段引号对完整，{q0}/{q1} 分别取首个/末个
    #[test]
    fn 测试变体后缀捕获() {
        let _守卫 = crate::语言::测试语言("zh");
        let toml内容 = r#"
["消息翻译"."~ are never constructed"]
"消息模板" = "`{q0}` 和 `{q1}` 从未被构造"
"#;
        let file = 创建错误消息文件(toml内容);
        let 管理器 = 错误翻译管理器::自文件加载(file.path()).unwrap();
        let 翻译器 = 诊断翻译器::新建翻译器(管理器, 创建测试类型映射());

        let 诊断 = 编译器诊断 {
            诊断消息: "variants `黄灯` and `绿灯` are never constructed".to_string(),
            诊断码: None,
            诊断级别: "warning".to_string(),
            跨度列表: vec![],
            子诊断: vec![],
            预渲染文本: None,
        };
        let 教学 = 翻译器.翻译诊断(&诊断);

        assert_eq!(教学.翻译消息, "`黄灯` 和 `绿灯` 从未被构造");
    }

    /// 构造带主 span 标签的诊断（模拟 rustc JSON：主消息 + primary 标签）
    fn 构造诊断(
        错误码: Option<&str>, 消息: &str, 标签: Option<&str>
    ) -> 编译器诊断 {
        编译器诊断 {
            诊断消息: 消息.to_string(),
            诊断码: 错误码.map(|码| 诊断码 {
                码值: 码.to_string(),
                解释: None,
            }),
            诊断级别: "error".to_string(),
            跨度列表: vec![诊断跨度 {
                源文件名: "src/main.rs".to_string(),
                起始行: 3,
                起始列: 5,
                结束行: 3,
                结束列: 10,
                源码文本: None,
                起始字节: None,
                结束字节: None,
                是主跨度: true,
                标签: 标签.map(|串| 串.to_string()),
                建议替换: None,
            }],
            子诊断: vec![],
            预渲染文本: None,
        }
    }

    /// CLI/LSP 主消息译文 parity（P1-1）：同一 (code, message, 主 span 标签) 下，
    /// LSP 复用的 `渲染主消息` 与 CLI 的 `翻译诊断` 主消息
    /// 必须逐字一致，杜绝“同诊断 CLI 详、LSP 略”的分叉。
    #[test]
    fn 测试CLILSP主消息一致() {
        let _守卫 = crate::语言::测试语言("zh");
        let toml内容 = r#"
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
        let 管理器 = 错误翻译管理器::从字符串载入(toml内容).unwrap();
        let 翻译器 = 诊断翻译器::新建翻译器(管理器, 创建测试类型映射());

        // 均为“可完整渲染命中”的代表性用例（占位符可从 标签/消息 回填）；
        // 错误码桩无法回填时 CLI 回英文、LSP 回消息表属预期分叉，不纳入 parity。
        let 用例: Vec<(Option<&str>, &str, Option<&str>)> = vec![
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
        for (错误码, 消息, 标签) in 用例 {
            let 命令行 = 翻译器.翻译诊断(&构造诊断(错误码, 消息, 标签)).翻译消息;
            let 编辑器 = 翻译器
                .渲染主消息(错误码, 消息, 标签)
                .unwrap_or_else(|| panic!("LSP 应命中表并渲染：{:?}", (错误码, 消息, 标签)))
                .主消息文本;
            assert_eq!(
                命令行,
                编辑器,
                "CLI/LSP 主消息译文不一致，case={:?}",
                (错误码, 消息, 标签)
            );
        }
    }

    /// E0583 码模板的 `{模块名}` 必须从消息反引号内容回填，
    /// 不得输出字面 `{模块名}`（多层模块断裂假红回归）
    #[test]
    fn 测试E0583模块名占位符填充() {
        let _守卫 = crate::语言::测试语言("zh");
        let toml内容 = r#"
[E0583]
"消息模板" = "找不到模块 `{模块名}` 对应的文件"
"教学提示" = "模块声明要求同级目录存在同名文件。"
"#;
        let 管理器 = 错误翻译管理器::从字符串载入(toml内容).unwrap();
        let 翻译器 = 诊断翻译器::新建翻译器(管理器, 创建测试类型映射());
        let 错误码 = Some("E0583");
        let 消息 = "file not found for module `工具`";

        let 渲染结果 = 翻译器
            .渲染主消息(错误码, 消息, None)
            .expect("E0583 应命中码表并渲染");
        assert!(
            渲染结果.主消息文本.contains("工具") && !渲染结果.主消息文本.contains("{模块名}"),
            "LSP 路径应回填模块名：{}",
            渲染结果.主消息文本
        );

        let 命令行 = 翻译器.翻译诊断(&构造诊断(错误码, 消息, None)).翻译消息;
        assert!(
            命令行.contains("工具") && !命令行.contains("{模块名}"),
            "CLI 路径应回填模块名：{命令行}"
        );
    }
}
