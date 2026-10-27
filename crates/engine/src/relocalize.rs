//! 反译重生（relocalize）：把标准 Rust 重生为目标方言
//!
//! 服务「跨母语阅读」：任一母语方言工程正向转译得到的标准 Rust，可再反译成
//! 另一母语方言，产出结构与源一致、可被 `rzc build` 编译的工程。本模块把此前
//! 只在 LSP/`dialect-convert.py` 原型里出现的反译能力产品化为引擎级纯函数。
//!
//! 实现复用 [`crate::lexer::reverse_transpile`]（token 感知、逐 token 匹配，
//! 天然避免子串误替换，注释与字符串内容原样保留）与其「排序最小者优先」的确定性
//! 反查语义：正向管线的英文产物，经关键字 + 宏 + 派生特征 + 别名四表合并反转
//! （英文 → 目标母语）后反译回方言。
//!
//! 保真与边界（须写进 `--help` 与文档，诚实标注）：
//! - 目标语言包缺该词条（尤其带 `::` 的多段模块路径如 `std::`）时，该段保留
//!   英文（可编译，只是未本地化）；
//! - 一词多母语映射的反查取确定候选（排序最小者），跨母语「阅读」可接受，
//!   不保证逐字节还原某一种子方言的原词；
//! - 输入应为标准 Rust（正向转译产物），故不携带代理添加的 `crate::` 前缀，
//!   反译时 `added_crate_tokens` 传空集。

use std::collections::{HashMap, HashSet};

use crate::lexer;
use crate::mapping_manager::MappingManager;

/// 合并目标语言包的关键字 / 宏 / 派生特征 / 别名四表，构建「英文 → 母语」反查表。
///
/// 与 `build_reverse_map` 同规则（每个英文键取排序最小的母语词，保证同一输入恒
/// 得同一输出），但按**表优先级** `关键字 > 宏 > 派生特征 > 别名` 依次填充：同一
/// 英文词被多表命中时保留更通用、更早的表项。否则一个核心词（如 `main`）可能被
/// 第三方 crate 映射（如 tokio 的 `メイン関数`）抢先反查，产物换出的母语词在不含
/// 该 crate 的普通工程里无法再转译回 `main`，破坏 `rzc build` 可编译性。
///
/// 恒等映射（母语 == 英文）与含 `::`/`.`/空格的复合值跳过——它们不是单个
/// Rust 标识符，交给词法反译无意义（模块路径段的本地化保留英文）。
pub(crate) fn reverse_map_of(target: &MappingManager) -> HashMap<String, String> {
    let mut reverse: HashMap<String, String> = HashMap::new();
    // 表优先级：越靠前越通用，先占者胜（or_insert 保留首个）
    for table in [
        target.get_keyword_map(),
        target.get_macro_map(),
        target.get_derive_map(),
        target.get_alias_map(),
    ] {
        // 表内按 (母语, 英文) 升序排序，令同一英文的多个母语候选取排序最小者（确定性）
        let mut entries: Vec<(&String, &String)> = table
            .iter()
            .filter(|(native, english)| {
                native != english
                    && english
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
            .collect();
        entries.sort();
        for (native, english) in entries {
            reverse
                .entry(english.clone())
                .or_insert_with(|| native.clone());
        }
    }
    reverse
}

/// 把标准 Rust 源码反译重生为目标方言（纯函数，无副作用、不 panic）。
///
/// `standard_rust` 为标准 Rust（英文）源码；`target` 为目标语言包的映射管理器。
/// 返回目标方言源码：结构与源一致，词法可被 `rzc` 正向转译回标准 Rust 后编译。
pub fn relocalize(standard_rust: &str, target: &MappingManager) -> String {
    let reverse = reverse_map_of(target);
    lexer::reverse_transpile(standard_rust, &reverse, &HashSet::new(), &HashSet::new())
}
