//! AI 驱动的映射生成（DeepSeek，OpenAI 兼容接口）：
//! 只发送 API 英文名与类型签名（法律合规约束，见 mod.rs 模块文档）。

use anyhow::{anyhow, bail};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use super::ApiEntry;

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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_detect_system_language() {
        let lang = detect_system_language();
        assert!(
            lang == "zh" || lang == "ru",
            "应返回 zh 或 ru，实际: {}",
            lang
        );
    }
}
