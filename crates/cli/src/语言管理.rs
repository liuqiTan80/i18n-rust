//! 语言包管理模块
//!
//! 实现 `rzc lang list / install / remove` 子命令的底层逻辑：
//! 管理用户全局语言包目录（默认 `~/.rz/lang-packs/`，可用语言目录环境变量覆盖）。
//!
//! 每个语言包是一个目录，目录名即语言代码（如 `zh`、`ru`），
//! 内含 `keywords.toml`、`errors.toml`、`module_paths.toml` 及可选的第三方库子目录。
//!
//! 远程安装：默认依次尝试 [`默认仓库源列表`]（主平台首选，失败自动回退备用平台），
//! 每个源优先克隆仓库，克隆工具不存在或克隆失败时回退下载命令取压缩包并解压；
//! 设置仓库地址环境变量后完全使用用户指定地址（不再尝试默认源）。

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 默认远程语言包源列表（按顺序依次尝试）
///
/// 首选主平台（国内用户访问速度快），失败自动回退备用平台（海外用户可用）。
/// 仓库结构：每个语言包一个目录，目录名即语言代码，内含
/// `keywords.toml`、`errors.toml`、`module_paths.toml` 及可选第三方库子目录。
///
/// 注意：占位符使用前请替换为实际的主平台/备用平台用户名。
/// 设置仓库地址环境变量后完全使用用户指定地址，不再尝试默认源。
pub const 默认仓库源列表: [&str; 2] = [
    "https://gitcode.com/tan80/i18n-rust",
    "https://github.com/liuqiTan80/i18n-rust",
];

/// 远程仓库源：克隆地址与下载命令取压缩包的地址
#[derive(Debug, Clone)]
pub(crate) struct 仓库源项 {
    /// 克隆使用的仓库地址
    pub(crate) 仓库地址: String,
    /// 下载命令取压缩包的地址
    pub(crate) 压缩包地址: String,
}

impl 仓库源项 {
    /// 从仓库地址生成克隆与压缩包下载地址
    ///
    /// - 主平台：仓库归档接口地址（默认分支的压缩包）
    /// - 备用平台：仓库源码归档接口地址（默认分支的压缩包）
    /// - 其他平台（含本地回环服务测试）：按备用平台风格生成
    pub(crate) fn 从地址(地址: &str) -> Self {
        let 规范地址 = 地址.trim_end_matches('/');
        let 压缩包地址 = if 规范地址.contains("gitcode.com") {
            format!("{}/repository/archive/master.zip", 规范地址)
        } else {
            format!("{}/archive/refs/heads/main.zip", 规范地址)
        };
        Self {
            仓库地址: 规范地址.to_string(),
            压缩包地址,
        }
    }
}

/// 收集本次安装要尝试的仓库源列表
///
/// 若设置了仓库地址环境变量，完全使用用户指定的地址；
/// 否则依次尝试 [`默认仓库源列表`]（主平台首选，备用平台备用）。
fn 收集仓库源() -> Vec<仓库源项> {
    if let Ok(自定义值) = std::env::var("RZ_LANG_REPO") {
        let 规范值 = 自定义值.trim().trim_end_matches('/').to_string();
        if !规范值.is_empty() {
            return vec![仓库源项::从地址(&规范值)];
        }
    }
    默认仓库源列表
        .iter()
        .map(|地址值| 仓库源项::从地址(地址值))
        .collect()
}

/// 语言包来源
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 语言来源 {
    /// 内置在 rzc 可执行文件中（不可删除）
    内置,
    /// 用户通过 `rzc lang install` 安装到全局目录
    用户安装,
}

/// 单个已安装语言包的信息
pub struct 语言信息 {
    /// 语言代码（目录名）
    pub 语言代码: String,
    /// 来源
    pub 语言来源: 语言来源,
    /// 扩展名（不含点）；旧语言包无 lang_info.toml 时从静态映射推断
    pub 扩展名: Option<String>,
    /// 版本号；旧语言包无 lang_info.toml 时为空值
    pub 版本: Option<String>,
    /// 语言显示名称（来自 lang_info.toml；旧语言包为空值）
    pub 显示名称: Option<String>,
}

/// 语言包元数据（来自 lang_info.toml）
pub struct 语言元数据 {
    /// 语言名称
    pub 名称: String,
    /// 源码文件扩展名（不含点）
    pub 扩展名: String,
    /// 版本号
    pub 版本: String,
}

/// 远程地址是否允许用于克隆/下载：https、ssh、git@，或本机回环 http
///
/// 明文 http 只放行 localhost / 127.x / ::1（测试与本机仓库服务）；
/// 远程明文地址会暴露仓库内容并允许中间人篡改语言包映射。
pub(crate) fn 地址安全(地址: &str) -> bool {
    let 小写地址 = 地址.to_lowercase();
    if 小写地址.starts_with("https://")
        || 小写地址.starts_with("ssh://")
        || 小写地址.starts_with("git@")
        || 小写地址.starts_with("file://")
        || !小写地址.contains("://")
    {
        return true;
    }
    if !小写地址.starts_with("http://") {
        return false;
    }
    let 主机部分 = 小写地址.strip_prefix("http://").unwrap_or("");
    let 主机 = 主机部分.split('/').next().unwrap_or("");
    主机 == "localhost"
        || 主机.starts_with("localhost:")
        || 主机.starts_with("127.")
        || 主机 == "[::1]"
        || 主机.starts_with("[::1]:")
}

/// 下载远程文件：协议/重定向限制 + 连接与总时长上限 + 大小上限
///
/// curl 的 `--max-filesize` 在服务端未声明长度时不能完全兜底，但能挡住
/// 常见的大文件/压缩炸弹；协议限制与 [`地址安全`] 共同阻断降级到远程 http。
fn 下载到(地址: &str, 目标: &Path) -> anyhow::Result<()> {
    if !地址安全(地址) {
        anyhow::bail!(
            "下载地址必须使用 https 或 ssh（本机回环可用 http）：{}",
            地址
        );
    }
    let 协议参数 = if 地址.to_lowercase().starts_with("https://") {
        "=https"
    } else {
        "=http"
    };
    let 输出值 = Command::new("curl")
        .arg("-L")
        .arg("--fail")
        .arg("--connect-timeout")
        .arg("15")
        .arg("--max-time")
        .arg("300")
        .arg("--max-filesize")
        .arg("67108864")
        .arg("--proto")
        .arg(协议参数)
        .arg("--proto-redir")
        .arg(协议参数)
        .arg("-o")
        .arg(目标)
        .arg(地址)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|错误值| {
            anyhow::anyhow!(
                "{}",
                crate::界面::界面消息表::全局表()
                    .取带参消息("lc_err_curl_run", &[&错误值.to_string()])
            )
        })?;
    if !crate::界面::命令成功(&输出值) {
        let 明细 = String::from_utf8_lossy(&输出值.stderr);
        anyhow::bail!(
            "{}",
            crate::界面::界面消息表::全局表()
                .取带参消息("lc_err_curl_download", &[地址, 明细.trim()])
        );
    }
    Ok(())
}

/// 解析 lang_info.toml 内容
///
/// 格式（引号包裹键，符合 TOML 规范——裸键仅允许 ASCII）：
/// ```toml
/// ["语言包"]
/// "名称" = "俄语"
/// "扩展名" = "ru"
/// "版本" = "1.0"
/// ```
/// 版本缺省时为 "1.0"；名称或扩展名缺失/为空时返回空值。
/// 兼容用户手写的裸键写法：标准解析失败时回退手工解析。
fn 解析语言信息(内容串: &str) -> Option<语言元数据> {
    // 1. 标准 TOML 解析：兼容引号包裹的键
    if let Ok(值项) = toml::from_str::<toml::Value>(内容串)
        && let Some(节表) = 值项.get("语言包")
        && let Some(元数据) = 从节表提取(节表)
    {
        return Some(元数据);
    }
    // 2. 回退手工解析：兼容中文裸键的用户示例写法
    手工解析(内容串)
}

/// 从解析出的 [语言包] 节表提取元数据
fn 从节表提取(节表: &toml::Value) -> Option<语言元数据> {
    let 名称值 = 节表.get("名称")?.as_str()?.trim();
    let 扩展名值 = 节表.get("扩展名")?.as_str()?.trim();
    if 名称值.is_empty() || 扩展名值.is_empty() {
        return None;
    }
    let 版本值 = 节表
        .get("版本")
        .and_then(|值内容| 值内容.as_str())
        .unwrap_or("1.0")
        .trim()
        .to_string();
    Some(语言元数据 {
        名称: 名称值.to_string(),
        扩展名: 扩展名值.to_string(),
        版本: 版本值,
    })
}

/// 手工逐行解析 `键 = "值"`，兼容标准 TOML 不允许的非 ASCII 裸键
///
/// 只识别 名称 / 扩展名 / 版本 三个字段，忽略注释与节头。
fn 手工解析(内容串: &str) -> Option<语言元数据> {
    let mut 名称值: Option<String> = None;
    let mut 扩展名值: Option<String> = None;
    let mut 版本值: Option<String> = None;
    let mut 有语言节 = false;
    for 行文本 in 内容串.lines() {
        let 行文本 = 行文本.trim();
        if 行文本.is_empty() || 行文本.starts_with('#') {
            continue;
        }
        if 行文本.starts_with('[') {
            let 节名 = 行文本
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim_matches('"')
                .trim();
            if 节名 == "语言包" {
                有语言节 = true;
            }
            continue;
        }
        let (键名, 原始值) = 行文本.split_once('=')?;
        let 键名 = 键名.trim().trim_matches('"').trim();
        let 原始值 = 原始值.trim().trim_matches('"').trim();
        if 原始值.is_empty() {
            return None;
        }
        match 键名 {
            "名称" => 名称值 = Some(原始值.to_string()),
            "扩展名" => 扩展名值 = Some(原始值.to_string()),
            "版本" => 版本值 = Some(原始值.to_string()),
            _ => {}
        }
    }
    if !有语言节 {
        return None;
    }
    let 扩展名值 = 扩展名值?;
    if !扩展名安全(&扩展名值) {
        return None;
    }
    Some(语言元数据 {
        名称: 名称值?,
        扩展名: 扩展名值,
        版本: 版本值.unwrap_or_else(|| "1.0".to_string()),
    })
}

/// 语言包声明的扩展名是否可安全用于用户源码
///
/// 第三方语言包可自定义扩展名：若声明成 `rs`，`rzc` 会把用户的 `.rs` 当方言源
/// 转译并覆盖；`toml/json/lock` 等配置后缀同理。仅允许 ASCII 字母数字短后缀。
fn 扩展名安全(扩展名值: &str) -> bool {
    if 扩展名值.is_empty() || 扩展名值.len() > 16 {
        return false;
    }
    if !扩展名值.chars().all(|本字| 本字.is_ascii_alphanumeric()) {
        return false;
    }
    let 小写值 = 扩展名值.to_lowercase();
    !["rs", "toml", "json", "lock", "txt"].contains(&小写值.as_str())
}

/// 读取目录中语言包的元数据（读取并解析 lang_info.toml）
pub fn 读取语言信息(目录值: &Path) -> Option<语言元数据> {
    let 内容串 = fs::read_to_string(目录值.join("lang_info.toml")).ok()?;
    解析语言信息(&内容串)
}

/// 获取内置语言包的元数据（来自嵌入可执行文件的 lang_info.toml）
pub fn 获取内置元数据(语言码: &str) -> Option<语言元数据> {
    // 内置代码列表内的语言必然存在；未知代码回退到中文不影响元数据查询
    let 数据项 = crate::内置语言::获取内置数据(语言码);
    解析语言信息(数据项.语言信息内容)
}

/// 获取全局语言包根目录
///
/// 优先使用语言目录环境变量，否则默认 `~/.rz/lang-packs/`。
pub fn 全局语言目录() -> PathBuf {
    if let Ok(目录值) = std::env::var("RZ_LANG_DIR")
        && !目录值.is_empty()
    {
        return PathBuf::from(目录值);
    }
    // HOME（Unix）优先，USERPROFILE（Windows）回退，保证跨平台安装目录正确
    let 主目录 = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(主目录).join(".rz").join("lang-packs")
}

/// 列出所有已安装的语言包（内置 + 用户安装）
///
/// 内置语言包固定在前，用户安装的按名称排序在后。
/// 扩展名/版本来自 lang_info.toml；旧语言包无该文件时扩展名回退静态映射推断。
pub fn 列出语言() -> Vec<语言信息> {
    let mut 清单: Vec<语言信息> = crate::内置语言::内置语言清单()
        .into_iter()
        .map(|代码值| {
            let 元数据 = 获取内置元数据(代码值);
            语言信息 {
                语言代码: 代码值.to_string(),
                语言来源: 语言来源::内置,
                扩展名: 元数据
                    .as_ref()
                    .map(|元项| 元项.扩展名.clone())
                    .or_else(|| 静态代码转扩展名(代码值)),
                版本: 元数据.as_ref().map(|元项| 元项.版本.clone()),
                显示名称: 元数据.map(|元项| 元项.名称.clone()),
            }
        })
        .collect();

    // 扫描全局目录：含 keywords.toml 的子目录视为语言包
    let 根目录 = 全局语言目录();
    let mut 用户安装: Vec<(String, Option<语言元数据>)> = Vec::new();
    if let Ok(目录读) = fs::read_dir(&根目录) {
        for 条目值 in 目录读.flatten() {
            let 路径值 = 条目值.path();
            if 路径值.is_dir()
                && 路径值.join("keywords.toml").is_file()
                && let Some(名称值) = 路径值.file_name().and_then(|本名| 本名.to_str())
            {
                用户安装.push((名称值.to_string(), 读取语言信息(&路径值)));
            }
        }
    }
    用户安装.sort_by(|甲, 乙| 甲.0.cmp(&乙.0));
    for (代码值, 元数据) in 用户安装 {
        清单.push(语言信息 {
            扩展名: 元数据
                .as_ref()
                .map(|元项| 元项.扩展名.clone())
                .or_else(|| 静态代码转扩展名(&代码值)),
            版本: 元数据.as_ref().map(|元项| 元项.版本.clone()),
            显示名称: 元数据.map(|元项| 元项.名称.clone()),
            语言代码: 代码值,
            语言来源: 语言来源::用户安装,
        });
    }
    清单
}

/// 静态扩展名映射（扩展名 → 语言代码）
///
/// 作为动态映射的回退，保证无 lang_info.toml 的旧语言包向后兼容。
pub fn 静态扩展名映射() -> HashMap<String, String> {
    [
        ("zh", "中文"),
        ("ru", "俄语"),
        ("ja", "日语"),
        ("ko", "韩语"),
    ]
    .into_iter()
    .map(|(后缀, 代码值)| (后缀.to_string(), 代码值.to_string()))
    .collect()
}

/// 静态映射反查：语言代码 → 扩展名（旧语言包推断用）
fn 静态代码转扩展名(语言码: &str) -> Option<String> {
    静态扩展名映射()
        .into_iter()
        .find(|(_, 代码值)| 代码值 == 语言码)
        .map(|(后缀, _)| 后缀)
}

/// 构建 扩展名 → 语言代码 的动态映射表
///
/// 来源：内置语言包元数据 + 全局用户语言包目录。
/// 用户安装的语言包优先于内置（同名扩展名时覆盖）；
/// 无 lang_info.toml 的旧语言包回退静态映射推断。
pub fn 构建扩展名映射() -> HashMap<String, String> {
    let mut 映射表 = HashMap::new();

    // 1. 内置语言包
    for 代码值 in crate::内置语言::内置语言清单() {
        if let Some(元数据) = 获取内置元数据(代码值) {
            映射表.insert(元数据.扩展名, 代码值.to_string());
        } else if let Some(后缀) = 静态代码转扩展名(代码值) {
            映射表.insert(后缀, 代码值.to_string());
        }
    }

    // 2. 全局用户语言包（覆盖内置）
    let 根目录 = 全局语言目录();
    if let Ok(目录读) = fs::read_dir(&根目录) {
        for 条目值 in 目录读.flatten() {
            let 路径值 = 条目值.path();
            if !路径值.is_dir() || !路径值.join("keywords.toml").is_file() {
                continue;
            }
            let Some(代码值) = 路径值
                .file_name()
                .and_then(|本名| 本名.to_str())
                .map(String::from)
            else {
                continue;
            };
            if let Some(元数据) = 读取语言信息(&路径值) {
                映射表.insert(元数据.扩展名, 代码值.clone());
            } else if let Some(后缀) = 静态代码转扩展名(&代码值) {
                映射表.insert(后缀, 代码值);
            }
        }
    }
    映射表
}

/// 查询 扩展名 → 语言代码 的动态映射
pub fn 查询扩展名映射(扩展名值: &str) -> Option<String> {
    构建扩展名映射().remove(扩展名值)
}

/// 全局语言包目录中的一个候选条目
///
/// 只要子目录内含 `keywords.toml` 就视为语言包（与 `构建扩展名映射` 同口径），
/// `目录名` 为目录名（语言代码即取此名）。
pub struct 全局语言包 {
    pub 目录名: String,
    /// lang_info.toml 元数据；缺失或不可解析时为空值（扩展名回退静态映射推断）
    pub 元数据: Option<语言元数据>,
}

/// 扫描全局语言包目录（`~/.rz/lang-packs/`，语言目录环境变量可改）
///
/// 供 `rzc doctor` 做语言包健康检查：定位陈旧副本遮蔽内置表、备份/杂项目录
/// 被当语言包吃掉等问题（这类问题只在用户环境发生，静态门检查不和）。
/// 目录不存在或不可读时返回空列表（不报错）。
pub fn 扫描全局语言包() -> Vec<全局语言包> {
    let mut 包列表 = Vec::new();
    if let Ok(目录读) = fs::read_dir(全局语言目录()) {
        for 条目值 in 目录读.flatten() {
            let 路径值 = 条目值.path();
            if !路径值.is_dir() || !路径值.join("keywords.toml").is_file() {
                continue;
            }
            let Some(名称值) = 路径值.file_name().and_then(|本名| 本名.to_str()) else {
                continue;
            };
            包列表.push(全局语言包 {
                目录名: 名称值.to_string(),
                元数据: 读取语言信息(&路径值),
            });
        }
    }
    包列表.sort_by(|甲, 乙| 甲.目录名.cmp(&乙.目录名));
    包列表
}

/// 全局语言包目录下的子目录名（含不具备 `keywords.toml` 的）
///
/// 用于识别改名后仍留在 `lang-packs/` 内的备份目录（它们仍会被当语言包吃掉）。
pub fn 全局语言包子目录名() -> Vec<String> {
    let mut 名称们 = Vec::new();
    if let Ok(目录读) = fs::read_dir(全局语言目录()) {
        for 条目值 in 目录读.flatten() {
            let 路径值 = 条目值.path();
            if !路径值.is_dir() {
                continue;
            }
            if let Some(名称值) = 路径值.file_name().and_then(|本名| 本名.to_str()) {
                名称们.push(名称值.to_string());
            }
        }
    }
    名称们.sort();
    名称们
}

/// 所有当前可用的扩展名（按名称排序）
pub fn 全部可用扩展名() -> Vec<String> {
    let mut 清单: Vec<String> = 构建扩展名映射().keys().cloned().collect();
    清单.sort();
    清单
}

/// 安装语言包
///
/// - `<source>` 是本地存在的目录路径：校验结构后整体复制到全局语言包目录。
/// - 否则视为语言代码：从远程仓库下载安装。
/// - 目标已存在时默认报错；强制参数为真时覆盖安装。
pub fn 安装语言(来源值: &str, 强制: bool) -> anyhow::Result<()> {
    let 源路径 = PathBuf::from(来源值);
    if 源路径.is_dir() {
        校验语言包目录(&源路径)?;
        let 语言码 = 源路径
            .file_name()
            .and_then(|本名| 本名.to_str())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "{}",
                    crate::界面::界面消息表::全局表().取带参消息(
                        "lc_err_lang_code_from_path",
                        &[&源路径.display().to_string()]
                    )
                )
            })?;
        return 复制到全局目录(&源路径, 语言码, 强制);
    }
    安装远程语言(来源值, 强制)
}

/// 删除用户安装的语言包
///
/// 内置语言包不可删除；同名用户安装包存在时优先删除用户安装的。
pub fn 移除语言(语言码: &str) -> anyhow::Result<()> {
    let 界面表 = crate::界面::界面消息表::全局表();
    // 校验语言代码，防止 ../ 等路径遍历导致删除语言包目录之外的目录
    if !校验语言代码(语言码) {
        anyhow::bail!("{}", 界面表.取带参消息("invalid_lang_code", &[语言码]));
    }
    let 目标目录 = 全局语言目录().join(语言码);
    if 目标目录.is_dir() {
        fs::remove_dir_all(&目标目录)?;
        println!("{}", 界面表.取带参消息("lang_removed", &[语言码]));
        return Ok(());
    }

    if crate::内置语言::内置语言清单().contains(&语言码) {
        anyhow::bail!(
            "{}",
            界面表.取带参消息("lang_builtin_not_removable", &[语言码])
        );
    }

    anyhow::bail!(
        "{}",
        界面表.取带参消息("lang_not_found", &[语言码, &目标目录.display().to_string()])
    );
}

/// 从远程仓库安装语言包
fn 安装远程语言(语言码: &str, 强制: bool) -> anyhow::Result<()> {
    let 界面表 = crate::界面::界面消息表::全局表();
    if !校验语言代码(语言码) {
        anyhow::bail!("{}", 界面表.取带参消息("invalid_lang_code", &[语言码]));
    }
    let 源列表 = 收集仓库源();
    let 临时 = 临时目录句柄::新建()?;
    尝试全部源(语言码, &源列表, &临时, 强制)
}

/// 远程语言包市场条目（`rzc lang search` 的浏览结果）
#[derive(Debug, Clone)]
pub struct 远程语言信息 {
    /// 语言代码（目录名）
    pub 语言代码: String,
    /// 显示名称（来自 lang_info.toml；旧语言包为空值）
    pub 显示名称: Option<String>,
    /// 版本号（来自 lang_info.toml；旧语言包为空值）
    pub 版本: Option<String>,
}

/// 浏览远程语言包市场：下载仓库压缩包后扫描可用语言包
///
/// 依次尝试 [`收集仓库源`] 中的源，首个成功下载并解析的源即返回其
/// 语言包列表（与安装的源回退策略一致）；关键词参数为有值时按
/// 语言代码或显示名称模糊过滤（不区分大小写）。全部失败时聚合错误。
pub fn 搜索远程语言(关键词: Option<&str>) -> anyhow::Result<Vec<远程语言信息>> {
    let 源列表 = 收集仓库源();
    let 临时 = 临时目录句柄::新建()?;
    let mut 错误明细 = Vec::new();
    for (序号, 源项) in 源列表.iter().enumerate() {
        match 拉取仓库压缩包(源项, &临时) {
            Ok(仓库目录) => {
                let mut 已找到 = 扫描仓库语言(&仓库目录);
                已找到.sort_by(|甲, 乙| 甲.语言代码.cmp(&乙.语言代码));
                return Ok(过滤远程语言(已找到, 关键词.unwrap_or("")));
            }
            Err(错误值) => 错误明细.push(crate::界面::界面消息表::全局表().取带参消息(
                "lc_err_source_detail",
                &[&(序号 + 1).to_string(), &源项.仓库地址, &错误值.to_string()],
            )),
        }
    }
    let 界面表 = crate::界面::界面消息表::全局表();
    anyhow::bail!(
        "{}",
        界面表.取带参消息(
            "lang_search_failed",
            &[
                关键词.unwrap_or(""),
                &源列表.len().to_string(),
                &错误明细.join("\n"),
            ]
        )
    )
}

/// 按关键词过滤远程语言包（匹配语言代码或显示名称，不区分大小写）
fn 过滤远程语言(
    已找到: Vec<远程语言信息>, 关键词: &str
) -> Vec<远程语言信息> {
    let 小写关键词 = 关键词.trim().to_lowercase();
    if 小写关键词.is_empty() {
        return 已找到;
    }
    已找到
        .into_iter()
        .filter(|信息值| {
            信息值.语言代码.to_lowercase().contains(&小写关键词)
                || 信息值
                    .显示名称
                    .as_deref()
                    .is_some_and(|名称值| 名称值.to_lowercase().contains(&小写关键词))
        })
        .collect()
}

/// 下载仓库压缩包并解压，返回仓库根目录（自动下沉单层根目录）
///
/// 设为公开(包)以便第三方包仓库模块复用同一套下载/解压逻辑。
pub(crate) fn 拉取仓库压缩包(
    源项: &仓库源项,
    临时: &临时目录句柄,
) -> anyhow::Result<PathBuf> {
    // 复用下载命令（与安装回退路径一致）；主平台/备用平台的压缩包顶层均为
    // `<仓库>-<分支>/` 目录，解压后若仅含一个目录则以其为仓库根。
    let 下载路径 = 临时.存放路径().join("lang_market.zip");
    下载到(&源项.压缩包地址, &下载路径)?;
    let 解压目录 = 临时.存放路径().join("market_extracted");
    解压压缩包(&下载路径, &解压目录)?;
    let mut 条目们: Vec<PathBuf> = fs::read_dir(&解压目录)
        .ok()
        .map(|目录读| 目录读.flatten().map(|条目值| 条目值.path()).collect())
        .unwrap_or_default();
    if 条目们.len() == 1 && 条目们[0].is_dir() {
        Ok(条目们.remove(0))
    } else {
        Ok(解压目录)
    }
}

/// 扫描仓库目录中的语言包（三种布局全部覆盖）
///
/// 1. 仓库根 `<语言码>/`（自建语言包仓库的约定结构）；
/// 2. `lang-packs/<语言码>/`（兼容旧版双副本结构与第三方仓库）；
/// 3. `crates/engine/lang-packs/<语言码>/`（主仓库单副本结构）。
fn 扫描仓库语言(仓库目录: &Path) -> Vec<远程语言信息> {
    let mut 已找到: Vec<远程语言信息> = Vec::new();
    扫描目录语言(仓库目录, &mut 已找到);
    扫描目录语言(&仓库目录.join("lang-packs"), &mut 已找到);
    扫描目录语言(&仓库目录.join("crates/engine/lang-packs"), &mut 已找到);
    已找到
}

/// 扫描单个目录下的语言包子目录（含 keywords.toml 即视为语言包）
fn 扫描目录语言(目录值: &Path, 已找到: &mut Vec<远程语言信息>) {
    let Ok(目录读) = fs::read_dir(目录值) else {
        return;
    };
    for 条目值 in 目录读.flatten() {
        let 路径值 = 条目值.path();
        if !路径值.is_dir() || !路径值.join("keywords.toml").is_file() {
            continue;
        }
        let Some(代码值) = 路径值.file_name().and_then(|本名| 本名.to_str()) else {
            continue;
        };
        // 布局间同名去重（保留先发现的；自建仓库根与 lang-packs 并存时）
        if 已找到.iter().any(|信息值| 信息值.语言代码 == 代码值) {
            continue;
        }
        let 元数据 = 读取语言信息(&路径值);
        已找到.push(远程语言信息 {
            语言代码: 代码值.to_string(),
            显示名称: 元数据.as_ref().map(|元项| 元项.名称.clone()),
            版本: 元数据.as_ref().map(|元项| 元项.版本.clone()),
        });
    }
}

/// 依次尝试多个源，全部失败时聚合错误并建议仓库地址环境变量
fn 尝试全部源(
    语言码: &str,
    源列表: &[仓库源项],
    临时: &临时目录句柄,
    强制: bool,
) -> anyhow::Result<()> {
    let mut 错误明细 = Vec::new();
    for (序号, 源项) in 源列表.iter().enumerate() {
        match 尝试单个源(语言码, 源项, 临时, 强制) {
            Ok(()) => return Ok(()),
            Err(错误值) => 错误明细.push(crate::界面::界面消息表::全局表().取带参消息(
                "lc_err_source_detail",
                &[&(序号 + 1).to_string(), &源项.仓库地址, &错误值.to_string()],
            )),
        }
    }
    let 界面表 = crate::界面::界面消息表::全局表();
    anyhow::bail!(
        "{}",
        界面表.取带参消息(
            "remote_install_failed",
            &[语言码, &源列表.len().to_string(), &错误明细.join("\n"),]
        )
    )
}

/// 尝试从单个源安装：优先克隆仓库，失败回退下载命令取压缩包
fn 尝试单个源(
    语言码: &str,
    源项: &仓库源项,
    临时: &临时目录句柄,
    强制: bool,
) -> anyhow::Result<()> {
    let 克隆目录 = 临时.存放路径().join("repo");
    if !地址安全(&源项.仓库地址) || !地址安全(&源项.压缩包地址) {
        anyhow::bail!(
            "语言包仓库地址必须使用 https 或 ssh（本机回环可用 http）：{}",
            源项.仓库地址
        );
    }
    let 克隆结果 = Command::new("git")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(&源项.仓库地址)
        .arg(&克隆目录)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output();
    match 克隆结果 {
        Ok(输出值) if crate::界面::命令成功(&输出值) => {
            从仓库目录安装(语言码, &克隆目录, 强制)
        }
        Ok(输出值) => {
            let 克隆错误 = String::from_utf8_lossy(&输出值.stderr).trim().to_string();
            match 尝试下载安装(语言码, &源项.压缩包地址, 临时, 强制) {
                Ok(()) => Ok(()),
                Err(下载错误) => {
                    if 克隆错误.is_empty() {
                        Err(下载错误)
                    } else {
                        anyhow::bail!(
                            "{}",
                            crate::界面::界面消息表::全局表().取带参消息(
                                "lc_err_git_curl_both",
                                &[&克隆错误, &下载错误.to_string()]
                            )
                        )
                    }
                }
            }
        }
        Err(错误值) => {
            if 错误值.kind() == std::io::ErrorKind::NotFound {
                return 尝试下载安装(语言码, &源项.压缩包地址, 临时, 强制);
            }
            Err(anyhow::anyhow!(
                "{}",
                crate::界面::界面消息表::全局表()
                    .取带参消息("lc_err_git_run", &[&错误值.to_string()])
            ))
        }
    }
}

/// 从仓库目录安装指定语言包
///
/// 依次探测三个位置：
/// 1. 仓库根 `<语言码>/`（自建语言包仓库的约定结构）；
/// 2. 仓库根 `lang-packs/<语言码>/`（兼容旧版双副本结构与第三方仓库）；
/// 3. `crates/engine/lang-packs/<语言码>/`（主仓库单副本结构，
///    使 `rzc lang install <码>` 可直接从主仓库安装）。
fn 从仓库目录安装(语言码: &str, 仓库目录: &Path, 强制: bool) -> anyhow::Result<()> {
    let 语言源 = 仓库目录.join(语言码);
    let 语言源 = if 语言源.is_dir() {
        语言源
    } else if 仓库目录.join("lang-packs").join(语言码).is_dir() {
        仓库目录.join("lang-packs").join(语言码)
    } else if 仓库目录
        .join("crates/engine/lang-packs")
        .join(语言码)
        .is_dir()
    {
        仓库目录.join("crates/engine/lang-packs").join(语言码)
    } else {
        let 界面表 = crate::界面::界面消息表::全局表();
        anyhow::bail!("{}", 界面表.取带参消息("remote_lang_not_found", &[语言码]));
    };
    校验语言包目录(&语言源)?;
    复制到全局目录(&语言源, 语言码, 强制)
}

/// 回退方案：下载命令取压缩包并解压安装
fn 尝试下载安装(
    语言码: &str,
    压缩包地址: &str,
    临时: &临时目录句柄,
    强制: bool,
) -> anyhow::Result<()> {
    let 下载路径 = 临时.存放路径().join("lang_pack.zip");
    下载到(压缩包地址, &下载路径)?;
    let 解压目录 = 临时.存放路径().join("extracted");
    解压压缩包(&下载路径, &解压目录)?;
    let 语言源 = 在解压目录查找语言(&解压目录, 语言码).ok_or_else(|| {
        let 界面表 = crate::界面::界面消息表::全局表();
        anyhow::anyhow!("{}", 界面表.取带参消息("remote_lang_not_found", &[语言码]))
    })?;
    校验语言包目录(&语言源)?;
    复制到全局目录(&语言源, 语言码, 强制)
}

/// 解压压缩包到目标目录（保留完整条目路径）
///
/// 每个条目名先经 [`安全条目路径`] 校验，命中穿越向量的条目直接跳过。
fn 解压压缩包(压缩包路径: &Path, 解压目录: &Path) -> anyhow::Result<()> {
    let 压缩数据 = fs::read(压缩包路径).map_err(|错误值| {
        anyhow::anyhow!(
            "{}",
            crate::界面::界面消息表::全局表().取带参消息(
                "lc_err_open_zip",
                &[&压缩包路径.display().to_string(), &错误值.to_string()]
            )
        )
    })?;
    let 文件游标 = std::io::Cursor::new(压缩数据);
    let mut 归档 = zip::ZipArchive::new(文件游标).map_err(|错误值| {
        anyhow::anyhow!(
            "{}",
            crate::界面::界面消息表::全局表()
                .取带参消息("lc_err_read_zip", &[&错误值.to_string()])
        )
    })?;
    for 序号 in 0..归档.len() {
        let mut 条目值 = 归档.by_index(序号).map_err(|错误值| {
            anyhow::anyhow!(
                "{}",
                crate::界面::界面消息表::全局表()
                    .取带参消息("lc_err_read_zip_entry", &[&错误值.to_string()])
            )
        })?;
        // 条目名解码可能失败（非 UTF-8/无效编码），失败条目跳过；
        // 取拥有形态以免与下方可变借用的拷贝借用冲突
        let Ok(条目名) = 条目值.name().map(|名称值| 名称值.into_owned()) else {
            continue;
        };
        // 条目名安全校验：拒绝绝对路径、盘符前缀与 `..` 上跳（详见函数文档）
        let Some(目标路径) = 安全条目路径(&条目名).map(|相对值| 解压目录.join(相对值))
        else {
            continue;
        };
        if 条目值.is_dir() {
            fs::create_dir_all(&目标路径)?;
        } else {
            if let Some(父级) = 目标路径.parent() {
                fs::create_dir_all(父级)?;
            }
            let mut 条目内容 = Vec::new();
            条目值.read_to_end(&mut 条目内容).map_err(|错误值| {
                anyhow::anyhow!(
                    "{}",
                    crate::界面::界面消息表::全局表().取带参消息(
                        "lc_err_write_file",
                        &[&目标路径.display().to_string(), &错误值.to_string()]
                    )
                )
            })?;
            fs::write(&目标路径, &条目内容)?;
        }
    }
    Ok(())
}

/// 校验压缩包条目名，返回可安全拼接到解压根目录的相对路径
///
/// 拒绝三类条目（均为路径穿越向量，命中返回空值）：
/// - **空名**：无有效路径段；
/// - **绝对路径**：以斜杠或反斜杠开头的路径。反斜杠须先归一化为斜杠——压缩包规范
///   要求 `/` 分隔，但 Windows 打包工具会写入 `\`，而 Unix 的
///   `路径类型::组件` 不把 `\` 当分隔符，只按平台原生规则判断会漏拦；
/// - **`..` 上跳段**：`a/../../x`；
/// - **Windows 盘符前缀**：`C:/Users/x/...`。`路径类型::连接` 遇带前缀的路径会
///   **整体替换**基准目录（而非拼接），解压会写到 `解压目录` 之外；该前缀
///   在 Unix 上不被 `路径类型::组件` 识别为前缀，故必须显式判断，
///   仅检查 `..` 会完全漏掉这一向量。
pub(crate) fn 安全条目路径(条目名: &str) -> Option<PathBuf> {
    let 归一化 = 条目名.replace('\\', "/");
    if 归一化.is_empty() || 归一化.starts_with('/') {
        return None;
    }
    if 归一化.split('/').any(|段值| 段值 == "..") {
        return None;
    }
    // 盘符前缀：首两字节为「ASCII 字母 + 冒号」即判定（`C:/x` 与盘符相对路径
    // `c:foo` 都属带前缀路径，`路径类型::连接` 都会整体替换基准目录）
    let 字节项 = 归一化.as_bytes();
    if 字节项.len() >= 2 && 字节项[0].is_ascii_alphabetic() && 字节项[1] == b':' {
        return None;
    }
    Some(PathBuf::from(归一化))
}

/// 在解压目录中查找语言包目录
fn 在解压目录查找语言(解压目录: &Path, 语言码: &str) -> Option<PathBuf> {
    let 直达路径 = 解压目录.join(语言码);
    if 直达路径.is_dir() {
        return Some(直达路径);
    }
    for 条目值 in fs::read_dir(解压目录).ok()?.flatten() {
        let 路径值 = 条目值.path();
        if 路径值.is_dir() {
            let 嵌套路径 = 路径值.join(语言码);
            if 嵌套路径.is_dir() {
                return Some(嵌套路径);
            }
        }
    }
    None
}

/// 将源语言包目录复制到全局目录
///
/// 复制前后维护一份首次信任内容指纹：对本次取回的源码
/// 目录计算确定性摘要，与该语言包上次安装时记录的指纹比对——
/// - 首装（无记录）：直接信任并写入指纹；
/// - 重装（强制参数为真）且内容变化：向标准错误打出显著告警（附新旧指纹）但**不
///   阻断**——合法的内容更新是常态，硬失败不可用；此告警的价值在于把「同一
///   源静默返回不同字节」（无论是上游迭代还是分发被篡改）从不可见变为可见。
///
/// 完整性/篡改类告警沿用安装模块惯例（硬编码中文、不走界面消息表）。
/// 指纹以旁路文件存于语言包目录内，卸载时随整目录一并清除，不留孤儿状态。
fn 复制到全局目录(源目录: &Path, 语言码: &str, 强制: bool) -> anyhow::Result<()> {
    let 界面表 = crate::界面::界面消息表::全局表();
    let 目标目录 = 全局语言目录().join(语言码);
    let 新指纹 = 内容指纹(源目录)?;
    if 目标目录.exists() {
        if !强制 {
            anyhow::bail!(
                "{}",
                界面表.取带参消息(
                    "lang_already_installed",
                    &[语言码, &目标目录.display().to_string(), 语言码]
                )
            );
        }
        if let Some(旧指纹) = 读取记录指纹(&目标目录)
            && 旧指纹 != 新指纹
        {
            eprintln!(
                "⚠️ 语言包「{语言码}」内容自上次安装后发生变化（可能源已更新或分发遭篡改）：上次指纹 {旧指纹}，本次指纹 {新指纹}。如非有意更新，请核实源仓库后再使用。"
            );
        }
        fs::remove_dir_all(&目标目录)?;
    }
    复制目录递归(源目录, &目标目录)?;
    写入记录指纹(&目标目录, &新指纹)?;
    println!(
        "{}",
        界面表.取带参消息("lang_installed", &[语言码, &目标目录.display().to_string()])
    );
    println!("{}", 界面表.取消息("lang_install_hint"));
    Ok(())
}

/// 语言包内容指纹旁路文件名（存于语言包目录内，卸载随目录一并清除）
const 完整性文件: &str = ".rzc-integrity";

/// 计算语言包目录的内容指纹。
///
/// 对目录内全部文件递归求摘要，按相对路径排序后逐项拼合
/// 「相对路径\0文件摘要\n」再整体取摘要（十六进制小写）。与文件
/// 时间戳、磁盘遍历顺序无关，保证「内容相同→指纹相同、内容变动→指纹
/// 变动」，是首次信任变更检测的基础。排除旁路文件自身，避免记录动作污染后续计算。
fn 内容指纹(目录值: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let mut 文件对们: Vec<(String, String)> = Vec::new();
    收集指纹文件(目录值, 目录值, &mut 文件对们)?;
    文件对们.sort_by(|甲, 乙| 甲.0.cmp(&乙.0));
    let mut 摘要器 = Sha256::new();
    for (相对值, 摘要值) in 文件对们 {
        摘要器.update(相对值.as_bytes());
        摘要器.update(b"\0");
        摘要器.update(摘要值.as_bytes());
        摘要器.update(b"\n");
    }
    Ok(format!("{:x}", 摘要器.finalize()))
}

/// 递归收集（相对路径，单文件摘要），跳过旁路文件。
fn 收集指纹文件(
    根路径: &Path,
    当前路径: &Path,
    输出项: &mut Vec<(String, String)>,
) -> anyhow::Result<()> {
    for 条目值 in fs::read_dir(当前路径)? {
        let 条目值 = 条目值?;
        let 路径值 = 条目值.path();
        if 路径值.is_dir() {
            收集指纹文件(根路径, &路径值, 输出项)?;
        } else if 条目值.file_name() != 完整性文件 {
            let 相对值 = 路径值
                .strip_prefix(根路径)
                .unwrap_or(&路径值)
                .to_string_lossy()
                .replace('\\', "/");
            输出项.push((相对值, 文件摘要(&路径值)?));
        }
    }
    Ok(())
}

/// 单文件摘要（十六进制小写）。
fn 文件摘要(路径值: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let 数据项 = fs::read(路径值)?;
    let mut 摘要器 = Sha256::new();
    摘要器.update(&数据项);
    Ok(format!("{:x}", 摘要器.finalize()))
}

/// 判断文本是否为 64 位十六进制串（小写或大写字母均认）。
fn 是十六进制串(文本串: &str) -> bool {
    文本串.len() == 64 && 文本串.chars().all(是十六进制位)
}

/// 单个字符是否为十六进制位（数字或大小写字母）。
fn 是十六进制位(字符值: char) -> bool {
    字符值.is_ascii_digit() || ('a'..='f').contains(&字符值.to_ascii_lowercase())
}

/// 读取语言包目录中已记录的内容指纹；缺失或格式非法时返回空值
///（空值视为「无历史可比较」——旧版本安装或首装，降级为静默记录，不误报）。
fn 读取记录指纹(目标目录: &Path) -> Option<String> {
    let 文本串 = fs::read_to_string(目标目录.join(完整性文件)).ok()?;
    let 十六进制值 = 文本串.trim().to_string();
    if 是十六进制串(&十六进制值) {
        Some(十六进制值)
    } else {
        None
    }
}

/// 将本次内容指纹写入语言包目录的旁路文件。
fn 写入记录指纹(目标目录: &Path, 指纹: &str) -> anyhow::Result<()> {
    fs::write(目标目录.join(完整性文件), format!("{指纹}\n"))?;
    Ok(())
}

/// 校验语言包目录结构：必须包含 keywords.toml
fn 校验语言包目录(路径值: &Path) -> anyhow::Result<()> {
    if !路径值.join("keywords.toml").is_file() {
        let 界面表 = crate::界面::界面消息表::全局表();
        anyhow::bail!(
            "{}",
            界面表.取带参消息("invalid_lang_dir", &[&路径值.display().to_string()])
        );
    }
    Ok(())
}

/// 校验语言代码可作为安全目录名（防止路径穿越）
pub(crate) fn 校验语言代码(代码值: &str) -> bool {
    !代码值.is_empty() && !代码值.contains(['/', '\\']) && 代码值 != "." && 代码值 != ".."
}

/// 递归复制目录
fn 复制目录递归(源目录: &Path, 目标目录: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(目标目录)?;
    for 条目值 in fs::read_dir(源目录)? {
        let 条目值 = 条目值?;
        let 源路径 = 条目值.path();
        let 目标路径 = 目标目录.join(条目值.file_name());
        if 源路径.is_dir() {
            复制目录递归(&源路径, &目标路径)?;
        } else {
            fs::copy(&源路径, &目标路径)?;
        }
    }
    Ok(())
}

/// 临时目录句柄：释放时递归清理
///
/// 设为公开(包)以便第三方包仓库模块持有临时仓库目录的生命周期。
pub(crate) struct 临时目录句柄(PathBuf);

impl 临时目录句柄 {
    pub(crate) fn 新建() -> anyhow::Result<Self> {
        let 时间戳 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|持续值| 持续值.as_nanos())
            .unwrap_or(0);
        // 用户隔离 + 符号链接校验（与临时守卫/语言服务虚拟目录同一威胁模型）
        let 路径值 = crate::临时守卫::安全临时路径(&format!(
            "rzc-lang-{}-{}-{}",
            crate::临时守卫::安全用户段(),
            std::process::id(),
            时间戳
        ))?;
        fs::create_dir_all(&路径值)?;
        Ok(Self(路径值))
    }

    pub(crate) fn 存放路径(&self) -> &Path {
        &self.0
    }
}

impl Drop for 临时目录句柄 {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
pub(crate) use self::母语测试 as tests;
#[cfg(test)]
pub(crate) mod 母语测试 {
    use super::*;

    /// 环境变量互斥锁：测试并行运行时保护语言目录等全局环境变量。
    /// 包级共享（而非模块私有）：其他模块（main/ui）中依赖环境变量状态、
    /// 或修改环境变量的测试同样必须持有本锁，否则并发执行时互相污染。
    static 环境互斥锁: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 获取环境变量互斥锁（供其他测试模块复用）。
    ///
    /// 锁被毒化时（持锁测试断言失败恐慌）取回内部数据继续运行，
    /// 避免一次失败级联成后续所有持锁测试的中毒假故障。
    pub(crate) fn 环境锁<'甲>() -> std::sync::MutexGuard<'甲, ()> {
        环境互斥锁
            .lock()
            .unwrap_or_else(|错误值| 错误值.into_inner())
    }

    /// 环境变量临时接管守卫（供其他测试模块复用）：保存指定变量原值并从
    /// 环境移除，释放时恢复——断言失败恐慌时也不污染后续测试。
    pub(crate) struct 环境还原(Vec<(&'static str, Option<String>)>);

    impl 环境还原 {
        /// 接管（保存并移除）指定环境变量；调用方须持有环境锁
        pub(crate) fn 接管(键们: &[&'static str]) -> Self {
            let 已保存 = 键们
                .iter()
                .map(|键值| (*键值, std::env::var(键值).ok()))
                .collect();
            for 键值 in 键们 {
                // Rust 2024：读写进程级环境变量为不安全，须由环境锁串行化
                unsafe {
                    std::env::remove_var(键值);
                }
            }
            Self(已保存)
        }
    }

    impl Drop for 环境还原 {
        fn drop(&mut self) {
            for (键值, 值项) in &self.0 {
                match 值项 {
                    Some(值内容) => unsafe {
                        std::env::set_var(键值, 值内容);
                    },
                    None => unsafe {
                        std::env::remove_var(键值);
                    },
                }
            }
        }
    }

    /// 在临时根下制作一个最小语言包目录（含第三方库子目录）
    fn 制作临时语言包(根目录: &Path, 名称值: &str) {
        let 目录值 = 根目录.join(名称值);
        fs::create_dir_all(目录值.join("crates")).unwrap();
        fs::write(
            目录值.join("keywords.toml"),
            "[\"声明\"]\n\"函数\" = \"fn\"\n",
        )
        .unwrap();
        fs::write(目录值.join("errors.toml"), "[示例]\n\"a\" = \"b\"\n").unwrap();
        fs::write(目录值.join("module_paths.toml"), "[示例]\n\"a\" = \"b\"\n").unwrap();
        fs::write(目录值.join("crates/test.toml"), "[示例]\n\"a\" = \"b\"\n").unwrap();
    }

    #[test]
    fn 安全条目路径拒绝穿越() {
        // 正常相对路径放行
        assert_eq!(
            安全条目路径("zh/keywords.toml"),
            Some(PathBuf::from("zh/keywords.toml"))
        );
        assert_eq!(
            安全条目路径("zh\\keywords.toml"),
            Some(PathBuf::from("zh/keywords.toml")),
            "反斜杠应归一化为正斜杠"
        );
        // 绝对路径（含反斜杠形态）
        assert!(安全条目路径("/etc/passwd").is_none());
        assert!(安全条目路径("\\etc\\passwd").is_none());
        // `..` 上跳
        assert!(安全条目路径("../x").is_none());
        assert!(安全条目路径("a/../../x").is_none());
        assert!(安全条目路径("..\\..\\x").is_none());
        // Windows 盘符前缀：`路径类型::连接` 会整体替换基准目录，务必拒绝
        assert!(安全条目路径("C:/Users/x/Startup/y.cmd").is_none());
        assert!(安全条目路径("c:relative").is_none());
        // 空名
        assert!(安全条目路径("").is_none());
        // 首段含冒号但非盘符（首字符非 ASCII 字母）仍放行
        assert!(安全条目路径("1b:c/x").is_some());
    }

    #[test]
    fn 本地安装与删除流程() {
        let _锁 = 环境锁();
        let 临时根 = tempfile::tempdir().unwrap();
        制作临时语言包(临时根.path(), "日语");
        let 源目录 = 临时根.path().join("日语");
        unsafe {
            std::env::set_var("RZ_LANG_DIR", 临时根.path().join("global"));
        }
        let 全局目录 = 全局语言目录();

        // 1. 安装前：只有内置
        let 清单甲 = 列出语言();
        assert!(
            清单甲
                .iter()
                .all(|信息值| 信息值.语言来源 == 语言来源::内置)
        );

        // 2. 安装
        安装语言(源目录.to_str().unwrap(), false).unwrap();
        assert!(全局目录.join("日语/keywords.toml").is_file());
        assert!(全局目录.join("日语/crates/test.toml").is_file());

        // 3. 列表中出现用户安装的
        let 清单乙 = 列出语言();
        let 日语项 = 清单乙
            .iter()
            .find(|信息值| 信息值.语言代码 == "日语")
            .expect("日语应被列出");
        assert_eq!(日语项.语言来源, 语言来源::用户安装);

        // 4. 重复安装报错
        assert!(安装语言(源目录.to_str().unwrap(), false).is_err());
        // 4b. 强制覆盖成功
        安装语言(源目录.to_str().unwrap(), true).unwrap();
        assert!(全局目录.join("日语/keywords.toml").is_file());

        // 5. 删除内置报错
        assert!(移除语言("zh").is_err());

        // 6. 删除成功
        移除语言("日语").unwrap();
        let 清单丙 = 列出语言();
        assert!(清单丙.iter().all(|信息值| 信息值.语言代码 != "日语"));

        // 7. 删除不存在报错
        assert!(移除语言("不存在").is_err());

        unsafe {
            std::env::remove_var("RZ_LANG_DIR");
        }
    }

    #[test]
    fn 从仓库目录安装流程() {
        let _锁 = 环境锁();
        let 临时根 = tempfile::tempdir().unwrap();
        制作临时语言包(临时根.path(), "俄语");
        let 仓库目录 = 临时根.path();
        unsafe {
            std::env::set_var("RZ_LANG_DIR", 临时根.path().join("global"));
        }

        assert!(从仓库目录安装("日语", 仓库目录, false).is_err());
        从仓库目录安装("俄语", 仓库目录, false).unwrap();
        assert!(全局语言目录().join("俄语/keywords.toml").is_file());
        assert!(从仓库目录安装("俄语", 仓库目录, false).is_err());
        从仓库目录安装("俄语", 仓库目录, true).unwrap();
        assert!(全局语言目录().join("俄语/keywords.toml").is_file());

        unsafe {
            std::env::remove_var("RZ_LANG_DIR");
        }
    }

    /// 语言包放在仓库 `lang-packs/<语言码>/` 子目录时也能安装（兼容旧结构）
    #[test]
    fn 从仓库目录安装嵌套语言包() {
        let _锁 = 环境锁();
        let 临时根 = tempfile::tempdir().unwrap();
        制作临时语言包(&临时根.path().join("lang-packs"), "法语");
        let 仓库目录 = 临时根.path();
        unsafe {
            std::env::set_var("RZ_LANG_DIR", 临时根.path().join("global"));
        }

        从仓库目录安装("法语", 仓库目录, false).unwrap();
        assert!(全局语言目录().join("法语/keywords.toml").is_file());

        unsafe {
            std::env::remove_var("RZ_LANG_DIR");
        }
    }

    /// 语言包放在 `crates/engine/lang-packs/<语言码>/` 时也能安装（主仓库单副本结构）
    #[test]
    fn 从仓库目录安装引擎布局() {
        let _锁 = 环境锁();
        let 临时根 = tempfile::tempdir().unwrap();
        制作临时语言包(&临时根.path().join("crates/engine/lang-packs"), "德语");
        let 仓库目录 = 临时根.path();
        unsafe {
            std::env::set_var("RZ_LANG_DIR", 临时根.path().join("global"));
        }

        从仓库目录安装("德语", 仓库目录, false).unwrap();
        assert!(全局语言目录().join("德语/keywords.toml").is_file());

        unsafe {
            std::env::remove_var("RZ_LANG_DIR");
        }
    }

    #[test]
    fn 解压压缩包并查找() {
        use std::io::Write;
        let _锁 = 环境锁();
        let 临时根 = tempfile::tempdir().unwrap();

        let 压缩包路径 = 临时根.path().join("lang_pack.zip");
        let mut 写入器 = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let 选项值 = zip::write::SimpleFileOptions::default();
        写入器
            .start_file("language-packs-main/中文/keywords.toml", 选项值)
            .unwrap();
        写入器
            .write_all("[\"声明\"]\n\"函数\" = \"fn\"\n".as_bytes())
            .unwrap();
        写入器
            .start_file("language-packs-main/中文/errors.toml", 选项值)
            .unwrap();
        写入器
            .write_all("[示例]\n\"a\" = \"b\"\n".as_bytes())
            .unwrap();
        fs::write(&压缩包路径, 写入器.finish().unwrap().into_inner()).unwrap();

        let 解压目录 = 临时根.path().join("extracted");
        解压压缩包(&压缩包路径, &解压目录).unwrap();
        assert!(
            解压目录
                .join("language-packs-main/中文/keywords.toml")
                .is_file()
        );

        let 中文项 = 在解压目录查找语言(&解压目录, "中文").expect("应找到中文");
        assert!(中文项.join("keywords.toml").is_file());
        assert!(在解压目录查找语言(&解压目录, "俄语").is_none());
    }

    #[test]
    fn 校验语言代码规则() {
        assert!(校验语言代码("中文"));
        assert!(校验语言代码("en-US"));
        assert!(!校验语言代码(""));
        assert!(!校验语言代码("../x"));
        assert!(!校验语言代码("a/b"));
        assert!(!校验语言代码(".."));
        assert!(!校验语言代码("."));
    }

    /// 远程地址只允许 https/ssh/git@ 与本机回环 http
    #[test]
    fn 地址安全规则() {
        assert!(地址安全("https://github.com/x/y"));
        assert!(地址安全("ssh://git@github.com/x/y"));
        assert!(地址安全("git@github.com:x/y.git"));
        assert!(地址安全("http://localhost:18080/x"));
        assert!(地址安全("http://127.0.0.1:18080/x"));
        assert!(地址安全("http://[::1]:18080/x"));
        assert!(!地址安全("http://example.com/x"));
        assert!(!地址安全("ftp://example.com/x"));
        assert!(地址安全("file:///tmp/repo"));
        assert!(地址安全("/tmp/repo"));
        assert!(地址安全("git@github.com:x/y.git"));
    }

    /// 下载入口在发起 curl 前就拒绝远程明文地址
    #[test]
    fn 下载拒绝远程明文() {
        let 临时 = tempfile::tempdir().unwrap();
        assert!(下载到("http://example.com/x", &临时.path().join("x")).is_err());
    }

    /// 第三方语言包扩展名不得覆盖 Rust/配置源码后缀
    #[test]
    fn 扩展名安全规则() {
        assert!(扩展名安全("zh"));
        assert!(扩展名安全("ja"));
        assert!(扩展名安全("py2"));
        assert!(!扩展名安全(""));
        assert!(!扩展名安全("rs"));
        assert!(!扩展名安全("RS"));
        assert!(!扩展名安全("toml"));
        assert!(!扩展名安全("json"));
        assert!(!扩展名安全("lock"));
        assert!(!扩展名安全("a/b"));
        assert!(!扩展名安全("../x"));
        assert!(!扩展名安全("x.y"));
        assert!(!扩展名安全("中文"));
    }

    #[test]
    fn 解析语言信息用例() {
        let 元数据 = 解析语言信息(
            "[\"语言包\"]\n\"名称\" = \"俄语\"\n\"扩展名\" = \"ru\"\n\"版本\" = \"1.0\"\n",
        )
        .unwrap();
        assert_eq!(元数据.名称, "俄语");
        assert_eq!(元数据.扩展名, "ru");
        assert_eq!(元数据.版本, "1.0");

        let 元数据 = 解析语言信息("[语言包]\n名称 = \"日语\"\n扩展名 = \"ja\"\n").unwrap();
        assert_eq!(元数据.名称, "日语");
        assert_eq!(元数据.扩展名, "ja");
        assert_eq!(元数据.版本, "1.0");

        assert!(解析语言信息("[语言包]\n名称 = \"俄语\"\n").is_none());
        assert!(解析语言信息("[其他]\n名称 = \"俄语\"\n扩展名 = \"ru\"\n").is_none());
        assert!(解析语言信息("这不是 TOML").is_none());
    }

    #[test]
    fn 动态扩展名映射用例() {
        let _锁 = 环境锁();
        let 临时根 = tempfile::tempdir().unwrap();
        制作临时语言包(临时根.path(), "日语");
        unsafe {
            std::env::set_var("RZ_LANG_DIR", 临时根.path().join("global"));
        }
        安装语言(临时根.path().join("日语").to_str().unwrap(), false).unwrap();

        assert_eq!(查询扩展名映射("ja").as_deref(), Some("日语"));
        let 清单 = 列出语言();
        let 日语项 = 清单
            .iter()
            .find(|信息值| 信息值.语言代码 == "日语")
            .expect("日语应被列出");
        assert_eq!(日语项.扩展名.as_deref(), Some("ja"));
        assert!(日语项.版本.is_none());

        assert_eq!(查询扩展名映射("zh").as_deref(), Some("zh"));
        let 中文项 = 清单
            .iter()
            .find(|信息值| 信息值.语言代码 == "zh")
            .expect("zh应被列出");
        assert_eq!(中文项.扩展名.as_deref(), Some("zh"));
        assert_eq!(中文项.版本.as_deref(), Some("1.0"));

        // 补写 lang_info.toml
        fs::write(
            临时根.path().join("global/日语/lang_info.toml"),
            "[语言包]\n名称 = \"日语\"\n扩展名 = \"ja\"\n版本 = \"2.1\"\n",
        )
        .unwrap();
        assert_eq!(查询扩展名映射("ja").as_deref(), Some("日语"));
        let 清单 = 列出语言();
        let 日语项 = 清单
            .iter()
            .find(|信息值| 信息值.语言代码 == "日语")
            .expect("日语应被列出");
        assert_eq!(日语项.版本.as_deref(), Some("2.1"));

        assert_eq!(
            静态扩展名映射().get("ru").map(|值项| 值项.as_str()),
            Some("俄语")
        );

        unsafe {
            std::env::remove_var("RZ_LANG_DIR");
        }
    }

    #[test]
    fn 默认仓库源与压缩包地址() {
        assert_eq!(默认仓库源列表.len(), 2);
        assert!(默认仓库源列表[0].starts_with("https://gitcode.com/"));
        assert!(默认仓库源列表[1].starts_with("https://github.com/"));

        let 主平台项 = 仓库源项::从地址("https://gitcode.com/用户名/zrRust");
        assert_eq!(主平台项.仓库地址, "https://gitcode.com/用户名/zrRust");
        assert_eq!(
            主平台项.压缩包地址,
            "https://gitcode.com/用户名/zrRust/repository/archive/master.zip"
        );

        let 备用平台项 = 仓库源项::从地址("https://github.com/用户名/zrRust");
        assert_eq!(
            备用平台项.压缩包地址,
            "https://github.com/用户名/zrRust/archive/refs/heads/main.zip"
        );

        let 本地项 = 仓库源项::从地址("http://127.0.0.1:18080/语言包");
        assert_eq!(
            本地项.压缩包地址,
            "http://127.0.0.1:18080/语言包/archive/refs/heads/main.zip"
        );
        assert_eq!(
            仓库源项::从地址("https://github.com/用户名/zrRust/").仓库地址,
            "https://github.com/用户名/zrRust"
        );
    }

    #[test]
    fn 仓库地址环境变量优先() {
        let _锁 = 环境锁();
        unsafe {
            std::env::set_var("RZ_LANG_REPO", "http://127.0.0.1:18080/自定义仓库/");
            let 源列表 = 收集仓库源();
            assert_eq!(源列表.len(), 1);
            assert_eq!(源列表[0].仓库地址, "http://127.0.0.1:18080/自定义仓库");
            std::env::remove_var("RZ_LANG_REPO");
        }
        let 默认源 = 收集仓库源();
        assert_eq!(默认源.len(), 2);
        assert_eq!(默认源[0].仓库地址, 默认仓库源列表[0]);
        assert_eq!(默认源[1].仓库地址, 默认仓库源列表[1]);
    }

    #[test]
    fn 多源回退安装() {
        let _锁 = 环境锁();
        let 临时根 = tempfile::tempdir().unwrap();
        let 备用仓库 = 临时根.path().join("lang_repo");
        fs::create_dir_all(&备用仓库).unwrap();
        制作临时语言包(&备用仓库, "中文");
        for 参数值 in [
            &["init", "-b", "main"][..],
            &["add", "."][..],
            &["commit", "-m", "init"][..],
        ] {
            let 结果值 = Command::new("git")
                .args(参数值)
                .current_dir(&备用仓库)
                .env("GIT_AUTHOR_NAME", "测试")
                .env("GIT_AUTHOR_EMAIL", "test@example.com")
                .env("GIT_COMMITTER_NAME", "测试")
                .env("GIT_COMMITTER_EMAIL", "test@example.com")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output();
            let 状态值 = 结果值.expect("执行 git 失败").status;
            assert!(状态值.success(), "git 子命令 {:?} 失败", 参数值[0]);
        }

        let 首选源 = 仓库源项::从地址("file:///不存在的仓库路径");
        let 备用源 = 仓库源项::从地址(备用仓库.to_str().unwrap());
        unsafe {
            std::env::set_var("RZ_LANG_DIR", 临时根.path().join("global"));
        }

        let 临时 = 临时目录句柄::新建().unwrap();
        let 结果值 = 尝试全部源("中文", &[首选源, 备用源], &临时, false);
        assert!(
            结果值.is_ok(),
            "首选源失败后应自动回退备用源：{}",
            结果值
                .err()
                .map(|错误值| 错误值.to_string())
                .unwrap_or_default()
        );
        assert!(全局语言目录().join("中文/keywords.toml").is_file());

        unsafe {
            std::env::remove_var("RZ_LANG_DIR");
        }
    }

    #[test]
    fn 全部源失败错误信息() {
        let _锁 = 环境锁();
        let 临时根 = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("RZ_LANG_DIR", 临时根.path().join("global"));
            // 固定界面语言为中文，保证错误消息断言稳定（系统语言无关）
            std::env::set_var("RZ_LANG", "zh");
        }
        let 坏源甲 = 仓库源项::从地址("file:///不存在的仓库1");
        let 坏源乙 = 仓库源项::从地址("file:///不存在的仓库2");
        let 临时 = 临时目录句柄::新建().unwrap();
        let 错误文本 = 尝试全部源("中文", &[坏源甲, 坏源乙], &临时, false)
            .expect_err("两个源都失败应报错")
            .to_string();
        assert!(
            错误文本.contains("已依次尝试 2 个源"),
            "应报告尝试的源数量：{}",
            错误文本
        );
        assert!(
            错误文本.contains("file:///不存在的仓库1"),
            "应列出源1：{}",
            错误文本
        );
        assert!(
            错误文本.contains("file:///不存在的仓库2"),
            "应列出源2：{}",
            错误文本
        );
        assert!(
            错误文本.contains("RZ_LANG_REPO"),
            "应建议 RZ_LANG_REPO：{}",
            错误文本
        );
        unsafe {
            std::env::remove_var("RZ_LANG_DIR");
            std::env::remove_var("RZ_LANG");
        }
    }

    /// 市场扫描：三种仓库布局全部覆盖，无 keywords.toml 的目录不误报
    #[test]
    fn 扫描仓库语言三种布局() {
        let 临时根 = tempfile::tempdir().unwrap();
        // 1. 仓库根直接是语言包（自建语言包仓库约定）
        制作临时语言包(临时根.path(), "中文");
        // 2. lang-packs/ 布局
        制作临时语言包(&临时根.path().join("lang-packs"), "日语");
        // 3. crates/engine/lang-packs/ 布局（主仓库单副本）
        制作临时语言包(&临时根.path().join("crates/engine/lang-packs"), "俄语");
        // 非语言包目录（无 keywords.toml）不应列出
        fs::create_dir_all(临时根.path().join("docs")).unwrap();

        let 已找到 = 扫描仓库语言(临时根.path());
        // 扫描顺序：根 → lang-packs → crates/engine（排序在搜索入口完成）
        let 代码们: Vec<&str> = 已找到
            .iter()
            .map(|信息值| 信息值.语言代码.as_str())
            .collect();
        assert_eq!(代码们, ["中文", "日语", "俄语"]);
        assert!(已找到.iter().all(|信息值| 信息值.显示名称.is_none()));
    }

    /// 市场扫描：布局间同名去重（保留先发现的根目录版本）
    #[test]
    fn 扫描仓库语言去重() {
        let 临时根 = tempfile::tempdir().unwrap();
        制作临时语言包(临时根.path(), "中文");
        制作临时语言包(&临时根.path().join("lang-packs"), "中文");

        let 已找到 = 扫描仓库语言(临时根.path());
        assert_eq!(已找到.len(), 1, "同名语言包应去重：{:?}", 已找到);
    }

    /// 关键词过滤：匹配语言代码或显示名称，不区分大小写
    #[test]
    fn 过滤远程语言用例() {
        let 已找到 = vec![
            远程语言信息 {
                语言代码: "vi".to_string(),
                显示名称: Some("越南语".to_string()),
                版本: Some("1.0".to_string()),
            },
            远程语言信息 {
                语言代码: "zh".to_string(),
                显示名称: Some("中文".to_string()),
                版本: None,
            },
        ];

        // 匹配代码（小写关键词）
        let 按代码 = 过滤远程语言(已找到.clone(), "VI");
        assert_eq!(按代码.len(), 1);
        assert_eq!(按代码[0].语言代码, "vi");
        // 匹配显示名称
        let 按名称 = 过滤远程语言(已找到.clone(), "越南");
        assert_eq!(按名称.len(), 1);
        // 空关键词返回全部
        assert_eq!(过滤远程语言(已找到.clone(), "  ").len(), 2);
        // 无匹配返回空
        assert!(过滤远程语言(已找到, "英语").is_empty());
    }

    // ==================== 网络下载层回环服务集成测试 ====================
    //
    // 安装链路（下载命令取压缩包 → 解压 → 单层根目录下沉 → 源回退）是
    // 语言管理对外不确定性最大的面，此前仅有本地文件形态与纯函数
    // 单测，网络传输段只能手动触达。以下用本机回环服务 + 内存压缩包
    // 复现真实链路（真下载子进程，无外网依赖）：网络错误码必须响亮失败
    // （失败即停语义，不得拿半坏文件继续）、成功路径下沉与解压精确、
    // 克隆在回环源上失败时必须由压缩包回退兜底。

    /// 制作带"单一顶层目录"的仓库归档压缩包（主平台/备用平台归档布局）
    fn 制作仓库归档(顶层目录: &str, 语言码: &str) -> Vec<u8> {
        use std::io::Write as _;
        use zip::write::FileOptions;
        let mut 写入器 = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let 选项值: FileOptions<'_, '_, ()> =
            FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        写入器
            .start_file(format!("{顶层目录}/{语言码}/keywords.toml"), 选项值)
            .unwrap();
        写入器
            .write_all("[\"声明\"]\n\"函数\" = \"fn\"\n".as_bytes())
            .unwrap();
        写入器
            .start_file(format!("{顶层目录}/{语言码}/errors.toml"), 选项值)
            .unwrap();
        写入器
            .write_all("[示例]\n\"a\" = \"b\"\n".as_bytes())
            .unwrap();
        写入器.finish().unwrap().into_inner()
    }

    /// 启动回环服务：对至多 `次数` 个连接应答固定状态码与正文后退出
    ///
    /// 绑定临时端口返回给调用方；请求无正文，读一次即应答；
    /// 应答完即关连接（连接即关），克隆/下载解析失败均为快速失败不会挂起。
    fn 启动回环服务(状态码: u16, 响应体: Vec<u8>, 次数: usize) -> u16 {
        use std::io::{Read as _, Write as _};
        let 监听器 = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let 端口值 = 监听器.local_addr().unwrap().port();
        let 原因值 = if 状态码 == 200 { "OK" } else { "Not Found" };
        std::thread::spawn(move || {
            for 流值 in 监听器.incoming().take(次数) {
                let Ok(mut 流内容) = 流值 else { continue };
                let mut 缓冲值 = [0; 4096];
                let _ = 流内容.read(&mut 缓冲值);
                let 头部 = format!(
                    "HTTP/1.1 {状态码} {原因值}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    响应体.len()
                );
                let _ = 流内容.write_all(头部.as_bytes());
                let _ = 流内容.write_all(&响应体);
                let _ = 流内容.flush();
            }
        });
        端口值
    }

    /// 成功路径：下载→解压→单一顶层目录自动下沉，语言包目录完整可见
    #[test]
    fn 回环下载成功并下沉() {
        let 压缩包 = 制作仓库归档("i18n-rust-main", "xy");
        let 端口值 = 启动回环服务(200, 压缩包, 1);
        let 源项 = 仓库源项::从地址(&format!("http://127.0.0.1:{端口值}/repo"));
        assert_eq!(
            源项.压缩包地址,
            format!("http://127.0.0.1:{端口值}/repo/archive/refs/heads/main.zip")
        );
        let 临时 = 临时目录句柄::新建().unwrap();
        let 根目录 = 拉取仓库压缩包(&源项, &临时).expect("回环压缩包下载解压应成功");
        assert_eq!(根目录.file_name().unwrap(), "i18n-rust-main");
        assert!(
            根目录.join("xy/keywords.toml").is_file(),
            "单一顶层目录应自动下沉：{:?}",
            根目录
        );
    }

    /// 网络错误码必须响亮失败：错误消息含失败地址，不得留下半坏文件继续解压
    #[test]
    fn 回环下载错误响亮失败() {
        let 端口值 = 启动回环服务(404, Vec::new(), 1);
        let 源项 = 仓库源项::从地址(&format!("http://127.0.0.1:{端口值}/repo"));
        let 临时 = 临时目录句柄::新建().unwrap();
        let 错误值 = 拉取仓库压缩包(&源项, &临时).expect_err("网络 404 必须失败");
        assert!(
            错误值.to_string().contains(&源项.压缩包地址),
            "错误应包含失败地址：{错误值}"
        );
    }

    /// 真实回环源的克隆失败→下载兜底链路（本地文件形态已有测试，这里覆盖
    /// 回环形态）：模拟服务对任何请求都回压缩包字节，克隆必失败，
    /// 尝试单个源必须回退到下载压缩包并安装成功
    #[test]
    fn 克隆失败回退下载() {
        let _锁 = 环境锁();
        let 临时根 = tempfile::tempdir().unwrap();
        // Rust 2024：进程级环境变量读写为不安全，由环境锁串行化
        unsafe {
            std::env::set_var("RZ_LANG_DIR", 临时根.path().join("global"));
        }
        let 压缩包 = 制作仓库归档("repo-main", "xy");
        // 应答预算充裕：克隆失败前可能发 1~2 个请求，下载再取一次压缩包
        let 端口值 = 启动回环服务(200, 压缩包, 6);
        let 源项 = 仓库源项::从地址(&format!("http://127.0.0.1:{端口值}/repo"));
        let 临时 = 临时目录句柄::新建().unwrap();
        尝试单个源("xy", &源项, &临时, false).expect("克隆失败应由压缩包回退兜底");
        assert!(
            全局语言目录().join("xy/keywords.toml").is_file(),
            "回退安装应落盘到全局语言目录"
        );
        unsafe {
            std::env::remove_var("RZ_LANG_DIR");
        }
    }

    // ==================== 首次信任内容指纹 ====================

    #[test]
    fn 内容指纹确定性() {
        let 临时 = tempfile::tempdir().unwrap();
        let 目录值 = 临时.path();
        fs::write(目录值.join("keywords.toml"), "[a]\nb = 1\n").unwrap();
        fs::create_dir_all(目录值.join("crates")).unwrap();
        fs::write(目录值.join("crates/rand.toml"), "x = 2\n").unwrap();
        let 甲 = 内容指纹(目录值).unwrap();
        let 乙 = 内容指纹(目录值).unwrap();
        assert_eq!(甲, 乙, "相同内容应产生相同指纹");
        assert_eq!(甲.len(), 64);
        assert!(甲.chars().all(是十六进制位));
    }

    #[test]
    fn 内容指纹检测变化() {
        let 临时 = tempfile::tempdir().unwrap();
        let 目录值 = 临时.path();
        fs::write(目录值.join("keywords.toml"), "v = 1\n").unwrap();
        let 前值 = 内容指纹(目录值).unwrap();
        fs::write(目录值.join("keywords.toml"), "v = 2\n").unwrap();
        let 后值 = 内容指纹(目录值).unwrap();
        assert_ne!(前值, 后值, "内容变化应改变指纹");
    }

    #[test]
    fn 内容指纹忽略旁路文件() {
        let 临时 = tempfile::tempdir().unwrap();
        let 目录值 = 临时.path();
        fs::write(目录值.join("keywords.toml"), "v = 1\n").unwrap();
        let 无旁路值 = 内容指纹(目录值).unwrap();
        写入记录指纹(目录值, &无旁路值).unwrap();
        let 有旁路值 = 内容指纹(目录值).unwrap();
        assert_eq!(无旁路值, 有旁路值, "旁路文件不应计入指纹");
    }

    #[test]
    fn 记录指纹往返与畸形() {
        let 临时 = tempfile::tempdir().unwrap();
        let 目录值 = 临时.path();
        assert_eq!(读取记录指纹(目录值), None, "缺失 -> 空值");
        let 有效值 = "a".repeat(64);
        写入记录指纹(目录值, &有效值).unwrap();
        assert_eq!(读取记录指纹(目录值).as_deref(), Some(有效值.as_str()));
        fs::write(目录值.join(完整性文件), "not-a-hash\n").unwrap();
        assert_eq!(读取记录指纹(目录值), None, "畸形 -> 空值");
    }
}
