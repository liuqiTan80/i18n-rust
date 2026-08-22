// 第三方库映射校验与脚手架模块
//
// 提供 `rzc mapping check` 与 `rzc mapping scaffold` 的核心逻辑：
// - check：校验单个语言包的 crates/*.toml 映射质量
//   （TOML 可解析 / 无重复键 / 关键字避让 / 跨文件同键冲突 / 条目数统计），
//   以及跨内置语言的条目数一致性对比。
// - scaffold：从源语言 crates 生成目标语言的翻译骨架（保留英文值，母语键留待翻译）。
//
// 校验规则来自翻译实践（见记忆 ff7678c2）：
// 1. 关键字避让：crates 键与 keywords.toml 键相撞时，关键字先替换，crates 键永不生效 → error
// 2. crates 文件之间同键不同值：read_dir 顺序未定义，合并非确定 → error
// 3. crates 键与 stdlib 标识符同键不同值：stdlib 最后加载优先，crates 键被覆盖失效 → warning

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// “母语词 → 英文”映射表
type NameMap = HashMap<String, String>;

/// 语言包映射数据的统一视图（内置数据与外部目录两种来源归一化）
pub struct LangPackView {
    /// 语言代码（如 zh / ru）
    pub lang: String,
    /// keywords.toml 内容
    pub keywords_toml: String,
    /// stdlib.toml 内容
    pub stdlib_toml: String,
    /// 第三方库映射（文件名, TOML 内容），按文件名排序保证确定性
    pub crates: Vec<(String, String)>,
}

impl LangPackView {
    /// 从内置语言包数据构造视图
    pub fn from_builtin(lang: &str) -> Self {
        let data = crate::builtin_lang::get_builtin_data(lang);
        let mut crates: Vec<(String, String)> = data
            .crates_data
            .iter()
            .map(|(name, content)| (name.to_string(), content.to_string()))
            .collect();
        crates.sort_by(|a, b| a.0.cmp(&b.0));
        LangPackView {
            lang: lang.to_string(),
            keywords_toml: data.keywords_toml.to_string(),
            stdlib_toml: data.stdlib_toml.to_string(),
            crates,
        }
    }

    /// 从外部语言包目录构造视图（读取 keywords.toml / stdlib.toml / crates/*.toml）
    pub fn from_dir(dir: &Path) -> anyhow::Result<Self> {
        let lang = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let keywords_toml = std::fs::read_to_string(dir.join("keywords.toml"))
            .map_err(|e| anyhow::anyhow!("读取 keywords.toml 失败: {e}"))?;
        // stdlib.toml 可选（部分语言包可能未提供）
        let stdlib_toml = std::fs::read_to_string(dir.join("stdlib.toml")).unwrap_or_default();
        let mut crates = Vec::new();
        let crates_dir = dir.join("crates");
        if crates_dir.is_dir() {
            let mut files: Vec<PathBuf> = std::fs::read_dir(&crates_dir)
                .map_err(|e| anyhow::anyhow!("读取 crates 目录失败: {e}"))?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("toml"))
                .collect();
            files.sort();
            for path in files {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown.toml")
                    .to_string();
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| anyhow::anyhow!("读取 {} 失败: {e}", path.display()))?;
                crates.push((name, content));
            }
        }
        Ok(LangPackView {
            lang,
            keywords_toml,
            stdlib_toml,
            crates,
        })
    }
}

/// 校验统计信息
#[derive(Debug, Default, Clone)]
pub struct CheckStats {
    /// crates 文件数
    pub crate_files: usize,
    /// 模块路径节条目总数
    pub module_path_entries: usize,
    /// 标识符节条目总数
    pub ident_entries: usize,
}

/// 校验报告
#[derive(Debug, Default)]
pub struct CheckReport {
    /// 必须修复的错误
    pub errors: Vec<String>,
    /// 建议修复的警告
    pub warnings: Vec<String>,
    /// 统计信息
    pub stats: CheckStats,
}

impl CheckReport {
    /// 是否通过（无错误；警告不阻断）
    pub fn passed(&self) -> bool {
        self.errors.is_empty()
    }
}

/// 提取 TOML 中 `["模块路径"]` 与 `["标识符"]` 两节的键值对
///
/// 返回 (模块路径表, 标识符表)。TOML 解析失败（含重复键）时返回 Err。
fn extract_sections(content: &str) -> Result<(NameMap, NameMap), String> {
    let value: toml::Value = toml::from_str(content).map_err(|e| e.to_string())?;
    let mut module_paths = HashMap::new();
    let mut idents = HashMap::new();
    if let toml::Value::Table(table) = value {
        if let Some(toml::Value::Table(mp)) = table.get("模块路径") {
            for (k, v) in mp {
                if let toml::Value::String(s) = v {
                    module_paths.insert(k.clone(), s.clone());
                }
            }
        }
        if let Some(toml::Value::Table(id)) = table.get("标识符") {
            for (k, v) in id {
                if let toml::Value::String(s) = v {
                    idents.insert(k.clone(), s.clone());
                }
            }
        }
    }
    Ok((module_paths, idents))
}

/// 提取 keywords.toml 中所有节的“母语词 → 英文”映射
///
/// 复用引擎权威语义（flatten_sections：按节名升序合并，后到覆盖），
/// 与运行时实际生效的关键字映射保持一致，避免校验误报。
///
/// 用于关键字避让检测：需比较 crates 键与关键字的**值**是否一致，
/// 同值视为安全冗余（关键字替换与 crates 替换结果相同），不同值才是真冲突。
fn extract_keyword_map(content: &str) -> HashMap<String, String> {
    match i18n_rust_engine::mapping_source::parse_toml_sections(content) {
        Ok(sections) => i18n_rust_engine::mapping_source::flatten_sections(&sections),
        Err(_) => HashMap::new(),
    }
}

/// 校验单个语言包的 crates 映射质量
///
/// 检查项（按严重级别）：
/// - error：TOML 解析失败（含重复键）
/// - error：crates 键与 keywords.toml 键相撞（关键字先替换，crates 键永不生效）
/// - error：crates 文件之间标识符同键不同值（合并非确定）
/// - warning：crates 标识符键与 stdlib 标识符同键不同值（stdlib 优先，crates 键失效）
pub fn check_lang_pack(view: &LangPackView) -> CheckReport {
    let mut report = CheckReport::default();
    let keyword_map = extract_keyword_map(&view.keywords_toml);

    // stdlib 标识符表（用于 warning 级别的覆盖检测）
    let stdlib_idents = extract_sections(&view.stdlib_toml)
        .map(|(_, idents)| idents)
        .unwrap_or_default();

    // 跨文件标识符键追踪：键 -> (值, 首次出现的文件)
    let mut seen_idents: HashMap<String, (String, String)> = HashMap::new();

    for (file_name, content) in &view.crates {
        report.stats.crate_files += 1;
        // 1. TOML 解析（重复键 / 格式错误在此暴露）
        let (module_paths, idents) = match extract_sections(content) {
            Ok(sections) => sections,
            Err(e) => {
                report
                    .errors
                    .push(format!("mc_parse_failed|{file_name}|{e}"));
                continue;
            }
        };
        report.stats.module_path_entries += module_paths.len();
        report.stats.ident_entries += idents.len();

        // 2. 关键字避让 + 3. 跨文件冲突（对模块路径节与标识符节的键都检查）
        for section_name in ["模块路径", "标识符"] {
            let entries = if section_name == "模块路径" {
                &module_paths
            } else {
                &idents
            };
            for (key, value) in entries {
                // 关键字避让：仅当 crates 键与关键字**值不同**时报错；
                // 同值时关键字替换与 crates 替换结果一致，属安全冗余不报错
                if keyword_map
                    .get(key)
                    .is_some_and(|keyword_value| keyword_value != value)
                {
                    report.errors.push(format!(
                        "mc_keyword_collision|{file_name}|{section_name}|{key}|{value}|{}",
                        keyword_map[key]
                    ));
                }
                // 标识符节的跨文件冲突与 stdlib 覆盖检测
                if section_name == "标识符" {
                    if let Some((prev_value, prev_file)) = seen_idents.get(key) {
                        if prev_value != value {
                            report.errors.push(format!(
                                "mc_cross_conflict|{key}|{prev_file}|{prev_value}|{file_name}|{value}"
                            ));
                        }
                    } else {
                        seen_idents.insert(key.clone(), (value.clone(), file_name.clone()));
                    }
                    // stdlib 优先覆盖检测
                    if stdlib_idents
                        .get(key)
                        .is_some_and(|stdlib_value| stdlib_value != value)
                    {
                        report.warnings.push(format!(
                            "mc_stdlib_shadow|{file_name}|{key}|{value}|{}",
                            stdlib_idents[key]
                        ));
                    }
                }
            }
        }
    }
    report
}

/// 从源语言 crates 生成目标语言的翻译骨架
///
/// 保留 TOML 结构与英文值，在每个含母语键的行后追加 TODO 注释提示翻译。
/// 返回生成的文件数。目标目录不存在时自动创建。
pub fn scaffold(
    source: &LangPackView,
    target_lang: &str,
    output_dir: &Path,
) -> anyhow::Result<(usize, usize)> {
    std::fs::create_dir_all(output_dir)
        .map_err(|e| anyhow::anyhow!("创建目录 {} 失败: {e}", output_dir.display()))?;
    let mut created = 0;
    let mut skipped = 0;
    for (file_name, content) in &source.crates {
        let target_path = output_dir.join(file_name);
        // 已存在的文件跳过：保留既有翻译成果，保证重跑幂等
        if target_path.exists() {
            skipped += 1;
            continue;
        }
        let scaffolded = add_scaffold_todo(content, &source.lang, target_lang);
        std::fs::write(&target_path, scaffolded)
            .map_err(|e| anyhow::anyhow!("写入 {} 失败: {e}", target_path.display()))?;
        created += 1;
    }
    Ok((created, skipped))
}

/// 为 TOML 内容中每个 `母语键 = 英文值` 行追加 TODO 翻译注释
///
/// 仅处理 `["模块路径"]` 与 `["标识符"]` 节内的键值行；节标题、注释、空行原样保留。
fn add_scaffold_todo(content: &str, source_lang: &str, target_lang: &str) -> String {
    let mut out = String::new();
    let mut in_target_section = false;
    for line in content.lines() {
        let trimmed = line.trim();
        // 节标题检测：进入/离开目标节
        if trimmed.starts_with('[') {
            in_target_section = trimmed == "[\"模块路径\"]" || trimmed == "[\"标识符\"]";
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // 目标节内的键值行（含 `=` 且非注释）追加 TODO
        if in_target_section && trimmed.contains('=') && !trimmed.starts_with('#') {
            // 已有行尾注释时不重复追加
            if !trimmed.contains("TODO") {
                out.push_str(line);
                out.push_str(&format!(
                    "  # TODO({target_lang}): 将键从 {source_lang} 翻译"
                ));
                out.push('\n');
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// 打印单个语言包的校验报告（本地化输出）
///
/// 报告条目格式为 `code|arg1|arg2...`，按 code 查 ui.toml 模板渲染。
fn print_report(view_lang: &str, report: &CheckReport) {
    let ui = crate::ui::Ui::global();
    println!("{}", ui.f("mc_header", &[view_lang]));
    println!(
        "{}",
        ui.f(
            "mc_stats",
            &[
                &report.stats.crate_files.to_string(),
                &report.stats.module_path_entries.to_string(),
                &report.stats.ident_entries.to_string()
            ]
        )
    );
    for error in &report.errors {
        println!("{}", render_issue(error));
    }
    for warning in &report.warnings {
        println!("{}", render_issue(warning));
    }
    if report.passed() {
        println!(
            "{}",
            if report.warnings.is_empty() {
                ui.t("mc_ok")
            } else {
                ui.f("mc_ok_with_warnings", &[&report.warnings.len().to_string()])
            }
        );
    } else {
        println!(
            "{}",
            ui.f(
                "mc_failed",
                &[
                    &report.errors.len().to_string(),
                    &report.warnings.len().to_string()
                ]
            )
        );
    }
}

/// 将 `code|arg1|arg2...` 格式的问题条目渲染为本地化消息
fn render_issue(issue: &str) -> String {
    let ui = crate::ui::Ui::global();
    let mut parts = issue.split('|');
    let code = parts.next().unwrap_or(issue);
    let args: Vec<&str> = parts.collect();
    ui.f(code, &args)
}

/// `rzc mapping check` 入口
///
/// - target 为 None：校验全部内置语言并输出跨语言条目数一致性对比
/// - target 为已存在的目录路径：按外部语言包目录校验
/// - 否则按内置语言代码校验
pub fn run_check(target: Option<&str>) -> anyhow::Result<bool> {
    let ui = crate::ui::Ui::global();
    let Some(target) = target else {
        // 全部内置语言 + 跨语言一致性对比
        let mut all_passed = true;
        for lang in crate::builtin_lang::builtin_lang_codes() {
            let view = LangPackView::from_builtin(lang);
            let report = check_lang_pack(&view);
            all_passed &= report.passed();
            print_report(lang, &report);
        }
        print_cross_lang_counts();
        return Ok(all_passed);
    };
    let path = Path::new(target);
    if path.is_dir() {
        let view = LangPackView::from_dir(path)?;
        let report = check_lang_pack(&view);
        let passed = report.passed();
        print_report(&view.lang, &report);
        Ok(passed)
    } else if crate::builtin_lang::has_builtin_lang(target) {
        let view = LangPackView::from_builtin(target);
        let report = check_lang_pack(&view);
        let passed = report.passed();
        print_report(target, &report);
        Ok(passed)
    } else {
        anyhow::bail!(ui.f("mc_unknown_target", &[target]));
    }
}

/// 打印跨内置语言的条目数一致性对比（不一致时输出警告，不阻断）
///
/// 无 crates 文件的语言（如 en：母语即英文，无需第三方映射）
/// 仅展示不参与一致性比较。
fn print_cross_lang_counts() {
    let ui = crate::ui::Ui::global();
    let langs = crate::builtin_lang::builtin_lang_codes();
    let mut counts: Vec<(String, usize, usize)> = Vec::new();
    for lang in langs {
        let view = LangPackView::from_builtin(lang);
        let report = check_lang_pack(&view);
        counts.push((
            lang.to_string(),
            report.stats.crate_files,
            report.stats.ident_entries,
        ));
    }
    counts.sort_by(|a, b| a.0.cmp(&b.0));
    println!("{}", ui.t("mc_cross_lang_header"));
    for (lang, _, n) in &counts {
        println!("  {}: {}", lang, n);
    }
    // 仅对有 crates 文件的语言比较条目数
    let with_crates: Vec<usize> = counts
        .iter()
        .filter(|(_, files, _)| *files > 0)
        .map(|(_, _, n)| *n)
        .collect();
    let first = with_crates.first().copied();
    let inconsistent = with_crates.iter().any(|n| Some(*n) != first);
    if inconsistent {
        println!("{}", ui.t("mc_cross_lang_inconsistent"));
    } else {
        println!("{}", ui.t("mc_cross_lang_consistent"));
    }
}

/// `rzc mapping scaffold` 入口
///
/// source 必须是内置语言代码；output 为 None 时默认写入
/// 项目语言包根 `<target>/crates/`（主仓库内为 crates/engine/lang-packs/，
/// 用户项目为 lang-packs/）。
/// provider：`rule`（默认，生成 TODO 骨架待人工翻译）或
/// `deepseek`（AI 自动翻译键名，需 DEEPSEEK_API_KEY）。
pub fn run_scaffold(
    source: &str,
    target: &str,
    output: Option<&Path>,
    provider: &str,
) -> anyhow::Result<()> {
    let ui = crate::ui::Ui::global();
    if !crate::builtin_lang::has_builtin_lang(source) {
        anyhow::bail!(ui.f("mc_unknown_source", &[source]));
    }
    let view = LangPackView::from_builtin(source);
    if view.crates.is_empty() {
        anyhow::bail!(ui.f("mc_no_crates", &[source]));
    }
    let output_dir = match output {
        Some(dir) => dir.to_path_buf(),
        None => {
            let base = std::env::current_dir()
                .ok()
                .and_then(|cwd| crate::find_project_root_upward(&cwd))
                .unwrap_or_else(|| PathBuf::from("."));
            crate::lang_pack_root_of(&base).join(format!("{}/crates", target))
        }
    };
    let (created, skipped) = scaffold(&view, target, &output_dir)?;
    if created > 0 {
        println!(
            "{}",
            ui.f(
                "mc_scaffold_generated",
                &[&created.to_string(), &output_dir.display().to_string()]
            )
        );
    }
    if skipped > 0 {
        println!(
            "{}",
            ui.f("mc_scaffold_skip_existing", &[&skipped.to_string()])
        );
    }
    if provider == "deepseek" {
        ai_translate_scaffold(&view, source, target, &output_dir)?;
    } else {
        println!("{}", ui.f("mc_scaffold_hint", &[target]));
    }
    Ok(())
}

// ==================== AI 翻译（--provider deepseek） ====================

/// 每批送 AI 的键数上限
const AI_BATCH_SIZE: usize = 60;
/// 冲突改名最大重试轮数
const AI_MAX_RETRY_ROUNDS: usize = 2;

/// AI 翻译脚手架：提取 TODO 键 → 批量翻译 → 写回 → 校验冲突 → 改名重试
fn ai_translate_scaffold(
    source_view: &LangPackView,
    source_lang: &str,
    target: &str,
    output_dir: &Path,
) -> anyhow::Result<()> {
    let ui = crate::ui::Ui::global();
    // 禁用词：源包 keywords 键（译名撞关键字会永不生效）
    let forbidden: Vec<String> = extract_keyword_map(&source_view.keywords_toml)
        .keys()
        .cloned()
        .collect();

    let items = collect_todo_keys(output_dir, target)?;
    if items.is_empty() {
        return Ok(());
    }
    // 按 (键, 英文值) 去重，保证跨文件同键翻译一致
    let mut unique: Vec<(String, String)> = Vec::new();
    for (_, key, value) in &items {
        if !unique.iter().any(|(k, v)| k == key && v == value) {
            unique.push((key.clone(), value.clone()));
        }
    }

    // 批量翻译
    let mut translations: HashMap<String, String> = HashMap::new();
    let total_batches = unique.len().div_ceil(AI_BATCH_SIZE);
    for (i, batch) in unique.chunks(AI_BATCH_SIZE).enumerate() {
        println!(
            "{}",
            ui.f(
                "mc_scaffold_ai_batch",
                &[&(i + 1).to_string(), &total_batches.to_string()]
            )
        );
        let used: Vec<String> = translations.values().cloned().collect();
        let batch_map = ai_translate_keys(source_lang, target, batch, &forbidden, &used)?;
        translations.extend(batch_map);
    }
    let replaced = apply_translations(output_dir, &translations, target);
    println!(
        "{}",
        ui.f(
            "mc_scaffold_ai_done",
            &[&replaced.to_string(), &items.len().to_string()]
        )
    );
    if replaced < items.len() {
        println!(
            "{}",
            ui.f(
                "mc_scaffold_ai_partial",
                &[&(items.len() - replaced).to_string()]
            )
        );
    }

    // 校验回环：对生成的 crates 目录重新校验，冲突键送 AI 改名重试
    for round in 1..=AI_MAX_RETRY_ROUNDS {
        let report = check_lang_pack(&view_from_crates_dir(output_dir, source_view));
        let conflict_items = collect_conflict_items(&report, output_dir);
        if conflict_items.is_empty() {
            if !report.warnings.is_empty() {
                println!(
                    "{}",
                    ui.f("mc_ok_with_warnings", &[&report.warnings.len().to_string()])
                );
            } else {
                println!("{}", ui.t("mc_ok"));
            }
            return Ok(());
        }
        println!(
            "{}",
            ui.f(
                "mc_scaffold_ai_retry",
                &[
                    &conflict_items.len().to_string(),
                    &round.to_string(),
                    &AI_MAX_RETRY_ROUNDS.to_string()
                ]
            )
        );
        let renames = ai_rename_conflicts(target, &conflict_items, &forbidden)?;
        apply_file_renames(output_dir, &renames, target);
    }
    // 重试后仍可能有残留冲突：输出最终报告供人工处理
    let view = view_from_crates_dir(output_dir, source_view);
    let report = check_lang_pack(&view);
    if !report.passed() {
        print_report(target, &report);
        println!("{}", ui.f("mc_scaffold_ai_retry_left", &[target]));
    } else {
        println!("{}", ui.t("mc_ok"));
    }
    Ok(())
}

/// 扫描输出目录中含 `TODO(<target>)` 标记的行，返回 (文件名, 键, 英文值)
fn collect_todo_keys(
    output_dir: &Path,
    target: &str,
) -> anyhow::Result<Vec<(String, String, String)>> {
    let mut items = Vec::new();
    let todo_mark = format!("TODO({target})");
    for entry in list_toml_files(output_dir)? {
        let file_name = entry
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let content = std::fs::read_to_string(&entry)?;
        for line in content.lines() {
            if !line.contains(&todo_mark) {
                continue;
            }
            if let Some((key, value)) = parse_key_value_line(line) {
                items.push((file_name.clone(), key, value));
            }
        }
    }
    Ok(items)
}

/// 列出目录下的 .toml 文件（按名排序保证确定性）
fn list_toml_files(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("toml"))
        .collect();
    files.sort();
    Ok(files)
}

/// 解析 `"键" = "值" ...` 行，返回 (键, 值)
fn parse_key_value_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    let mut parts = trimmed.splitn(2, '=');
    let key_part = parts.next()?.trim();
    let value_part = parts.next()?.trim();
    let key = key_part.strip_prefix('"')?.strip_suffix('"')?;
    // 值可能带行尾注释，取第一个引号对
    let value = value_part.strip_prefix('"')?;
    let value = value.split('"').next()?;
    Some((key.to_string(), value.to_string()))
}

/// 调用 AI 批量翻译键名，返回 源键→目标语言键
fn ai_translate_keys(
    source_lang: &str,
    target: &str,
    items: &[(String, String)],
    forbidden: &[String],
    used: &[String],
) -> anyhow::Result<HashMap<String, String>> {
    let system_prompt = "You are a programming terminology translation engine for a \
Rust teaching dialect. Translate mapping keys from the source language into the \
target language. Rules:\n\
1. Output ONLY a JSON object {\"source_key\": \"translated_key\"}, no explanation.\n\
2. Translations must be natural programming terms in the target language.\n\
3. Never reuse words from the forbidden list (language keywords) or the used list.\n\
4. Different source keys must map to different translations.\n\
5. Keep the same style as existing pack keys (single word or short phrase).";
    let key_list = items
        .iter()
        .map(|(k, v)| format!("{} = {}", k, v))
        .collect::<Vec<_>>()
        .join("\n");
    let user_prompt = format!(
        "source language: {}\ntarget language: {}\nforbidden words: {}\n\
used words: {}\nkeys to translate (key = its English API):\n{}",
        source_lang,
        target,
        forbidden.join(", "),
        used.join(", "),
        key_list
    );
    let content = crate::mapping_gen::deepseek_chat(system_prompt, &user_prompt)?;
    parse_json_map(&content)
}

/// 从校验报告提取冲突项 (文件名, 键)，供改名重试
fn collect_conflict_items(report: &CheckReport, output_dir: &Path) -> Vec<(String, String)> {
    let mut items: Vec<(String, String)> = Vec::new();
    for error in &report.errors {
        let parts: Vec<&str> = error.split('|').collect();
        match parts.first().copied() {
            Some("mc_keyword_collision") if parts.len() >= 4 => {
                items.push((parts[1].to_string(), parts[3].to_string()));
            }
            Some("mc_cross_conflict") if parts.len() >= 6 => {
                let key = parts[1];
                items.push((parts[2].to_string(), key.to_string()));
                items.push((parts[4].to_string(), key.to_string()));
            }
            _ => {}
        }
    }
    // 去重并确认文件仍存在
    items.retain(|(file, _)| output_dir.join(file).is_file());
    items.sort();
    items.dedup();
    items
}

/// 调用 AI 为冲突键改名，返回 (文件名, 旧键)→新键
fn ai_rename_conflicts(
    target: &str,
    items: &[(String, String)],
    forbidden: &[String],
) -> anyhow::Result<HashMap<(String, String), String>> {
    let system_prompt = "You rename conflicting mapping keys of a Rust teaching \
dialect language pack. Rules:\n\
1. Output ONLY a JSON object {\"file|key\": \"new_key\"}, no explanation.\n\
2. New keys must be multi-word terms in the target language, avoiding the forbidden \
list and all other existing keys.\n\
3. Keep the technical meaning of the original key.";
    let list = items
        .iter()
        .map(|(f, k)| format!("{}|{}", f, k))
        .collect::<Vec<_>>()
        .join("\n");
    let user_prompt = format!(
        "target language: {}\nforbidden words: {}\n\
conflicting entries (file|key) to rename:\n{}",
        target,
        forbidden.join(", "),
        list
    );
    let content = crate::mapping_gen::deepseek_chat(system_prompt, &user_prompt)?;
    let raw = parse_json_map(&content)?;
    let mut renames = HashMap::new();
    for (composite, new_key) in raw {
        if let Some((file, key)) = composite.split_once('|') {
            renames.insert((file.to_string(), key.to_string()), new_key);
        }
    }
    Ok(renames)
}

/// 解析 AI 返回文本中的 JSON 对象（容忍 ```json 围栏与前后杂讯）
fn parse_json_map(text: &str) -> anyhow::Result<HashMap<String, String>> {
    let start = text
        .find('{')
        .ok_or_else(|| anyhow::anyhow!("AI 返回中未找到 JSON 对象"))?;
    let end = text[start..]
        .rfind('}')
        .map(|pos| start + pos + 1)
        .ok_or_else(|| anyhow::anyhow!("AI 返回的 JSON 不完整"))?;
    let value: serde_json::Value = serde_json::from_str(&text[start..end])?;
    let mut map = HashMap::new();
    if let serde_json::Value::Object(obj) = value {
        for (k, v) in obj {
            if let serde_json::Value::String(s) = v
                && !k.is_empty()
                && !s.is_empty()
            {
                map.insert(k, s);
            }
        }
    }
    Ok(map)
}

/// 将翻译结果写回：替换 TODO 行的键并移除 TODO 注释；未命中键保留待人工
fn apply_translations(
    output_dir: &Path,
    translations: &HashMap<String, String>,
    target: &str,
) -> usize {
    let mut replaced = 0;
    let todo_mark = format!("TODO({target})");
    for entry in list_toml_files(output_dir).unwrap_or_default() {
        let content = match std::fs::read_to_string(&entry) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let mut out = String::new();
        for line in content.lines() {
            if line.contains(&todo_mark)
                && let Some((key, value)) = parse_key_value_line(line)
                && let Some(new_key) = translations.get(&key)
            {
                // 重写为无 TODO 的行（丢弃原行尾注释中的 TODO 部分）
                out.push_str(&format!("\"{}\" = \"{}\"\n", new_key, value));
                replaced += 1;
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        let _ = std::fs::write(&entry, out);
    }
    replaced
}

/// 按 (文件名, 旧键) 精确改名（冲突重试轮次用）
fn apply_file_renames(
    output_dir: &Path,
    renames: &HashMap<(String, String), String>,
    target: &str,
) {
    for entry in list_toml_files(output_dir).unwrap_or_default() {
        let file_name = entry
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let content = match std::fs::read_to_string(&entry) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let mut out = String::new();
        for line in content.lines() {
            let mut written = false;
            if let Some((key, value)) = parse_key_value_line(line)
                && let Some(new_key) = renames.get(&(file_name.clone(), key.clone()))
            {
                let suffix = if line.contains(&format!("TODO({target})")) {
                    String::new()
                } else {
                    // 保留原有行尾注释（非 TODO 部分）
                    line.split('#')
                        .nth(1)
                        .map(|c| format!("  # {}", c.trim()))
                        .unwrap_or_default()
                };
                out.push_str(&format!("\"{}\" = \"{}\"{}\n", new_key, value, suffix));
                written = true;
            }
            if !written {
                out.push_str(line);
                out.push('\n');
            }
        }
        let _ = std::fs::write(&entry, out);
    }
}

/// 从已生成的 crates 目录构造校验视图（keywords/stdlib 取自源包）
fn view_from_crates_dir(crates_dir: &Path, source_view: &LangPackView) -> LangPackView {
    let mut crates = Vec::new();
    for entry in list_toml_files(crates_dir).unwrap_or_default() {
        let name = entry
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.toml")
            .to_string();
        if let Ok(content) = std::fs::read_to_string(&entry) {
            crates.push((name, content));
        }
    }
    // 目标语言包目录（crates 的父目录）若已有 keywords/stdlib 则优先用它们
    let lang_dir = crates_dir.parent().map(|p| p.to_path_buf());
    let keywords_toml = lang_dir
        .as_ref()
        .map(|d| std::fs::read_to_string(d.join("keywords.toml")).unwrap_or_default())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| source_view.keywords_toml.clone());
    let stdlib_toml = lang_dir
        .as_ref()
        .map(|d| std::fs::read_to_string(d.join("stdlib.toml")).unwrap_or_default())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| source_view.stdlib_toml.clone());
    LangPackView {
        lang: crates_dir
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string(),
        keywords_toml,
        stdlib_toml,
        crates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造最小视图用于测试
    fn view_with_crates(crates: Vec<(&str, &str)>) -> LangPackView {
        LangPackView {
            lang: "zh".to_string(),
            keywords_toml: "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n".to_string(),
            stdlib_toml: "[\"标识符\"]\n\"字符串\" = \"String\"\n".to_string(),
            crates: crates
                .into_iter()
                .map(|(n, c)| (n.to_string(), c.to_string()))
                .collect(),
        }
    }

    /// 正常映射通过校验
    #[test]
    fn test_clean_mapping_passes() {
        let view = view_with_crates(vec![(
            "a.toml",
            "[\"标识符\"]\n\"服务器\" = \"Server\"\n\"路由\" = \"Router\"\n",
        )]);
        let report = check_lang_pack(&view);
        assert!(report.passed(), "干净映射应通过: {:?}", report.errors);
        assert_eq!(report.stats.ident_entries, 2);
        assert_eq!(report.stats.crate_files, 1);
    }

    /// crates 键与 keywords 键相撞报 error
    #[test]
    fn test_keyword_collision_detected() {
        let view = view_with_crates(vec![(
            "a.toml",
            "[\"标识符\"]\n\"函数\" = \"some_fn\"\n", // "函数" 是关键字
        )]);
        let report = check_lang_pack(&view);
        assert!(!report.passed());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.starts_with("mc_keyword_collision"))
        );
    }

    /// crates 文件之间同键不同值报 error
    #[test]
    fn test_cross_file_conflict_detected() {
        let view = view_with_crates(vec![
            ("a.toml", "[\"标识符\"]\n\"连接\" = \"connect_a\"\n"),
            ("b.toml", "[\"标识符\"]\n\"连接\" = \"connect_b\"\n"),
        ]);
        let report = check_lang_pack(&view);
        assert!(!report.passed());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.starts_with("mc_cross_conflict"))
        );
    }

    /// crates 文件之间同键同值不报错
    #[test]
    fn test_cross_file_same_value_ok() {
        let view = view_with_crates(vec![
            ("a.toml", "[\"标识符\"]\n\"连接\" = \"connect\"\n"),
            ("b.toml", "[\"标识符\"]\n\"连接\" = \"connect\"\n"),
        ]);
        let report = check_lang_pack(&view);
        assert!(
            !report
                .errors
                .iter()
                .any(|e| e.starts_with("mc_cross_conflict")),
            "同键同值不应报冲突: {:?}",
            report.errors
        );
    }

    /// crates 键与 stdlib 标识符同键不同值报 warning（不阻断）
    #[test]
    fn test_stdlib_shadow_warning() {
        let view = view_with_crates(vec![(
            "a.toml",
            "[\"标识符\"]\n\"字符串\" = \"MyString\"\n", // stdlib 中 "字符串" = "String"
        )]);
        let report = check_lang_pack(&view);
        assert!(report.passed(), "stdlib 覆盖只是 warning 不阻断");
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.starts_with("mc_stdlib_shadow"))
        );
    }

    /// 重复键导致 TOML 解析失败，报 error
    #[test]
    fn test_duplicate_key_parse_error() {
        let view = view_with_crates(vec![(
            "a.toml",
            "[\"标识符\"]\n\"连接\" = \"connect\"\n\"连接\" = \"link\"\n",
        )]);
        let report = check_lang_pack(&view);
        assert!(!report.passed());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.starts_with("mc_parse_failed"))
        );
    }

    /// scaffold 为键值行追加 TODO 注释，节标题与注释保留
    #[test]
    fn test_scaffold_adds_todo() {
        let content = "[\"标识符\"]\n# 注释行\n\"服务器\" = \"Server\"\n";
        let result = add_scaffold_todo(content, "zh", "vi");
        assert!(result.contains("TODO(vi)"));
        assert!(result.contains("# 注释行"));
        assert!(result.contains("\"服务器\" = \"Server\""));
    }

    /// 内置中文包自检：历史冲突已清理，校验应完全通过。
    #[test]
    fn test_builtin_zh_passes_clean() {
        let view = LangPackView::from_builtin("zh");
        let report = check_lang_pack(&view);
        // 工具正常运行，统计数据合理
        assert_eq!(report.stats.crate_files, 10);
        assert!(report.stats.ident_entries > 0);
        // 历史冲突（关键字避让/跨文件同键不同值）已清理完毕
        assert!(report.passed(), "内置 zh 包应无错误: {:?}", report.errors);
    }

    /// 各内置语言的标识符条目数均可统计
    #[test]
    fn test_entry_counts_per_lang() {
        for lang in ["zh", "ru", "de"] {
            let view = LangPackView::from_builtin(lang);
            let report = check_lang_pack(&view);
            assert!(report.stats.ident_entries > 0, "{lang} 应有条目");
        }
    }

    /// 全部内置语言均应通过校验（CI 门禁的测试层基线）
    #[test]
    fn test_all_builtin_langs_pass() {
        for lang in crate::builtin_lang::builtin_lang_codes() {
            let view = LangPackView::from_builtin(lang);
            let report = check_lang_pack(&view);
            assert!(
                report.passed(),
                "内置包 {lang} 应通过校验: {:?}",
                report.errors
            );
        }
    }

    /// 键值行解析：提取引号内键与值，忽略行尾注释
    #[test]
    fn test_parse_key_value_line() {
        let (k, v) =
            parse_key_value_line("\"服务器\" = \"Server\"  # TODO(vi): 将键从 zh 翻译").unwrap();
        assert_eq!(k, "服务器");
        assert_eq!(v, "Server");
        assert!(parse_key_value_line("# 注释行").is_none());
        assert!(parse_key_value_line("[\"标识符\"]").is_none());
    }

    /// AI 返回 JSON 解析：容忍围栏与杂讯，丢弃空条目
    #[test]
    fn test_parse_json_map() {
        let text =
            "好的，以下是翻译结果：\n```json\n{\"服务器\": \"Máy chủ\", \"空\": \"\"}\n```\n";
        let map = parse_json_map(text).unwrap();
        assert_eq!(map.get("服务器").unwrap(), "Máy chủ");
        assert!(!map.contains_key("空"), "空值条目应丢弃");
        assert!(parse_json_map("没有任何 JSON").is_err());
    }

    /// TODO 键提取 + 翻译写回：替换键并移除 TODO 标记，未命中键保留
    #[test]
    fn test_collect_and_apply_translations() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.toml");
        std::fs::write(
            &file,
            "[\"标识符\"]\n\"服务器\" = \"Server\"  # TODO(vi): 将键从 zh 翻译\n\
             \"路由\" = \"Router\"  # TODO(vi): 将键从 zh 翻译\n",
        )
        .unwrap();

        let items = collect_todo_keys(dir.path(), "vi").unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].1, "服务器");

        let mut translations = HashMap::new();
        translations.insert("服务器".to_string(), "Máy chủ".to_string());
        let replaced = apply_translations(dir.path(), &translations, "vi");
        assert_eq!(replaced, 1);

        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.contains("\"Máy chủ\" = \"Server\""));
        assert!(
            !content.contains("Máy chủ\" = \"Server\"  # TODO"),
            "已译键不应带 TODO"
        );
        assert!(content.contains("TODO(vi)"), "未命中键应保留 TODO");
    }

    /// 冲突项提取：keyword_collision 与 cross_conflict 均能定位到 (文件, 键)
    #[test]
    fn test_collect_conflict_items() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.toml"), "").unwrap();
        std::fs::write(dir.path().join("b.toml"), "").unwrap();
        let mut report = CheckReport::default();
        report
            .errors
            .push("mc_keyword_collision|a.toml|标识符|错误|bail|Err".to_string());
        report
            .errors
            .push("mc_cross_conflict|连接|a.toml|join|b.toml|Connection".to_string());
        let items = collect_conflict_items(&report, dir.path());
        assert!(items.contains(&("a.toml".to_string(), "错误".to_string())));
        assert!(items.contains(&("a.toml".to_string(), "连接".to_string())));
        assert!(items.contains(&("b.toml".to_string(), "连接".to_string())));
    }

    /// 按文件精确改名：保留行尾注释，仅替换目标键
    #[test]
    fn test_apply_file_renames() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.toml");
        std::fs::write(
            &file,
            "\"连接\" = \"join\"\n\"其他\" = \"other\"  # 注释保留\n",
        )
        .unwrap();
        let mut renames = HashMap::new();
        renames.insert(
            ("a.toml".to_string(), "连接".to_string()),
            "连接等待".to_string(),
        );
        apply_file_renames(dir.path(), &renames, "vi");
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.contains("\"连接等待\" = \"join\""));
        assert!(content.contains("\"其他\" = \"other\"  # 注释保留"));
    }
}
