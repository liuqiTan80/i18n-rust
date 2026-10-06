//! 转译管线模糊测试（proptest）
//!
//! 随机生成方言源码片段（关键字/标识符/数字/字符串/注释/空白/标点的任意
//! 组合，含畸形输入：未闭合字符串、孤立标点、RTL 字符等），验证转译管线
//! 的关键不变量：
//! - 永不 panic（任何输入都必须产出结果）；
//! - 不改变行结构（LSP 行号映射依赖：转译只做 token 级替换，不增删换行）；
//! - 源映射条目的偏移/长度全部落在源文本内，且 original 与源文本逐字节一致
//!   （LSP 列映射与诊断还原的坐标换算依赖此精确性）；
//! - 管线映射 按源偏移严格升序（消费方回放依赖，乱序会破坏列映射）；
//! - 幂等：转译输出再次转译保持不变（映射表无英文键、无来回抖动）。

use std::path::Path;

use i18n_rust_engine::映射管理::映射管理器;
use i18n_rust_engine::转译管线;
use proptest::prelude::*;

/// 内置 zh 语言包映射管理器（与 CLI 默认一致）
fn 内置管理器() -> 映射管理器 {
    let 目录 = Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh");
    映射管理器::自目录加载(&目录).expect("加载 zh 语言包失败")
}

/// 方言源码片段策略：方言词表 + 畸形输入的任意组合
///
/// 词表覆盖各转译阶段：词法关键字（函数/让/打印行!/类型）、模块路径
/// （标准库::…）、标识符、字符串（含未闭合）、注释（行/块）、原始字符串、
/// 数字、标点、空白；另混入任意英文片段与任意中文片段（fuzz 词法边界判定
/// 与最长匹配逻辑）。
fn 源码策略() -> impl Strategy<Value = String> {
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
    .prop_map(|片段列表| 片段列表.concat())
}

proptest! {
    #![proptest_config(ProptestConfig {
        // 本地默认 256 用例；CI 全量测试中该文件同样执行（约 3-5 秒）
        cases: 256,
        ..ProptestConfig::default()
    })]

    /// 转译不 panic、行结构不变、源映射边界合法且 original 与源一致、管线映射 升序
    #[test]
    fn 管线保行与映射不变(源串 in 源码策略()) {
        let 管理器 = 内置管理器();
        let 输出 = 转译管线(&源串, &管理器);
        prop_assert_eq!(
            输出.产出.matches('\n').count(),
            源串.matches('\n').count(),
            "转译不得改变行结构（LSP 行号映射依赖）"
        );
        for 条目项 in 输出.源映射.iter().chain(输出.管线映射.iter()) {
            prop_assert!(条目项.源偏移 <= 源串.len(), "源映射偏移越界");
            prop_assert!(
                条目项.源偏移 + 条目项.字节长度 <= 源串.len(),
                "源映射长度越界"
            );
            prop_assert_eq!(
                &源串[条目项.源偏移..条目项.源偏移 + 条目项.字节长度],
                &条目项.原文,
                "源映射 original 与源文本不一致"
            );
        }
        // 管线映射 源偏移严格升序（列映射消费方回放依赖）
        let mut 上个偏移: Option<usize> = None;
        for 条目项 in &输出.管线映射 {
            if let Some(前偏移) = 上个偏移 {
                prop_assert!(前偏移 < 条目项.源偏移, "pipeline_map 未按源偏移升序");
            }
            上个偏移 = Some(条目项.源偏移);
        }
    }

    /// 幂等：转译输出再次转译保持不变（映射无抖动）
    #[test]
    fn 转译输出幂等(源串 in 源码策略()) {
        let 管理器 = 内置管理器();
        let 首次 = 转译管线(&源串, &管理器).产出;
        let 二次 = 转译管线(&首次, &管理器).产出;
        prop_assert_eq!(首次, 二次, "转译输出应幂等");
    }

    /// 教学检查（全角标点/教学 lint）在任意输入上不 panic
    #[test]
    fn 教学检查永不崩溃(源串 in 源码策略()) {
        let _ = i18n_rust_engine::全角标点::查找全角标点(&源串);
        let _ = i18n_rust_engine::教学检查::执行教学检查(&源串);
    }
}

/// 回归（#2）：模块路径词在表达式位生效
///
/// stdlib.toml【标识符】节的安全子集副本让表达式内的完整限定路径
/// （`标准库::输入输出::标准错误`）逐段转译；重名冲突词（路径/时间/
/// 同步/切片/格式化）不在此列，仍走 use 别名导入。
#[test]
fn 模块路径词在表达式路径生效() {
    let 管理器 = 内置管理器();
    let 源串 = "函数 主函数() {\n    让 e = 标准库::输入输出::标准错误();\n    让 a = 标准库::进程::参数();\n    让 m = 标准库::集合::哈希映射::新建();\n}";
    let 产物 = 转译管线(源串, &管理器).产出;
    assert!(
        产物.contains("std::io::stderr()"),
        "表达式路径段应逐段转译：{产物}"
    );
    assert!(
        产物.contains("std::process::args()"),
        "进程段应转译：{产物}"
    );
    assert!(
        产物.contains("std::collections::HashMap::new()"),
        "集合段应转译：{产物}"
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
fn 跨文件声明项目上下文豁免() {
    let 管理器 = 内置管理器();
    // 定义侧：平台Linux.zh 声明与映射词同名的项目函数
    let 定义侧源码 = "pub 函数 新建() -> 自我 { 自我 { 状态: 0 } }";
    // 调用侧：其他文件走完整限定路径 + 库 API 路径
    let 调用侧源码 = "函数 初始化() {\n    让 e = 包::平台Linux::Linux内存源::新建();\n    让 b = 盒子::新建();\n}";

    // 项目扫描：模块名=文件词干，声明名=全部文件汇总；
    // 定义侧声明的 `新建` 须入上下文（否则调用位无豁免）
    let 项目上下文 = i18n_rust_engine::别名替换::项目上下文::自源文件新建(
        std::collections::HashSet::from(["平台Linux".to_string(), "平台接口".to_string()]),
        [定义侧源码, 调用侧源码],
        &管理器,
    );
    assert!(
        项目上下文.声明名.contains("新建"),
        "项目扫描应收集定义侧声明：{项目上下文:?}"
    );
    let 产物 =
        i18n_rust_engine::转译管线静默并项目(调用侧源码, &管理器, Some(&项目上下文)).产出;
    assert!(
        产物.contains("crate::平台Linux::Linux内存源::新建()"),
        "跨文件调用位应与声明位一致（新建 不被替换）：{产物}"
    );
    assert!(
        产物.contains("Box::new()"),
        "库 API 路径段照常替换（盒子::新建 → Box::new）：{产物}"
    );
    // 反例：无项目上下文（旧行为）时跨文件调用被替换出 `new`
    let 无上下文产物 = 转译管线(调用侧源码, &管理器).产出;
    assert!(
        无上下文产物.contains("crate::平台Linux::Linux内存源::new()"),
        "无上下文时保持旧行为：{无上下文产物}"
    );
}
