//! 延迟项目上下文入口的等价性（`transpile_pipeline_with_deferred_project`）
//!
//! 老路径：先 `collect_source_declarations` 预扫本文件声明 → 合并其他文件声明
//! 建项目上下文 → 跑完整管线（本文件被词法转译两次）。
//! 新路径：管线先做词法阶段，本文件声明从词法产物顺带收集 → 回调合并其他
//! 文件声明 → 继续后续阶段（只转译一次）。
//!
//! 两条路径的项目上下文按构造相同，转译产物必须逐字节一致；本测试把它钉住，
//! 避免「顺带收集」在声明位启发式上与预扫版本漂移。

use std::collections::HashSet;
use std::path::Path;

use i18n_rust_engine::alias::{ProjectContext, collect_source_declarations};
use i18n_rust_engine::映射管理器::映射管理器;

fn zh_manager() -> 映射管理器 {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh");
    映射管理器::从目录载入(&dir).expect("加载 zh 语言包失败")
}

/// 甲文件：声明 `新建`（撞 `新建`=new 映射）与结构体字段
const JIA: &str = "结构体 平台 {
    端口: 整数,
}

函数 新建(端口: 整数) -> 平台 {
    返回 平台 { 端口 };
}
";

/// 乙文件：跨文件调用甲的成员（应豁免）+ 库 API 调用（应照常替换）
const YI: &str = "函数 启动() {
    让 实例 = 甲::新建(8080);
    让 盒 = 盒子::新建();
    让 值 = 实例.端口;
}
";

fn modules() -> HashSet<String> {
    ["甲".to_string(), "乙".to_string()].into_iter().collect()
}

#[test]
fn deferred_project_matches_precomputed_project() {
    let manager = zh_manager();
    let module_names = modules();
    let alias_map = manager.取别名表();
    assert!(
        alias_map.contains_key("新建") && alias_map.contains_key("盒子"),
        "测试前提：zh 语言包应含 `新建`/`盒子` 的别名映射"
    );

    // 老路径：先预扫两文件声明，再建上下文
    let 甲声明 = collect_source_declarations(JIA, &manager);
    let 乙声明 = collect_source_declarations(YI, &manager);
    let 上下文 = ProjectContext::from_declarations(module_names.clone(), [&甲声明, &乙声明]);
    let 老产物 = i18n_rust_engine::transpile_pipeline_with_map_and_project(
        YI,
        &manager,
        Some(&module_names),
        Some(&上下文),
    );
    assert!(
        老产物.output.contains("甲::新建(8080)"),
        "跨文件声明位应与声明侧一致：{}",
        老产物.output
    );
    assert!(
        老产物.output.contains("Box::new()"),
        "库 API 路径段照常替换：{}",
        老产物.output
    );

    // 新路径：本文件声明由词法阶段顺带产出，回调只合并其他文件声明
    let (新产物, 本文件声明, 新上下文) = i18n_rust_engine::transpile_pipeline_with_deferred_project(
        YI,
        &manager,
        Some(&module_names),
        |本文件| ProjectContext::from_declarations(module_names.clone(), [&甲声明, 本文件]),
    );

    assert_eq!(新产物.output, 老产物.output, "两条路径的译文必须逐字节一致");
    assert_eq!(
        新产物.pipeline_map.len(),
        老产物.pipeline_map.len(),
        "编辑表条目数必须一致"
    );
    assert_eq!(
        本文件声明.items, 乙声明.items,
        "顺带收集的项名必须与预扫一致"
    );
    assert_eq!(本文件声明.variables, 乙声明.variables);
    assert_eq!(本文件声明.members, 乙声明.members);
    assert_eq!(本文件声明.library_fns, 乙声明.library_fns);
    assert_eq!(
        新上下文.fingerprint(),
        上下文.fingerprint(),
        "两条路径的项目上下文指纹必须一致"
    );
}

/// 静默变体除告警外与标准变体完全一致
#[test]
fn quiet_variant_output_matches() {
    let manager = zh_manager();
    let module_names = modules();
    let 甲声明 = collect_source_declarations(JIA, &manager);
    let 乙声明 = collect_source_declarations(YI, &manager);
    let 上下文 = ProjectContext::from_declarations(module_names.clone(), [&甲声明, &乙声明]);

    let 带告警 = i18n_rust_engine::transpile_pipeline_with_map_and_project(
        YI,
        &manager,
        Some(&module_names),
        Some(&上下文),
    );
    let 静默 = i18n_rust_engine::transpile_pipeline_quiet_with_map_and_project(
        YI,
        &manager,
        Some(&module_names),
        Some(&上下文),
    );
    // 整份 TranspileOutput 比（derive PartialEq 覆盖 output/source_map/pipeline_map 三字段）：
    // 静默入口只关教学告警的日志输出，任何字段有差异都属于实现漂移
    assert_eq!(静默, 带告警, "静默变体与带告警变体的转译产物必须逐字段一致");
}
