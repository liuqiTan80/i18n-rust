//! 英文残留兜底与待译清单
//!
//! 背景：消息表（errors.toml 的「消息翻译」节）覆盖哪些句子，静态检查看不全
//! ——只有真跑编译器/构建工具才知道今天会吐哪些新句子。旧行为是「未命中就
//! 静默原样输出英文」：用户看到中英夹杂却无从反馈，维护者也收不到「哪句漏了」
//! 的信号，只能靠实跑撞见。
//!
//! 本模块把这条反馈链固化成机制：
//!
//! 1. **兜底**：译文缺失/残段未译时，渲染照旧回拼英文原文（绝不吞诊断），
//!    但调用方在**本轮结束时**统一提示一次「有 N 条消息暂无译文，已按原文显示」，
//!    不再让用户以为这是产品行为；逐条加前缀会污染输出，故只在汇总行说明。
//! 2. **待译清单**：每条未译记录（语言/通道/错误码/消息原文）去重后落盘
//!    `~/.rz/gaps/<语言>.tsv`，格式与语料门禁的 `diag-corpus.tsv` 同构
//!    （`LEVEL\tCODE\t消息原文`，多一列语言便于合并），维护者拿到清单
//!    即可补表；补完后用 `rzc gaps --clear` 清零。
//!
//! 通道（level）取值与语料门禁一致：`main` 主消息、`help` 建议子诊断，
//! 以及 `tool`（cargo 等非 JSON 通道的纯文本错误行）。
//!
//! 恒等语言例外：en 的「原文」就是母语输出，所以 [`record_gap`] 对
//! [`crate::语言::is_identity_language`] 直接返回，不登记也不落盘。否则英语用户
//! 每次报错都会多一行「有 N 条消息暂无译文」（它在 en 下永远为真），
//! `~/.rz/gaps/en.tsv` 也会积成一份无法补表的清单（实测曾积 1000+ 行）。

use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// 一条未译记录：`(语言, 通道, 错误码, 消息原文)`
pub type TranslationGap = (String, String, String, String);

/// 主消息通道（与语料门禁的 `main` 同义）
pub const LEVEL_MAIN: &str = "main";
/// help 子诊断通道（与语料门禁的 `help` 同义）
pub const LEVEL_HELP: &str = "help";
/// 构建工具纯文本通道（cargo 等非 JSON 输出，见 `LEVEL_*` 文档）
pub const LEVEL_TOOL: &str = "tool";

/// 进程内去重登记表（同一轮重复报错只记一次）
fn registry() -> &'static Mutex<BTreeSet<TranslationGap>> {
    static REGISTRY: OnceLock<Mutex<BTreeSet<TranslationGap>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(BTreeSet::new()))
}

/// 登记一条未译消息（英文原文照旧显示，仅记录待补线索）
///
/// 空消息不记（无补表价值）。`lang` 为空时取当前界面语言。
/// 恒等语言（en）不记：它在消息表未命中时拿到的原句就是目标语输出，
/// 登记它只会把正常结果谎报成「暂无译文」（见模块文档的恒等语言例外）。
pub fn record_gap(lang: &str, level: &str, code: &str, message: &str) {
    let message = message.trim();
    if message.is_empty() {
        return;
    }
    let lang = if lang.is_empty() {
        crate::语言::current_language()
    } else {
        lang.to_string()
    };
    if crate::语言::is_identity_language(&lang) {
        return;
    }
    let gap = (
        lang,
        level.to_string(),
        code.to_string(),
        message.to_string(),
    );
    if let Ok(mut set) = registry().lock() {
        set.insert(gap);
    }
}

/// 取出并清空本轮登记（调用方负责落盘与提示，避免重复处理）
pub fn drain_gaps() -> Vec<TranslationGap> {
    let Ok(mut set) = registry().lock() else {
        return Vec::new();
    };
    std::mem::take(&mut *set).into_iter().collect()
}

/// 当前登记快照（不清空，供测试与只读查看）
pub fn peek_gaps() -> Vec<TranslationGap> {
    registry()
        .lock()
        .map(|set| set.iter().cloned().collect())
        .unwrap_or_default()
}

/// 待译清单目录：`~/.rz/gaps/`
///
/// HOME 不可用时回退临时目录（保证提示不阻断主流程）。
pub fn gaps_dir() -> PathBuf {
    match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        Ok(home) if !home.is_empty() => PathBuf::from(home).join(".rz").join("gaps"),
        _ => std::env::temp_dir().join("rz-gaps"),
    }
}

/// 某语言的待译清单路径
pub fn gap_file(lang: &str) -> PathBuf {
    gaps_dir().join(format!("{lang}.tsv"))
}

/// 一行清单文本（与 [`parse_gap_lines`] 互逆）
fn gap_line(gap: &TranslationGap) -> String {
    let (lang, level, code, message) = gap;
    format!("{lang}\t{level}\t{code}\t{message}")
}

/// 解析清单文本：容忍注释行与缺列（缺列按 main/空码处理）
pub fn parse_gap_lines(raw: &str) -> Vec<TranslationGap> {
    let mut out = Vec::new();
    for line in raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let cols: Vec<&str> = line.splitn(4, '\t').collect();
        let gap = match cols.as_slice() {
            [lang, level, code, message] => (
                lang.to_string(),
                level.to_string(),
                code.to_string(),
                message.to_string(),
            ),
            // 兼容语料门禁格式（LEVEL\tCODE\t消息），语言取默认
            [level, code, message] => (
                crate::语言::current_language(),
                level.to_string(),
                code.to_string(),
                message.to_string(),
            ),
            [message] => (
                crate::语言::current_language(),
                LEVEL_MAIN.to_string(),
                String::new(),
                message.to_string(),
            ),
            _ => continue,
        };
        if !out.contains(&gap) {
            out.push(gap);
        }
    }
    out
}

/// 把本轮登记合并写入清单文件（去重追加），返回新增条数
///
/// 写盘失败不阻断主流程（返回 0，调用方只影响提示措辞）。
pub fn flush_gaps_to_dir(dir: &Path, gaps: &[TranslationGap]) -> usize {
    if gaps.is_empty() {
        return 0;
    }
    let mut added = 0usize;
    for lang in gaps.iter().map(|g| g.0.clone()).collect::<BTreeSet<_>>() {
        let mine: Vec<&TranslationGap> = gaps.iter().filter(|g| g.0 == lang).collect();
        let path = dir.join(format!("{lang}.tsv"));
        let mut existing = std::fs::read_to_string(&path)
            .ok()
            .map(|raw| parse_gap_lines(&raw))
            .unwrap_or_default();
        let mut fresh = Vec::new();
        for gap in mine {
            if !existing.contains(gap) {
                existing.push(gap.clone());
                fresh.push(gap_line(gap));
            }
        }
        if fresh.is_empty() {
            continue;
        }
        added += fresh.len();
        if let Some(parent) = path.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            continue;
        }
        let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        else {
            continue;
        };
        for line in fresh {
            let _ = writeln!(file, "{line}");
        }
    }
    added
}

/// [`flush_gaps_to_dir`] 的默认目录版本
pub fn flush_gaps(gaps: &[TranslationGap]) -> usize {
    flush_gaps_to_dir(&gaps_dir(), gaps)
}

/// 读取某语言已积累的待译清单
pub fn read_gaps(lang: &str) -> Vec<TranslationGap> {
    std::fs::read_to_string(gap_file(lang))
        .map(|raw| parse_gap_lines(&raw))
        .unwrap_or_default()
}

/// 清空某语言的待译清单（补完表后归零），返回是否确实删除了文件
pub fn clear_gaps(lang: &str) -> bool {
    std::fs::remove_file(gap_file(lang)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 登记表是进程全局的：凡 record / peek / drain 的用例统一持本锁串行跑，
    /// 否则并行的 `drain_gaps` 会拿走兄弟用例刚登记的记录，表现为随机红。
    static GAPS_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn serialise_gaps() -> std::sync::MutexGuard<'static, ()> {
        GAPS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// 兜底记录与取出：drain 后登记清空，peek 不改变状态
    #[test]
    fn record_drain_and_peek_round_trip() {
        let _serial = serialise_gaps();
        let gap = (
            "tstlang".to_string(),
            LEVEL_MAIN.to_string(),
            "E9999".to_string(),
            "unique message A".to_string(),
        );
        // 直接播种到登记表（避免并发测试互相干扰全局 current_language）
        registry().lock().unwrap().insert(gap.clone());
        assert!(peek_gaps().contains(&gap));
        let drained = drain_gaps();
        assert!(drained.contains(&gap), "drain 应取出已登记记录");
        assert!(!peek_gaps().contains(&gap), "drain 后应清空");
    }

    /// 空消息不登记（无补表价值）——用独占语言隔离并行测试对全局登记的干扰
    #[test]
    fn empty_message_is_not_recorded() {
        let _serial = serialise_gaps();
        record_gap("tstempty", LEVEL_HELP, "", "   ");
        assert!(
            !peek_gaps().iter().any(|g| g.0 == "tstempty"),
            "空白消息不应进入待译登记"
        );
    }

    /// 恒等语言（en）不登记，其余语言同消息照记
    ///
    /// 起因：en 不携带「消息翻译」节，所以工具通道 `query_by_message` 在 en 下
    /// 永远未命中 → 旧行为是每句都记一条 gap，于是英语用户每次报错都被告知
    /// 「有 N 条消息暂无译文」（而 en 拿到的原句已是最终形态），
    /// `~/.rz/gaps/en.tsv` 积到 1000+ 行且永远补不完。
    /// 对照分支锁住「不登记」只源于语言判定：换非恒等语言后同一消息必须入库。
    #[test]
    fn identity_language_records_no_gap() {
        let _serial = serialise_gaps();
        let 消息 = "crate name `计算器` passed to `--extern` is not a valid ASCII identifier";
        record_gap("en", LEVEL_TOOL, "", 消息);
        assert!(
            !peek_gaps().iter().any(|g| g.0 == "en" && g.3 == 消息),
            "en 是恒等语言，未命中消息表不算待译"
        );
        // 同一条消息交给非恒等语言：必须登记（证明上面的例外不是消息本身被过滤）
        record_gap("de", LEVEL_TOOL, "", 消息);
        assert!(
            peek_gaps().iter().any(|g| g.0 == "de" && g.3 == 消息),
            "de 不是恒等语言，同一句应照常登记待译"
        );
    }

    /// 界面语言为 en 时，空 `lang` 回退到当前语言后仍然不登记
    ///
    /// 判定必须发生在「解析出实际语言」之后，否则 `record_gap("", …)` 在英语
    /// 环境下会绕过恒等语言例外。
    #[test]
    fn identity_language_applies_to_current_language_fallback() {
        let _serial = serialise_gaps();
        let _guard = crate::语言::test_language("en");
        let 消息 = "an english message with no entry in the identity pack";
        record_gap("", LEVEL_MAIN, "", 消息);
        assert!(
            !peek_gaps().iter().any(|g| g.3 == 消息),
            "界面语言为 en 时也不得登记待译"
        );
    }

    /// 清单行格式与解析互逆；注释/空行忽略；缺列容错
    #[test]
    fn gap_line_round_trip_and_tolerance() {
        let gap = (
            "zh".to_string(),
            LEVEL_TOOL.to_string(),
            String::new(),
            "failed to parse manifest at `/x/Cargo.toml`".to_string(),
        );
        let text = format!("# 注释\n\n{}\n", gap_line(&gap));
        let parsed = parse_gap_lines(&text);
        assert_eq!(parsed, vec![gap]);
        // 语料门禁三列格式（LEVEL\tCODE\t消息）也可读，消息含制表符时不被截断
        let three = "help\t\tthe trait `?` is not implemented";
        let parsed = parse_gap_lines(three);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].1, "help");
        assert_eq!(parsed[0].3, "the trait `?` is not implemented");
    }

    /// 合并写入：同语言去重追加，重复写入零新增
    #[test]
    fn flush_dedups_across_runs() {
        let dir = tempfile::tempdir().unwrap();
        let gaps = vec![
            (
                "zhtest".to_string(),
                LEVEL_MAIN.to_string(),
                "E0000".to_string(),
                "first message".to_string(),
            ),
            (
                "zhtest".to_string(),
                LEVEL_HELP.to_string(),
                String::new(),
                "help message".to_string(),
            ),
        ];
        assert_eq!(flush_gaps_to_dir(dir.path(), &gaps), 2);
        assert_eq!(flush_gaps_to_dir(dir.path(), &gaps), 0, "重复登记不应翻倍");
        let read =
            parse_gap_lines(&std::fs::read_to_string(dir.path().join("zhtest.tsv")).unwrap());
        assert_eq!(read, gaps);
    }
}
