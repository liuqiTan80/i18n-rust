//! AI 驱动的映射生成（DeepSeek，OpenAI 兼容接口）：
//! 只发送 API 英文名与类型签名（法律合规约束，见 mod.rs 模块文档）。

use anyhow::{anyhow, bail};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use super::ApiEntry;

/// AI 请求超时配置（秒）
const AI_CONNECT_TIMEOUT: u64 = 30;
/// 单次 AI 调用最多处理的 API 条目数：长映射（数百条目）单次调用会超
/// 模型输出上限，按批调用后合并结果
const AI_BATCH_SIZE: usize = 50;
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
        // 输出上限 8K：分批后单批输出仍可能接近默认 4K 上限
        "max_tokens": 8000,
        "stream": false
    });
    let agent = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(AI_CONNECT_TIMEOUT)))
        .timeout_global(Some(std::time::Duration::from_secs(
            AI_READ_TIMEOUT + AI_WRITE_TIMEOUT,
        )))
        .build()
        .new_agent();
    let resp = agent
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", &format!("Bearer {}", api_key))
        .send(request_body.to_string())
        .map_err(|e| anyhow!("{}", ui.f("mg_err_ai_request", &[&e.to_string()])))?;
    if resp.status() != 200 {
        bail!(
            "{}",
            ui.f("mg_err_ai_status", &[&resp.status().to_string()])
        );
    }
    let resp_text = resp
        .into_body()
        .read_to_string()
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

/// 构建单批请求的 user 提示词（crate 名 + 该批 API 列表）
fn build_user_prompt(crate_name: &str, lang: &str, batch: &[ApiEntry]) -> String {
    let api_list = batch
        .iter()
        .map(|e| format!("- {} {}", e.kind.display(), e.signature))
        .collect::<Vec<_>>()
        .join("\n");
    if lang == "zh" {
        format!(
            "crate: {}\n公开 API 列表（名称 + 类型签名）：\n{}",
            crate_name, api_list
        )
    } else {
        format!(
            "crate: {}\nPublic API list (name + type signature):\n{}",
            crate_name, api_list
        )
    }
}

/// 调用 DeepSeek 生成中文名与解释，返回 (中文名→英文名, 中文名→解释)
///
/// 只发送 API 英文名与类型签名；长映射按 [`AI_BATCH_SIZE`] 分批调用后合并
/// （单次调用的输出会超模型上限）；任一批失败整体返回 Err，由上层回退规则模式。
pub fn call_ai_generate_mapping(
    crate_name: &str,
    lang: &str,
    entries: &[ApiEntry],
) -> anyhow::Result<(HashMap<String, String>, HashMap<String, String>)> {
    let system_prompt = if lang == "zh" {
        AI_PROMPT_ZH
    } else {
        AI_PROMPT_EN
    };
    let ui = crate::ui::Ui::global();
    let batches: Vec<&[ApiEntry]> = entries.chunks(AI_BATCH_SIZE).collect();
    let mut identifier_map = HashMap::new();
    let mut explanation_map = HashMap::new();
    for (index, batch) in batches.iter().enumerate() {
        println!(
            "{}",
            ui.f(
                "mg_ai_batch_progress",
                &[&(index + 1).to_string(), &batches.len().to_string()]
            )
        );
        let user_prompt = build_user_prompt(crate_name, lang, batch);
        let content = deepseek_chat(system_prompt, &user_prompt)?;
        // 只接受本批条目中的英文名（防止 AI 幻觉改名）
        let valid_english_names: HashSet<String> =
            batch.iter().map(|e| e.english_name.clone()).collect();
        let (batch_identifiers, batch_explanations) =
            parse_ai_result(&content, &valid_english_names)?;
        identifier_map.extend(batch_identifiers);
        explanation_map.extend(batch_explanations);
    }
    Ok((identifier_map, explanation_map))
}

/// 解析 AI 返回的 TOML 文本为 (中文名→英文名, 中文名→解释)；
/// 英文名不在合法集合中的条目丢弃（防止 AI 幻觉改名）
///
/// 严格 TOML 解析失败时（AI 偶发输出重复节头等不规范内容）回退逐行宽松
/// 扫描抢救可辨识条目；抢救不到任何内容时仍返回原始解析错误。
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
    let table = match toml::from_str::<toml::Value>(toml_part) {
        Ok(table) => table,
        Err(strict_err) => {
            let rescued = lenient_parse_sections(toml_part);
            if rescued.is_empty() {
                return Err(anyhow!(
                    "{}",
                    crate::ui::Ui::global().f("mg_err_ai_toml_parse", &[&strict_err.to_string()])
                ));
            }
            toml::Value::Table(rescued)
        }
    };
    Ok(sections_from_table(&table, valid_english_names))
}

/// 从 {标识符, 解释} 表过滤合法条目：标识符需英文名在合法集合内，
/// 解释逐条校验长度（超 40 字打印警告）；严格与宽松两条解析路径共用
fn sections_from_table(
    table: &toml::Value,
    valid_english_names: &HashSet<String>,
) -> (HashMap<String, String>, HashMap<String, String>) {
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
    (identifier_map, explanation_map)
}

/// 宽松逐行扫描 AI 输出：识别含"标识符"/"解释"的节头（容忍重复出现，
/// 同名节内容合并），节内解析 `"键" = "值"` 行；返回重新组装的表值
/// （仅含实际出现的节），供与严格解析相同的过滤路径复用
fn lenient_parse_sections(toml_part: &str) -> toml::value::Table {
    let mut section: Option<&str> = None;
    let mut identifiers = toml::value::Table::new();
    let mut explanations = toml::value::Table::new();
    for line in toml_part.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            // 节头判定放宽到"包含"：容忍多余空格/引号/围栏残缺
            section = if trimmed.contains("标识符") {
                Some("标识符")
            } else if trimmed.contains("解释") {
                Some("解释")
            } else {
                None
            };
            continue;
        }
        let Some(current) = section else {
            continue;
        };
        let Some((key, value)) = split_key_value(trimmed) else {
            continue;
        };
        if current == "标识符" {
            identifiers.insert(key, toml::Value::String(value));
        } else {
            explanations.insert(key, toml::Value::String(value));
        }
    }
    let mut table = toml::value::Table::new();
    if !identifiers.is_empty() {
        table.insert("标识符".to_string(), toml::Value::Table(identifiers));
    }
    if !explanations.is_empty() {
        table.insert("解释".to_string(), toml::Value::Table(explanations));
    }
    table
}

/// 解析 `"键" = "值"` 行；无 `=` 或键/值为空时返回 None
fn split_key_value(line: &str) -> Option<(String, String)> {
    let (raw_key, raw_value) = line.split_once('=')?;
    Some((extract_fragment(raw_key)?, extract_fragment(raw_value)?))
}

/// 提取片段内容：优先取引号（半/全角）内文本，未加引号时取 `#` 前文本；
/// 内容为空返回 None
fn extract_fragment(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    for (open, close) in [('"', '"'), ('\'', '\''), ('“', '”'), ('「', '」')] {
        if let Some(stripped) = trimmed.strip_prefix(open) {
            let inner = stripped
                .split_once(close)
                .map_or(stripped, |(before, _)| before)
                .trim();
            return (!inner.is_empty()).then(|| inner.to_string());
        }
    }
    let plain = trimmed.split('#').next().unwrap_or(trimmed).trim();
    (!plain.is_empty()).then(|| plain.to_string())
}

/// 检测系统语言（用于 --lang 缺省值）：完整支持全部内置语言，默认 "zh"
///
/// 实现见 [`crate::ui::detect_system_language`]（读取 LC_ALL / LC_MESSAGES / LANG）。
pub fn detect_system_language() -> String {
    crate::ui::detect_system_language()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping_gen::ApiKind;

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

    /// 分批请求的提示词只包含本批条目，语言开关选择正确模板
    #[test]
    fn test_build_user_prompt_per_batch() {
        let entries = vec![
            ApiEntry {
                kind: ApiKind::Function,
                english_name: "new".to_string(),
                signature: "fn new() -> Self".to_string(),
            },
            ApiEntry {
                kind: ApiKind::Struct,
                english_name: "Window".to_string(),
                signature: "struct Window".to_string(),
            },
        ];
        let zh = build_user_prompt("tauri", "zh", &entries);
        assert!(zh.contains("crate: tauri"));
        assert!(zh.contains("fn new() -> Self"));
        assert!(zh.contains("struct Window"));
        let single = build_user_prompt("tauri", "zh", &entries[..1]);
        assert!(single.contains("fn new() -> Self"));
        assert!(!single.contains("struct Window"), "不应包含其他批的条目");
        let en = build_user_prompt("tauri", "ru", &entries[..1]);
        assert!(en.contains("Public API list"));
    }

    #[test]
    fn test_ai_result_non_toml_error() {
        let valid = HashSet::new();
        assert!(parse_ai_result("抱歉，我无法完成。", &valid).is_err());
    }

    /// AI 偶发重复节头（两次 ["解释"] 等）：严格 TOML 解析必然失败，
    /// 宽松扫描应合并重复节并抢救出全部条目
    #[test]
    fn test_parse_ai_result_duplicate_sections() {
        let valid: HashSet<String> = ["new", "Error"].iter().map(|s| s.to_string()).collect();
        let text = "```toml\n[\"标识符\"]\n\"新建\" = \"new\"\n[\"标识符\"]\n\"错误\" = \"Error\"\n[\"解释\"]\n\"新建\" = \"创建实例\"\n[\"解释\"]\n\"错误\" = \"错误类型\"\n```\n";
        let (identifiers, explanations) = parse_ai_result(text, &valid).expect("宽松解析应能抢救");
        assert_eq!(identifiers.len(), 2, "重复标识符节应合并");
        assert_eq!(
            explanations.get("错误").map(String::as_str),
            Some("错误类型"),
            "重复解释节应合并"
        );
    }

    /// 完全无法抢救（无可用节与键值对）时保留严格解析的原始错误
    #[test]
    fn test_parse_ai_result_unparseable_keeps_error() {
        let valid = HashSet::new();
        let text = "[标题]\n- 列表项，没有任何键值对\n";
        let err = parse_ai_result(text, &valid).expect_err("应保留解析错误");
        assert!(
            err.to_string().contains("TOML parse error"),
            "应含 toml crate 原始错误: {}",
            err
        );
    }

    #[test]
    fn test_detect_system_language() {
        // 系统语言完全由环境变量决定：临时接管清除 LC_ALL / LC_MESSAGES / LANG
        // 后逐个设定确定性断言。直接读真实环境断言会因 CI 平台 locale 差异
        // 假失败（macos runner 预设 en，此前断言 zh/ru 即因此挂掉）
        let _lock = crate::lang_manager::tests::env_lock();
        let _env = crate::lang_manager::tests::EnvRestore::take(&["LC_ALL", "LC_MESSAGES", "LANG"]);
        unsafe {
            std::env::set_var("LANG", "zh_CN.UTF-8");
        }
        assert_eq!(detect_system_language(), "zh", "zh_CN 应识别为 zh");
        unsafe {
            std::env::set_var("LANG", "ru_RU.UTF-8");
        }
        assert_eq!(detect_system_language(), "ru", "ru_RU 应识别为 ru");
        unsafe {
            std::env::remove_var("LANG");
        }
        assert_eq!(detect_system_language(), "zh", "无区域设置应回退默认 zh");
    }
}
