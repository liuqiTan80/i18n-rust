//! 诊断译文残留审计：用引擎自身的匹配器判定每条 rustc 消息原文能否译全。
//!
//! 存在的原因：消息表（errors.toml 的「消息翻译」节）覆盖哪些句子，静态检查
//! 只能看词条形态，看不出「真实编译器会吐哪些句子」。本入口把两者接起来：
//! 输入一批消息原文（由 `tools/diag-corpus/` 采集），输出每条的命中形态与
//! 渲染后仍残留的英文片段，用于批量定位缺口。
//!
//! 渲染路径与 `诊断翻译器` 完全一致（模板 → `{qN}` 填充 → 未消费则
//! 回拼残段），因此这里报出的残留就是用户在终端里实际看到的文字。
//!
//! 用法：
//!   `cargo run -p i18n-rust-engine --example diag_audit --` \
//!     `--errors crates/engine/lang-packs/zh/errors.toml` `--messages /tmp/msgs.txt`
//!
//! 审计判据已下沉为库函数 [`审计消息`]（门禁测试 `tests/diag_corpus_gate.rs`
//! 与采样反例语料共用同一实现，避免示例与测试漂移）。
use std::collections::HashSet;
use std::path::PathBuf;

use i18n_rust_engine::诊断::{审计消息, 错误消息管理器};

fn main() {
    let mut 错误表路径: Option<PathBuf> = None;
    let mut 消息文件: Option<PathBuf> = None;
    let mut 参数 = std::env::args().skip(1);
    while let Some(标志) = 参数.next() {
        let mut 取值 = |名称: &str| {
            if 标志 == 名称 {
                参数.next().map(PathBuf::from)
            } else {
                None
            }
        };
        if let Some(路径) = 取值("--errors") {
            错误表路径 = Some(路径);
        } else if let Some(路径) = 取值("--messages") {
            消息文件 = Some(路径);
        }
    }
    let (Some(错误表路径), Some(消息文件)) = (错误表路径, 消息文件) else {
        eprintln!("用法: diag_audit --errors <errors.toml> --messages <每行一条消息原文>");
        std::process::exit(2);
    };

    let 管理器 = 错误消息管理器::从文件载入(&错误表路径)
        .unwrap_or_else(|故障值| panic!("无法加载 {}: {故障值:?}", 错误表路径.display()));
    let 原始清单 = std::fs::read_to_string(&消息文件)
        .unwrap_or_else(|故障值| panic!("无法打开 {}: {故障值:?}", 消息文件.display()));

    let mut 已见 = HashSet::new();
    let (mut 总数, mut 完全未命中, mut 部分残留, mut 已译全) = (0usize, 0usize, 0usize, 0usize);
    // 每行格式：`LEVEL<TAB>CODE<TAB>消息原文`（采集器标注；代码可为空）；
    // 也兼容 `LEVEL<TAB>消息` 与裸消息行。
    for line in 原始清单
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
        if !已见.insert(消息.to_string()) {
            continue;
        }
        总数 += 1;
        let 审计 = 审计消息(&管理器, 级别, 码, 消息);
        if 审计.残留.is_empty() {
            已译全 += 1;
            continue;
        }
        match 审计.种类.as_str() {
            "MISS" => 完全未命中 += 1,
            _ => 部分残留 += 1,
        }
        println!("{级别}\t{}\t{消息}", 审计.种类);
        println!("  译文: {}", 审计.已渲染);
        println!("  残留: {}", 审计.残留.join(" | "));
    }
    println!("# 合计 {总数} 条：完全未命中 {完全未命中}、部分残留 {部分残留}、已译全 {已译全}");
    if 完全未命中 + 部分残留 > 0 {
        std::process::exit(1);
    }
}
