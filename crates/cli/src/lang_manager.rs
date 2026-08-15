// 语言包管理模块
//
// 实现 `rzc lang list / install / remove` 子命令的底层逻辑：
// 管理用户全局语言包目录（默认 `~/.rz/lang-packs/`，可用 `RZ_LANG_DIR` 环境变量覆盖）。
//
// 每个语言包是一个目录，目录名即语言代码（如 `zh`、`ru`），
// 内含 `keywords.toml`、`errors.toml`、`module_paths.toml` 及可选的 `crates/` 子目录。
//
// 远程安装：默认依次尝试 [`DEFAULT_REPO_SOURCES`]（GitCode 首选，失败自动回退 GitHub），
// 每个源优先 `git clone`，git 不存在或克隆失败时回退 `curl` 下载 ZIP 压缩包并解压；
// 设置 `RZ_LANG_REPO` 环境变量后完全使用用户指定地址（不再尝试默认源）。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 默认远程语言包源列表（按顺序依次尝试）
///
/// 首选 GitCode（中国用户访问速度快），失败自动回退 GitHub（海外用户可用）。
/// 仓库结构：每个语言包一个目录，目录名即语言代码，内含
/// `keywords.toml`、`errors.toml`、`module_paths.toml` 及可选 `crates/` 子目录。
///
/// 注意：占位符使用前请替换为实际的 GitCode/GitHub 用户名。
/// 设置 `RZ_LANG_REPO` 环境变量后完全使用用户指定地址，不再尝试默认源。
pub const DEFAULT_REPO_SOURCES: [&str; 2] = [
    "https://gitcode.com/tan80/zrRust",
    "https://github.com/liuqiTan80/i18n-rust",
];

/// 远程仓库源：git clone 地址与 curl 下载 ZIP 的地址
#[derive(Debug, Clone)]
struct RepoSource {
    /// git clone 使用的仓库地址
    git_url: String,
    /// curl 下载 ZIP 压缩包的地址
    zip_url: String,
}

impl RepoSource {
    /// 从仓库地址生成 git 与 ZIP 下载地址
    ///
    /// - GitCode：`<仓库>/repository/archive/master.zip`（默认分支 master）
    /// - GitHub：`<仓库>/archive/refs/heads/main.zip`（默认分支 main）
    /// - 其他平台（含本地 HTTP 服务器测试）：按 GitHub 风格生成
    fn from_url(url: &str) -> Self {
        let url = url.trim_end_matches('/');
        let zip_url = if url.contains("gitcode.com") {
            format!("{}/repository/archive/master.zip", url)
        } else {
            format!("{}/archive/refs/heads/main.zip", url)
        };
        Self {
            git_url: url.to_string(),
            zip_url,
        }
    }
}

/// 收集本次安装要尝试的仓库源列表
///
/// 若设置了 `RZ_LANG_REPO` 环境变量，完全使用用户指定的地址；
/// 否则依次尝试 [`DEFAULT_REPO_SOURCES`]（GitCode 首选，GitHub 备用）。
fn collect_sources() -> Vec<RepoSource> {
    if let Ok(custom) = std::env::var("RZ_LANG_REPO") {
        let custom = custom.trim().trim_end_matches('/').to_string();
        if !custom.is_empty() {
            return vec![RepoSource::from_url(&custom)];
        }
    }
    DEFAULT_REPO_SOURCES
        .iter()
        .map(|url| RepoSource::from_url(url))
        .collect()
}

/// 语言包来源
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// 内置在 rzc 可执行文件中（不可删除）
    Builtin,
    /// 用户通过 `rzc lang install` 安装到全局目录
    UserInstalled,
}

/// 单个已安装语言包的信息
pub struct LangInfo {
    /// 语言代码（目录名）
    pub lang_code: String,
    /// 来源
    pub source: Source,
    /// 扩展名（不含点）；旧语言包无 lang_info.toml 时从静态映射推断
    pub extension: Option<String>,
    /// 版本号；旧语言包无 lang_info.toml 时为 None
    pub version: Option<String>,
    /// 语言显示名称（来自 lang_info.toml；旧语言包为 None）
    pub display_name: Option<String>,
}

/// 语言包元数据（来自 lang_info.toml）
pub struct LangMetadata {
    /// 语言名称
    pub name: String,
    /// 源码文件扩展名（不含点）
    pub extension: String,
    /// 版本号
    pub version: String,
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
/// 版本缺省时为 "1.0"；名称或扩展名缺失/为空时返回 None。
/// 兼容用户手写的裸键写法：标准解析失败时回退手工解析。
fn parse_lang_info(content: &str) -> Option<LangMetadata> {
    // 1. 标准 TOML 解析：兼容引号包裹的键
    if let Ok(value) = toml::from_str::<toml::Value>(content)
        && let Some(table) = value.get("语言包")
        && let Some(metadata) = extract_from_table(table)
    {
        return Some(metadata);
    }
    // 2. 回退手工解析：兼容中文裸键的用户示例写法
    manual_parse(content)
}

/// 从解析出的 [语言包] 节表提取元数据
fn extract_from_table(table: &toml::Value) -> Option<LangMetadata> {
    let name = table.get("名称")?.as_str()?.trim();
    let extension = table.get("扩展名")?.as_str()?.trim();
    if name.is_empty() || extension.is_empty() {
        return None;
    }
    let version = table
        .get("版本")
        .and_then(|v| v.as_str())
        .unwrap_or("1.0")
        .trim()
        .to_string();
    Some(LangMetadata {
        name: name.to_string(),
        extension: extension.to_string(),
        version,
    })
}

/// 手工逐行解析 `键 = "值"`，兼容标准 TOML 不允许的非 ASCII 裸键
///
/// 只识别 名称 / 扩展名 / 版本 三个字段，忽略注释与节头。
fn manual_parse(content: &str) -> Option<LangMetadata> {
    let mut name: Option<String> = None;
    let mut extension: Option<String> = None;
    let mut version: Option<String> = None;
    let mut has_lang_section = false;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            let section_name = line
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim_matches('"')
                .trim();
            if section_name == "语言包" {
                has_lang_section = true;
            }
            continue;
        }
        let (key, val) = line.split_once('=')?;
        let key = key.trim().trim_matches('"').trim();
        let val = val.trim().trim_matches('"').trim();
        if val.is_empty() {
            return None;
        }
        match key {
            "名称" => name = Some(val.to_string()),
            "扩展名" => extension = Some(val.to_string()),
            "版本" => version = Some(val.to_string()),
            _ => {}
        }
    }
    if !has_lang_section {
        return None;
    }
    Some(LangMetadata {
        name: name?,
        extension: extension?,
        version: version.unwrap_or_else(|| "1.0".to_string()),
    })
}

/// 读取目录中语言包的元数据（读取并解析 lang_info.toml）
pub fn read_lang_info(dir: &Path) -> Option<LangMetadata> {
    let content = fs::read_to_string(dir.join("lang_info.toml")).ok()?;
    parse_lang_info(&content)
}

/// 获取内置语言包的元数据（来自嵌入可执行文件的 lang_info.toml）
pub fn get_builtin_metadata(lang_code: &str) -> Option<LangMetadata> {
    // 内置代码列表内的语言必然存在；未知代码回退到中文不影响元数据查询
    let data = crate::builtin_lang::get_builtin_data(lang_code);
    parse_lang_info(data.lang_info_toml)
}

/// 获取全局语言包根目录
///
/// 优先使用 `RZ_LANG_DIR` 环境变量，否则默认 `~/.rz/lang-packs/`。
pub fn global_lang_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("RZ_LANG_DIR")
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".rz").join("lang-packs")
}

/// 列出所有已安装的语言包（内置 + 用户安装）
///
/// 内置语言包固定在前，用户安装的按名称排序在后。
/// 扩展名/版本来自 lang_info.toml；旧语言包无该文件时扩展名回退静态映射推断。
pub fn list_langs() -> Vec<LangInfo> {
    let mut list: Vec<LangInfo> = crate::builtin_lang::builtin_lang_codes()
        .into_iter()
        .map(|code| {
            let metadata = get_builtin_metadata(code);
            LangInfo {
                lang_code: code.to_string(),
                source: Source::Builtin,
                extension: metadata
                    .as_ref()
                    .map(|m| m.extension.clone())
                    .or_else(|| static_code_to_extension(code)),
                version: metadata.as_ref().map(|m| m.version.clone()),
                display_name: metadata.map(|m| m.name.clone()),
            }
        })
        .collect();

    // 扫描全局目录：含 keywords.toml 的子目录视为语言包
    let root_dir = global_lang_dir();
    let mut user_installed: Vec<(String, Option<LangMetadata>)> = Vec::new();
    if let Ok(entries) = fs::read_dir(&root_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir()
                && path.join("keywords.toml").is_file()
                && let Some(name) = path.file_name().and_then(|s| s.to_str())
            {
                user_installed.push((name.to_string(), read_lang_info(&path)));
            }
        }
    }
    user_installed.sort_by(|a, b| a.0.cmp(&b.0));
    for (code, metadata) in user_installed {
        list.push(LangInfo {
            extension: metadata
                .as_ref()
                .map(|m| m.extension.clone())
                .or_else(|| static_code_to_extension(&code)),
            version: metadata.as_ref().map(|m| m.version.clone()),
            display_name: metadata.map(|m| m.name.clone()),
            lang_code: code,
            source: Source::UserInstalled,
        });
    }
    list
}

/// 静态扩展名映射（扩展名 → 语言代码）
///
/// 作为动态映射的回退，保证无 lang_info.toml 的旧语言包向后兼容。
pub fn static_extension_map() -> HashMap<String, String> {
    [
        ("zh", "中文"),
        ("ru", "俄语"),
        ("ja", "日语"),
        ("ko", "韩语"),
        ("en", "英语"),
    ]
    .into_iter()
    .map(|(ext, code)| (ext.to_string(), code.to_string()))
    .collect()
}

/// 静态映射反查：语言代码 → 扩展名（旧语言包推断用）
fn static_code_to_extension(lang_code: &str) -> Option<String> {
    static_extension_map()
        .into_iter()
        .find(|(_, code)| code == lang_code)
        .map(|(ext, _)| ext)
}

/// 构建 扩展名 → 语言代码 的动态映射表
///
/// 来源：内置语言包元数据 + 全局用户语言包目录。
/// 用户安装的语言包优先于内置（同名扩展名时覆盖）；
/// 无 lang_info.toml 的旧语言包回退静态映射推断。
pub fn build_extension_map() -> HashMap<String, String> {
    let mut map = HashMap::new();

    // 1. 内置语言包
    for code in crate::builtin_lang::builtin_lang_codes() {
        if let Some(metadata) = get_builtin_metadata(code) {
            map.insert(metadata.extension, code.to_string());
        } else if let Some(ext) = static_code_to_extension(code) {
            map.insert(ext, code.to_string());
        }
    }

    // 2. 全局用户语言包（覆盖内置）
    let root_dir = global_lang_dir();
    if let Ok(entries) = fs::read_dir(&root_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() || !path.join("keywords.toml").is_file() {
                continue;
            }
            let Some(code) = path.file_name().and_then(|s| s.to_str()).map(String::from) else {
                continue;
            };
            if let Some(metadata) = read_lang_info(&path) {
                map.insert(metadata.extension, code.clone());
            } else if let Some(ext) = static_code_to_extension(&code) {
                map.insert(ext, code);
            }
        }
    }
    map
}

/// 查询 扩展名 → 语言代码 的动态映射
pub fn query_extension_map(extension: &str) -> Option<String> {
    build_extension_map().remove(extension)
}

/// 所有当前可用的扩展名（按名称排序）
pub fn all_available_extensions() -> Vec<String> {
    let mut list: Vec<String> = build_extension_map().into_keys().collect();
    list.sort();
    list
}

/// 安装语言包
///
/// - `<source>` 是本地存在的目录路径：校验结构后整体复制到全局语言包目录。
/// - 否则视为语言代码：从远程仓库下载安装。
/// - 目标已存在时默认报错；`force` 为 true 时覆盖安装。
pub fn install_lang(source: &str, force: bool) -> anyhow::Result<()> {
    let source_path = PathBuf::from(source);
    if source_path.is_dir() {
        validate_lang_pack_dir(&source_path)?;
        let lang_code = source_path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "{}",
                    crate::ui::Ui::global().f(
                        "lc_err_lang_code_from_path",
                        &[&source_path.display().to_string()]
                    )
                )
            })?;
        return copy_to_global_dir(&source_path, lang_code, force);
    }
    install_remote_lang(source, force)
}

/// 删除用户安装的语言包
///
/// 内置语言包不可删除；同名用户安装包存在时优先删除用户安装的。
pub fn remove_lang(lang_code: &str) -> anyhow::Result<()> {
    let ui = crate::ui::Ui::global();
    let target_dir = global_lang_dir().join(lang_code);
    if target_dir.is_dir() {
        fs::remove_dir_all(&target_dir)?;
        println!("{}", ui.f("lang_removed", &[lang_code]));
        return Ok(());
    }

    if crate::builtin_lang::builtin_lang_codes().contains(&lang_code) {
        anyhow::bail!("{}", ui.f("lang_builtin_not_removable", &[lang_code]));
    }

    anyhow::bail!(
        "{}",
        ui.f(
            "lang_not_found",
            &[lang_code, &target_dir.display().to_string()]
        )
    );
}

/// 从远程仓库安装语言包
fn install_remote_lang(lang_code: &str, force: bool) -> anyhow::Result<()> {
    let ui = crate::ui::Ui::global();
    if !validate_lang_code(lang_code) {
        anyhow::bail!("{}", ui.f("invalid_lang_code", &[lang_code]));
    }
    let sources = collect_sources();
    let temp = TempDir::new()?;
    try_all_sources(lang_code, &sources, &temp, force)
}

/// 依次尝试多个源，全部失败时聚合错误并建议 `RZ_LANG_REPO`
fn try_all_sources(
    lang_code: &str,
    sources: &[RepoSource],
    temp: &TempDir,
    force: bool,
) -> anyhow::Result<()> {
    let mut error_details = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        match try_single_source(lang_code, source, temp, force) {
            Ok(()) => return Ok(()),
            Err(err) => {
                error_details.push(crate::ui::Ui::global().f(
                    "lc_err_source_detail",
                    &[&(index + 1).to_string(), &source.git_url, &err.to_string()],
                ))
            }
        }
    }
    let ui = crate::ui::Ui::global();
    anyhow::bail!(
        "{}",
        ui.f(
            "remote_install_failed",
            &[
                lang_code,
                &sources.len().to_string(),
                &error_details.join("\n"),
            ]
        )
    )
}

/// 尝试从单个源安装：优先 git clone，失败回退 curl 下载 ZIP
fn try_single_source(
    lang_code: &str,
    source: &RepoSource,
    temp: &TempDir,
    force: bool,
) -> anyhow::Result<()> {
    let clone_dir = temp.path().join("repo");
    let git_result = Command::new("git")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(&source.git_url)
        .arg(&clone_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output();
    match git_result {
        Ok(output) if output.status.success() => {
            install_from_repo_dir(lang_code, &clone_dir, force)
        }
        Ok(output) => {
            let git_error = String::from_utf8_lossy(&output.stderr).trim().to_string();
            match try_curl_install(lang_code, &source.zip_url, temp, force) {
                Ok(()) => Ok(()),
                Err(curl_error) => {
                    if git_error.is_empty() {
                        Err(curl_error)
                    } else {
                        anyhow::bail!(
                            "{}",
                            crate::ui::Ui::global().f(
                                "lc_err_git_curl_both",
                                &[&git_error, &curl_error.to_string()]
                            )
                        )
                    }
                }
            }
        }
        Err(err) => {
            if err.kind() == std::io::ErrorKind::NotFound {
                return try_curl_install(lang_code, &source.zip_url, temp, force);
            }
            Err(anyhow::anyhow!(
                "{}",
                crate::ui::Ui::global().f("lc_err_git_run", &[&err.to_string()])
            ))
        }
    }
}

/// 从仓库目录安装指定语言包
fn install_from_repo_dir(lang_code: &str, repo_dir: &Path, force: bool) -> anyhow::Result<()> {
    let lang_source = repo_dir.join(lang_code);
    if !lang_source.is_dir() {
        let ui = crate::ui::Ui::global();
        anyhow::bail!("{}", ui.f("remote_lang_not_found", &[lang_code]));
    }
    validate_lang_pack_dir(&lang_source)?;
    copy_to_global_dir(&lang_source, lang_code, force)
}

/// 回退方案：curl 下载 ZIP 压缩包并解压安装
fn try_curl_install(
    lang_code: &str,
    zip_url: &str,
    temp: &TempDir,
    force: bool,
) -> anyhow::Result<()> {
    let download_path = temp.path().join("lang_pack.zip");
    let output = Command::new("curl")
        .arg("-L")
        .arg("--fail")
        .arg("-o")
        .arg(&download_path)
        .arg(zip_url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| {
            anyhow::anyhow!(
                "{}",
                crate::ui::Ui::global().f("lc_err_curl_run", &[&e.to_string()])
            )
        })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "{}",
            crate::ui::Ui::global().f(
                "lc_err_curl_download",
                &[zip_url, &detail.trim().to_string()]
            )
        );
    }
    let extract_dir = temp.path().join("extracted");
    extract_zip(&download_path, &extract_dir)?;
    let lang_source = find_lang_in_extracted(&extract_dir, lang_code).ok_or_else(|| {
        let ui = crate::ui::Ui::global();
        anyhow::anyhow!("{}", ui.f("remote_lang_not_found", &[lang_code]))
    })?;
    validate_lang_pack_dir(&lang_source)?;
    copy_to_global_dir(&lang_source, lang_code, force)
}

/// 解压 ZIP 压缩包到目标目录（保留完整条目路径）
///
/// 拒绝含 `..` 路径段的条目，防止路径穿越。
fn extract_zip(archive_path: &Path, extract_dir: &Path) -> anyhow::Result<()> {
    let file = fs::File::open(archive_path).map_err(|e| {
        anyhow::anyhow!(
            "{}",
            crate::ui::Ui::global().f(
                "lc_err_open_zip",
                &[&archive_path.display().to_string(), &e.to_string()]
            )
        )
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        anyhow::anyhow!(
            "{}",
            crate::ui::Ui::global().f("lc_err_read_zip", &[&e.to_string()])
        )
    })?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| {
            anyhow::anyhow!(
                "{}",
                crate::ui::Ui::global().f("lc_err_read_zip_entry", &[&e.to_string()])
            )
        })?;
        let name = entry.name().replace('\\', "/");
        if name.starts_with('/') || name.split('/').any(|seg| seg == "..") {
            continue;
        }
        let target = extract_dir.join(&name);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = fs::File::create(&target).map_err(|e| {
                anyhow::anyhow!(
                    "{}",
                    crate::ui::Ui::global().f(
                        "lc_err_write_file",
                        &[&target.display().to_string(), &e.to_string()]
                    )
                )
            })?;
            std::io::copy(&mut entry, &mut file)?;
        }
    }
    Ok(())
}

/// 在解压目录中查找语言包目录
fn find_lang_in_extracted(extract_dir: &Path, lang_code: &str) -> Option<PathBuf> {
    let direct = extract_dir.join(lang_code);
    if direct.is_dir() {
        return Some(direct);
    }
    for entry in fs::read_dir(extract_dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let nested = path.join(lang_code);
            if nested.is_dir() {
                return Some(nested);
            }
        }
    }
    None
}

/// 将源语言包目录复制到全局目录
fn copy_to_global_dir(source_dir: &Path, lang_code: &str, force: bool) -> anyhow::Result<()> {
    let ui = crate::ui::Ui::global();
    let target_dir = global_lang_dir().join(lang_code);
    if target_dir.exists() {
        if !force {
            anyhow::bail!(
                "{}",
                ui.f(
                    "lang_already_installed",
                    &[lang_code, &target_dir.display().to_string(), lang_code]
                )
            );
        }
        fs::remove_dir_all(&target_dir)?;
    }
    copy_dir_recursive(source_dir, &target_dir)?;
    println!(
        "{}",
        ui.f("lang_installed", &[lang_code, &target_dir.display().to_string()])
    );
    println!("{}", ui.t("lang_install_hint"));
    Ok(())
}

/// 校验语言包目录结构：必须包含 keywords.toml
fn validate_lang_pack_dir(path: &Path) -> anyhow::Result<()> {
    if !path.join("keywords.toml").is_file() {
        let ui = crate::ui::Ui::global();
        anyhow::bail!("{}", ui.f("invalid_lang_dir", &[&path.display().to_string()]));
    }
    Ok(())
}

/// 校验语言代码可作为安全目录名（防止路径穿越）
fn validate_lang_code(code: &str) -> bool {
    !code.is_empty() && !code.contains(['/', '\\']) && code != "." && code != ".."
}

/// 递归复制目录
fn copy_dir_recursive(source: &Path, target: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else {
            fs::copy(&source_path, &target_path)?;
        }
    }
    Ok(())
}

/// 临时目录句柄：Drop 时递归清理
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> anyhow::Result<Self> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path =
            std::env::temp_dir().join(format!("rzc-lang-{}-{}", std::process::id(), timestamp));
        fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 环境变量互斥锁：测试并行运行时保护 RZ_LANG_DIR 等全局环境变量
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 在临时根下制作一个最小语言包目录（含 crates/ 子目录）
    fn make_temp_lang_pack(root: &Path, name: &str) {
        let dir = root.join(name);
        fs::create_dir_all(dir.join("crates")).unwrap();
        fs::write(dir.join("keywords.toml"), "[\"声明\"]\n\"函数\" = \"fn\"\n").unwrap();
        fs::write(dir.join("errors.toml"), "[示例]\n\"a\" = \"b\"\n").unwrap();
        fs::write(dir.join("module_paths.toml"), "[示例]\n\"a\" = \"b\"\n").unwrap();
        fs::write(dir.join("crates/test.toml"), "[示例]\n\"a\" = \"b\"\n").unwrap();
    }

    #[test]
    fn test_local_install_and_delete_flow() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_root = tempfile::tempdir().unwrap();
        make_temp_lang_pack(temp_root.path(), "日语");
        let source_dir = temp_root.path().join("日语");
        unsafe {
            std::env::set_var("RZ_LANG_DIR", temp_root.path().join("global"));
        }
        let global_dir = global_lang_dir();

        // 1. 安装前：只有内置
        let list1 = list_langs();
        assert!(list1.iter().all(|info| info.source == Source::Builtin));

        // 2. 安装
        install_lang(source_dir.to_str().unwrap(), false).unwrap();
        assert!(global_dir.join("日语/keywords.toml").is_file());
        assert!(global_dir.join("日语/crates/test.toml").is_file());

        // 3. 列表中出现用户安装的
        let list2 = list_langs();
        let japanese = list2
            .iter()
            .find(|info| info.lang_code == "日语")
            .expect("日语应被列出");
        assert_eq!(japanese.source, Source::UserInstalled);

        // 4. 重复安装报错
        assert!(install_lang(source_dir.to_str().unwrap(), false).is_err());
        // 4b. --force 覆盖成功
        install_lang(source_dir.to_str().unwrap(), true).unwrap();
        assert!(global_dir.join("日语/keywords.toml").is_file());

        // 5. 删除内置报错
        assert!(remove_lang("zh").is_err());

        // 6. 删除成功
        remove_lang("日语").unwrap();
        let list3 = list_langs();
        assert!(list3.iter().all(|info| info.lang_code != "日语"));

        // 7. 删除不存在报错
        assert!(remove_lang("不存在").is_err());

        unsafe {
            std::env::remove_var("RZ_LANG_DIR");
        }
    }

    #[test]
    fn test_install_from_repo_dir() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_root = tempfile::tempdir().unwrap();
        make_temp_lang_pack(temp_root.path(), "俄语");
        let repo = temp_root.path();
        unsafe {
            std::env::set_var("RZ_LANG_DIR", temp_root.path().join("global"));
        }

        assert!(install_from_repo_dir("日语", repo, false).is_err());
        install_from_repo_dir("俄语", repo, false).unwrap();
        assert!(global_lang_dir().join("俄语/keywords.toml").is_file());
        assert!(install_from_repo_dir("俄语", repo, false).is_err());
        install_from_repo_dir("俄语", repo, true).unwrap();
        assert!(global_lang_dir().join("俄语/keywords.toml").is_file());

        unsafe {
            std::env::remove_var("RZ_LANG_DIR");
        }
    }

    #[test]
    fn test_extract_zip_and_find() {
        use std::io::Write;
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_root = tempfile::tempdir().unwrap();

        let zip_path = temp_root.path().join("lang_pack.zip");
        let file = fs::File::create(&zip_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer
            .start_file("language-packs-main/中文/keywords.toml", options)
            .unwrap();
        writer
            .write_all("[\"声明\"]\n\"函数\" = \"fn\"\n".as_bytes())
            .unwrap();
        writer
            .start_file("language-packs-main/中文/errors.toml", options)
            .unwrap();
        writer
            .write_all("[示例]\n\"a\" = \"b\"\n".as_bytes())
            .unwrap();
        writer.finish().unwrap();

        let extract_dir = temp_root.path().join("extracted");
        extract_zip(&zip_path, &extract_dir).unwrap();
        assert!(
            extract_dir
                .join("language-packs-main/中文/keywords.toml")
                .is_file()
        );

        let zh = find_lang_in_extracted(&extract_dir, "中文").expect("应找到中文");
        assert!(zh.join("keywords.toml").is_file());
        assert!(find_lang_in_extracted(&extract_dir, "俄语").is_none());
    }

    #[test]
    fn test_validate_lang_code() {
        assert!(validate_lang_code("中文"));
        assert!(validate_lang_code("en-US"));
        assert!(!validate_lang_code(""));
        assert!(!validate_lang_code("../x"));
        assert!(!validate_lang_code("a/b"));
        assert!(!validate_lang_code(".."));
        assert!(!validate_lang_code("."));
    }

    #[test]
    fn test_parse_lang_info() {
        let metadata = parse_lang_info(
            "[\"语言包\"]\n\"名称\" = \"俄语\"\n\"扩展名\" = \"ru\"\n\"版本\" = \"1.0\"\n",
        )
        .unwrap();
        assert_eq!(metadata.name, "俄语");
        assert_eq!(metadata.extension, "ru");
        assert_eq!(metadata.version, "1.0");

        let metadata = parse_lang_info("[语言包]\n名称 = \"日语\"\n扩展名 = \"ja\"\n").unwrap();
        assert_eq!(metadata.name, "日语");
        assert_eq!(metadata.extension, "ja");
        assert_eq!(metadata.version, "1.0");

        assert!(parse_lang_info("[语言包]\n名称 = \"俄语\"\n").is_none());
        assert!(parse_lang_info("[其他]\n名称 = \"俄语\"\n扩展名 = \"ru\"\n").is_none());
        assert!(parse_lang_info("这不是 TOML").is_none());
    }

    #[test]
    fn test_dynamic_extension_map() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_root = tempfile::tempdir().unwrap();
        make_temp_lang_pack(temp_root.path(), "日语");
        unsafe {
            std::env::set_var("RZ_LANG_DIR", temp_root.path().join("global"));
        }
        install_lang(temp_root.path().join("日语").to_str().unwrap(), false).unwrap();

        assert_eq!(query_extension_map("ja").as_deref(), Some("日语"));
        let list = list_langs();
        let japanese = list
            .iter()
            .find(|info| info.lang_code == "日语")
            .expect("日语应被列出");
        assert_eq!(japanese.extension.as_deref(), Some("ja"));
        assert!(japanese.version.is_none());

        assert_eq!(query_extension_map("zh").as_deref(), Some("zh"));
        let chinese = list
            .iter()
            .find(|info| info.lang_code == "zh")
            .expect("zh应被列出");
        assert_eq!(chinese.extension.as_deref(), Some("zh"));
        assert_eq!(chinese.version.as_deref(), Some("1.0"));

        // 补写 lang_info.toml
        fs::write(
            temp_root.path().join("global/日语/lang_info.toml"),
            "[语言包]\n名称 = \"日语\"\n扩展名 = \"ja\"\n版本 = \"2.1\"\n",
        )
        .unwrap();
        assert_eq!(query_extension_map("ja").as_deref(), Some("日语"));
        let list = list_langs();
        let japanese = list
            .iter()
            .find(|info| info.lang_code == "日语")
            .expect("日语应被列出");
        assert_eq!(japanese.version.as_deref(), Some("2.1"));

        assert_eq!(
            static_extension_map().get("ru").map(String::as_str),
            Some("俄语")
        );

        unsafe {
            std::env::remove_var("RZ_LANG_DIR");
        }
    }

    #[test]
    fn test_default_repo_sources_and_zip_urls() {
        assert_eq!(DEFAULT_REPO_SOURCES.len(), 2);
        assert!(DEFAULT_REPO_SOURCES[0].starts_with("https://gitcode.com/"));
        assert!(DEFAULT_REPO_SOURCES[1].starts_with("https://github.com/"));

        let gitcode = RepoSource::from_url("https://gitcode.com/用户名/zrRust");
        assert_eq!(gitcode.git_url, "https://gitcode.com/用户名/zrRust");
        assert_eq!(
            gitcode.zip_url,
            "https://gitcode.com/用户名/zrRust/repository/archive/master.zip"
        );

        let github = RepoSource::from_url("https://github.com/用户名/zrRust");
        assert_eq!(
            github.zip_url,
            "https://github.com/用户名/zrRust/archive/refs/heads/main.zip"
        );

        let local = RepoSource::from_url("http://127.0.0.1:18080/语言包");
        assert_eq!(
            local.zip_url,
            "http://127.0.0.1:18080/语言包/archive/refs/heads/main.zip"
        );
        assert_eq!(
            RepoSource::from_url("https://github.com/用户名/zrRust/").git_url,
            "https://github.com/用户名/zrRust"
        );
    }

    #[test]
    fn test_rz_lang_repo_env_priority() {
        let _lock = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("RZ_LANG_REPO", "http://127.0.0.1:18080/自定义仓库/");
            let sources = collect_sources();
            assert_eq!(sources.len(), 1);
            assert_eq!(sources[0].git_url, "http://127.0.0.1:18080/自定义仓库");
            std::env::remove_var("RZ_LANG_REPO");
        }
        let default = collect_sources();
        assert_eq!(default.len(), 2);
        assert_eq!(default[0].git_url, DEFAULT_REPO_SOURCES[0]);
        assert_eq!(default[1].git_url, DEFAULT_REPO_SOURCES[1]);
    }

    #[test]
    fn test_multi_source_fallback_install() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_root = tempfile::tempdir().unwrap();
        let backup_repo = temp_root.path().join("lang_repo");
        fs::create_dir_all(&backup_repo).unwrap();
        make_temp_lang_pack(&backup_repo, "中文");
        for args in [
            &["init", "-b", "main"][..],
            &["add", "."][..],
            &["commit", "-m", "init"][..],
        ] {
            let result = Command::new("git")
                .args(args)
                .current_dir(&backup_repo)
                .env("GIT_AUTHOR_NAME", "测试")
                .env("GIT_AUTHOR_EMAIL", "test@example.com")
                .env("GIT_COMMITTER_NAME", "测试")
                .env("GIT_COMMITTER_EMAIL", "test@example.com")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output();
            let status = result.expect("执行 git 失败").status;
            assert!(status.success(), "git 子命令 {:?} 失败", args[0]);
        }

        let primary = RepoSource::from_url("file:///不存在的仓库路径");
        let backup = RepoSource::from_url(backup_repo.to_str().unwrap());
        unsafe {
            std::env::set_var("RZ_LANG_DIR", temp_root.path().join("global"));
        }

        let temp = TempDir::new().unwrap();
        let result = try_all_sources("中文", &[primary, backup], &temp, false);
        assert!(
            result.is_ok(),
            "首选源失败后应自动回退备用源：{}",
            result.err().map(|e| e.to_string()).unwrap_or_default()
        );
        assert!(global_lang_dir().join("中文/keywords.toml").is_file());

        unsafe {
            std::env::remove_var("RZ_LANG_DIR");
        }
    }

    #[test]
    fn test_all_sources_failed_error_message() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp_root = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("RZ_LANG_DIR", temp_root.path().join("global"));
            // 固定界面语言为中文，保证错误消息断言稳定（系统语言无关）
            std::env::set_var("RZ_LANG", "zh");
        }
        let bad1 = RepoSource::from_url("file:///不存在的仓库1");
        let bad2 = RepoSource::from_url("file:///不存在的仓库2");
        let temp = TempDir::new().unwrap();
        let error = try_all_sources("中文", &[bad1, bad2], &temp, false)
            .expect_err("两个源都失败应报错")
            .to_string();
        assert!(
            error.contains("已依次尝试 2 个源"),
            "应报告尝试的源数量：{}",
            error
        );
        assert!(
            error.contains("file:///不存在的仓库1"),
            "应列出源1：{}",
            error
        );
        assert!(
            error.contains("file:///不存在的仓库2"),
            "应列出源2：{}",
            error
        );
        assert!(
            error.contains("RZ_LANG_REPO"),
            "应建议 RZ_LANG_REPO：{}",
            error
        );
        unsafe {
            std::env::remove_var("RZ_LANG_DIR");
            std::env::remove_var("RZ_LANG");
        }
    }
}
