//! 诊断消息翻译与所有权详情提取（CLI 同源消息表）。
//!
//! 由服务器启动时初始化错误消息翻译器（语言包 errors.toml，缺失时
//! 回退引擎内嵌 zh），为 publishDiagnostics 提供消息中文化与
//! 所有权错误的叙事化详情。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use i18n_rust_engine::diagnostic::{DiagnosticLocation, OwnershipDetails};
use serde_json::Value;

/// 诊断消息翻译器（errors.toml 消息表）：与 CLI 同源，覆盖 rustc/rust-analyzer
/// 的常见消息（精确/最长前缀/最长后缀匹配）。由服务器启动时初始化
///（语言包目录 errors.toml，缺失时回退内置 zh）；未初始化时为 None，
/// 翻译退化为下方轻量短语替换。
static DIAGNOSTIC_TRANSLATOR: std::sync::OnceLock<
    Option<i18n_rust_engine::diagnostic::ErrorTranslationManager>,
> = std::sync::OnceLock::new();

/// 初始化诊断消息翻译器（语言包 errors.toml；失败/缺失时尝试内置 zh）
pub fn init_diagnostic_translator(lang_pack_path: &std::path::Path) {
    let translator = i18n_rust_engine::diagnostic::ErrorTranslationManager::load_from_file(
        &lang_pack_path.join("errors.toml"),
    )
    .ok()
    .or_else(builtin_zh_error_translator);
    let _ = DIAGNOSTIC_TRANSLATOR.set(translator);
}

/// 物化引擎内嵌的中文 errors.toml 并加载翻译器（语言包目录缺失时兜底）
fn builtin_zh_error_translator() -> Option<i18n_rust_engine::diagnostic::ErrorTranslationManager> {
    let dir = tempfile::tempdir().ok()?;
    let content = i18n_rust_engine::语言::builtin_lang_files("zh")
        .iter()
        .find(|(f, _)| *f == "errors.toml")?
        .1;
    std::fs::write(dir.path().join("errors.toml"), content).ok()?;
    i18n_rust_engine::diagnostic::ErrorTranslationManager::load_from_file(
        &dir.path().join("errors.toml"),
    )
    .ok()
}

/// 翻译诊断消息为当前界面语言
///
/// 优先使用错误消息表（errors.toml [消息翻译] 节，与 CLI 同源，含教学提示）；
/// 未命中时退化为轻量短语替换。多行消息按行逐条翻译后拼接。
/// 替换仅作用于反引号之外的文本，避免误伤消息中引用的
/// 标识符/类型名（如变量名 `expected_value` 含子串 "expected"）。
///
/// pub(crate)：镜像检查（真实项目 rustc 诊断）复用同一消息表。
pub(crate) fn translate_diagnostic_message(message: &str) -> String {
    // 多行消息（rust-analyzer 的 E0004 等）逐行翻译
    if message.contains('\n') {
        let mut first = true;
        let lines: Vec<String> = message
            .split('\n')
            .map(|line| {
                let translated = translate_diagnostic_message_single(line, first);
                first = false;
                translated
            })
            .collect();
        return lines.join("\n");
    }
    translate_diagnostic_message_single(message, true)
}

/// 轻量短语替换表（诊断翻译兜底）：UI 全局语言固定，首次构建后缓存。
/// 诊断每次按键都会发布，若每次翻译都重新构建 ~30 对 String 是纯浪费。
static DIAG_PHRASE_REPLACEMENTS: std::sync::OnceLock<Vec<(String, String)>> =
    std::sync::OnceLock::new();

/// 获取（并惰性构建）轻量短语替换表
fn diag_phrase_replacements() -> &'static Vec<(String, String)> {
    DIAG_PHRASE_REPLACEMENTS.get_or_init(|| {
        let ui = crate::ui::global();
        let mut replacements: Vec<(String, String)> = vec![
            ("{integer}".to_string(), ui.t("diag_rustc_integer")),
            ("{float}".to_string(), ui.t("diag_rustc_float")),
            (
                "floating-point number".to_string(),
                ui.t("diag_rustc_float"),
            ),
            ("integer".to_string(), ui.t("diag_rustc_integer")),
        ];

        // 常见错误模式翻译
        let replace_table = [
            ("cannot find value", ui.t("lsp_phrase_cannot_find_value")),
            ("cannot find type", ui.t("lsp_phrase_cannot_find_type")),
            (
                "cannot find function",
                ui.t("lsp_phrase_cannot_find_function"),
            ),
            ("cannot find module", ui.t("lsp_phrase_cannot_find_module")),
            ("mismatched types", ui.t("lsp_phrase_mismatched_types")),
            ("type mismatch", ui.t("lsp_phrase_type_mismatch")),
            ("expected", ui.t("lsp_phrase_expected")),
            // "found" 仅在类型不匹配场景（"expected ..., found ..."）译为
            // 「实际为」：以带逗号的长键限定语境——裸词 "found" 会把 E0599 的
            // "no method named ... found for struct ..." 误译为「实际为」
            //（"找到"义），也会与 "method not found" 变体相互干扰
            (", found ", format!("，{} ", ui.t("lsp_phrase_found"))),
            ("unused variable", ui.t("lsp_phrase_unused_variable")),
            ("unused import", ui.t("lsp_phrase_unused_import")),
            ("cannot borrow", ui.t("lsp_phrase_cannot_borrow")),
            (
                "borrowed as immutable",
                ui.t("lsp_phrase_borrowed_immutable"),
            ),
            ("borrowed as mutable", ui.t("lsp_phrase_borrowed_mutable")),
            ("no method named", ui.t("lsp_phrase_no_method_named")),
            ("method not found", ui.t("lsp_phrase_method_not_found")),
            ("field", ui.t("lsp_phrase_field")),
            ("does not implement", ui.t("lsp_phrase_does_not_implement")),
            ("the trait", ui.t("lsp_phrase_the_trait")),
            ("is not satisfied", ui.t("lsp_phrase_is_not_satisfied")),
            ("unresolved import", ui.t("lsp_phrase_unresolved_import")),
            ("file not found", ui.t("lsp_phrase_file_not_found")),
            ("aborting due to", ui.t("lsp_phrase_aborting_due_to")),
            ("previous error", ui.t("lsp_phrase_previous_error")),
        ];

        for (en, localized) in replace_table {
            replacements.push((en.to_string(), localized));
        }
        // 长键优先：按键长降序应用，避免短串先替换打碎长短语
        //（如 "found" 先于 "file not found" 会把后者拆成 "file not 实际为"）。
        // 语境冲突已从源头消解（裸词 "found" 改为 ", found " 语境键），
        // 排序仍作为一般性防御保留。
        replacements.sort_by_key(|a| std::cmp::Reverse(a.0.len()));
        replacements
    })
}

/// 单行诊断消息翻译：消息表优先，轻量短语表兜底
fn translate_diagnostic_message_single(message: &str, with_hint: bool) -> String {
    let ui = crate::ui::global();

    // 1. 错误消息表（与 CLI 同源）：精确/最长前缀/最长后缀匹配
    if let Some(translator) = DIAGNOSTIC_TRANSLATOR.get().and_then(|opt| opt.as_ref())
        && let Some((entry, rest)) = translator.query_by_message(message)
    {
        let mut text = entry.message_template.clone();
        if let Some(rest) = rest {
            // {q0}/{q1} 占位符：从动态部分提取引号内容填充（如
            // "trait `Datelike` which provides `year` is never used" →
            //  "特征 `Datelike` 从未被使用"）；填充不足时拼接动态原文
            //（保留 did you mean `x` 等）。
            let (filled, consumed) =
                i18n_rust_engine::diagnostic::fill_dynamic_placeholders(&text, &rest);
            text = filled;
            if !consumed {
                text.push_str(rest.text());
            }
        }
        if with_hint && let Some(hint) = &entry.teaching_hint {
            text.push('\n');
            text.push_str(hint);
        }
        return text;
    }

    // 2. 轻量短语替换（兜底）：静态表首次构建后缓存，避免每次分配
    let replacements = diag_phrase_replacements();
    let mut result = replace_outside_backticks(message, replacements);

    // 添加教学提示
    if message.contains("mismatched types") || message.contains("type mismatch") {
        result.push_str(&ui.t("lsp_hint_mismatched_types"));
    } else if message.contains("cannot find") {
        result.push_str(&ui.t("lsp_hint_cannot_find"));
    } else if message.contains("unused") {
        result.push_str(&ui.t("lsp_hint_unused"));
    } else if i18n_rust_engine::diagnostic::is_unresolved_import_message(message) {
        // 未解析导入：提示通过 `rzc add <crate>` 添加缺失依赖
        if let Some(crate_name) =
            i18n_rust_engine::diagnostic::unresolved_crate_candidates(message).first()
        {
            result.push_str(&ui.f("lsp_hint_add_dependency", &[crate_name]));
        }
    }

    result
}

/// 整词替换：仅当目标前后字符均非 ASCII 标识符字符时替换，
/// 避免裸词模式误伤标识符子串（如 unexpected 中的 expected）
fn replace_whole_word(text: &str, from: &str, to: &str) -> String {
    let is_ident_char = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find(from) {
        let end = pos + from.len();
        let before_ok = rest[..pos]
            .chars()
            .next_back()
            .is_none_or(|c| !is_ident_char(c));
        let after_ok = rest[end..].chars().next().is_none_or(|c| !is_ident_char(c));
        result.push_str(&rest[..pos]);
        result.push_str(if before_ok && after_ok { to } else { from });
        rest = &rest[end..];
    }
    result.push_str(rest);
    result
}

/// 按顺序对反引号包裹之外的文本应用替换，反引号内的内容（标识符/
/// 类型名引用）保持原样；未成对的反引号后文本仍参与替换
fn replace_outside_backticks(input: &str, replacements: &[(String, String)]) -> String {
    let apply = |segment: &str| -> String {
        let mut text = segment.to_string();
        for (from, to) in replacements {
            // 单词模式（如 integer/expected）用整词匹配，避免误伤
            // to_integer/unexpected 等标识符子串；短语模式保持子串替换
            text = if from.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                replace_whole_word(&text, from, to)
            } else {
                text.replace(from.as_str(), to.as_str())
            };
        }
        text
    };

    let mut result = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find('`') {
        result.push_str(&apply(&rest[..start]));
        let after = &rest[start + 1..];
        match after.find('`') {
            Some(end) => {
                // 成对反引号：内部内容原样保留
                result.push('`');
                result.push_str(&after[..end]);
                result.push('`');
                rest = &after[end + 1..];
            }
            None => {
                // 未成对：剩余文本照常替换
                result.push('`');
                result.push_str(&apply(after));
                rest = "";
                break;
            }
        }
    }
    result.push_str(&apply(rest));
    result
}

/// 判断一条诊断是否为虚拟项目 main.rs 中关于 main 函数的 Hint 级提示
///
/// 虚拟项目将用户代码作为模块聚合到 main.rs 中，`fn main()` 在模块内
/// 并非真正的程序入口，rust-analyzer 会发出 "here is a function named `main`"
/// 等教学无关的提示。此函数识别并过滤这类诊断。
pub(super) fn is_main_fn_hint(diag: &Value, _virtual_uri: &str) -> bool {
    // 仅过滤 Hint 级别（severity = 4）
    let severity = diag.get("severity").and_then(|v| v.as_u64()).unwrap_or(0);
    if severity != 4 {
        return false;
    }
    let message = diag.get("message").and_then(|v| v.as_str()).unwrap_or("");
    // 过滤 "here is a function named `main`" 类提示
    message.contains("here is a function named `main`")
        || (message.contains("function `main`") && message.contains("never used"))
}

/// 过滤虚拟项目固有的过程宏误报（#3 症状二）
///
/// 虚拟项目禁用了过程宏（`procMacro.enable=false`）且不含第三方依赖，
/// `#[派生(解析器)]`→`#[derive(Parser)]`（clap 等）无法展开，其辅助属性与
/// 派生宏的名字解析必然失败：
/// - `cannot find attribute `arg``（helper attribute，如 `#[arg(长参数)]`）
/// - `cannot find derive macro `Parser``
///
/// 这两类诊断在虚拟项目语境下恒为误报（用户项目中 `rzc check` 正常通过），
/// 直接过滤；`unresolved import` 不过滤——它是“添加依赖”快速修复的输入，
/// 且用户项目真实缺依赖时同样出现。
pub(super) fn is_missing_dependency_noise(diag: &Value) -> bool {
    let message = diag.get("message").and_then(|v| v.as_str()).unwrap_or("");
    message.contains("cannot find attribute") || message.contains("cannot find derive macro")
}

/// 过滤“引用未打开模块文件”的 E0433/E0432 误报
///
/// LSP 虚拟项目只聚合当前打开的文件：`crate::日志设置` 引用的
/// `日志设置.zh` 未打开时，聚合 main.rs 中没有对应模块声明，
/// rust-analyzer 会报 E0433/E0432。该引用在本项目中实际有效
/// （同名文件存在于同目录，打开后即可解析），属于虚拟项目固有误报；
/// 条目同目录存在同名方言文件时过滤。文件确实不存在（如拼写错误）时
/// 诊断保留，供用户修正。
///
/// 覆盖的 rust-analyzer 消息格式（0.3.3025 起为 rustc 1.98 风格）：
/// - `cannot find module or crate `X` in this scope`（旧 E0433）
/// - `cannot find `X` in `crate``（新 E0433）
/// - `unresolved import `crate::X``（新 E0432，可含 `::Y` 尾段）
/// - `unresolved import `X``（裸路径）
/// - `no `X` in the root`（severity=4 同伴 hint）
pub(super) fn is_unopened_module_reference(
    diag: &Value,
    entry: Option<&crate::translation_cache::TranslationEntry>,
) -> bool {
    let Some(entry) = entry else {
        return false;
    };
    let message = diag.get("message").and_then(|v| v.as_str()).unwrap_or("");
    let Some(name) = unopened_module_name(message) else {
        return false;
    };
    // 同目录存在 `名字.<原扩展名>` 时视为“模块文件未打开”的误报
    let (Some(dir), Some(ext)) = (
        entry.original_path.parent(),
        entry.original_path.extension().and_then(|s| s.to_str()),
    ) else {
        return false;
    };
    dir.join(format!("{name}.{ext}")).is_file()
}

/// 从诊断消息中提取“未打开模块”候选名（仅识别已知消息格式）
fn unopened_module_name(message: &str) -> Option<&str> {
    const RESERVED: &[&str] = &["crate", "self", "super", "std", "core", "alloc"];
    let path = if let Some(rest) = message.strip_prefix("cannot find module or crate `") {
        rest.split_once('`')?.0
    } else if let Some(rest) = message.strip_prefix("cannot find `") {
        let (name, tail) = rest.split_once('`')?;
        if !tail.starts_with(" in `crate`") {
            return None;
        }
        name
    } else if let Some(rest) = message.strip_prefix("unresolved import `") {
        rest.split_once('`')?.0
    } else {
        let rest = message.strip_prefix("no `")?;
        let (name, tail) = rest.split_once('`')?;
        if !tail.starts_with(" in the root") {
            return None;
        }
        name
    };
    // 去掉 `crate::` 前缀后取首段（`crate::X::Y` → X）
    let path = path.strip_prefix("crate::").unwrap_or(path);
    let first = path.split("::").next().unwrap_or(path);
    // 模块名必须是单个安全路径段（防御恶意构造的路径穿越，如 `..`/`/`），
    // 且不能是保留字路径段（std/core/alloc 等系统 crate 不可能是本地模块）
    if first.is_empty()
        || first.contains('/')
        || first.contains('\\')
        || first.contains("..")
        || first.starts_with('.')
        || RESERVED.contains(&first)
    {
        return None;
    }
    Some(first)
}

/// 过滤“第三方依赖在 LSP 虚拟项目缺失”的误报
///
/// 虚拟项目 Cargo.toml 不含用户项目依赖，`serde`/`serde_json` 等已声明
/// 依赖的导入与引用在虚拟项目中必然无法解析。当消息中的候选 crate 名
/// 出现在用户项目最近 Cargo.toml 的依赖表（dependencies/dev/build、
/// workspace、target 各节；`-`/`_` 与大小写归一化，另含去全部
/// 分隔符的紧致形式——`md-5` 的 lib 名为 `md5`）中时，判定为虚拟项目
/// 固有误报并过滤；未列入依赖表的名字（拼写错误、真正缺失的库）保留
/// 诊断，继续提供 `rzc add` 教学提示。
pub(super) fn is_missing_project_dependency(
    diag: &Value,
    entry: Option<&crate::translation_cache::TranslationEntry>,
) -> bool {
    let Some(entry) = entry else {
        return false;
    };
    let message = diag.get("message").and_then(|v| v.as_str()).unwrap_or("");
    // 仅处理“导入/引用未声明 crate”类消息，避免误伤同名变量/函数的诊断
    let is_import_like = message.starts_with("unresolved import `")
        || message.starts_with("cannot find module or crate `")
        || message.contains("use of undeclared crate or module `");
    if !is_import_like {
        return false;
    }
    let Some(deps) = project_dependency_names(&entry.original_path) else {
        return false;
    };
    i18n_rust_engine::diagnostic::extract_backtick_first_segments(message)
        .iter()
        .any(|seg| {
            seg.is_ascii() && crate_name_forms(seg).iter().any(|form| deps.contains(form))
        })
}

/// 过滤“include_str!/include_bytes! 资源在虚拟项目中缺失”的误报
///
/// 虚拟 .rs 位于 `/tmp/i18n_lsp_virtual_*/src/` 下，`include_str!("界面.html")`
/// 相对虚拟目录解析必然失败（rust-analyzer 报 couldn't read `src/界面.html`，
/// 路径按虚拟项目根显示）。去掉 `src` 前缀后能在原方言文件同目录找到该
/// 文件时判定为误报；原项目真实缺失该资源时诊断保留。
pub(super) fn is_missing_include_asset(
    diag: &Value,
    entry: Option<&crate::translation_cache::TranslationEntry>,
) -> bool {
    let Some(entry) = entry else {
        return false;
    };
    let message = diag.get("message").and_then(|v| v.as_str()).unwrap_or("");
    let Some((path, _)) = message
        .split("couldn't read `")
        .nth(1)
        .and_then(|rest| rest.split_once('`'))
    else {
        return false;
    };
    if path.is_empty() || path.contains("..") {
        return false;
    }
    let Some(dir) = entry.original_path.parent() else {
        return false;
    };
    let path = Path::new(path);
    // 虚拟项目根相对路径（如 `src/界面.html`）：去 `src` 前缀后按原目录解析
    let relative = path.strip_prefix("src").unwrap_or(path);
    dir.join(relative).is_file()
}

/// rust-analyzer 编译级诊断抑制判定：文件位于 cargo 项目时，RA 的
/// E 系列 error（severity=1）与 hint（severity=4）不转发给客户端，
/// 编译级诊断以代理自跑的镜像/虚拟项目 cargo check（rustc 口径）为权威来源
///
/// RA 在虚拟项目上对第三方依赖与过程宏的类型推断恒为假红（依赖缺失的
/// E0432/E0599、derive 不展开的 E0277 Display 级联等），且其原生诊断与
/// 真错无法从消息文本可靠区分；镜像检查（或不可用时的虚拟检查）在
/// didOpen/didSave 时提供与 rzc 构建同口径的权威结果。E 系列 hint 是
/// 编译错误的伴生信息拆分发布（“no external crate ...”、“由 this / formatting
/// parameter”等），同为编译级判定的产物，一并抑制。非 cargo 项目
/// （单文件教学场景）不受影响，RA 诊断全量保留；非 E 系列（语法错误、
/// RA 内部错误等）与 warning（severity=2，可能有价值）一律保留。
pub(crate) fn suppress_ra_compile_error(diag: &Value, original_path: &Path) -> bool {
    let severity = diag.get("severity").and_then(|v| v.as_u64()).unwrap_or(0);
    if severity != 1 && severity != 4 {
        return false;
    }
    let Some(code) = diag.get("code").and_then(|v| v.as_str()) else {
        return false;
    };
    if !is_rustc_error_code(code) {
        return false;
    }
    in_cargo_project(original_path)
}

/// E 系列 rustc 错误码（E0432/E0277 等；RA 自带诊断沿用该格式）
fn is_rustc_error_code(code: &str) -> bool {
    let Some(digits) = code.strip_prefix('E') else {
        return false;
    };
    !digits.is_empty() && digits.len() <= 4 && digits.chars().all(|c| c.is_ascii_digit())
}

/// 文件路径 → 是否位于 cargo 项目（最近上层存在 Cargo.toml）的进程级缓存
static IN_CARGO_PROJECT_CACHE: OnceLock<Mutex<HashMap<PathBuf, bool>>> = OnceLock::new();

/// 判断文件是否位于 cargo 项目（向上查找最近 Cargo.toml），按路径缓存
fn in_cargo_project(file_path: &Path) -> bool {
    let cache = IN_CARGO_PROJECT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(v) = cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(file_path)
    {
        return *v;
    }
    let mut dir = file_path.parent();
    let mut has_manifest = false;
    while let Some(d) = dir {
        if d.join("Cargo.toml").is_file() {
            has_manifest = true;
            break;
        }
        dir = d.parent();
    }
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(file_path.to_path_buf(), has_manifest);
    has_manifest
}

/// 依赖清单解析结果缓存（Cargo.toml 路径 → (mtime, 归一化依赖名集合)）
type ProjectDepsCache = Mutex<HashMap<PathBuf, (SystemTime, Arc<HashSet<String>>)>>;

/// 项目依赖清单 mtime 缓存（键为 Cargo.toml 路径）
static PROJECT_DEPS_CACHE: OnceLock<ProjectDepsCache> = OnceLock::new();

/// 向上查找最近的 Cargo.toml 并解析依赖名集合（归一化），按 mtime 缓存；
/// 未找到清单时返回 None（调用方保持原诊断行为）
fn project_dependency_names(file_path: &Path) -> Option<Arc<HashSet<String>>> {
    let mut dir = file_path.parent();
    let manifest = loop {
        let d = dir?;
        let candidate = d.join("Cargo.toml");
        if candidate.is_file() {
            break candidate;
        }
        dir = d.parent();
    };
    let mtime = std::fs::metadata(&manifest).ok()?.modified().ok()?;
    let cache = PROJECT_DEPS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((cached_mtime, deps)) = guard.get(&manifest)
        && *cached_mtime == mtime
    {
        return Some(deps.clone());
    }
    let text = std::fs::read_to_string(&manifest).ok()?;
    let deps = Arc::new(parse_dependency_names(&text));
    guard.insert(manifest, (mtime, deps.clone()));
    Some(deps)
}

/// 解析 Cargo.toml 中的全部依赖名（含 dev/build、workspace、target 各节）
fn parse_dependency_names(text: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    let Ok(value) = text.parse::<toml::Value>() else {
        return names;
    };
    let mut collect = |table: Option<&toml::Value>| {
        if let Some(t) = table.and_then(|v| v.as_table()) {
            names.extend(t.keys().flat_map(|k| crate_name_forms(k)));
        }
    };
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        collect(value.get(section));
    }
    collect(value.get("workspace").and_then(|w| w.get("dependencies")));
    if let Some(targets) = value.get("target").and_then(|t| t.as_table()) {
        for (_, cfg) in targets {
            for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
                collect(cfg.get(section));
            }
        }
    }
    names
}

/// 依赖名归一化：Cargo 视 `-`/`_` 等价，crate 名统一小写比较
fn normalize_crate_name(name: &str) -> String {
    name.to_ascii_lowercase().replace('-', "_")
}

/// crate 名比对候选集：归一形式外另收录去全部分隔符的紧致形式——
/// 包名与实际 lib 名可能只在紧致意义上对应（`md-5` 的 lib 名为 `md5`，
/// 代码中 `使用 md5::...` 而清单声明 `md-5 = "0.10"`）
fn crate_name_forms(name: &str) -> Vec<String> {
    let underscore = normalize_crate_name(name);
    let compact: String = underscore.chars().filter(|c| *c != '_').collect();
    if compact == underscore {
        vec![underscore]
    } else {
        vec![underscore, compact]
    }
}

/// 从 LSP 诊断（rust-analyzer 格式）中提取所有权错误详情
///
/// 变量名取自原始消息中的反引号（如 use of moved value: `x`）；
/// 移动/借用/再次使用位置取自已还原到母语文件的 range 与 relatedInformation，
/// LSP 的 0-based 行号统一转为 1-based（与 rustc 诊断一致）。
/// 仅处理 E0382/E0502/E0507 及消息模式匹配的所有权错误。
pub(crate) fn extract_ownership_details(
    original_diag: &Value,
    restored: &Value,
    original_uri: &str,
) -> Option<OwnershipDetails> {
    let message = original_diag["message"].as_str()?;
    let error_code = original_diag["code"].as_str().unwrap_or("");
    let is_ownership_error = matches!(error_code, "E0382" | "E0502" | "E0507")
        || message.contains("use of moved value")
        || message.contains("moved value")
        || message.contains("cannot borrow")
        || message.contains("cannot move out of");
    if !is_ownership_error {
        return None;
    }

    let var_name = extract_backtick_var_name(message)?;
    let main_location = construct_position_from_range(original_uri, &restored["range"]);

    let mut move_location = None;
    let mut borrow_location = None;
    let mut reuse_location = None;

    if let Some(related_info) = restored["relatedInformation"].as_array() {
        for item in related_info {
            let label = item["message"].as_str().unwrap_or("");
            let Some(location) =
                construct_position_from_range(original_uri, &item["location"]["range"])
            else {
                continue;
            };
            // 注意顺序："borrow later used here" 同时含 borrow 与 used here，应归为再次使用
            if label.contains("used here")
                || label.contains("later used")
                || label.contains("after move")
            {
                reuse_location.get_or_insert(location);
            } else if label.contains("move") {
                move_location.get_or_insert(location);
            } else if label.contains("borrow") {
                borrow_location.get_or_insert(location);
            }
        }
    }

    // 主 range 兜底：E0382 → 再次使用；E0502 → 借用发生；E0507 → 移动发生
    if let Some(location) = main_location {
        if matches!(error_code, "E0382") || message.contains("moved value") {
            reuse_location.get_or_insert(location);
        } else if matches!(error_code, "E0502") || message.contains("cannot borrow") {
            borrow_location.get_or_insert(location);
        } else if matches!(error_code, "E0507") || message.contains("cannot move out of") {
            move_location.get_or_insert(location);
        }
    }

    if move_location.is_none() && borrow_location.is_none() && reuse_location.is_none() {
        return None;
    }
    Some(OwnershipDetails {
        var_name,
        move_location,
        borrow_location,
        reuse_location,
    })
}

/// 提取消息中反引号包裹的变量名
///
/// 示例："use of moved value: `数据`" → "数据"。
fn extract_backtick_var_name(message: &str) -> Option<String> {
    let start = message.find('`')?;
    let rest = &message[start + 1..];
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
}

/// 从 LSP range 构造诊断位置（0-based 行号转为 1-based）
fn construct_position_from_range(file_name: &str, range: &Value) -> Option<DiagnosticLocation> {
    let line_start = range["start"]["line"].as_u64()? as u32;
    let col_start = range["start"]["character"].as_u64()? as u32;
    let line_end = range["end"]["line"].as_u64()? as u32;
    let col_end = range["end"]["character"].as_u64()? as u32;
    Some(DiagnosticLocation {
        file_name: file_name.to_string(),
        line_start: line_start + 1,
        column_start: col_start + 1,
        line_end: line_end + 1,
        column_end: col_end + 1,
        source_text: None,
        label: None,
        is_primary: false,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        crate_name_forms, is_rustc_error_code, parse_dependency_names, suppress_ra_compile_error,
    };

    /// 紧致形式：`md-5` → md_5 + md5（lib 名去分隔符）
    #[test]
    fn test_crate_name_forms_compact() {
        let forms = crate_name_forms("md-5");
        assert!(forms.contains(&"md_5".to_string()));
        assert!(forms.contains(&"md5".to_string()));
        // 无分隔符的名字保持单一形式
        assert_eq!(crate_name_forms("rayon"), vec!["rayon".to_string()]);
    }

    /// 依赖清单解析收录紧致形式，供 md-5 风格的包名匹配
    #[test]
    fn test_parse_dependency_names_compact() {
        let deps = parse_dependency_names("[dependencies]\nmd-5 = \"0.10\"\n");
        assert!(deps.contains("md_5"));
        assert!(deps.contains("md5"));
        assert!(!deps.contains("serde"));
    }

    /// E 系列错误码识别（E + 至多 4 位数字）
    #[test]
    fn test_is_rustc_error_code() {
        assert!(is_rustc_error_code("E0432"));
        assert!(is_rustc_error_code("E0277"));
        assert!(!is_rustc_error_code("e0432"));
        assert!(!is_rustc_error_code("E"));
        assert!(!is_rustc_error_code("E04a2"));
        assert!(!is_rustc_error_code("unused_imports"));
    }

    /// cargo 项目内的 E 系列 error 抑制；项目外/非 E 码/非 error 级不抑制
    #[test]
    fn test_suppress_ra_compile_error() {
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("diag_text_suppress_{pid}"));
        let src = dir.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"t\"\n").unwrap();
        let file = src.join("main.zh");
        std::fs::write(&file, "").unwrap();

        let err = serde_json::json!({ "severity": 1, "code": "E0432", "message": "unresolved import `md5`" });
        assert!(suppress_ra_compile_error(&err, &file));
        // E 系列 hint（编译错误的伴生信息拆分发布）同样抑制
        let hint = serde_json::json!({ "severity": 4, "code": "E0432", "message": "no external crate `toml`" });
        assert!(suppress_ra_compile_error(&hint, &file));
        // warning 不抑制（可能有价值）
        let warn = serde_json::json!({ "severity": 2, "code": "E0432" });
        assert!(!suppress_ra_compile_error(&warn, &file));
        // 非 E 系列（语法错误等）不抑制
        let syntax = serde_json::json!({ "severity": 1, "code": "syntax" });
        assert!(!suppress_ra_compile_error(&syntax, &file));
        // 非 E 系列 hint 不抑制（如 main 函数提示）
        let plain_hint = serde_json::json!({ "severity": 4, "code": "unused" });
        assert!(!suppress_ra_compile_error(&plain_hint, &file));
        // 非 cargo 项目（单文件教学场景）不抑制
        let outside = std::env::temp_dir().join(format!("diag_text_suppress_out_{pid}.zh"));
        std::fs::write(&outside, "").unwrap();
        assert!(!suppress_ra_compile_error(&err, &outside));
        assert!(!suppress_ra_compile_error(&hint, &outside));

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&outside);
    }
}
