//! 第三方库共享注册中心模块
//!
//! 实现 `rzc crate` 子命令：检索、安装、列出、移除、更新、发布。
//! 注册中心即一个版本控制仓库（代码托管平台或任意版本控制可达地址 / 本地路径），
//! 根含索引文件，映射按语言与库名分层存放
//! （与语言包内的映射目录结构一致）。
//!
//! 远程获取复用语言管理器的仓库源 / 临时目录 / 压缩包拉取能力；
//! 本地路径注册中心直接复用目录，无需网络。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::界面;
use crate::语言管理::{临时目录句柄, 仓库源项, 校验语言代码};

/// 默认注册中心地址（可用专门的环境变量覆盖）。
/// 注册中心已合并进主仓库，位于 `third-party/` 子目录；
/// 故默认即指向主仓库，克隆后从 `third-party/index.json` 读取。
const 默认包仓库地址: &str = "https://gitcode.com/tan80/i18n-rust";

/// 单条映射在注册中心的索引条目
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct 映射条目 {
    /// 库名（连字符归一为下划线）
    #[serde(rename = "crate_name")]
    pub 包名: String,
    /// 语言代码
    #[serde(rename = "lang")]
    pub 语言代码: String,
    /// 映射版本（语义自定，默认 1.0）
    #[serde(rename = "version")]
    pub 版本: String,
    /// 译者署名
    #[serde(rename = "author")]
    pub 作者: String,
    /// 映射文件许可证（默认宽松许可证）
    #[serde(rename = "license")]
    pub 许可证: String,
    /// 质量分（0–1，发布校验通过即 1.0）
    #[serde(rename = "quality")]
    pub 质量分: f64,
    /// 下载计数
    #[serde(rename = "downloads")]
    pub 下载计数: u64,
    /// 映射文件在仓库内的相对路径
    #[serde(rename = "file")]
    pub 映射文件: String,
    /// 最后更新日期
    #[serde(rename = "updated")]
    pub 更新日期: String,
}

/// 注册中心索引文件结构
#[derive(Serialize, Deserialize, Default)]
pub struct 包索引 {
    #[serde(rename = "registry")]
    pub 仓库名: String,
    #[serde(rename = "updated")]
    pub 更新日期: String,
    #[serde(rename = "mappings")]
    pub 映射清单: Vec<映射条目>,
}

/// 注册中心仓库地址：专门的环境变量优先，否则默认地址。
///
/// 默认情况下，若当前工作目录是主仓库源码树（存在 `third-party/index.json`），
/// 则直接使用该本地子目录，离线即可检索/安装；否则回退到远端主仓库。
fn 仓库地址() -> String {
    if let Ok(环境值) = std::env::var("RZ_CRATE_REPO") {
        let 规范值 = 环境值.trim().trim_end_matches('/').to_string();
        if !规范值.is_empty() {
            return 规范值;
        }
    }
    if Path::new("third-party").join("index.json").is_file() {
        return "third-party".to_string();
    }
    默认包仓库地址.to_string()
}

/// 构造注册中心源（与语言安装同源回退策略一致）
fn 收集仓库源() -> Vec<仓库源项> {
    vec![仓库源项::从地址(&仓库地址())]
}

/// 注册中心在仓库内的实际目录。合并进主仓库后位于 `third-party/` 子目录，
/// 独立仓库则直接在根；两种布局都兼容。
fn 仓库子目录(仓库根: &Path) -> PathBuf {
    let 嵌套目录 = 仓库根.join("third-party");
    if 嵌套目录.join("index.json").is_file() {
        嵌套目录
    } else {
        仓库根.to_path_buf()
    }
}

/// 拉取注册中心仓库到可访问的本地路径
///
/// - 本地路径注册中心：直接复用，无需下载；
/// - 远程地址：优先浅克隆，失败回退压缩包下载。
///
/// 返回临时句柄（持有生命周期）与仓库根路径。
fn 拉取仓库() -> anyhow::Result<(临时目录句柄, PathBuf)> {
    let 地址 = 仓库地址();
    // 本地路径注册中心：直接复用（返回的临时句柄为占位，不影响真实目录）
    if Path::new(&地址).is_dir() {
        let 临时 = 临时目录句柄::新建()?;
        return Ok((临时, PathBuf::from(&地址)));
    }
    let 临时 = 临时目录句柄::新建()?;
    let 源项 = &收集仓库源()[0];
    let 克隆目录 = 临时.存放路径().join("repo");
    let 克隆成功 = Command::new("git")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(&地址)
        .arg(&克隆目录)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|退出状态值| 退出状态值.success())
        .unwrap_or(false);
    if 克隆成功 && 仓库子目录(&克隆目录).join("index.json").is_file() {
        return Ok((临时, 克隆目录));
    }
    // 回退压缩包下载（压缩包拉取已处理单层根目录下沉）
    let 仓库目录 = crate::语言管理::拉取仓库压缩包(源项, &临时)?;
    if 仓库子目录(&仓库目录).join("index.json").is_file() {
        return Ok((临时, 仓库目录));
    }
    anyhow::bail!("注册中心索引 index.json 未找到（仓库：{}）", 仓库地址())
}

/// 读取索引文件（缺失时返回空索引）
fn 读取索引(路径: &Path) -> anyhow::Result<包索引> {
    if !路径.is_file() {
        return Ok(包索引::default());
    }
    let 内容串 = std::fs::read_to_string(路径)?;
    let 索引对象: 包索引 = serde_json::from_str(&内容串).map_err(|错误值| {
        anyhow::anyhow!(
            "{}",
            界面::界面消息表::全局表()
                .取带参消息("crate_index_parse_failed", &[&错误值.to_string()])
        )
    })?;
    Ok(索引对象)
}

/// 写入索引文件（美化格式）
fn 写入索引(路径: &Path, 索引对象: &包索引) -> anyhow::Result<()> {
    if let Some(父级) = 路径.parent() {
        std::fs::create_dir_all(父级)?;
    }
    std::fs::write(路径, serde_json::to_string_pretty(索引对象)?)?;
    Ok(())
}

/// 已安装清单路径：~/.rz/crate-registry.json
fn 已装清单路径() -> PathBuf {
    crate::语言管理::全局语言目录()
        .parent()
        .map(|父级| 父级.join("crate-registry.json"))
        .unwrap_or_else(|| PathBuf::from("crate-registry.json"))
}

/// 读取已安装清单（(库, 语言) → 条目）
fn 读取已装清单() -> BTreeMap<(String, String), 映射条目> {
    let 路径 = 已装清单路径();
    if let Ok(内容串) = std::fs::read_to_string(&路径)
        && let Ok(清单) = serde_json::from_str::<Vec<映射条目>>(&内容串)
    {
        return 清单
            .into_iter()
            .map(|条目值| ((条目值.包名.clone(), 条目值.语言代码.clone()), 条目值))
            .collect();
    }
    BTreeMap::new()
}

/// 写入已安装清单
fn 写入已装清单(
    清单映射: &BTreeMap<(String, String), 映射条目>
) -> anyhow::Result<()> {
    let 路径 = 已装清单路径();
    if let Some(父级) = 路径.parent() {
        std::fs::create_dir_all(父级)?;
    }
    let 列表: Vec<映射条目> = 清单映射.values().cloned().collect();
    std::fs::write(路径, serde_json::to_string_pretty(&列表)?)?;
    Ok(())
}

/// `rzc crate search [关键词]`：检索注册中心索引
pub fn 检索(关键词: Option<&str>) -> anyhow::Result<()> {
    let 界面表 = 界面::界面消息表::全局表();
    let (_临时, 根目录) = 拉取仓库()?;
    let 注册根 = 仓库子目录(&根目录);
    let 索引对象 = 读取索引(&注册根.join("index.json"))?;
    let 小写关键词 = 关键词.map(|原文| 原文.to_lowercase()).unwrap_or_default();
    let mut 匹配项: Vec<&映射条目> = 索引对象
        .映射清单
        .iter()
        .filter(|条目值| {
            小写关键词.is_empty()
                || 条目值.包名.to_lowercase().contains(&小写关键词)
                || 条目值.语言代码.to_lowercase().contains(&小写关键词)
                || 条目值.作者.to_lowercase().contains(&小写关键词)
        })
        .collect();
    匹配项.sort_by(|甲, 乙| 乙.下载计数.cmp(&甲.下载计数).then(甲.包名.cmp(&乙.包名)));
    if 匹配项.is_empty() {
        println!("{}", 界面表.取消息("crate_search_empty"));
        return Ok(());
    }
    for 条目值 in &匹配项 {
        println!(
            "  {:<18} {:<6} v{:<6} 下载 {:>5}  {}",
            条目值.包名, 条目值.语言代码, 条目值.版本, 条目值.下载计数, 条目值.作者
        );
    }
    println!();
    println!("{}", 界面表.取消息("crate_search_hint"));
    Ok(())
}

/// 校验库名可作为全局语言包内 `crates/<库名>.toml` 的文件名
///
/// 只允许 ASCII 字母、数字、连字符与下划线，拒绝空名与前导 `-`
/// （避免被后续命令当作选项），长度上限 64 与官方注册中心的包名规则一致。
/// 库名参与路径拼接：未校验时 `../` 可写/删全局语言目录外的任意 `.toml`。
fn 校验库名(包名: &str) -> anyhow::Result<()> {
    let 界面表 = 界面::界面消息表::全局表();
    if 包名.is_empty() {
        anyhow::bail!("{}", 界面表.取消息("mapping_crate_empty"));
    }
    let 合法 = 包名.len() <= 64
        && !包名.starts_with("-")
        && 包名
            .chars()
            .all(|本字| 本字.is_ascii_alphanumeric() || 本字 == '-' || 本字 == '_');
    if !合法 {
        anyhow::bail!("{}", 界面表.取带参消息("mapping_crate_invalid", &[包名]));
    }
    Ok(())
}

/// 校验语言代码与库名（`rzc crate` 各子命令的公共入口校验）
fn 校验语言与库名(语言代码: &str, 包名: &str) -> anyhow::Result<()> {
    let 界面表 = 界面::界面消息表::全局表();
    if !校验语言代码(语言代码) {
        anyhow::bail!("{}", 界面表.取带参消息("invalid_lang_code", &[语言代码]));
    }
    校验库名(包名)
}

/// 全局语言包内某语言的社区映射目录
fn 映射目录(语言代码: &str) -> PathBuf {
    crate::语言管理::全局语言目录()
        .join(语言代码)
        .join("crates")
}

/// 全局语言包内的映射安装路径
fn 映射安装路径(语言代码: &str, 包名: &str) -> PathBuf {
    映射目录(语言代码).join(format!("{}.toml", 包名))
}

/// 相对路径按当前目录补成绝对路径（取不到当前目录时原样返回）
fn 绝对化路径(路径值: &Path) -> PathBuf {
    if 路径值.is_absolute() {
        return 路径值.to_path_buf();
    }
    std::env::current_dir()
        .map(|当前目录| 当前目录.join(路径值))
        .unwrap_or_else(|_| 路径值.to_path_buf())
}

/// 目标路径（含尚未创建的末段）是否仍在全局语言目录子树内
///
/// 词法校验已拒绝 `..`；这里再解析真实路径做纵深防御，防止语言目录本身
/// 被替换为指向别处的符号链接。路径尚不存在时回退到父目录的规范路径。
fn 在全局语言目录内(路径值: &Path) -> bool {
    let 根 = 绝对化路径(&crate::语言管理::全局语言目录());
    let 规范根 = 根.canonicalize().unwrap_or_else(|_| 根.clone());
    let 目标 = 绝对化路径(路径值);
    let 规范值 = 目标.canonicalize().unwrap_or_else(|_| {
        目标
            .parent()
            .map(|父级| 父级.canonicalize().unwrap_or_else(|_| 父级.to_path_buf()))
            .unwrap_or_else(|| 目标.clone())
    });
    规范值.starts_with(&规范根)
}

/// 将索引文件中（第三方可控）的映射相对路径安全解析到注册中心根下。
///
/// 复用语言管理器的安全条目路径校验（拒空/绝对/上跳/盘符，
/// 与压缩包条目名同一已审清洗器），并二次确认结果仍在注册根子树内——防恶意
/// 索引的映射文件字段借复制文件把仓库外的任意本地文件读入安装位。非法即 `无`。
fn 解析映射源(注册根: &Path, 映射文件: &str) -> Option<PathBuf> {
    let 相对路径 = crate::语言管理::安全条目路径(映射文件)?;
    let 源路径 = 注册根.join(相对路径);
    源路径.starts_with(注册根).then_some(源路径)
}

/// `rzc crate install <库> --lang <语言> [--force]`：拉取单个映射
pub fn 安装(包名: &str, 语言代码: &str, 强制: bool) -> anyhow::Result<()> {
    let 界面表 = 界面::界面消息表::全局表();
    校验语言与库名(语言代码, 包名)?;
    let (_临时, 根目录) = 拉取仓库()?;
    let 注册根 = 仓库子目录(&根目录);
    let 索引对象 = 读取索引(&注册根.join("index.json"))?;
    let 条目值 = 索引对象
        .映射清单
        .iter()
        .find(|候选条目| 候选条目.包名 == 包名 && 候选条目.语言代码 == 语言代码)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{}",
                界面表.取带参消息("crate_not_found", &[包名, 语言代码])
            )
        })?;
    let 源路径 = 解析映射源(&注册根, &条目值.映射文件).ok_or_else(|| {
        anyhow::anyhow!(
            "{}",
            界面表.取带参消息("crate_file_missing", &[&条目值.映射文件])
        )
    })?;
    if !源路径.is_file() {
        anyhow::bail!(
            "{}",
            界面表.取带参消息("crate_file_missing", &[&条目值.映射文件])
        );
    }
    let 目标目录 = 映射目录(语言代码);
    let 目标路径 = 映射安装路径(语言代码, 包名);
    if !在全局语言目录内(&目标路径) {
        anyhow::bail!("{}", 界面表.取带参消息("mapping_crate_invalid", &[包名]));
    }
    std::fs::create_dir_all(&目标目录)?;
    if 目标路径.exists() && !强制 {
        anyhow::bail!(
            "{}",
            界面表.取带参消息("crate_already_installed", &[包名, 语言代码])
        );
    }
    std::fs::copy(&源路径, &目标路径)?;
    let mut 已装清单 = 读取已装清单();
    已装清单.insert((包名.to_string(), 语言代码.to_string()), 条目值.clone());
    写入已装清单(&已装清单)?;
    println!(
        "{}",
        界面表.取带参消息(
            "crate_installed",
            &[包名, 语言代码, &目标路径.display().to_string()]
        )
    );
    Ok(())
}

/// `rzc crate list`：列出已安装的社区映射
pub fn 列出() -> anyhow::Result<()> {
    let 界面表 = 界面::界面消息表::全局表();
    let 已装清单 = 读取已装清单();
    if 已装清单.is_empty() {
        println!("{}", 界面表.取消息("crate_list_empty"));
        return Ok(());
    }
    println!(
        "{}",
        界面表.取带参消息("crate_list_header", &[&已装清单.len().to_string()])
    );
    for ((包名, 语言代码), 条目值) in &已装清单 {
        println!(
            "  {:<18} {:<6} v{}  {}",
            包名, 语言代码, 条目值.版本, 条目值.作者
        );
    }
    Ok(())
}

/// `rzc crate remove <库> --lang <语言>`：删除已安装映射
pub fn 移除已装(包名: &str, 语言代码: &str) -> anyhow::Result<()> {
    let 界面表 = 界面::界面消息表::全局表();
    校验语言与库名(语言代码, 包名)?;
    let 目标路径 = 映射安装路径(语言代码, 包名);
    if !在全局语言目录内(&目标路径) {
        anyhow::bail!("{}", 界面表.取带参消息("mapping_crate_invalid", &[包名]));
    }
    if 目标路径.exists() {
        std::fs::remove_file(&目标路径)?;
    }
    let mut 已装清单 = 读取已装清单();
    if 已装清单
        .remove(&(包名.to_string(), 语言代码.to_string()))
        .is_some()
    {
        写入已装清单(&已装清单)?;
    }
    println!("{}", 界面表.取带参消息("crate_removed", &[包名, 语言代码]));
    Ok(())
}

/// `rzc crate update`：按清单重新拉取所有已安装映射（获取他人更新）
pub fn 更新() -> anyhow::Result<()> {
    let 界面表 = 界面::界面消息表::全局表();
    let 已装清单 = 读取已装清单();
    if 已装清单.is_empty() {
        println!("{}", 界面表.取消息("crate_list_empty"));
        return Ok(());
    }
    let (_临时, 根目录) = 拉取仓库()?;
    let 注册根 = 仓库子目录(&根目录);
    let 索引对象 = 读取索引(&注册根.join("index.json"))?;
    let mut 计数: usize = 0;
    for (包名, 语言代码) in 已装清单.keys() {
        // 清单是本地文件，可能被篡改：非法条目跳过而不是拼出目录外路径
        if 校验语言与库名(语言代码, 包名).is_err() {
            continue;
        }
        if let Some(条目值) = 索引对象
            .映射清单
            .iter()
            .find(|候选条目| &候选条目.包名 == 包名 && &候选条目.语言代码 == 语言代码)
        {
            let Some(源路径) = 解析映射源(&注册根, &条目值.映射文件) else {
                continue;
            };
            if 源路径.is_file() {
                let 目标目录 = 映射目录(语言代码);
                let 目标路径 = 映射安装路径(语言代码, 包名);
                if !在全局语言目录内(&目标路径) {
                    continue;
                }
                std::fs::create_dir_all(&目标目录)?;
                std::fs::copy(&源路径, 目标路径)?;
                计数 += 1;
            }
        }
    }
    println!(
        "{}",
        界面表.取带参消息(
            "crate_updated",
            &[&计数.to_string(), &已装清单.len().to_string()]
        )
    );
    Ok(())
}

/// `rzc crate publish <库> --lang <语言> [--file <路径>] [--author <名>]`：上传映射
pub fn 发布(
    包名: &str,
    语言代码: &str,
    指定文件: Option<PathBuf>,
    作者: Option<&str>,
) -> anyhow::Result<()> {
    let 界面表 = 界面::界面消息表::全局表();
    // 发布路径同样由库名/语言代码拼接（本地注册中心直写），入口统一校验
    校验语言与库名(语言代码, 包名)?;
    // 1. 定位待发布的映射文件
    let 源路径 = 解析发布文件(包名, 语言代码, 指定文件)?;
    // 2. 质量门禁：配置可解析 + 节名合法（重复键解析即失败）
    let 内容串 = std::fs::read_to_string(&源路径)?;
    校验映射表(&内容串)?;
    // 3. 写入注册中心（本地路径直写；远程地址克隆后提交推送）
    let 仓库地址值 = 仓库地址();
    let 署名 = 作者
        .map(String::from)
        .or_else(取提交者名)
        .unwrap_or_else(|| "anonymous".to_string());
    if Path::new(&仓库地址值).is_dir() {
        写入本地仓库(Path::new(&仓库地址值), 包名, 语言代码, &内容串, &署名)?;
    } else {
        写入远程仓库(&仓库地址值, 包名, 语言代码, &内容串, &署名)?;
    }
    println!(
        "{}",
        界面表.取带参消息("crate_published", &[包名, 语言代码])
    );
    Ok(())
}

/// 解析待发布文件：显式文件优先，否则按常见位置查找
fn 解析发布文件(
    包名: &str,
    语言代码: &str,
    指定文件: Option<PathBuf>,
) -> anyhow::Result<PathBuf> {
    let 界面表 = 界面::界面消息表::全局表();
    if let Some(文件值) = 指定文件 {
        if 文件值.is_file() {
            return Ok(文件值);
        }
        anyhow::bail!(
            "{}",
            界面表.取带参消息(
                "crate_publish_file_missing",
                &[&文件值.display().to_string()]
            )
        );
    }
    let 候选路径 = [
        crate::语言管理::全局语言目录()
            .join(语言代码)
            .join("crates")
            .join(format!("{}.toml", 包名)),
        crate::语言包根目录(Path::new("."))
            .join(语言代码)
            .join("crates")
            .join(format!("{}.toml", 包名)),
        PathBuf::from(format!("{}.toml", 包名)),
        PathBuf::from(语言代码)
            .join("crates")
            .join(format!("{}.toml", 包名)),
    ];
    for 候选 in &候选路径 {
        if 候选.is_file() {
            return Ok(候选.clone());
        }
    }
    anyhow::bail!(
        "{}",
        界面表.取带参消息("crate_publish_not_found", &[包名, 语言代码])
    );
}

/// 质量门禁：映射文件必须是合法配置表格，且仅含允许的节
fn 校验映射表(内容串: &str) -> anyhow::Result<()> {
    let 界面表 = 界面::界面消息表::全局表();
    let 值对象: toml::Value = toml::from_str(内容串).map_err(|错误值| {
        anyhow::anyhow!(
            "{}",
            界面表.取带参消息("crate_publish_invalid_toml", &[&错误值.to_string()])
        )
    })?;
    let 表格 = 值对象
        .as_table()
        .ok_or_else(|| anyhow::anyhow!("{}", 界面表.取消息("crate_publish_not_table")))?;
    let 允许节 = ["模块路径", "标识符", "解释"];
    for 节名 in 表格.keys() {
        if !允许节.contains(&节名.as_str()) {
            anyhow::bail!(
                "{}",
                界面表.取带参消息("crate_publish_bad_section", &[节名.as_str()])
            );
        }
    }
    Ok(())
}

/// 取版本控制用户名（发布署名用）
fn 取提交者名() -> Option<String> {
    Command::new("git")
        .arg("config")
        .arg("user.name")
        .output()
        .ok()
        .filter(|子进程输出| 子进程输出.status.success())
        .and_then(|子进程输出| String::from_utf8(子进程输出.stdout).ok())
        .map(|文本值| 文本值.trim().to_string())
        .filter(|文本值| !文本值.is_empty())
}

/// 写入本地路径注册中心并提交（尽力而为）
fn 写入本地仓库(
    仓库目录: &Path,
    包名: &str,
    语言代码: &str,
    内容串: &str,
    署名: &str,
) -> anyhow::Result<()> {
    let 界面表 = 界面::界面消息表::全局表();
    let 注册根 = 仓库子目录(仓库目录);
    let 目标路径 = 注册根
        .join(语言代码)
        .join("crates")
        .join(format!("{}.toml", 包名));
    if let Some(父级) = 目标路径.parent() {
        std::fs::create_dir_all(父级)?;
    }
    std::fs::write(&目标路径, 内容串)?;
    let 索引路径 = 注册根.join("index.json");
    let mut 索引对象 = 读取索引(&索引路径)?;
    置入或更新条目(&mut 索引对象, 包名, 语言代码, 署名);
    写入索引(&索引路径, &索引对象)?;
    提交改动(仓库目录, &format!("publish {} ({})", 包名, 语言代码));
    println!(
        "{}",
        界面表.取带参消息(
            "crate_publish_wrote_local",
            &[&仓库目录.display().to_string()]
        )
    );
    Ok(())
}

/// 写入远程注册中心：克隆 → 写文件与索引 → 提交 → 推送
fn 写入远程仓库(
    地址: &str,
    包名: &str,
    语言代码: &str,
    内容串: &str,
    署名: &str,
) -> anyhow::Result<()> {
    let 界面表 = 界面::界面消息表::全局表();
    let 临时 = 临时目录句柄::新建()?;
    let 克隆目录 = 临时.存放路径().join("repo");
    let 克隆成功 = Command::new("git")
        .arg("clone")
        .arg(地址)
        .arg(&克隆目录)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|退出状态值| 退出状态值.success())
        .unwrap_or(false);
    if !克隆成功 {
        anyhow::bail!(
            "{}",
            界面表.取带参消息("crate_publish_clone_failed", &[地址])
        );
    }
    let 注册根 = 仓库子目录(&克隆目录);
    let 目标路径 = 注册根
        .join(语言代码)
        .join("crates")
        .join(format!("{}.toml", 包名));
    if let Some(父级) = 目标路径.parent() {
        std::fs::create_dir_all(父级)?;
    }
    std::fs::write(&目标路径, 内容串)?;
    let 索引路径 = 注册根.join("index.json");
    let mut 索引对象 = 读取索引(&索引路径)?;
    置入或更新条目(&mut 索引对象, 包名, 语言代码, 署名);
    写入索引(&索引路径, &索引对象)?;
    提交改动(&克隆目录, &format!("publish {} ({})", 包名, 语言代码));
    let 推送结果 = Command::new("git")
        .arg("push")
        .current_dir(&克隆目录)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output();
    if let Ok(输出值) = &推送结果 {
        if !输出值.status.success() {
            let 错误文本 = String::from_utf8_lossy(&输出值.stderr);
            anyhow::bail!(
                "{}",
                界面表.取带参消息("crate_publish_push_failed", &[错误文本.trim()])
            );
        }
    } else if let Err(错误值) = &推送结果 {
        anyhow::bail!(
            "{}",
            界面表.取带参消息("crate_publish_push_failed", &[&错误值.to_string()])
        );
    }
    Ok(())
}

/// 在索引中置入或更新一条映射条目
fn 置入或更新条目(
    索引对象: &mut 包索引, 包名: &str, 语言代码: &str, 署名: &str
) {
    let 今日 = 今日世界标准时();
    let 映射文件 = format!("{}/crates/{}.toml", 语言代码, 包名);
    if let Some(既有条目) = 索引对象
        .映射清单
        .iter_mut()
        .find(|候选条目| 候选条目.包名 == 包名 && 候选条目.语言代码 == 语言代码)
    {
        既有条目.版本 = 递增版本(&既有条目.版本);
        既有条目.作者 = 署名.to_string();
        既有条目.更新日期 = 今日.clone();
        既有条目.映射文件 = 映射文件;
    } else {
        索引对象.映射清单.push(映射条目 {
            包名: 包名.to_string(),
            语言代码: 语言代码.to_string(),
            版本: "1.0".to_string(),
            作者: 署名.to_string(),
            许可证: "MIT".to_string(),
            质量分: 1.0,
            下载计数: 0,
            映射文件,
            更新日期: 今日.clone(),
        });
    }
    索引对象.更新日期 = 今日.clone();
    索引对象
        .映射清单
        .sort_by(|甲, 乙| 甲.包名.cmp(&乙.包名).then(甲.语言代码.cmp(&乙.语言代码)));
}

/// 版本号末段加一（1.0 → 1.1；无法解析则回退 1.0）
fn 递增版本(版本串: &str) -> String {
    if let Some((主段, 尾段)) = 版本串.split_once('.')
        && let Ok(尾号) = 尾段.parse::<u32>()
    {
        return format!("{}.{}", 主段, 尾号 + 1);
    }
    "1.0".to_string()
}

/// 提交改动（尽力而为，失败不影响文件写入结果）
fn 提交改动(仓库目录: &Path, 提交消息: &str) {
    let _ = Command::new("git")
        .arg("add")
        .arg("-A")
        .current_dir(仓库目录)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let _ = Command::new("git")
        .arg("commit")
        .arg("-m")
        .arg(提交消息)
        .current_dir(仓库目录)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// 当前世界标准时日期，无外部时间依赖
fn 今日世界标准时() -> String {
    let 总秒数 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|时长| 时长.as_secs())
        .unwrap_or(0);
    let mut 剩余日数 = (总秒数 / 86_400) as i64;
    let mut 年份: i64 = 1970;
    loop {
        let 闰日数 = if (年份 % 4 == 0 && 年份 % 100 != 0) || 年份 % 400 == 0 {
            366
        } else {
            365
        };
        if 剩余日数 < 闰日数 {
            break;
        }
        剩余日数 -= 闰日数;
        年份 += 1;
    }
    let 月天数 = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let 闰年 = (年份 % 4 == 0 && 年份 % 100 != 0) || 年份 % 400 == 0;
    let mut 月份索引: usize = 0;
    let mut 余日 = 剩余日数;
    loop {
        let 本月天数 = 月天数[月份索引] + if 月份索引 == 1 && 闰年 { 1 } else { 0 };
        if 余日 < 本月天数 {
            break;
        }
        余日 -= 本月天数;
        月份索引 += 1;
    }
    format!("{:04}-{:02}-{:02}", 年份, 月份索引 + 1, 余日 + 1)
}

#[cfg(test)]
mod 母语测试 {
    use super::*;

    #[test]
    fn 今日日期格式正确() {
        let 日期串 = 今日世界标准时();
        assert_eq!(日期串.len(), 10);
        assert!(日期串.starts_with("20")); // 二十一世纪
        assert_eq!(日期串.as_bytes()[4], b'-');
        assert_eq!(日期串.as_bytes()[7], b'-');
    }

    /// 索引文件的映射文件字段是第三方可控数据：合法相对路径解析到注册根下，
    /// 上跳/绝对/盘符均拒绝（防复制文件读仓库外任意本地文件）。
    #[test]
    fn 解析映射源拒绝路径穿越() {
        let 注册根 = Path::new("/repo/third-party");
        let 合法结果 = 解析映射源(注册根, "zh/crates/foo.toml");
        let 合法结果 = 合法结果.expect("合法相对路径应解析成功");
        assert!(
            合法结果.starts_with(注册根),
            "解析结果应在注册根子树内：{合法结果:?}"
        );
        assert!(
            合法结果.ends_with(Path::new("zh").join("crates").join("foo.toml")),
            "应保留完整相对段：{合法结果:?}"
        );
        assert!(解析映射源(注册根, "../secret").is_none(), "上跳应拒");
        assert!(解析映射源(注册根, "a/../../b").is_none(), "内嵌上跳应拒");
        assert!(解析映射源(注册根, "/etc/passwd").is_none(), "绝对路径应拒");
        assert!(
            解析映射源(注册根, "C:/Windows/x").is_none(),
            "盘符应拒（连接会整体替换根路径）"
        );
        assert!(解析映射源(注册根, "").is_none(), "空名应拒");
    }

    /// 库名/语言代码是路径段：`../`、分隔符、前导 `-` 与空名必须拒绝，
    /// 否则 `rzc crate install/remove` 可写/删全局语言目录外的任意 `.toml`
    #[test]
    fn 库名与语言代码拒绝路径穿越() {
        assert!(校验库名("serde").is_ok());
        assert!(校验库名("serde_json").is_ok());
        assert!(校验库名("serde-json2").is_ok());
        assert!(校验库名("").is_err(), "空名应拒");
        assert!(校验库名("..").is_err(), "上跳应拒");
        assert!(校验库名("../../.cargo/config").is_err(), "路径穿越应拒");
        assert!(校验库名("a/b").is_err(), "正斜杠应拒");
        assert!(校验库名("a\\b").is_err(), "反斜杠应拒");
        assert!(校验库名("中文").is_err(), "非 ASCII 应拒");
        assert!(校验库名("-evil").is_err(), "前导连字符应拒");
        assert!(校验库名(&"a".repeat(65)).is_err(), "超长应拒");
        assert!(校验语言代码("../zh") == false);
        assert!(校验语言代码("..") == false);
        assert!(校验语言代码("zh") == true);
    }

    /// 安装路径始终落在全局语言目录内（纵深防御的规范路径检查）
    #[test]
    fn 映射安装路径限制在全局语言目录内() {
        assert!(在全局语言目录内(&映射安装路径("zh", "serde")));
        assert!(!在全局语言目录内(&Path::new("/etc/passwd")));
    }

    #[test]
    fn 版本号递增规则() {
        assert_eq!(递增版本("1.0"), "1.1");
        assert_eq!(递增版本("2.3"), "2.4");
        assert_eq!(递增版本("garbage"), "1.0");
    }

    #[test]
    fn 校验映射表各分支() {
        // 合法：仅含允许的节
        assert!(校验映射表("[\"标识符\"]\n\"服务器\" = \"Server\"\n").is_ok());
        // 非法节名
        assert!(校验映射表("[\"未知节\"]\n\"a\" = \"b\"\n").is_err());
        // 重复键：配置解析失败
        assert!(校验映射表("[\"标识符\"]\n\"a\" = \"b\"\n\"a\" = \"c\"\n").is_err());
    }

    #[test]
    fn 索引往返序列化() {
        let mut 索引对象 = 包索引::default();
        置入或更新条目(&mut 索引对象, "serde", "zh", "tan80");
        置入或更新条目(&mut 索引对象, "tokio", "zh", "alice");
        assert_eq!(索引对象.映射清单.len(), 2);
        // 重复置入应更新版本而非新增
        置入或更新条目(&mut 索引对象, "serde", "zh", "bob");
        assert_eq!(索引对象.映射清单.len(), 2);
        let 序列化条目 = 索引对象
            .映射清单
            .iter()
            .find(|候选条目| 候选条目.包名 == "serde" && 候选条目.语言代码 == "zh")
            .unwrap();
        assert_eq!(序列化条目.版本, "1.1");
        assert_eq!(序列化条目.作者, "bob");
        // 序列化再反序列化
        let 索引文本 = serde_json::to_string_pretty(&索引对象).unwrap();
        let 回读索引: 包索引 = serde_json::from_str(&索引文本).unwrap();
        assert_eq!(回读索引.映射清单.len(), 2);
    }

    /// 注册中心子目录两种布局：合并进主仓库后在 `third-party/`，独立仓库在根
    #[test]
    fn 注册中心子目录两种布局() {
        let 临时 = tempfile::tempdir().unwrap();
        // 平铺布局：索引文件在根
        let 平铺根 = 临时.path().join("flat");
        std::fs::create_dir_all(&平铺根).unwrap();
        std::fs::write(平铺根.join("index.json"), "{}").unwrap();
        assert_eq!(仓库子目录(&平铺根), 平铺根);

        // 嵌套布局：third-party/index.json
        let 嵌套根 = 临时.path().join("nested");
        let 嵌套子目录 = 嵌套根.join("third-party");
        std::fs::create_dir_all(&嵌套子目录).unwrap();
        std::fs::write(嵌套子目录.join("index.json"), "{}").unwrap();
        assert_eq!(仓库子目录(&嵌套根), 嵌套子目录);
    }

    /// 读取索引：缺失 → 空索引；合法 → 解析；损坏文件 → 报错
    #[test]
    fn 读取索引缺失合法损坏() {
        let 临时 = tempfile::tempdir().unwrap();
        let 缺失路径 = 临时.path().join("absent.json");
        assert!(读取索引(&缺失路径).unwrap().映射清单.is_empty());

        let 正常文件 = 临时.path().join("good.json");
        let mut 索引对象 = 包索引::default();
        置入或更新条目(&mut 索引对象, "serde", "zh", "tan80");
        写入索引(&正常文件, &索引对象).unwrap();
        assert_eq!(读取索引(&正常文件).unwrap().映射清单.len(), 1);

        let 损坏文件 = 临时.path().join("bad.json");
        std::fs::write(&损坏文件, "{ 不是合法 JSON").unwrap();
        assert!(读取索引(&损坏文件).is_err());
    }

    /// 写入索引：父目录不存在时逐级创建，可回读
    #[test]
    fn 写入索引自动创建父目录() {
        let 临时 = tempfile::tempdir().unwrap();
        let 深层路径 = 临时.path().join("a").join("b").join("index.json");
        写入索引(&深层路径, &包索引::default()).unwrap();
        assert!(深层路径.is_file());
        assert_eq!(读取索引(&深层路径).unwrap().映射清单.len(), 0);
    }

    /// 新条目默认字段齐全且按 (库, 语言) 排序；映射文件路径与注册中心布局一致
    #[test]
    fn 置入条目默认值与排序() {
        let mut 索引对象 = 包索引::default();
        置入或更新条目(&mut 索引对象, "tokio", "zh", "alice");
        置入或更新条目(&mut 索引对象, "serde", "ja", "bob");
        置入或更新条目(&mut 索引对象, "serde", "zh", "carol");
        let 键列表: Vec<_> = 索引对象
            .映射清单
            .iter()
            .map(|条目值| (条目值.包名.as_str(), 条目值.语言代码.as_str()))
            .collect();
        assert_eq!(
            键列表,
            vec![("serde", "ja"), ("serde", "zh"), ("tokio", "zh")]
        );
        let 首项 = &索引对象.映射清单[0];
        assert_eq!(首项.版本, "1.0");
        assert_eq!(首项.许可证, "MIT");
        assert_eq!(首项.质量分, 1.0);
        assert_eq!(首项.下载计数, 0);
        assert_eq!(首项.映射文件, "ja/crates/serde.toml");
        assert_eq!(首项.更新日期.len(), 10);
        assert_eq!(索引对象.更新日期, 首项.更新日期);
    }

    /// 显式指定文件：存在即采用；不存在报错（不进入候选路径扫描）
    #[test]
    fn 解析发布文件显式分支() {
        let 临时 = tempfile::tempdir().unwrap();
        let 已存在文件 = 临时.path().join("mine.toml");
        std::fs::write(&已存在文件, "[\"标识符\"]\n\"a\" = \"b\"\n").unwrap();
        assert_eq!(
            解析发布文件("serde", "zh", Some(已存在文件.clone())).unwrap(),
            已存在文件
        );
        let 缺失文件 = 临时.path().join("absent.toml");
        assert!(解析发布文件("serde", "zh", Some(缺失文件)).is_err());
    }

    /// 写入本地（嵌套 `third-party` 布局，即并入主仓库后的布局）：
    /// 映射文件 + 索引条目一并落盘，版本控制提交尽力而为（非仓库目录静默失败）
    #[test]
    fn 写入本地仓库嵌套布局() {
        let 临时 = tempfile::tempdir().unwrap();
        let 仓库目录 = 临时.path().join("repo");
        // 真实主仓库中 `third-party/index.json` 已存在，写入方据此识别嵌套布局
        let 第三方目录 = 仓库目录.join("third-party");
        std::fs::create_dir_all(&第三方目录).unwrap();
        写入索引(&第三方目录.join("index.json"), &包索引::default()).unwrap();

        let 内容串 = "[\"标识符\"]\n\"服务器\" = \"Server\"\n";
        写入本地仓库(&仓库目录, "serde", "zh", 内容串, "tan80").unwrap();

        let 映射路径 = 第三方目录.join("zh/crates/serde.toml");
        assert_eq!(std::fs::read_to_string(&映射路径).unwrap(), 内容串);
        let 索引对象 = 读取索引(&第三方目录.join("index.json")).unwrap();
        assert_eq!(索引对象.映射清单.len(), 1);
        let 条目值 = &索引对象.映射清单[0];
        assert_eq!(条目值.包名, "serde");
        assert_eq!(条目值.语言代码, "zh");
        assert_eq!(条目值.版本, "1.0");
        assert_eq!(条目值.映射文件, "zh/crates/serde.toml");
    }

    /// 写入本地（独立注册中心仓库的平铺布局）：映射文件直接落在 <语言>/crates/
    #[test]
    fn 写入本地仓库平铺布局() {
        let 临时 = tempfile::tempdir().unwrap();
        let 仓库目录 = 临时.path().join("repo");
        std::fs::create_dir_all(&仓库目录).unwrap();
        写入索引(&仓库目录.join("index.json"), &包索引::default()).unwrap();
        写入本地仓库(
            &仓库目录,
            "tokio",
            "ja",
            "[\"标识符\"]\n\"a\"=\"b\"\n",
            "alice",
        )
        .unwrap();
        assert!(仓库目录.join("ja/crates/tokio.toml").is_file());
        let 索引对象 = 读取索引(&仓库目录.join("index.json")).unwrap();
        assert_eq!(索引对象.映射清单.len(), 1);
        assert_eq!(索引对象.映射清单[0].映射文件, "ja/crates/tokio.toml");
    }

    /// 已安装清单：缺失 → 空表；写入 → 按 (库, 语言) 键回读；损坏 → 回退空表
    #[test]
    fn 已装清单往返与损坏回退() {
        let _守卫 = crate::语言管理::tests::环境锁();
        let 临时 = tempfile::tempdir().unwrap();
        let 已存值 = std::env::var("RZ_LANG_DIR").ok();
        unsafe {
            std::env::set_var("RZ_LANG_DIR", 临时.path().join("lang-packs"));
        }
        let 还原 = || unsafe {
            if let Some(原值) = &已存值 {
                std::env::set_var("RZ_LANG_DIR", 原值);
            } else {
                std::env::remove_var("RZ_LANG_DIR");
            }
        };

        // 首次读取：清单不存在 → 空表（不报错）
        assert!(读取已装清单().is_empty());

        // 经置入条目构造一条真实条目并写清单
        let mut 索引对象 = 包索引::default();
        置入或更新条目(&mut 索引对象, "serde", "zh", "tan80");
        let 条目值 = 索引对象.映射清单[0].clone();
        let mut 清单映射 = BTreeMap::new();
        清单映射.insert(("serde".to_string(), "zh".to_string()), 条目值.clone());
        写入已装清单(&清单映射).unwrap();
        // 清单位于语言包目录的上一级（全局根）
        assert!(临时.path().join("crate-registry.json").is_file());
        let 回读清单 = 读取已装清单();
        // 索引条目未派生部分相等特征（生产无等值比较需求），逐字段核对
        let 读回条目 = 回读清单
            .get(&("serde".to_string(), "zh".to_string()))
            .expect("回读清单应含 serde/zh");
        assert_eq!(读回条目.版本, 条目值.版本);
        assert_eq!(读回条目.作者, 条目值.作者);
        assert_eq!(读回条目.映射文件, 条目值.映射文件);

        // 清单损坏（手工改坏 JSON）→ 回退空表，不传播错误
        std::fs::write(临时.path().join("crate-registry.json"), "[坏").unwrap();
        assert!(读取已装清单().is_empty());

        还原();
    }
}
