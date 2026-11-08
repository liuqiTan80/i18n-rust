//! 门禁：诊断实跑反例语料的译文覆盖回归。
//!
//! 背景：消息表（errors.toml 的「消息翻译」节）覆盖哪些句子，静态检查永远看不全
//! ——只有把反例源码喂给真实 rustc 采集消息原文，才知道编译器今天会吐哪些话。
//! 本测试把这套「实跑采样」固化下来：
//!
//! - 语料 `data/diag-corpus.tsv` 由 `tools/diag-corpus/collect.py` 从
//!   `tools/diag-corpus/fixtures/*.rs` 采集生成（每行 `级别\t代码\t消息原文`）；
//!   rustc 新增/改动消息句子时，重跑采集器刷新语料即可暴露缺口。
//! - 审计判据 [`审计消息`] 与开发期诊断审计示例同源，渲染路径与
//!   `诊断翻译器` 一致，这里报出的残留即用户在终端实际看到的文字。
//!
//! 只要有人删键、改坏通配段匹配、或补了等于没补（模板回落英文原文），
//! 本测试立刻在 `cargo test --workspace` 里失败，无需再靠手工实跑发现。
//!
//! 例外：`en` 是直通语言（rustc 原文即英文，无「消息翻译」节属预期），跳过。

use std::collections::HashSet;

use i18n_rust_engine::诊断::{审计消息, 错误消息管理器};

/// 每行 `级别\t代码\t消息原文`（代码可为空）；裸行按主消息、空码处理。
fn 解析语料(原始: &str) -> Vec<(String, String, String)> {
    let mut 已见 = HashSet::new();
    let mut 解析结果 = Vec::new();
    for line in 原始
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let mut column = line.splitn(3, '\t');
        let (级别, 码, 消息) = match (column.next(), column.next(), column.next()) {
            (Some(级), Some(码), Some(消)) => (级, 码, 消),
            (Some(级), Some(消), None) => (级, "", 消),
            _ => ("main", "", line),
        };
        // 与诊断审计示例一致：按消息原文去重。
        if !已见.insert(消息.to_string()) {
            continue;
        }
        解析结果.push((级别.to_string(), 码.to_string(), 消息.to_string()));
    }
    解析结果
}

#[test]
fn 诊断反例语料在全部内置语言包中译文无残留() {
    let 原始 = include_str!("data/diag-corpus.tsv");
    let 语料 = 解析语料(原始);
    assert!(
        语料.len() >= 50,
        "语料过少（{} 条），疑似采集或签入异常",
        语料.len()
    );

    let mut 失败项: Vec<String> = Vec::new();
    let mut 已查语言数 = 0usize;
    for 语言码 in i18n_rust_engine::语言::内置语言代码() {
        if 语言码 == "en" {
            continue; // 直通语言，rustc 原文即英文，不查消息表
        }
        let 错误表 = i18n_rust_engine::语言::内置语言包文件(语言码)
            .into_iter()
            .find(|(名, _)| *名 == "errors.toml")
            .map(|(_, 内容)| 内容)
            .unwrap_or_else(|| panic!("{语言码} 语言包缺少 errors.toml"));
        let 管理器 = 错误消息管理器::从字符串载入(错误表)
            .unwrap_or_else(|故障值| panic!("{语言码}/errors.toml 解析失败：{故障值}"));
        已查语言数 += 1;
        for (级别, 码, 消息) in &语料 {
            let 审计 = 审计消息(&管理器, 级别, 码, 消息);
            if !审计.残留.is_empty() {
                失败项.push(format!(
                    "[{语言码}] {级别}/{码} 形态={} 残留={:?}\n    原文: {消息}\n    译文: {}",
                    审计.种类, 审计.残留, 审计.已渲染
                ));
            }
        }
    }

    assert!(
        已查语言数 >= 9,
        "应审计至少 9 个非 en 内置语言，实际 {已查语言数}"
    );
    assert!(
        失败项.is_empty(),
        "诊断反例语料存在未译全的句子（{} 处，涉及 {} 个语言）：\n{}",
        失败项.len(),
        已查语言数,
        失败项.join("\n")
    );
}

/// 语料必须同时覆盖主消息与修复建议子诊断两级
///
/// 帮助建议行是用户实际看到的中英夹杂重灾区（`did you mean`/`consider …` 全在
/// 子诊断里），但静态检查看不到它。采集器或语料一旦被重写，最容易整段丢掉
/// 帮助建议行而主消息检查仍然全绿——这里钉住两级下限，删光帮助建议就红。
#[test]
fn 语料同时覆盖主消息与帮助建议两级() {
    let 原始 = include_str!("data/diag-corpus.tsv");
    let 语料 = 解析语料(原始);
    let 计数 = |级别: &str| 语料.iter().filter(|(级, _, _)| 级 == 级别).count();
    let (主消息数, 建议数) = (计数("main"), 计数("help"));
    assert!(
        主消息数 >= 50,
        "主消息语料不应少于 50 条，实际 {主消息数} 条（疑似采集异常）"
    );
    assert!(
        建议数 >= 30,
        "help 子诊断语料不应少于 30 条，实际 {建议数} 条"
    );
    // 通道集合限定为已定义三种（主消息/帮助建议来自 rustc 实跑，工具通道为 cargo 纯文本），
    // 新通道必须同时在采集器与待译机制两侧登记，不能让语料默默长出无人审计的级别
    let 已知通道 = ["main", "help", "tool"];
    assert!(
        语料
            .iter()
            .all(|(级, _, _)| 已知通道.contains(&级.as_str())),
        "语料出现未知通道，采集器与门禁需同步"
    );
}
