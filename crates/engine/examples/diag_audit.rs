//! 诊断译文残留审计：用引擎自身的匹配器判定每条 rustc 消息原文能否译全。
//!
//! 存在的原因：消息表（errors.toml 的「消息翻译」节）覆盖哪些句子，静态检查
//! 只能看词条形态，看不出「真实编译器会吐哪些句子」。本入口把两者接起来：
//! 输入一批消息原文（由 `tools/diag-corpus/` 采集），输出每条的命中形态与
//! 渲染后仍残留的英文片段，用于批量定位缺口。
//!
//! 渲染路径与 `DiagnosticTranslator` 完全一致（模板 → `{qN}` 填充 → 未消费则
//! 回拼残段），因此这里报出的残留就是用户在终端里实际看到的文字。
//!
//! 用法：
//!   cargo run -p i18n-rust-engine --example diag_audit -- \
//!     --errors crates/engine/lang-packs/zh/errors.toml --messages /tmp/msgs.txt
//!
//! 审计判据已下沉为库函数 [`审计消息`]（门禁测试 `tests/diag_corpus_gate.rs`
//! 与采样反例语料共用同一实现，避免示例与测试漂移）。
use std::collections::HashSet;
use std::io::Read;
use std::path::PathBuf;

use i18n_rust_engine::诊断::{审计消息, 错误翻译管理器};

fn main() {
    let mut 错误表路径: Option<PathBuf> = None;
    let mut 消息清单路径: Option<PathBuf> = None;
    let mut 入参迭代 = std::env::args().skip(1);
    while let Some(标志串) = 入参迭代.next() {
        let mut 取路径 = |目标名: &str| {
            if 标志串 == 目标名 {
                入参迭代.next().map(PathBuf::from)
            } else {
                None
            }
        };
        if let Some(取回值) = 取路径("--errors") {
            错误表路径 = Some(取回值);
        } else if let Some(取回值) = 取路径("--messages") {
            消息清单路径 = Some(取回值);
        }
    }
    let (Some(错误表路径), Some(消息清单路径)) = (错误表路径, 消息清单路径)
    else {
        eprintln!("用法: diag_audit --errors <errors.toml> --messages <每行一条消息原文>");
        std::process::exit(2);
    };

    let 管理器 = 错误翻译管理器::自文件加载(&错误表路径)
        .unwrap_or_else(|错误值| panic!("无法加载 {}: {错误值:?}", 错误表路径.display()));
    let mut 原文本 = String::new();
    std::fs::File::open(&消息清单路径)
        .unwrap_or_else(|错误值| panic!("无法打开 {}: {错误值:?}", 消息清单路径.display()))
        .read_to_string(&mut 原文本)
        .expect("读取消息清单失败");

    let mut 已见集 = HashSet::new();
    let (mut 总数, mut 未中数, mut 残段数, mut 全译数) = (0usize, 0usize, 0usize, 0usize);
    // 每行格式：`LEVEL<TAB>CODE<TAB>消息原文`（采集器标注；CODE 可为空）；
    // 也兼容 `LEVEL<TAB>消息` 与裸消息行。
    for 每行 in 原文本
        .lines()
        .map(str::trim)
        .filter(|行项| !行项.is_empty() && !行项.starts_with('#'))
    {
        let mut 分段迭代 = 每行.splitn(3, '\t');
        let (级别串, 状态码, 消息文本) = match (分段迭代.next(), 分段迭代.next(), 分段迭代.next())
        {
            (Some(甲), Some(乙), Some(丙)) => (甲, 乙, 丙),
            (Some(甲), Some(丙), None) => (甲, "", 丙),
            _ => ("main", "", 每行),
        };
        if !已见集.insert(消息文本.to_string()) {
            continue;
        }
        总数 += 1;
        let 审计项 = 审计消息(&管理器, 级别串, 状态码, 消息文本);
        if 审计项.残留.is_empty() {
            全译数 += 1;
            continue;
        }
        match 审计项.种类 {
            "MISS" => 未中数 += 1,
            _ => 残段数 += 1,
        }
        println!("{级别串}\t{}\t{消息文本}", 审计项.种类);
        println!("  译文: {}", 审计项.渲染文本);
        println!("  残留: {}", 审计项.残留.join(" | "));
    }
    println!("# 合计 {总数} 条：完全未命中 {未中数}、部分残留 {残段数}、已译全 {全译数}");
    if 未中数 + 残段数 > 0 {
        std::process::exit(1);
    }
}
