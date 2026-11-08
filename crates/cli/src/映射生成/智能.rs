//! AI 驱动的映射生成（OpenAI 兼容接口，默认端点 DeepSeek）：
//! 只发送 API 英文名与类型签名（法律合规约束，见 mod.rs 模块文档）。

use serde_json::Value;
use std::collections::{HashMap, HashSet};

use super::接口条目;

/// AI 请求超时配置（秒）
const 智能连接超时: u64 = 30;
/// 单次 AI 调用最多处理的 API 条目数：长映射（数百条目）单次调用会超
/// 模型输出上限，按批调用后合并结果
const 智能批大小: usize = 50;
/// AI 系统提示语（中文）：面向 zh 语言包，生成中文名 + 中文解释
const 智能提示中文: &str = "你是面向 Rust 新手的教学翻译专家。任务：把第三方 crate 的公开 API 翻译成中文教学映射。\n\
    输入：API 英文名 + 类型签名列表。你只能依据名称和类型签名推测含义，\n\
    禁止使用、翻译或复制任何官方文档内容。\n\
    输出：严格 TOML 格式，仅两个节：\n\
    [\"标识符\"]\n\"中文名\" = \"英文名\"\n\
    [\"解释\"]\n\"中文名\" = \"一句不超过 40 字的大白话解释\"\n\
    要求：中文名直观好记；解释说明这个 API 是干什么的、怎么用，面向新手；\n\
    不能确定的条目省略；只输出 TOML，不要输出任何其他文字。";
/// AI 系统提示语（英文）：面向非 zh 语言包，名称保持英文，生成英文新手解释
/// （TOML 节名必须保持 \"标识符\" / \"解释\"，与解析器及映射文件格式兼容）
const 智能提示英文: &str = "You are a teaching translation expert for Rust beginners. Task: produce a teaching mapping for the public API of a third-party crate.\n\
    Input: a list of API English names + type signatures. Infer meaning from names and signatures only;\n\
    never use, translate, or copy any official documentation content.\n\
    Output: strict TOML with exactly two sections:\n\
    [\"标识符\"]\n\"name\" = \"EnglishName\" (keep the names as-is; only include entries you are sure about)\n\
    [\"解释\"]\n\"name\" = \"one plain-language explanation under 40 words for beginners\"\n\
    Requirements: the explanation must say what the API does and how to use it, in simple English;\n\
    omit entries you cannot determine; output only TOML and nothing else.";
const 智能读取超时: u64 = 120;
const 智能写入超时: u64 = 60;

/// 默认端点与模型（DeepSeek，OpenAI 兼容协议）
const 默认基础地址: &str = "https://api.deepseek.com";
const 默认模型: &str = "deepseek-chat";

/// AI 端点配置：三项与 VS Code 扩展的 `i18n-rust.ai.baseUrl / apiKey / model`
/// 设置一一对应，CLI 侧走环境变量（无图形界面与 CI 场景要能脚本化）
#[derive(Debug, PartialEq, Eq)]
pub struct 智能端点<'甲> {
    /// chat/completions 的基础 URL（不含 `/chat/completions` 后缀）
    pub 基础地址: String,
    /// Bearer 密钥，只用于请求头，绝不输出
    pub 接口密钥: String,
    /// 请求体的模型字段
    pub 模型: String,
    /// 实际生效的密钥环境变量名（诊断文案回显，避免用户查错变量）
    pub 密钥变量名: &'甲 str,
    /// 端点是否被用户改成自定义地址（决定密钥回退链）
    pub 自定义: bool,
}

/// 解析 AI 端点配置（环境变量读取函数由调用方注入，单测不碰真实环境）
///
/// 首选 `RZ_AI_BASE_URL / RZ_AI_API_KEY / RZ_AI_MODEL`，未设置时回退历史
/// DeepSeek 专用变量。密钥回退有一条安全约束：默认端点是 DeepSeek，只有当用户
/// 显式改成自定义端点时才考虑 `OPENAI_API_KEY`——否则会把用户的 OpenAI 密钥
/// 发往另一台主机（拿到一个 401，账单却可能记在 OpenAI 侧）。
fn 解析端点<'甲>(
    取值函数: &dyn Fn(&str) -> Option<String>,
) -> anyhow::Result<智能端点<'甲>> {
    let 界面表 = crate::界面::界面消息表::全局表();
    // 空串视为未设置：CI 里 `export RZ_AI_BASE_URL=` 不应把端点变成空 URL
    let 读取配置 = |变量名: &str| {
        取值函数(变量名)
            .map(|值文本| 值文本.trim().to_string())
            .filter(|值文本| !值文本.is_empty())
    };
    let 基础地址 = 读取配置("RZ_AI_BASE_URL")
        .or_else(|| 读取配置("DEEPSEEK_BASE_URL"))
        .unwrap_or_else(|| 默认基础地址.to_string());
    let 自定义 = 基础地址 != 默认基础地址;
    let 模型 = 读取配置("RZ_AI_MODEL").unwrap_or_else(|| 默认模型.to_string());
    let 链: &[&'甲 str] = if 自定义 {
        &["RZ_AI_API_KEY", "OPENAI_API_KEY", "DEEPSEEK_API_KEY"]
    } else {
        &["RZ_AI_API_KEY", "DEEPSEEK_API_KEY"]
    };
    let mut 空变量: Option<&'甲 str> = None;
    for &变量名 in 链 {
        match 取值函数(变量名) {
            Some(值文本) if 值文本.trim().is_empty() => {
                空变量.get_or_insert(变量名);
            }
            Some(值文本) => {
                return Ok(智能端点 {
                    基础地址,
                    接口密钥: 值文本.trim().to_string(),
                    模型,
                    密钥变量名: 变量名,
                    自定义,
                });
            }
            None => {}
        }
    }
    match 空变量 {
        // 密钥存在但为空串：点名是哪个变量，不让用户逐个试
        Some(变量名) => {
            anyhow::bail!("{}", 界面表.取带参消息("mg_err_api_key_empty", &[变量名]))
        }
        None => anyhow::bail!("{}", 界面表.取带参消息("mg_err_no_api_key", &[链[0]])),
    }
}

/// 拼出 chat/completions 完整 URL
///
/// 编辑器扩展习惯把端点写到版本段，而 CLI 沿用的默认值
/// 是不带 `/v1` 的裸主机名；这里统一为“已带后缀就原样用，否则补一段”，
/// 两侧粘同一个 URL 都能跑。
pub fn 对话地址(基础文本: &str) -> String {
    let 裁剪文本 = 基础文本.trim_end_matches('/');
    if 裁剪文本.ends_with("/chat/completions") {
        裁剪文本.to_string()
    } else {
        format!("{裁剪文本}/chat/completions")
    }
}

/// AI 模式的前置凭据检查：在写任何文件之前确认密钥可用
///
/// `mapping scaffold --provider <ai>` 是「生成骨架 + AI 填充」两步，但没配密钥时
/// 不该先留下几十个待手工翻译的骨架文件——那会让「命令失败」与「文件已被改动」
/// 同时成立。只跑 [`解析端点`]，不发网络请求。
pub fn 确保凭据() -> anyhow::Result<()> {
    解析端点(&|变量名| std::env::var(变量名).ok()).map(|_| ())
}

/// `--provider` 参数的归一化结果
#[derive(Debug, PartialEq, Eq)]
pub enum 服务商 {
    /// 调用 OpenAI 兼容端点翻译
    智能,
    /// 离线规则模式（不联网）
    规则,
}

/// 解析服务商选项：深度求索 / 开放人工智能 / 兼容端点走同一条 OpenAI
/// 兼容链路（区别只在环境变量里的端点与模型），规则为离线模式。
///
/// 保留深度求索作为默认写法（已有脚本与文档都在用），但名字不再绑死厂商。
pub fn 解析服务商(值文本: &str) -> Option<服务商> {
    match 值文本 {
        "deepseek" | "openai" | "compatible" => Some(服务商::智能),
        "rule" => Some(服务商::规则),
        _ => None,
    }
}

/// 调用 OpenAI 兼容对话补全接口的通用底层：发送系统与用户提示词，返回模型文本
///
/// 端点、密钥、模型由 [`解析端点`] 决定。供映射生成
/// （调用智能生成映射）与脚手架翻译
/// （映射检查::运行脚手架的服务商选项）共用。
pub fn 智能对话(系统提示: &str, 用户提示: &str) -> anyhow::Result<String> {
    let 界面表 = crate::界面::界面消息表::全局表();
    let 端点 = 解析端点(&|变量名| std::env::var(变量名).ok())?;
    let 地址 = 对话地址(&端点.基础地址);
    // 端点只回显一次：分批调用逐批打印会淹没进度；只回显密钥变量名，不碰密钥本身
    static 端点已报告: std::sync::Once = std::sync::Once::new();
    端点已报告.call_once(|| {
        println!(
            "{}",
            界面表.取带参消息("mg_ai_endpoint", &[&地址, &端点.模型, 端点.密钥变量名])
        );
    });
    let 请求体 = serde_json::json!({
        "model": 端点.模型,
        "messages": [
            { "role": "system", "content": 系统提示 },
            { "role": "user", "content": 用户提示 }
        ],
        "temperature": 0.2,
        // 输出上限 8K：分批后单批输出仍可能接近默认 4K 上限
        "max_tokens": 8000,
        "stream": false
    });
    let 代理 = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(智能连接超时)))
        .timeout_global(Some(std::time::Duration::from_secs(
            智能读取超时 + 智能写入超时,
        )))
        .build()
        .new_agent();
    let 响应 = 代理
        .post(&地址)
        .header("Content-Type", "application/json")
        .header("Authorization", &format!("Bearer {}", 端点.接口密钥))
        .send(请求体.to_string())
        .map_err(|错误值| {
            anyhow::anyhow!(
                "{}",
                界面表.取带参消息("mg_err_ai_request", &[&错误值.to_string()])
            )
        })?;
    if 响应.status() != 200 {
        anyhow::bail!(
            "{}",
            界面表.取带参消息("mg_err_ai_status", &[&响应.status().to_string()])
        );
    }
    // 读取上限：被控/故障端点的响应若无界，读取全部字符串会持续累积
    // 直到进程内存耗尽（超时只限制时长，不限制字节数）。
    // 8 MiB 远超单批键名翻译的合理响应（数百条短键），仅作安全上限。
    const 响应上限: u64 = 8 * 1024 * 1024;
    let mut 主体 = 响应.into_body();
    let 响应文本 = 主体
        .with_config()
        .limit(响应上限)
        .read_to_string()
        .map_err(|错误值| {
            anyhow::anyhow!(
                "{}",
                界面表.取带参消息("mg_err_ai_read", &[&错误值.to_string()])
            )
        })?;
    let 响应文档: Value = serde_json::from_str(&响应文本).map_err(|错误值| {
        anyhow::anyhow!(
            "{}",
            界面表.取带参消息("mg_err_ai_parse", &[&错误值.to_string()])
        )
    })?;
    响应文档
        .get("choices")
        .and_then(|选择值| 选择值.get(0))
        .and_then(|消息值| 消息值.get("message"))
        .and_then(|消息值| 消息值.get("content"))
        .and_then(Value::as_str)
        .map(|文本值| 文本值.to_string())
        .ok_or_else(|| anyhow::anyhow!("{}", 界面表.取消息("mg_err_ai_no_content")))
}

/// 构建单批请求的用户提示词（包名 + 该批接口列表）
fn 构建用户提示(包名: &str, 语言代码: &str, 批: &[接口条目]) -> String {
    let 接口列表 = 批
        .iter()
        .map(|条目值| format!("- {} {}", 条目值.种类.取展示名(), 条目值.签名))
        .collect::<Vec<_>>()
        .join("\n");
    if 语言代码 == "zh" {
        format!(
            "crate: {}\n公开 API 列表（名称 + 类型签名）：\n{}",
            包名, 接口列表
        )
    } else {
        format!(
            "crate: {}\nPublic API list (name + type signature):\n{}",
            包名, 接口列表
        )
    }
}

/// 调用 OpenAI 兼容端点生成中文名与解释，返回 (中文名→英文名, 中文名→解释)
///
/// 只发送 API 英文名与类型签名；长映射按 [`智能批大小`] 分批调用后合并
/// （单次调用的输出会超模型上限）；任一批失败整体返回错误，由上层回退规则模式。
pub fn 调用智能生成映射(
    包名: &str,
    语言代码: &str,
    条目们: &[接口条目],
) -> anyhow::Result<(HashMap<String, String>, HashMap<String, String>)> {
    let 系统提示 = if 语言代码 == "zh" {
        智能提示中文
    } else {
        智能提示英文
    };
    let 界面表 = crate::界面::界面消息表::全局表();
    let 批们: Vec<&[接口条目]> = 条目们.chunks(智能批大小).collect();
    let mut 标识符映射 = HashMap::new();
    let mut 解释映射 = HashMap::new();
    for (批序号, 批) in 批们.iter().enumerate() {
        println!(
            "{}",
            界面表.取带参消息(
                "mg_ai_batch_progress",
                &[&(批序号 + 1).to_string(), &批们.len().to_string()]
            )
        );
        let 用户提示 = 构建用户提示(包名, 语言代码, 批);
        let 内容 = 智能对话(系统提示, &用户提示)?;
        // 只接受本批条目中的英文名（防止 AI 幻觉改名）
        let 合法英文名: HashSet<String> = 批.iter().map(|条目值| 条目值.英文名.clone()).collect();
        let (批标识符, 批解释) = 解析智能结果(&内容, &合法英文名)?;
        标识符映射.extend(批标识符);
        解释映射.extend(批解释);
    }
    Ok((标识符映射, 解释映射))
}

/// 解析 AI 返回的 TOML 文本为 (中文名→英文名, 中文名→解释)；
/// 英文名不在合法集合中的条目丢弃（防止 AI 幻觉改名）
///
/// 严格 TOML 解析失败时（AI 偶发输出重复节头等不规范内容）回退逐行宽松
/// 扫描抢救可辨识条目；抢救不到任何内容时仍返回原始解析错误。
pub fn 解析智能结果(
    文本内容: &str,
    合法英文名: &HashSet<String>,
) -> anyhow::Result<(HashMap<String, String>, HashMap<String, String>)> {
    // 清洗：去掉 ```toml 围栏与前后杂讯，只保留第一个 [ 开始、结尾围栏之前的部分
    let 起点 = 文本内容.find('[').ok_or_else(|| {
        anyhow::anyhow!(
            "{}",
            crate::界面::界面消息表::全局表().取消息("mg_err_ai_no_toml")
        )
    })?;
    let 终点 = 文本内容[起点..]
        .find("```")
        .map(|位置值| 起点 + 位置值)
        .unwrap_or(文本内容.len());
    let 映射片段 = &文本内容[起点..终点];
    let 表值 = match toml::from_str::<toml::Value>(映射片段) {
        Ok(表值) => 表值,
        Err(严格错误) => {
            let 抢救结果 = 宽松解析各节(映射片段);
            if 抢救结果.is_empty() {
                return Err(anyhow::anyhow!(
                    "{}",
                    crate::界面::界面消息表::全局表()
                        .取带参消息("mg_err_ai_toml_parse", &[&严格错误.to_string()])
                ));
            }
            toml::Value::Table(抢救结果)
        }
    };
    Ok(从表提取各节(&表值, 合法英文名))
}

/// 从 {标识符, 解释} 表过滤合法条目：标识符需英文名在合法集合内，
/// 解释逐条校验长度（超 40 字打印警告）；严格与宽松两条解析路径共用
fn 从表提取各节(
    表值: &toml::Value,
    合法英文名: &HashSet<String>,
) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut 标识符映射 = HashMap::new();
    let mut 解释映射 = HashMap::new();
    if let Some(节值) = 表值.get("标识符").and_then(toml::Value::as_table) {
        for (中文名, 值) in 节值 {
            if let Some(英文名) = 值.as_str()
                && 合法英文名.contains(英文名)
                && !中文名.is_empty()
            {
                标识符映射.insert(中文名.clone(), 英文名.to_string());
            }
        }
    }
    if let Some(节值) = 表值.get("解释").and_then(toml::Value::as_table) {
        for (中文名, 值) in 节值 {
            if let Some(解释) = 值.as_str()
                && !解释.is_empty()
            {
                if 解释.chars().count() > 40 {
                    eprintln!(
                        "{}",
                        crate::界面::界面消息表::全局表().取带参消息(
                            "mg_warn_ai_explain_long",
                            &[&解释.chars().count().to_string(), 中文名]
                        )
                    );
                }
                解释映射.insert(中文名.clone(), 解释.to_string());
            }
        }
    }
    (标识符映射, 解释映射)
}

/// 宽松逐行扫描 AI 输出：识别含"标识符"/"解释"的节头（容忍重复出现，
/// 同名节内容合并），节内解析 `"键" = "值"` 行；返回重新组装的表值
/// （仅含实际出现的节），供与严格解析相同的过滤路径复用
fn 宽松解析各节(映射片段: &str) -> toml::Table {
    let mut 节名: Option<&str> = None;
    let mut 标识符表 = toml::Table::new();
    let mut 解释表 = toml::Table::new();
    for 行文本 in 映射片段.lines() {
        let 裁剪行 = 行文本.trim();
        if 裁剪行.starts_with('[') {
            // 节头判定放宽到"包含"：容忍多余空格/引号/围栏残缺
            节名 = if 裁剪行.contains("标识符") {
                Some("标识符")
            } else if 裁剪行.contains("解释") {
                Some("解释")
            } else {
                None
            };
            continue;
        }
        let Some(当前节) = 节名 else {
            continue;
        };
        let Some((键, 值)) = 拆分键值(裁剪行) else {
            continue;
        };
        if 当前节 == "标识符" {
            标识符表.insert(键, toml::Value::String(值));
        } else {
            解释表.insert(键, toml::Value::String(值));
        }
    }
    let mut 表值 = toml::Table::new();
    if !标识符表.is_empty() {
        表值.insert("标识符".to_string(), toml::Value::Table(标识符表));
    }
    if !解释表.is_empty() {
        表值.insert("解释".to_string(), toml::Value::Table(解释表));
    }
    表值
}

/// 解析 `"键" = "值"` 行；无等号或键/值为空时返回空值
fn 拆分键值(行文本: &str) -> Option<(String, String)> {
    let (原始键, 原始值) = 行文本.split_once('=')?;
    Some((提取片段(原始键)?, 提取片段(原始值)?))
}

/// 提取片段内容：优先取引号（半/全角）内文本，未加引号时取 `#` 前文本；
/// 内容为空返回空值
fn 提取片段(原始文本: &str) -> Option<String> {
    let 裁剪文本 = 原始文本.trim();
    for (左引号, 右引号) in [('"', '"'), ('\'', '\''), ('“', '”'), ('「', '」')] {
        if let Some(去引文本) = 裁剪文本.strip_prefix(左引号) {
            let 内部 = 去引文本
                .split_once(右引号)
                .map_or(去引文本, |(前段, _)| 前段)
                .trim();
            return (!内部.is_empty()).then(|| 内部.to_string());
        }
    }
    let 平文本 = 裁剪文本.split('#').next().unwrap_or(裁剪文本).trim();
    (!平文本.is_empty()).then(|| 平文本.to_string())
}

/// 检测系统语言（用于 --lang 缺省值）：完整支持全部内置语言，默认 "zh"
///
/// 实现见 [`包::界面::探测系统语言`]（读取 LC_ALL / LC_MESSAGES / LANG）。
pub fn 探测系统语言() -> String {
    crate::界面::探测系统语言()
}

#[cfg(test)]
mod 母语测试 {
    use super::*;
    use crate::映射生成::接口种类;

    /// 环境变量假象器：把键值表包成读取闭包，单测不碰真实进程环境
    /// （进程级环境改动会与并行其它测试互相干扰）
    fn 假环境(环境条目: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let 表: std::collections::HashMap<String, String> = 环境条目
            .iter()
            .map(|(键, 值)| (键.to_string(), 值.to_string()))
            .collect();
        move |键| 表.get(键).cloned()
    }

    /// 默认端点下的配置与密钥回退链
    #[test]
    fn 测试解析端点默认深度求索() {
        let 端点 = 解析端点(&假环境(&[("DEEPSEEK_API_KEY", "sk-d")])).unwrap();
        assert_eq!(端点.基础地址, 默认基础地址);
        assert_eq!(端点.模型, 默认模型);
        assert_eq!(端点.接口密钥, "sk-d");
        assert_eq!(端点.密钥变量名, "DEEPSEEK_API_KEY");
        assert!(!端点.自定义);

        // 统一前缀的变量优先于历史变量
        let 端点 = 解析端点(&假环境(&[
            ("RZ_AI_API_KEY", "sk-rz"),
            ("DEEPSEEK_API_KEY", "sk-d"),
            ("RZ_AI_MODEL", "deepseek-reasoner"),
        ]))
        .unwrap();
        assert_eq!(端点.接口密钥, "sk-rz");
        assert_eq!(端点.密钥变量名, "RZ_AI_API_KEY");
        assert_eq!(端点.模型, "deepseek-reasoner");

        // 用旧变量名配的自定义端点同样算自定义（按最终端点判定，不按变量名）
        let 端点 = 解析端点(&假环境(&[
            ("DEEPSEEK_BASE_URL", "https://mirror.example/v1"),
            ("OPENAI_API_KEY", "sk-o"),
        ]))
        .unwrap();
        assert!(端点.自定义);
        assert_eq!(端点.密钥变量名, "OPENAI_API_KEY");
    }

    /// 安全约束：默认端点绝不读 OPENAI_API_KEY（不把别家密钥发往 DeepSeek）
    #[test]
    fn 测试解析端点不泄露开放智能密钥() {
        let 错误值 = 解析端点(&假环境(&[("OPENAI_API_KEY", "sk-o")])).unwrap_err();
        assert!(
            错误值.to_string().contains("RZ_AI_API_KEY"),
            "应提示主密钥变量：{错误值}"
        );
        // 自定义端点才启用 OPENAI_API_KEY，且优先于 DEEPSEEK_API_KEY
        let 端点 = 解析端点(&假环境(&[
            ("RZ_AI_BASE_URL", "https://api.openai.com/v1"),
            ("OPENAI_API_KEY", "sk-o"),
            ("DEEPSEEK_API_KEY", "sk-d"),
        ]))
        .unwrap();
        assert!(端点.自定义);
        assert_eq!(端点.接口密钥, "sk-o");
        assert_eq!(端点.密钥变量名, "OPENAI_API_KEY");
        assert_eq!(
            对话地址(&端点.基础地址),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    /// 密钥/端点为空串时点名变量，不拿空值去请求
    #[test]
    fn 测试解析端点点名问题变量() {
        let 错误值 = 解析端点(&假环境(&[("RZ_AI_API_KEY", "")])).unwrap_err();
        assert!(错误值.to_string().contains("RZ_AI_API_KEY"), "{错误值}");
        // 空串不阻断回退：主变量为空、历史变量可用时照旧能跑
        let 端点 = 解析端点(&假环境(&[
            ("RZ_AI_API_KEY", ""),
            ("DEEPSEEK_API_KEY", "sk-d"),
        ]))
        .unwrap();
        assert_eq!(端点.密钥变量名, "DEEPSEEK_API_KEY");
        // 空白端点视为未设置，不拼出只补后缀的废 URL
        let 端点 = 解析端点(&假环境(&[
            ("RZ_AI_BASE_URL", "   "),
            ("DEEPSEEK_API_KEY", "sk-d"),
        ]))
        .unwrap();
        assert_eq!(端点.基础地址, 默认基础地址);
    }

    /// chat/completions 拼接：带 /v1 与不带、已带后缀、结尾斜杠均能跑
    #[test]
    fn 测试对话地址形态() {
        assert_eq!(
            对话地址("https://api.deepseek.com"),
            "https://api.deepseek.com/chat/completions"
        );
        assert_eq!(
            对话地址("http://localhost:11434/v1/"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            对话地址("https://x.example/v1/chat/completions"),
            "https://x.example/v1/chat/completions"
        );
    }

    /// 服务商别名归一：三种写法同一条链路，规则为离线，其余报空值
    #[test]
    fn 测试解析服务商别名() {
        for 别名 in ["deepseek", "openai", "compatible"] {
            assert_eq!(解析服务商(别名), Some(服务商::智能), "{别名}");
        }
        assert_eq!(解析服务商("rule"), Some(服务商::规则));
        assert_eq!(解析服务商("deeepseek"), None);
        assert_eq!(解析服务商(""), None);
    }

    #[test]
    fn 测试解析智能结果() {
        let 合法英文名: HashSet<String> = ["new", "Error", "Context"]
            .iter()
            .map(|词| 词.to_string())
            .collect();
        let 文本内容 = "好的，以下是映射：\n```toml\n[\"标识符\"]\n\"新建\" = \"new\"\n\"错误\" = \"Error\"\n\"幻觉\" = \"NotExist\"\n\n[\"解释\"]\n\"新建\" = \"创建一个新的实例，非常简单\"\n\"错误\" = \"描述错误信息的类型\"\n```\n";
        let (标识映射, 解释) = 解析智能结果(文本内容, &合法英文名).expect("应能解析");
        assert!(标识映射.contains_key("新建"));
        assert!(标识映射.contains_key("错误"));
        // 幻觉条目（英文名不在合法集合）被丢弃
        assert!(!标识映射.contains_key("幻觉"));
        assert_eq!(
            解释.get("新建").map(String::as_str),
            Some("创建一个新的实例，非常简单")
        );
    }

    /// 分批请求的提示词只包含本批条目，语言开关选择正确模板
    #[test]
    fn 测试构建用户提示按批() {
        let 条目们 = vec![
            接口条目 {
                种类: 接口种类::函数项,
                英文名: "new".to_string(),
                签名: "fn new() -> Self".to_string(),
            },
            接口条目 {
                种类: 接口种类::结构体项,
                英文名: "Window".to_string(),
                签名: "struct Window".to_string(),
            },
        ];
        let 中文提示 = 构建用户提示("tauri", "zh", &条目们);
        assert!(中文提示.contains("crate: tauri"));
        assert!(中文提示.contains("fn new() -> Self"));
        assert!(中文提示.contains("struct Window"));
        let 单个提示 = 构建用户提示("tauri", "zh", &条目们[..1]);
        assert!(单个提示.contains("fn new() -> Self"));
        assert!(!单个提示.contains("struct Window"), "不应包含其他批的条目");
        let 英文提示 = 构建用户提示("tauri", "ru", &条目们[..1]);
        assert!(英文提示.contains("Public API list"));
    }

    #[test]
    fn 测试智能结果非映射错误() {
        let 合法英文名 = HashSet::new();
        assert!(解析智能结果("抱歉，我无法完成。", &合法英文名).is_err());
    }

    /// AI 偶发重复节头（两次 ["解释"] 等）：严格 TOML 解析必然失败，
    /// 宽松扫描应合并重复节并抢救出全部条目
    #[test]
    fn 测试解析智能结果重复节() {
        let 合法英文名: HashSet<String> =
            ["new", "Error"].iter().map(|词| 词.to_string()).collect();
        let 文本内容 = "```toml\n[\"标识符\"]\n\"新建\" = \"new\"\n[\"标识符\"]\n\"错误\" = \"Error\"\n[\"解释\"]\n\"新建\" = \"创建实例\"\n[\"解释\"]\n\"错误\" = \"错误类型\"\n```\n";
        let (标识映射, 解释) = 解析智能结果(文本内容, &合法英文名).expect("宽松解析应能抢救");
        assert_eq!(标识映射.len(), 2, "重复标识符节应合并");
        assert_eq!(
            解释.get("错误").map(String::as_str),
            Some("错误类型"),
            "重复解释节应合并"
        );
    }

    /// 完全无法抢救（无可用节与键值对）时保留严格解析的原始错误
    #[test]
    fn 测试解析智能结果无法解析保留错误() {
        let 合法英文名 = HashSet::new();
        let 文本内容 = "[标题]\n- 列表项，没有任何键值对\n";
        let 错误值 = 解析智能结果(文本内容, &合法英文名).expect_err("应保留解析错误");
        assert!(
            错误值.to_string().contains("TOML parse error"),
            "应含 toml crate 原始错误: {}",
            错误值
        );
    }

    #[test]
    fn 测试探测系统语言() {
        // 系统语言完全由环境变量决定：临时接管清除 LC_ALL / LC_MESSAGES / LANG
        // 后逐个设定确定性断言。直接读真实环境断言会因 CI 平台区域设置差异
        // 假失败（苹果系统执行器预设英文，此前断言中文/俄文即因此挂掉）
        let _锁 = crate::语言管理::tests::环境锁();
        let _环境 =
            crate::语言管理::tests::环境还原::接管(&["LC_ALL", "LC_MESSAGES", "LANG"]);
        unsafe {
            std::env::set_var("LANG", "zh_CN.UTF-8");
        }
        assert_eq!(探测系统语言(), "zh", "zh_CN 应识别为 zh");
        unsafe {
            std::env::set_var("LANG", "ru_RU.UTF-8");
        }
        assert_eq!(探测系统语言(), "ru", "ru_RU 应识别为 ru");
        unsafe {
            std::env::remove_var("LANG");
        }
        assert_eq!(探测系统语言(), "zh", "无区域设置应回退默认 zh");
    }
}
