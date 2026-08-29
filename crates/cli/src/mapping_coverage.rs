// 后端源码语料覆盖矩阵（`rzc mapping coverage`）
//
// 将仓库内后端真实 Rust 源码（engine / cli / lsp 的 src/*.rs）作为真实语料，
// 对每个语言包检验「用户写代码时会遇到的名字」的覆盖度，自动列出缺失的母语映射：
// - 关键字（fn/let/...）：缺失时用户无法用母语写出该语法结构（error 级）
// - API 名（std/core/alloc 路径段、use 导入、首字母大写类型名）：缺失时只能
//   写英文原名，全母语体验打折（warning 级）
//
// 用法：在仓库根目录运行 `rzc mapping coverage [--lang <代码>]`（默认全部内置语言）。
// 语料来自仓库内 crates/{engine,cli,lsp}/src/*.rs；发布版（无源码）环境下不可用，
// 此命令面向语言包维护者与 CI 门禁。

use std::collections::HashMap;
use std::path::Path;

/// 一个名字的出现统计（出现次数 + 示例文件）
type NameStats = (usize, String);

/// 语料提取结果：四类名字 → (出现次数, 示例文件)
#[derive(Default)]
struct Corpus {
    /// 关键字（fn/let/...）
    keywords: HashMap<String, NameStats>,
    /// std/core/alloc 路径段（模块名与类型名）
    std_segments: HashMap<String, NameStats>,
    /// use 导入的第三方 crate 名
    external_crates: HashMap<String, NameStats>,
    /// 首字母大写标识符（类型名，路径链之外）
    type_names: HashMap<String, NameStats>,
}

/// Rust 稳定关键字全集（方言母语映射应覆盖；rustc_lexer 不区分关键字，需自行匹配）
const RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "async", "await", "union",
];

impl Corpus {
    fn record(map: &mut HashMap<String, NameStats>, name: &str, file: &str) {
        map.entry(name.to_string())
            .and_modify(|(n, _)| *n += 1)
            .or_insert((1, file.to_string()));
    }
}

/// 收集仓库内后端源码（engine/cli/lsp 的 src/*.rs），返回 (相对路径, 内容)
fn collect_backend_sources(root: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    for dir in ["engine", "cli", "lsp"] {
        let src = root.join("crates").join(dir).join("src");
        let Ok(entries) = std::fs::read_dir(&src) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("rs") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                files.push((
                    format!(
                        "crates/{dir}/src/{}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ),
                    content,
                ));
            }
        }
    }
    files.sort();
    files
}

/// 下一个非空白 token 的种类（rustc_lexer 将 `::` 拆为两个 Colon，中间允许空白）
fn next_non_ws_kind(tokens: &[rustc_lexer::Token], from: usize) -> Option<rustc_lexer::TokenKind> {
    tokens[from..]
        .iter()
        .map(|t| t.kind)
        .find(|k| !matches!(k, rustc_lexer::TokenKind::Whitespace))
}

/// 前一个非空白 token 的种类（from 为当前 token 索引）
fn prev_non_ws_kind(tokens: &[rustc_lexer::Token], from: usize) -> Option<rustc_lexer::TokenKind> {
    tokens[..from]
        .iter()
        .rev()
        .map(|t| t.kind)
        .find(|k| !matches!(k, rustc_lexer::TokenKind::Whitespace))
}

/// 从单个源文件提取语料名字（单遍 rustc_lexer 扫描，无语法树依赖）
///
/// 状态机：
/// - `in_use`：use 语句内（到 `;` 结束）
/// - `path` / `path_mode`：当前 `::` 路径链（含暂存的首段）；
///   标识符先暂存进链，遇到 `::` 才确认是路径（否则按普通标识符处理）
/// - use 内的 `as X` 别名跳过；`crate::`/`super::`/`self::` 内部路径忽略
fn extract_names_from_source(file: &str, source: &str, corpus: &mut Corpus) {
    let tokens: Vec<rustc_lexer::Token> = rustc_lexer::tokenize(source).collect();
    // 逐 token 的字节偏移（rustc_lexer 词法流覆盖全源，偏移连续）
    let mut offsets: Vec<usize> = Vec::with_capacity(tokens.len());
    let mut acc = 0usize;
    for t in &tokens {
        offsets.push(acc);
        acc += t.len;
    }

    let mut in_use = false;
    let mut path: Vec<String> = Vec::new();
    let mut path_mode = false;
    let mut skip_next_use_ident = false;

    for (i, token) in tokens.iter().enumerate() {
        let text = &source[offsets[i]..offsets[i] + token.len];
        match token.kind {
            rustc_lexer::TokenKind::Ident => {
                if RUST_KEYWORDS.contains(&text) {
                    match text {
                        "use" if !in_use => {
                            in_use = true;
                            path.clear();
                            path_mode = false;
                        }
                        "as" if in_use => {
                            skip_next_use_ident = true;
                        }
                        // 内部路径首段（crate::/super::/self::）：入链整体忽略，
                        // 避免 crate 被 flush 后其后的内部模块名（ui/builtin_lang 等）
                        // 误入 external_crates；单独出现的 self 仍按普通关键字处理
                        "crate" | "super" | "self" | "Self"
                            if matches!(
                                next_non_ws_kind(&tokens, i + 1),
                                Some(rustc_lexer::TokenKind::Colon)
                            ) =>
                        {
                            path.push(text.to_string());
                            path_mode = false;
                        }
                        // 普通关键字：结束当前链
                        _ => {
                            if !in_use && !matches!(text, "crate" | "super" | "self" | "Self") {
                                Corpus::record(&mut corpus.keywords, text, file);
                            }
                            flush_path(&mut path, path_mode, in_use, corpus, file);
                            in_use = false;
                            path_mode = false;
                        }
                    }
                } else if skip_next_use_ident {
                    skip_next_use_ident = false;
                    path.clear();
                    path_mode = false;
                } else if in_use || path_mode {
                    path.push(text.to_string());
                } else {
                    // 暂存：等下一个 token 决定是路径链还是普通标识符
                    path.push(text.to_string());
                    path_mode = false;
                }
            }
            rustc_lexer::TokenKind::Colon => {
                // rustc_lexer 将 `::` 拆为两个 Colon（中间允许空白）：
                // 第一半前瞻下一个非空白仍是 Colon；第二半（前一个非空白
                // 是 Colon）保持路径链，仅单冒号（类型标注等）结束当前链
                let prev_colon = matches!(
                    prev_non_ws_kind(&tokens, i),
                    Some(rustc_lexer::TokenKind::Colon)
                );
                let next_colon = matches!(
                    next_non_ws_kind(&tokens, i + 1),
                    Some(rustc_lexer::TokenKind::Colon)
                );
                if prev_colon {
                    // `::` 第二半：路径链继续（path_mode 已由第一半设置）
                } else if next_colon {
                    path_mode = true;
                } else {
                    // 单个冒号（类型标注等）：结束当前链
                    flush_path(&mut path, path_mode, in_use, corpus, file);
                    path_mode = false;
                }
            }
            rustc_lexer::TokenKind::Semi => {
                flush_path(&mut path, path_mode, in_use, corpus, file);
                in_use = false;
                path_mode = false;
            }
            _ => {
                // 其他 token：结束当前链；若链中只有一个暂存标识符，按普通标识符处理
                flush_path(&mut path, path_mode, in_use, corpus, file);
                path_mode = false;
            }
        }
    }
    flush_path(&mut path, path_mode, in_use, corpus, file);
}

/// 结束当前路径链：按 use/非 use 与首段归属分类记录到语料
fn flush_path(
    path: &mut Vec<String>,
    path_mode: bool,
    in_use: bool,
    corpus: &mut Corpus,
    file: &str,
) {
    if path.is_empty() {
        return;
    }
    // 非路径链的单个暂存标识符：首字母大写且非单字符 → 类型名
    if !path_mode && !in_use {
        let name = &path[0];
        let first = name.chars().next().unwrap_or('a');
        if first.is_uppercase() && name.len() > 1 {
            Corpus::record(&mut corpus.type_names, name, file);
        }
        path.clear();
        return;
    }
    let first = &path[0];
    let is_internal = matches!(first.as_str(), "crate" | "super" | "self");
    let is_std = matches!(first.as_str(), "std" | "core" | "alloc");
    if !is_internal {
        if is_std || in_use {
            // std 路径引用 / use 导入：段全部记录（模块名与类型名）
            for seg in path.iter() {
                if is_std {
                    Corpus::record(&mut corpus.std_segments, seg, file);
                }
            }
        }
        // 首段（crate 名）：use 导入或路径引用的第三方 crate；
        // 惯例上 crate 名小写、类型名大写（如 MyServer::new() 的 MyServer）
        if !is_std {
            let first_c = first.chars().next().unwrap_or('a');
            if first_c.is_uppercase() {
                Corpus::record(&mut corpus.type_names, first, file);
            } else {
                Corpus::record(&mut corpus.external_crates, first, file);
            }
        }
        // use 内的非首段：大写段是类型名（如 use serde::Serialize 的 Serialize）
        if in_use && !is_std {
            for seg in path.iter().skip(1) {
                let c = seg.chars().next().unwrap_or('a');
                if c.is_uppercase() && seg.len() > 1 {
                    Corpus::record(&mut corpus.type_names, seg, file);
                }
            }
        }
        // 非 use 的第三方路径引用（如 ureq::get）：后续段按类型名/忽略
        if !in_use && !is_std {
            for seg in path.iter().skip(1) {
                let c = seg.chars().next().unwrap_or('a');
                if c.is_uppercase() && seg.len() > 1 {
                    Corpus::record(&mut corpus.type_names, seg, file);
                }
            }
        }
    }
    path.clear();
}

/// 构建语言包反向索引：英文原名 → 母语键
///
/// 合并 keywords.toml（排除派生特征节，与运行时语义一致）、module_paths.toml、
/// stdlib.toml、crates/*.toml 的全部映射值；同值多键时保留首个（报告无需区分）。
fn build_reverse_index(
    keywords_toml: &str,
    module_paths_toml: &str,
    stdlib_toml: &str,
    crates_data: &[(&str, &str)],
) -> HashMap<String, String> {
    let mut reverse: HashMap<String, String> = HashMap::new();
    let insert_rev = |reverse: &mut HashMap<String, String>, value: &str, key: &str| {
        if !value.is_empty() {
            reverse.entry(value.to_string()).or_insert_with(|| key.to_string());
        }
    };
    if let Ok(sections) = i18n_rust_engine::mapping_source::parse_toml_sections(keywords_toml) {
        for (section, map) in &sections {
            if section == "派生特征" {
                continue;
            }
            for (k, v) in map {
                insert_rev(&mut reverse, v, k);
            }
        }
    }
    for toml_content in [module_paths_toml, stdlib_toml] {
        if let Ok(sections) = i18n_rust_engine::mapping_source::parse_toml_sections(toml_content) {
            for map in sections.values() {
                for (k, v) in map {
                    insert_rev(&mut reverse, v, k);
                }
            }
        }
    }
    for (_, content) in crates_data {
        if let Ok(sections) = i18n_rust_engine::mapping_source::parse_toml_sections(content) {
            for map in sections.values() {
                for (k, v) in map {
                    insert_rev(&mut reverse, v, k);
                }
            }
        }
    }
    reverse
}

/// 单个语言包的覆盖报告
pub struct CoverageReport {
    /// 语言代码
    pub lang: String,
    /// 缺失关键字（error 级）：(名字, 出现次数, 示例文件)
    pub missing_keywords: Vec<(String, usize, String)>,
    /// 缺失 std/core/alloc 段（warning 级，用户写代码最常遇到）
    pub missing_std: Vec<(String, usize, String)>,
    /// 缺失第三方 crate 名（warning 级，用外部库时需要）
    pub missing_crates: Vec<(String, usize, String)>,
    /// 缺失类型名（warning 级，多为编译器内部实现，可按需忽略）
    pub missing_types: Vec<(String, usize, String)>,
    /// 覆盖统计
    pub total_names: usize,
    pub covered_names: usize,
}

/// 按出现次数降序排序缺失清单（同次数按名字升序，稳定输出）
fn sort_missing(list: &mut [(String, usize, String)]) {
    list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
}

/// 用反向索引检测语料覆盖，产出报告
fn coverage_for(corpus: &Corpus, reverse: &HashMap<String, String>, lang: &str) -> CoverageReport {
    let mut report = CoverageReport {
        lang: lang.to_string(),
        missing_keywords: Vec::new(),
        missing_std: Vec::new(),
        missing_crates: Vec::new(),
        missing_types: Vec::new(),
        total_names: 0,
        covered_names: 0,
    };
    // 关键字缺失（error 级）
    for (name, (count, file)) in &corpus.keywords {
        report.total_names += 1;
        if !reverse.contains_key(name) {
            report
                .missing_keywords
                .push((name.clone(), *count, file.clone()));
        } else {
            report.covered_names += 1;
        }
    }
    // API 缺失（warning 级）：std 段 / 外部 crate / 类型名分三组
    for (name, (count, file)) in &corpus.std_segments {
        report.total_names += 1;
        if !reverse.contains_key(name) {
            report
                .missing_std
                .push((name.clone(), *count, file.clone()));
        } else {
            report.covered_names += 1;
        }
    }
    for (name, (count, file)) in &corpus.external_crates {
        report.total_names += 1;
        if !reverse.contains_key(name) {
            report
                .missing_crates
                .push((name.clone(), *count, file.clone()));
        } else {
            report.covered_names += 1;
        }
    }
    for (name, (count, file)) in &corpus.type_names {
        report.total_names += 1;
        if !reverse.contains_key(name) {
            report
                .missing_types
                .push((name.clone(), *count, file.clone()));
        } else {
            report.covered_names += 1;
        }
    }
    sort_missing(&mut report.missing_keywords);
    sort_missing(&mut report.missing_std);
    sort_missing(&mut report.missing_crates);
    sort_missing(&mut report.missing_types);
    report
}

/// 打印单个语言包的覆盖报告（本地化输出）
fn print_report(report: &CoverageReport, source_count: usize) {
    let ui = crate::ui::Ui::global();
    println!("{}", ui.f("mc_cov_header", &[&report.lang]));
    let pct = report
        .covered_names
        .checked_mul(100)
        .and_then(|n| n.checked_div(report.total_names))
        .unwrap_or(0);
    println!(
        "{}",
        ui.f(
            "mc_cov_stats",
            &[
                &report.covered_names.to_string(),
                &report.total_names.to_string(),
                &pct.to_string(),
                &source_count.to_string()
            ]
        )
    );
    // 关键字缺失（error 级，最多展示 30 条）
    let kw_total = report.missing_keywords.len();
    for (name, count, file) in report.missing_keywords.iter().take(30) {
        println!(
            "{}",
            ui.f("mc_cov_kw_missing", &[name, &count.to_string(), file])
        );
    }
    if kw_total > 30 {
        println!(
            "{}",
            ui.f("mc_cov_more", &[&(kw_total - 30).to_string()])
        );
    }
    // std 段缺失（warning 级，最多展示 25 条）：用户写代码最常遇到，优先补齐
    let std_total = report.missing_std.len();
    for (name, count, file) in report.missing_std.iter().take(25) {
        println!(
            "{}",
            ui.f("mc_cov_std_missing", &[name, &count.to_string(), file])
        );
    }
    if std_total > 25 {
        println!(
            "{}",
            ui.f("mc_cov_more", &[&(std_total - 25).to_string()])
        );
    }
    // 第三方 crate 缺失（warning 级，最多展示 15 条）
    let crate_total = report.missing_crates.len();
    for (name, count, file) in report.missing_crates.iter().take(15) {
        println!(
            "{}",
            ui.f("mc_cov_crate_missing", &[name, &count.to_string(), file])
        );
    }
    if crate_total > 15 {
        println!(
            "{}",
            ui.f("mc_cov_more", &[&(crate_total - 15).to_string()])
        );
    }
    // 类型名缺失（warning 级，最多展示 15 条；多为编译器内部实现，可按需忽略）
    let type_total = report.missing_types.len();
    for (name, count, file) in report.missing_types.iter().take(15) {
        println!(
            "{}",
            ui.f("mc_cov_type_missing", &[name, &count.to_string(), file])
        );
    }
    if type_total > 15 {
        println!(
            "{}",
            ui.f("mc_cov_more", &[&(type_total - 15).to_string()])
        );
    }
    if kw_total == 0 {
        println!("{}", ui.t("mc_cov_kw_ok"));
    }
    println!();
}

/// `rzc mapping coverage` 入口
///
/// - lang 为 None：检测全部内置语言
/// - lang 为内置语言代码：仅检测该语言
///
/// 返回是否全部语言关键字覆盖（error 级）通过。
pub fn run_coverage(lang: Option<&str>) -> anyhow::Result<bool> {
    let ui = crate::ui::Ui::global();
    // 语料必须来自源码仓库（发布版无 crates/{engine,cli,lsp}/src 结构）
    let cwd = std::env::current_dir().map_err(|e| anyhow::anyhow!("获取当前目录失败: {e}"))?;
    let root = crate::find_project_root_upward(&cwd)
        .ok_or_else(|| anyhow::anyhow!("{}", ui.t("mc_cov_no_repo")))?;
    let sources = collect_backend_sources(&root);
    if sources.is_empty() {
        anyhow::bail!("{}", ui.t("mc_cov_no_repo"));
    }
    // 提取语料（一次，供所有语言包复用）
    let mut corpus = Corpus::default();
    for (file, content) in &sources {
        extract_names_from_source(file, content, &mut corpus);
    }
    println!("{}", ui.t("mc_cov_corpus_intro"));
    let langs: Vec<&str> = match lang {
        Some(l) => vec![l],
        None => crate::builtin_lang::builtin_lang_codes(),
    };
    let mut all_ok = true;
    for lang_code in langs {
        if !crate::builtin_lang::has_builtin_lang(lang_code) {
            anyhow::bail!(ui.f("mc_unknown_target", &[lang_code]));
        }
        let data = crate::builtin_lang::get_builtin_data(lang_code);
        let reverse = build_reverse_index(
            data.keywords_toml,
            data.module_paths_toml,
            data.stdlib_toml,
            data.crates_data,
        );
        let report = coverage_for(&corpus, &reverse, lang_code);
        all_ok &= report.missing_keywords.is_empty();
        print_report(&report, sources.len());
    }
    if all_ok {
        println!("{}", ui.t("mc_cov_all_ok"));
    }
    Ok(all_ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单文件语料提取：关键字 / std 路径段 / 第三方 crate / 类型名
    #[test]
    fn test_extract_names_basic() {
        let src = r#"
use std::collections::HashMap;
use serde::Serialize;
use crate::toolchain::find_toolchain;

pub fn main() -> Result<(), Box<dyn Error>> {
    let map: HashMap<String, Vec<u8>> = HashMap::new();
    std::fs::read_to_string("x")?;
    ureq::get("https://x").call()?;
    let server = MyServer::new();
    Ok(())
}
"#;
        let mut corpus = Corpus::default();
        extract_names_from_source("t.rs", src, &mut corpus);
        // 关键字
        for kw in ["fn", "let", "pub", "dyn"] {
            assert!(corpus.keywords.contains_key(kw), "缺少关键字 {kw}");
        }
        assert!(!corpus.keywords.contains_key("use"), "use 不应作为关键字语料");
        // std 路径段
        for seg in ["std", "collections", "HashMap", "fs", "read_to_string"] {
            assert!(corpus.std_segments.contains_key(seg), "缺少 std 段 {seg}");
        }
        // 第三方 crate 名
        for c in ["serde", "ureq"] {
            assert!(corpus.external_crates.contains_key(c), "缺少 crate {c}");
        }
        // 内部路径忽略：i18n_rust_engine 是 crate 名（外部），crate:: 内部忽略
        assert!(!corpus.external_crates.contains_key("crate"), "crate:: 不应计入");
        // 类型名：use 段中的大写 + 普通大写标识符
        assert!(corpus.type_names.contains_key("Serialize"));
        assert!(corpus.type_names.contains_key("MyServer"));
        assert!(corpus.type_names.contains_key("Error"), "Box<dyn Error> 的 Error");
        assert!(
            !corpus.external_crates.contains_key("HashMap"),
            "裸 HashMap::new() 是类型引用而非 crate"
        );
        assert!(
            corpus.std_segments.contains_key("HashMap"),
            "use 路径链中的 HashMap 计入 std 段"
        );
        // 非 use 的内部路径调用（crate::ui::Ui）整体忽略，内部模块名不入 external_crates
        let src2 = "fn f() { crate::ui::Ui::global(); super::helper::run(); self::x::y(); }\n";
        let mut corpus2 = Corpus::default();
        extract_names_from_source("t2.rs", src2, &mut corpus2);
        for n in ["ui", "Ui", "helper", "run", "x", "y"] {
            assert!(
                !corpus2.external_crates.contains_key(n) && !corpus2.type_names.contains_key(n),
                "内部路径段 {n} 不应计入"
            );
        }
    }

    /// use as 别名与泛型单字符不进入语料
    #[test]
    fn test_extract_names_alias_and_generics() {
        let src =
            "use std::fmt::Result as FmtResult;\nfn foo<T>(x: T) -> Result<T> { Ok(x) }\n";
        let mut corpus = Corpus::default();
        extract_names_from_source("t.rs", src, &mut corpus);
        assert!(!corpus.type_names.contains_key("FmtResult"), "as 别名应跳过");
        assert!(!corpus.type_names.contains_key("T"), "单字符泛型参数不应计入");
        assert!(corpus.std_segments.contains_key("fmt"));
        assert!(corpus.keywords.contains_key("fn"));
        assert!(
            corpus.type_names.contains_key("Result"),
            "返回类型中的真实类型引用应记录"
        );
    }

    /// 反向索引：合并 keywords/stdlib/module_paths/crates 的值
    #[test]
    fn test_build_reverse_index() {
        let keywords = "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n[\"派生特征\"]\n\"调试\" = \"Debug\"\n";
        let module_paths = "[\"模块路径\"]\n\"标准库\" = \"std\"\n";
        let stdlib = "[\"标识符\"]\n\"字符串\" = \"String\"\n";
        let crates = [("serde.toml", "[\"模块路径\"]\n\"序列化\" = \"serde\"\n[\"标识符\"]\n\"序列化特征\" = \"Serialize\"\n")];
        let rev = build_reverse_index(keywords, module_paths, stdlib, &crates);
        assert_eq!(rev.get("fn").map(String::as_str), Some("函数"));
        assert_eq!(rev.get("let").map(String::as_str), Some("让"));
        assert_eq!(rev.get("std").map(String::as_str), Some("标准库"));
        assert_eq!(rev.get("String").map(String::as_str), Some("字符串"));
        assert_eq!(rev.get("serde").map(String::as_str), Some("序列化"));
        assert_eq!(rev.get("Serialize").map(String::as_str), Some("序列化特征"));
        assert!(!rev.contains_key("Debug"), "派生特征节不应并入关键字索引");
    }

    /// 覆盖检测：关键字缺失入 error 列表，API 缺失入 warning 列表
    #[test]
    fn test_coverage_for() {
        let keywords = "[\"声明\"]\n\"函数\" = \"fn\"\n";
        let module_paths = "[\"模块路径\"]\n\"标准库\" = \"std\"\n";
        let rev = build_reverse_index(keywords, module_paths, "", &[]);
        let mut corpus = Corpus::default();
        corpus.keywords.insert("fn".into(), (3, "a.rs".into()));
        corpus.keywords.insert("unsafe".into(), (1, "b.rs".into()));
        corpus.std_segments.insert("std".into(), (9, "a.rs".into()));
        corpus.type_names.insert("NotCovered".into(), (2, "b.rs".into()));
        let report = coverage_for(&corpus, &rev, "zh");
        assert!(report.missing_keywords.iter().any(|(n, ..)| n == "unsafe"));
        assert!(!report.missing_keywords.iter().any(|(n, ..)| n == "fn"));
        assert!(report.missing_types.iter().any(|(n, ..)| n == "NotCovered"));
        assert!(!report.missing_std.iter().any(|(n, ..)| n == "std"));
        assert_eq!(report.total_names, 4);
        assert_eq!(report.covered_names, 2);
    }

    /// 关键字门禁：全部内置语言包必须覆盖 Rust 稳定关键字全集（error 级）——
    /// 任一门语言缺任一关键字（用户无法用母语写出该语法结构）都会在此失败
    #[test]
    fn test_all_builtin_langs_keywords_complete() {
        let mut failed: Vec<String> = Vec::new();
        for lang in crate::builtin_lang::builtin_lang_codes() {
            let data = crate::builtin_lang::get_builtin_data(lang);
            let rev = build_reverse_index(data.keywords_toml, "", "", &[]);
            let missing: Vec<&str> = RUST_KEYWORDS
                .iter()
                .copied()
                .filter(|k| !rev.contains_key(*k))
                .collect();
            if !missing.is_empty() {
                failed.push(format!("{lang} 缺 {missing:?}"));
            }
        }
        assert!(failed.is_empty(), "关键字门禁失败: {failed:?}");
    }

    /// 全内置语言跑一遍覆盖检测（不 panic、有统计）
    #[test]
    fn test_run_coverage_all_langs_no_panic() {
        for lang in crate::builtin_lang::builtin_lang_codes() {
            let data = crate::builtin_lang::get_builtin_data(lang);
            let rev = build_reverse_index(
                data.keywords_toml,
                data.module_paths_toml,
                data.stdlib_toml,
                data.crates_data,
            );
            // 用最小语料冒烟：语料为空时统计为 0 即可
            let corpus = Corpus::default();
            let report = coverage_for(&corpus, &rev, lang);
            assert_eq!(report.total_names, 0, "{lang} 空语料统计应为 0");
            assert!(report.missing_keywords.is_empty());
        }
    }
}
