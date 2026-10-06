//! 自动映射生成：`rzc mapping auto` 的实现。
//!
//! 流程：在临时项目中把目标 crate 作为依赖，用 `cargo build` 编译后，
//! 通过 `cargo metadata` 定位 registry 源码，再手动调用 rustdoc 生成 JSON 文档，
//! 解析出公开 API（名称 + 类型签名），最后由 AI 或规则生成中文映射。
//!
//! 法律合规（本项目强制约束）：
//! 1. 只提取 API 名称与类型签名（rustdoc JSON 的 `name` / `inner` 字段），
//!    绝不读取 doc comment（`docs` 字段）；`docs` 字段在本模块任何地方都不会被访问。
//! 2. 生成的映射文件不含任何来自原 crate 的文档注释翻译或复制。
//! 3. 解释由 AI 根据 API 英文名称和类型签名自行生成；规则模式解释留空。
//! 4. 输出文件头部固定附免责声明（见 [`免责声明文本`]）。
//!
//! 子模块划分（与本文件此前的分段注释一致）：
//! - [`rustdoc_extract`]：rustdoc JSON 解析与签名渲染
//! - [`doc_json`]：工具链（临时项目 + metadata + build + rustdoc）
//! - [`rules`]：规则驱动的中文名生成
//! - [`toml_output`]：映射 TOML 构建
//! - [`ai`]：AI 驱动（DeepSeek，OpenAI 兼容接口）
//!
//! 注：外部 std/三方 ABI 走英文透传——crate 根 `crate::语言包根目录`（`main.zh` 已翻转）、`anyhow`/`std`/
//! `serde_json`/`toml`/`tempfile` 类型与方法链；已翻转中文 ABI：`crate::ui::界面::按语言加载/全局/取文/取文带参`、
//! 子模块再导出（`检测系统语言`/`深度求索对话`/`调用智能生成映射`/`检测关键字冲突`/
//! `生成库名本地化`/`规则生成本地化名`/`提取公开接口`/`构建映射文本`）。

mod ai;
mod doc_json;
mod rules;
mod rustdoc_extract;
mod toml_output;

pub use ai::{检测系统语言, 深度求索对话, 调用智能生成映射};
pub use rules::{检测关键字冲突, 生成库名本地化, 规则生成本地化名};
pub use rustdoc_extract::提取公开接口;
pub use toml_output::构建映射文本;

use anyhow::{Context, bail};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// 输出文件头部的法律免责声明（按目标语言输出，逐字写入生成文件）
pub fn 免责声明文本(语种: &str) -> String {
    crate::ui::界面::按语言加载(语种).取文("mapping_disclaimer")
}

/// API 种类（对应需求：函数/结构体/枚举/特征/类型别名/宏/常量）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum 接口种类 {
    /// 函数
    函数项,
    /// 结构体
    结构项,
    /// 枚举
    枚举项,
    /// 特征（trait）
    特征项,
    /// 类型别名
    类型别名,
    /// 宏
    宏项,
    /// 常量
    常量项,
}

impl 接口种类 {
    /// 显示名（随界面语言变化）
    pub fn 显示名(&self) -> String {
        let 文案键 = match self {
            接口种类::函数项 => "mapping_kind_function",
            接口种类::结构项 => "mapping_kind_struct",
            接口种类::枚举项 => "mapping_kind_enum",
            接口种类::特征项 => "mapping_kind_trait",
            接口种类::类型别名 => "mapping_kind_type_alias",
            接口种类::宏项 => "mapping_kind_macro",
            接口种类::常量项 => "mapping_kind_const",
        };
        crate::ui::界面::全局().取文(文案键)
    }
}

/// 提取出的公开 API 条目：只含名称与类型签名（无任何文档内容）
#[derive(Debug, Clone)]
pub struct 接口条目 {
    /// API 种类
    pub 种类: 接口种类,
    /// 英文原名
    pub 英文原名: String,
    /// 类型签名，如 `fn new() -> Result<Self>`、`struct Error`、`const MAX: u32`
    pub 类型签名: String,
}

/// `--target-version` 校验并转换为 Cargo 版本需求（生成基准锁定）：
///
/// - `x.y.z`（可带 `-预发布` / `+构建` 后缀）→ `=x.y.z` 精确锁定；
/// - `x.y` → `x.y.*`、`x` → `x.*`（该线最新，生产映射建议完整版本号）；
/// - 兼容前导 `=` 与 `v` 写法（如 `=2.11.5`、`v2.11.5`）；
/// - 非法输入返回 `None`（调用方以 `mapping_version_invalid` 提示）。
pub(crate) fn 版本需求(版本串: &str) -> Option<String> {
    let 版本串 = 版本串.strip_prefix('=').unwrap_or(版本串);
    let 版本串 = 版本串
        .strip_prefix('v')
        .or_else(|| 版本串.strip_prefix('V'))
        .unwrap_or(版本串);
    // 拆出构建(+)与预发布(-)后缀（仅完整 x.y.z 才允许带后缀）
    let (剩余段, 构建后缀) = match 版本串.split_once('+') {
        Some((甲, 乙)) => (甲, Some(乙)),
        None => (版本串, None),
    };
    let (核心段, 预发布) = match 剩余段.split_once('-') {
        Some((甲, 乙)) => (甲, Some(乙)),
        None => (剩余段, None),
    };
    let 分段: Vec<&str> = 核心段.split('.').collect();
    let 是数字 = |子段: &str| -> bool {
        !子段.is_empty() && 子段.chars().all(|符| 符.is_ascii_digit())
    };
    if 分段.is_empty() || 分段.len() > 3 || 分段.iter().any(|段| !是数字(段)) {
        return None;
    }
    // 后缀字符集：字母、数字、点、连字符，且点分段非空（语义版本规范）
    let 后缀合法 = |后缀: &str| -> bool {
        !后缀.is_empty()
            && 后缀.split('.').all(|子段| {
                !子段.is_empty()
                    && 子段
                        .chars()
                        .all(|符| 符.is_ascii_alphanumeric() || 符 == '-')
            })
    };
    if (预发布.is_some() || 构建后缀.is_some()) && 分段.len() != 3 {
        return None;
    }
    if 预发布.is_some_and(|段| !后缀合法(段)) || 构建后缀.is_some_and(|段| !后缀合法(段))
    {
        return None;
    }
    match 分段.len() {
        3 => Some(format!("={}", 版本串)),
        2 => Some(format!("{}.{}.*", 分段[0], 分段[1])),
        _ => Some(format!("{}.*", 分段[0])),
    }
}

/// AI 解释覆盖率（生成后自检）：按最终母语名表统计非空解释。
///
/// AI 偶发省略部分条目的解释（同批不同调用间有随机波动），生成后
/// 显式输出覆盖率，便于及时发现缺失并重跑补齐。
fn 解释覆盖率(
    中文名表: &[(String, String)],
    解释表: &HashMap<String, String>,
) -> (usize, usize) {
    let 已覆盖数 = 中文名表
        .iter()
        .filter(|(名, _)| 解释表.get(名).is_some_and(|文本串| !文本串.is_empty()))
        .count();
    (已覆盖数, 中文名表.len())
}

/// 主入口：`rzc mapping auto`
///
/// - `库名`：目标 crate（已安装或可从 crates.io 拉取）
/// - `语种`：语言包目录名（如 zh、ru），用于冲突检测与默认输出位置
/// - `生成方式`：`deepseek`（调用 AI）或 `rule`（离线规则模式）
/// - `输出路径`：输出文件路径
/// - `目标版本`：锁定提取基准版本（`2.11.5` → `=2.11.5` 精确锁定；
///   `2.11` / `2` → 该线最新）；`None` 时解析最新版（结果不可复现，打印提示）
pub fn 运行自动生成(
    库名: &str,
    语种: &str,
    生成方式: &str,
    输出路径: &Path,
    目标版本: Option<&str>,
) -> anyhow::Result<()> {
    let 界面 = crate::ui::界面::按语言加载(语种);
    if 库名.is_empty() {
        bail!("{}", 界面.取文("mapping_crate_empty"));
    }
    if !库名
        .chars()
        .all(|符| 符.is_ascii_alphanumeric() || 符 == '-' || 符 == '_')
    {
        bail!("{}", 界面.取文带参("mapping_crate_invalid", &[库名]));
    }
    // 版本锁定参数校验：非法输入直接报错，避免生成基准被静默放宽
    let 版本需求值 = match 目标版本 {
        Some(版本串) => match 版本需求(版本串) {
            Some(需求) => Some(需求),
            None => bail!("{}", 界面.取文带参("mapping_version_invalid", &[版本串])),
        },
        None => {
            eprintln!("{}", 界面.取文("mapping_version_unlocked"));
            None
        }
    };

    println!("{}", 界面.取文带参("mapping_extracting", &[库名]));
    let (文档串集, 已解析版本) = doc_json::提取库文档(库名, 版本需求值.as_deref())?;
    // 薄壳 crate（meta crate，如 salvo 仅 `pub use salvo_core::*`）的公开 API
    // 来自 glob 重导出链上的依赖 crate（如 salvo_core），逐个 JSON 合并提取；
    // 同名 API 只保留首个条目（链上 crate 可能导出同名类型）
    let mut 条目表 = Vec::new();
    let mut 已见英文名 = HashSet::new();
    for (文档库名, 文档内容) in &文档串集 {
        println!("{}", 界面.取文带参("mapping_extracting", &[文档库名]));
        for 项 in 提取公开接口(文档内容)? {
            if 已见英文名.insert(项.英文原名.clone()) {
                条目表.push(项);
            }
        }
    }
    if 条目表.is_empty() {
        bail!("{}", 界面.取文带参("mapping_no_api", &[库名]));
    }

    // 统计各类数量
    let mut 各类计数: HashMap<接口种类, usize> = HashMap::new();
    for 项 in &条目表 {
        *各类计数.entry(项.种类).or_insert(0) += 1;
    }
    let 计数文本 = [
        接口种类::函数项,
        接口种类::结构项,
        接口种类::枚举项,
        接口种类::特征项,
        接口种类::类型别名,
        接口种类::常量项,
        接口种类::宏项,
    ]
    .iter()
    .map(|种类项| {
        format!(
            "{} {}",
            各类计数.get(种类项).copied().unwrap_or(0),
            种类项.显示名()
        )
    })
    .collect::<Vec<_>>()
    .join(if 语种 == "zh" { "、" } else { ", " });
    println!(
        "{}",
        界面.取文带参("mapping_extracted", &[&条目表.len().to_string(), &计数文本])
    );
    // rustdoc JSON 格式当前工具链不输出 macro_rules! 宏定义（官方格式限制），提示用户
    if !各类计数.contains_key(&接口种类::宏项) {
        eprintln!("{}", 界面.取文("mapping_no_macro"));
    }

    // 1. 名称：zh 走规则生成中文名，其他语言保留英文原名（AI 模式成功后由 AI 结果覆盖）
    let mut 中文名表: Vec<(String, String)> = Vec::new();
    let mut 已用中文名 = HashSet::new();
    for 项 in &条目表 {
        let 中文名 = 规则生成本地化名(语种, &项.英文原名);
        if !已用中文名.insert(中文名.clone()) {
            eprintln!(
                "{}",
                界面.取文带参("mapping_name_conflict", &[&中文名, &项.英文原名])
            );
            continue;
        }
        中文名表.push((中文名, 项.英文原名.clone()));
    }

    // 2. 解释：AI 模式调用服务商，失败或无配置回退规则模式（解释留空）
    let mut 解释表: HashMap<String, String> = HashMap::new();
    match 生成方式 {
        "deepseek" => {
            match 调用智能生成映射(库名, 语种, &条目表) {
                Ok((智能中文名, 智能解释表)) => {
                    // AI 中文名覆盖规则名（校验英文名合法性，防止 AI 幻觉改名）
                    for (中文名, 英文名) in 智能中文名 {
                        // 先采集 AI 解释：即使名字与规则名重复（常见情形），
                        // 解释也不应丢弃
                        if let Some(解释文本) = 智能解释表.get(&中文名)
                            && !解释文本.is_empty()
                        {
                            解释表.insert(中文名.clone(), 解释文本.clone());
                        }
                        if 中文名表.iter().any(|(名, _)| 名 == &中文名) {
                            continue;
                        }
                        if let Some(序号) = 中文名表.iter().position(|(_, 英)| 英 == &英文名)
                        {
                            中文名表.remove(序号);
                            中文名表.push((中文名.clone(), 英文名));
                        }
                    }
                    println!("{}", 界面.取文带参("mapping_ai_success", &[生成方式]));
                    // 解释覆盖率自检：缺失时显式警告（AI 偶发省略条目）
                    let (已覆盖数, 总数) = 解释覆盖率(&中文名表, &解释表);
                    if 已覆盖数 < 总数 {
                        eprintln!(
                            "{}",
                            界面.取文带参(
                                "mapping_ai_coverage_missing",
                                &[
                                    &已覆盖数.to_string(),
                                    &总数.to_string(),
                                    &(总数 - 已覆盖数).to_string()
                                ]
                            )
                        );
                    } else {
                        println!(
                            "{}",
                            界面.取文带参(
                                "mapping_ai_coverage",
                                &[&已覆盖数.to_string(), &总数.to_string()]
                            )
                        );
                    }
                }
                Err(错误信息) => {
                    eprintln!(
                        "{}",
                        界面.取文带参(
                            "mapping_ai_fallback",
                            &["DEEPSEEK_API_KEY", &错误信息.to_string()]
                        )
                    );
                }
            }
        }
        "rule" => {
            eprintln!("{}", 界面.取文("mapping_rule_mode"));
        }
        其余 => bail!("{}", 界面.取文带参("mapping_unknown_provider", &[其余])),
    }

    // 3. 输出 TOML
    let 库中文名 = 生成库名本地化(语种, 库名);
    let 映射文本 = 构建映射文本(
        语种,
        库名,
        &库中文名,
        已解析版本.as_deref(),
        &条目表,
        &中文名表,
        &解释表,
    );

    // 4. 冲突检测：词法转译（关键字映射）先于别名替换执行，冲突的中文名生成后不会生效；
    // 语言包目录优先从输出路径推导（…/<lang>/crates/<crate>.toml 的上两级），
    // 推导失败时回退 cwd 的项目语言包根（主仓库为 crates/engine/lang-packs/）
    let 语言包目录 = 输出路径
        .parent()
        .and_then(|段| 段.parent())
        .filter(|目录段| 目录段.join("keywords.toml").exists())
        .map(|目录段| 目录段.to_path_buf())
        .unwrap_or_else(|| {
            std::env::current_dir()
                .map(|工作目录| crate::语言包根目录(&工作目录).join(语种))
                .unwrap_or_else(|_| PathBuf::from(format!("lang-packs/{}", 语种)))
        });
    for (中文, 关键字英名, 本英名) in 检测关键字冲突(&语言包目录, &中文名表)
    {
        eprintln!(
            "{}",
            界面.取文带参(
                "mapping_keyword_conflict",
                &[&中文, &中文, &关键字英名, &本英名]
            )
        );
    }

    // 目标文件已存在时给出覆盖警告（防止静默覆盖手工调整过的映射）
    if 输出路径.exists() {
        eprintln!(
            "{}",
            界面.取文带参("mapping_overwrite_warn", &[&输出路径.display().to_string()])
        );
    }
    if let Some(上级) = 输出路径.parent() {
        fs::create_dir_all(上级)
            .with_context(|| 界面.取文带参("mg_err_mkdir", &[&上级.display().to_string()]))?;
    }
    fs::write(输出路径, 映射文本)
        .with_context(|| 界面.取文带参("mg_err_write", &[&输出路径.display().to_string()]))?;
    println!(
        "{}",
        界面.取文带参("mapping_generated", &[&输出路径.display().to_string()])
    );
    println!(
        "{}",
        界面.取文带参("mapping_crate_name_line", &[&库中文名, &库中文名, 库名])
    );
    println!("{}", 界面.取文带参("mapping_usage_hint", &[语种]));
    Ok(())
}

#[cfg(test)]
mod 单元测试 {
    use super::{版本需求, 解释覆盖率, 运行自动生成};
    use std::collections::HashMap;

    /// 完整三段版本 → `=x.y.z` 精确锁定；兼容 =/v 前导与预发布/构建后缀
    #[test]
    fn 测试版本需求精确() {
        assert_eq!(版本需求("2.11.5").as_deref(), Some("=2.11.5"));
        assert_eq!(版本需求("=2.11.5").as_deref(), Some("=2.11.5"));
        assert_eq!(版本需求("v2.11.5").as_deref(), Some("=2.11.5"));
        assert_eq!(版本需求("3.0.0-alpha.0").as_deref(), Some("=3.0.0-alpha.0"));
        assert_eq!(版本需求("1.0.0+build.7").as_deref(), Some("=1.0.0+build.7"));
    }

    /// 一段/两段版本 → 该线最新（前缀锁定）
    #[test]
    fn 测试版本需求前缀线() {
        assert_eq!(版本需求("2.11").as_deref(), Some("2.11.*"));
        assert_eq!(版本需求("2").as_deref(), Some("2.*"));
    }

    /// 非法输入一律 None（由调用方报 mapping_version_invalid）
    #[test]
    fn 测试版本需求非法() {
        for 坏值 in [
            "",
            " ",
            "abc",
            "2.11.*",
            "2.11.5.6",
            "2..5",
            "2.11.5 x",
            "-1.0.0",
            "2.11.5-",
            "2.11.5-alpha..1",
            "v",
            "=",
        ] {
            assert_eq!(版本需求(坏值), None, "应判非法: {坏值:?}");
        }
    }

    /// 解释覆盖率：空串解释不计入；空表安全返回 (0, 0)
    #[test]
    fn 测试解释覆盖率() {
        let 表 = vec![
            ("新建".to_string(), "new".to_string()),
            ("错误".to_string(), "Error".to_string()),
            ("状态".to_string(), "State".to_string()),
        ];
        let mut 解释 = HashMap::new();
        解释.insert("新建".to_string(), "创建新对象。".to_string());
        解释.insert("错误".to_string(), String::new());
        assert_eq!(解释覆盖率(&表, &解释), (1, 3));
        解释.insert("错误".to_string(), "错误类型。".to_string());
        解释.insert("状态".to_string(), "状态值。".to_string());
        assert_eq!(解释覆盖率(&表, &解释), (3, 3));
        assert_eq!(解释覆盖率(&[], &解释), (0, 0));
    }

    /// 入参非法时在任何网络/提包动作之前 bail，且不落盘任何文件
    #[test]
    fn 测试运行自动生成校验先于网络() {
        let 目录 = tempfile::tempdir().unwrap();
        let 输出 = 目录.path().join("out.toml");
        // 空 crate 名
        assert!(运行自动生成("", "zh", "rule", &输出, None).is_err());
        // 名字含路径穿越/空格等非法字符
        assert!(运行自动生成("../evil name", "zh", "rule", &输出, None).is_err());
        // 版本号非法（None 之外才会在此 bail；合法版本随后才进入提包网络流程）
        assert!(运行自动生成("serde", "zh", "rule", &输出, Some("abc")).is_err());
        assert!(运行自动生成("serde", "zh", "rule", &输出, Some("2.x")).is_err());
        assert!(!输出.exists(), "校验失败不得产生输出文件");
    }
}
