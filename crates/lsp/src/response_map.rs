//! 响应映射模块
//!
//! 将 rust-analyzer 响应中的位置信息（指向虚拟 .rs 文件）
//! 还原为原始 .zh 文件的位置。同时处理诊断信息的中文翻译。

use std::sync::Arc;

use i18n_rust_engine::diagnostic::{DiagnosticLocation, OwnershipDetails};
use serde_json::{Value, json};

use crate::translation_cache::{TranslationCache, TranslationEntry};

/// 响应映射器
///
/// 持有翻译缓存的引用，负责将 rust-analyzer 的各种响应
/// 中的位置/URI 从虚拟文件还原到原始中文文件。
pub struct ResponseMapper {
    cache: Arc<TranslationCache>,
    /// 预构建的反向映射表（英文 → 母语），O(1) 查找且结果确定
    reverse_map: std::collections::HashMap<String, String>,
    /// 是否启用补全语言过滤（禁止串语言）
    ///
    /// 方言关键字与英文存在差异时启用（zh/ja/ru 等）；
    /// 英文包为恒等映射（fn=fn），不过滤。
    strict_native_filter: bool,
}

impl ResponseMapper {
    /// 创建新的响应映射器
    ///
    /// 反向表直接复用 TranslationCache 构造时预构建的合并表
    /// （关键字反转后合并别名反转，多对一冲突保留排序最小者，
    /// 结果确定），避免每次构造映射器重复构建。
    pub fn new(cache: Arc<TranslationCache>) -> Self {
        // 英文语言包为恒等映射（键=值），无需也无法做语言过滤
        let strict_native_filter = cache
            .keyword_map()
            .iter()
            .any(|(native, english)| native != english);
        Self {
            reverse_map: cache.reverse_map().clone(),
            cache,
            strict_native_filter,
        }
    }

    /// 判断一个 URI 是否指向我们的虚拟文件
    pub fn is_virtual_uri(&self, uri: &str) -> bool {
        self.cache.query_by_virtual_uri(uri).is_some()
    }

    /// 将虚拟 URI 替换为原始 URI
    pub fn restore_uri(&self, uri: &str) -> String {
        if let Some(entry) = self.cache.query_by_virtual_uri(uri) {
            entry.original_uri.clone()
        } else {
            uri.to_string()
        }
    }

    /// 将英文（虚拟文件）行号映射回中文（原始文件）行号
    pub fn restore_line(&self, uri: &str, en_line: u32) -> u32 {
        if let Some(entry) = self.cache.query_by_virtual_uri(uri) {
            restore_line_single(&entry, en_line)
        } else {
            en_line
        }
    }

    /// 映射一条 LSP 位置（行、列）从虚拟文件到原始文件
    pub fn restore_position(&self, uri: &str, line: u32, col: u32) -> (u32, u32) {
        let zh_line = self.restore_line(uri, line);
        let zh_col = self.cache.en_col_to_zh_col(uri, line, col);
        (zh_line, zh_col)
    }

    /// 映射一个 LSP Range
    pub fn restore_range(&self, uri: &str, range: &Value) -> Value {
        let start_line = range["start"]["line"].as_u64().unwrap_or(0) as u32;
        let start_col = range["start"]["character"].as_u64().unwrap_or(0) as u32;
        let end_line = range["end"]["line"].as_u64().unwrap_or(0) as u32;
        let end_col = range["end"]["character"].as_u64().unwrap_or(0) as u32;

        let (zh_start_line, zh_start_col) = self.restore_position(uri, start_line, start_col);
        let (zh_end_line, zh_end_col) = self.restore_position(uri, end_line, end_col);

        json!({
            "start": { "line": zh_start_line, "character": zh_start_col },
            "end": { "line": zh_end_line, "character": zh_end_col }
        })
    }

    /// 映射一个 LSP Location（URI + Range）
    pub fn restore_location(&self, location: &Value) -> Value {
        let virtual_uri = location["uri"].as_str().unwrap_or("");
        let original_uri = self.restore_uri(virtual_uri);
        let original_range = self.restore_range(virtual_uri, &location["range"]);

        json!({
            "uri": original_uri,
            "range": original_range
        })
    }

    /// 映射 rust-analyzer 的 publishDiagnostics 通知
    ///
    /// 将诊断信息中的 URI 和位置还原为原始 .zh 文件，
    /// 并尝试翻译诊断消息为中文。
    pub fn map_diagnostics(&self, params: &Value) -> Value {
        let virtual_uri = params["uri"].as_str().unwrap_or("");
        let original_uri = self.restore_uri(virtual_uri);
        let diagnostics_list = params["diagnostics"].as_array();

        let mut mapped_diagnostics = Vec::new();

        if let Some(diagnostics_array) = diagnostics_list {
            for diag in diagnostics_array {
                // 过滤虚拟项目 main.rs 中关于 main 函数的 Hint 级提示
                // （fn main() 在模块内不是真正的入口，rust-analyzer 会发出
                // "here is a function named `main`" 等教学无关的提示）
                if is_main_fn_hint(diag, virtual_uri) {
                    continue;
                }

                let mut mapped = diag.clone();

                // 映射范围（使用列映射）
                if diag.get("range").is_some() {
                    mapped["range"] = self.restore_range(virtual_uri, &diag["range"]);
                }

                // 映射 relatedInformation 中的位置
                if let Some(related_info) =
                    diag.get("relatedInformation").and_then(|v| v.as_array())
                {
                    let mut mapped_related = Vec::new();
                    for item in related_info {
                        let mut mapped_item = item.clone();
                        if let Some(location) = item.get("location") {
                            mapped_item["location"] = self.restore_location(location);
                        }
                        // 子消息（help/note）同样翻译——悬停查看诊断详情时
                        // 不泄漏英文（如 "value moved here"、"consider ..."）
                        if let Some(message) = item.get("message").and_then(|v| v.as_str()) {
                            mapped_item["message"] =
                                Value::String(translate_diagnostic_message(message));
                        }
                        mapped_related.push(mapped_item);
                    }
                    mapped["relatedInformation"] = Value::Array(mapped_related);
                }

                // 翻译诊断消息
                mapped["message"] = Value::String(translate_diagnostic_message(
                    diag["message"].as_str().unwrap_or(""),
                ));

                // 所有权错误：提取叙事化详情并存入 data 字段（供 VS Code 扩展可视化）
                if let Some(details) = extract_ownership_details(diag, &mapped, &original_uri)
                    && let Ok(details_value) = serde_json::to_value(&details)
                {
                    // 保留 rust-analyzer 已有的 data（如代码操作数据），嵌套存入
                    match mapped.get_mut("data") {
                        Some(existing) if existing.is_object() => {
                            existing["所有权详情"] = details_value;
                        }
                        _ => {
                            mapped["data"] = details_value;
                        }
                    }
                }

                // 添加教学提示标记
                mapped["source"] = Value::String("i18n-rust".to_string());

                mapped_diagnostics.push(mapped);
            }
        }

        json!({
            "uri": original_uri,
            "diagnostics": mapped_diagnostics,
            "version": params.get("version").cloned().unwrap_or(Value::Null)
        })
    }

    /// 映射补全响应中的位置信息
    ///
    /// 将 textEdit/additionalTextEdits 中的 range 映射回原始文件，
    /// 并将英文标识符/代码反向翻译为母语（否则接受补全会把
    /// 英文关键字插入母语源文件，或自动导入编辑落在错误位置）。
    pub fn map_completion_response(&self, response: &Value, original_uri: &str) -> Value {
        let mut result = response.clone();

        if let Some(items_list) = result.get("items").and_then(|v| v.as_array()) {
            let mut mapped_items = Vec::new();
            // 语言过滤白名单（用户源码中出现过的标识符）懒加载，
            // 仅在确实遇到未翻译的纯英文项时才扫描一次
            let mut user_tokens: Option<std::collections::HashSet<String>> = None;
            for item in items_list {
                let mut mapped = item.clone();

                // 1. 映射 textEdit 的 range 并反向翻译 newText
                if let Some(text_edit) = item.get("textEdit") {
                    if let Some(range) = text_edit.get("range") {
                        mapped["textEdit"]["range"] = self.restore_range(original_uri, range);
                    }
                    if let Some(new_text) = text_edit.get("newText").and_then(|v| v.as_str()) {
                        mapped["textEdit"]["newText"] =
                            Value::String(self.translate_code(new_text));
                    }
                }

                // 2. additionalTextEdits（如自动导入）：位置与内容同样需要还原
                if let Some(extra_edits) = item.get("additionalTextEdits") {
                    mapped["additionalTextEdits"] =
                        self.map_edit_list(extra_edits, original_uri, true);
                }

                // 3. label：先精确反查（关键字等），未命中则词法级转译
                //    （标准库 API 如 Vec::new / println! 也能还原为母语）
                if let Some(label) = item.get("label").and_then(|v| v.as_str()) {
                    let mapped_label = self
                        .reverse_lookup(label)
                        .unwrap_or_else(|| self.translate_code(label));

                    // 3.5 语言过滤（禁止串语言）：非英文方言下，补全列表
                    //     只保留母语项，过滤未翻译的外部英文项（第三方库、
                    //     未收录的标准库 API 等）。保留条件（满足其一）：
                    //     a. 翻译命中（mapped_label 与原文不同）；
                    //     b. label 含母语字符（用户定义的母语标识符）；
                    //     c. label 的末段标识符在用户源码中出现过
                    //        （用户自己定义的项，含英文命名）。
                    if self.strict_native_filter {
                        let native_char = !mapped_label.is_ascii();
                        let translated = mapped_label != label;
                        let user_defined = if native_char || translated {
                            true
                        } else {
                            let tokens =
                                user_tokens.get_or_insert_with(|| self.cache.user_defined_tokens());
                            label_identifier_suffix(label).is_some_and(|name| tokens.contains(name))
                        };
                        if !(native_char || translated || user_defined) {
                            continue;
                        }
                    }

                    mapped["label"] = Value::String(mapped_label);
                }

                // 4. detail（类型签名）：词法级转译，如 fn push(...) → 函数 推入(...)
                if let Some(detail) = item.get("detail").and_then(|v| v.as_str()) {
                    mapped["detail"] = Value::String(self.translate_code(detail));
                }

                // 4.5 labelDetails：VS Code 提示框右侧优先显示此字段
                //     （description 为签名如 fn()、detail 为 crate/模块路径），
                //     不还原会把英文 fn() 泄漏给母语用户
                if let Some(label_details) = item.get("labelDetails").and_then(|v| v.as_object()) {
                    let mut mapped_details = label_details.clone();
                    if let Some(desc) = label_details.get("description").and_then(|v| v.as_str()) {
                        mapped_details.insert(
                            "description".to_string(),
                            Value::String(self.translate_code(desc)),
                        );
                    }
                    if let Some(detail) = label_details.get("detail").and_then(|v| v.as_str()) {
                        mapped_details.insert(
                            "detail".to_string(),
                            Value::String(self.translate_code(detail)),
                        );
                    }
                    mapped["labelDetails"] = Value::Object(mapped_details);
                }

                // 5. documentation：命中解释表（大白话）时替换为中文，
                //    未命中保留英文原文（避免丢失签名等关键信息）
                if let Some(doc) = item.get("documentation") {
                    mapped["documentation"] = self.translate_completion_doc(item, doc);
                }

                // 6. 反向映射 insertText（可能含 snippet 占位符，仅精确匹配时替换）
                if let Some(insert_text) = item.get("insertText").and_then(|v| v.as_str())
                    && let Some(zh_name) = self.reverse_lookup(insert_text)
                {
                    mapped["insertText"] = Value::String(zh_name);
                }

                // 7. 方法/函数补全补括号：rust-analyzer 的 snippet 配置在代理
                //    环境不可靠（方法补全默认不带括号），这里对方法/函数类
                //    补全项在 textEdit 末尾补 snippet 括号，光标自动落在括号内
                //    （教学常用场景如 `长度()`）；已带括号（含参数占位）跳过。
                let kind = item.get("kind").and_then(|v| v.as_i64()).unwrap_or(0);
                if matches!(kind, 2 | 3)
                    && let Some(text) = mapped["textEdit"]["newText"].as_str()
                    && !text.contains('(')
                {
                    mapped["textEdit"]["newText"] = Value::String(format!("{}(${{1:}})", text));
                    // 2 = Snippet 格式：占位符由客户端解析，光标落在括号内
                    mapped["insertTextFormat"] = Value::Number(2.into());
                }

                // 8. 关键字补全前导空格：rust-analyzer 的 "let mut" 组合 snippet
                //    在代理环境不可用，散落的关键字项（如 `可变`）直接插入会与
                //    前一标识符粘连（`让可变`）。判断 newText 的首个标识符是否
                //    为关键字映射表中的词（不依赖 kind，rust-analyzer 的关键字
                //    补全 kind 不可靠），且前一字符是标识符时在 textEdit 前补空格。
                if let Some(text) = mapped["textEdit"]["newText"].as_str() {
                    let first_word = text
                        .split(|c: char| c.is_whitespace() || c == '$')
                        .next()
                        .unwrap_or("");
                    if !first_word.is_empty() && self.cache.keyword_map().contains_key(first_word) {
                        let start = mapped["textEdit"]["range"]["start"].clone();
                        if let (Some(line), Some(col)) = (
                            start["line"].as_u64().map(|v| v as usize),
                            start["character"].as_u64().map(|v| v as usize),
                        ) && col > 0
                            && let Some(entry) = self.cache.query_original(original_uri)
                            && let Some(line_text) = entry.zh_content.lines().nth(line)
                        {
                            let prev = line_text.chars().nth(col - 1);
                            let needs_space = prev.is_some_and(|c| {
                                !c.is_whitespace()
                                    && !matches!(c, '(' | '.' | ':' | ',' | ';' | '{' | '[' | '!')
                            });
                            if needs_space && !text.starts_with(char::is_whitespace) {
                                mapped["textEdit"]["newText"] = Value::String(format!(" {text}"));
                            }
                        }
                    }
                }

                mapped_items.push(mapped);
            }
            result["items"] = Value::Array(mapped_items);
        }

        result
    }

    /// 补全文档（documentation）：优先用中文解释表替换英文文档
    ///
    /// 查键顺序：从文档解析类型::方法（如 Option::unwrap）→ label 直查
    /// → 方法名兜底；全部未命中时保留英文原文。
    fn translate_completion_doc(&self, item: &Value, doc: &Value) -> Value {
        let doc_text = match doc {
            Value::String(s) => Some(s.as_str()),
            Value::Object(obj) => obj.get("value").and_then(|v| v.as_str()),
            _ => None,
        };
        if let Some(text) = doc_text
            && let Some(explain) = self.lookup_explanation(text)
        {
            return Value::String(explain);
        }
        // label 直查 + 方法名兜底
        let ui = crate::ui::global();
        if let Some(label) = item.get("label").and_then(|v| v.as_str())
            && let Some(explain) = ui.explanation(label)
        {
            return Value::String(explain.to_string());
        }
        if let Some(text) = doc_text
            && let Some(name) = extract_fn_name(text)
            && let Some(explain) = ui.explanation(&name)
        {
            return Value::String(explain.to_string());
        }
        doc.clone()
    }

    /// 映射文档高亮响应（DocumentHighlight[]：range + kind）
    ///
    /// 请求方向已把位置转为虚拟文件坐标，响应必须还原，
    /// 否则高亮位置以虚拟文件坐标泄漏给客户端。
    pub fn map_document_highlight_response(&self, response: &Value, original_uri: &str) -> Value {
        match response {
            Value::Array(items) => {
                let mapped: Vec<Value> = items
                    .iter()
                    .map(|item| {
                        let mut mapped = item.clone();
                        if let Some(range) = item.get("range") {
                            mapped["range"] = self.restore_range(original_uri, range);
                        }
                        mapped
                    })
                    .collect();
                Value::Array(mapped)
            }
            Value::Null => Value::Array(Vec::new()),
            _ => response.clone(),
        }
    }

    /// 映射语义着色响应（semanticTokens/full、semanticTokens/range）
    ///
    /// rust-analyzer 返回的 token 坐标基于虚拟 .rs（转译产物），
    /// 必须还原到方言文件坐标，否则变量/参数等颜色落在错误位置
    /// 或完全不显示。data 为 LSP delta 编码（每 5 项一组）：
    /// `[deltaLine, deltaStart, length, tokenType, tokenModifiers]`——
    /// 先还原为绝对坐标，逐 token 映射起点与终点列，再重新 delta 编码。
    /// resultId 原样透传（客户端依赖它做增量请求）。
    pub fn map_semantic_tokens_response(&self, response: &Value, original_uri: &str) -> Value {
        let mut result = response.clone();
        let Some(data) = result.get("data").and_then(|v| v.as_array()) else {
            return result;
        };

        // 1. delta 编码 → 绝对坐标（跨行时列归零重置，同行时列累加）
        let mut tokens: Vec<(u32, u32, u32, u32, u32)> = Vec::new();
        let mut line = 0u32;
        let mut col = 0u32;
        for chunk in data.chunks(5) {
            if chunk.len() < 5 {
                break;
            }
            let delta_line = chunk[0].as_u64().unwrap_or(0) as u32;
            let delta_start = chunk[1].as_u64().unwrap_or(0) as u32;
            let length = chunk[2].as_u64().unwrap_or(0) as u32;
            let token_type = chunk[3].as_u64().unwrap_or(0) as u32;
            let modifiers = chunk[4].as_u64().unwrap_or(0) as u32;
            line = line.saturating_add(delta_line);
            col = if delta_line == 0 {
                col.saturating_add(delta_start)
            } else {
                delta_start
            };
            tokens.push((line, col, length, token_type, modifiers));
        }

        // 2. 起点/终点列分别还原到方言坐标，长度取映射后的差值
        //（关键字替换改变了列宽，如 `让`(1) → `let`(3)，长度必须重算）
        let mut mapped: Vec<(u32, u32, u32, u32, u32)> = Vec::new();
        for (t_line, t_col, t_len, t_type, t_mod) in tokens {
            let (zh_line, zh_start) = self.restore_position(original_uri, t_line, t_col);
            let (_, zh_end) =
                self.restore_position(original_uri, t_line, t_col.saturating_add(t_len));
            let zh_len = zh_end.saturating_sub(zh_start).max(1);
            mapped.push((zh_line, zh_start, zh_len, t_type, t_mod));
        }

        // 3. 重新 delta 编码
        let mut new_data = Vec::with_capacity(mapped.len() * 5);
        let mut prev_line = 0u32;
        let mut prev_col = 0u32;
        for (t_line, t_col, t_len, t_type, t_mod) in mapped {
            let delta_line = t_line.saturating_sub(prev_line);
            let delta_start = if delta_line == 0 {
                t_col.saturating_sub(prev_col)
            } else {
                t_col
            };
            new_data.push(json!(delta_line));
            new_data.push(json!(delta_start));
            new_data.push(json!(t_len));
            new_data.push(json!(t_type));
            new_data.push(json!(t_mod));
            prev_line = t_line;
            prev_col = t_col;
        }
        result["data"] = Value::Array(new_data);
        result
    }

    /// 映射定义跳转响应（Location 或 Location[]）
    pub fn map_definition_response(&self, response: &Value) -> Value {
        match response {
            Value::Null => Value::Null,
            Value::Array(array) => {
                let mapped: Vec<Value> = array
                    .iter()
                    .map(|item| self.restore_location(item))
                    .collect();
                Value::Array(mapped)
            }
            Value::Object(_) => {
                // 单个 Location
                self.restore_location(response)
            }
            _ => response.clone(),
        }
    }

    /// 映射悬停响应中的位置信息
    ///
    /// range 字段映射回原始文件位置；contents 命中语言包 ["解释"] 表时
    /// 在文档上方插入一行加粗大白话提示（未命中保持原样透传）。
    pub fn map_hover_response(&self, response: &Value, original_uri: &str) -> Value {
        let mut result = response.clone();
        if let Some(range) = response.get("range") {
            result["range"] = self.restore_range(original_uri, range);
        }
        if let Some(contents) = response.get("contents") {
            result["contents"] = self.enrich_hover_contents(contents);
        }
        result
    }

    /// 给 hover contents 前置大白话提示（MarkupContent / MarkedString / 数组）
    fn enrich_hover_contents(&self, contents: &Value) -> Value {
        match contents {
            // MarkupContent：{"kind": "markdown", "value": ...}
            // MarkedString：{"language": "rust", "value": ...}
            Value::Object(obj)
                if obj.get("kind").and_then(|k| k.as_str()) == Some("markdown")
                    || (obj.get("language").is_some()
                        && obj.get("value").and_then(|v| v.as_str()).is_some()) =>
            {
                if let Some(value) = obj.get("value").and_then(|v| v.as_str()) {
                    let mut mapped = obj.clone();
                    mapped["value"] = Value::String(self.prepend_if_hit(value));
                    Value::Object(mapped)
                } else {
                    contents.clone()
                }
            }
            Value::String(text) => Value::String(self.prepend_if_hit(text)),
            Value::Array(items) => Value::Array(
                items
                    .iter()
                    .map(|it| self.enrich_hover_contents(it))
                    .collect(),
            ),
            _ => contents.clone(),
        }
    }

    /// 命中解释表时在文档上方插入加粗大白话行，否则原样返回
    fn prepend_if_hit(&self, doc: &str) -> String {
        match self.lookup_explanation(doc) {
            Some(hint) => format!("**大白话：{}**\n\n{}", hint, doc),
            None => doc.to_string(),
        }
    }

    /// 从 hover 文档提取候选解释键并查表（完整路径 → 类型::方法 → 方法名）
    fn lookup_explanation(&self, doc: &str) -> Option<String> {
        let (type_name, code_line) = extract_hover_parts(doc);
        let mut keys: Vec<String> = Vec::new();
        if let Some(line) = code_line {
            // 1. 完整路径形式（如 std::option::Option<T>::unwrap），清洗泛型参数
            if line.contains("::") && !line.contains("fn ") {
                let cleaned = clean_path_segments(&line);
                if cleaned.len() >= 2 {
                    keys.push(cleaned.join("::"));
                    // 降级匹配末两段（如 Option::unwrap），兼容短路径键数据
                    if cleaned.len() > 2 {
                        keys.push(cleaned[cleaned.len() - 2..].join("::"));
                    }
                }
            }
            // 2. 类型::方法（短路径，如 Option::unwrap）+ 3. 方法名兜底
            if let Some(name) = extract_fn_name(&line) {
                if let Some(ty) = &type_name {
                    keys.push(format!("{}::{}", ty, name));
                }
                if !keys.iter().any(|k| k == &name) {
                    keys.push(name);
                }
            }
        }
        let ui = crate::ui::global();
        keys.iter()
            .find_map(|k| ui.explanation(k).map(str::to_string))
    }

    /// 映射引用响应
    ///
    /// 引用响应是 Location[]；无结果时为 null，统一转为空数组。
    pub fn map_references_response(&self, response: &Value) -> Value {
        match response {
            Value::Null => Value::Array(Vec::new()),
            _ => self.map_definition_response(response), // 与定义跳转格式相同
        }
    }

    /// 映射重命名响应
    ///
    /// 处理跨文件重命名：
    /// - `changes`: { uri → [TextEdit] }
    /// - `documentChanges`: [TextDocumentEdit | ...]
    ///
    /// 将每个编辑的 range 映射回原始文件，并将 newText 反向翻译为母语。
    ///
    /// 编辑目标不是已打开 .zh 的虚拟文件时（如聚合模块的 main.rs、
    /// Cargo.toml）直接丢弃，避免客户端被引导编辑虚拟项目内部文件。
    pub fn map_rename_response(&self, response: &Value) -> Value {
        let mut result = response.clone();

        // 1. 处理 changes 形式
        if let Some(changes) = response.get("changes").and_then(|v| v.as_object()) {
            let mut mapped_changes = serde_json::Map::new();
            for (uri, edits_list) in changes {
                if !self.is_virtual_uri(uri) {
                    continue;
                }
                let target_uri = self.restore_uri(uri);
                let mapped_edits = self.map_edit_list(edits_list, uri, true);
                mapped_changes.insert(target_uri, mapped_edits);
            }
            result["changes"] = Value::Object(mapped_changes);
        }

        // 2. 处理 documentChanges 形式
        if let Some(doc_changes) = response.get("documentChanges").and_then(|v| v.as_array()) {
            let mapped_doc_changes: Vec<Value> = doc_changes
                .iter()
                .filter_map(|item| {
                    let uri = item
                        .get("textDocument")
                        .and_then(|td| td.get("uri"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !self.is_virtual_uri(uri) {
                        return None;
                    }
                    let mut mapped = item.clone();
                    mapped["textDocument"]["uri"] = Value::String(self.restore_uri(uri));
                    if let Some(edits) = item.get("edits") {
                        mapped["edits"] = self.map_edit_list(edits, uri, true);
                    }
                    Some(mapped)
                })
                .collect();
            result["documentChanges"] = Value::Array(mapped_doc_changes);
        }

        result
    }

    /// 映射代码操作响应
    ///
    /// LSP 规范中 `textDocument/codeAction` 响应是 `CodeAction[]` 数组，
    /// 每个操作可能携带 `edit`（WorkspaceEdit）。将编辑位置映射回原始文件，
    /// 插入的英文代码反向翻译为母语（与源文件语言保持一致）。
    pub fn map_code_action_response(&self, response: &Value, _original_uri: &str) -> Value {
        match response {
            Value::Array(actions) => {
                let mapped_actions: Vec<Value> = actions
                    .iter()
                    .map(|action| {
                        let mut mapped = action.clone();
                        if let Some(edit) = action.get("edit") {
                            mapped["edit"] = self.map_edit(edit, true);
                        }
                        mapped
                    })
                    .collect();
                Value::Array(mapped_actions)
            }
            Value::Null => Value::Array(Vec::new()),
            _ => response.clone(),
        }
    }

    /// 映射代码操作解析（codeAction/resolve）响应
    ///
    /// resolve 响应是单个 CodeAction 对象（非数组）：VSCode 点击操作后
    /// 解析懒加载的 edit。若不映射，edit 的 uri 是虚拟路径，
    /// VSCode 无法应用编辑（报 Request textDocument/codeAction failed）。
    pub fn map_code_action_resolve_response(&self, response: &Value) -> Value {
        match response {
            Value::Object(_) => {
                let mut mapped = response.clone();
                if let Some(edit) = response.get("edit") {
                    mapped["edit"] = self.map_edit(edit, true);
                }
                mapped
            }
            _ => response.clone(),
        }
    }

    /// 注入“添加依赖”快捷修复：未解析导入错误时提供一键 cargo add
    ///
    /// 命令由 VS Code 扩展注册（i18n-rust.cargoAdd）在工作区终端执行；
    /// 动作标题中的 crate 名反查母语别名（如 reqwest → HTTP客户端），
    /// 与母语源码的阅读体验保持一致。
    pub fn inject_add_dependency_actions(&self, response: &Value, crates: &[String]) -> Value {
        if crates.is_empty() {
            return response.clone();
        }
        let mut actions = match response {
            Value::Array(list) => list.clone(),
            _ => Vec::new(),
        };
        for crate_name in crates {
            // 英文 crate 名反查母语别名（关键字/别名表），未命中保持原名
            let display = self
                .reverse_lookup(crate_name)
                .unwrap_or_else(|| crate_name.clone());
            let title = crate::ui::global().f("lsp_action_add_dependency", &[&display]);
            actions.push(json!({
                "title": title,
                "kind": "quickfix",
                "command": {
                    "title": title,
                    "command": "i18n-rust.cargoAdd",
                    "arguments": [crate_name]
                }
            }));
        }
        Value::Array(actions)
    }

    /// 映射文档符号响应
    ///
    /// 将每个符号的 range 和 selectionRange 映射回原始文件，
    /// 并递归处理子符号。
    pub fn map_document_symbol_response(&self, response: &Value, original_uri: &str) -> Value {
        match response {
            Value::Array(array) => {
                let mapped: Vec<Value> = array
                    .iter()
                    .map(|symbol| self.map_single_symbol(symbol, original_uri))
                    .collect();
                Value::Array(mapped)
            }
            Value::Null => Value::Array(Vec::new()),
            _ => response.clone(),
        }
    }

    /// 递归映射单个文档符号
    fn map_single_symbol(&self, symbol: &Value, original_uri: &str) -> Value {
        let mut mapped = symbol.clone();

        // 将符号名反向恢复为中文（如 main → 主函数）
        if let Some(name) = symbol.get("name").and_then(|v| v.as_str())
            && let Some(zh_name) = self.reverse_lookup(name)
        {
            mapped["name"] = Value::String(zh_name);
        }

        if let Some(range) = symbol.get("range") {
            mapped["range"] = self.restore_range(original_uri, range);
        }
        if let Some(selection) = symbol.get("selectionRange") {
            mapped["selectionRange"] = self.restore_range(original_uri, selection);
        }
        if let Some(children) = symbol.get("children").and_then(|v| v.as_array()) {
            let mapped_children: Vec<Value> = children
                .iter()
                .map(|s| self.map_single_symbol(s, original_uri))
                .collect();
            mapped["children"] = Value::Array(mapped_children);
        }
        mapped
    }

    /// 从关键字映射中反向查找：英文 → 中文（预构建表，O(1)）
    fn reverse_lookup(&self, en_name: &str) -> Option<String> {
        self.reverse_map.get(en_name).cloned()
    }

    /// 将英文代码片段反向翻译为母语
    ///
    /// 精确命中关键字时直接替换；含 snippet 占位符（`${}`）的片段
    /// 保持原样（避免破坏占位符结构）；其余交给引擎词法级反向转译。
    fn translate_code(&self, text: &str) -> String {
        if let Some(zh) = self.reverse_lookup(text) {
            return zh;
        }
        if text.contains("${") {
            return text.to_string();
        }
        self.cache.reverse_transpile(text)
    }

    /// 映射一个 WorkspaceEdit（changes + documentChanges）
    ///
    /// 编辑目标不是已打开 .zh 的虚拟文件时直接丢弃。
    fn map_edit(&self, edit: &Value, translate_new_text: bool) -> Value {
        let mut mapped = edit.clone();

        // changes: { uri → [TextEdit] }
        if let Some(changes) = edit.get("changes").and_then(|v| v.as_object()) {
            let mut mapped_changes = serde_json::Map::new();
            for (uri, edits_list) in changes {
                if !self.is_virtual_uri(uri) {
                    continue;
                }
                let target_uri = self.restore_uri(uri);
                let mapped_edits = self.map_edit_list(edits_list, uri, translate_new_text);
                mapped_changes.insert(target_uri, mapped_edits);
            }
            mapped["changes"] = Value::Object(mapped_changes);
        }

        // documentChanges: [TextDocumentEdit]
        if let Some(doc_changes) = edit.get("documentChanges").and_then(|v| v.as_array()) {
            let mapped_doc_changes: Vec<Value> = doc_changes
                .iter()
                .filter_map(|item| {
                    let uri = item
                        .get("textDocument")
                        .and_then(|td| td.get("uri"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !self.is_virtual_uri(uri) {
                        return None;
                    }
                    let mut mapped = item.clone();
                    mapped["textDocument"]["uri"] = Value::String(self.restore_uri(uri));
                    if let Some(edits) = item.get("edits") {
                        mapped["edits"] = self.map_edit_list(edits, uri, translate_new_text);
                    }
                    Some(mapped)
                })
                .collect();
            mapped["documentChanges"] = Value::Array(mapped_doc_changes);
        }

        mapped
    }

    /// 映射一组 TextEdit 的位置
    ///
    /// 当 `translate_new_text` 为 true 时（重命名/代码操作/补全），
    /// 将 newText 反向翻译为母语；为 false 时 newText 保持英文。
    fn map_edit_list(
        &self,
        edits_list: &Value,
        virtual_uri: &str,
        translate_new_text: bool,
    ) -> Value {
        let mapped: Vec<Value> = edits_list
            .as_array()
            .map(|array| {
                array
                    .iter()
                    .map(|edit| {
                        let mut mapped_edit = edit.clone();
                        if let Some(range) = edit.get("range") {
                            mapped_edit["range"] = self.restore_range(virtual_uri, range);
                        }
                        if translate_new_text
                            && let Some(new_text) = edit.get("newText").and_then(|v| v.as_str())
                        {
                            mapped_edit["newText"] = Value::String(self.translate_code(new_text));
                        }
                        mapped_edit
                    })
                    .collect()
            })
            .unwrap_or_default();
        Value::Array(mapped)
    }
}

/// 判断字符串是否为合法 Rust 标识符片段（方法名/类型名，不含泛型）
fn is_ident(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c == '_' || c.is_alphabetic())
        && chars.all(|c| c == '_' || c.is_alphanumeric())
}

/// 从 hover 文档提取标题类型名与代码块首行
///
/// - 标题行：`**impl<T> Option<T>**` / `` **`Option<T>`** `` → 类型名 `Option`
/// - 代码行：第一个 ``` 代码块内的首个非空行（通常为签名或完整路径）
fn extract_hover_parts(doc: &str) -> (Option<String>, Option<String>) {
    let mut type_name = None;
    let mut code_line = None;
    let lines: Vec<&str> = doc.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if type_name.is_none() && t.starts_with("**") && t.ends_with("**") {
            let mut inner = t[2..t.len() - 2].replace('`', "");
            inner = inner.trim().to_string();
            let name = if let Some(rest) = inner.strip_prefix("impl") {
                // impl<...> X<...> for Y / impl str：跳过泛型段后取第一个标识符
                let mut rest = rest.trim();
                while rest.starts_with('<') {
                    let end = rest.find('>').map(|p| p + 1).unwrap_or(rest.len());
                    rest = &rest[end..];
                }
                rest.split('<').next().unwrap_or("").trim().to_string()
            } else {
                // **`Option<T>`** → Option
                inner.split('<').next().unwrap_or("").trim().to_string()
            };
            if is_ident(&name) {
                type_name = Some(name);
            }
        }
        if code_line.is_none() && t.starts_with("```") {
            for l in lines.iter().skip(i + 1) {
                let lt = l.trim();
                if lt.is_empty() {
                    continue;
                }
                if lt.starts_with("```") {
                    break;
                }
                code_line = Some(lt.to_string());
                break;
            }
        }
        if code_line.is_some() {
            break;
        }
    }
    // MarkedString 纯代码形态（无 ``` 围栏）：整段文本当作代码行
    if code_line.is_none() && !doc.lines().any(|l| l.trim().starts_with("```")) {
        let t = doc.trim();
        if !t.is_empty() {
            code_line = Some(t.to_string());
        }
    }
    (type_name, code_line)
}

/// 把完整路径行清洗为段列表（去掉泛型参数与空白）
///
/// `std::option::Option<T>::unwrap` → [std, option, Option, unwrap]
fn clean_path_segments(line: &str) -> Vec<String> {
    line.split("::")
        .map(|seg| seg.split('<').next().unwrap_or("").trim())
        .filter(|s| !s.is_empty())
        .filter(|s| is_ident(s))
        .map(str::to_string)
        .collect()
}

/// 从代码行提取方法名：路径末段（`a::b::c`）、签名（`fn name<...>(`）或宏（`macro_rules! name`）
fn extract_fn_name(code_line: &str) -> Option<String> {
    let line = code_line.trim();
    if line.contains("::") {
        if let Some(last) = line.rsplit("::").next() {
            let name = last.split(['(', '<', ' ']).next().unwrap_or("").trim();
            if is_ident(name) {
                return Some(name.to_string());
            }
        }
        return None;
    }
    if let Some(idx) = line.find("fn ") {
        let after = &line[idx + 3..];
        let name = after.split(['(', '<', ' ']).next().unwrap_or("").trim();
        if is_ident(name) {
            return Some(name.to_string());
        }
    }
    // 宏形式：macro_rules! select
    if let Some(idx) = line.find("macro_rules!") {
        let after = &line[idx + 12..];
        let name = after
            .split(['(', '<', ' ', '!'])
            .next()
            .unwrap_or("")
            .trim();
        if is_ident(name) {
            return Some(name.to_string());
        }
    }
    None
}

/// 提取补全 label 的末段标识符
///
/// rust-analyzer 的 label 可能带后缀/路径前缀，如
/// `foo(…)`、`Foo {…}`、`m::Spam::Bar(…)`、`m::`；
/// 此处提取末段标识符（如 `Bar`）用于用户词汇白名单匹配。
/// label 无标识符段（如纯符号）时返回 None。
fn label_identifier_suffix(label: &str) -> Option<&str> {
    // 去掉形如 `(…)`、`{…}` 的参数/字段后缀
    let head = label.split(['(', '{']).next().unwrap_or(label);
    // 去掉宏感叹号与模块补全的尾部 `::`（如 `m::`）
    let head = head.trim().trim_end_matches('!').trim_end_matches("::");
    let last = head.rsplit("::").next().unwrap_or("").trim();
    if last.is_empty() { None } else { Some(last) }
}

/// 根据翻译条目的行映射还原行号
fn restore_line_single(entry: &TranslationEntry, en_line: u32) -> u32 {
    let idx = en_line as usize;
    if idx < entry.line_map.len() {
        entry.line_map[idx]
    } else if let Some(&last) = entry.line_map.last() {
        last
    } else {
        en_line
    }
}

/// 从 LSP 诊断（rust-analyzer 格式）中提取所有权错误详情
///
/// 变量名取自原始消息中的反引号（如 use of moved value: `x`）；
/// 移动/借用/再次使用位置取自已还原到母语文件的 range 与 relatedInformation，
/// LSP 的 0-based 行号统一转为 1-based（与 rustc 诊断一致）。
/// 仅处理 E0382/E0502/E0507 及消息模式匹配的所有权错误。
fn extract_ownership_details(
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
fn translate_diagnostic_message(message: &str) -> String {
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

    // 2. 轻量短语替换（兜底）
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

    let mut result = replace_outside_backticks(message, &replacements);

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
fn is_main_fn_hint(diag: &Value, _virtual_uri: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_cache() -> (Arc<TranslationCache>, tempfile::TempDir) {
        let map = HashMap::from([("函数".into(), "fn".into()), ("让".into(), "let".into())]);
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(
            map,
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
            temp.path().to_path_buf(),
        );
        (cache, temp)
    }

    #[test]
    fn test_restore_uri() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());

        let (entry, _) = cache
            .update_document("file:///test/main.zh", "让 x = 1;", 1)
            .unwrap();

        assert_eq!(
            mapper.restore_uri(&entry.virtual_uri),
            "file:///test/main.zh"
        );
    }

    #[test]
    fn test_restore_line() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());

        let (entry, _) = cache
            .update_document("file:///test/main.zh", "让 x = 1;\n让 y = 2;", 1)
            .unwrap();

        assert_eq!(mapper.restore_line(&entry.virtual_uri, 0), 0);
        assert_eq!(mapper.restore_line(&entry.virtual_uri, 1), 1);
    }

    #[test]
    fn test_map_references_response() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        let (entry, _) = cache
            .update_document("file:///test/main.zh", "让 x = 1;\n函数 主() {}", 1)
            .unwrap();

        let response = json!([
            {
                "uri": entry.virtual_uri,
                "range": {
                    "start": { "line": 1, "character": 0 },
                    "end": { "line": 1, "character": 2 }
                }
            }
        ]);

        let mapped = mapper.map_references_response(&response);
        assert_eq!(mapped[0]["uri"].as_str().unwrap(), "file:///test/main.zh");
        assert_eq!(mapped[0]["range"]["start"]["line"], 1);
        assert_eq!(mapped[0]["range"]["start"]["character"], 0);

        // 无结果（null）时应返回空数组而不是 null
        assert_eq!(
            mapper.map_references_response(&Value::Null),
            Value::Array(Vec::new())
        );
    }

    #[test]
    fn test_map_rename_response_cross_file() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        let (entry_a, _) = cache
            .update_document("file:///test/main.zh", "让 x = 1;\n函数 主() {}", 1)
            .unwrap();
        let (entry_b, _) = cache
            .update_document("file:///test/lib.zh", "函数 主() {}", 1)
            .unwrap();

        // rust-analyzer 返回跨文件编辑（changes 形式）
        let response = json!({
            "changes": {
                entry_a.virtual_uri.clone(): [{
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 2 }
                    },
                    "newText": "fn"
                }],
                entry_b.virtual_uri.clone(): [{
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 2 }
                    },
                    "newText": "fn"
                }]
            }
        });

        let mapped = mapper.map_rename_response(&response);
        let changes = mapped["changes"].as_object().unwrap();

        // 两个文件的 URI 都还原为 .zh 源文件
        assert!(changes.contains_key("file:///test/main.zh"));
        assert!(changes.contains_key("file:///test/lib.zh"));

        // newText 反向翻译：fn → 函数
        assert_eq!(
            changes["file:///test/main.zh"][0]["newText"]
                .as_str()
                .unwrap(),
            "函数"
        );
        assert_eq!(
            changes["file:///test/lib.zh"][0]["newText"]
                .as_str()
                .unwrap(),
            "函数"
        );

        // documentChanges 形式也应正确处理
        let response2 = json!({
            "documentChanges": [{
                "textDocument": { "uri": entry_a.virtual_uri.clone(), "version": null },
                "edits": [{
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 2 }
                    },
                    "newText": "fn"
                }]
            }]
        });
        let mapped2 = mapper.map_rename_response(&response2);
        assert_eq!(
            mapped2["documentChanges"][0]["textDocument"]["uri"]
                .as_str()
                .unwrap(),
            "file:///test/main.zh"
        );
        assert_eq!(
            mapped2["documentChanges"][0]["edits"][0]["newText"]
                .as_str()
                .unwrap(),
            "函数"
        );
    }

    #[test]
    fn test_map_code_action_response() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        let (entry, _) = cache
            .update_document("file:///test/main.zh", "让 x = 1;", 1)
            .unwrap();

        let response = json!([
            {
                "title": "导入 std::io",
                "kind": "quickfix",
                "edit": {
                    "changes": {
                        entry.virtual_uri.clone(): [{
                            "range": {
                                "start": { "line": 0, "character": 0 },
                                "end": { "line": 0, "character": 2 }
                            },
                            "newText": "use std::io;"
                        }]
                    }
                }
            }
        ]);

        let mapped = mapper.map_code_action_response(&response, "file:///test/main.zh");
        let changes = mapped[0]["edit"]["changes"].as_object().unwrap();
        assert!(changes.contains_key("file:///test/main.zh"));

        // 代码操作插入的英文代码经反向翻译：测试映射表中无 use 关键字，
        // 故保持英文原样（若语言包含 使用→use 映射，则会被还原为母语）
        assert_eq!(
            changes["file:///test/main.zh"][0]["newText"]
                .as_str()
                .unwrap(),
            "use std::io;"
        );

        // null 响应 → 空数组
        assert_eq!(
            mapper.map_code_action_response(&Value::Null, ""),
            Value::Array(Vec::new())
        );
    }

    #[test]
    fn test_map_code_action_resolve_response() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        let (entry, _) = cache
            .update_document("file:///test/main.zh", "让 x = 1;", 1)
            .unwrap();

        // resolve 响应是单个 CodeAction 对象（非数组），edit 走 documentChanges
        let response = json!({
            "title": "改为 pub(crate)",
            "kind": "refactor.rewrite",
            "data": { "id": 1 },
            "edit": {
                "documentChanges": [{
                    "textDocument": { "uri": entry.virtual_uri, "version": 1 },
                    "edits": [{
                        "range": {
                            "start": { "line": 0, "character": 0 },
                            "end": { "line": 0, "character": 0 }
                        },
                        "newText": "pub(crate) "
                    }]
                }]
            }
        });

        let mapped = mapper.map_code_action_resolve_response(&response);
        // 虚拟 URI 必须还原为原始文件 URI（否则 VSCode 无法应用编辑）
        assert_eq!(
            mapped["edit"]["documentChanges"][0]["textDocument"]["uri"]
                .as_str()
                .unwrap(),
            "file:///test/main.zh"
        );
        // 位置从虚拟坐标映射回母语坐标
        let range = &mapped["edit"]["documentChanges"][0]["edits"][0]["range"];
        assert_eq!(range["start"]["line"], 0);
        assert_eq!(range["start"]["character"], 0);
        // data 与 title 原样保留
        assert_eq!(mapped["data"]["id"], 1);
        assert_eq!(mapped["title"].as_str().unwrap(), "改为 pub(crate)");

        // 无 edit 字段时原样返回
        let plain = json!({"title": "仅命令", "command": {"title": "c", "command": "x"}});
        assert_eq!(mapper.map_code_action_resolve_response(&plain), plain);
        // null 响应原样返回
        assert_eq!(
            mapper.map_code_action_resolve_response(&Value::Null),
            Value::Null
        );
    }

    #[test]
    fn test_map_document_symbol_response() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        let (entry, _) = cache
            .update_document("file:///test/main.zh", "函数 主() {\n    让 x = 1;\n}", 1)
            .unwrap();
        assert_eq!(entry.en_content, "fn 主() {\n    let x = 1;\n}");

        let response = json!([
            {
                "name": "fn",
                "kind": 12,
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 2, "character": 1 }
                },
                "selectionRange": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 2 }
                },
                "children": [{
                    "name": "let",
                    "kind": 13,
                    "range": {
                        "start": { "line": 1, "character": 0 },
                        "end": { "line": 1, "character": 3 }
                    },
                    "selectionRange": {
                        "start": { "line": 1, "character": 0 },
                        "end": { "line": 1, "character": 3 }
                    }
                }]
            }
        ]);

        let mapped = mapper.map_document_symbol_response(&response, "file:///test/main.zh");

        // 符号名恢复为中文
        assert_eq!(mapped[0]["name"].as_str().unwrap(), "函数");
        assert_eq!(mapped[0]["children"][0]["name"].as_str().unwrap(), "让");

        // 位置映射回母语文件（行 1:1、列按偏移转换）
        assert_eq!(mapped[0]["range"]["start"]["line"], 0);
        assert_eq!(mapped[0]["selectionRange"]["start"]["character"], 0);
        assert_eq!(mapped[0]["children"][0]["range"]["start"]["line"], 1);

        // null 响应 → 空数组
        assert_eq!(
            mapper.map_document_symbol_response(&Value::Null, ""),
            Value::Array(Vec::new())
        );
    }

    #[test]
    fn test_map_diagnostics_ownership_details_in_data() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        // 多行文档，保证行映射存在（诊断行号 2/4 可还原）
        let (entry, _) = cache
            .update_document(
                "file:///test/main.zh",
                "让 数据 = 1;\n让 a = 1;\n让 b = 1;\n让 c = 1;\n让 d = 1;",
                1,
            )
            .unwrap();

        // rust-analyzer 风格的 E0382 诊断：主 range 是再次使用处，relatedInformation 标记移动
        let diag = json!({
            "range": {
                "start": { "line": 4, "character": 4 },
                "end": { "line": 4, "character": 8 }
            },
            "severity": 1,
            "code": "E0382",
            "source": "rust-analyzer",
            "message": "use of moved value: `数据`",
            "relatedInformation": [{
                "location": {
                    "uri": entry.virtual_uri.clone(),
                    "range": {
                        "start": { "line": 2, "character": 8 },
                        "end": { "line": 2, "character": 10 }
                    }
                },
                "message": "value moved here"
            }]
        });
        let params = json!({
            "uri": entry.virtual_uri,
            "version": 1,
            "diagnostics": [diag]
        });

        let mapped = mapper.map_diagnostics(&params);
        let data = mapped["diagnostics"][0]["data"]
            .as_object()
            .expect("所有权诊断的 data 字段应为 JSON 对象");

        // 变量名与位置（LSP 0-based 行号 +1 → 1-based）
        assert_eq!(data["变量名"], "数据");
        assert_eq!(data["移动发生"]["起始行"], 3);
        assert_eq!(data["再次使用"]["起始行"], 5);
        assert!(data["借用发生"].is_null());
    }

    #[test]
    fn test_map_diagnostics_non_ownership_no_ownership_details() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        let (entry, _) = cache
            .update_document("file:///test/main.zh", "让 x = 1;", 1)
            .unwrap();

        // 类型不匹配错误不应附带所有权详情
        let diag = json!({
            "range": {
                "start": { "line": 0, "character": 4 },
                "end": { "line": 0, "character": 8 }
            },
            "severity": 1,
            "message": "mismatched types",
            "relatedInformation": []
        });
        let params = json!({
            "uri": entry.virtual_uri,
            "version": 1,
            "diagnostics": [diag]
        });

        let mapped = mapper.map_diagnostics(&params);
        assert!(mapped["diagnostics"][0].get("data").is_none());
    }

    /// documentHighlight 响应的 range 必须还原为母语坐标
    #[test]
    fn test_map_document_highlight_response() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        let (_entry, _) = cache
            .update_document("file:///test/main.zh", "让 x = 1;\n让 y = x;", 1)
            .unwrap();

        // 英文坐标（"let" 占 3 列；行0 x 在英文列 4，行1 "let y = " 后 x 在英文列 8）
        let response = json!([
            { "range": { "start": { "line": 0, "character": 4 }, "end": { "line": 0, "character": 5 } }, "kind": 2 },
            { "range": { "start": { "line": 1, "character": 8 }, "end": { "line": 1, "character": 9 } }, "kind": 2 }
        ]);
        let mapped = mapper.map_document_highlight_response(&response, "file:///test/main.zh");
        // 中文列："让 x" 中 x 在列 2（"让" 占 1 个 UTF-16 单元）；"让 y = x" 中 x 在列 6
        assert_eq!(mapped[0]["range"]["start"]["character"], 2);
        assert_eq!(mapped[1]["range"]["start"]["character"], 6);
        assert_eq!(mapped[0]["kind"], 2);

        // null 响应 → 空数组
        assert_eq!(
            mapper.map_document_highlight_response(&Value::Null, ""),
            Value::Array(Vec::new())
        );
    }

    /// 语义着色响应的 delta 编码必须还原为母语坐标，
    /// 且长度按映射后的列差重算（关键字替换改变列宽）
    #[test]
    fn test_map_semantic_tokens_response() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        let (_entry, _) = cache
            .update_document("file:///test/main.zh", "让 x = 1;\n让 y = x;", 1)
            .unwrap();

        // 英文坐标（"let" 占 3 列，变量 x/y 在列 4/8）；
        // delta 编码：[deltaLine, deltaStart, length, tokenType, tokenModifiers]
        let response = json!({
            "resultId": "abc",
            "data": [
                0, 0, 3, 14, 0,   // let   行0 列0
                0, 4, 1, 6, 0,    // x     行0 列4
                1, 4, 1, 6, 0,    // y     行1 列4（跨行，列重置为绝对）
                0, 4, 1, 6, 0     // x     行1 列8
            ]
        });
        let mapped = mapper.map_semantic_tokens_response(&response, "file:///test/main.zh");
        // resultId 透传；重新 delta 编码后的中文坐标：
        // 让(0,0,len1)、x(0,2)、y(1,2)、x(1,6)
        assert_eq!(mapped["resultId"], "abc");
        assert_eq!(
            mapped["data"],
            json!([0, 0, 1, 14, 0, 0, 2, 1, 6, 0, 1, 2, 1, 6, 0, 0, 4, 1, 6, 0])
        );
    }

    /// hover 命中解释表：完整路径降级匹配短路径键（std::option::Option<T>::unwrap）
    #[test]
    fn test_map_hover_response_full_path_hit() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        let doc =
            "```rust\nstd::option::Option<T>::unwrap\n```\n\nPanics if the value is a [`None`]...";
        let response = json!({"contents": {"kind": "markdown", "value": doc}});
        let mapped = mapper.map_hover_response(&response, "file:///test/main.zh");
        let value = mapped["contents"]["value"].as_str().unwrap();
        assert!(value.starts_with("**大白话："), "应插入加粗提示: {value}");
        assert!(
            value.contains("直接取出"),
            "解释应为 unwrap 的大白话: {value}"
        );
        assert!(value.ends_with(doc), "原文应保留在提示之后: {value}");
    }

    /// hover 命中解释表：impl 标题 + 签名行 → 短路径键（Option::unwrap）
    #[test]
    fn test_map_hover_response_impl_title_hit() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        let doc = "**`impl<T> Option<T>`**\n\n```rust\npub fn unwrap(self) -> T\n```\n\nPanics if the value is a None...";
        let response = json!({"contents": {"kind": "markdown", "value": doc}});
        let mapped = mapper.map_hover_response(&response, "file:///test/main.zh");
        let value = mapped["contents"]["value"].as_str().unwrap();
        assert!(value.starts_with("**大白话："), "应插入加粗提示: {value}");
        assert!(value.ends_with(doc));
    }

    /// hover 未命中解释表：原样透传，不改变任何内容
    #[test]
    fn test_map_hover_response_miss_unchanged() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        let doc = "```rust\npub fn 自定义函数(x: i32) -> i32\n```\n\n自定义函数说明";
        let response = json!({"contents": {"kind": "markdown", "value": doc}});
        let mapped = mapper.map_hover_response(&response, "file:///test/main.zh");
        assert_eq!(mapped["contents"]["value"].as_str().unwrap(), doc);
    }

    /// hover MarkedString 数组形式：简单键命中（clone），其余元素原样
    #[test]
    fn test_map_hover_response_marked_string_array_hit() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        let response = json!({
            "contents": [
                {"language": "rust", "value": "pub fn clone(&self) -> Self"},
                {"kind": "markdown", "value": "Returns a copy of the value."}
            ]
        });
        let mapped = mapper.map_hover_response(&response, "file:///test/main.zh");
        assert!(
            mapped["contents"][0]["value"]
                .as_str()
                .unwrap()
                .starts_with("**大白话：")
        );
        assert_eq!(
            mapped["contents"][1]["value"].as_str().unwrap(),
            "Returns a copy of the value."
        );
    }

    /// hover 内容为 null 等异常形态时安全透传
    #[test]
    fn test_map_hover_response_null_contents() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        let mapped = mapper.map_hover_response(&json!({"contents": null}), "");
        assert!(mapped["contents"].is_null());
    }

    /// 补全响应的 textEdit.newText 反向翻译、additionalTextEdits 位置还原
    #[test]
    fn test_map_completion_text_edit_reverse_translated() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        let (entry, _) = cache
            .update_document("file:///test/main.zh", "让 x = 1;", 1)
            .unwrap();

        let response = json!({
            "items": [{
                "label": "let",
                "textEdit": {
                    "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 3 } },
                    "newText": "let"
                },
                "additionalTextEdits": [{
                    "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } },
                    "newText": "fn 辅助() {}"
                }]
            }]
        });
        let mapped = mapper.map_completion_response(&response, "file:///test/main.zh");
        let item = &mapped["items"][0];
        // label 与 newText 均还原为母语关键字
        assert_eq!(item["label"].as_str().unwrap(), "让");
        assert_eq!(item["textEdit"]["newText"].as_str().unwrap(), "让");
        // snippet 占位符保持原样
        let snippet_item = json!({
            "items": [{ "label": "x", "textEdit": {
                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } },
                "newText": "fn ${1:name}() {}" } }]
        });
        let mapped_snippet = mapper.map_completion_response(&snippet_item, "file:///test/main.zh");
        assert_eq!(
            mapped_snippet["items"][0]["textEdit"]["newText"]
                .as_str()
                .unwrap(),
            "fn ${1:name}() {}"
        );
        // additionalTextEdits 中的 fn 被反向翻译
        assert_eq!(
            item["additionalTextEdits"][0]["newText"].as_str().unwrap(),
            "函数 辅助() {}"
        );
        let _ = entry;
    }

    /// labelDetails 反向翻译：VS Code 提示框右侧优先显示此字段，
    /// fn() 等英文签名必须还原为母语（如 函数()）
    #[test]
    fn test_map_completion_label_details_translated() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        // 文档中声明 my_func，使其通过语言过滤的用户词汇白名单
        let (entry, _) = cache
            .update_document("file:///test/main.zh", "函数 my_func() {}", 1)
            .unwrap();
        let response = json!({
            "items": [{
                "label": "my_func",
                "detail": "fn my_func()",
                "labelDetails": {
                    "description": "fn()",
                    "detail": "crate::辅助"
                }
            }]
        });
        let mapped = mapper.map_completion_response(&response, "file:///test/main.zh");
        let item = &mapped["items"][0];
        assert_eq!(item["detail"].as_str().unwrap(), "函数 my_func()");
        assert_eq!(
            item["labelDetails"]["description"].as_str().unwrap(),
            "函数()"
        );
        // crate/模块路径中无映射命中时保持原样
        assert_eq!(
            item["labelDetails"]["detail"].as_str().unwrap(),
            "crate::辅助"
        );
        let _ = entry;
    }

    /// 语言过滤：非英文方言下补全列表不得串语言。
    /// 保留：翻译命中项、母语字符项、用户自定义项（含英文命名）；
    /// 过滤：未翻译的外部英文项。
    #[test]
    fn test_map_completion_language_filter() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        // 用户源码同时含母语定义（自定义函数）与英文命名（helper）
        let (entry, _) = cache
            .update_document(
                "file:///test/main.zh",
                "函数 自定义函数() {} 函数 helper() {}",
                1,
            )
            .unwrap();

        let response = json!({
            "items": [
                { "label": "let" },
                { "label": "自定义函数" },
                { "label": "helper(…)" },
                { "label": "serde_json" },
                { "label": "BTreeMap" }
            ]
        });
        let mapped = mapper.map_completion_response(&response, "file:///test/main.zh");
        let labels: Vec<&str> = mapped["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["label"].as_str().unwrap())
            .collect();
        // 翻译命中（let → 让）、母语标识符、用户定义的英文名保留
        assert_eq!(labels, vec!["让", "自定义函数", "helper(…)"]);
        // 未翻译的外部英文项（serde_json/BTreeMap）被过滤
        assert!(!labels.contains(&"serde_json"));
        assert!(!labels.contains(&"BTreeMap"));
        let _ = entry;
    }

    /// 英文语言包（恒等映射）不启用语言过滤，所有项保留
    #[test]
    fn test_map_completion_no_filter_for_identity_pack() {
        let map = HashMap::from([("fn".into(), "fn".into()), ("let".into(), "let".into())]);
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(
            map,
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
            temp.path().to_path_buf(),
        );
        let mapper = ResponseMapper::new(cache);

        let response = json!({
            "items": [
                { "label": "let" },
                { "label": "BTreeMap" },
                { "label": "serde_json" }
            ]
        });
        let mapped = mapper.map_completion_response(&response, "file:///test/main.zh");
        assert_eq!(mapped["items"].as_array().unwrap().len(), 3);
    }

    /// label 末段标识符提取：后缀、路径前缀、模块补全形式
    #[test]
    fn test_label_identifier_suffix() {
        assert_eq!(label_identifier_suffix("foo(…)"), Some("foo"));
        assert_eq!(label_identifier_suffix("Foo {…}"), Some("Foo"));
        assert_eq!(label_identifier_suffix("m::Spam::Bar(…)"), Some("Bar"));
        assert_eq!(label_identifier_suffix("m::"), Some("m"));
        assert_eq!(label_identifier_suffix("println!"), Some("println"));
        assert_eq!(label_identifier_suffix("(…)"), None);
    }

    /// 注入添加依赖动作：空候选列表原样返回，非空追加 quickfix 动作
    #[test]
    fn test_inject_add_dependency_actions() {
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());

        // 空 crates：原响应原样返回
        let original = json!([{"title": "既有动作", "kind": "quickfix"}]);
        assert_eq!(
            mapper.inject_add_dependency_actions(&original, &[]),
            original
        );

        // 非空 crates：追加动作，command 指向扩展注册的 cargoAdd
        let injected = mapper.inject_add_dependency_actions(&original, &["serde_json".to_string()]);
        let actions = injected.as_array().unwrap();
        assert_eq!(actions.len(), 2);
        let added = &actions[1];
        assert_eq!(added["kind"], "quickfix");
        assert_eq!(added["command"]["command"], "i18n-rust.cargoAdd");
        assert_eq!(added["command"]["arguments"][0], "serde_json");

        // 非数组响应（如 null）也能注入
        let injected = mapper.inject_add_dependency_actions(&Value::Null, &["tokio".to_string()]);
        assert_eq!(injected.as_array().unwrap().len(), 1);
    }

    /// 未解析导入诊断追加依赖提示（内置 zh 回退含 lsp_hint_add_dependency 键）
    #[test]
    fn test_translate_diagnostic_unresolved_import_hint() {
        let translated = translate_diagnostic_message("unresolved import `serde_json`");
        // 反引号内容保留（提取依赖原名），且追加了 rzc add 提示
        assert!(translated.contains("`serde_json`"));
        assert!(translated.contains("rzc add serde_json"));
    }

    /// 诊断翻译不替换反引号内的标识符（避免误伤变量名中的子串）
    #[test]
    fn test_translate_diagnostic_skips_backtick_content() {
        let translated =
            translate_diagnostic_message("cannot find value `expected_value` in this scope");
        // 反引号内的标识符保持原样
        assert!(translated.contains("`expected_value`"));
        // 反引号外的短语已被翻译（不再含英文原短语）
        assert!(!translated.contains("cannot find value"));
    }
}
