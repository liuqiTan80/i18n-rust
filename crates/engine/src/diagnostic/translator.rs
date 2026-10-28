//! 诊断翻译器：结合错误消息翻译表与类型映射，将 rustc 的英文诊断
//! 转化为面向母语学习者的教学诊断信息。

use std::collections::HashMap;

use super::message::{ErrorTranslationManager, fill_dynamic_placeholders};
use super::model::CompilerDiagnostic;
use super::teaching::{
    DiagnosticLevel, DiagnosticLocation, TeachingDiagnostic, extract_ownership_details,
};
use crate::mapping_manager::MappingManager;

/// 诊断翻译器：将 rustc 诊断翻译为教学诊断
///
/// 结合错误消息翻译表和类型映射，将 rustc 的英文诊断转化为
/// 面向中文学习者的教学诊断信息。
pub struct DiagnosticTranslator {
    translation_manager: ErrorTranslationManager,
    type_map: HashMap<String, String>, // 来自关键字映射表，用于替换消息中的英文类型
    /// 待译清单登记用语言代码（`None` 取当前界面语言，见 [`super::gaps`]）
    gap_lang: Option<String>,
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

/// 去掉全部前导 `&` 与 `mut ` 后的核心类型
///
/// 用于判定两侧差异是否**仅为引用层数**：仅当剥去引用后的核心类型相同
/// （`&String` vs `String`、`&&i32` vs `i32`）时，提示「检查 `&` 数量」才
/// 成立；核心类型不同（`&str` vs `String`、`i32` vs `&str`）是真实类型不
/// 匹配，此时提示引用层数是假阳性。
/// 示例：`&&mut String` → `String`，`&str` → `str`，`i32` → `i32`。
pub(crate) fn ref_base_type(ty: &str) -> &str {
    let mut rest = ty;
    loop {
        if let Some(after) = rest.strip_prefix('&') {
            rest = after;
        } else if let Some(after) = rest.strip_prefix("mut ") {
            rest = after;
        } else {
            break;
        }
    }
    rest.trim()
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

/// 提取 rustc "if you wanted to use a crate named `X`" 建议中的 crate 名
fn suggested_crate_name(message: &str) -> Option<&str> {
    message
        .strip_prefix("if you wanted to use a crate named `")
        .and_then(|rest| rest.split('`').next())
        .filter(|name| !name.is_empty())
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

/// 由映射管理器构建诊断类型反查表（英文类型名 → 母语名），CLI 与 LSP 共用。
///
/// 数据来源与优先级（与历史 CLI 内联逻辑完全一致，抽出以消除两处漂移）：
/// keywords `[类型]` 节反转为主，stdlib/别名表仅补充缺失条目（不覆盖），
/// 模块路径映射（std → 标准库 等）覆盖同名第三方条目以稳定标准库路径翻译。
/// 仅收录「母语键」（含非 ASCII 字符）的反向项，避免把 `format`→`fmt` 之类
/// 纯英文修正项引入诊断译文（会让译文出现英文值）。
pub fn build_type_map(manager: &MappingManager) -> HashMap<String, String> {
    let 是母语键 = |键: &str| {
        !键.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    let mut reverse_map: HashMap<String, String> = manager
        .get_section_mapping("类型")
        .map(|section| {
            section
                .iter()
                .filter(|(k, _)| 是母语键(k))
                .map(|(k, v)| (v.clone(), k.clone()))
                .collect()
        })
        .unwrap_or_default();
    for (母语, 英文) in manager.get_alias_map() {
        if 是母语键(母语) {
            reverse_map
                .entry(英文.clone())
                .or_insert_with(|| 母语.clone());
        }
    }
    for (母语, 英文) in manager.get_module_path_map() {
        if 是母语键(母语) {
            reverse_map.insert(英文.clone(), 母语.clone());
        }
    }
    reverse_map
}

/// 错误码模板（`[E0xxx]` 节）支持的动态占位符：按此顺序从消息原文反引号
/// 内容中依次回填。顺序须与各模板内占位符的出现先后相容（如 E0609 的
/// `{类型}` 先于 `{字段名}`、E0027 的 `{结构名}` 先于 `{字段名}`）；
/// 模板不含某占位符时不消费 token，故跨模板共用一份列表不会串位。
/// `{期望}`/`{实际}` 另有专门的 expected/found 提取逻辑，不在此列。
///
/// `pub(super)`：语言包门禁（`mod tests`）要用同一份清单校验教学提示，
/// 新增占位符时自动纳入检查，不必两处维护。
pub(super) const CODE_TEMPLATE_PLACEHOLDERS: &[&str] = &[
    "{变量名}",
    "{名称}",
    "{类型}",
    "{特征}",
    "{模块名}",
    "{字段名}",
    "{方法名}",
    "{函数名}",
    "{结构名}",
    "{关联类型}",
    "{特征名}",
    "{生命周期}",
];

/// [`DiagnosticTranslator::render_main_message`] 的返回：主消息译文与其教学提示。
#[derive(Debug, Clone)]
pub struct RenderedMessage {
    /// 已回填占位符并完成类型中文化的主消息译文
    pub text: String,
    /// 命中条目的教学提示（若有），供调用方按需追加
    pub hint: Option<String>,
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
            gap_lang: None,
        }
    }

    /// 指定待译清单登记语言（CLI 按源文件扩展名传入，避免把 ja 的漏译记到 zh）
    pub fn with_gap_lang(mut self, lang: impl Into<String>) -> Self {
        self.gap_lang = Some(lang.into());
        self
    }

    /// 消息表本体（供 cargo 纯文本错误行等**非 JSON 通道**复用同一张表）
    pub fn translation_manager(&self) -> &ErrorTranslationManager {
        &self.translation_manager
    }

    /// 登记一条英文残留（兜底照旧输出原文，仅记录待补线索）
    fn note_gap(&self, level: &str, code: Option<&str>, message: &str) {
        super::gaps::record_gap(
            self.gap_lang.as_deref().unwrap_or(""),
            level,
            code.unwrap_or(""),
            message,
        );
    }

    /// 仅凭 (错误码, 消息原文, 主 span 标签) 渲染主消息——CLI/LSP 同口径共享入口。
    ///
    /// 渲染链与 [`DiagnosticTranslator::translate_diagnostic`] 的主消息分支一致：
    /// 错误码表优先（完整桩/静态模板）→ 消息表（精确/前缀/后缀/通配段 + `{qN}`
    /// 回填）→ 占位符按序回填 → `type_map` 类型中文化。区别在于不依赖完整
    /// `CompilerDiagnostic`：期望/实际取自 `primary_label`（rustc 主 span 标签），
    /// 其次从消息原文 `expected X, found Y`/前两个反引号类型提取；
    /// `{变量名}`/`{名称}`/`{类型}`/`{特征}` 按序取消息反引号内容。
    ///
    /// 安全兵：错误码模板存在无法回填的占位符时（如 rust-analyzer 把
    /// `expected/found` 放在 relatedInformation 而非主消息的 E0308）
    /// **回退消息表**而非输出英文或裸占位符——保证译文不劣于旧行为。
    /// 命中任一表返回 `Some`，两表皆未命中返回 `None`（调用方走轻量兵底）。
    pub fn render_main_message(
        &self,
        code: Option<&str>,
        message: &str,
        primary_label: Option<&str>,
    ) -> Option<RenderedMessage> {
        let code_entry = code
            .filter(|c| !c.is_empty())
            .and_then(|c| self.translation_manager.query(c));
        let message_query = self.translation_manager.query_by_message(message);

        // 1. 错误码表优先（与 CLI `code_entry.or_else(message_query)` 对齐）
        if let Some(entry) = code_entry {
            let mut template = entry.message_template.clone();
            let expected_found = primary_label
                .and_then(extract_expected_found)
                .or_else(|| extract_expected_found(message))
                .or_else(|| extract_types_from_message(message));
            if let Some((expected, found)) = expected_found
                && !expected.is_empty()
                && !found.is_empty()
            {
                template = template
                    .replace("{期望}", &expected)
                    .replace("{实际}", &found);
            }
            // 其余占位符按出现顺序从消息反引号内容回退填充
            let mut tokens = extract_backtick_tokens(message).into_iter();
            for placeholder in CODE_TEMPLATE_PLACEHOLDERS {
                if template.contains(placeholder) {
                    match tokens.next() {
                        Some(token) => template = template.replace(placeholder, &token),
                        None => break,
                    }
                }
            }
            // 仍有未回填占位符 → 码表桩无法从可用信息渲染 → 回退消息表（不出英文/裸占位符）
            let unresolved = ["{期望}", "{实际}"]
                .iter()
                .chain(CODE_TEMPLATE_PLACEHOLDERS.iter())
                .any(|p| template.contains(p));
            if !unresolved {
                return Some(RenderedMessage {
                    text: self.replace_type_names(template),
                    hint: entry.teaching_hint.clone(),
                });
            }
        }

        // 2. 消息表（精确/前缀/后缀/通配段）+ `{qN}` 回填，额外补 type_map 类型中文化
        if let Some((entry, rest)) = message_query {
            let mut text = entry.message_template.clone();
            if let Some(rest) = rest {
                let (filled, consumed) = fill_dynamic_placeholders(&text, &rest);
                text = filled;
                if !consumed {
                    text.push_str(rest.text());
                    // 残段未被模板消费 → 英文拼回尾巴（兜底），登记待译
                    self.note_gap(super::gaps::LEVEL_MAIN, code, message);
                }
            }
            return Some(RenderedMessage {
                text: self.replace_type_names(text),
                hint: entry.teaching_hint.clone(),
            });
        }

        // 两表皆未命中：调用方兜底显示英文原文 → 登记待译清单
        self.note_gap(super::gaps::LEVEL_MAIN, code, message);
        None
    }

    /// 翻译单条诊断信息
    pub fn translate_diagnostic(&self, diagnostic: &CompilerDiagnostic) -> TeachingDiagnostic {
        let error_code = diagnostic.code.as_ref().map(|c| c.code.clone());

        let code_entry = error_code
            .as_deref()
            .and_then(|code| self.translation_manager.query(code));
        // 无错误码或错误码未收录时，按消息原文匹配（[消息翻译] 节）；
        // 消息表查询结果保留匹配来源（前缀/后缀键），供动态部分回填
        let message_query = self
            .translation_manager
            .query_by_message(&diagnostic.message);
        let matched_entry = code_entry.or_else(|| message_query.map(|(entry, _)| entry));

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
                self.note_gap(
                    super::gaps::LEVEL_MAIN,
                    error_code.as_deref(),
                    &diagnostic.message,
                );
            }
            // 动态部分回填仅对消息表条目生效（前缀/后缀键匹配时保留动态名，
            // 如 "did you mean " → "你是否想用 `foo`?"）；错误码条目是完整桩，
            // 回拼会在中文模板后粘上英文残段（如 E0624 的
            // "字段或方法是私有的year` is private"）。错误码条目需要展示动态
            // 名称时用 {名称} 等占位符（从消息反引号内容回退填充，见下方）。
            if code_entry.is_none()
                && let Some((_, Some(rest))) = message_query
            {
                let (filled, consumed) = fill_dynamic_placeholders(&template, &rest);
                template = filled;
                if !consumed {
                    template.push_str(rest.text());
                    self.note_gap(
                        super::gaps::LEVEL_MAIN,
                        error_code.as_deref(),
                        &diagnostic.message,
                    );
                }
            }
            // 对模板中的类型名进行中文化替换
            self.replace_type_names(template)
        } else {
            // 错误码表与消息表均未命中：原样显示英文（兜底），登记待译清单
            self.note_gap(
                super::gaps::LEVEL_MAIN,
                error_code.as_deref(),
                &diagnostic.message,
            );
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
                // "if you wanted to use a crate named `X`" 建议：crate 名规则不允许
                // 非 ASCII，中文名实为拼写错误的母语模块，建议 `cargo add 中文名`
                // 必然无效（如 `add 规则类型`）；静默丢弃，避免误导初学者。
                if suggested_crate_name(&child.message).is_some_and(|name| !name.is_ascii()) {
                    continue;
                }
                // help 短语优先查消息表翻译（前缀匹配保留动态后缀），未命中保留原文
                let hint = self
                    .translation_manager
                    .query_by_message(&child.message)
                    .map(|(entry, rest)| {
                        let mut text = entry.message_template.clone();
                        if let Some(rest) = rest {
                            // 模板含 {q0}/{q1} 捕获占位符时，从动态部分提取引号内容填充
                            //（如 "trait `Datelike` which provides `year` is never used" →
                            //  "特征 `Datelike` 从未被使用"）；
                            // 捕获完整覆盖时不再追加英文残段。
                            let (filled, consumed) = fill_dynamic_placeholders(&text, &rest);
                            text = filled;
                            if !consumed {
                                text.push_str(rest.text());
                                self.note_gap(
                                    super::gaps::LEVEL_HELP,
                                    error_code.as_deref(),
                                    &child.message,
                                );
                            }
                        }
                        text
                    })
                    .unwrap_or_else(|| {
                        // help 通道未译：英文原样进建议行，登记待译清单
                        self.note_gap(
                            super::gaps::LEVEL_HELP,
                            error_code.as_deref(),
                            &child.message,
                        );
                        child.message.clone()
                    });
                teaching_hints.push(crate::语言::f("diag_fix_suggestion", &[&hint]));
            }
        }

        // E0308 引用层数教学提示：两侧引用层数不同（如 `expected `&String`,
        // found `String``）且**剥去引用后的核心类型相同**时，追加一条检查
        // `&` 数量的提示，覆盖初学者最常见的借用错误之一。核心类型不同
        //（`&str` vs `String`、`i32` vs `&str`）是真实类型不匹配，不追加——
        // 否则建议「调整 & 数量」会误导（加/去 `&` 变不出另一种核心类型）。
        if error_code.as_deref() == Some("E0308") && !expected.is_empty() && !found.is_empty() {
            let exp_refs = count_ref_prefix(&expected);
            let fnd_refs = count_ref_prefix(&found);
            if exp_refs != fnd_refs
                && (exp_refs > 0 || fnd_refs > 0)
                && ref_base_type(&expected) == ref_base_type(&found)
            {
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

        // 其余占位符（{变量名}/{名称}/{类型}/{特征}/{模块名}/{字段名} 等）
        // 按出现顺序从消息反引号内容回退填充，覆盖无 label 的错误
        //（E0384 重复赋值、E0433 未找到类型、E0583 模块文件缺失等）；
        // 提取不到时回退 rustc 原文，避免输出裸占位符。
        let mut tokens = extract_backtick_tokens(&diagnostic.message).into_iter();
        for placeholder in CODE_TEMPLATE_PLACEHOLDERS {
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
