//! 自动映射生成：`rzc mapping auto` 的实现。
//!
//! 流程：在临时项目中把目标包作为依赖，用 `cargo build` 编译后，
//! 通过 `cargo metadata` 定位注册仓库源码，再手动调用 rustdoc 生成 JSON 文档，
//! 解析出公开接口（名称 + 类型签名），最后由智能服务或规则生成中文映射。
//!
//! 法律合规（本项目强制约束）：
//! 1. 只提取接口名称与类型签名（rustdoc JSON 的名称与内部节点字段），
//!    绝不读取文档注释字段；该字段在本模块任何地方都不会被访问。
//! 2. 生成的映射文件不含任何来自原包的文档注释翻译或复制。
//! 3. 解释由智能服务根据接口英文名称和类型签名自行生成；规则模式解释留空。
//! 4. 输出文件头部固定附免责声明（见 [`免责声明文本`]）。
//!
//! 子模块划分（与本文件此前的分段注释一致）：
//! - [`文档抽取`]：rustdoc JSON 解析与签名渲染
//! - [`文档结构`]：工具链（临时项目、元数据、编译、文档生成）
//! - [`规则`]：规则驱动的中文名生成
//! - [`映射输出`]：映射 TOML 构建
//! - [`智能`]：智能服务驱动（深度求索，兼容开放人工智能接口）

#[path = "文档抽取.rs"]
pub mod 文档抽取;
#[path = "文档结构.rs"]
pub mod 文档结构;
#[path = "映射输出.rs"]
pub mod 映射输出;
#[path = "智能.rs"]
pub mod 智能;
#[path = "规则.rs"]
pub mod 规则;

pub use 文档抽取::提取公开接口;
pub use 映射输出::构建映射表;
pub use 智能::{
    探测系统语言, 智能对话, 服务商, 确保凭据, 解析服务商, 调用智能生成映射
};
pub use 规则::{检测关键字冲突, 生成包本地化名, 规则生成本地化名};

use anyhow::Context;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// 输出文件头部的法律免责声明（按目标语言输出，逐字写入生成文件）
pub fn 免责声明文本(语言代码: &str) -> String {
    crate::界面::界面消息表::按语言建表(语言代码).取消息("mapping_disclaimer")
}

/// API 种类（对应需求：函数/结构体/枚举/特征/类型别名/宏/常量）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum 接口种类 {
    /// 函数
    函数项,
    /// 结构体
    结构体项,
    /// 枚举
    枚举项,
    /// 特征
    特征项,
    /// 类型别名
    类型别名项,
    /// 宏
    宏项,
    /// 常量
    常量项,
}

impl 接口种类 {
    /// 显示名（随界面语言变化）
    pub fn 取展示名(&self) -> String {
        let 消息键 = match self {
            Self::函数项 => "mapping_kind_function",
            Self::结构体项 => "mapping_kind_struct",
            Self::枚举项 => "mapping_kind_enum",
            Self::特征项 => "mapping_kind_trait",
            Self::类型别名项 => "mapping_kind_type_alias",
            Self::宏项 => "mapping_kind_macro",
            Self::常量项 => "mapping_kind_const",
        };
        crate::界面::界面消息表::全局表().取消息(消息键)
    }
}

/// 提取出的公开 API 条目：只含名称与类型签名（无任何文档内容）
#[derive(Debug, Clone)]
pub struct 接口条目 {
    /// API 种类
    pub 种类: 接口种类,
    /// 英文原名
    pub 英文名: String,
    /// 类型签名，如 `fn new() -> Result<Self>`、`struct Error`、`const MAX: u32`
    pub 签名: String,
}

/// `--target-version` 校验并转换为 Cargo 版本需求（生成基准锁定）：
///
/// - `x.y.z`（可带 `-预发布` / `+构建` 后缀）→ `=x.y.z` 精确锁定；
/// - `x.y` → `x.y.*`、`x` → `x.*`（该线最新，生产映射建议完整版本号）；
/// - 兼容前导等号与版本字母写法（如 `=2.11.5`）；
/// - 非法输入返回空值（调用方以 `mapping_version_invalid` 提示）。
pub(crate) fn 版本需求(版本文: &str) -> Option<String> {
    let 版本文 = 版本文.strip_prefix('=').unwrap_or(版本文);
    let 版本文 = 版本文
        .strip_prefix('v')
        .or_else(|| 版本文.strip_prefix('V'))
        .unwrap_or(版本文);
    // 拆出构建(+)与预发布(-)后缀（仅完整 x.y.z 才允许带后缀）
    let (余文, 构建段) = match 版本文.split_once('+') {
        Some((前段, 后段)) => (前段, Some(后段)),
        None => (版本文, None),
    };
    let (核心段, 预发布段) = match 余文.split_once('-') {
        Some((前段, 后段)) => (前段, Some(后段)),
        None => (余文, None),
    };
    let 段们: Vec<&str> = 核心段.split('.').collect();
    let 是否为数字串 =
        |段文本: &str| !段文本.is_empty() && 段文本.chars().all(|单字| 单字.is_ascii_digit());
    if 段们.is_empty() || 段们.len() > 3 || 段们.iter().any(|段| !是否为数字串(段)) {
        return None;
    }
    // 后缀字符集：字母、数字、点、连字符，且点分段非空（语义版本规范）
    let 后缀有效 = |后缀: &str| {
        !后缀.is_empty()
            && 后缀.split('.').all(|分段| {
                !分段.is_empty()
                    && 分段
                        .chars()
                        .all(|单字| 单字.is_ascii_alphanumeric() || 单字 == '-')
            })
    };
    if (预发布段.is_some() || 构建段.is_some()) && 段们.len() != 3 {
        return None;
    }
    if 预发布段.is_some_and(|段| !后缀有效(段)) || 构建段.is_some_and(|段| !后缀有效(段))
    {
        return None;
    }
    match 段们.len() {
        3 => Some(format!("={}", 版本文)),
        2 => Some(format!("{}.{}.*", 段们[0], 段们[1])),
        _ => Some(format!("{}.*", 段们[0])),
    }
}

/// 智能解释覆盖率（生成后自检）：按最终母语名表统计非空解释。
///
/// 智能服务偶发省略部分条目的解释（同批不同调用间有随机波动），生成后
/// 显式输出覆盖率，便于及时发现缺失并重跑补齐。
fn 解释覆盖率(
    中文名表: &[(String, String)],
    解释表: &HashMap<String, String>,
) -> (usize, usize) {
    let 覆盖数 = 中文名表
        .iter()
        .filter(|(名字, _)| 解释表.get(名字).is_some_and(|文本值| !文本值.is_empty()))
        .count();
    (覆盖数, 中文名表.len())
}

/// 主入口：`rzc mapping auto`
///
/// - 包名：目标包（已安装或可从官方仓库拉取）
/// - 语言代码：语言包目录名（如中文、俄文），用于冲突检测与默认输出位置
/// - 服务商：深度求索 / 开放人工智能 / 兼容端点（均走开放人工智能兼容链路，调用智能服务）
///   或规则（离线规则模式）
/// - 输出路径：生成文件的写入位置
/// - 目标版本：锁定提取基准版本（`2.11.5` → `=2.11.5` 精确锁定；
///   `2.11` / `2` → 该线最新）；空值时解析最新版（结果不可复现，打印提示）
pub fn 运行自动生成(
    包名: &str,
    语言代码: &str,
    服务商: &str,
    输出路径: &Path,
    目标版本: Option<&str>,
) -> anyhow::Result<()> {
    let 界面表 = crate::界面::界面消息表::按语言建表(语言代码);
    if 包名.is_empty() {
        anyhow::bail!("{}", 界面表.取消息("mapping_crate_empty"));
    }
    if !包名
        .chars()
        .all(|单字| 单字.is_ascii_alphanumeric() || 单字 == '-' || 单字 == '_')
    {
        anyhow::bail!("{}", 界面表.取带参消息("mapping_crate_invalid", &[包名]));
    }
    // 版本锁定参数校验：非法输入直接报错，避免生成基准被静默放宽
    let 版本需求值 = match 目标版本 {
        Some(版本文) => match 版本需求(版本文) {
            Some(需求值) => Some(需求值),
            None => anyhow::bail!(
                "{}",
                界面表.取带参消息("mapping_version_invalid", &[版本文])
            ),
        },
        None => {
            eprintln!("{}", 界面表.取消息("mapping_version_unlocked"));
            None
        }
    };

    // 服务商参数早校验：拼错的服务商选项不该先跑完文档提取再报错
    let Some(模式) = 解析服务商(服务商) else {
        anyhow::bail!(
            "{}",
            界面表.取带参消息("mapping_unknown_provider", &[服务商])
        );
    };

    println!("{}", 界面表.取带参消息("mapping_extracting", &[包名]));
    let (结构化文档们, 解析版本) = 文档结构::提取包文档(包名, 版本需求值.as_deref())?;
    // 薄壳包（转发包，自身只做整批重导出）的公开接口
    // 来自通配重导出链上的依赖包，逐个 JSON 合并提取；
    // 同名接口只保留首个条目（链上包可能导出同名类型）
    let mut 条目们 = Vec::new();
    let mut 已见英文名 = HashSet::new();
    for (文档包名, 结构化文档值) in &结构化文档们 {
        println!("{}", 界面表.取带参消息("mapping_extracting", &[文档包名]));
        for 条目值 in 提取公开接口(结构化文档值)? {
            if 已见英文名.insert(条目值.英文名.clone()) {
                条目们.push(条目值);
            }
        }
    }
    if 条目们.is_empty() {
        anyhow::bail!("{}", 界面表.取带参消息("mapping_no_api", &[包名]));
    }

    // 统计各类数量
    let mut 统计表: HashMap<接口种类, usize> = HashMap::new();
    for 条目值 in &条目们 {
        *统计表.entry(条目值.种类).or_insert(0) += 1;
    }
    let 统计文本 = [
        接口种类::函数项,
        接口种类::结构体项,
        接口种类::枚举项,
        接口种类::特征项,
        接口种类::类型别名项,
        接口种类::常量项,
        接口种类::宏项,
    ]
    .iter()
    .map(|种类| {
        format!(
            "{} {}",
            统计表.get(种类).copied().unwrap_or(0),
            种类.取展示名()
        )
    })
    .collect::<Vec<_>>()
    .join(if 语言代码 == "zh" { "、" } else { ", " });
    println!(
        "{}",
        界面表.取带参消息("mapping_extracted", &[&条目们.len().to_string(), &统计文本])
    );
    // rustdoc JSON 格式当前工具链不输出声明式宏定义（官方格式限制），提示用户
    if !统计表.contains_key(&接口种类::宏项) {
        eprintln!("{}", 界面表.取消息("mapping_no_macro"));
    }

    // 1. 名称：中文走规则生成中文名，其他语言保留英文原名（智能模式成功后由智能结果覆盖）
    let mut 中文名表: Vec<(String, String)> = Vec::new();
    let mut 已用中文名 = HashSet::new();
    for 条目值 in &条目们 {
        let 中文名 = 规则生成本地化名(语言代码, &条目值.英文名);
        if !已用中文名.insert(中文名.clone()) {
            eprintln!(
                "{}",
                界面表.取带参消息("mapping_name_conflict", &[&中文名, &条目值.英文名])
            );
            continue;
        }
        中文名表.push((中文名, 条目值.英文名.clone()));
    }

    // 2. 解释：智能模式调用服务商，失败或无配置回退规则模式（解释留空）
    let mut 解释表: HashMap<String, String> = HashMap::new();
    match 模式 {
        服务商::智能 => {
            match 调用智能生成映射(包名, 语言代码, &条目们) {
                Ok((智能标识符们, 智能解释们)) => {
                    // 智能中文名覆盖规则名（校验英文名合法性，防止智能服务幻觉改名）
                    for (中文名, 英文名) in 智能标识符们 {
                        // 先采集智能解释：即使名字与规则名重复（常见情形），
                        // 解释也不应丢弃
                        if let Some(解释) = 智能解释们.get(&中文名)
                            && !解释.is_empty()
                        {
                            解释表.insert(中文名.clone(), 解释.clone());
                        }
                        if 中文名表.iter().any(|(名字, _)| 名字 == &中文名) {
                            continue;
                        }
                        if let Some(位置值) =
                            中文名表.iter().position(|(_, 英文)| 英文 == &英文名)
                        {
                            中文名表.remove(位置值);
                            中文名表.push((中文名.clone(), 英文名));
                        }
                    }
                    println!("{}", 界面表.取带参消息("mapping_ai_success", &[服务商]));
                    // 解释覆盖率自检：缺失时显式警告（智能服务偶发省略条目）
                    let (覆盖数, 总数) = 解释覆盖率(&中文名表, &解释表);
                    if 覆盖数 < 总数 {
                        eprintln!(
                            "{}",
                            界面表.取带参消息(
                                "mapping_ai_coverage_missing",
                                &[
                                    &覆盖数.to_string(),
                                    &总数.to_string(),
                                    &(总数 - 覆盖数).to_string()
                                ]
                            )
                        );
                    } else {
                        println!(
                            "{}",
                            界面表.取带参消息(
                                "mapping_ai_coverage",
                                &[&覆盖数.to_string(), &总数.to_string()]
                            )
                        );
                    }
                }
                Err(错误值) => {
                    eprintln!(
                        "{}",
                        界面表.取带参消息(
                            "mapping_ai_fallback",
                            &["RZ_AI_API_KEY / DEEPSEEK_API_KEY", &错误值.to_string()]
                        )
                    );
                }
            }
        }
        服务商::规则 => {
            eprintln!("{}", 界面表.取消息("mapping_rule_mode"));
        }
    }

    // 3. 输出 TOML
    let 包中文名 = 生成包本地化名(语言代码, 包名);
    let 映射文本 = 构建映射表(
        语言代码,
        包名,
        &包中文名,
        解析版本.as_deref(),
        &条目们,
        &中文名表,
        &解释表,
    );

    // 4. 冲突检测：词法转译（关键字映射）先于别名替换执行，冲突的中文名生成后不会生效；
    // 语言包目录优先从输出路径推导（…/<lang>/crates/<crate>.toml 的上两级），
    // 推导失败时回退当前目录的项目语言包根（主仓库为 crates/engine/lang-packs/）
    let 语言包目录 = 输出路径
        .parent()
        .and_then(|路径值| 路径值.parent())
        .filter(|目录值| 目录值.join("keywords.toml").exists())
        .map(|目录值| 目录值.to_path_buf())
        .unwrap_or_else(|| {
            std::env::current_dir()
                .map(|工作目录| crate::语言包根目录(&工作目录).join(语言代码))
                .unwrap_or_else(|_| PathBuf::from(format!("lang-packs/{}", 语言代码)))
        });
    for (中文名, 关键字英文, 本项英文) in 检测关键字冲突(&语言包目录, &中文名表)
    {
        eprintln!(
            "{}",
            界面表.取带参消息(
                "mapping_keyword_conflict",
                &[&中文名, &中文名, &关键字英文, &本项英文]
            )
        );
    }

    // 目标文件已存在时给出覆盖警告（防止静默覆盖手工调整过的映射）
    if 输出路径.exists() {
        eprintln!(
            "{}",
            界面表.取带参消息("mapping_overwrite_warn", &[&输出路径.display().to_string()])
        );
    }
    if let Some(父路径) = 输出路径.parent() {
        fs::create_dir_all(父路径).with_context(|| {
            界面表.取带参消息("mg_err_mkdir", &[&父路径.display().to_string()])
        })?;
    }
    fs::write(输出路径, 映射文本).with_context(|| {
        界面表.取带参消息("mg_err_write", &[&输出路径.display().to_string()])
    })?;
    println!(
        "{}",
        界面表.取带参消息("mapping_generated", &[&输出路径.display().to_string()])
    );
    println!(
        "{}",
        界面表.取带参消息("mapping_crate_name_line", &[&包中文名, &包中文名, 包名])
    );
    println!("{}", 界面表.取带参消息("mapping_usage_hint", &[语言代码]));
    Ok(())
}

#[cfg(test)]
mod 母语测试 {
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

    /// 非法输入一律为空值（由调用方给出非法版本提示）
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
        let mut 解释表 = HashMap::new();
        解释表.insert("新建".to_string(), "创建新对象。".to_string());
        解释表.insert("错误".to_string(), String::new());
        assert_eq!(解释覆盖率(&表, &解释表), (1, 3));
        解释表.insert("错误".to_string(), "错误类型。".to_string());
        解释表.insert("状态".to_string(), "状态值。".to_string());
        assert_eq!(解释覆盖率(&表, &解释表), (3, 3));
        assert_eq!(解释覆盖率(&[], &解释表), (0, 0));
    }

    /// 入参非法时在任何网络/提包动作之前直接报错返回，且不落盘任何文件
    #[test]
    fn 测试运行自动生成先校验后联网() {
        let 目录值 = tempfile::tempdir().unwrap();
        let 输出文件 = 目录值.path().join("out.toml");
        // 空包名
        assert!(运行自动生成("", "zh", "rule", &输出文件, None).is_err());
        // 名字含路径穿越/空格等非法字符
        assert!(运行自动生成("../evil name", "zh", "rule", &输出文件, None).is_err());
        // 版本号非法（空值之外才会在此报错；合法版本随后才进入提包网络流程）
        assert!(运行自动生成("serde", "zh", "rule", &输出文件, Some("abc")).is_err());
        assert!(运行自动生成("serde", "zh", "rule", &输出文件, Some("2.x")).is_err());
        assert!(!输出文件.exists(), "校验失败不得产生输出文件");
    }
}
