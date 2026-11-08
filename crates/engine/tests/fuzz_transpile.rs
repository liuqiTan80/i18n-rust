//! 转译管线模糊测试（proptest）
//!
//! 随机生成方言源码片段（关键字/标识符/数字/字符串/注释/空白/标点的任意
//! 组合，含畸形输入：未闭合字符串、孤立标点、RTL 字符等），验证转译管线
//! 的关键不变量：
//! - 永不 panic（任何输入都必须产出结果）；
//! - 不改变行结构（LSP 行号映射依赖：转译只做 token 级替换，不增删换行）；
//! - 源映射条目的偏移/长度全部落在源文本内，且 original 与源文本逐字节一致
//!   （LSP 列映射与诊断还原的坐标换算依赖此精确性）；
//! - pipeline_map 按源偏移严格升序（消费方回放依赖，乱序会破坏列映射）；
//! - 词法阶段的两张编辑表逐条等价（全管线表回放等于输出，词法表只差补的 `!`）；
//! - 幂等：转译输出再次转译保持不变（映射表无英文键、无来回抖动）。

use std::path::Path;

use i18n_rust_engine::映射管理器::映射管理器;
use i18n_rust_engine::词法器;
use i18n_rust_engine::转译管线;
use proptest::prelude::*;

/// 内置 zh 语言包映射管理器（与 CLI 默认一致）
fn zh_manager() -> 映射管理器 {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh");
    映射管理器::从目录载入(&dir).expect("加载 zh 语言包失败")
}

/// 方言源码片段策略：方言词表 + 畸形输入的任意组合
///
/// 词表覆盖各转译阶段：词法关键字（函数/让/打印行!/类型）、模块路径
/// （标准库::…）、标识符、字符串（含未闭合）、注释（行/块）、原始字符串、
/// 数字、标点、空白；另混入任意英文片段与任意中文片段（fuzz 词法边界判定
/// 与最长匹配逻辑）。
fn source_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just("函数 ".to_string()),
            Just("主函数".to_string()),
            Just("让 ".to_string()),
            Just("打印行!".to_string()),
            Just("整数".to_string()),
            Just("返回 ".to_string()),
            Just("使用 ".to_string()),
            Just("标准库::集合::映射".to_string()),
            Just("变量名".to_string()),
            Just("\"字符串内容\"".to_string()),
            Just("\"未闭合字符串".to_string()),
            Just("// 注释".to_string()),
            Just("/* 块注释 */".to_string()),
            Just("r#\"原始\"#".to_string()),
            Just("42".to_string()),
            Just(";".to_string()),
            Just("{".to_string()),
            Just("}".to_string()),
            Just("\n".to_string()),
            Just("\t".to_string()),
            Just("fn".to_string()),
            Just("let".to_string()),
            // 近期边界 bug 对抗词族：通配符/反引号/路径分隔符/空捕获/连续标点
            Just("?".to_string()),
            Just("??".to_string()),
            Just("`".to_string()),
            Just("``".to_string()),
            Just("::".to_string()),
            Just("src/子模块/主程序".to_string()),
            Just("->".to_string()),
            Just("=>".to_string()),
            Just("(( ))".to_string()),
            Just("，；".to_string()),
            "[a-z]{0,6}".prop_map(String::from),
            "[\u{4e00}-\u{9fff}]{0,4}".prop_map(String::from),
        ],
        1..150,
    )
    .prop_map(|parts| parts.concat())
}

/// 「方言词仅出现在串/注释内」的安全容器策略：每个片段整体是一个字符串
/// 字面量或块注释，内含在代码位会被翻译的方言关键词。整段都是 Literal/Comment
/// token，词法层绝不进入替换分支——锁死「串/注释内容不被误改」这一语义核心
/// 不变量（其余 fuzz 用例只保证不 panic / 行结构 / 坐标 / 幂等，而幂等恰恰
/// 察觉不了「串内被译成英文后二次仍不变」这类语义误改）。
fn quoted_dialect_source() -> impl Strategy<Value = String> {
    let 方言词 = prop::collection::vec(
        prop_oneof![
            Just("函数 ".to_string()),
            Just("让 ".to_string()),
            Just("使用 ".to_string()),
            Just("返回 ".to_string()),
            Just("类型 ".to_string()),
            Just("整数".to_string()),
            Just("布尔".to_string()),
            Just("标准库::集合::映射".to_string()),
            Just("打印行".to_string()),
            Just("声明 ".to_string()),
            Just("特征 ".to_string()),
        ],
        1..4,
    );
    let 容器 = prop_oneof![
        方言词
            .clone()
            .prop_map(|ws| format!(r#""{s}""#, s = ws.concat())),
        方言词.prop_map(|ws| format!("/* {s} */", s = ws.concat())),
    ];
    prop::collection::vec(容器, 1..12).prop_map(|v| v.join(" "))
}

proptest! {
    #![proptest_config(ProptestConfig {
        // 本地默认 256 用例；CI 全量测试中该文件同样执行（约 3-5 秒）
        cases: 256,
        ..ProptestConfig::default()
    })]

    /// 转译不 panic、行结构不变、源映射边界合法且 original 与源一致、pipeline_map 升序
    #[test]
    fn fuzz_pipeline_preserves_lines_and_maps(src in source_strategy()) {
        let manager = zh_manager();
        let output = 转译管线(&src, &manager);
        prop_assert_eq!(
            output.输出.matches('\n').count(),
            src.matches('\n').count(),
            "转译不得改变行结构（LSP 行号映射依赖）"
        );
        for entry in output.源映射.iter().chain(output.流程映射.iter()) {
            prop_assert!(entry.源偏移 <= src.len(), "源映射偏移越界");
            prop_assert!(
                entry.源偏移 + entry.源码长度 <= src.len(),
                "源映射长度越界"
            );
            prop_assert_eq!(
                &src[entry.源偏移..entry.源偏移 + entry.源码长度],
                &entry.原文,
                "源映射 original 与源文本不一致"
            );
        }
        // pipeline_map 源偏移严格升序（列映射消费方回放依赖）
        let mut prev: Option<usize> = None;
        for entry in &output.流程映射 {
            if let Some(p) = prev {
                prop_assert!(p < entry.源偏移, "pipeline_map 未按源偏移升序");
            }
            prev = Some(entry.源偏移);
        }
    }

    /// 词法阶段两张编辑表逐条等价（复制逻辑的回归锁，口径见 词法器 模块
    /// `build_lex_source_map`）
    ///
    /// 主循环只收全管线表、词法表循环后由它复制并按下标覆盖补 `!` 的差异条目。
    /// 复制逻辑漏条目、写错替换值、弄丢「词法表不含自动补的 `!`」这一差别，都会
    /// 在随机语料上暴露：① 全管线表回放必须逐字等于词法输出；② 两张表逐条同
    /// 偏移同原文、替换值只可能差一个自动补的 `!`，且不进词法表的条目只能是
    /// 「原文 + `!`」那一种。
    #[test]
    fn fuzz_lex_stage_tables_stay_in_lockstep(src in source_strategy()) {
        let manager = zh_manager();
        let result = 词法器::分词转译(
            &src,
            manager.取关键词表(),
            manager.取宏表(),
            manager.取派生特征表(),
            manager.取让位词(),
            manager.取方法让位词(),
            manager.取别名表(),
        );

        let mut replay = String::new();
        let mut copied = 0usize;
        for edit in &result.最终编辑表 {
            prop_assert!(
                edit.源偏移 >= copied,
                "全管线表必须按偏移升序且不重叠：{edit:?}"
            );
            replay.push_str(&src[copied..edit.源偏移]);
            replay.push_str(edit.替换文字.文本切片());
            copied = edit.源偏移 + edit.源码长度;
        }
        replay.push_str(&src[copied..]);
        prop_assert_eq!(replay, result.输出.clone(), "全管线表回放必须逐字等于输出");

        let mut lex = result.源映射.iter();
        let mut pending = lex.next();
        for edit in &result.最终编辑表 {
            if pending.is_some_and(|l| l.源偏移 == edit.源偏移) {
                let entry = pending.take().expect("上面 is_some 判定的就是这个条目");
                prop_assert_eq!(entry.源码长度, edit.源码长度, "两表条目长度必须一致");
                prop_assert_eq!(&entry.原文, &edit.原文, "两表条目原文必须一致");
                let diff = edit.替换文字.文本切片().strip_prefix(entry.替换文字.文本切片());
                prop_assert!(
                    diff == Some("") || diff == Some("!"),
                    "偏移 {}：词法表 {:?} 与全管线表 {:?} 只能差一个补的 `!`",
                    edit.源偏移,
                    entry.替换文字,
                    edit.替换文字
                );
                pending = lex.next();
            } else {
                let original = &src[edit.源偏移..edit.源偏移 + edit.源码长度];
                prop_assert_eq!(
                    edit.替换文字.文本切片(),
                    format!("{original}!"),
                    "不进词法表的条目只能是「原文只补 `!`」"
                );
            }
        }
        prop_assert!(pending.is_none(), "词法表不能有全管线表之外的条目");
    }

    /// 幂等：转译输出再次转译保持不变（映射无抖动）
    #[test]
    fn fuzz_transpile_idempotent_on_output(src in source_strategy()) {
        let manager = zh_manager();
        let once = 转译管线(&src, &manager).输出;
        let twice = 转译管线(&once, &manager).输出;
        prop_assert_eq!(once, twice, "转译输出应幂等");
    }

    /// 教学检查（全角标点/教学 lint）在任意输入上不 panic
    #[test]
    fn fuzz_teaching_checks_never_panic(src in source_strategy()) {
        let _ = i18n_rust_engine::全角标点::检测全角标点(&src);
        let _ = i18n_rust_engine::校验::检查教学(&src);
    }

    /// 方言词只要整体处于字符串/块注释内，转译输出必须逐字节不变
    /// （语义核心不变量：串/注释内容不得被转译；弥补幂等断言的盲区）
    #[test]
    fn fuzz_dialect_words_in_strings_and_comments_untouched(src in quoted_dialect_source()) {
        let manager = zh_manager();
        let output = 转译管线(&src, &manager).输出;
        prop_assert_eq!(&output, &src, "串/注释内的方言词不得被转译");
    }
}

/// 回归（#2）：模块路径词在表达式位生效
///
/// stdlib.toml【标识符】节的安全子集副本让表达式内的完整限定路径
/// （`标准库::输入输出::标准错误`）逐段转译；重名冲突词（路径/时间/
/// 同步/切片/格式化）不在此列，仍走 use 别名导入。
#[test]
fn test_module_path_words_in_expression_paths() {
    let manager = zh_manager();
    let src = "函数 主函数() {\n    让 e = 标准库::输入输出::标准错误();\n    让 a = 标准库::进程::参数();\n    让 m = 标准库::集合::哈希映射::新建();\n}";
    let out = 转译管线(src, &manager).输出;
    assert!(
        out.contains("std::io::stderr()"),
        "表达式路径段应逐段转译：{out}"
    );
    assert!(out.contains("std::process::args()"), "进程段应转译：{out}");
    assert!(
        out.contains("std::collections::HashMap::new()"),
        "集合段应转译：{out}"
    );
}

/// 回归（#8）：跨文件声明豁免（项目级声明上下文）
///
/// weix-1 实测场景：`平台Linux.zh` 声明 `pub 函数 新建()`，`平台接口.zh`
/// 调用 `包::平台Linux::Linux内存源::新建()`——单文件声明豁免看不到其他
/// 文件的声明，`新建` 被替换出 `new`（E0599，与声明侧 `fn 新建` 不一致）。
/// 项目上下文（同名模块 + 声明名）让调用位与声明位保持一致；
/// 库 API 路径段（`盒子::新建` → `Box::new`）不受影响。
#[test]
fn test_project_context_cross_file_declarations() {
    let manager = zh_manager();
    // 定义侧：平台Linux.zh 声明与映射词同名的项目函数
    let def_source = "pub 函数 新建() -> 自我 { 自我 { 状态: 0 } }";
    // 调用侧：其他文件走完整限定路径 + 库 API 路径
    let call_source = "函数 初始化() {\n    让 e = 包::平台Linux::Linux内存源::新建();\n    让 b = 盒子::新建();\n}";

    // 项目扫描：模块名=文件词干，声明名=全部文件汇总；
    // 定义侧声明的 `新建` 须入上下文（否则调用位无豁免）
    let ctx = i18n_rust_engine::别名::项目上下文::从源集合(
        std::collections::HashSet::from(["平台Linux".to_string(), "平台接口".to_string()]),
        [def_source, call_source],
        &manager,
    );
    assert!(
        ctx.名字集.contains("新建"),
        "项目扫描应收集定义侧声明：{ctx:?}"
    );
    let out = i18n_rust_engine::静默带项目转译管线(call_source, &manager, Some(&ctx)).输出;
    assert!(
        out.contains("crate::平台Linux::Linux内存源::新建()"),
        "跨文件调用位应与声明位一致（新建 不被替换）：{out}"
    );
    assert!(
        out.contains("Box::new()"),
        "库 API 路径段照常替换（盒子::新建 → Box::new）：{out}"
    );
    // 反例：无项目上下文（旧行为）时跨文件调用被替换出 `new`
    let out_no_ctx = 转译管线(call_source, &manager).输出;
    assert!(
        out_no_ctx.contains("crate::平台Linux::Linux内存源::new()"),
        "无上下文时保持旧行为：{out_no_ctx}"
    );
}

/// 回归：串/行注释/块注释/原始字符串内的方言词一律不被转译，而代码位的
/// `函数`→`fn` 照常翻译。独立于 proptest 随机用例的确定性锚点，
/// 即使 fuzz 配置缩减也始终执行。
#[test]
fn test_dialect_words_in_string_comment_raw_preserved() {
    let manager = zh_manager();
    let src = r##"函数 主函数() {
    // 让 使用 标准库::集合::映射 打印行
    let 名字 = "函数 让 标准库::集合 类型";
    /* 返回 特征 声明 */
    let 原始 = r"串里的 函数 让 不变";
}"##;
    let out = 转译管线(src, &manager).输出;
    // 代码位关键词正常翻译
    assert!(out.contains("fn "), "代码位 函数 应翻译为 fn：{out}");
    // 串/注释内容逐字节保留
    assert!(
        out.contains(r#""函数 让 标准库::集合 类型""#),
        "字符串内容须原样：{out}"
    );
    assert!(
        out.contains("// 让 使用 标准库::集合::映射 打印行"),
        "行注释须原样：{out}"
    );
    assert!(out.contains("/* 返回 特征 声明 */"), "块注释须原样：{out}");
    assert!(
        out.contains(r#"r"串里的 函数 让 不变""#),
        "原始字符串须原样：{out}"
    );
}

/// 回归（串/注释不变量 与 数字类型后缀状态机的交叉边界）：数字紧跟中文类型名
/// （真实 zh 数据：keywords 表 `无符号微整数`→u8、stdlib 表 `无符号机器整数`→usize）
/// 在代码位照常替换，但同形内容整体落在字符串内时必须逐字节不变。后缀替换只认
/// Literal token 的真实后缀区（字符串的 suffix_start 等于全长、后缀为空），借此
/// 锁死后缀状态机不误伤串内内容——弥补两条数字后缀单测仅用合成 map、未覆盖此交叉。
#[test]
fn test_number_type_suffix_in_string_preserved() {
    let manager = zh_manager();
    // 代码位：真实 zh 数据下数字+类型名应替换为 0u8 / 0usize
    let code_src = "让 x = 0无符号微整数;\n让 y = 0无符号机器整数;";
    let code_out = 转译管线(code_src, &manager).输出;
    assert!(code_out.contains("0u8"), "代码位应替换为 0u8：{code_out}");
    assert!(
        code_out.contains("0usize"),
        "代码位应替换为 0usize：{code_out}"
    );
    // 串内：同形的数字+类型名必须逐字节保留
    let str_src = r#"让 s = "0无符号微整数 0无符号机器整数";"#;
    let str_out = 转译管线(str_src, &manager).输出;
    assert!(
        str_out.contains(r#""0无符号微整数 0无符号机器整数""#),
        "串内数字类型名须原样：{str_out}"
    );
}
