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
//! - 幂等：转译输出再次转译保持不变（映射表无英文键、无来回抖动）。

use std::path::Path;

use i18n_rust_engine::mapping_manager::MappingManager;
use i18n_rust_engine::transpile_pipeline;
use proptest::prelude::*;

/// 内置 zh 语言包映射管理器（与 CLI 默认一致）
fn zh_manager() -> MappingManager {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh");
    MappingManager::load_from_dir(&dir).expect("加载 zh 语言包失败")
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
            Just("，；".to_string()),
            "[a-z]{0,6}".prop_map(String::from),
            "[\u{4e00}-\u{9fff}]{0,4}".prop_map(String::from),
        ],
        1..150,
    )
    .prop_map(|parts| parts.concat())
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
        let output = transpile_pipeline(&src, &manager);
        prop_assert_eq!(
            output.output.matches('\n').count(),
            src.matches('\n').count(),
            "转译不得改变行结构（LSP 行号映射依赖）"
        );
        for entry in output.source_map.iter().chain(output.pipeline_map.iter()) {
            prop_assert!(entry.source_offset <= src.len(), "源映射偏移越界");
            prop_assert!(
                entry.source_offset + entry.length <= src.len(),
                "源映射长度越界"
            );
            prop_assert_eq!(
                &src[entry.source_offset..entry.source_offset + entry.length],
                &entry.original,
                "源映射 original 与源文本不一致"
            );
        }
        // pipeline_map 源偏移严格升序（列映射消费方回放依赖）
        let mut prev: Option<usize> = None;
        for entry in &output.pipeline_map {
            if let Some(p) = prev {
                prop_assert!(p < entry.source_offset, "pipeline_map 未按源偏移升序");
            }
            prev = Some(entry.source_offset);
        }
    }

    /// 幂等：转译输出再次转译保持不变（映射无抖动）
    #[test]
    fn fuzz_transpile_idempotent_on_output(src in source_strategy()) {
        let manager = zh_manager();
        let once = transpile_pipeline(&src, &manager).output;
        let twice = transpile_pipeline(&once, &manager).output;
        prop_assert_eq!(once, twice, "转译输出应幂等");
    }

    /// 教学检查（全角标点/教学 lint）在任意输入上不 panic
    #[test]
    fn fuzz_teaching_checks_never_panic(src in source_strategy()) {
        let _ = i18n_rust_engine::fullwidth::find_fullwidth_punct(&src);
        let _ = i18n_rust_engine::lint::lint_teaching(&src);
    }
}
