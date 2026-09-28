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
//! 审计判据已下沉为库函数 [`audit_message`]（门禁测试 `tests/diag_corpus_gate.rs`
//! 与采样反例语料共用同一实现，避免示例与测试漂移）。
use std::collections::HashSet;
use std::io::Read;
use std::path::PathBuf;

use i18n_rust_engine::diagnostic::{ErrorTranslationManager, audit_message};

fn main() {
    let mut errors: Option<PathBuf> = None;
    let mut messages_file: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut take = |name: &str| {
            if flag == name {
                args.next().map(PathBuf::from)
            } else {
                None
            }
        };
        if let Some(p) = take("--errors") {
            errors = Some(p);
        } else if let Some(p) = take("--messages") {
            messages_file = Some(p);
        }
    }
    let (Some(errors), Some(messages_file)) = (errors, messages_file) else {
        eprintln!("用法: diag_audit --errors <errors.toml> --messages <每行一条消息原文>");
        std::process::exit(2);
    };

    let manager = ErrorTranslationManager::load_from_file(&errors)
        .unwrap_or_else(|e| panic!("无法加载 {}: {e:?}", errors.display()));
    let mut raw = String::new();
    std::fs::File::open(&messages_file)
        .unwrap_or_else(|e| panic!("无法打开 {}: {e:?}", messages_file.display()))
        .read_to_string(&mut raw)
        .expect("读取消息清单失败");

    let mut seen = HashSet::new();
    let (mut total, mut miss, mut partial, mut ok) = (0usize, 0usize, 0usize, 0usize);
    // 每行格式：`LEVEL<TAB>CODE<TAB>消息原文`（采集器标注；CODE 可为空）；
    // 也兼容 `LEVEL<TAB>消息` 与裸消息行。
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
        if !seen.insert(message.to_string()) {
            continue;
        }
        total += 1;
        let audit = audit_message(&manager, level, code, message);
        if audit.residue.is_empty() {
            ok += 1;
            continue;
        }
        match audit.kind {
            "MISS" => miss += 1,
            _ => partial += 1,
        }
        println!("{level}\t{}\t{message}", audit.kind);
        println!("  译文: {}", audit.rendered);
        println!("  残留: {}", audit.residue.join(" | "));
    }
    println!("# 合计 {total} 条：完全未命中 {miss}、部分残留 {partial}、已译全 {ok}");
    if miss + partial > 0 {
        std::process::exit(1);
    }
}
