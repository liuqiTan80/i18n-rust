//! 别名阶段复用声明名的等价性
//!
//! 延迟项目上下文的管线入口（`延迟项目转译管线`）先在
//! **词法产物**上收集本文件声明，才能建项目上下文；别名阶段的输入却是
//! 「词法 → use 路径替换 → `crate::` 前缀」之后的文本。本条优化把前者的声明
//! 直接借给后者，省掉别名阶段内部那趟「分词 + 声明状态机」。
//!
//! 前提是「两处的声明集合相同」：后续两个阶段只改 use 段路径与路径根位前缀，
//! 不动声明关键字与声明位名字。本测试用两条路径的逐字节对比把它钉住：
//! - 阶段级：同一输入上「现场收集」vs「复用外部声明」；
//! - 管线级：延迟入口（在词法产物上收集）vs 预计算入口（别名阶段现场收集），
//!   项目上下文按构造相同，故唯一变量就是声明名的来源。

use std::collections::HashSet;
use std::path::Path;

use i18n_rust_engine::别名::{self, 项目上下文};
use i18n_rust_engine::映射管理器::映射管理器;
use i18n_rust_engine::缓存::源码映射条目;

fn zh_manager() -> 映射管理器 {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh");
    映射管理器::从目录载入(&dir).expect("加载 zh 语言包失败")
}

/// 虚拟项目的模块名集合（`crate::` 前缀阶段用）
fn modules() -> HashSet<String> {
    ["工具".to_string(), "辅助".to_string()]
        .into_iter()
        .collect()
}

/// 差分语料：覆盖 use 路径、`crate::` 前缀、声明位撞名、库特征实现块、
/// 模式绑定、闭包与 for 绑定、宏调用
const CORPUS: &[&str] = &[
    // ① use 路径 + 声明位撞名（`盒子`=Box、`字符串`=String、`新建`=new）
    "使用 标准库::文件系统::文件;
使用 工具::辅助::计数;
使用 标准库::集合::哈希映射;

结构体 盒子 {
    端口: 整数,
    名称: 字符串,
}

函数 新建(端口: 整数) -> 盒子 {
    盒子 { 端口, 名称: 字符串::新建() }
}
",
    // ② 库特征实现块：块内方法名是库 API（`显示`=Display），照常替换
    "结构体 计数器 {
    值: 整数,
}

impl 显示 for 计数器 {
    fn 显示(&self) -> 字符串 {
        格式化!(\"{}\", self.值)
    }
}
",
    // ③ 模式绑定：`连接`=join 被用作绑定名，必须豁免
    "函数 处理(结果: 结果<整数, 字符串>) {
    匹配 结果 {
        成功(连接) => 打印!(\"{连接}\"),
        错误(消息) => 打印!(\"{消息}\"),
    }
}
",
    // ④ for 绑定、闭包参数、方法链（`迭代`=iter、`收集`=collect）
    "函数 求和(项: 切片<整数>) -> 整数 {
    让 mut 总计 = 0;
    for 元素 in 项 {
        总计 += 元素;
    }
    让 加倍 = |值| 值 * 2;
    项.迭代().收集::<向量<整数>>()
}
",
    // ⑤ 声明位与库 API 撞名：`长度`=len 作常量与参数名
    "类型 回调 = fn(整数) -> 整数;

常量 长度: 整数 = 3;

函数 计算(长度: 整数) -> 整数 {
    长度 * 2
}
",
    // ⑥ 路径根位：项目模块名（`crate::` 前缀）+ 模块路径表回退
    "函数 入口() {
    工具::辅助::运行();
    辅助::工具::运行();
    标准库::打印!(\"hi\");
    让 盒 = 盒子::默认();
}
",
    // ⑦ use 别名导入：`as` 后是同名声明位（`长度`=len），路径段替换后
    // 仍不得改变声明集合
    "使用 标准库::集合::哈希映射 as 长度;
使用 工具::辅助 as 辅助模块;

函数 统计(数据: 长度<字符串, 整数>) -> 整数 {
    数据.长度()
}
",
];

fn assert_edits_eq(label: &str, a: &[源码映射条目], b: &[源码映射条目]) {
    assert_eq!(a.len(), b.len(), "{label}：编辑表条目数不一致");
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        assert_eq!(x.源偏移, y.源偏移, "{label}：第 {i} 条偏移");
        assert_eq!(x.源码长度, y.源码长度, "{label}：第 {i} 条长度");
        assert_eq!(x.原文, y.原文, "{label}：第 {i} 条原文");
        assert_eq!(x.替换文字, y.替换文字, "{label}：第 {i} 条替换文本");
    }
}

/// 阶段级等价：`declared = Some(外部收集)` 与 `None`（现场收集）逐字段一致
#[test]
fn reuse_declared_matches_fresh_collection() {
    let manager = zh_manager();
    let alias_map = manager.取别名表();
    let module_path_map = manager.取模块路径表();
    let module_names = modules();
    let mut with_edits = 0;
    for (idx, source) in CORPUS.iter().enumerate() {
        let declared = 别名::收集声明名(source, alias_map);
        let project = 项目上下文::从声明集合(module_names.clone(), [&declared]);
        let fresh = 别名::带上下文与模块路径替换别名(
            source,
            alias_map,
            module_path_map,
            Some(&project),
        );
        let reused = 别名::按声明替换别名(
            source,
            alias_map,
            module_path_map,
            Some(&project),
            Some(&declared),
        );
        let label = format!("语料 #{idx}");
        assert_eq!(fresh.输出文本, reused.输出文本, "{label}：译文不一致");
        assert_edits_eq(&label, &fresh.编辑表, &reused.编辑表);
        if !fresh.编辑表.is_empty() {
            with_edits += 1;
        }
    }
    assert_eq!(with_edits, CORPUS.len(), "语料应全部含替换候选（避免空跑）");
}

/// 管线级等价：延迟入口（声明取自词法产物）与预计算入口（别名阶段现场收集）
/// 在同一项目上下文下产出完全相同的译文与全管线地图
#[test]
fn deferred_entry_matches_precomputed_entry() {
    let manager = zh_manager();
    let module_names = modules();
    let mut with_edits = 0;
    for (idx, source) in CORPUS.iter().enumerate() {
        let declared = 别名::收集源声明(source, &manager);
        let project = 项目上下文::从声明集合(module_names.clone(), [&declared]);

        let precomputed = i18n_rust_engine::带映射与项目转译管线(
            source,
            &manager,
            Some(&module_names),
            Some(&project),
        );
        let (deferred, 本文件声明, 上下文) = i18n_rust_engine::静默延迟项目转译管线(
            source,
            &manager,
            Some(&module_names),
            |本文件| 项目上下文::从声明集合(module_names.clone(), [本文件]),
        );

        let label = format!("语料 #{idx}");
        assert_eq!(precomputed.输出, deferred.输出, "{label}：译文不一致");
        assert_eq!(
            上下文.指纹(),
            project.指纹(),
            "{label}：项目上下文指纹不一致"
        );
        assert_eq!(
            本文件声明.条目集, declared.条目集,
            "{label}：项名集合不一致"
        );
        assert_eq!(本文件声明.变量集, declared.变量集);
        assert_eq!(本文件声明.成员集, declared.成员集);
        assert_eq!(本文件声明.库方法集, declared.库方法集);
        assert_edits_eq(&label, &precomputed.流程映射, &deferred.流程映射);
        if !precomputed.流程映射.is_empty() {
            with_edits += 1;
        }
    }
    assert_eq!(with_edits, CORPUS.len(), "语料应全部产出编辑（避免空跑）");
}
