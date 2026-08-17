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

use anyhow::{Context, anyhow, bail};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// 主入口：`rzc mapping auto`
///
/// - `crate_name`：目标 crate（已安装或可从 crates.io 拉取）
/// - `lang`：语言包目录名（如 zh、ru），用于冲突检测与默认输出位置
/// - `provider`：`deepseek`（调用 AI）或 `rule`（离线规则模式）
/// - `output_path`：输出文件路径
pub fn run_auto_generate(
    crate_name: &str,
    lang: &str,
    provider: &str,
    output_path: &Path,
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

    println!("{}", ui.f("mapping_extracting", &[crate_name]));
    let doc_jsons = extract_crate_doc(crate_name)?;
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
                        if let Some(explanation) = ai_explanations.get(&chinese_name)
                            && !explanation.is_empty()
                        {
                            explanation_table.insert(chinese_name, explanation.clone());
                        }
                    }
                    println!("{}", ui.f("mapping_ai_success", &[provider]));
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
        &entries,
        &chinese_name_table,
        &explanation_table,
    );

    // 4. 冲突检测：词法转译（关键字映射）先于别名替换执行，冲突的中文名生成后不会生效
    let lang_pack_dir = PathBuf::from(format!("lang-packs/{}", lang));
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

// ==================== rustdoc JSON 提取 ====================

/// 解析 rustdoc JSON，提取所有公开 API（仅名称与签名，绝不读取 docs 字段）
pub fn extract_public_api(json_text: &str) -> anyhow::Result<Vec<ApiEntry>> {
    Ok(extract_public_api_with_glob_sources(json_text)?.0)
}

/// 同 [`extract_public_api`]，额外返回 glob 重导出（`pub use 依赖::*`）指向的
/// 依赖 crate 名列表：薄壳 crate（meta crate）的公开 API 全部来自这些重导出，
/// 调用方需对这些依赖 crate 单独生成 rustdoc JSON 并合并提取
fn extract_public_api_with_glob_sources(
    json_text: &str,
) -> anyhow::Result<(Vec<ApiEntry>, Vec<String>)> {
    let doc: Value = serde_json::from_str(json_text).map_err(|e| {
        anyhow!(
            "{}",
            crate::ui::Ui::global().f("mg_err_parse_rustdoc", &[&e.to_string()])
        )
    })?;
    let index = doc
        .get("index")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("{}", crate::ui::Ui::global().t("mg_err_no_index")))?;
    let root = doc
        .get("root")
        .map(|v| {
            v.as_str()
                .map(String::from)
                .unwrap_or_else(|| v.to_string())
        })
        .unwrap_or_else(|| "0".to_string());
    let mut results = Vec::new();
    let mut glob_sources = Vec::new();
    let mut visited = HashSet::new();
    collect_module_items(index, &root, &mut visited, &mut results, &mut glob_sources);
    Ok((results, glob_sources))
}

/// 深度优先收集模块树中的公开项（跳过 impl 方法、私有项、struct 字段等）
///
/// `glob_sources` 收集 glob 重导出指向的依赖 crate 名，供薄壳 crate 追踪。
fn collect_module_items(
    index: &Map<String, Value>,
    id: &str,
    visited: &mut HashSet<String>,
    results: &mut Vec<ApiEntry>,
    glob_sources: &mut Vec<String>,
) {
    if !visited.insert(id.to_string()) {
        return;
    }
    let Some(item) = index.get(id) else { return };
    if item.get("visibility").and_then(Value::as_str) != Some("public") {
        return;
    }
    let Some((key, inner)) = item
        .get("inner")
        .and_then(Value::as_object)
        .and_then(|o| o.iter().next())
    else {
        return;
    };
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    match key.as_str() {
        "module" => {
            if let Some(items) = inner.get("items").and_then(Value::as_array) {
                for child in items {
                    collect_module_items(index, &child.to_string(), visited, results, glob_sources);
                }
            }
        }
        "function" => {
            results.push(ApiEntry {
                kind: ApiKind::Function,
                english_name: name.clone(),
                signature: render_function_signature(&name, item),
            });
        }
        "struct" => {
            let generics = render_generics(inner.get("generics").unwrap_or(&Value::Null));
            results.push(ApiEntry {
                kind: ApiKind::Struct,
                english_name: name.clone(),
                signature: format!("struct {}{}", name, generics),
            });
        }
        "enum" => {
            let generics = render_generics(inner.get("generics").unwrap_or(&Value::Null));
            results.push(ApiEntry {
                kind: ApiKind::Enum,
                english_name: name.clone(),
                signature: format!("enum {}{}", name, generics),
            });
        }
        "trait" => {
            let generics = render_generics(inner.get("generics").unwrap_or(&Value::Null));
            results.push(ApiEntry {
                kind: ApiKind::Trait,
                english_name: name.clone(),
                signature: format!("trait {}{}", name, generics),
            });
        }
        "type_alias" => {
            let ty = render_type(inner.get("type").unwrap_or(&Value::Null));
            results.push(ApiEntry {
                kind: ApiKind::TypeAlias,
                english_name: name.clone(),
                signature: format!("type {} = {}", name, ty),
            });
        }
        "constant" => {
            let ty = render_type(inner.get("type").unwrap_or(&Value::Null));
            results.push(ApiEntry {
                kind: ApiKind::Const,
                english_name: name.clone(),
                signature: format!("const {}: {}", name, ty),
            });
        }
        // rustdoc JSON 当前工具链不输出 macro_rules! 宏（官方格式限制），预留支持
        "macro" => {
            results.push(ApiEntry {
                kind: ApiKind::Macro,
                english_name: name.clone(),
                signature: format!("{}!", name),
            });
        }
        // re-export（pub use）：薄壳 crate（如 serde 1.0.229 = serde_core 的转发层）
        // 的公开 API 全部是 re-export。名称在 inner.use.name（顶层 name 为 null）。
        "use" => {
            let use_obj = inner; // inner 即 use 对象（inner 单键解构结果）
            if use_obj
                .get("is_glob")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                // glob 重导出（pub use 依赖::*）：外部 crate 的项不在本 JSON 的
                // index 中，无法枚举名称；记录 source 路径的首段（被重导出的
                // crate 名，如 `salvo_core::prelude` → `salvo_core`），由调用方
                // 对该依赖 crate 单独生成文档后合并提取（薄壳 crate 追踪）。
                // crate/self/super 开头的内部 glob 无法枚举，跳过。
                if let Some(source) = use_obj.get("source").and_then(Value::as_str) {
                    let source = source.trim_start_matches("::");
                    let dep_name = source.split("::").next().unwrap_or("");
                    if !dep_name.is_empty()
                        && dep_name != "crate"
                        && dep_name != "self"
                        && dep_name != "super"
                    {
                        glob_sources.push(dep_name.to_string());
                    }
                }
                return;
            }
            let Some(english_name) = use_obj.get("name").and_then(Value::as_str) else {
                return;
            };
            // 若指向本 crate 内的模块（pub use crate::模块），递归跟随收集其公开项
            if let Some(target_id) = use_obj.get("id").map(|v| v.to_string())
                && target_id != id
                && index.contains_key(&target_id)
                && index[&target_id]
                    .get("inner")
                    .and_then(|i| i.get("module"))
                    .is_some()
            {
                collect_module_items(index, target_id.as_str(), visited, results, glob_sources);
                return;
            }
            let source = use_obj.get("source").and_then(Value::as_str).unwrap_or("?");
            results.push(ApiEntry {
                kind: ApiKind::TypeAlias,
                english_name: english_name.to_string(),
                signature: format!("type {} = {}", english_name, source),
            });
        }
        // impl / import / struct_field / variant / assoc_type 等跳过
        _ => {}
    }
}

/// 渲染函数签名：`async fn name<T>(a: u32) -> Result<T>`
fn render_function_signature(name: &str, item: &Value) -> String {
    let f = item
        .get("inner")
        .and_then(|i| i.get("function"))
        .unwrap_or(&Value::Null);
    let sig = f.get("sig").unwrap_or(&Value::Null);
    let header = f.get("header").unwrap_or(&Value::Null);
    let mut s = String::new();
    if header
        .get("is_const")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        s.push_str("const ");
    }
    if header
        .get("is_unsafe")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        s.push_str("unsafe ");
    }
    if header
        .get("is_async")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        s.push_str("async ");
    }
    s.push_str("fn ");
    s.push_str(name);
    s.push_str(&render_generics(f.get("generics").unwrap_or(&Value::Null)));
    s.push_str(&render_io(
        sig.get("inputs").unwrap_or(&Value::Null),
        sig.get("output"),
    ));
    s
}

/// 渲染参数列表与返回类型：`(a: u32, b: String) -> Result<T>`
fn render_io(inputs: &Value, output: Option<&Value>) -> String {
    let params: Vec<String> = inputs
        .as_array()
        .map(|list| {
            list.iter()
                .map(|item| {
                    if let Some(pair) = item.as_array() {
                        let name = pair.first().and_then(Value::as_str).unwrap_or("_");
                        let ty = pair.get(1).unwrap_or(&Value::Null);
                        format!("{}: {}", name, render_type(ty))
                    } else {
                        render_type(item)
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let ret = output.map(render_type).filter(|s| s != "()");
    match ret {
        Some(ret) => format!("({}) -> {}", params.join(", "), ret),
        None => format!("({})", params.join(", ")),
    }
}

/// 渲染泛型参数：`<T, U>` 或空串
fn render_generics(generics: &Value) -> String {
    let params: Vec<String> = generics
        .get("params")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|p| {
                    let name = p.get("name").and_then(Value::as_str).unwrap_or("?");
                    if p.get("kind").and_then(|k| k.get("lifetime")).is_some() {
                        if name.starts_with('\'') {
                            name.to_string()
                        } else {
                            format!("'{}'", name)
                        }
                    } else if p.get("kind").and_then(|k| k.get("const")).is_some() {
                        format!("const {}", name)
                    } else {
                        name.to_string()
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    if params.is_empty() {
        String::new()
    } else {
        format!("<{}>", params.join(", "))
    }
}

/// 递归渲染 rustdoc 类型节点为可读签名（不支持的类型显示 `?`）
fn render_type(ty: &Value) -> String {
    if let Some(s) = ty.get("primitive").and_then(Value::as_str) {
        return if s == "unit" {
            "()".to_string()
        } else {
            s.to_string()
        };
    }
    if let Some(s) = ty.get("generic").and_then(Value::as_str) {
        return s.to_string();
    }
    if let Some(rp) = ty.get("resolved_path") {
        let name = rp.get("path").and_then(Value::as_str).unwrap_or("?");
        if let Some(args) = rp.get("args").and_then(|a| a.get("angle_bracketed")) {
            let params: Vec<String> = args
                .get("args")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .map(|item| {
                            if let Some(t) = item.get("type") {
                                render_type(t)
                            } else if let Some(l) = item.get("lifetime") {
                                format!("'{}'", l.as_str().unwrap_or("?"))
                            } else {
                                "?".to_string()
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            return if params.is_empty() {
                name.to_string()
            } else {
                format!("{}<{}>", name, params.join(", "))
            };
        }
        return name.to_string();
    }
    if let Some(br) = ty.get("borrowed_ref") {
        let mut s = String::from("&");
        if let Some(l) = br.get("lifetime").and_then(Value::as_str) {
            s.push('\'');
            s.push_str(l);
            s.push(' ');
        }
        if br.get("mutable").and_then(Value::as_bool).unwrap_or(false) {
            s.push_str("mut ");
        }
        s.push_str(&render_type(br.get("type").unwrap_or(&Value::Null)));
        return s;
    }
    if let Some(t) = ty.get("tuple") {
        let elements: Vec<String> = t
            .as_array()
            .map(|list| list.iter().map(render_type).collect())
            .unwrap_or_default();
        return format!("({})", elements.join(", "));
    }
    if let Some(sl) = ty.get("slice") {
        return format!("[{}]", render_type(sl.get("type").unwrap_or(&Value::Null)));
    }
    if let Some(arr) = ty.get("array") {
        let len = arr
            .get("len")
            .and_then(|l| l.get("expr"))
            .and_then(Value::as_str)
            .unwrap_or("?");
        return format!(
            "[{}; {}]",
            render_type(arr.get("type").unwrap_or(&Value::Null)),
            len
        );
    }
    if let Some(i) = ty.get("impl_trait") {
        return format!("impl {}", render_bounds(i.get("bounds")));
    }
    if let Some(d) = ty.get("dyn_trait") {
        return format!("dyn {}", render_bounds(d.get("bounds")));
    }
    if let Some(q) = ty.get("qualified_path") {
        let type_name = q.get("name").and_then(Value::as_str).unwrap_or("?");
        return format!(
            "{}::{}",
            render_type(q.get("self_type").unwrap_or(&Value::Null)),
            type_name
        );
    }
    if let Some(f) = ty.get("function_pointer")
        && let Some(sig) = f.get("sig")
    {
        return format!("fn{}", render_io(&sig["inputs"], sig.get("output")));
    }
    if let Some(rp) = ty.get("raw_pointer") {
        let mutable = rp.get("mutable").and_then(Value::as_bool).unwrap_or(false);
        let asterisk = if mutable { "*mut " } else { "*const " };
        return format!(
            "{}{}",
            asterisk,
            render_type(rp.get("type").unwrap_or(&Value::Null))
        );
    }
    "?".to_string()
}

/// 渲染 trait 边界列表：`A + B`
fn render_bounds(bounds: Option<&Value>) -> String {
    bounds
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|b| {
                    b.get("trait_bound")
                        .and_then(|tb| tb.get("trait"))
                        .and_then(|t| t.get("resolved_path"))
                        .and_then(|p| p.get("path"))
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join(" + ")
        })
        .unwrap_or_else(|| "?".to_string())
}

// ==================== 工具链：提取 crate 文档 JSON ====================

/// 临时项目目录守卫（Drop 时自动清理）
struct TempProject(PathBuf);

impl TempProject {
    /// 创建临时项目目录
    fn new(crate_name: &str) -> anyhow::Result<Self> {
        let path =
            std::env::temp_dir().join(format!("rzc-mapping-{}-{}", crate_name, std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join("src")).map_err(|e| {
            anyhow::anyhow!(
                "{}",
                crate::ui::Ui::global().f("mg_err_tempdir", &[&e.to_string()])
            )
        })?;
        Ok(TempProject(path))
    }

    /// 获取临时项目路径
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 提取 crate 及其 glob 重导出链上依赖 crate 的 rustdoc JSON 文本列表
///
/// 首个元素为目标 crate 本身；后续元素为被 glob 重导出（`pub use 依赖::*`）
/// 的依赖 crate——薄壳 crate（如 salvo）的公开 API 全部来自这些 crate。
fn extract_crate_doc(crate_name: &str) -> anyhow::Result<Vec<(String, String)>> {
    let temp = TempProject::new(crate_name)?;
    // 1. 临时项目：把目标 crate 作为唯一依赖（* 允许任意已发布版本）
    fs::write(
        temp.path().join("Cargo.toml"),
        format!(
            "[package]\nname = \"rzc-mapping-temp\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n\"{}\" = \"*\"\n\n[workspace]\n",
            crate_name
        ),
    )?;
    fs::write(
        temp.path().join("src/lib.rs"),
        "// 仅供提取依赖 API 的空库\n",
    )?;
    extract_doc_json_internal(&temp, crate_name)
}

/// 工具链核心（对测试开放）：在给定临时项目中定位 crate 并生成其 rustdoc JSON
///
/// 1. `cargo metadata` 定位目标 crate 的源码目录（registry 或本地 path 依赖均可）
/// 2. `cargo build` 编译依赖树，解析每个依赖的 .rlib/.so 路径
/// 3. 手动调用 `rustdoc -Z unstable-options --output-format json` 文档化目标 crate
/// 4. 薄壳 crate（meta crate，如 salvo 仅 `pub use salvo_core::*`）的公开 API
///    来自 glob 重导出：解析每个 crate 的 glob 重导出（inner.use.is_glob）的
///    source 路径，对被重导出的依赖 crate 也生成文档并继续追踪，直至闭环。
///    返回 (crate 名, JSON 文本) 列表，首个为目标 crate 本身。
fn extract_doc_json_internal(
    temp: &TempProject,
    crate_name: &str,
) -> anyhow::Result<Vec<(String, String)>> {
    let project_root = temp.path();
    let ui = crate::ui::Ui::global();

    // 1. metadata：定位目标 crate 的 manifest
    let metadata_output = run_command(
        Command::new("cargo")
            .arg("metadata")
            .arg("--format-version")
            .arg("1")
            .current_dir(project_root),
        &ui.t("mg_cmd_parse_deps"),
    )?;
    let metadata: Value = serde_json::from_str(&metadata_output)
        .map_err(|e| anyhow!("{}", ui.f("mg_err_parse_meta", &[&e.to_string()])))?;
    // 包元数据索引（manifest_path / features / edition），供 glob 重导出链上的
    // 依赖 crate 查询（薄壳 crate 的 API 在其依赖中）。同时索引 package 名
    // （CLI 参数，如 mini-core）与 lib target 的 crate 名（rustdoc glob 重导出
    // source，如 mini_core），两者在连字符/下划线写法上可能不同。
    let mut pkg_index: HashMap<String, Value> = HashMap::new();
    for p in metadata["packages"].as_array().into_iter().flatten() {
        if let Some(pkg_name) = p.get("name").and_then(Value::as_str) {
            pkg_index.insert(pkg_name.to_string(), p.clone());
        }
        if let Some(lib_name) = p
            .get("targets")
            .and_then(Value::as_array)
            .and_then(|ts| {
                ts.iter().find(|t| {
                    t.get("kind")
                        .and_then(Value::as_array)
                        .map(|k| k.iter().any(|v| v.as_str() == Some("lib")))
                        == Some(true)
                })
            })
            .and_then(|t| t.get("name"))
            .and_then(Value::as_str)
        {
            pkg_index.insert(lib_name.to_string(), p.clone());
        }
    }
    if !pkg_index.contains_key(crate_name) {
        bail!("{}", ui.f("mg_err_crate_not_found", &[crate_name]));
    }

    // resolve.nodes：完整依赖解析图（含多版本共存时的精确解析），按
    // package_id 索引每个 crate 的直接依赖（别名, 依赖 package_id）。
    // --extern 只传直接依赖的精确版本；传递依赖由 rustc 按 rlib 元数据
    // hash 在 -L 目录中自动解析（同名多版本 crate 无法从 -L 手动解析）
    let mut direct_dep_table: HashMap<String, Vec<(String, String)>> = HashMap::new();
    if let Some(nodes) = metadata
        .get("resolve")
        .and_then(|r| r.get("nodes"))
        .and_then(Value::as_array)
    {
        for node in nodes {
            let Some(node_id) = node.get("id").and_then(Value::as_str) else {
                continue;
            };
            let mut deps: Vec<(String, String)> = Vec::new();
            if let Some(dep_list) = node.get("deps").and_then(Value::as_array) {
                for dep in dep_list {
                    let Some(dep_name) = dep.get("name").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some(dep_pkg) = dep.get("pkg").and_then(Value::as_str) else {
                        continue;
                    };
                    // 只保留 normal 依赖（dev/build 依赖不进入 lib 的 extern prelude）；
                    // 目标特定依赖（cfg(...)）无法静态判定，保守保留
                    let is_normal = dep
                        .get("dep_kinds")
                        .and_then(Value::as_array)
                        .map(|kinds| {
                            kinds
                                .iter()
                                .any(|k| k.get("kind").is_none_or(|v| v.is_null()))
                        })
                        .unwrap_or(true);
                    if is_normal {
                        deps.push((dep_name.to_string(), dep_pkg.to_string()));
                    }
                }
            }
            direct_dep_table.insert(node_id.to_string(), deps);
        }
    }

    // 2. cargo build：编译依赖树，解析依赖 .rlib/.so 路径
    let build_output = run_command(
        Command::new("cargo")
            .arg("build")
            .arg("--message-format=json")
            .current_dir(project_root),
        &ui.t("mg_cmd_build_deps"),
    )?;
    // package_id -> (lib target 名, .rlib/.so 路径, 实际启用的 features)。
    // 按 package_id 索引而非 target 名：依赖树中同名不同版本的 crate 共存时
    // （如 rand 0.8.7 与 0.10.2），按名字覆盖会链接错误版本；features 取 cargo
    // build 的 feature 统一解析结果而非包声明的 default——依赖方可能未启用部分
    // default features（如 salvo 不启用 salvo_core 的 unix），注入不一致的 cfg
    // 会导致编译失败（缺 nix 等依赖）
    let mut artifact_table: HashMap<String, (String, String, Vec<String>)> = HashMap::new();
    // 已编译 lib 的 crate 名集合（glob 重导出链入队检查用）
    let mut compiled_lib_names: HashSet<String> = HashSet::new();
    // package_id -> 构建脚本产物（见模块级 [`BuildScriptInfo`] 类型注释）
    let mut build_script_table: HashMap<String, BuildScriptInfo> = HashMap::new();
    for line in build_output.lines() {
        let Ok(msg) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(reason) = msg.get("reason").and_then(Value::as_str) else {
            continue;
        };
        if reason != "compiler-artifact" && reason != "build-script-executed" {
            continue;
        }
        let Some(pkg_id) = msg.get("package_id").and_then(Value::as_str) else {
            continue;
        };
        if reason == "build-script-executed" {
            let out_dir = msg.get("out_dir").and_then(Value::as_str).map(String::from);
            let cfgs = msg
                .get("cfgs")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            let env_vars = msg
                .get("env")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            build_script_table.insert(pkg_id.to_string(), (out_dir, cfgs, env_vars));
            continue;
        }
        // compiler-artifact：按 package_id 索引（与 metadata packages[].id 一致），
        // 多版本共存时精确关联到具体版本；target.name 仅作为 lib 名记录
        let Some(target_name) = msg
            .get("target")
            .and_then(|t| t.get("name"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        // 只保留 ASCII 合法标识符的 crate 名（本地 workspace 成员如中文名项目会被排除）
        if target_name.is_empty()
            || target_name == "rzc-mapping-temp"
            || !target_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            continue;
        }
        if let Some(filenames) = msg.get("filenames").and_then(Value::as_array)
            && let Some(file) = filenames
                .iter()
                .filter_map(Value::as_str)
                .find(|f| f.ends_with(".rlib") || f.ends_with(".so"))
        {
            let features = msg
                .get("features")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            artifact_table.insert(
                pkg_id.to_string(),
                (target_name.to_string(), file.to_string(), features),
            );
            compiled_lib_names.insert(target_name.to_string());
        }
    }
    if artifact_table.is_empty() {
        bail!("{}", ui.f("mg_err_build_failed", &[crate_name]));
    }

    // 3. rustdoc 队列：目标 crate → glob 重导出链上的依赖 crate。
    //    薄壳 crate（如 salvo）的 index 只有 re-export 节点，公开 API 全部
    //    来自被重导出的依赖 crate（如 salvo_core），逐个生成文档后合并提取。
    let mut queue: Vec<String> = vec![crate_name.to_string()];
    let mut visited: HashSet<String> = HashSet::new();
    let mut results: Vec<(String, String)> = Vec::new();
    while let Some(name) = queue.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let Some(pkg) = pkg_index.get(&name) else {
            continue;
        };
        let pkg_id = pkg
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        // 直接依赖（别名, 精确版本 package_id）：--extern 只传这些
        let direct_deps = direct_dep_table.get(&pkg_id).cloned().unwrap_or_default();
        let json_text = rustdoc_single(
            project_root,
            pkg,
            &name,
            &pkg_id,
            &artifact_table,
            &direct_deps,
            &build_script_table,
        )?;
        // glob 重导出（pub use 依赖::*）→ 被重导出 crate 名（source 首段），
        // 若在依赖树中则入队继续提取；feature 未启用而未编译的依赖自动跳过
        let (_, glob_sources) = extract_public_api_with_glob_sources(&json_text)?;
        for source in glob_sources {
            if compiled_lib_names.contains(&source) && !visited.contains(&source) {
                queue.push(source);
            }
        }
        results.push((name, json_text));
    }
    Ok(results)
}

/// 构建脚本产物：OUT_DIR（include! 生成代码）、cfg（条件编译）、rustc-env 列表。
/// 部分 crate（如 serde）的 build.rs 会生成 include! 的源码或声明 cfg，
/// 手动 rustdoc 时必须注入，否则编译失败（如 OUT_DIR 未定义）。
type BuildScriptInfo = (Option<String>, Vec<String>, Vec<String>);

/// 对单个 crate 手动调用 rustdoc 生成 JSON 文档
///
/// 注入目标 crate 实际启用的 features（cfg）、构建脚本产物（OUT_DIR / cfg /
/// rustc-env）与直接依赖的 .rlib/.so 路径（--extern，按 package_id 精确版本），
/// 保证 cfg(feature) 与 include! 生成的 API 不缺失。
fn rustdoc_single(
    project_root: &Path,
    pkg: &Value,
    crate_name: &str,
    pkg_id: &str,
    artifact_table: &HashMap<String, (String, String, Vec<String>)>,
    direct_deps: &[(String, String)],
    build_script_table: &HashMap<String, BuildScriptInfo>,
) -> anyhow::Result<String> {
    let ui = crate::ui::Ui::global();
    let manifest_path = PathBuf::from(
        pkg.get("manifest_path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("{}", ui.f("mg_err_no_manifest", &[crate_name])))?,
    );
    let source_dir = manifest_path
        .parent()
        .ok_or_else(|| anyhow!("{}", ui.f("mg_err_no_src_dir", &[crate_name])))?;
    let lib_file = source_dir.join("src/lib.rs");
    if !lib_file.exists() {
        bail!("{}", ui.f("mg_err_no_lib", &[crate_name]));
    }
    // 默认 features：cargo 直接传给 rustc（--cfg feature=...），不经过 build script，
    // 手动 rustdoc 时必须显式补传，否则 cfg(feature) 裁掉的 API 会缺失
    let default_features = pkg
        .get("features")
        .and_then(|f| f.get("default"))
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let manifest_content = fs::read_to_string(&manifest_path).with_context(|| {
        ui.f(
            "mg_err_read_manifest",
            &[&manifest_path.display().to_string()],
        )
    })?;
    let manifest: toml::Value = toml::from_str(&manifest_content).map_err(|e| {
        anyhow!(
            "{}",
            ui.f(
                "mg_err_parse_manifest",
                &[&manifest_path.display().to_string(), &e.to_string()]
            )
        )
    })?;
    let edition = manifest
        .get("package")
        .and_then(|p| p.get("edition"))
        .and_then(toml::Value::as_str)
        .unwrap_or("2015")
        .to_string();
    let proc_macro = manifest
        .get("lib")
        .and_then(|l| l.get("proc-macro"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);

    let crate_name_underscore = crate_name.replace('-', "_");
    let json_dir = project_root.join("mapping-json");
    fs::create_dir_all(&json_dir)?;
    let mut cmd = Command::new("rustdoc");
    cmd.arg(&lib_file)
        .arg("--crate-name")
        .arg(&crate_name_underscore)
        .arg("--crate-type")
        .arg(if proc_macro { "proc-macro" } else { "lib" })
        .arg("--edition")
        .arg(&edition)
        .arg("-L")
        .arg(format!(
            "dependency={}",
            project_root.join("target/debug/deps").display()
        ))
        .env("RUSTC_BOOTSTRAP", "1");
    // --extern 只传直接依赖的精确版本：同名多版本 crate（如 rand 0.8.7 与
    // 0.10.2 共存）无法从 -L 目录自动解析，rustc 可能链接错误版本；传递依赖
    // 由 rustc 按 rlib 元数据 hash 在 -L 中自动解析
    for (dep_name, dep_pkg_id) in direct_deps {
        if let Some((_, path, _)) = artifact_table.get(dep_pkg_id) {
            cmd.arg("--extern").arg(format!("{}={}", dep_name, path));
        }
    }
    // 注入 cargo build 实际启用的 features（feature 统一解析结果）作为 cfg，
    // 与编译产物保持一致；未命中时回退包声明的 default features
    let features = artifact_table
        .get(pkg_id)
        .map(|(_, _, f)| f.clone())
        .unwrap_or(default_features);
    for feature in &features {
        cmd.arg("--cfg").arg(format!("feature=\"{}\"", feature));
    }
    // 注入目标 crate 构建脚本产物：OUT_DIR（include! 生成代码）、cfg（条件编译）、rustc-env
    if let Some((out_dir, cfgs, env_vars)) = build_script_table.get(pkg_id) {
        if let Some(dir) = out_dir {
            cmd.env("OUT_DIR", dir);
        }
        for cfg in cfgs {
            cmd.arg("--cfg").arg(cfg);
        }
        for item in env_vars {
            if let Some((key, value)) = item.split_once('=') {
                cmd.env(key, value);
            }
        }
    }
    cmd.arg("-Z")
        .arg("unstable-options")
        .arg("--output-format")
        .arg("json")
        .arg("--output")
        .arg(&json_dir);
    run_command(&mut cmd, &ui.t("mg_cmd_gen_doc"))?;

    let json_path = json_dir.join(format!("{}.json", crate_name_underscore));
    fs::read_to_string(&json_path)
        .with_context(|| ui.f("mg_err_no_doc_json", &[&json_path.display().to_string()]))
}

/// 运行命令并返回 stdout；失败时附加 stderr 摘要
const ERROR_SUMMARY_LINES: usize = 15;
fn run_command(cmd: &mut Command, description: &str) -> anyhow::Result<String> {
    let output = cmd
        .output()
        .with_context(|| crate::ui::Ui::global().f("mg_err_run_failed", &[description]))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let summary = stderr
            .lines()
            .take(ERROR_SUMMARY_LINES)
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "{}",
            crate::ui::Ui::global().f("mg_err_failed", &[description, &summary])
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// ==================== 规则驱动：中文名生成 ====================

/// 检测中文名与语言包关键字映射的冲突。
///
/// 引擎的词法转译（关键字映射）先于别名替换执行，因此若生成的中文名
/// 在 keywords.toml 中已映射为不同英文（如 `错误` → `Err`），则本映射不会生效。
/// 返回冲突列表：(中文名, 关键字映射的英文, 本映射的英文)；语言包目录不存在时返回空。
pub fn detect_keyword_conflicts(
    lang_pack_dir: &Path,
    chinese_name_table: &[(String, String)],
) -> Vec<(String, String, String)> {
    let Ok(content) = fs::read_to_string(lang_pack_dir.join("keywords.toml")) else {
        return Vec::new();
    };
    let Ok(toml::Value::Table(table)) = toml::from_str::<toml::Value>(&content) else {
        return Vec::new();
    };
    let mut conflicts = Vec::new();
    for (chinese, english) in chinese_name_table {
        // 遍历关键字表所有节（声明/控制流/类型/错误处理…）
        for (_, section) in &table {
            let toml::Value::Table(entry) = section else {
                continue;
            };
            let Some(toml::Value::String(keyword_english)) = entry.get(chinese) else {
                continue;
            };
            if keyword_english != english {
                conflicts.push((chinese.clone(), keyword_english.clone(), english.clone()));
            }
            break;
        }
    }
    conflicts
}

/// 整名特例（优先于一切规则，保证常见 API 的中文名准确直观）
const WHOLE_NAME_EXCEPTIONS: &[(&str, &str)] = &[
    ("new", "新建"),
    ("from_str", "从字符串解析"),
    ("from_bytes", "从字节解析"),
    ("to_string", "转字符串"),
    ("to_vec", "转字节向量"),
    ("as_str", "作为字符串"),
    ("is_empty", "是否为空"),
    ("is_some", "是否有值"),
    ("is_none", "是否无值"),
    ("is_ok", "是否成功"),
    ("is_err", "是否出错"),
    ("unwrap", "解包"),
    ("expect", "期望"),
    ("default", "默认值"),
    ("clone", "克隆"),
    ("parse", "解析"),
    ("len", "长度"),
    ("capacity", "容量"),
    ("clear", "清空"),
    ("push", "压入"),
    ("pop", "弹出"),
    ("insert", "插入"),
    ("remove", "移除"),
    ("contains", "包含"),
    ("find", "查找"),
    ("sort", "排序"),
    ("iter", "迭代"),
    ("iter_mut", "可变迭代"),
    ("into_iter", "转迭代器"),
    ("map", "映射"),
    ("filter", "过滤"),
    ("collect", "收集"),
    ("join", "拼接"),
    ("split", "分割"),
    ("trim", "去除空白"),
    ("hash", "哈希"),
    ("to_owned", "转自有"),
    ("to_lowercase", "转小写"),
    ("to_uppercase", "转大写"),
    ("main", "主函数"),
    ("println", "打印行"),
];

/// 前缀规则（按长度降序匹配，`get_xxx` → 获取xxx）
const PREFIX_RULES: &[(&str, &str)] = &[
    ("from_str", "从字符串解析"),
    ("to_string", "转字符串"),
    ("is_empty", "是否为空"),
    ("is_some", "是否有值"),
    ("is_none", "是否无值"),
    ("is_ok", "是否成功"),
    ("is_err", "是否出错"),
    ("to_vec", "转字节向量"),
    ("try_", "尝试"),
    ("get_", "获取"),
    ("set_", "设置"),
    ("is_", "是否"),
    ("has_", "是否有"),
    ("to_", "转为"),
    ("as_", "作为"),
    ("from_", "从"),
    ("into_", "转为"),
    ("parse_", "解析"),
    ("with_", "设置"),
    ("create_", "创建"),
    ("build_", "构建"),
    ("add_", "添加"),
    ("remove_", "移除"),
    ("update_", "更新"),
    ("delete_", "删除"),
    ("read_", "读取"),
    ("write_", "写入"),
    ("load_", "加载"),
    ("save_", "保存"),
    ("send_", "发送"),
    ("receive_", "接收"),
    ("encode_", "编码"),
    ("decode_", "解码"),
    ("connect_", "连接"),
    ("listen_", "监听"),
];

/// 英文单词 → 中文（用于类型名与函数名的拆词翻译）
const WORD_TABLE: &[(&str, &str)] = &[
    ("anyhow", "任意错误"),
    ("serde", "序列化"),
    ("serde_json", "JSON序列化"),
    ("error", "错误"),
    ("errors", "错误列表"),
    ("result", "结果"),
    ("option", "选项"),
    ("builder", "构建器"),
    ("config", "配置"),
    ("configuration", "配置"),
    ("settings", "设置"),
    ("client", "客户端"),
    ("server", "服务器"),
    ("request", "请求"),
    ("response", "响应"),
    ("handler", "处理器"),
    ("parser", "解析器"),
    ("reader", "读取器"),
    ("writer", "写入器"),
    ("iterator", "迭代器"),
    ("mapping", "映射"),
    ("map", "映射"),
    ("set", "集合"),
    ("list", "列表"),
    ("vector", "向量"),
    ("vec", "向量"),
    ("string", "字符串"),
    ("str", "字符串"),
    ("char", "字符"),
    ("bytes", "字节"),
    ("number", "数字"),
    ("integer", "整数"),
    ("float", "浮点数"),
    ("boolean", "布尔值"),
    ("bool", "布尔值"),
    ("time", "时间"),
    ("date", "日期"),
    ("duration", "时长"),
    ("thread", "线程"),
    ("task", "任务"),
    ("event", "事件"),
    ("callback", "回调"),
    ("factory", "工厂"),
    ("manager", "管理器"),
    ("service", "服务"),
    ("protocol", "协议"),
    ("format", "格式"),
    ("version", "版本"),
    ("path", "路径"),
    ("file", "文件"),
    ("directory", "目录"),
    ("folder", "文件夹"),
    ("network", "网络"),
    ("connection", "连接"),
    ("stream", "流"),
    ("buffer", "缓冲区"),
    ("cache", "缓存"),
    ("context", "上下文"),
    ("metadata", "元数据"),
    ("parameter", "参数"),
    ("arguments", "参数"),
    ("arg", "参数"),
    ("options", "选项"),
    ("default", "默认"),
    ("info", "信息"),
    ("message", "消息"),
    ("data", "数据"),
    ("value", "值"),
    ("key", "键"),
    ("token", "令牌"),
    ("session", "会话"),
    ("user", "用户"),
    ("name", "名称"),
    ("item", "条目"),
    ("entry", "条目"),
    ("state", "状态"),
    ("status", "状态"),
    ("code", "代码"),
    ("kind", "类型"),
    ("type", "类型"),
    ("id", "标识"),
    ("url", "网址"),
    ("uri", "资源标识"),
    ("serialize", "序列化"),
    ("deserialize", "反序列化"),
    ("serializer", "序列化器"),
    ("deserializer", "反序列化器"),
    ("serialization", "序列化"),
    ("async", "异步"),
    ("await", "等待"),
    ("future", "未来值"),
    ("runtime", "运行时"),
    ("logger", "日志器"),
    ("log", "日志"),
    ("json", "JSON"),
    ("xml", "XML"),
    ("yaml", "YAML"),
    ("toml", "TOML"),
    ("http", "HTTP"),
    ("https", "HTTPS"),
    ("address", "地址"),
    ("socket", "套接字"),
    ("port", "端口"),
    ("query", "查询"),
    ("behavior", "行为"),
    ("max", "最大"),
    ("bail", "立即报错"),
    ("ensure", "确保"),
    ("panic", "恐慌"),
    ("abort", "中止"),
    ("header", "头部"),
    ("body", "主体"),
    ("method", "方法"),
    ("function", "函数"),
    ("module", "模块"),
    ("trait", "特征"),
    ("enum", "枚举"),
    ("struct", "结构体"),
    ("macro", "宏"),
    ("constant", "常量"),
    ("variable", "变量"),
    ("mut", "可变"),
    ("const", "常量"),
    ("static", "静态"),
    ("pub", "公开"),
    ("private", "私有"),
    ("public", "公开"),
    ("import", "导入"),
    ("export", "导出"),
    ("new", "新建"),
    ("create", "创建"),
    ("build", "构建"),
    ("add", "添加"),
    ("remove", "移除"),
    ("delete", "删除"),
    ("update", "更新"),
    ("get", "获取"),
    ("set", "设置"),
    ("open", "打开"),
    ("close", "关闭"),
    ("read", "读取"),
    ("write", "写入"),
    ("load", "加载"),
    ("save", "保存"),
    ("send", "发送"),
    ("receive", "接收"),
    ("run", "运行"),
    ("start", "启动"),
    ("stop", "停止"),
    ("serialize", "序列化"),
    ("parse", "解析"),
    ("encode", "编码"),
    ("decode", "解码"),
    ("connect", "连接"),
    ("disconnect", "断开"),
    ("listen", "监听"),
    ("bind", "绑定"),
    ("print", "打印"),
    ("assert", "断言"),
    ("wait", "等待"),
    ("sleep", "休眠"),
    ("count", "数量"),
    ("size", "大小"),
    ("length", "长度"),
    ("capacity", "容量"),
    ("first", "首个"),
    ("last", "末尾"),
    ("next", "下一个"),
    ("current", "当前"),
    ("all", "全部"),
    ("any", "任意"),
    ("some", "某些"),
    ("none", "无"),
    ("ok", "成功"),
    ("yes", "是"),
    ("no", "否"),
    ("true", "真"),
    ("false", "假"),
    ("left", "左"),
    ("right", "右"),
    ("top", "顶部"),
    ("bottom", "底部"),
    ("begin", "开始"),
    ("end", "结束"),
    ("in", "进入"),
    ("out", "输出"),
    ("up", "上"),
    ("down", "下"),
    ("push", "压入"),
    ("pop", "弹出"),
    ("insert", "插入"),
    ("clear", "清空"),
    ("contains", "包含"),
    ("find", "查找"),
    ("search", "搜索"),
    ("sort", "排序"),
    ("iter", "迭代"),
    ("map", "映射"),
    ("filter", "过滤"),
    ("collect", "收集"),
    ("join", "拼接"),
    ("split", "分割"),
    ("trim", "去除空白"),
    ("hash", "哈希"),
    ("sign", "签名"),
    ("verify", "验证"),
    ("validate", "校验"),
    ("auth", "认证"),
    ("login", "登录"),
    ("logout", "登出"),
    ("secret", "密钥"),
    ("cert", "证书"),
    ("lock", "锁"),
    ("unlock", "解锁"),
    ("queue", "队列"),
    ("stack", "栈"),
    ("tree", "树"),
    ("node", "节点"),
    ("graph", "图"),
    ("edge", "边"),
    ("pair", "对"),
    ("group", "组"),
    ("record", "记录"),
    ("table", "表"),
    ("row", "行"),
    ("column", "列"),
    ("field", "字段"),
    ("index", "索引"),
    ("position", "位置"),
    ("location", "位置"),
    ("source", "源"),
    ("target", "目标"),
    ("destination", "目的地"),
    ("input", "输入"),
    ("output", "输出"),
    ("internal", "内部"),
    ("external", "外部"),
    ("local", "本地"),
    ("remote", "远程"),
    ("global", "全局"),
    ("single", "单个"),
    ("multiple", "多个"),
    ("simple", "简单"),
    ("complex", "复杂"),
    ("unknown", "未知"),
    ("invalid", "无效"),
    ("valid", "有效"),
    ("empty", "空"),
    ("full", "满"),
    ("enabled", "启用"),
    ("disabled", "禁用"),
    ("active", "活跃"),
    ("inactive", "不活跃"),
    ("success", "成功"),
    ("failure", "失败"),
    ("warning", "警告"),
    ("danger", "危险"),
    ("secure", "安全"),
    ("unsecure", "不安全"),
    ("unique", "唯一"),
    ("common", "通用"),
    ("special", "特殊"),
    ("normal", "普通"),
    ("standard", "标准"),
    ("advanced", "高级"),
    ("basic", "基础"),
    ("primary", "主要"),
    ("secondary", "次要"),
    ("optional", "可选"),
    ("required", "必需"),
    ("mandatory", "必需"),
    ("generic", "泛型"),
    ("dynamic", "动态"),
    ("fixed", "固定"),
    ("random", "随机"),
];

/// crate 名 → 中文名特例表（常见 crate，保证输出准确）
const CRATE_NAME_EXCEPTIONS: &[(&str, &str)] = &[
    ("anyhow", "任意错误"),
    ("serde", "序列化"),
    ("serde_json", "JSON序列化"),
    ("serde_derive", "序列化推导"),
    ("toml", "TOML处理"),
    ("clap", "命令行解析"),
    ("tokio", "异步运行时"),
    ("thiserror", "错误推导"),
    ("log", "日志"),
    ("env_logger", "日志初始化"),
    ("tracing", "追踪日志"),
    ("reqwest", "HTTP客户端"),
    ("ureq", "轻量HTTP客户端"),
    ("rand", "随机数"),
    ("chrono", "日期时间"),
    ("regex", "正则表达式"),
    ("itertools", "迭代器工具"),
    ("rayon", "并行计算"),
    ("nom", "解析组合子"),
    ("syn", "语法树解析"),
    ("quote", "代码生成"),
    ("proc_macro2", "过程宏"),
    ("csv", "CSV处理"),
    ("html", "HTML处理"),
    ("url", "网址处理"),
    ("uuid", "UUID标识"),
    ("glob", "通配符匹配"),
    ("walkdir", "目录遍历"),
    ("zip", "ZIP压缩"),
    ("tar", "TAR归档"),
    ("flate2", "压缩解压"),
    ("sha2", "SHA2哈希"),
    ("hex", "十六进制"),
    ("base64", "Base64编码"),
    ("percent_encoding", "百分号编码"),
];

/// 按目标语言生成 crate 名称：zh 用特例表/拆词翻译，其他语言保留英文原名
pub fn generate_crate_localized_name(lang: &str, crate_name: &str) -> String {
    if lang == "zh" {
        generate_crate_chinese_name(crate_name)
    } else {
        crate_name.to_string()
    }
}

/// 生成 crate 的中文名（特例表优先，未命中拆词翻译，仍未知则保留原名）
pub fn generate_crate_chinese_name(crate_name: &str) -> String {
    if let Some((_, chinese)) = CRATE_NAME_EXCEPTIONS
        .iter()
        .find(|(en, _)| *en == crate_name)
    {
        return chinese.to_string();
    }
    let word_seq = split_words(crate_name);
    if word_seq.is_empty() {
        return crate_name.to_string();
    }
    let translations: Vec<String> = word_seq
        .iter()
        .map(|word| {
            WORD_TABLE
                .iter()
                .find(|(en, _)| *en == word)
                .map(|(_, zh)| zh.to_string())
                .unwrap_or_else(|| word.clone())
        })
        .collect();
    let result = translations.concat();
    if result == crate_name {
        crate_name.to_string()
    } else {
        result
    }
}

/// 按目标语言生成 API 名称：zh 走规则生成中文名，其他语言保留英文原名
pub fn rule_generate_localized_name(lang: &str, english_name: &str) -> String {
    if lang == "zh" {
        rule_generate_chinese_name(english_name)
    } else {
        english_name.to_string()
    }
}

/// 规则驱动：根据英文名生成中文名（整名特例 → 前缀规则 → 拆词翻译 → 原名）
pub fn rule_generate_chinese_name(english_name: &str) -> String {
    // 1. 整名特例
    if let Some((_, chinese)) = WHOLE_NAME_EXCEPTIONS
        .iter()
        .find(|(en, _)| *en == english_name)
    {
        return chinese.to_string();
    }
    // 2. 前缀规则（按长度降序匹配，如 get_value → 获取值）
    let mut prefix_candidates: Vec<&(&str, &str)> = PREFIX_RULES
        .iter()
        .filter(|(prefix, _)| english_name.starts_with(prefix) && english_name.len() > prefix.len())
        .collect();
    prefix_candidates.sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));
    if let Some((prefix, chinese)) = prefix_candidates.first() {
        let remainder = &english_name[prefix.len()..];
        return format!("{}{}", chinese, translate_words(remainder));
    }
    // 3. 拆词翻译（如 SerializeValue → 序列化值）
    let translation = translate_words(english_name);
    if !translation.is_empty() {
        return translation;
    }
    // 4. 兆底：保留原名
    english_name.to_string()
}

/// 把标识符拆成小写单词序列（支持 snake_case / 驼峰 / 连字符）
fn split_words(identifier: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut prev_was_upper = false;
    for c in identifier.chars() {
        if c == '_' || c == '-' {
            if !current.is_empty() {
                words.push(current.clone());
                current.clear();
            }
            prev_was_upper = false;
            continue;
        }
        if c.is_ascii_uppercase() {
            if !current.is_empty() && !prev_was_upper {
                words.push(current.clone());
                current.clear();
            }
            current.push(c.to_ascii_lowercase());
            prev_was_upper = true;
        } else {
            current.push(c);
            prev_was_upper = false;
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// 拆词后逐词查词表翻译并拼接
fn translate_words(english_name: &str) -> String {
    let word_seq = split_words(english_name);
    if word_seq.is_empty() {
        return String::new();
    }
    let all_known = word_seq
        .iter()
        .all(|w| WORD_TABLE.iter().any(|(en, _)| en == w));
    if !all_known {
        return String::new();
    }
    word_seq
        .iter()
        .map(|w| {
            WORD_TABLE
                .iter()
                .find(|(en, _)| en == w)
                .map(|(_, zh)| *zh)
                .unwrap_or(w)
        })
        .collect()
}

// ==================== TOML 输出 ====================

/// TOML 字符串转义
fn escape_toml(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

/// 当前时间字符串（UTC，`YYYY-MM-DD HH:MM:SS`）
fn current_timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (secs / 86_400) as i64;
    let day_secs = secs % 86_400;
    let (year, month, day) = decompose_date(days);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year,
        month,
        day,
        day_secs / 3_600,
        (day_secs % 3_600) / 60,
        day_secs % 60
    )
}

/// 天数 → (年, 月, 日)（civil_from_days 算法，与引擎 logger.rs 一致）
fn decompose_date(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_of_year = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_of_year + 2) / 5 + 1) as u32;
    let month = (month_of_year + if month_of_year < 10 { 3 } else { -9 }) as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

/// 构建语言包映射 TOML（与现有 crates/*.toml 格式一致：
/// `["模块路径"]` / `["标识符"]` 两节为映射管理器识别的 String 值格式，
/// `["解释"]` 节为扩展，映射管理器加载时自动忽略，保持兼容）
pub fn build_mapping_toml(
    lang: &str,
    crate_name: &str,
    crate_chinese_name: &str,
    entries: &[ApiEntry],
    chinese_name_table: &[(String, String)],
    explanation_table: &HashMap<String, String>,
) -> String {
    let ui = crate::ui::Ui::for_lang(lang);
    let mut output = String::new();
    output.push_str(&format!("{}\n", ui.t("mg_header_title")));
    output.push_str(&format!("{}\n", ui.f("mg_header_crate", &[crate_name])));
    output.push_str(&format!(
        "{}\n",
        ui.f("mg_header_generated_at", &[&current_timestamp()])
    ));
    output.push_str(&format!("# {}\n\n", disclaimer_text(lang)));

    output.push_str("[\"模块路径\"]\n");
    output.push_str(&format!(
        "\"{}\" = \"{}\"\n\n",
        escape_toml(crate_chinese_name),
        escape_toml(crate_name)
    ));

    output.push_str("[\"标识符\"]\n");
    // 预构建英文名→签名索引，避免 O(n²) 线性查找
    let sig_index: HashMap<&str, &str> = entries
        .iter()
        .map(|e| (e.english_name.as_str(), e.signature.as_str()))
        .collect();
    for (chinese_name, english_name) in chinese_name_table {
        let sig = sig_index.get(english_name.as_str()).copied().unwrap_or("");
        output.push_str(&format!(
            "\"{}\" = \"{}\"  # {}\n",
            escape_toml(chinese_name),
            escape_toml(english_name),
            sig
        ));
    }
    output.push('\n');

    output.push_str("[\"解释\"]\n");
    for (chinese_name, _) in chinese_name_table {
        let explanation = explanation_table
            .get(chinese_name)
            .map(String::as_str)
            .unwrap_or("");
        output.push_str(&format!(
            "\"{}\" = \"{}\"\n",
            escape_toml(chinese_name),
            escape_toml(explanation)
        ));
    }
    output
}

// ==================== AI 驱动（DeepSeek，OpenAI 兼容接口） ====================

/// AI 请求超时配置（秒）
const AI_CONNECT_TIMEOUT: u64 = 30;
/// AI system 提示语（中文）：面向 zh 语言包，生成中文名 + 中文解释
const AI_PROMPT_ZH: &str = "你是面向 Rust 新手的教学翻译专家。任务：把第三方 crate 的公开 API 翻译成中文教学映射。\n\
    输入：API 英文名 + 类型签名列表。你只能依据名称和类型签名推测含义，\n\
    禁止使用、翻译或复制任何官方文档内容。\n\
    输出：严格 TOML 格式，仅两个节：\n\
    [\"标识符\"]\n\"中文名\" = \"英文名\"\n\
    [\"解释\"]\n\"中文名\" = \"一句不超过 40 字的大白话解释\"\n\
    要求：中文名直观好记；解释说明这个 API 是干什么的、怎么用，面向新手；\n\
    不能确定的条目省略；只输出 TOML，不要输出任何其他文字。";
/// AI system 提示语（英文）：面向非 zh 语言包，名称保持英文，生成英文新手解释
/// （TOML 节名必须保持 \"标识符\" / \"解释\"，与解析器及映射文件格式兼容）
const AI_PROMPT_EN: &str = "You are a teaching translation expert for Rust beginners. Task: produce a teaching mapping for the public API of a third-party crate.\n\
    Input: a list of API English names + type signatures. Infer meaning from names and signatures only;\n\
    never use, translate, or copy any official documentation content.\n\
    Output: strict TOML with exactly two sections:\n\
    [\"标识符\"]\n\"name\" = \"EnglishName\" (keep the names as-is; only include entries you are sure about)\n\
    [\"解释\"]\n\"name\" = \"one plain-language explanation under 40 words for beginners\"\n\
    Requirements: the explanation must say what the API does and how to use it, in simple English;\n\
    omit entries you cannot determine; output only TOML and nothing else.";
const AI_READ_TIMEOUT: u64 = 120;
const AI_WRITE_TIMEOUT: u64 = 60;

/// 调用 DeepSeek chat 接口的通用底层：发送 system+user 提示词，返回模型文本
///
/// 供映射生成（call_ai_generate_mapping）与脚手架翻译
/// （mapping_check::run_scaffold --provider deepseek）共用。
pub fn deepseek_chat(system_prompt: &str, user_prompt: &str) -> anyhow::Result<String> {
    let ui = crate::ui::Ui::global();
    let api_key =
        std::env::var("DEEPSEEK_API_KEY").map_err(|_| anyhow!("{}", ui.t("mg_err_no_api_key")))?;
    if api_key.is_empty() {
        bail!("{}", ui.t("mg_err_api_key_empty"));
    }
    let base_url = std::env::var("DEEPSEEK_BASE_URL")
        .unwrap_or_else(|_| "https://api.deepseek.com".to_string());
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let request_body = serde_json::json!({
        "model": "deepseek-chat",
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ],
        "temperature": 0.2,
        "stream": false
    });
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(AI_CONNECT_TIMEOUT))
        .timeout_read(std::time::Duration::from_secs(AI_READ_TIMEOUT))
        .timeout_write(std::time::Duration::from_secs(AI_WRITE_TIMEOUT))
        .build();
    let resp = agent
        .post(&url)
        .set("Content-Type", "application/json")
        .set("Authorization", &format!("Bearer {}", api_key))
        .send_string(&request_body.to_string())
        .map_err(|e| anyhow!("{}", ui.f("mg_err_ai_request", &[&e.to_string()])))?;
    if resp.status() != 200 {
        bail!(
            "{}",
            ui.f("mg_err_ai_status", &[&resp.status().to_string()])
        );
    }
    let resp_text = resp
        .into_string()
        .map_err(|e| anyhow!("{}", ui.f("mg_err_ai_read", &[&e.to_string()])))?;
    let resp_json: Value = serde_json::from_str(&resp_text)
        .map_err(|e| anyhow!("{}", ui.f("mg_err_ai_parse", &[&e.to_string()])))?;
    resp_json
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|m| m.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("{}", ui.t("mg_err_ai_no_content")))
}

/// 调用 DeepSeek 生成中文名与解释，返回 (中文名→英文名, 中文名→解释)
///
/// 只发送 API 英文名与类型签名；失败时上层回退规则模式。
pub fn call_ai_generate_mapping(
    crate_name: &str,
    lang: &str,
    entries: &[ApiEntry],
) -> anyhow::Result<(HashMap<String, String>, HashMap<String, String>)> {
    let api_list = entries
        .iter()
        .map(|e| format!("- {} {}", e.kind.display(), e.signature))
        .collect::<Vec<_>>()
        .join("\n");
    let system_prompt = if lang == "zh" {
        AI_PROMPT_ZH
    } else {
        AI_PROMPT_EN
    };
    let user_prompt = if lang == "zh" {
        format!(
            "crate: {}\n公开 API 列表（名称 + 类型签名）：\n{}",
            crate_name, api_list
        )
    } else {
        format!(
            "crate: {}\nPublic API list (name + type signature):\n{}",
            crate_name, api_list
        )
    };
    let content = deepseek_chat(system_prompt, &user_prompt)?;

    let valid_english_names: HashSet<String> =
        entries.iter().map(|e| e.english_name.clone()).collect();
    parse_ai_result(&content, &valid_english_names)
}

/// 解析 AI 返回的 TOML 文本为 (中文名→英文名, 中文名→解释)；
/// 英文名不在合法集合中的条目丢弃（防止 AI 幻觉改名）
pub fn parse_ai_result(
    text: &str,
    valid_english_names: &HashSet<String>,
) -> anyhow::Result<(HashMap<String, String>, HashMap<String, String>)> {
    // 清洗：去掉 ```toml 围栏与前后杂讯，只保留第一个 [ 开始、结尾围栏之前的部分
    let start = text
        .find('[')
        .ok_or_else(|| anyhow!("{}", crate::ui::Ui::global().t("mg_err_ai_no_toml")))?;
    let end = text[start..]
        .find("```")
        .map(|pos| start + pos)
        .unwrap_or(text.len());
    let toml_part = &text[start..end];
    let table: toml::Value = toml::from_str(toml_part).map_err(|e| {
        anyhow!(
            "{}",
            crate::ui::Ui::global().f("mg_err_ai_toml_parse", &[&e.to_string()])
        )
    })?;
    let mut identifier_map = HashMap::new();
    let mut explanation_map = HashMap::new();
    if let Some(section) = table.get("标识符").and_then(toml::Value::as_table) {
        for (chinese_name, value) in section {
            if let Some(english_name) = value.as_str()
                && valid_english_names.contains(english_name)
                && !chinese_name.is_empty()
            {
                identifier_map.insert(chinese_name.clone(), english_name.to_string());
            }
        }
    }
    if let Some(section) = table.get("解释").and_then(toml::Value::as_table) {
        for (chinese_name, value) in section {
            if let Some(explanation) = value.as_str()
                && !explanation.is_empty()
            {
                if explanation.chars().count() > 40 {
                    eprintln!(
                        "{}",
                        crate::ui::Ui::global().f(
                            "mg_warn_ai_explain_long",
                            &[&explanation.chars().count().to_string(), chinese_name]
                        )
                    );
                }
                explanation_map.insert(chinese_name.clone(), explanation.to_string());
            }
        }
    }
    Ok((identifier_map, explanation_map))
}

/// 检测系统语言（用于 --lang 缺省值）：完整支持全部内置语言，默认 "zh"
///
/// 实现见 [`crate::ui::detect_system_language`]（读取 LC_ALL / LC_MESSAGES / LANG）。
pub fn detect_system_language() -> String {
    crate::ui::detect_system_language()
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 迷你 rustdoc JSON 样本（结构与真实输出一致：index/root/inner 等）
    fn sample_json() -> String {
        r#"{
  "root": "100",
  "index": {
    "100": { "id": 100, "name": "sample", "visibility": "public", "inner": { "module": { "items": [101, 102, 103, 104, 105, 106, 107, 108, 111, 112, 115] } } },
    "101": { "id": 101, "name": "new", "visibility": "public", "inner": { "function": { "sig": { "inputs": [["x", { "primitive": "u32" }]], "output": { "resolved_path": { "path": "Result", "id": 9, "args": { "angle_bracketed": { "args": [ { "type": { "generic": "T" } }, { "type": { "resolved_path": { "path": "String", "id": 10, "args": null } } } ], "constraints": [] } } } } }, "generics": { "params": [ { "name": "T", "kind": { "type": { "bounds": [], "default": null, "is_synthetic": false } } } ], "where_predicates": [] }, "header": { "is_const": false, "is_unsafe": false, "is_async": true, "abi": "Rust" }, "has_body": true } } },
    "102": { "id": 102, "name": "Foo", "visibility": "public", "inner": { "struct": { "kind": { "plain": { "fields": [], "has_stripped_fields": false } }, "generics": { "params": [], "where_predicates": [] }, "impls": [] } } },
    "103": { "id": 103, "name": "State", "visibility": "public", "inner": { "enum": { "generics": { "params": [], "where_predicates": [] }, "has_stripped_variants": false, "variants": [], "impls": [] } } },
    "104": { "id": 104, "name": "Behavior", "visibility": "public", "inner": { "trait": { "is_auto": false, "is_unsafe": false, "is_dyn_compatible": true, "items": [], "generics": { "params": [], "where_predicates": [] }, "bounds": [], "implementations": [] } } },
    "105": { "id": 105, "name": "Count", "visibility": "public", "inner": { "type_alias": { "type": { "primitive": "u32" }, "generics": { "params": [], "where_predicates": [] } } } },
    "106": { "id": 106, "name": "MAX", "visibility": "public", "inner": { "constant": { "type": { "primitive": "u32" }, "const": { "expr": "100", "value": "100u32", "is_literal": true } } } },
    "107": { "id": 107, "name": "print", "visibility": "public", "inner": { "macro": {} } },
    "108": { "id": 108, "name": "inner_tools", "visibility": "crate", "inner": { "module": { "items": [110] } } },
    "110": { "id": 110, "name": "hidden_fn", "visibility": "public", "inner": { "function": { "sig": { "inputs": [], "output": null }, "generics": { "params": [], "where_predicates": [] }, "header": { "is_const": false, "is_unsafe": false, "is_async": false, "abi": "Rust" }, "has_body": true } } },
    "109": { "id": 109, "name": "实例方法", "visibility": "public", "inner": { "function": { "sig": { "inputs": [["self", { "borrowed_ref": { "lifetime": null, "mutable": false, "type": { "resolved_path": { "path": "Foo", "id": 102, "args": null } } } }]], "output": null }, "generics": { "params": [], "where_predicates": [] }, "header": { "is_const": false, "is_unsafe": false, "is_async": false, "abi": "Rust" }, "has_body": true } } },
    "111": { "id": 111, "name": null, "visibility": "public", "inner": { "use": { "source": "serde_core::Deserialize", "name": "Deserialize", "id": 999, "is_glob": false, "is_import": true } } },
    "112": { "id": 112, "name": null, "visibility": "public", "inner": { "use": { "source": "crate::sub", "name": "sub", "id": 113, "is_glob": false, "is_import": true } } },
    "113": { "id": 113, "name": "sub", "visibility": "public", "inner": { "module": { "items": [114] } } },
    "114": { "id": 114, "name": "子函数", "visibility": "public", "inner": { "function": { "sig": { "inputs": [], "output": null }, "generics": { "params": [], "where_predicates": [] }, "header": { "is_const": false, "is_unsafe": false, "is_async": false, "abi": "Rust" }, "has_body": true } } },
    "115": { "id": 115, "name": null, "visibility": "public", "inner": { "use": { "source": "core::*", "name": null, "id": 1, "is_glob": true, "is_import": true } } }
  }
}"#
        .to_string()
    }

    #[test]
    fn test_extract_public_api() {
        let entries = extract_public_api(&sample_json()).expect("应能解析");
        // 顶层 7 类 + 递归子模块中的公开项
        let name_list: Vec<(&str, ApiKind)> = entries
            .iter()
            .map(|e| (e.english_name.as_str(), e.kind))
            .collect();
        assert!(name_list.contains(&("new", ApiKind::Function)));
        assert!(name_list.contains(&("Foo", ApiKind::Struct)));
        assert!(name_list.contains(&("State", ApiKind::Enum)));
        assert!(name_list.contains(&("Behavior", ApiKind::Trait)));
        assert!(name_list.contains(&("Count", ApiKind::TypeAlias)));
        assert!(name_list.contains(&("MAX", ApiKind::Const)));
        assert!(name_list.contains(&("print", ApiKind::Macro)));
        // re-export：名称在 inner.use.name，按 类型别名（type 名 = 来源路径）提取
        assert!(name_list.contains(&("Deserialize", ApiKind::TypeAlias)));
        let deser = entries
            .iter()
            .find(|e| e.english_name == "Deserialize")
            .unwrap();
        assert_eq!(
            deser.signature,
            "type Deserialize = serde_core::Deserialize"
        );
        // 模块 re-export 递归跟随（pub use crate::sub）
        assert!(name_list.contains(&("子函数", ApiKind::Function)));
        // glob 导入（pub use core::*）跳过、私有模块项跳过
        assert!(!name_list.contains(&("hidden_fn", ApiKind::Function)));
        // 私有模块（visibility=crate）不提取；impl 里的方法不提取
        assert!(!name_list.iter().any(|(name, _)| *name == "hidden_fn"));
        assert!(!name_list.iter().any(|(name, _)| *name == "实例方法"));
    }

    #[test]
    fn test_signature_rendering() {
        let entries = extract_public_api(&sample_json()).unwrap();
        let new_fn = entries.iter().find(|e| e.english_name == "new").unwrap();
        // async fn new<T>(x: u32) -> Result<T, String>
        assert_eq!(
            new_fn.signature,
            "async fn new<T>(x: u32) -> Result<T, String>"
        );
        let alias = entries.iter().find(|e| e.english_name == "Count").unwrap();
        assert_eq!(alias.signature, "type Count = u32");
        let constant = entries.iter().find(|e| e.english_name == "MAX").unwrap();
        assert_eq!(constant.signature, "const MAX: u32");
    }

    #[test]
    fn test_rule_chinese_name() {
        assert_eq!(rule_generate_chinese_name("new"), "新建");
        assert_eq!(rule_generate_chinese_name("get_value"), "获取值");
        assert_eq!(rule_generate_chinese_name("set_name"), "设置名称");
        assert_eq!(rule_generate_chinese_name("is_empty"), "是否为空");
        assert_eq!(rule_generate_chinese_name("from_str"), "从字符串解析");
        assert_eq!(rule_generate_chinese_name("to_string"), "转字符串");
        assert_eq!(rule_generate_chinese_name("Error"), "错误");
        assert_eq!(rule_generate_chinese_name("Client"), "客户端");
        assert_eq!(rule_generate_chinese_name("Serialize"), "序列化");
        assert_eq!(rule_generate_chinese_name("Deserialize"), "反序列化");
        assert_eq!(rule_generate_chinese_name("未知标识符"), "未知标识符");
    }

    #[test]
    fn test_crate_chinese_name() {
        assert_eq!(generate_crate_chinese_name("anyhow"), "任意错误");
        assert_eq!(generate_crate_chinese_name("serde"), "序列化");
        assert_eq!(generate_crate_chinese_name("serde_json"), "JSON序列化");
        assert_eq!(generate_crate_chinese_name("tokio"), "异步运行时");
        // 未命中特例时拆词翻译
        assert_eq!(generate_crate_chinese_name("error_handler"), "错误处理器");
        // 无法翻译时保留原名
        assert_eq!(generate_crate_chinese_name("zxxyzq"), "zxxyzq");
    }

    #[test]
    fn test_toml_output_and_disclaimer() {
        let entries = extract_public_api(&sample_json()).unwrap();
        let chinese_name_table: Vec<(String, String)> = entries
            .iter()
            .map(|e| {
                (
                    rule_generate_chinese_name(&e.english_name),
                    e.english_name.clone(),
                )
            })
            .collect();
        let mut explanation_table = HashMap::new();
        explanation_table.insert("新建".to_string(), "创建新的值".to_string());
        let toml = build_mapping_toml(
            "zh",
            "示例",
            "示例库",
            &entries,
            &chinese_name_table,
            &explanation_table,
        );
        // 免责声明必须存在
        assert!(toml.contains(&disclaimer_text("zh")), "缺少免责声明");
        assert!(toml.contains("# crate: 示例"));
        // 可被标准 TOML 解析
        let value: toml::Value = toml::from_str(&toml).expect("TOML 应可解析");
        assert!(value.get("模块路径").is_some());
        assert!(value.get("标识符").is_some());
        assert!(value.get("解释").is_some());
        assert_eq!(value["模块路径"]["示例库"].as_str(), Some("示例"));
        assert_eq!(value["标识符"]["新建"].as_str(), Some("new"));
        assert_eq!(value["标识符"]["状态"].as_str(), Some("State"));
        assert_eq!(value["解释"]["新建"].as_str(), Some("创建新的值"));
    }

    #[test]
    fn test_toml_loadable_by_mapping_manager() {
        let entries = extract_public_api(&sample_json()).unwrap();
        let chinese_name_table: Vec<(String, String)> = entries
            .iter()
            .map(|e| {
                (
                    rule_generate_chinese_name(&e.english_name),
                    e.english_name.clone(),
                )
            })
            .collect();
        let toml = build_mapping_toml(
            "zh",
            "示例",
            "示例库",
            &entries,
            &chinese_name_table,
            &HashMap::new(),
        );
        let temp = tempfile::tempdir().unwrap();
        // 模拟语言包目录结构：keywords.toml + crates/示例.toml（映射管理器要求 keywords.toml 存在）
        fs::write(
            temp.path().join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();
        let crates_dir = temp.path().join("crates");
        fs::create_dir_all(&crates_dir).unwrap();
        fs::write(crates_dir.join("示例.toml"), toml).unwrap();
        let manager = i18n_rust_engine::mapping_manager::MappingManager::load_from_dir(temp.path())
            .expect("映射管理器应能加载");
        // ["解释"] 节被忽略，["标识符"]/["模块路径"] 正常生效
        assert_eq!(
            manager.module_path_map.get("示例库").map(String::as_str),
            Some("示例")
        );
        assert_eq!(
            manager.alias_map.get("新建").map(String::as_str),
            Some("new")
        );
        assert_eq!(
            manager.alias_map.get("状态").map(String::as_str),
            Some("State")
        );
    }

    #[test]
    fn test_parse_ai_result() {
        let valid: HashSet<String> = ["new", "Error", "Context"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let text = "好的，以下是映射：\n```toml\n[\"标识符\"]\n\"新建\" = \"new\"\n\"错误\" = \"Error\"\n\"幻觉\" = \"NotExist\"\n\n[\"解释\"]\n\"新建\" = \"创建一个新的实例，非常简单\"\n\"错误\" = \"描述错误信息的类型\"\n```\n";
        let (identifiers, explanations) = parse_ai_result(text, &valid).expect("应能解析");
        assert!(identifiers.contains_key("新建"));
        assert!(identifiers.contains_key("错误"));
        // 幻觉条目（英文名不在合法集合）被丢弃
        assert!(!identifiers.contains_key("幻觉"));
        assert_eq!(
            explanations.get("新建").map(String::as_str),
            Some("创建一个新的实例，非常简单")
        );
    }

    #[test]
    fn test_ai_result_non_toml_error() {
        let valid = HashSet::new();
        assert!(parse_ai_result("抱歉，我无法完成。", &valid).is_err());
    }

    #[test]
    fn test_split_words() {
        assert_eq!(split_words("get_value"), vec!["get", "value"]);
        assert_eq!(split_words("SerializeValue"), vec!["serialize", "value"]);
        assert_eq!(split_words("into_iter"), vec!["into", "iter"]);
        assert_eq!(split_words("JSON"), vec!["json"]);
    }

    #[test]
    fn test_detect_system_language() {
        let lang = detect_system_language();
        assert!(
            lang == "zh" || lang == "ru",
            "应返回 zh 或 ru，实际: {}",
            lang
        );
    }

    /// 真实工具链集成测试（完全离线）：本地迷你 crate → 临时项目 path 依赖 →
    /// metadata + build + rustdoc → 提取 API。验证 extract_doc_json_internal 全流程。
    #[test]
    fn test_real_toolchain_extraction() {
        let temp = tempfile::tempdir().unwrap();
        let mini = temp.path().join("mini-crate");
        fs::create_dir_all(mini.join("src")).unwrap();
        fs::write(
            mini.join("Cargo.toml"),
            "[package]\nname = \"mini-crate\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            mini.join("src/lib.rs"),
            "pub fn 新建(x: u32) -> Result<Foo, String> { Ok(Foo) }\npub struct Foo;\npub enum 颜色 { 红, 蓝 }\npub trait 行为 {}\npub type 数量 = u32;\npub const 最大值: u32 = 100;\nfn 私有函数() {}\n",
        )
        .unwrap();
        let shell = temp.path().join("外壳");
        fs::create_dir_all(shell.join("src")).unwrap();
        fs::write(
            shell.join("Cargo.toml"),
            "[package]\nname = \"外壳\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-crate = { path = \"../mini-crate\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(shell.join("src/lib.rs"), "// 空库\n").unwrap();

        let temp_guard = TempProject::new("mini-crate").expect("创建临时项目失败");
        // 覆盖临时项目路径为外壳项目
        let _ = fs::remove_dir_all(temp_guard.path());
        fs::create_dir_all(shell.join("src")).unwrap();
        let doc_jsons = extract_doc_json_internal(&TempProject(shell.clone()), "mini-crate")
            .expect("工具链应能提取文档");
        let entries = extract_public_api(&doc_jsons[0].1).unwrap();
        let name_list: Vec<&str> = entries.iter().map(|e| e.english_name.as_str()).collect();
        assert!(
            name_list.contains(&"新建"),
            "应提取到函数 新建: {:?}",
            name_list
        );
        assert!(name_list.contains(&"Foo"));
        assert!(name_list.contains(&"颜色"));
        assert!(name_list.contains(&"行为"));
        assert!(name_list.contains(&"数量"));
        assert!(name_list.contains(&"最大值"));
        assert!(!name_list.contains(&"私有函数"), "私有函数不应被提取");
        // 签名包含类型信息
        let new_fn = entries.iter().find(|e| e.english_name == "新建").unwrap();
        assert!(
            new_fn.signature.contains("u32"),
            "签名应含参数类型: {}",
            new_fn.signature
        );
    }

    /// 不存在的 crate 应报错（含"未找到"提示）
    #[test]
    fn test_nonexistent_crate_error() {
        let temp = TempProject::new("rzc-不存在的crate-xyz-123").expect("创建临时项目失败");
        let result = extract_doc_json_internal(&temp, "rzc-不存在的crate-xyz-123");
        let err = result.expect_err("应报错");
        assert!(
            err.to_string().contains("未找到") || err.to_string().contains("失败"),
            "错误应提示未找到: {}",
            err
        );
    }

    /// 关键字冲突检测：中文名在 keywords.toml 中映射为不同英文时应报告冲突
    #[test]
    fn test_keyword_conflict_detection() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("keywords.toml"),
            "[\"类型\"]\n\"错误\" = \"Err\"\n\"结果\" = \"Result\"\n",
        )
        .unwrap();
        let chinese_name_table = vec![
            ("错误".to_string(), "Error".to_string()),
            ("结果".to_string(), "Result".to_string()),
            ("上下文".to_string(), "Context".to_string()),
        ];
        let conflicts = detect_keyword_conflicts(temp.path(), &chinese_name_table);
        assert_eq!(conflicts.len(), 1, "应只有 '错误' 冲突: {:?}", conflicts);
        assert_eq!(
            conflicts[0],
            ("错误".to_string(), "Err".to_string(), "Error".to_string())
        );
        // 语言包目录不存在时返回空
        assert!(
            detect_keyword_conflicts(&temp.path().join("不存在"), &chinese_name_table).is_empty()
        );
    }

    /// 真实工具链 + 构建脚本（build.rs 生成 include! 源码）：
    /// 验证 OUT_DIR 注入，覆盖 serde 等依赖 build.rs 的 crate
    #[test]
    fn test_build_script_out_dir_injection() {
        let temp = tempfile::tempdir().unwrap();
        let mini = temp.path().join("mini-gen");
        fs::create_dir_all(mini.join("src")).unwrap();
        fs::write(
            mini.join("Cargo.toml"),
            "[package]\nname = \"mini-gen\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[build-dependencies]\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            mini.join("build.rs"),
            "fn main() {\n    let dir = std::env::var(\"OUT_DIR\").unwrap();\n    std::fs::write(std::path::Path::new(&dir).join(\"generated.rs\"), \"pub const GENERATED_VALUE: u32 = 42;\\n\").unwrap();\n}\n",
        )
        .unwrap();
        fs::write(
            mini.join("src/lib.rs"),
            "include!(concat!(env!(\"OUT_DIR\"), \"/generated.rs\"));\npub fn use_generated_value() -> u32 { GENERATED_VALUE }\n",
        )
        .unwrap();
        let shell = temp.path().join("外壳2");
        fs::create_dir_all(shell.join("src")).unwrap();
        fs::write(
            shell.join("Cargo.toml"),
            "[package]\nname = \"外壳2\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-gen = { path = \"../mini-gen\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(shell.join("src/lib.rs"), "// 空库\n").unwrap();

        let doc_jsons = extract_doc_json_internal(&TempProject(shell.clone()), "mini-gen")
            .expect("OUT_DIR 注入后应能生成文档");
        let entries = extract_public_api(&doc_jsons[0].1).unwrap();
        let name_list: Vec<&str> = entries.iter().map(|e| e.english_name.as_str()).collect();
        assert!(
            name_list.contains(&"use_generated_value"),
            "应提取到 include! 生成的代码后的函数: {:?}",
            name_list
        );
    }

    /// 默认 feature 注入：cfg(feature) 裁掉的 API 需通过 --cfg feature=... 恢复
    #[test]
    fn test_default_feature_injection() {
        let temp = tempfile::tempdir().unwrap();
        let mini = temp.path().join("mini-feat");
        fs::create_dir_all(mini.join("src")).unwrap();
        fs::write(
            mini.join("Cargo.toml"),
            "[package]\nname = \"mini-feat\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[features]\ndefault = [\"magic\"]\n\"magic\" = []\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            mini.join("src/lib.rs"),
            "pub fn normal_fn() {}\n#[cfg(feature = \"magic\")]\npub fn magic_fn() {}\n",
        )
        .unwrap();
        let shell = temp.path().join("外壳3");
        fs::create_dir_all(shell.join("src")).unwrap();
        fs::write(
            shell.join("Cargo.toml"),
            "[package]\nname = \"外壳3\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-feat = { path = \"../mini-feat\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(shell.join("src/lib.rs"), "// 空库\n").unwrap();

        let doc_jsons = extract_doc_json_internal(&TempProject(shell.clone()), "mini-feat")
            .expect("应能生成文档");
        let entries = extract_public_api(&doc_jsons[0].1).unwrap();
        let name_list: Vec<&str> = entries.iter().map(|e| e.english_name.as_str()).collect();
        assert!(
            name_list.contains(&"magic_fn"),
            "默认 feature 下的 API 应被提取: {:?}",
            name_list
        );
        assert!(name_list.contains(&"normal_fn"));
    }

    /// 薄壳 crate（meta crate）glob 重导出追踪：目标 crate 仅 `pub use 依赖::*`，
    /// 其公开 API 应通过追踪被重导出的依赖 crate 提取到（如 salvo → salvo_core）
    #[test]
    fn test_glob_reexport_tracking() {
        let temp = tempfile::tempdir().unwrap();
        // 真实 crate：mini-core 提供全部 API
        let core = temp.path().join("mini-core");
        fs::create_dir_all(core.join("src")).unwrap();
        fs::write(
            core.join("Cargo.toml"),
            "[package]\nname = \"mini-core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            core.join("src/lib.rs"),
            "pub fn handle_request() -> Result<Response, Error> { Ok(Response) }\npub struct Response;\npub enum Error { E }\npub trait Handler {}\n",
        )
        .unwrap();
        // 薄壳 crate：全部 API 来自 glob 重导出（与 salvo 的 lib.rs 结构一致）
        let facade = temp.path().join("mini-facade");
        fs::create_dir_all(facade.join("src")).unwrap();
        fs::write(
            facade.join("Cargo.toml"),
            "[package]\nname = \"mini-facade\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-core = { path = \"../mini-core\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            facade.join("src/lib.rs"),
            "pub use mini_core::*;\npub use mini_core as core;\n",
        )
        .unwrap();
        let shell = temp.path().join("外壳4");
        fs::create_dir_all(shell.join("src")).unwrap();
        fs::write(
            shell.join("Cargo.toml"),
            "[package]\nname = \"外壳4\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-facade = { path = \"../mini-facade\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(shell.join("src/lib.rs"), "// 空库\n").unwrap();

        let doc_jsons = extract_doc_json_internal(&TempProject(shell.clone()), "mini-facade")
            .expect("工具链应能提取文档");
        // 目标 crate + 被 glob 重导出的依赖 crate（mini-core）
        let crate_names: Vec<&str> = doc_jsons.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            doc_jsons.len() >= 2,
            "应追踪到被重导出的依赖 crate: {:?}",
            crate_names
        );
        assert!(
            crate_names.contains(&"mini_core"),
            "glob 重导出链应包含 mini_core（crate 名下划线形式）: {:?}",
            crate_names
        );
        // 合并全部 JSON 提取，薄壳 crate 的 API 应全部到位
        let mut entries = Vec::new();
        let mut seen_names = HashSet::new();
        for (_, json_text) in &doc_jsons {
            for entry in extract_public_api(json_text).unwrap() {
                if seen_names.insert(entry.english_name.clone()) {
                    entries.push(entry);
                }
            }
        }
        let name_list: Vec<&str> = entries.iter().map(|e| e.english_name.as_str()).collect();
        assert!(name_list.contains(&"handle_request"));
        assert!(name_list.contains(&"Response"));
        assert!(name_list.contains(&"Error"));
        assert!(name_list.contains(&"Handler"));
    }
}
