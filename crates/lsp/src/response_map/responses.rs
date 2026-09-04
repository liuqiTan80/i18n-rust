//! 各 LSP 请求类型的响应映射：诊断、补全、悬停、签名帮助、
//! 引用/定义、重命名、代码操作、文档符号与教学动作注入。

use serde_json::{Value, json};

use super::ResponseMapper;
use super::diag_text::{extract_ownership_details, is_main_fn_hint, translate_diagnostic_message};
use super::restore_line_single;
use crate::translation_cache::en_col_to_zh_col_single;

impl ResponseMapper {
    /// 映射 rust-analyzer 的 publishDiagnostics 通知
    ///
    /// 将诊断信息中的 URI 和位置还原为原始 .zh 文件，
    /// 并尝试翻译诊断消息为中文。
    pub fn map_diagnostics(&self, params: &Value) -> Value {
        let virtual_uri = params["uri"].as_str().unwrap_or("");
        // 预取条目一次：所有诊断的 range/relatedInformation 映射基于同一
        // 文档，循环内复用条目走无锁纯函数（诊断通常 10-50 条，避免每条
        // range × 2 锁、每条 relatedInformation × 4 锁）
        let entry = self
            .cache
            .query_by_virtual_uri(virtual_uri)
            .or_else(|| self.cache.query_original(virtual_uri));
        let original_uri = match &entry {
            Some(e) => e.original_uri.clone(),
            None => virtual_uri.to_string(),
        };
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
                    mapped["range"] =
                        Self::restore_range_with_entry(entry.as_deref(), &diag["range"]);
                }

                // 映射 relatedInformation 中的位置
                if let Some(related_info) =
                    diag.get("relatedInformation").and_then(|v| v.as_array())
                {
                    let mut mapped_related = Vec::new();
                    for item in related_info {
                        let mut mapped_item = item.clone();
                        if let Some(location) = item.get("location") {
                            // 同文件位置复用主条目（跨文件才按需查询）
                            let loc_uri = location["uri"].as_str().unwrap_or("");
                            let loc_entry = if loc_uri == virtual_uri {
                                entry.clone()
                            } else {
                                self.cache
                                    .query_by_virtual_uri(loc_uri)
                                    .or_else(|| self.cache.query_original(loc_uri))
                            };
                            mapped_item["location"] = Self::restore_location_with_entry(
                                loc_entry.as_deref(),
                                loc_uri,
                                location,
                            );
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
            // 仅在确实遇到未翻译的纯英文项时才扫描一次；
            // Arc 共享避免每次补全请求克隆整个集合
            let mut user_tokens: Option<std::sync::Arc<std::collections::HashSet<String>>> = None;
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

        // 预取翻译条目：每个 token 的列映射都基于同一文档，
        // 只查一次（O(1) 索引），避免每个 token 重复锁与表扫描
        let entry = self
            .cache
            .query_by_virtual_uri(original_uri)
            .or_else(|| self.cache.query_original(original_uri));
        // 无条目（异常 URI）时位置原样透传
        let map_position = |line: u32, col: u32| -> (u32, u32) {
            match &entry {
                Some(e) => {
                    let zh_line = restore_line_single(e, line);
                    let zh_col = en_col_to_zh_col_single(e, line, col);
                    (zh_line, zh_col)
                }
                None => (line, col),
            }
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
        // 修饰符强制清零：rust-analyzer 对同一标识符在不同位置打不同修饰符
        //（声明处 declaration、宏参数内 macro 等），VS Code 对修饰符有独立
        // 颜色/样式（如宏参数蓝色、声明加粗），导致同一变量不同行颜色不一，
        // 干扰用户按颜色查找标识符。教学场景以类型色为准，修饰符信息放弃。
        let mut mapped: Vec<(u32, u32, u32, u32, u32)> = Vec::new();
        for (t_line, t_col, t_len, t_type, _t_mod) in tokens {
            let (zh_line, zh_start) = map_position(t_line, t_col);
            let (_, zh_end) = map_position(t_line, t_col.saturating_add(t_len));
            let zh_len = zh_end.saturating_sub(zh_start).max(1);
            mapped.push((zh_line, zh_start, zh_len, t_type, 0));
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

    /// 映射签名帮助响应
    ///
    /// 签名 label（如 `fn push(&mut self, value: T)`）与参数 label 做词法级
    /// 中文化（`fn` → `函数` 等）；参数 label 为 [start, end] 索引形式时按
    /// 原 label 提取文本翻译后转为字符串形式（VS Code 不再高亮参数，但
    /// 母语用户可读性优先——翻译前后长度变化无法保持索引）。
    pub fn map_signature_help_response(&self, response: &Value) -> Value {
        let mut result = response.clone();
        let Some(signatures) = result.get("signatures").and_then(|v| v.as_array()) else {
            return result;
        };
        let mapped: Vec<Value> = signatures
            .iter()
            .map(|sig| {
                let mut mapped_sig = sig.clone();
                let Some(label) = sig.get("label").and_then(|v| v.as_str()) else {
                    return mapped_sig;
                };
                let translated_label = self.translate_code(label);
                if let Some(params) = sig.get("parameters").and_then(|v| v.as_array()) {
                    let translated_params: Vec<Value> = params
                        .iter()
                        .map(|p| {
                            let mut mapped_param = p.clone();
                            if let Some(range) = p.get("label").and_then(|v| v.as_array()) {
                                // [start, end] 索引：原 label 为英文（ASCII），
                                // UTF-16 索引 == 字节索引，可直接切片
                                if let (Some(s), Some(e)) = (range[0].as_i64(), range[1].as_i64())
                                    && let Some(param_text) = label.get(s as usize..e as usize)
                                {
                                    mapped_param["label"] =
                                        Value::String(self.translate_code(param_text));
                                }
                            } else if let Some(param_label) =
                                p.get("label").and_then(|v| v.as_str())
                            {
                                mapped_param["label"] =
                                    Value::String(self.translate_code(param_label));
                            }
                            mapped_param
                        })
                        .collect();
                    mapped_sig["parameters"] = Value::Array(translated_params);
                }
                mapped_sig["label"] = Value::String(translated_label);
                mapped_sig
            })
            .collect();
        result["signatures"] = Value::Array(mapped);
        result
    }

    /// 给 hover contents 前置大白话提示（MarkupContent / MarkedString / 数 组）
    ///
    /// MarkedString（language 字段存在）是纯代码签名行：先按英文原文查
    /// 解释表加“大白话”前缀，再对整个文本做词法级中文化（`fn` → `函数`）；
    /// markdown 文本只查解释表，不翻译正文（正文可能含教学性中文说明）。
    fn enrich_hover_contents(&self, contents: &Value) -> Value {
        match contents {
            // MarkupContent：{"kind": "markdown", "value": ...}
            Value::Object(obj)
                if obj.get("kind").and_then(|k| k.as_str()) == Some("markdown")
                    && obj.get("value").and_then(|v| v.as_str()).is_some() =>
            {
                let mut mapped = obj.clone();
                // 守卫已确认 value 为字符串，get 回退空串仅为满足类型
                let value = obj.get("value").and_then(|v| v.as_str()).unwrap_or("");
                mapped["value"] = Value::String(self.prepend_if_hit(value));
                Value::Object(mapped)
            }
            // MarkedString：{"language": "rust", "value": 代码签名}
            Value::Object(obj)
                if obj.get("language").is_some()
                    && obj.get("value").and_then(|v| v.as_str()).is_some() =>
            {
                let mut mapped = obj.clone();
                // 守卫已确认 value 为字符串，get 回退空串仅为满足类型
                let value = obj.get("value").and_then(|v| v.as_str()).unwrap_or("");
                let hinted = self.prepend_if_hit(value);
                mapped["value"] = Value::String(self.translate_code(&hinted));
                Value::Object(mapped)
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

    /// 注入教学诊断的快捷修复：全角标点一键替换半角、教学 lint 忽略此行
    ///
    /// 教学诊断由代理自身发布（方言坐标），rust-analyzer 不会为其生成
    /// 修复动作，故在响应时按上下文诊断注入。编辑坐标直接使用诊断携带的
    /// 方言坐标（客户端应用编辑的文件即方言源文件）。
    pub fn inject_teaching_actions(
        &self,
        response: &Value,
        teaching_diags: &[Value],
        original_uri: &str,
    ) -> Value {
        if teaching_diags.is_empty() {
            return response.clone();
        }
        let mut actions = match response {
            Value::Array(list) => list.clone(),
            _ => Vec::new(),
        };
        let ui = crate::ui::global();
        for diag in teaching_diags {
            let code = diag["code"].as_str().unwrap_or("");
            if code == "fullwidth" {
                // 全角标点：可修复时提供替换动作；仅提示字符（顿号等）无动作
                let Some(replacement) = diag["data"]["replacement"].as_str().map(|s| s.to_string())
                else {
                    continue;
                };
                let character = diag["data"]["character"].as_str().unwrap_or("").to_string();
                let range = diag["range"].clone();
                let title = ui.f("lsp_action_fix_fullwidth", &[&character, &replacement]);
                actions.push(json!({
                    "title": title,
                    "kind": "quickfix",
                    "diagnostics": [diag],
                    "edit": {
                        "changes": {
                            original_uri: [{
                                "range": range,
                                "newText": replacement
                            }]
                        }
                    }
                }));
            } else if code.starts_with("lint-") {
                // 教学 lint：行尾插入忽略标记（教师标注故意不修的示例）
                let Some(line) = diag["range"]["start"]["line"].as_u64() else {
                    continue;
                };
                let Some(entry) = self.entry_for_original(original_uri) else {
                    continue;
                };
                let line_end = Self::line_end_utf16(&entry.zh_content, line as usize);
                let range = json!({
                    "start": { "line": line, "character": line_end },
                    "end": { "line": line, "character": line_end }
                });
                let title = ui.t("lsp_action_ignore_lint");
                actions.push(json!({
                    "title": title,
                    "kind": "quickfix",
                    "diagnostics": [diag],
                    "edit": {
                        "changes": {
                            original_uri: [{
                                "range": range,
                                "newText": format!("  // {}", i18n_rust_engine::lint::IGNORE_MARK)
                            }]
                        }
                    }
                }));
            }
        }
        Value::Array(actions)
    }

    /// 计算指定行（0 起）行尾的 UTF-16 字符偏移；行不存在时回退 0
    fn line_end_utf16(content: &str, line: usize) -> u32 {
        content
            .lines()
            .nth(line)
            .map(|l| l.encode_utf16().count() as u32)
            .unwrap_or(0)
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
pub(super) fn label_identifier_suffix(label: &str) -> Option<&str> {
    // 去掉形如 `(…)`、`{…}` 的参数/字段后缀
    let head = label.split(['(', '{']).next().unwrap_or(label);
    // 去掉宏感叹号与模块补全的尾部 `::`（如 `m::`）
    let head = head.trim().trim_end_matches('!').trim_end_matches("::");
    let last = head.rsplit("::").next().unwrap_or("").trim();
    if last.is_empty() { None } else { Some(last) }
}
