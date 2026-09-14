//! 自动映射生成：`rzc mapping auto` 的实现。
//!
//! 流程：在临时项目中把目标 crate 作为依赖，用 `cargo build` 编译后，
//! 通过 `cargo metadata` 定位 registry 源码，再手动调用 rustdoc 生成 JSON 文档，
//! 解析出公开 API（名称 + 类型签名），最后由 AI 或规则生成中文映射。
//!
//! 法律合规（本项目强制约束）：
//! 1. 只提取 API 名称与类型签名（rustdoc JSON 的 `name` / `inner` 字段），
//!    绝不读取 doc comment（`docs` 字段）；`docs` 字段在本模块任何地方都不会被访问。
//! 2. 生成的映射文件不含任何来自原 crate 的文档注释翻译或复制。
//! 3. 解释由 AI 根据 API 英文名称和类型签名自行生成；规则模式解释留空。
//! 4. 输出文件头部固定附免责声明（见 [`disclaimer_text`]）。
//!
//! 子模块划分（与本文件此前的分段注释一致）：
//! - [`rustdoc_extract`]：rustdoc JSON 解析与签名渲染
//! - [`doc_json`]：工具链（临时项目 + metadata + build + rustdoc）
//! - [`rules`]：规则驱动的中文名生成
//! - [`toml_output`]：映射 TOML 构建
//! - [`ai`]：AI 驱动（DeepSeek，OpenAI 兼容接口）

mod ai;
mod doc_json;
mod rules;
mod rustdoc_extract;
mod toml_output;

pub use ai::{call_ai_generate_mapping, deepseek_chat, detect_system_language};
pub use rules::{
    detect_keyword_conflicts, generate_crate_localized_name, rule_generate_localized_name,
};
pub use rustdoc_extract::extract_public_api;
pub use toml_output::build_mapping_toml;

use anyhow::{Context, bail};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// 输出文件头部的法律免责声明（按目标语言输出，逐字写入生成文件）
pub fn disclaimer_text(lang: &str) -> String {
    crate::ui::Ui::for_lang(lang).t("mapping_disclaimer")
}

/// API 种类（对应需求：函数/结构体/枚举/特征/类型别名/宏/常量）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiKind {
    /// 函数
    Function,
    /// 结构体
    Struct,
    /// 枚举
    Enum,
    /// 特征（trait）
    Trait,
    /// 类型别名
    TypeAlias,
    /// 宏
    Macro,
    /// 常量
    Const,
}

impl ApiKind {
    /// 显示名（随界面语言变化）
    pub fn display(&self) -> String {
        let key = match self {
            ApiKind::Function => "mapping_kind_function",
            ApiKind::Struct => "mapping_kind_struct",
            ApiKind::Enum => "mapping_kind_enum",
            ApiKind::Trait => "mapping_kind_trait",
            ApiKind::TypeAlias => "mapping_kind_type_alias",
            ApiKind::Macro => "mapping_kind_macro",
            ApiKind::Const => "mapping_kind_const",
        };
        crate::ui::Ui::global().t(key)
    }
}

/// 提取出的公开 API 条目：只含名称与类型签名（无任何文档内容）
#[derive(Debug, Clone)]
pub struct ApiEntry {
    /// API 种类
    pub kind: ApiKind,
    /// 英文原名
    pub english_name: String,
    /// 类型签名，如 `fn new() -> Result<Self>`、`struct Error`、`const MAX: u32`
    pub signature: String,
}

/// `--target-version` 校验并转换为 Cargo 版本需求（生成基准锁定）：
///
/// - `x.y.z`（可带 `-预发布` / `+构建` 后缀）→ `=x.y.z` 精确锁定；
/// - `x.y` → `x.y.*`、`x` → `x.*`（该线最新，生产映射建议完整版本号）；
/// - 兼容前导 `=` 与 `v` 写法（如 `=2.11.5`、`v2.11.5`）；
/// - 非法输入返回 `None`（调用方以 `mapping_version_invalid` 提示）。
pub(crate) fn version_requirement(version: &str) -> Option<String> {
    let version = version.strip_prefix('=').unwrap_or(version);
    let version = version
        .strip_prefix('v')
        .or_else(|| version.strip_prefix('V'))
        .unwrap_or(version);
    // 拆出构建(+)与预发布(-)后缀（仅完整 x.y.z 才允许带后缀）
    let (rest, build) = match version.split_once('+') {
        Some((a, b)) => (a, Some(b)),
        None => (version, None),
    };
    let (core, pre_release) = match rest.split_once('-') {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    let numeric = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    if parts.is_empty() || parts.len() > 3 || parts.iter().any(|p| !numeric(p)) {
        return None;
    }
    // 后缀字符集：字母、数字、点、连字符，且点分段非空（语义版本规范）
    let suffix_valid = |suffix: &str| {
        !suffix.is_empty()
            && suffix.split('.').all(|seg| {
                !seg.is_empty() && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            })
    };
    if (pre_release.is_some() || build.is_some()) && parts.len() != 3 {
        return None;
    }
    if pre_release.is_some_and(|p| !suffix_valid(p)) || build.is_some_and(|b| !suffix_valid(b)) {
        return None;
    }
    match parts.len() {
        3 => Some(format!("={}", version)),
        2 => Some(format!("{}.{}.*", parts[0], parts[1])),
        _ => Some(format!("{}.*", parts[0])),
    }
}

/// AI 解释覆盖率（生成后自检）：按最终母语名表统计非空解释。
///
/// AI 偶发省略部分条目的解释（同批不同调用间有随机波动），生成后
/// 显式输出覆盖率，便于及时发现缺失并重跑补齐。
fn explanation_coverage(
    chinese_name_table: &[(String, String)],
    explanation_table: &HashMap<String, String>,
) -> (usize, usize) {
    let covered = chinese_name_table
        .iter()
        .filter(|(name, _)| {
            explanation_table
                .get(name)
                .is_some_and(|text| !text.is_empty())
        })
        .count();
    (covered, chinese_name_table.len())
}

/// 主入口：`rzc mapping auto`
///
/// - `crate_name`：目标 crate（已安装或可从 crates.io 拉取）
/// - `lang`：语言包目录名（如 zh、ru），用于冲突检测与默认输出位置
/// - `provider`：`deepseek`（调用 AI）或 `rule`（离线规则模式）
/// - `output_path`：输出文件路径
/// - `target_version`：锁定提取基准版本（`2.11.5` → `=2.11.5` 精确锁定；
///   `2.11` / `2` → 该线最新）；`None` 时解析最新版（结果不可复现，打印提示）
pub fn run_auto_generate(
    crate_name: &str,
    lang: &str,
    provider: &str,
    output_path: &Path,
    target_version: Option<&str>,
) -> anyhow::Result<()> {
    let ui = crate::ui::Ui::for_lang(lang);
    if crate_name.is_empty() {
        bail!("{}", ui.t("mapping_crate_empty"));
    }
    if !crate_name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("{}", ui.f("mapping_crate_invalid", &[crate_name]));
    }
    // 版本锁定参数校验：非法输入直接报错，避免生成基准被静默放宽
    let version_requirement = match target_version {
        Some(version) => match version_requirement(version) {
            Some(requirement) => Some(requirement),
            None => bail!("{}", ui.f("mapping_version_invalid", &[version])),
        },
        None => {
            eprintln!("{}", ui.t("mapping_version_unlocked"));
            None
        }
    };

    println!("{}", ui.f("mapping_extracting", &[crate_name]));
    let (doc_jsons, resolved_version) =
        doc_json::extract_crate_doc(crate_name, version_requirement.as_deref())?;
    // 薄壳 crate（meta crate，如 salvo 仅 `pub use salvo_core::*`）的公开 API
    // 来自 glob 重导出链上的依赖 crate（如 salvo_core），逐个 JSON 合并提取；
    // 同名 API 只保留首个条目（链上 crate 可能导出同名类型）
    let mut entries = Vec::new();
    let mut seen_english_names = HashSet::new();
    for (doc_crate_name, json_text) in &doc_jsons {
        println!("{}", ui.f("mapping_extracting", &[doc_crate_name]));
        for entry in extract_public_api(json_text)? {
            if seen_english_names.insert(entry.english_name.clone()) {
                entries.push(entry);
            }
        }
    }
    if entries.is_empty() {
        bail!("{}", ui.f("mapping_no_api", &[crate_name]));
    }

    // 统计各类数量
    let mut stats: HashMap<ApiKind, usize> = HashMap::new();
    for entry in &entries {
        *stats.entry(entry.kind).or_insert(0) += 1;
    }
    let stats_text = [
        ApiKind::Function,
        ApiKind::Struct,
        ApiKind::Enum,
        ApiKind::Trait,
        ApiKind::TypeAlias,
        ApiKind::Const,
        ApiKind::Macro,
    ]
    .iter()
    .map(|kind| {
        format!(
            "{} {}",
            stats.get(kind).copied().unwrap_or(0),
            kind.display()
        )
    })
    .collect::<Vec<_>>()
    .join(if lang == "zh" { "、" } else { ", " });
    println!(
        "{}",
        ui.f(
            "mapping_extracted",
            &[&entries.len().to_string(), &stats_text]
        )
    );
    // rustdoc JSON 格式当前工具链不输出 macro_rules! 宏定义（官方格式限制），提示用户
    if !stats.contains_key(&ApiKind::Macro) {
        eprintln!("{}", ui.t("mapping_no_macro"));
    }

    // 1. 名称：zh 走规则生成中文名，其他语言保留英文原名（AI 模式成功后由 AI 结果覆盖）
    let mut chinese_name_table: Vec<(String, String)> = Vec::new();
    let mut used_chinese_names = HashSet::new();
    for entry in &entries {
        let chinese_name = rule_generate_localized_name(lang, &entry.english_name);
        if !used_chinese_names.insert(chinese_name.clone()) {
            eprintln!(
                "{}",
                ui.f(
                    "mapping_name_conflict",
                    &[&chinese_name, &entry.english_name]
                )
            );
            continue;
        }
        chinese_name_table.push((chinese_name, entry.english_name.clone()));
    }

    // 2. 解释：AI 模式调用服务商，失败或无配置回退规则模式（解释留空）
    let mut explanation_table: HashMap<String, String> = HashMap::new();
    match provider {
        "deepseek" => {
            match call_ai_generate_mapping(crate_name, lang, &entries) {
                Ok((ai_identifiers, ai_explanations)) => {
                    // AI 中文名覆盖规则名（校验英文名合法性，防止 AI 幻觉改名）
                    for (chinese_name, english_name) in ai_identifiers {
                        // 先采集 AI 解释：即使名字与规则名重复（常见情形），
                        // 解释也不应丢弃
                        if let Some(explanation) = ai_explanations.get(&chinese_name)
                            && !explanation.is_empty()
                        {
                            explanation_table.insert(chinese_name.clone(), explanation.clone());
                        }
                        if chinese_name_table
                            .iter()
                            .any(|(name, _)| name == &chinese_name)
                        {
                            continue;
                        }
                        if let Some(pos) = chinese_name_table
                            .iter()
                            .position(|(_, en)| en == &english_name)
                        {
                            chinese_name_table.remove(pos);
                            chinese_name_table.push((chinese_name.clone(), english_name));
                        }
                    }
                    println!("{}", ui.f("mapping_ai_success", &[provider]));
                    // 解释覆盖率自检：缺失时显式警告（AI 偶发省略条目）
                    let (covered, total) =
                        explanation_coverage(&chinese_name_table, &explanation_table);
                    if covered < total {
                        eprintln!(
                            "{}",
                            ui.f(
                                "mapping_ai_coverage_missing",
                                &[
                                    &covered.to_string(),
                                    &total.to_string(),
                                    &(total - covered).to_string()
                                ]
                            )
                        );
                    } else {
                        println!(
                            "{}",
                            ui.f(
                                "mapping_ai_coverage",
                                &[&covered.to_string(), &total.to_string()]
                            )
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "{}",
                        ui.f("mapping_ai_fallback", &["DEEPSEEK_API_KEY", &e.to_string()])
                    );
                }
            }
        }
        "rule" => {
            eprintln!("{}", ui.t("mapping_rule_mode"));
        }
        other => bail!("{}", ui.f("mapping_unknown_provider", &[other])),
    }

    // 3. 输出 TOML
    let crate_chinese_name = generate_crate_localized_name(lang, crate_name);
    let toml = build_mapping_toml(
        lang,
        crate_name,
        &crate_chinese_name,
        resolved_version.as_deref(),
        &entries,
        &chinese_name_table,
        &explanation_table,
    );

    // 4. 冲突检测：词法转译（关键字映射）先于别名替换执行，冲突的中文名生成后不会生效；
    // 语言包目录优先从输出路径推导（…/<lang>/crates/<crate>.toml 的上两级），
    // 推导失败时回退 cwd 的项目语言包根（主仓库为 crates/engine/lang-packs/）
    let lang_pack_dir = output_path
        .parent()
        .and_then(|p| p.parent())
        .filter(|dir| dir.join("keywords.toml").exists())
        .map(|dir| dir.to_path_buf())
        .unwrap_or_else(|| {
            std::env::current_dir()
                .map(|cwd| crate::lang_pack_root_of(&cwd).join(lang))
                .unwrap_or_else(|_| PathBuf::from(format!("lang-packs/{}", lang)))
        });
    for (chinese, keyword_english, this_english) in
        detect_keyword_conflicts(&lang_pack_dir, &chinese_name_table)
    {
        eprintln!(
            "{}",
            ui.f(
                "mapping_keyword_conflict",
                &[&chinese, &chinese, &keyword_english, &this_english]
            )
        );
    }

    // 目标文件已存在时给出覆盖警告（防止静默覆盖手工调整过的映射）
    if output_path.exists() {
        eprintln!(
            "{}",
            ui.f(
                "mapping_overwrite_warn",
                &[&output_path.display().to_string()]
            )
        );
    }
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| ui.f("mg_err_mkdir", &[&parent.display().to_string()]))?;
    }
    fs::write(output_path, toml)
        .with_context(|| ui.f("mg_err_write", &[&output_path.display().to_string()]))?;
    println!(
        "{}",
        ui.f("mapping_generated", &[&output_path.display().to_string()])
    );
    println!(
        "{}",
        ui.f(
            "mapping_crate_name_line",
            &[&crate_chinese_name, &crate_chinese_name, crate_name]
        )
    );
    println!("{}", ui.f("mapping_usage_hint", &[lang]));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{explanation_coverage, version_requirement};
    use std::collections::HashMap;

    /// 完整三段版本 → `=x.y.z` 精确锁定；兼容 =/v 前导与预发布/构建后缀
    #[test]
    fn test_version_requirement_exact() {
        assert_eq!(version_requirement("2.11.5").as_deref(), Some("=2.11.5"));
        assert_eq!(version_requirement("=2.11.5").as_deref(), Some("=2.11.5"));
        assert_eq!(version_requirement("v2.11.5").as_deref(), Some("=2.11.5"));
        assert_eq!(
            version_requirement("3.0.0-alpha.0").as_deref(),
            Some("=3.0.0-alpha.0")
        );
        assert_eq!(
            version_requirement("1.0.0+build.7").as_deref(),
            Some("=1.0.0+build.7")
        );
    }

    /// 一段/两段版本 → 该线最新（前缀锁定）
    #[test]
    fn test_version_requirement_prefix_lines() {
        assert_eq!(version_requirement("2.11").as_deref(), Some("2.11.*"));
        assert_eq!(version_requirement("2").as_deref(), Some("2.*"));
    }

    /// 非法输入一律 None（由调用方报 mapping_version_invalid）
    #[test]
    fn test_version_requirement_invalid() {
        for bad in [
            "",
            " ",
            "abc",
            "2.11.*",
            "2.11.5.6",
            "2..5",
            "2.11.5 x",
            "-1.0.0",
            "2.11.5-",
            "2.11.5-alpha..1",
            "v",
            "=",
        ] {
            assert_eq!(version_requirement(bad), None, "应判非法: {bad:?}");
        }
    }

    /// 解释覆盖率：空串解释不计入；空表安全返回 (0, 0)
    #[test]
    fn test_explanation_coverage() {
        let table = vec![
            ("新建".to_string(), "new".to_string()),
            ("错误".to_string(), "Error".to_string()),
            ("状态".to_string(), "State".to_string()),
        ];
        let mut explanations = HashMap::new();
        explanations.insert("新建".to_string(), "创建新对象。".to_string());
        explanations.insert("错误".to_string(), String::new());
        assert_eq!(explanation_coverage(&table, &explanations), (1, 3));
        explanations.insert("错误".to_string(), "错误类型。".to_string());
        explanations.insert("状态".to_string(), "状态值。".to_string());
        assert_eq!(explanation_coverage(&table, &explanations), (3, 3));
        assert_eq!(explanation_coverage(&[], &explanations), (0, 0));
    }
}
