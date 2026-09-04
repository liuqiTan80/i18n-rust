//! 诊断翻译器：结合错误消息翻译表与类型映射，将 rustc 的英文诊断
//! 转化为面向母语学习者的教学诊断信息。

use std::collections::HashMap;

use super::message::ErrorTranslationManager;
use super::model::CompilerDiagnostic;
use super::teaching::{
    DiagnosticLevel, DiagnosticLocation, TeachingDiagnostic, extract_ownership_details,
};

/// 诊断翻译器：将 rustc 诊断翻译为教学诊断
///
/// 结合错误消息翻译表和类型映射，将 rustc 的英文诊断转化为
/// 面向中文学习者的教学诊断信息。
pub struct DiagnosticTranslator {
    translation_manager: ErrorTranslationManager,
    type_map: HashMap<String, String>, // 来自关键字映射表，用于替换消息中的英文类型
}

/// 对带模块路径的类型名做最长后缀匹配
///
/// `std::fmt::Display` 逐段尝试（fmt::Display → Display），命中映射段后
/// 保留原路径前缀，仅替换命中段：`std::fmt::Display` → `std::fmt::显示`。
pub(crate) fn replace_type_token(
    token: &str,
    type_map: &HashMap<String, String>,
) -> Option<String> {
    let segments: Vec<&str> = token.split("::").collect();
    for i in 1..segments.len() {
        let suffix = segments[i..].join("::");
        if let Some(zh) = type_map.get(&suffix) {
            let mut result = segments[..i].join("::");
            if !result.is_empty() {
                result.push_str("::");
            }
            result.push_str(zh);
            return Some(result);
        }
    }
    None
}

/// 本地化带模块路径的类型名（三段式）
///
/// 1. 类型后缀段映射：`std::fmt::Display` → `std::fmt::可显示`
/// 2. 路径前缀完整匹配（从长到短）：`std::fmt` → `标准库::格式化`（或 `std` → `标准库`）
/// 3. 剩余中间段单段映射：`fmt` → `格式化`
///    最终：`std::fmt::Display` → `标准库::格式化::可显示`；
///    仅含路径（如 `std::io::Error`）未命中任何段时返回 None 保持原样。
pub(crate) fn localize_type_token(token: &str, map: &HashMap<String, String>) -> Option<String> {
    let original = token.to_string();
    // 1. 类型后缀段映射
    let result = replace_type_token(token, map).unwrap_or_else(|| token.to_string());
    // 2. 路径前缀完整匹配（从长到短）
    let mut segments: Vec<String> = result.split("::").map(|s| s.to_string()).collect();
    if segments.len() >= 2 {
        for i in (1..segments.len()).rev() {
            let prefix = segments[..i].join("::");
            if let Some(zh) = map.get(&prefix) {
                let rest = segments[i..].join("::");
                segments = vec![zh.clone()];
                segments.extend(rest.split("::").map(|s| s.to_string()));
                break;
            }
        }
        // 3. 剩余中间段单段映射（跳过首尾，避免误伤已替换的路径与类型段）
        for i in 1..segments.len().saturating_sub(1) {
            if let Some(zh) = map.get(&segments[i]) {
                segments[i] = zh.clone();
            }
        }
    }
    let result = segments.join("::");
    (result != original).then_some(result)
}

/// 用消息的动态部分（前缀匹配的后缀原文 / 后缀匹配的前缀原文）中的
/// 单引号内容填充模板的 {q0}/{q1} 捕获占位符
///
/// 用于 "function `foo` is never used" 这类动态名在中间的消息：
/// 模板写完整语义（"函数 `{q0}` 从未被使用"），捕获不足时回退原模板
///（调用方追加动态原文），完整覆盖时不追加，避免中英文混排。
fn fill_quote_captures(template: &str, dynamic: &str) -> (String, bool) {
    let mut result = template.to_string();
    let mut consumed_any = false;
    for (i, placeholder) in ["{q0}", "{q1}"].iter().enumerate() {
        if result.contains(placeholder) {
            // 反引号场景（dead_code 等）：
            // - {q0} 取第一个引号对内容（后缀键："variants `黄灯` and `绿灯` "）；
            //   但前缀键（"function `foo` is never used" 的 rest="foo` is never used"）
            //   只有一个引号，rsplit 才能取到引号内内容，故引号数为 1 时用 rsplit；
            // - {q1} 取最后一个引号对内容（rsplit）。
            // 单引号场景（Unicode 混淆 help）按段隔取（第 0/2 段为两个被比较的字符）。
            let content = if dynamic.contains('`') {
                if i == 1 || dynamic.matches('`').count() == 1 {
                    dynamic.rsplit('`').nth(1)
                } else {
                    dynamic.split('`').nth(1)
                }
            } else {
                dynamic.split('\'').nth(i * 2)
            };
            match content {
                Some(content) => {
                    result = result.replace(placeholder, content);
                    consumed_any = true;
                }
                None => return (template.to_string(), false),
            }
        }
    }
    (result, consumed_any)
}

/// 统计类型字符串开头的引用层数（`&` 前缀个数）
///
/// 规则：连续 strip `&` 前缀，遇 `mut`（含 `mut ` / `mut` 结尾）即停止
/// —— `&mut T` 是单层可变引用，不算两层。
/// 示例：`&String` → 1，`&&String` → 2，`&mut String` → 1，`String` → 0。
pub(crate) fn count_ref_prefix(ty: &str) -> usize {
    let mut rest = ty;
    let mut count = 0;
    while let Some(after) = rest.strip_prefix('&') {
        count += 1;
        if after.starts_with("mut ") || after == "mut" || after.starts_with("mut\t") {
            break;
        }
        rest = after;
    }
    count
}

/// 本地化引用类型：先整体查 type_map（如 `&str` → `字符串引用`），
/// 未命中时拆开 `&` 前缀，对本体做类型本地化后拼回
/// （如 `&String` → `&字符串`、`&&i32` → `&&整数`）。
pub(crate) fn localize_ref_type(ty: &str, map: &HashMap<String, String>) -> String {
    if let Some(zh) = map.get(ty) {
        return zh.clone();
    }
    let mut prefix = String::new();
    let mut rest = ty;
    while let Some(after) = rest.strip_prefix('&') {
        prefix.push('&');
        rest = after;
    }
    if prefix.is_empty() {
        return ty.to_string();
    }
    let (mutable, base) = match rest.strip_prefix("mut ") {
        Some(base) => ("mut ", base),
        None => ("", rest),
    };
    let base_zh = map
        .get(base)
        .cloned()
        .or_else(|| localize_type_token(base, map))
        .unwrap_or_else(|| base.to_string());
    format!("{prefix}{mutable}{base_zh}")
}

/// 从 `expected `X`, found `Y`` 形式的文本（rustc label）中提取期望/实际类型
fn extract_expected_found(text: &str) -> Option<(String, String)> {
    let pos = text.find("expected ")?;
    let after = &text[pos + "expected ".len()..];
    let comma_pos = after.find(", found ")?;
    let exp = after[..comma_pos].trim().trim_matches('`').to_string();
    let fnd = after[comma_pos + ", found ".len()..]
        .trim()
        .trim_matches('`')
        .to_string();
    Some((exp, fnd))
}

/// 整词替换：仅当目标前后字符均非标识符字符（字母/数字/下划线）时替换，
/// 避免裸词模式（如 integer）误伤 to_integer/integer_count 等标识符子串
pub(crate) fn replace_whole_word(text: &str, from: &str, to: &str) -> String {
    let is_ident_char = |c: char| c.is_alphanumeric() || c == '_';
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

/// 提取文本中所有反引号包裹的内容（按出现顺序）
fn extract_backtick_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        match after.find('`') {
            Some(end) => {
                tokens.push(after[..end].to_string());
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    tokens
}

/// 从诊断消息中提取前两个反引号包裹的类型
///
/// rustc 1.97+ 的算术错误不再输出 label，类型信息嵌入 message，
/// 如 E0369：`cannot add `{integer}` to `&str``。
fn extract_types_from_message(message: &str) -> Option<(String, String)> {
    let tokens = extract_backtick_tokens(message);
    if tokens.len() >= 2 {
        Some((tokens[0].clone(), tokens[1].clone()))
    } else {
        None
    }
}

impl DiagnosticTranslator {
    /// 创建诊断翻译器
    pub fn new(
        translation_manager: ErrorTranslationManager,
        type_map: HashMap<String, String>,
    ) -> Self {
        Self {
            translation_manager,
            type_map,
        }
    }

    /// 翻译单条诊断信息
    pub fn translate_diagnostic(&self, diagnostic: &CompilerDiagnostic) -> TeachingDiagnostic {
        let error_code = diagnostic.code.as_ref().map(|c| c.code.clone());

        let matched_entry = error_code
            .as_deref()
            .and_then(|code| self.translation_manager.query(code))
            // 无错误码或错误码未收录时，按消息原文匹配（[消息翻译] 节）
            .or_else(|| {
                self.translation_manager
                    .query_by_message(&diagnostic.message)
                    .map(|(entry, _)| entry)
            });

        // 从主要 span 的 label 中提取 expected 和 found；
        // rustc 1.97+ 的算术错误（E0369 等）不再输出 label，类型信息
        // 直接嵌入 message（如 `cannot add `{integer}` to `&str``），
        // 因此提取失败时回退从 message 中解析反引号包裹的类型。
        let primary_label = diagnostic
            .spans
            .iter()
            .find(|s| s.is_primary)
            .and_then(|s| s.label.as_deref());

        let (expected, found) = primary_label
            .and_then(extract_expected_found)
            .or_else(|| extract_types_from_message(&diagnostic.message))
            .unwrap_or_default();

        // 构建翻译消息
        let translated_message = if let Some(entry) = matched_entry {
            let mut template = entry.message_template.clone();
            if !expected.is_empty() && !found.is_empty() {
                template = template
                    .replace("{期望}", &expected)
                    .replace("{实际}", &found);
            } else if template.contains("{期望}") || template.contains("{实际}") {
                // 无法提取期望/实际类型时回退 rustc 原文，避免输出裸占位符
                template = diagnostic.message.clone();
            }
            // 消息表前缀/后缀匹配时，把未翻译的动态部分拼回模板
            //（如 "did you mean " → "你是否想用 `foo`?"），
            // 或经 {q0}/{q1} 捕获完整化（如 "function `foo` is never used" →
            //  "函数 `foo` 从未被使用"）；错误码条目（无动态部分）不受影响。
            if let Some((_, rest)) = self
                .translation_manager
                .query_by_message(&diagnostic.message)
                && let Some(rest) = rest
            {
                let (filled, consumed) = fill_quote_captures(&template, rest);
                template = filled;
                if !consumed {
                    template.push_str(rest);
                }
            }
            // 对模板中的类型名进行中文化替换
            self.replace_type_names(template)
        } else {
            self.replace_type_names(diagnostic.message.clone())
        };

        let mut teaching_hints = Vec::new();
        if let Some(entry) = matched_entry
            && let Some(hint) = &entry.teaching_hint
        {
            teaching_hints.push(hint.clone());
        }
        for child in &diagnostic.children {
            if child.level == "help" {
                // help 短语优先查消息表翻译（前缀匹配保留动态后缀），未命中保留原文
                let hint = self
                    .translation_manager
                    .query_by_message(&child.message)
                    .map(|(entry, rest)| {
                        let mut text = entry.message_template.clone();
                        if let Some(rest) = rest {
                            // 模板含 {q0}/{q1} 捕获占位符时，从后缀原文提取单引号内容填充
                            //（如 "Unicode character '，' (…) looks like ',' (…)" →
                            //  "Unicode 字符 '，' 形似 ','，但它并不是它"）；
                            // 捕获完整覆盖时不再追加英文后缀。
                            let (filled, consumed) = fill_quote_captures(&text, rest);
                            text = filled;
                            if !consumed {
                                text.push_str(rest);
                            }
                        }
                        text
                    })
                    .unwrap_or_else(|| child.message.clone());
                teaching_hints.push(crate::语言::f("diag_fix_suggestion", &[&hint]));
            }
        }

        // E0308 引用层数教学提示：期望/实际类型均为引用或其中之一为引用、
        // 但引用层数不同（如 `expected `&String`, found `String``）时，
        // 追加一条检查 `&` 数量的提示，覆盖初学者最常见的借用错误之一。
        if error_code.as_deref() == Some("E0308") && !expected.is_empty() && !found.is_empty() {
            let exp_refs = count_ref_prefix(&expected);
            let fnd_refs = count_ref_prefix(&found);
            if exp_refs != fnd_refs && (exp_refs > 0 || fnd_refs > 0) {
                teaching_hints.push(crate::语言::f(
                    "diag_ref_depth_hint",
                    &[
                        &localize_ref_type(&expected, &self.type_map),
                        &localize_ref_type(&found, &self.type_map),
                    ],
                ));
            }
        }

        let locations = diagnostic
            .spans
            .iter()
            .map(DiagnosticLocation::from_span)
            .collect();

        // 所有权错误：提取叙事化详情（变量名、移动/借用、再次使用位置）
        let ownership_details = error_code
            .as_deref()
            .and_then(|code| extract_ownership_details(code, diagnostic));

        // 翻译消息中含 {变量名} 占位符时，用提取到的变量名填充
        let mut translated_message = translated_message;
        if let Some(details) = &ownership_details {
            translated_message = translated_message.replace("{变量名}", &details.var_name);
        }

        // 其余占位符（{变量名}/{名称}/{类型}/{特征}）按出现顺序从消息反引号内容
        // 回退填充，覆盖无 label 的错误（E0384 重复赋值、E0433 未找到类型等）；
        // 提取不到时回退 rustc 原文，避免输出裸占位符。
        let mut tokens = extract_backtick_tokens(&diagnostic.message).into_iter();
        for placeholder in ["{变量名}", "{名称}", "{类型}", "{特征}"] {
            if translated_message.contains(placeholder) {
                match tokens.next() {
                    Some(token) => {
                        translated_message = translated_message.replace(placeholder, &token);
                    }
                    None => {
                        translated_message = diagnostic.message.clone();
                        break;
                    }
                }
            }
        }
        // 占位符填充的才是运行时实际类型名，需再次中文化
        //（如 E0277 的 `std::fmt::Display` → `std::fmt::显示`、`{integer}` → `整数`）
        translated_message = self.replace_type_names(translated_message);

        let children = diagnostic
            .children
            .iter()
            .map(|child| self.translate_diagnostic(child))
            .collect();

        TeachingDiagnostic {
            level: DiagnosticLevel::from_str(&diagnostic.level),
            error_code,
            translated_message,
            original_message: diagnostic.message.clone(),
            teaching_hints,
            locations,
            children,
            ownership_details,
        }
    }

    /// 使用类型映射替换消息中的英文类型名
    fn replace_type_names(&self, message: String) -> String {
        let mut result = message;
        // rustc 未推断字面量占位符（`{integer}`/`{float}`）及 1.97+ 裸显示名
        // （`integer`/`floating-point number`）按全局语言翻译；
        // 裸 integer 用整词匹配，避免误伤 to_integer/integer_count 等标识符
        result = result
            .replace("{integer}", &crate::语言::t("diag_rustc_integer"))
            .replace("{float}", &crate::语言::t("diag_rustc_float"))
            .replace("floating-point number", &crate::语言::t("diag_rustc_float"));
        result = replace_whole_word(&result, "integer", &crate::语言::t("diag_rustc_integer"));
        // 类型映射：仅替换反引号包裹的完整类型名（rustc 诊断中的类型均在反引号内），
        // 避免 "str"→"文本" 等短条目把消息中的 "string" 部分替换成 "文本ing"
        for token in extract_backtick_tokens(&result) {
            if let Some(zh) = self.type_map.get(&token) {
                result = result.replace(&format!("`{}`", token), &format!("`{}`", zh));
            } else if let Some(replaced) = localize_type_token(&token, &self.type_map) {
                // 带模块路径的类型名：三段式本地化
                //（std::fmt::Display → 标准库::格式化::可显示）
                result = result.replace(&format!("`{}`", token), &format!("`{}`", replaced));
            }
        }
        result
    }

    /// 批量翻译诊断列表
    pub fn batch_translate(&self, diagnostics: &[CompilerDiagnostic]) -> Vec<TeachingDiagnostic> {
        diagnostics
            .iter()
            .map(|d| self.translate_diagnostic(d))
            .collect()
    }
}
