//! 门禁：诊断实跑反例语料的译文覆盖回归。
//!
//! 背景：消息表（errors.toml 的「消息翻译」节）覆盖哪些句子，静态检查永远看不全
//! ——只有把反例源码喂给真实 rustc 采集消息原文，才知道编译器今天会吐哪些话。
//! 本测试把这套「实跑采样」固化下来：
//!
//! - 语料 `data/diag-corpus.tsv` 由 `tools/diag-corpus/collect.py` 从
//!   `tools/diag-corpus/fixtures/*.rs` 采集生成（每行 `LEVEL\tCODE\t消息原文`）；
//!   rustc 新增/改动消息句子时，重跑采集器刷新语料即可暴露缺口。
//! - 审计判据 [`audit_message`] 与开发期 `diag_audit` 示例同源，渲染路径与
//!   `DiagnosticTranslator` 一致，这里报出的残留即用户在终端实际看到的文字。
//!
//! 只要有人删键、改坏通配段匹配、或补了等于没补（模板回落英文原文），
//! 本测试立刻在 `cargo test --workspace` 里失败，无需再靠手工实跑发现。
//!
//! 例外：`en` 是直通语言（rustc 原文即英文，无「消息翻译」节属预期），跳过。

use std::collections::HashSet;

use i18n_rust_engine::diagnostic::{ErrorTranslationManager, audit_message};

/// 每行 `LEVEL\tCODE\t消息原文`（CODE 可为空）；裸行按 main/空码处理。
fn parse_corpus(raw: &str) -> Vec<(String, String, String)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for line in raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let mut cols = line.splitn(3, '\t');
        let (level, code, message) = match (cols.next(), cols.next(), cols.next()) {
            (Some(l), Some(c), Some(m)) => (l, c, m),
            (Some(l), Some(m), None) => (l, "", m),
            _ => ("main", "", line),
        };
        // 与 diag_audit 一致：按消息原文去重。
        if !seen.insert(message.to_string()) {
            continue;
        }
        out.push((level.to_string(), code.to_string(), message.to_string()));
    }
    out
}

#[test]
fn 诊断反例语料在全部内置语言包中译文无残留() {
    let raw = include_str!("data/diag-corpus.tsv");
    let corpus = parse_corpus(raw);
    assert!(
        corpus.len() >= 50,
        "语料过少（{} 条），疑似采集或签入异常",
        corpus.len()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut checked_langs = 0usize;
    for code in i18n_rust_engine::语言::内置语言代码() {
        if code == "en" {
            continue; // 直通语言，rustc 原文即英文，不查消息表
        }
        let errors_toml = i18n_rust_engine::语言::内置语言包文件(code)
            .into_iter()
            .find(|(n, _)| *n == "errors.toml")
            .map(|(_, c)| c)
            .unwrap_or_else(|| panic!("{code} 语言包缺少 errors.toml"));
        let manager = ErrorTranslationManager::load_from_string(errors_toml)
            .unwrap_or_else(|e| panic!("{code}/errors.toml 解析失败：{e}"));
        checked_langs += 1;
        for (level, ec, message) in &corpus {
            let audit = audit_message(&manager, level, ec, message);
            if !audit.residue.is_empty() {
                failures.push(format!(
                    "[{code}] {level}/{ec} 形态={} 残留={:?}\n    原文: {message}\n    译文: {}",
                    audit.kind, audit.residue, audit.rendered
                ));
            }
        }
    }

    assert!(
        checked_langs >= 9,
        "应审计至少 9 个非 en 内置语言，实际 {checked_langs}"
    );
    assert!(
        failures.is_empty(),
        "诊断反例语料存在未译全的句子（{} 处，涉及 {} 个语言）：\n{}",
        failures.len(),
        checked_langs,
        failures.join("\n")
    );
}

/// 语料必须同时覆盖 `main`（主消息）与 `help`（修复建议子诊断）两级
///
/// help 行是用户实际看到的中英夹杂重灾区（`did you mean`/`consider …` 全在
/// 子诊断里），但静态检查看不到它。采集器或语料一旦被重写，最容易整段丢掉
/// help 行而主消息检查仍然全绿——这里钉住两级下限，删光 help 就红。
#[test]
fn 语料同时覆盖主消息与help建议两级() {
    let raw = include_str!("data/diag-corpus.tsv");
    let corpus = parse_corpus(raw);
    let count = |level: &str| corpus.iter().filter(|(l, _, _)| l == level).count();
    let (main, help) = (count("main"), count("help"));
    assert!(
        main >= 50,
        "主消息语料不应少于 50 条，实际 {main} 条（疑似采集异常）"
    );
    assert!(help >= 30, "help 子诊断语料不应少于 30 条，实际 {help} 条");
    // 通道集合限定为已定义三种（main/help 来自 rustc 实跑，tool 为 cargo 纯文本），
    // 新通道必须同时在采集器与待译机制两侧登记，不能让语料默默长出无人审计的级别
    let known = ["main", "help", "tool"];
    assert!(
        corpus.iter().all(|(l, _, _)| known.contains(&l.as_str())),
        "语料出现未知通道，采集器与门禁需同步"
    );
}
