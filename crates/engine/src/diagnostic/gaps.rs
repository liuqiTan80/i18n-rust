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
//! 清单每行只有一个字段能装下整条记录，所以消息原文里的换行与制表符
//! 在落盘前会被转成 `\n`/`\r`/`\t` 字面量（见 [`encode_gap_field`]）：rustc 的
//! JSON 消息本身可以带换行（如 E0277 的「下列类型实现了该特征」清单），
//! 早先按原样落盘会把一条记录炸成十几行裸续行——`rzc gaps` 的条数被
//! 虚报、去重失效（每轮实跑都重贴同一坨），语言列也只剩首行带着。
//!
//! 恒等语言例外：en 的「原文」就是母语输出，所以 [`record_gap`] 对
//! [`crate::语言::is_identity_language`] 直接返回，不登记也不落盘。否则英语用户
//! 每次报错都会多一行「有 N 条消息暂无译文」（它在 en 下永远为真），
//! `~/.rz/gaps/en.tsv` 也会积成一份无法补表的清单（实测曾积 1000+ 行）。

use std::collections::BTreeSet;
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
        crate::语言::当前语言()
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
    format!("{lang}\t{level}\t{code}\t{}", encode_gap_field(message))
}

/// 把消息原文编码成单行 TSV 字段：`\` → `\\`，换行/回车/制表符 → `\n`/`\r`/`\t`
///
/// 先替换 `\` 再处理其余字符，故解码是单趟扫描、不会二次转义。反斜杠开头
/// 的其他序列（如 rustc 消息里的 `\x41`、`\n` 字面量）**原样保留**：
/// [`decode_gap_field`] 认不出它们，也就不会误改内容，代价只是这类消息在
/// 清单与记录之间不再逐字互逆（登记侧本就只保证 trim 后一致）。
pub fn encode_gap_field(message: &str) -> String {
    message
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// [`encode_gap_field`] 的逆操作：把 `\n`/`\r`/`\t`/`\\` 还原成真实字符
///
/// 认不出的转义序列（如消息自带的 `\x41`）连同前导反斜杠一起原样输出。
pub fn decode_gap_field(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            // 结尾孤立反斜杠：原样保留，不吞字符
            None => out.push('\\'),
        }
    }
    out
}

/// 把行尾多出来的列拼回消息（历史未转义行里消息本身带制表符）
fn tab_suffix(extras: &[&str]) -> String {
    extras
        .iter()
        .map(|s| format!("\t{s}"))
        .collect::<Vec<_>>()
        .join("")
}

/// 是否为已知通道名（清单行形状的唯一依据）
fn is_known_level(col: &str) -> bool {
    matches!(col, LEVEL_MAIN | LEVEL_HELP | LEVEL_TOOL)
}

/// 解析清单文本：容忍注释行与缺列（缺列按 main/空码处理）
///
/// 行形状按「第二列是不是通道名」判，不看首列长度：历史清单里的语言列
/// 可以是任意用户自定义代码（测试、`--lang` 传入的临时值），而通道只有三个固定取值。
///
/// 几类历史残缺也在这里收拾：
///
/// - **裸续行**：转义机制之前，含换行的消息按原样落盘，一条记录被炸成
///   首行 + 若干续行（可能被截成空错误码的两/三列）。这里把续行拼回上一条
///   记录的消息（还原成真实换行），否则它们会走「单列 = 消息」的兜底分支，
///   被按**当前界面语言**归类，既虚增条数又把 ja/ru 的记录挂到 zh 名下。
/// - **未转义的制表符**：消息里混进第 5 列时按前 3 列 + 剩余合并处理。
pub fn parse_gap_lines(raw: &str) -> Vec<TranslationGap> {
    let mut out: Vec<TranslationGap> = Vec::new();
    for line in raw
        .lines()
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
    {
        let cols: Vec<&str> = line.split('\t').collect();
        // 拼回上一条记录用：统一带换行前缀，尾随空白不带走格式信息
        let fold = |prev: &mut TranslationGap, tail: &str| {
            prev.3.push('\n');
            prev.3.push_str(tail.trim_end());
        };
        let gap = match cols.as_slice() {
            // 「语言\t通道\t错误码\t消息」：消息列本身可能含制表符（转义机制
            // 之前的历史行），前 3 列取定、剩余并回消息
            [lang, level, code, rest @ ..] if is_known_level(level) => (
                lang.to_string(),
                level.to_string(),
                code.to_string(),
                decode_gap_field(&rest.join("\t")),
            ),
            // 语料门禁三列格式（LEVEL\tCODE\t消息），语言取默认
            [level, code, message, extras @ ..] if is_known_level(level) => (
                crate::语言::当前语言(),
                level.to_string(),
                code.to_string(),
                decode_gap_field(&format!("{message}{}", tab_suffix(extras))),
            ),
            // 三列但第二列不是通道名：历史裸续行被未转义制表符/换行截成的
            // 残行（如 `ja\t\t续行`、`ja\tE0277\t`）：能对上上一条记录的语言就拼回去，
            // 对不上则收为新记录（错误码保留，消息不丢）。空语言列不走这里，归入兜底拼回
            [lang, code, message, extras @ ..] if !lang.is_empty() => {
                let tail = match (code.is_empty(), message.is_empty()) {
                    (true, _) => *code,
                    (_, true) => *message,
                    _ => &format!("{code}\t{message}{}", tab_suffix(extras)),
                };
                if out.last().is_some_and(|g| g.0 == *lang) {
                    if let Some(prev) = out.last_mut() {
                        fold(prev, tail);
                    }
                    continue;
                }
                (
                    lang.to_string(),
                    LEVEL_MAIN.to_string(),
                    code.to_string(),
                    decode_gap_field(message),
                )
            }
            // 两列且首列是通道名：语料格式的缺消息列变体
            [level, message] if is_known_level(level) => (
                crate::语言::当前语言(),
                level.to_string(),
                String::new(),
                decode_gap_field(message),
            ),
            // 两列且首列不是通道名：`语言\t消息` 的缺列变体（旧解析同样不保留错误码）
            [lang, message] if !lang.is_empty() => (
                lang.to_string(),
                LEVEL_MAIN.to_string(),
                String::new(),
                decode_gap_field(message),
            ),
            // 单列：优先当作上一条记录的裸续行拼回，否则按旧格式（消息列缺前导列）处理
            [message] => {
                if let Some(prev) = out.last_mut() {
                    fold(prev, message);
                    continue;
                }
                (
                    crate::语言::当前语言(),
                    LEVEL_MAIN.to_string(),
                    String::new(),
                    decode_gap_field(message),
                )
            }
            // 无主记录的残行（首列为空的截断行）：拼回上一条记录
            [..] => {
                if let Some(prev) = out.last_mut() {
                    fold(prev, line);
                }
                continue;
            }
        };
        if !out.contains(&gap) {
            out.push(gap);
        }
    }
    out
}

/// 把本轮登记合并写入清单文件（去重），返回新增条数
///
/// 写盘失败不阻断主流程（返回 0，调用方只影响提示措辞）。
///
/// 采用**整档重写**而非追加：历史文件里未转义的多行记录被读回后，
/// 若继续追加就变成「裸行 + 转义行」混档，每次实跑都重贴一份同一句
/// （实测 `and 56 others` 在 en/ja/ru 各积 64/42/42 次）；重写顺带完成格式迁移。
/// 代价是文件里的人工注释会被掉（清单由 rzc 生成，人工加注请走 `--export`）。
pub fn flush_gaps_to_dir(dir: &Path, gaps: &[TranslationGap]) -> usize {
    if gaps.is_empty() {
        return 0;
    }
    let mut added = 0usize;
    for lang in gaps.iter().map(|g| g.0.clone()).collect::<BTreeSet<_>>() {
        let mine: Vec<&TranslationGap> = gaps.iter().filter(|g| g.0 == lang).collect();
        let path = dir.join(format!("{lang}.tsv"));
        let raw = std::fs::read_to_string(&path).unwrap_or_default();
        let mut existing = parse_gap_lines(&raw);
        let mut fresh = Vec::new();
        for gap in mine {
            if !existing.contains(gap) {
                existing.push(gap.clone());
                fresh.push(gap);
            }
        }
        added += fresh.len();
        if fresh.is_empty() {
            // 本轮无新记录：不动盘。历史裸行会在下一次确实新增待译时随整档重写被迁移，
            // 不该只为修格式就反复改写用户文件
            continue;
        }
        if let Some(parent) = path.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            continue;
        }
        // 按记录列排序输出：同一文件多次重写得到稳定 diff，便于当补表素材
        existing.sort();
        let text = existing.iter().map(gap_line).collect::<Vec<_>>().join("\n");
        // 先写临时文件再改名：中途失败不会把已有清单截成半档
        let tmp = dir.join(format!(".{lang}.tsv.tmp"));
        if std::fs::write(&tmp, format!("{text}\n")).is_err()
            || std::fs::rename(&tmp, &path).is_err()
        {
            let _ = std::fs::remove_file(&tmp);
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
        // 直接播种到登记表（避免并发测试互相干扰全局 当前语言）
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
        let _guard = crate::语言::钉住语言("en");
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

    /// 含换行/制表符的消息落盘后仍是一行，读回逐字相等
    ///
    /// 这类消息不是假例：rustc JSON 的 E0277 「下列类型实现了该特征」清单本身就是
    /// 多行文本。旧行为原样落盘，一条记录炸成十几行，于是 `rzc gaps` 报的条数
    /// 不是待译句子数，且去重永远比不中（同一句每轮重贴一次）。
    #[test]
    fn multiline_message_round_trips_as_one_line() {
        let 消息 = "the following other types implement trait `Add<Rhs>`:\n  `&f16` implements `Add<f16>`\n  `&f64` implements `Add<f64>`\nand 56 others";
        let gap = (
            "ztml".to_string(),
            LEVEL_MAIN.to_string(),
            "E0277".to_string(),
            消息.to_string(),
        );
        let line = gap_line(&gap);
        assert_eq!(line.lines().count(), 1, "多行消息不得拆成多行");
        assert_eq!(line.split('\t').count(), 4, "一行恰好四列");
        assert_eq!(parse_gap_lines(&line), vec![gap.clone()]);

        // 制表符同理：不能多开一列
        let tabbed = (
            "ztml".to_string(),
            LEVEL_TOOL.to_string(),
            String::new(),
            "first\tsecond".to_string(),
        );
        let line = gap_line(&tabbed);
        assert_eq!(line.split('\t').count(), 4, "消息里的制表符不得多开列");
        assert_eq!(parse_gap_lines(&line), vec![tabbed]);

        // 反斜杠自身也要转义，否则消息里的 `\n` 字面量读回会被误当成换行：
        // 编码后可解码，但不再逐字互逆（本模块文档已说明该取舍）
        let 含字面量 = r#"expected `\n` in input"#;
        assert_eq!(
            decode_gap_field(&encode_gap_field(含字面量)),
            含字面量,
            "消息里的 `\n` 字面量应原样往返"
        );
    }

    /// 历史未转义文件：裸续行拼回上一条记录，不被当成新条目、不改语言归属
    ///
    /// 旧解析会把单列续行走「消息列」兜底分支，按当前界面语言归类（ja/ru 的记录
    /// 挂到 zh 名下），并把条数算成十几条。拼回后同一条重复行还能被去重。
    #[test]
    fn legacy_raw_continuations_fold_back() {
        let raw = "ja\thelp\tE0277\tthe following other types implement trait `Add<Rhs>`:\n  `&f16` implements `Add<f16>`\n  `&f64` implements `Add<f64>`\nand 56 others\nja\tmain\t\tstandalone message\n";
        let parsed = parse_gap_lines(raw);
        assert_eq!(parsed.len(), 2, "跨行记录 + 独立记录 = 2 条，不是 5 条");
        assert_eq!(parsed[0].0, "ja", "拼回的续行不得改变语言归属");
        assert_eq!(
            parsed[0].3,
            "the following other types implement trait `Add<Rhs>`:\n  `&f16` implements `Add<f16>`\n  `&f64` implements `Add<f64>`\nand 56 others"
        );
        assert_eq!(parsed[1].3, "standalone message");

        // 拼回后再落盘得到单行转义格式，二次读取逐字相等（迁移不丢内容）
        let line = gap_line(&parsed[0]);
        assert_eq!(line.lines().count(), 1);
        assert_eq!(parse_gap_lines(&line), vec![parsed[0].clone()]);

        // 文件开头就是续行（前无主记录）：仍按旧行为兜底，不丢文本
        let 开头裸行 = "orphan tail\nzh\tmain\t\treal message\n";
        let parsed = parse_gap_lines(开头裸行);
        assert_eq!(parsed.len(), 2, "无主续行不能吞掉后面的正常记录");
        assert_eq!(parsed[0].3, "orphan tail");
        assert_eq!(parsed[1].3, "real message");
    }

    /// 合并写入：同语言去重追加，重复写入零新增
    ///
    /// 整档重写按记录列排序，因此二次运行得到稳定 diff（补表者看得到真正的新行）。
    #[test]
    fn flush_dedups_across_runs() {
        let dir = tempfile::tempdir().unwrap();
        let mut gaps = vec![
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
        gaps.sort();
        assert_eq!(flush_gaps_to_dir(dir.path(), &gaps), 2);
        assert_eq!(flush_gaps_to_dir(dir.path(), &gaps), 0, "重复登记不应翻倍");
        let read =
            parse_gap_lines(&std::fs::read_to_string(dir.path().join("zhtest.tsv")).unwrap());
        assert_eq!(read, gaps);
    }

    /// 历史多行文件经过一次真实写入后完成格式迁移：条数不涨、不重复贴同一句
    ///
    /// 旧实现是「追加写」：未转义的多行记录读不回原记录 → 去重比不中 →
    /// 每次实跑重贴一整块裸行（实测 `and 56 others` 在 en/ja/ru 各积 64/42/42 次）。
    #[test]
    fn flush_migrates_legacy_multiline_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mlang.tsv");
        std::fs::write(
            &path,
            "mlang\thelp\tE0277\tthe following other types implement trait `Add<Rhs>`:\n  `&f16` implements `Add<f16>`\nand 56 others\n",
        )
        .unwrap();

        // 新一轮实跑：同一句多行消息 + 一句新的待译
        let 同一句 = (
            "mlang".to_string(),
            LEVEL_HELP.to_string(),
            "E0277".to_string(),
            "the following other types implement trait `Add<Rhs>`:\n  `&f16` implements `Add<f16>`\nand 56 others".to_string(),
        );
        let 新记录 = (
            "mlang".to_string(),
            LEVEL_MAIN.to_string(),
            String::new(),
            "a brand new english message".to_string(),
        );
        assert_eq!(
            flush_gaps_to_dir(dir.path(), &[同一句.clone(), 新记录]),
            1,
            "已存在的多行记录不得再算新增"
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2, "迁移后一行一条记录");
        assert_eq!(
            text.matches("and 56 others").count(),
            1,
            "同一句不得重复落盘"
        );

        // 再来一轮：仅同一句 → 零新增，也不会反复改写文件
        let before = std::fs::read_to_string(&path).unwrap();
        assert_eq!(flush_gaps_to_dir(dir.path(), &[同一句]), 0);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            before,
            "无新增时不应改动文件"
        );
        assert_eq!(
            parse_gap_lines(&before)
                .iter()
                .filter(|g| g.0 == "mlang")
                .count(),
            2,
            "rzc gaps 的条数应等于待译句子数"
        );
    }
}
