//! 第三方库共享注册中心模块
//!
//! 实现 `rzc crate` 子命令：search / install / list / remove / update / publish。
//! 注册中心即一个 Git 仓库（GitCode/GitHub 或任意 git 可达地址 / 本地路径），
//! 根含 index.json 索引，映射按 <语言>/crates/<crate>.toml 存放
//! （与语言包内 crates/ 子目录结构一致）。
//!
//! 远程获取复用 语言包工具 的 仓库源 / 临时目录句柄 / 取仓库压缩 能力；
//! 本地路径注册中心直接复用目录，无需网络。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::语言包工具::{临时目录句柄, 仓库源};

/// 默认注册中心地址（可用 RZ_CRATE_REPO 环境变量覆盖）。
/// 注册中心已合并进 zrRust 仓库，位于 `third-party/` 子目录；
/// 故默认即指向主仓库，克隆后从 `third-party/index.json` 读取。
const 默认库仓库地址: &str = "https://gitcode.com/tan80/i18n-rust";

/// 单条映射在注册中心的索引条目
///
/// 字段名为中文，但 index.json 是与已发布社区仓库共享的外部数据契约（英文键），
/// 故逐字段用 `#[serde(rename)]` 锁定线上键名，改名不影响序列化格式。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct 映射条目 {
    /// crate 名（连字符归一为下划线）
    #[serde(rename = "crate_name")]
    pub 库名: String,
    /// 语言代码（zh/ru/ja/…）
    #[serde(rename = "lang")]
    pub 语言: String,
    /// 映射版本（语义自定，默认 1.0）
    #[serde(rename = "version")]
    pub 版本号: String,
    /// 译者署名
    #[serde(rename = "author")]
    pub 作者: String,
    /// 映射文件许可证（默认 MIT）
    #[serde(rename = "license")]
    pub 许可证: String,
    /// 质量分（0–1，发布校验通过即 1.0）
    #[serde(rename = "quality")]
    pub 质量分: f64,
    /// 下载计数
    #[serde(rename = "downloads")]
    pub 下载数: u64,
    /// 映射文件在仓库内的相对路径
    #[serde(rename = "file")]
    pub 文件路径: String,
    /// 最后更新日期（YYYY-MM-DD）
    #[serde(rename = "updated")]
    pub 更新日期: String,
}

/// 注册中心 index.json 结构
#[derive(Serialize, Deserialize, Default)]
pub struct 注册中心索引 {
    #[serde(rename = "registry")]
    pub 注册中心名: String,
    #[serde(rename = "updated")]
    pub 更新日期: String,
    #[serde(rename = "mappings")]
    pub 映射表: Vec<映射条目>,
}

/// 注册中心仓库地址：RZ_CRATE_REPO 优先，否则默认地址。
///
/// 默认情况下，若当前工作目录是 zrRust 源码树（存在 `third-party/index.json`），
/// 则直接使用该本地子目录，离线即可 search/install；否则回退到远端主仓库。
fn 注册中心仓库地址() -> String {
    if let Ok(变量值) = std::env::var("RZ_CRATE_REPO") {
        let 变量值 = 变量值.trim().trim_end_matches('/').to_string();
        if !变量值.is_empty() {
            return 变量值;
        }
    }
    if Path::new("third-party").join("index.json").is_file() {
        return "third-party".to_string();
    }
    默认库仓库地址.to_string()
}

/// 构造注册中心源（与 lang install 同源回退策略一致）
fn 收集来源() -> Vec<仓库源> {
    vec![仓库源::自地址构造(&注册中心仓库地址())]
}

/// 注册中心在仓库内的实际目录。合并进 zrRust 后位于 `third-party/` 子目录，
/// 独立仓库则直接在根；两种布局都兼容。
fn 注册中心子目录(仓库根: &Path) -> PathBuf {
    let 嵌套 = 仓库根.join("third-party");
    if 嵌套.join("index.json").is_file() {
        嵌套
    } else {
        仓库根.to_path_buf()
    }
}

/// 拉取注册中心仓库到可访问的本地路径
///
/// - 本地路径注册中心：直接复用，无需下载；
/// - 远程地址：优先 `git clone --depth 1`，失败回退 `curl` 下载 ZIP。
///
/// 返回临时句柄（持有生命周期）与仓库根路径。
fn 拉取注册中心仓库() -> anyhow::Result<(临时目录句柄, PathBuf)> {
    let 网址 = 注册中心仓库地址();
    // 本地路径注册中心：直接复用（返回的临时句柄为占位，不影响真实目录）
    if Path::new(&网址).is_dir() {
        let 临时句柄 = 临时目录句柄::创建()?;
        return Ok((临时句柄, PathBuf::from(&网址)));
    }
    let 临时句柄 = 临时目录句柄::创建()?;
    let 源对象 = &收集来源()[0];
    let 克隆目录 = 临时句柄.目录路径().join("repo");
    let 成功标志 = Command::new("git")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(&源对象.仓库地址)
        .arg(&克隆目录)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|状态| 状态.success())
        .unwrap_or(false);
    if 成功标志 && 注册中心子目录(&克隆目录).join("index.json").is_file() {
        return Ok((临时句柄, 克隆目录));
    }
    // 回退 curl ZIP（取仓库压缩 已处理单层根目录下沉）
    let 仓库目录 = crate::语言包工具::取仓库压缩(源对象, &临时句柄)?;
    if 注册中心子目录(&仓库目录).join("index.json").is_file() {
        return Ok((临时句柄, 仓库目录));
    }
    anyhow::bail!(
        "注册中心索引 index.json 未找到（仓库：{}）",
        注册中心仓库地址()
    )
}

/// 读取 index.json（缺失时返回空索引）
fn 读索引(路径: &Path) -> anyhow::Result<注册中心索引> {
    if !路径.is_file() {
        return Ok(注册中心索引::default());
    }
    let 内容 = fs::read_to_string(路径)?;
    let 索引数据: 注册中心索引 = serde_json::from_str(&内容).map_err(|解析错| {
        anyhow::anyhow!(
            "{}",
            crate::本地化::界面::全局()
                .取文带参("crate_index_parse_failed", &[&解析错.to_string()])
        )
    })?;
    Ok(索引数据)
}

/// 写入 index.json（美化格式）
fn 写索引(路径: &Path, 索引数据: &注册中心索引) -> anyhow::Result<()> {
    if let Some(上层目录) = 路径.parent() {
        fs::create_dir_all(上层目录)?;
    }
    fs::write(路径, serde_json::to_string_pretty(索引数据)?)?;
    Ok(())
}

/// 已安装清单路径：~/.rz/crate-registry.json
fn 已安装清单路径() -> PathBuf {
    crate::语言包工具::全局语言目录()
        .parent()
        .map(|目录段| 目录段.join("crate-registry.json"))
        .unwrap_or_else(|| PathBuf::from("crate-registry.json"))
}

/// 读取已安装清单（(crate, lang) → 条目）
fn 读已安装() -> BTreeMap<(String, String), 映射条目> {
    let 路径 = 已安装清单路径();
    if let Ok(内容) = fs::read_to_string(&路径)
        && let Ok(列表) = serde_json::from_str::<Vec<映射条目>>(&内容)
    {
        return 列表
            .into_iter()
            .map(|条| ((条.库名.clone(), 条.语言.clone()), 条))
            .collect();
    }
    BTreeMap::new()
}

/// 写入已安装清单
fn 写已安装(清单: &BTreeMap<(String, String), 映射条目>) -> anyhow::Result<()> {
    let 路径 = 已安装清单路径();
    if let Some(上层目录) = 路径.parent() {
        fs::create_dir_all(上层目录)?;
    }
    let 列表: Vec<映射条目> = 清单.values().cloned().collect();
    fs::write(路径, serde_json::to_string_pretty(&列表)?)?;
    Ok(())
}

/// `rzc crate search [关键词]`：检索注册中心索引
pub fn 检索映射(关键词参: Option<&str>) -> anyhow::Result<()> {
    let 界面 = crate::本地化::界面::全局();
    let (_, 仓库根) = 拉取注册中心仓库()?;
    let 注册子目录 = 注册中心子目录(&仓库根);
    let 索引数据 = 读索引(&注册子目录.join("index.json"))?;
    let 小写关键词 = 关键词参.map(|串| 串.to_lowercase()).unwrap_or_default();
    let mut 命中项集: Vec<&映射条目> = 索引数据
        .映射表
        .iter()
        .filter(|项| {
            小写关键词.is_empty()
                || 项.库名.to_lowercase().contains(&小写关键词)
                || 项.语言.to_lowercase().contains(&小写关键词)
                || 项.作者.to_lowercase().contains(&小写关键词)
        })
        .collect();
    命中项集.sort_by(|甲, 乙| {
        乙.下载数
            .cmp(&甲.下载数)
            .then_with(|| 甲.库名.cmp(&乙.库名))
    });
    if 命中项集.is_empty() {
        println!("{}", 界面.取文("crate_search_empty"));
        return Ok(());
    }
    for 项 in &命中项集 {
        println!(
            "  {:<18} {:<6} v{:<6} 下载 {:>5}  {}",
            项.库名, 项.语言, 项.版本号, 项.下载数, 项.作者
        );
    }
    println!();
    println!("{}", 界面.取文("crate_search_hint"));
    Ok(())
}

/// `rzc crate install <crate> --lang <语言> [--force]`：拉取单个映射
pub fn 安装映射(库名: &str, 语言: &str, 强制: bool) -> anyhow::Result<()> {
    let 界面 = crate::本地化::界面::全局();
    let (_, 仓库根) = 拉取注册中心仓库()?;
    let 注册子目录 = 注册中心子目录(&仓库根);
    let 索引数据 = 读索引(&注册子目录.join("index.json"))?;
    let 该条目 = 索引数据
        .映射表
        .iter()
        .find(|项| 项.库名 == 库名 && 项.语言 == 语言)
        .ok_or_else(|| anyhow::anyhow!("{}", 界面.取文带参("crate_not_found", &[库名, 语言])))?;
    let 源路径 = 注册子目录.join(&该条目.文件路径);
    if !源路径.is_file() {
        anyhow::bail!(
            "{}",
            界面.取文带参("crate_file_missing", &[&该条目.文件路径])
        );
    }
    let 目标目录 = crate::语言包工具::全局语言目录().join(语言).join("crates");
    fs::create_dir_all(&目标目录)?;
    let 目标路径 = 目标目录.join(format!("{}.toml", 库名));
    if 目标路径.exists() && !强制 {
        anyhow::bail!(
            "{}",
            界面.取文带参("crate_already_installed", &[库名, 语言])
        );
    }
    fs::copy(&源路径, &目标路径)?;
    let mut 已安装 = 读已安装();
    已安装.insert((库名.to_string(), 语言.to_string()), 该条目.clone());
    写已安装(&已安装)?;
    println!(
        "{}",
        界面.取文带参(
            "crate_installed",
            &[库名, 语言, &目标路径.display().to_string()]
        )
    );
    Ok(())
}

/// `rzc crate list`：列出已安装的社区映射
pub fn 列出映射() -> anyhow::Result<()> {
    let 界面 = crate::本地化::界面::全局();
    let 已安装 = 读已安装();
    if 已安装.is_empty() {
        println!("{}", 界面.取文("crate_list_empty"));
        return Ok(());
    }
    println!(
        "{}",
        界面.取文带参("crate_list_header", &[&已安装.len().to_string()])
    );
    for ((库名, 语言), 条目项) in &已安装 {
        println!(
            "  {:<18} {:<6} v{}  {}",
            库名, 语言, 条目项.版本号, 条目项.作者
        );
    }
    Ok(())
}

/// `rzc crate remove <crate> --lang <语言>`：删除已安装映射
pub fn 移除映射(库名: &str, 语言: &str) -> anyhow::Result<()> {
    let 界面 = crate::本地化::界面::全局();
    let 目标路径 = crate::语言包工具::全局语言目录()
        .join(语言)
        .join("crates")
        .join(format!("{}.toml", 库名));
    if 目标路径.exists() {
        fs::remove_file(&目标路径)?;
    }
    let mut 已安装 = 读已安装();
    if 已安装
        .remove(&(库名.to_string(), 语言.to_string()))
        .is_some()
    {
        写已安装(&已安装)?;
    }
    println!("{}", 界面.取文带参("crate_removed", &[库名, 语言]));
    Ok(())
}

/// `rzc crate update`：按清单重新拉取所有已安装映射（获取他人更新）
pub fn 刷新映射() -> anyhow::Result<()> {
    let 界面 = crate::本地化::界面::全局();
    let 已安装 = 读已安装();
    if 已安装.is_empty() {
        println!("{}", 界面.取文("crate_list_empty"));
        return Ok(());
    }
    let (_, 仓库根) = 拉取注册中心仓库()?;
    let 注册子目录 = 注册中心子目录(&仓库根);
    let 索引数据 = 读索引(&注册子目录.join("index.json"))?;
    let mut 更新数量 = 0usize;
    for (库名, 语言) in 已安装.keys() {
        if let Some(该条目) = 索引数据
            .映射表
            .iter()
            .find(|项| &项.库名 == 库名 && &项.语言 == 语言)
        {
            let 源路径 = 注册子目录.join(&该条目.文件路径);
            if 源路径.is_file() {
                let 目标目录 = crate::语言包工具::全局语言目录().join(语言).join("crates");
                fs::create_dir_all(&目标目录)?;
                fs::copy(&源路径, 目标目录.join(format!("{}.toml", 库名)))?;
                更新数量 += 1;
            }
        }
    }
    println!(
        "{}",
        界面.取文带参(
            "crate_updated",
            &[&更新数量.to_string(), &已安装.len().to_string()]
        )
    );
    Ok(())
}

/// `rzc crate publish <crate> --lang <语言> [--file <路径>] [--author <名>]`：上传映射
pub fn 发布映射(
    库名: &str,
    语言: &str,
    文件参: Option<PathBuf>,
    作者参: Option<&str>,
) -> anyhow::Result<()> {
    let 界面 = crate::本地化::界面::全局();
    // 1. 定位待发布的映射文件
    let 源路径 = 解析发布文件(库名, 语言, 文件参)?;
    // 2. 质量门禁：TOML 可解析 + 节名合法（重复键 TOML 解析即失败）
    let 内容 = fs::read_to_string(&源路径)?;
    校验映射文本(&内容)?;
    // 3. 写入注册中心（本地路径直写；远程地址克隆后提交推送）
    let 仓库网址 = 注册中心仓库地址();
    let 作者参 = 作者参
        .map(String::from)
        .or_else(仓库用户名)
        .unwrap_or_else(|| "anonymous".to_string());
    if Path::new(&仓库网址).is_dir() {
        写入本地注册中心(Path::new(&仓库网址), 库名, 语言, &内容, &作者参)?;
    } else {
        写入远程注册中心(&仓库网址, 库名, 语言, &内容, &作者参)?;
    }
    println!("{}", 界面.取文带参("crate_published", &[库名, 语言]));
    Ok(())
}

/// 解析待发布文件：--file 优先，否则按常见位置查找
fn 解析发布文件(
    库名: &str, 语言: &str, 文件参: Option<PathBuf>
) -> anyhow::Result<PathBuf> {
    let 界面 = crate::本地化::界面::全局();
    if let Some(文件项) = 文件参 {
        if 文件项.is_file() {
            return Ok(文件项);
        }
        anyhow::bail!(
            "{}",
            界面.取文带参(
                "crate_publish_file_missing",
                &[&文件项.display().to_string()]
            )
        );
    }
    let 候选列表 = [
        crate::语言包工具::全局语言目录()
            .join(语言)
            .join("crates")
            .join(format!("{}.toml", 库名)),
        crate::语言包根目录(Path::new("."))
            .join(语言)
            .join("crates")
            .join(format!("{}.toml", 库名)),
        PathBuf::from(format!("{}.toml", 库名)),
        PathBuf::from(语言)
            .join("crates")
            .join(format!("{}.toml", 库名)),
    ];
    for 候选 in &候选列表 {
        if 候选.is_file() {
            return Ok(候选.clone());
        }
    }
    anyhow::bail!(
        "{}",
        界面.取文带参("crate_publish_not_found", &[库名, 语言])
    );
}

/// 质量门禁：映射文件必须是合法 TOML 表，且仅含允许的节
fn 校验映射文本(内容: &str) -> anyhow::Result<()> {
    let 界面 = crate::本地化::界面::全局();
    let 数值: toml::Value = toml::from_str(内容).map_err(|解析错| {
        anyhow::anyhow!(
            "{}",
            界面.取文带参("crate_publish_invalid_toml", &[&解析错.to_string()])
        )
    })?;
    let 表体 = 数值
        .as_table()
        .ok_or_else(|| anyhow::anyhow!("{}", 界面.取文("crate_publish_not_table")))?;
    let 允许节 = ["模块路径", "标识符", "解释"];
    for 键名 in 表体.keys() {
        if !允许节.contains(&键名.as_str()) {
            anyhow::bail!("{}", 界面.取文带参("crate_publish_bad_section", &[键名]));
        }
    }
    Ok(())
}

/// 取 git user.name（发布署名用）
fn 仓库用户名() -> Option<String> {
    Command::new("git")
        .arg("config")
        .arg("user.name")
        .output()
        .ok()
        .filter(|进程输出| 进程输出.status.success())
        .and_then(|进程输出| String::from_utf8(进程输出.stdout).ok())
        .map(|串| 串.trim().to_string())
        .filter(|串| !串.is_empty())
}

/// 写入本地路径注册中心并提交（best effort）
fn 写入本地注册中心(
    仓库根: &Path,
    库名: &str,
    语言: &str,
    内容: &str,
    作者参: &str,
) -> anyhow::Result<()> {
    let 界面 = crate::本地化::界面::全局();
    let 注册子目录 = 注册中心子目录(仓库根);
    let 目标路径 = 注册子目录
        .join(语言)
        .join("crates")
        .join(format!("{}.toml", 库名));
    if let Some(上层目录) = 目标路径.parent() {
        fs::create_dir_all(上层目录)?;
    }
    fs::write(&目标路径, 内容)?;
    let 索引路径 = 注册子目录.join("index.json");
    let mut 索引数据 = 读索引(&索引路径)?;
    更新条目(&mut 索引数据, 库名, 语言, 作者参);
    写索引(&索引路径, &索引数据)?;
    仓库提交(仓库根, &format!("publish {} ({})", 库名, 语言));
    println!(
        "{}",
        界面.取文带参(
            "crate_publish_wrote_local",
            &[&仓库根.display().to_string()]
        )
    );
    Ok(())
}

/// 写入远程注册中心：克隆 → 写文件+索引 → 提交 → 推送
fn 写入远程注册中心(
    网址: &str,
    库名: &str,
    语言: &str,
    内容: &str,
    作者参: &str,
) -> anyhow::Result<()> {
    let 界面 = crate::本地化::界面::全局();
    let 临时句柄 = 临时目录句柄::创建()?;
    let 克隆目录 = 临时句柄.目录路径().join("repo");
    let 克隆成功 = Command::new("git")
        .arg("clone")
        .arg(网址)
        .arg(&克隆目录)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|状态| 状态.success())
        .unwrap_or(false);
    if !克隆成功 {
        anyhow::bail!("{}", 界面.取文带参("crate_publish_clone_failed", &[网址]));
    }
    let 注册子目录 = 注册中心子目录(&克隆目录);
    let 目标路径 = 注册子目录
        .join(语言)
        .join("crates")
        .join(format!("{}.toml", 库名));
    if let Some(上层目录) = 目标路径.parent() {
        fs::create_dir_all(上层目录)?;
    }
    fs::write(&目标路径, 内容)?;
    let 索引路径 = 注册子目录.join("index.json");
    let mut 索引数据 = 读索引(&索引路径)?;
    更新条目(&mut 索引数据, 库名, 语言, 作者参);
    写索引(&索引路径, &索引数据)?;
    仓库提交(&克隆目录, &format!("publish {} ({})", 库名, 语言));
    let 推送结果 = Command::new("git")
        .arg("push")
        .current_dir(&克隆目录)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output();
    match 推送结果 {
        Ok(进程输出) if 进程输出.status.success() => {}
        Ok(进程输出) => {
            let 错误串 = String::from_utf8_lossy(&进程输出.stderr);
            anyhow::bail!(
                "{}",
                界面.取文带参("crate_publish_push_failed", &[错误串.trim()])
            );
        }
        Err(推送错) => anyhow::bail!(
            "{}",
            界面.取文带参("crate_publish_push_failed", &[&推送错.to_string()])
        ),
    }
    Ok(())
}

/// 在索引中插入或更新一条映射条目
fn 更新条目(
    索引数据: &mut 注册中心索引, 库名: &str, 语言: &str, 作者参: &str
) {
    let 今日 = 今日日期();
    let 文件路径 = format!("{}/crates/{}.toml", 语言, 库名);
    if let Some(现有条目) = 索引数据
        .映射表
        .iter_mut()
        .find(|项| 项.库名 == 库名 && 项.语言 == 语言)
    {
        现有条目.版本号 = 递增版本号(&现有条目.版本号);
        现有条目.作者 = 作者参.to_string();
        现有条目.更新日期 = 今日.clone();
        现有条目.文件路径 = 文件路径;
    } else {
        索引数据.映射表.push(映射条目 {
            库名: 库名.to_string(),
            语言: 语言.to_string(),
            版本号: "1.0".to_string(),
            作者: 作者参.to_string(),
            许可证: "MIT".to_string(),
            质量分: 1.0,
            下载数: 0,
            文件路径,
            更新日期: 今日.clone(),
        });
    }
    索引数据.更新日期 = 今日.clone();
    索引数据
        .映射表
        .sort_by(|甲, 乙| 甲.库名.cmp(&乙.库名).then_with(|| 甲.语言.cmp(&乙.语言)));
}

/// 版本号末段 +1（1.0 → 1.1；无法解析则回退 1.0）
fn 递增版本号(版本号: &str) -> String {
    if let Some((主版本, 次版本)) = 版本号.split_once('.')
        && let Ok(尾数) = 次版本.parse::<u32>()
    {
        return format!("{}.{}", 主版本, 尾数 + 1);
    }
    "1.0".to_string()
}

/// 提交改动（best effort，失败不影响文件写入结果）
fn 仓库提交(仓库根: &Path, 消息: &str) {
    let _ = Command::new("git")
        .arg("add")
        .arg("-A")
        .current_dir(仓库根)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let _ = Command::new("git")
        .arg("commit")
        .arg("-m")
        .arg(消息)
        .current_dir(仓库根)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// 当前 UTC 日期（YYYY-MM-DD），无外部时间依赖
fn 今日日期() -> String {
    let 历经秒数 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|时长| 时长.as_secs())
        .unwrap_or(0);
    let mut 剩余天数 = (历经秒数 / 86_400) as i64;
    let mut 年 = 1970i64;
    loop {
        let 闰年天数 = if (年 % 4 == 0 && 年 % 100 != 0) || 年 % 400 == 0 {
            366
        } else {
            365
        };
        if 剩余天数 < 闰年天数 {
            break;
        }
        剩余天数 -= 闰年天数;
        年 += 1;
    }
    let 每月天数 = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let 是否闰年 = (年 % 4 == 0 && 年 % 100 != 0) || 年 % 400 == 0;
    let mut 月序号 = 0usize;
    let mut 余天 = 剩余天数;
    loop {
        let 闰月额外 = if 月序号 == 1 && 是否闰年 { 1 } else { 0 };
        let 当月天数 = 每月天数[月序号] + 闰月额外;
        if 余天 < 当月天数 {
            break;
        }
        余天 -= 当月天数;
        月序号 += 1;
    }
    format!("{:04}-{:02}-{:02}", 年, 月序号 + 1, 余天 + 1)
}

#[cfg(test)]
mod 单元测试 {
    use super::*;

    #[test]
    fn 测试今日日期格式() {
        let 串 = 今日日期();
        assert_eq!(串.len(), 10);
        assert!(串.starts_with("20")); // 21 世纪
        assert_eq!(串.as_bytes()[4], b'-');
        assert_eq!(串.as_bytes()[7], b'-');
    }

    #[test]
    fn 测试递增版本号() {
        assert_eq!(递增版本号("1.0"), "1.1");
        assert_eq!(递增版本号("2.3"), "2.4");
        assert_eq!(递增版本号("garbage"), "1.0");
    }

    #[test]
    fn 测试校验映射文本() {
        // 合法：仅含允许的节
        assert!(校验映射文本("[\"标识符\"]\n\"服务器\" = \"Server\"\n").is_ok());
        // 非法节名
        assert!(校验映射文本("[\"未知节\"]\n\"a\" = \"b\"\n").is_err());
        // 重复键：TOML 解析失败
        assert!(校验映射文本("[\"标识符\"]\n\"a\" = \"b\"\n\"a\" = \"c\"\n").is_err());
    }

    #[test]
    fn 测试索引往返() {
        let mut 索引数据 = 注册中心索引::default();
        更新条目(&mut 索引数据, "serde", "zh", "tan80");
        更新条目(&mut 索引数据, "tokio", "zh", "alice");
        assert_eq!(索引数据.映射表.len(), 2);
        // 重复 更新条目 应更新版本而非新增
        更新条目(&mut 索引数据, "serde", "zh", "bob");
        assert_eq!(索引数据.映射表.len(), 2);
        let 检出项 = 索引数据
            .映射表
            .iter()
            .find(|项| 项.库名 == "serde" && 项.语言 == "zh")
            .unwrap();
        assert_eq!(检出项.版本号, "1.1");
        assert_eq!(检出项.作者, "bob");
        // 序列化再反序列化
        let 序列化文本 = serde_json::to_string_pretty(&索引数据).unwrap();
        let 回读: 注册中心索引 = serde_json::from_str(&序列化文本).unwrap();
        assert_eq!(回读.映射表.len(), 2);
    }

    /// 注册中心子目录两种布局：合并进主仓库后在 third-party/，独立仓库在根
    #[test]
    fn 测试注册中心子目录两布局() {
        let 临时句柄 = tempfile::tempdir().unwrap();
        // 平铺布局：index.json 在根
        let 扁平路径 = 临时句柄.path().join("flat");
        fs::create_dir_all(&扁平路径).unwrap();
        fs::write(扁平路径.join("index.json"), "{}").unwrap();
        assert_eq!(注册中心子目录(&扁平路径), 扁平路径);

        // 嵌套布局：third-party/index.json
        let 嵌套 = 临时句柄.path().join("nested");
        let 嵌套根 = 嵌套.join("third-party");
        fs::create_dir_all(&嵌套根).unwrap();
        fs::write(嵌套根.join("index.json"), "{}").unwrap();
        assert_eq!(注册中心子目录(&嵌套), 嵌套根);
    }

    /// 读索引：缺失 → 空索引；合法 → 解析；损坏 JSON → 报错
    #[test]
    fn 测试读索引缺失合法损坏() {
        let 临时句柄 = tempfile::tempdir().unwrap();
        let 缺失 = 临时句柄.path().join("absent.json");
        assert!(读索引(&缺失).unwrap().映射表.is_empty());

        let 良好 = 临时句柄.path().join("good.json");
        let mut 索引数据 = 注册中心索引::default();
        更新条目(&mut 索引数据, "serde", "zh", "tan80");
        写索引(&良好, &索引数据).unwrap();
        assert_eq!(读索引(&良好).unwrap().映射表.len(), 1);

        let 损坏 = 临时句柄.path().join("bad.json");
        fs::write(&损坏, "{ 不是合法 JSON").unwrap();
        assert!(读索引(&损坏).is_err());
    }

    /// 写索引：父目录不存在时逐级创建，可回读
    #[test]
    fn 测试写索引建父目录() {
        let 临时句柄 = tempfile::tempdir().unwrap();
        let 深层 = 临时句柄.path().join("a").join("b").join("index.json");
        写索引(&深层, &注册中心索引::default()).unwrap();
        assert!(深层.is_file());
        assert_eq!(读索引(&深层).unwrap().映射表.len(), 0);
    }

    /// 新条目默认字段齐全且按 (crate, lang) 排序；文件路径 路径与注册中心布局一致
    #[test]
    fn 测试更新条目默认与排序() {
        let mut 索引数据 = 注册中心索引::default();
        更新条目(&mut 索引数据, "tokio", "zh", "alice");
        更新条目(&mut 索引数据, "serde", "ja", "bob");
        更新条目(&mut 索引数据, "serde", "zh", "carol");
        let 键列表: Vec<_> = 索引数据
            .映射表
            .iter()
            .map(|项| (项.库名.as_str(), 项.语言.as_str()))
            .collect();
        assert_eq!(
            键列表,
            vec![("serde", "ja"), ("serde", "zh"), ("tokio", "zh")]
        );
        let 首个 = &索引数据.映射表[0];
        assert_eq!(首个.版本号, "1.0");
        assert_eq!(首个.许可证, "MIT");
        assert_eq!(首个.质量分, 1.0);
        assert_eq!(首个.下载数, 0);
        assert_eq!(首个.文件路径, "ja/crates/serde.toml");
        assert_eq!(首个.更新日期.len(), 10);
        assert_eq!(索引数据.更新日期, 首个.更新日期);
    }

    /// --file 显式指定：存在即采用；不存在报错（不进入候选路径扫描）
    #[test]
    fn 测试解析发布文件显式分支() {
        let 临时句柄 = tempfile::tempdir().unwrap();
        let 现有 = 临时句柄.path().join("mine.toml");
        fs::write(&现有, "[\"标识符\"]\n\"a\" = \"b\"\n").unwrap();
        assert_eq!(
            解析发布文件("serde", "zh", Some(现有.clone())).unwrap(),
            现有
        );
        let 缺失 = 临时句柄.path().join("absent.toml");
        assert!(解析发布文件("serde", "zh", Some(缺失)).is_err());
    }

    /// 写入本地（嵌套 third-party 布局，即并入主仓库后的布局）：
    /// 映射文件 + 索引条目一并落盘，git 提交 best effort（非 git 目录静默失败）
    #[test]
    fn 测试写入本地注册中心嵌套布局() {
        let 临时句柄 = tempfile::tempdir().unwrap();
        let 仓库根 = 临时句柄.path().join("repo");
        // 真实主仓库中 third-party/index.json 已存在，写入方据此识别嵌套布局
        let 嵌套根 = 仓库根.join("third-party");
        fs::create_dir_all(&嵌套根).unwrap();
        写索引(&嵌套根.join("index.json"), &注册中心索引::default()).unwrap();

        let 内容 = "[\"标识符\"]\n\"服务器\" = \"Server\"\n";
        写入本地注册中心(&仓库根, "serde", "zh", 内容, "tan80").unwrap();

        let 映射文件 = 嵌套根.join("zh/crates/serde.toml");
        assert_eq!(fs::read_to_string(&映射文件).unwrap(), 内容);
        let 索引数据 = 读索引(&嵌套根.join("index.json")).unwrap();
        assert_eq!(索引数据.映射表.len(), 1);
        let 该条目 = &索引数据.映射表[0];
        assert_eq!(该条目.库名, "serde");
        assert_eq!(该条目.语言, "zh");
        assert_eq!(该条目.版本号, "1.0");
        assert_eq!(该条目.文件路径, "zh/crates/serde.toml");
    }

    /// 写入本地（独立注册中心仓库的平铺布局）：映射文件直接落在 <lang>/crates/
    #[test]
    fn 测试写入本地注册中心平铺布局() {
        let 临时句柄 = tempfile::tempdir().unwrap();
        let 仓库根 = 临时句柄.path().join("repo");
        fs::create_dir_all(&仓库根).unwrap();
        写索引(&仓库根.join("index.json"), &注册中心索引::default()).unwrap();
        写入本地注册中心(
            &仓库根,
            "tokio",
            "ja",
            "[\"标识符\"]\n\"a\"=\"b\"\n",
            "alice",
        )
        .unwrap();
        assert!(仓库根.join("ja/crates/tokio.toml").is_file());
        let 索引数据 = 读索引(&仓库根.join("index.json")).unwrap();
        assert_eq!(索引数据.映射表.len(), 1);
        assert_eq!(索引数据.映射表[0].文件路径, "ja/crates/tokio.toml");
    }

    /// 已安装清单：缺失 → 空表；写入 → 按 (crate, lang) 键回读；损坏 → 回退空表
    #[test]
    fn 测试已安装清单往返与损坏回退() {
        let _守护 = crate::语言包工具::单元测试::环境锁();
        let 临时句柄 = tempfile::tempdir().unwrap();
        let 原值 = std::env::var("RZ_LANG_DIR").ok();
        unsafe {
            std::env::set_var("RZ_LANG_DIR", 临时句柄.path().join("lang-packs"));
        }
        let 复原 = || unsafe {
            match &原值 {
                Some(值) => std::env::set_var("RZ_LANG_DIR", 值),
                None => std::env::remove_var("RZ_LANG_DIR"),
            }
        };

        // 首次读取：清单不存在 → 空表（不报错）
        assert!(读已安装().is_empty());

        // 经 更新条目 构造一条真实条目并写清单
        let mut 索引数据 = 注册中心索引::default();
        更新条目(&mut 索引数据, "serde", "zh", "tan80");
        let 该条目 = 索引数据.映射表[0].clone();
        let mut 清单 = BTreeMap::new();
        清单.insert(("serde".to_string(), "zh".to_string()), 该条目.clone());
        写已安装(&清单).unwrap();
        // 清单位于语言包目录的上一级（全局 .rz 根）
        assert!(临时句柄.path().join("crate-registry.json").is_file());
        let 回读 = 读已安装();
        // 映射条目 未派生 PartialEq（生产无等值比较需求），逐字段核对
        let 取回 = 回读
            .get(&("serde".to_string(), "zh".to_string()))
            .expect("回读清单应含 serde/zh");
        assert_eq!(取回.版本号, 该条目.版本号);
        assert_eq!(取回.作者, 该条目.作者);
        assert_eq!(取回.文件路径, 该条目.文件路径);

        // 清单损坏（手工改坏 JSON）→ 回退空表，不传播错误
        fs::write(临时句柄.path().join("crate-registry.json"), "[坏").unwrap();
        assert!(读已安装().is_empty());

        复原();
    }
}
