//! AI 驱动的映射生成（DeepSeek，OpenAI 兼容接口）：
//! 只发送 API 英文名与类型签名（法律合规约束，见 mod.rs 模块文档）。

use anyhow::{anyhow, bail};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use super::接口条目;

/// AI 请求超时配置（秒）
const 连接超时: u64 = 30;
/// 单次 AI 调用最多处理的 API 条目数：长映射（数百条目）单次调用会超
/// 模型输出上限，按批调用后合并结果
const 批量大小: usize = 50;
/// AI system 提示语（中文）：面向 zh 语言包，生成中文名 + 中文解释
const 中文提示词: &str = "你是面向 Rust 新手的教学翻译专家。任务：把第三方 crate 的公开 API 翻译成中文教学映射。\n\
    输入：API 英文名 + 类型签名列表。你只能依据名称和类型签名推测含义，\n\
    禁止使用、翻译或复制任何官方文档内容。\n\
    输出：严格 TOML 格式，仅两个节：\n\
    [\"标识符\"]\n\"中文名\" = \"英文名\"\n\
    [\"解释\"]\n\"中文名\" = \"一句不超过 40 字的大白话解释\"\n\
    要求：中文名直观好记；解释说明这个 API 是干什么的、怎么用，面向新手；\n\
    不能确定的条目省略；只输出 TOML，不要输出任何其他文字。";
/// AI system 提示语（英文）：面向非 zh 语言包，名称保持英文，生成英文新手解释
/// （TOML 节名必须保持 \"标识符\" / \"解释\"，与解析器及映射文件格式兼容）
const 英文提示词: &str = "You are a teaching translation expert for Rust beginners. Task: produce a teaching mapping for the public API of a third-party crate.\n\
    Input: a list of API English names + type signatures. Infer meaning from names and signatures only;\n\
    never use, translate, or copy any official documentation content.\n\
    Output: strict TOML with exactly two sections:\n\
    [\"标识符\"]\n\"name\" = \"EnglishName\" (keep the names as-is; only include entries you are sure about)\n\
    [\"解释\"]\n\"name\" = \"one plain-language explanation under 40 words for beginners\"\n\
    Requirements: the explanation must say what the API does and how to use it, in simple English;\n\
    omit entries you cannot determine; output only TOML and nothing else.";
const 读取超时: u64 = 120;
const 写入超时: u64 = 60;

/// 调用 DeepSeek chat 接口的通用底层：发送 system+user 提示词，返回模型文本
///
/// 供映射生成（调用智能生成映射）与脚手架翻译
/// （mapping_check::run_scaffold --provider deepseek）共用。
pub fn 深度求索对话(系统提示词: &str, 用户提示词: &str) -> anyhow::Result<String> {
    let 界面 = crate::ui::界面::全局();
    let 接口密钥 = std::env::var("DEEPSEEK_API_KEY")
        .map_err(|_| anyhow!("{}", 界面.取文("mg_err_no_api_key")))?;
    if 接口密钥.is_empty() {
        bail!("{}", 界面.取文("mg_err_api_key_empty"));
    }
    let 基础地址 = std::env::var("DEEPSEEK_BASE_URL")
        .unwrap_or_else(|_| "https://api.deepseek.com".to_string());
    let 请求地址 = format!("{}/chat/completions", 基础地址.trim_end_matches('/'));
    let 请求体 = serde_json::json!({
        "model": "deepseek-chat",
        "messages": [
            { "role": "system", "content": 系统提示词 },
            { "role": "user", "content": 用户提示词 }
        ],
        "temperature": 0.2,
        // 输出上限 8K：分批后单批输出仍可能接近默认 4K 上限
        "max_tokens": 8000,
        "stream": false
    });
    let 代理 = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(连接超时)))
        .timeout_global(Some(std::time::Duration::from_secs(读取超时 + 写入超时)))
        .build()
        .new_agent();
    let 响应 = 代理
        .post(&请求地址)
        .header("Content-Type", "application/json")
        .header("Authorization", &format!("Bearer {}", 接口密钥))
        .send(请求体.to_string())
        .map_err(|首错| {
            anyhow!(
                "{}",
                界面.取文带参("mg_err_ai_request", &[&首错.to_string()])
            )
        })?;
    if 响应.status() != 200 {
        bail!(
            "{}",
            界面.取文带参("mg_err_ai_status", &[&响应.status().to_string()])
        );
    }
    // 读取上限：被控/故障端点的响应若无界，`read_to_string` 会持续累积
    // 直到进程 OOM（timeout 只限制时长，不限制字节数）。
    // 8 MiB 远超单批键名翻译的合理响应（数百条短键），仅作安全上限。
    const 响应上限: u64 = 8 * 1024 * 1024;
    let mut 响应体 = 响应.into_body();
    let 响应文本 = 响应体
        .with_config()
        .limit(响应上限)
        .read_to_string()
        .map_err(|首错| anyhow!("{}", 界面.取文带参("mg_err_ai_read", &[&首错.to_string()])))?;
    let 响应解析: Value = serde_json::from_str(&响应文本)
        .map_err(|首错| anyhow!("{}", 界面.取文带参("mg_err_ai_parse", &[&首错.to_string()])))?;
    响应解析
        .get("choices")
        .and_then(|候选| 候选.get(0))
        .and_then(|消息| 消息.get("message"))
        .and_then(|消息| 消息.get("content"))
        .and_then(Value::as_str)
        .map(|子| 子.to_string())
        .ok_or_else(|| anyhow!("{}", 界面.取文("mg_err_ai_no_content")))
}

/// 构建单批请求的 user 提示词（crate 名 + 该批 API 列表）
fn 构建用户提示(库名: &str, 语言: &str, 批次: &[接口条目]) -> String {
    let 接口清单 = 批次
        .iter()
        .map(|项| format!("- {} {}", 项.种类.显示名(), 项.类型签名))
        .collect::<Vec<_>>()
        .join("\n");
    if 语言 == "zh" {
        format!(
            "crate: {}\n公开 API 列表（名称 + 类型签名）：\n{}",
            库名, 接口清单
        )
    } else {
        format!(
            "crate: {}\nPublic API list (name + type signature):\n{}",
            库名, 接口清单
        )
    }
}

/// 调用 DeepSeek 生成中文名与解释，返回 (中文名→英文名, 中文名→解释)
///
/// 只发送 API 英文名与类型签名；长映射按 [`批量大小`] 分批调用后合并
/// （单次调用的输出会超模型上限）；任一批失败整体返回 Err，由上层回退规则模式。
pub fn 调用智能生成映射(
    库名: &str,
    语言: &str,
    条目表: &[接口条目],
) -> anyhow::Result<(HashMap<String, String>, HashMap<String, String>)> {
    let 系统提示词 = if 语言 == "zh" {
        中文提示词
    } else {
        英文提示词
    };
    let 界面 = crate::ui::界面::全局();
    let 批次表: Vec<&[接口条目]> = 条目表.chunks(批量大小).collect();
    let mut 标识符表 = HashMap::new();
    let mut 解释表 = HashMap::new();
    for (序号, 批次) in 批次表.iter().enumerate() {
        println!(
            "{}",
            界面.取文带参(
                "mg_ai_batch_progress",
                &[&(序号 + 1).to_string(), &批次表.len().to_string()]
            )
        );
        let 用户提示词 = 构建用户提示(库名, 语言, 批次);
        let 内容 = 深度求索对话(系统提示词, &用户提示词)?;
        // 只接受本批条目中的英文名（防止 AI 幻觉改名）
        let 合法英文名: HashSet<String> = 批次.iter().map(|项| 项.英文原名.clone()).collect();
        let (本批标识符, 本批解释) = 解析智能结果(&内容, &合法英文名)?;
        标识符表.extend(本批标识符);
        解释表.extend(本批解释);
    }
    Ok((标识符表, 解释表))
}

/// 解析 AI 返回的 TOML 文本为 (中文名→英文名, 中文名→解释)；
/// 英文名不在合法集合中的条目丢弃（防止 AI 幻觉改名）
///
/// 严格 TOML 解析失败时（AI 偶发输出重复节头等不规范内容）回退逐行宽松
/// 扫描抢救可辨识条目；抢救不到任何内容时仍返回原始解析错误。
pub fn 解析智能结果(
    相关文本: &str,
    合法英文名: &HashSet<String>,
) -> anyhow::Result<(HashMap<String, String>, HashMap<String, String>)> {
    // 清洗：去掉 ```toml 围栏与前后杂讯，只保留第一个 [ 开始、结尾围栏之前的部分
    let 起点 = 相关文本
        .find('[')
        .ok_or_else(|| anyhow!("{}", crate::ui::界面::全局().取文("mg_err_ai_no_toml")))?;
    let 终点 = 相关文本[起点..]
        .find("```")
        .map(|偏移| 起点 + 偏移)
        .unwrap_or(相关文本.len());
    let 文本段 = &相关文本[起点..终点];
    let 表值 = match toml::from_str::<toml::Value>(文本段) {
        Ok(表值) => 表值,
        Err(严格错误) => {
            let 抢救 = 宽松解析节(文本段);
            if 抢救.is_empty() {
                return Err(anyhow!(
                    "{}",
                    crate::ui::界面::全局()
                        .取文带参("mg_err_ai_toml_parse", &[&严格错误.to_string()])
                ));
            }
            toml::Value::Table(抢救)
        }
    };
    Ok(从表取节(&表值, 合法英文名))
}

/// 从 {标识符, 解释} 表过滤合法条目：标识符需英文名在合法集合内，
/// 解释逐条校验长度（超 40 字打印警告）；严格与宽松两条解析路径共用
fn 从表取节(
    表值: &toml::Value,
    合法英文名: &HashSet<String>,
) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut 标识符表 = HashMap::new();
    let mut 解释表 = HashMap::new();
    if let Some(节) = 表值.get("标识符").and_then(toml::Value::as_table) {
        for (中文名, 目标值) in 节 {
            if let Some(英文名) = 目标值.as_str()
                && 合法英文名.contains(英文名)
                && !中文名.is_empty()
            {
                标识符表.insert(中文名.clone(), 英文名.to_string());
            }
        }
    }
    if let Some(节) = 表值.get("解释").and_then(toml::Value::as_table) {
        for (中文名, 目标值) in 节 {
            if let Some(解释文本) = 目标值.as_str()
                && !解释文本.is_empty()
            {
                if 解释文本.chars().count() > 40 {
                    eprintln!(
                        "{}",
                        crate::ui::界面::全局().取文带参(
                            "mg_warn_ai_explain_long",
                            &[&解释文本.chars().count().to_string(), 中文名]
                        )
                    );
                }
                解释表.insert(中文名.clone(), 解释文本.to_string());
            }
        }
    }
    (标识符表, 解释表)
}

/// 宽松逐行扫描 AI 输出：识别含"标识符"/"解释"的节头（容忍重复出现，
/// 同名节内容合并），节内解析 `"键" = "值"` 行；返回重新组装的表值
/// （仅含实际出现的节），供与严格解析相同的过滤路径复用
fn 宽松解析节(文本段: &str) -> toml::value::Table {
    let mut 节: Option<&str> = None;
    let mut 标识符表 = toml::value::Table::new();
    let mut 解释表 = toml::value::Table::new();
    for 源行 in 文本段.lines() {
        let 修剪 = 源行.trim();
        if 修剪.starts_with('[') {
            // 节头判定放宽到"包含"：容忍多余空格/引号/围栏残缺
            节 = if 修剪.contains("标识符") {
                Some("标识符")
            } else if 修剪.contains("解释") {
                Some("解释")
            } else {
                None
            };
            continue;
        }
        let Some(当前节) = 节 else {
            continue;
        };
        let Some((键名, 目标值)) = 切分键值(修剪) else {
            continue;
        };
        if 当前节 == "标识符" {
            标识符表.insert(键名, toml::Value::String(目标值));
        } else {
            解释表.insert(键名, toml::Value::String(目标值));
        }
    }
    let mut 表值 = toml::value::Table::new();
    if !标识符表.is_empty() {
        表值.insert("标识符".to_string(), toml::Value::Table(标识符表));
    }
    if !解释表.is_empty() {
        表值.insert("解释".to_string(), toml::Value::Table(解释表));
    }
    表值
}

/// 解析 `"键" = "值"` 行；无 `=` 或键/值为空时返回 None
fn 切分键值(源行: &str) -> Option<(String, String)> {
    let (原始键, 原始值) = 源行.split_once('=')?;
    Some((提取片段(原始键)?, 提取片段(原始值)?))
}

/// 提取片段内容：优先取引号（半/全角）内文本，未加引号时取 `#` 前文本；
/// 内容为空返回 None
fn 提取片段(原始: &str) -> Option<String> {
    let 修剪 = 原始.trim();
    for (开符, 闭符) in [('"', '"'), ('\'', '\''), ('“', '”'), ('「', '」')] {
        if let Some(剥离) = 修剪.strip_prefix(开符) {
            let 内部 = 剥离.split_once(闭符).map_or(剥离, |(前, _)| 前).trim();
            return (!内部.is_empty()).then(|| 内部.to_string());
        }
    }
    let 裸文本 = 修剪.split('#').next().unwrap_or(修剪).trim();
    (!裸文本.is_empty()).then(|| 裸文本.to_string())
}

/// 检测系统语言（用于 --lang 缺省值）：完整支持全部内置语言，默认 "zh"
///
/// 实现见 [`crate::ui::检测系统语言`]（读取 LC_ALL / LC_MESSAGES / LANG）。
pub fn 检测系统语言() -> String {
    crate::ui::检测系统语言()
}

#[cfg(test)]
mod 单元测试 {
    use super::*;
    use crate::mapping_gen::接口种类;

    /// 解析 AI 结果：合法条目保留、幻觉条目丢弃
    #[test]
    fn 测试解析智能结果() {
        let 合法: HashSet<String> = ["new", "Error", "Context"]
            .iter()
            .map(|子| 子.to_string())
            .collect();
        let 样本 = "好的，以下是映射：\n```toml\n[\"标识符\"]\n\"新建\" = \"new\"\n\"错误\" = \"Error\"\n\"幻觉\" = \"NotExist\"\n\n[\"解释\"]\n\"新建\" = \"创建一个新的实例，非常简单\"\n\"错误\" = \"描述错误信息的类型\"\n```\n";
        let (ident, 解释) = 解析智能结果(样本, &合法).expect("应能解析");
        assert!(ident.contains_key("新建"));
        assert!(ident.contains_key("错误"));
        // 幻觉条目（英文名不在合法集合）被丢弃
        assert!(!ident.contains_key("幻觉"));
        assert_eq!(
            解释.get("新建").map(String::as_str),
            Some("创建一个新的实例，非常简单")
        );
    }

    /// 分批请求的提示词只包含本批条目，语言开关选择正确模板
    #[test]
    fn 测试构建用户提示分批() {
        let 条目表 = vec![
            接口条目 {
                种类: 接口种类::函数项,
                英文原名: "new".to_string(),
                类型签名: "fn new() -> Self".to_string(),
            },
            接口条目 {
                种类: 接口种类::结构项,
                英文原名: "Window".to_string(),
                类型签名: "struct Window".to_string(),
            },
        ];
        let 中 = 构建用户提示("tauri", "zh", &条目表);
        assert!(中.contains("crate: tauri"));
        assert!(中.contains("fn new() -> Self"));
        assert!(中.contains("struct Window"));
        let 单条 = 构建用户提示("tauri", "zh", &条目表[..1]);
        assert!(单条.contains("fn new() -> Self"));
        assert!(!单条.contains("struct Window"), "不应包含其他批的条目");
        let 英 = 构建用户提示("tauri", "ru", &条目表[..1]);
        assert!(英.contains("Public API list"));
    }

    #[test]
    fn 测试智能结果非文本报错() {
        let 合法 = HashSet::new();
        assert!(解析智能结果("抱歉，我无法完成。", &合法).is_err());
    }

    /// AI 偶发重复节头（两次 ["解释"] 等）：严格 TOML 解析必然失败，
    /// 宽松扫描应合并重复节并抢救出全部条目
    #[test]
    fn 测试解析智能结果重复节() {
        let 合法: HashSet<String> = ["new", "Error"].iter().map(|子| 子.to_string()).collect();
        let 样本 = "```toml\n[\"标识符\"]\n\"新建\" = \"new\"\n[\"标识符\"]\n\"错误\" = \"Error\"\n[\"解释\"]\n\"新建\" = \"创建实例\"\n[\"解释\"]\n\"错误\" = \"错误类型\"\n```\n";
        let (ident, 解释) = 解析智能结果(样本, &合法).expect("宽松解析应能抢救");
        assert_eq!(ident.len(), 2, "重复标识符节应合并");
        assert_eq!(
            解释.get("错误").map(String::as_str),
            Some("错误类型"),
            "重复解释节应合并"
        );
    }

    /// 完全无法抢救（无可用节与键值对）时保留严格解析的原始错误
    #[test]
    fn 测试解析智能结果不可解析保留错误() {
        let 合法 = HashSet::new();
        let 样本 = "[标题]\n- 列表项，没有任何键值对\n";
        let 报错 = 解析智能结果(样本, &合法).expect_err("应保留解析错误");
        assert!(
            报错.to_string().contains("TOML parse error"),
            "应含 toml crate 原始错误: {}",
            报错
        );
    }

    #[test]
    fn 测试检测系统语言() {
        // 系统语言完全由环境变量决定：临时接管清除 LC_ALL / LC_MESSAGES / LANG
        // 后逐个设定确定性断言。直接读真实环境断言会因 CI 平台 locale 差异
        // 假失败（macos runner 预设 en，此前断言 zh/ru 即因此挂掉）
        let _环境锁 = crate::lang_manager::单元测试::环境锁();
        let _环境恢复 =
            crate::lang_manager::单元测试::环境恢复::接管(
                &["LC_ALL", "LC_MESSAGES", "LANG"],
            );
        unsafe {
            std::env::set_var("LANG", "zh_CN.UTF-8");
        }
        assert_eq!(检测系统语言(), "zh", "zh_CN 应识别为 zh");
        unsafe {
            std::env::set_var("LANG", "ru_RU.UTF-8");
        }
        assert_eq!(检测系统语言(), "ru", "ru_RU 应识别为 ru");
        unsafe {
            std::env::remove_var("LANG");
        }
        assert_eq!(检测系统语言(), "zh", "无区域设置应回退默认 zh");
    }
}
