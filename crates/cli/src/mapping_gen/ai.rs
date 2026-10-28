//! AI 驱动的映射生成（OpenAI 兼容接口，默认端点 DeepSeek）：
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

/// 默认端点与模型（DeepSeek，OpenAI 兼容协议）
const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
const DEFAULT_MODEL: &str = "deepseek-chat";

/// AI 端点配置：三项与 VS Code 扩展的 `i18n-rust.ai.baseUrl / apiKey / model`
/// 设置一一对应，CLI 侧走环境变量（无图形界面与 CI 场景要能脚本化）
#[derive(Debug, PartialEq, Eq)]
pub struct AiEndpoint {
    /// chat/completions 的基础 URL（不含 `/chat/completions` 后缀）
    pub base_url: String,
    /// Bearer 密钥，只用于请求头，绝不输出
    pub api_key: String,
    /// 请求体的 model 字段
    pub model: String,
    /// 实际生效的密钥环境变量名（诊断文案回显，避免用户查错变量）
    pub key_var: &'static str,
    /// 端点是否被用户改成自定义地址（决定密钥回退链）
    pub custom: bool,
}

/// 解析 AI 端点配置（`get` 注入环境变量读取，单测不碰真实环境）
///
/// 首选 `RZ_AI_BASE_URL / RZ_AI_API_KEY / RZ_AI_MODEL`，未设置时回退历史
/// DeepSeek 专用变量。密钥回退有一条安全约束：默认端点是 DeepSeek，只有当用户
/// 显式改成自定义端点时才考虑 `OPENAI_API_KEY`——否则会把用户的 OpenAI 密钥
/// 发往另一台主机（拿到一个 401，账单却可能记在 OpenAI 侧）。
fn resolve_endpoint(get: &dyn Fn(&str) -> Option<String>) -> anyhow::Result<AiEndpoint> {
    let ui = crate::ui::Ui::global();
    // 空串视为未设置：CI 里 `export RZ_AI_BASE_URL=` 不应把端点变成空 URL
    let read = |name: &str| {
        get(name)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let base_url = read("RZ_AI_BASE_URL")
        .or_else(|| read("DEEPSEEK_BASE_URL"))
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let custom = base_url != DEFAULT_BASE_URL;
    let model = read("RZ_AI_MODEL").unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let chain: &[&'static str] = if custom {
        &["RZ_AI_API_KEY", "OPENAI_API_KEY", "DEEPSEEK_API_KEY"]
    } else {
        &["RZ_AI_API_KEY", "DEEPSEEK_API_KEY"]
    };
    let mut empty_var: Option<&'static str> = None;
    for &name in chain {
        match get(name) {
            Some(value) if value.trim().is_empty() => {
                empty_var.get_or_insert(name);
            }
            Some(value) => {
                return Ok(AiEndpoint {
                    base_url,
                    api_key: value.trim().to_string(),
                    model,
                    key_var: name,
                    custom,
                });
            }
            None => {}
        }
    }
    match empty_var {
        // 密钥存在但为空串：点名是哪个变量，不让用户逐个试
        Some(name) => bail!("{}", ui.f("mg_err_api_key_empty", &[name])),
        None => bail!("{}", ui.f("mg_err_no_api_key", &[chain[0]])),
    }
}

/// 拼出 chat/completions 完整 URL
///
/// 扩展侧 `i18n-rust.ai.baseUrl` 习惯写 `https://…/v1`，而 CLI 沿用的默认值
/// 是不带 `/v1` 的裸主机名；这里统一为“已带后缀就原样用，否则补一段”，
/// 两侧粘同一个 URL 都能跑。
pub fn chat_url(base: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

/// AI 模式的前置凭据检查：在写任何文件之前确认密钥可用
///
/// `mapping scaffold --provider <ai>` 是「生成骨架 + AI 填充」两步，但没配密钥时
/// 不该先留下几十个待手工翻译的骨架文件——那会让「命令失败」与「文件已被改动」
/// 同时成立。只跑 [`resolve_endpoint`]，不发网络请求。
pub fn ensure_credentials() -> anyhow::Result<()> {
    resolve_endpoint(&|name| std::env::var(name).ok()).map(|_| ())
}

/// `--provider` 参数的归一化结果
#[derive(Debug, PartialEq, Eq)]
pub enum Provider {
    /// 调用 OpenAI 兼容端点翻译
    Ai,
    /// 离线规则模式（不联网）
    Rule,
}

/// 解析 `--provider`：`deepseek` / `openai` / `compatible` 走同一条 OpenAI
/// 兼容链路（区别只在环境变量里的端点与模型），`rule` 为离线模式。
///
/// 保留 `deepseek` 作为默认写法（已有脚本与文档都在用），但名字不再绑死厂商。
pub fn parse_provider(value: &str) -> Option<Provider> {
    match value {
        "deepseek" | "openai" | "compatible" => Some(Provider::Ai),
        "rule" => Some(Provider::Rule),
        _ => None,
    }
}

/// 调用 OpenAI 兼容 chat/completions 接口的通用底层：发送 system+user 提示词，返回模型文本
///
/// 端点、密钥、模型由 [`resolve_endpoint`] 决定。供映射生成
/// （call_ai_generate_mapping）与脚手架翻译
/// （mapping_check::run_scaffold --provider）共用。
pub fn ai_chat(system_prompt: &str, user_prompt: &str) -> anyhow::Result<String> {
    let ui = crate::ui::Ui::global();
    let endpoint = resolve_endpoint(&|name| std::env::var(name).ok())?;
    let url = chat_url(&endpoint.base_url);
    // 端点只回显一次：分批调用逐批打印会淹没进度；只回显密钥变量名，不碰密钥本身
    static ENDPOINT_REPORTED: std::sync::Once = std::sync::Once::new();
    ENDPOINT_REPORTED.call_once(|| {
        println!(
            "{}",
            ui.f("mg_ai_endpoint", &[&url, &endpoint.model, endpoint.key_var])
        );
    });
    let request_body = serde_json::json!({
        "model": endpoint.model,
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
        .header("Authorization", &format!("Bearer {}", endpoint.api_key))
        .send(request_body.to_string())
        .map_err(|e| anyhow!("{}", ui.f("mg_err_ai_request", &[&e.to_string()])))?;
    if resp.status() != 200 {
        bail!(
            "{}",
            ui.f("mg_err_ai_status", &[&resp.status().to_string()])
        );
    }
    // 读取上限：被控/故障端点的响应若无界，`read_to_string` 会持续累积
    // 直到进程 OOM（timeout 只限制时长，不限制字节数）。
    // 8 MiB 远超单批键名翻译的合理响应（数百条短键），仅作安全上限。
    const 响应上限: u64 = 8 * 1024 * 1024;
    let mut body = resp.into_body();
    let resp_text = body
        .with_config()
        .limit(响应上限)
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

/// 调用 OpenAI 兼容端点生成中文名与解释，返回 (中文名→英文名, 中文名→解释)
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
        let content = ai_chat(system_prompt, &user_prompt)?;
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

    /// 环境变量假象器：把键值表包成 `get` 闭包，单测不碰真实进程环境
    /// （进程级 env 改动会与并行其它测试互相干扰）
    fn 假环境(条目: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let 表: std::collections::HashMap<String, String> = 条目
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |键| 表.get(键).cloned()
    }

    /// 默认端点下的配置与密钥回退链
    #[test]
    fn test_resolve_endpoint_default_deepseek() {
        let 端点 = resolve_endpoint(&假环境(&[("DEEPSEEK_API_KEY", "sk-d")])).unwrap();
        assert_eq!(端点.base_url, DEFAULT_BASE_URL);
        assert_eq!(端点.model, DEFAULT_MODEL);
        assert_eq!(端点.api_key, "sk-d");
        assert_eq!(端点.key_var, "DEEPSEEK_API_KEY");
        assert!(!端点.custom);

        // RZ_AI_* 优先于历史变量
        let 端点 = resolve_endpoint(&假环境(&[
            ("RZ_AI_API_KEY", "sk-rz"),
            ("DEEPSEEK_API_KEY", "sk-d"),
            ("RZ_AI_MODEL", "deepseek-reasoner"),
        ]))
        .unwrap();
        assert_eq!(端点.api_key, "sk-rz");
        assert_eq!(端点.key_var, "RZ_AI_API_KEY");
        assert_eq!(端点.model, "deepseek-reasoner");

        // 用旧变量名配的自定义端点同样算自定义（custom 按最终 URL 判定，不按变量名）
        let 端点 = resolve_endpoint(&假环境(&[
            ("DEEPSEEK_BASE_URL", "https://mirror.example/v1"),
            ("OPENAI_API_KEY", "sk-o"),
        ]))
        .unwrap();
        assert!(端点.custom);
        assert_eq!(端点.key_var, "OPENAI_API_KEY");
    }

    /// 安全约束：默认端点绝不读 OPENAI_API_KEY（不把别家密钥发往 DeepSeek）
    #[test]
    fn test_resolve_endpoint_does_not_leak_openai_key() {
        let 错 = resolve_endpoint(&假环境(&[("OPENAI_API_KEY", "sk-o")])).unwrap_err();
        assert!(
            错.to_string().contains("RZ_AI_API_KEY"),
            "应提示主密钥变量：{错}"
        );
        // 自定义端点才启用 OPENAI_API_KEY，且优先于 DEEPSEEK_API_KEY
        let 端点 = resolve_endpoint(&假环境(&[
            ("RZ_AI_BASE_URL", "https://api.openai.com/v1"),
            ("OPENAI_API_KEY", "sk-o"),
            ("DEEPSEEK_API_KEY", "sk-d"),
        ]))
        .unwrap();
        assert!(端点.custom);
        assert_eq!(端点.api_key, "sk-o");
        assert_eq!(端点.key_var, "OPENAI_API_KEY");
        assert_eq!(
            chat_url(&端点.base_url),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    /// 密钥/端点为空串时点名变量，不拿空值去请求
    #[test]
    fn test_resolve_endpoint_names_offending_var() {
        let 错 = resolve_endpoint(&假环境(&[("RZ_AI_API_KEY", "")])).unwrap_err();
        assert!(错.to_string().contains("RZ_AI_API_KEY"), "{错}");
        // 空串不阻断回退：主变量为空、历史变量可用时照旧能跑
        let 端点 = resolve_endpoint(&假环境(&[
            ("RZ_AI_API_KEY", ""),
            ("DEEPSEEK_API_KEY", "sk-d"),
        ]))
        .unwrap();
        assert_eq!(端点.key_var, "DEEPSEEK_API_KEY");
        // 空白端点视为未设置，不拼出只补后缀的废 URL
        let 端点 = resolve_endpoint(&假环境(&[
            ("RZ_AI_BASE_URL", "   "),
            ("DEEPSEEK_API_KEY", "sk-d"),
        ]))
        .unwrap();
        assert_eq!(端点.base_url, DEFAULT_BASE_URL);
    }

    /// chat/completions 拼接：带 /v1 与不带、已带后缀、结尾斜杠均能跑
    #[test]
    fn test_chat_url_forms() {
        assert_eq!(
            chat_url("https://api.deepseek.com"),
            "https://api.deepseek.com/chat/completions"
        );
        assert_eq!(
            chat_url("http://localhost:11434/v1/"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            chat_url("https://x.example/v1/chat/completions"),
            "https://x.example/v1/chat/completions"
        );
    }

    /// --provider 别名归一：三种写法同一条链路，rule 离线，其余报 None
    #[test]
    fn test_parse_provider_aliases() {
        for 别名 in ["deepseek", "openai", "compatible"] {
            assert_eq!(parse_provider(别名), Some(Provider::Ai), "{别名}");
        }
        assert_eq!(parse_provider("rule"), Some(Provider::Rule));
        assert_eq!(parse_provider("deeepseek"), None);
        assert_eq!(parse_provider(""), None);
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
