//! 诊断消息翻译与所有权详情提取（CLI 同源消息表）。
//!
//! 由服务器启动时初始化错误消息翻译器（语言包 errors.toml，缺失时
//! 回退引擎内嵌 zh），为 publishDiagnostics 提供消息中文化与
//! 所有权错误的叙事化详情。

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
pub(super) fn translate_diagnostic_message(message: &str) -> String {
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
            ("found", ui.t("lsp_phrase_found")),
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
            // {q0}/{q1} 占位符：从动态部分提取引号内容（如 `红绿灯::黄灯`）。
            // {q0} 取第一个引号对（后缀键场景）；引号数为 1 时（前缀键
            // 场景，rest 以 "foo`" 开头）用 rsplit 取唯一内容；{q1} 取最后一个。
            let mut filled = false;
            for (i, placeholder) in ["{q0}", "{q1}"].iter().enumerate() {
                if text.contains(placeholder) {
                    let content = if rest.contains('`') {
                        if i == 1 || rest.matches('`').count() == 1 {
                            rest.rsplit('`').nth(1)
                        } else {
                            rest.split('`').nth(1)
                        }
                    } else {
                        rest.split('\'').nth(i * 2 + 1)
                    };
                    if let Some(content) = content {
                        text = text.replace(placeholder, content);
                        filled = true;
                    }
                    break;
                }
            }
            if !filled {
                // 无占位符：模板后拼接动态部分（保留 did you mean `x` 等）
                text.push_str(rest);
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

/// 从 LSP 诊断（rust-analyzer 格式）中提取所有权错误详情
///
/// 变量名取自原始消息中的反引号（如 use of moved value: `x`）；
/// 移动/借用/再次使用位置取自已还原到母语文件的 range 与 relatedInformation，
/// LSP 的 0-based 行号统一转为 1-based（与 rustc 诊断一致）。
/// 仅处理 E0382/E0502/E0507 及消息模式匹配的所有权错误。
pub(super) fn extract_ownership_details(
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
