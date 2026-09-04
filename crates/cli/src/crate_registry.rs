// 第三方库共享注册中心模块
//
// 实现 `rzc crate` 子命令：search / install / list / remove / update / publish。
// 注册中心即一个 Git 仓库（GitCode/GitHub 或任意 git 可达地址 / 本地路径），
// 根含 index.json 索引，映射按 <语言>/crates/<crate>.toml 存放
// （与语言包内 crates/ 子目录结构一致）。
//
// 远程获取复用 lang_manager 的 RepoSource / TempDir / fetch_repo_zip 能力；
// 本地路径注册中心直接复用目录，无需网络。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::lang_manager::{RepoSource, TempDir};

/// 默认注册中心地址（可用 RZ_CRATE_REPO 环境变量覆盖）。
/// 注册中心已合并进 zrRust 仓库，位于 `third-party/` 子目录；
/// 故默认即指向主仓库，克隆后从 `third-party/index.json` 读取。
const DEFAULT_CRATE_REPO: &str = "https://gitcode.com/tan80/zrRust";

/// 单条映射在注册中心的索引条目
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MappingEntry {
    /// crate 名（连字符归一为下划线）
    pub crate_name: String,
    /// 语言代码（zh/ru/ja/…）
    pub lang: String,
    /// 映射版本（语义自定，默认 1.0）
    pub version: String,
    /// 译者署名
    pub author: String,
    /// 映射文件许可证（默认 MIT）
    pub license: String,
    /// 质量分（0–1，发布校验通过即 1.0）
    pub quality: f64,
    /// 下载计数
    pub downloads: u64,
    /// 映射文件在仓库内的相对路径
    pub file: String,
    /// 最后更新日期（YYYY-MM-DD）
    pub updated: String,
}

/// 注册中心 index.json 结构
#[derive(Serialize, Deserialize, Default)]
pub struct RegistryIndex {
    pub registry: String,
    pub updated: String,
    pub mappings: Vec<MappingEntry>,
}

/// 注册中心仓库地址：RZ_CRATE_REPO 优先，否则默认地址。
///
/// 默认情况下，若当前工作目录是 zrRust 源码树（存在 `third-party/index.json`），
/// 则直接使用该本地子目录，离线即可 search/install；否则回退到远端主仓库。
fn registry_repo_url() -> String {
    if let Ok(v) = std::env::var("RZ_CRATE_REPO") {
        let v = v.trim().trim_end_matches('/').to_string();
        if !v.is_empty() {
            return v;
        }
    }
    if Path::new("third-party").join("index.json").is_file() {
        return "third-party".to_string();
    }
    DEFAULT_CRATE_REPO.to_string()
}

/// 构造注册中心源（与 lang install 同源回退策略一致）
fn collect_sources() -> Vec<RepoSource> {
    vec![RepoSource::from_url(&registry_repo_url())]
}

/// 注册中心在仓库内的实际目录。合并进 zrRust 后位于 `third-party/` 子目录，
/// 独立仓库则直接在根；两种布局都兼容。
fn registry_subdir(repo_root: &Path) -> PathBuf {
    let nested = repo_root.join("third-party");
    if nested.join("index.json").is_file() {
        nested
    } else {
        repo_root.to_path_buf()
    }
}

/// 拉取注册中心仓库到可访问的本地路径
///
/// - 本地路径注册中心：直接复用，无需下载；
/// - 远程地址：优先 `git clone --depth 1`，失败回退 `curl` 下载 ZIP。
/// 返回临时句柄（持有生命周期）与仓库根路径。
fn fetch_registry_repo() -> anyhow::Result<(TempDir, PathBuf)> {
    let url = registry_repo_url();
    // 本地路径注册中心：直接复用（返回的临时句柄为占位，不影响真实目录）
    if Path::new(&url).is_dir() {
        let temp = TempDir::new()?;
        return Ok((temp, PathBuf::from(&url)));
    }
    let temp = TempDir::new()?;
    let source = &collect_sources()[0];
    let clone_dir = temp.path().join("repo");
    let git_ok = Command::new("git")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(&source.git_url)
        .arg(&clone_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if git_ok && registry_subdir(&clone_dir).join("index.json").is_file() {
        return Ok((temp, clone_dir));
    }
    // 回退 curl ZIP（fetch_repo_zip 已处理单层根目录下沉）
    let repo_dir = crate::lang_manager::fetch_repo_zip(source, &temp)?;
    if registry_subdir(&repo_dir).join("index.json").is_file() {
        return Ok((temp, repo_dir));
    }
    anyhow::bail!(
        "注册中心索引 index.json 未找到（仓库：{}）",
        registry_repo_url()
    )
}

/// 读取 index.json（缺失时返回空索引）
fn read_index(path: &Path) -> anyhow::Result<RegistryIndex> {
    if !path.is_file() {
        return Ok(RegistryIndex::default());
    }
    let content = fs::read_to_string(path)?;
    let index: RegistryIndex = serde_json::from_str(&content)
        .map_err(|e| anyhow::anyhow!("{}", crate::ui::Ui::global().f("crate_index_parse_failed", &[&e.to_string()])))?;
    Ok(index)
}

/// 写入 index.json（美化格式）
fn write_index(path: &Path, index: &RegistryIndex) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(index)?)?;
    Ok(())
}

/// 已安装清单路径：~/.rz/crate-registry.json
fn installed_manifest_path() -> PathBuf {
    crate::lang_manager::global_lang_dir()
        .parent()
        .map(|p| p.join("crate-registry.json"))
        .unwrap_or_else(|| PathBuf::from("crate-registry.json"))
}

/// 读取已安装清单（(crate, lang) → 条目）
fn read_installed() -> BTreeMap<(String, String), MappingEntry> {
    let path = installed_manifest_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(list) = serde_json::from_str::<Vec<MappingEntry>>(&content) {
            return list
                .into_iter()
                .map(|e| ((e.crate_name.clone(), e.lang.clone()), e))
                .collect();
        }
    }
    BTreeMap::new()
}

/// 写入已安装清单
fn write_installed(map: &BTreeMap<(String, String), MappingEntry>) -> anyhow::Result<()> {
    let path = installed_manifest_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let list: Vec<MappingEntry> = map.values().cloned().collect();
    fs::write(path, serde_json::to_string_pretty(&list)?)?;
    Ok(())
}

/// `rzc crate search [关键词]`：检索注册中心索引
pub fn search(keyword: Option<&str>) -> anyhow::Result<()> {
    let ui = crate::ui::Ui::global();
    let (_temp, root) = fetch_registry_repo()?;
    let reg = registry_subdir(&root);
    let index = read_index(&reg.join("index.json"))?;
    let kw = keyword.map(|s| s.to_lowercase()).unwrap_or_default();
    let mut matches: Vec<&MappingEntry> = index
        .mappings
        .iter()
        .filter(|m| {
            kw.is_empty()
                || m.crate_name.to_lowercase().contains(&kw)
                || m.lang.to_lowercase().contains(&kw)
                || m.author.to_lowercase().contains(&kw)
        })
        .collect();
    matches.sort_by(|a, b| {
        b.downloads
            .cmp(&a.downloads)
            .then_with(|| a.crate_name.cmp(&b.crate_name))
    });
    if matches.is_empty() {
        println!("{}", ui.t("crate_search_empty"));
        return Ok(());
    }
    for m in &matches {
        println!(
            "  {:<18} {:<6} v{:<6} 下载 {:>5}  {}",
            m.crate_name, m.lang, m.version, m.downloads, m.author
        );
    }
    println!();
    println!("{}", ui.t("crate_search_hint"));
    Ok(())
}

/// `rzc crate install <crate> --lang <语言> [--force]`：拉取单个映射
pub fn install(crate_name: &str, lang: &str, force: bool) -> anyhow::Result<()> {
    let ui = crate::ui::Ui::global();
    let (_temp, root) = fetch_registry_repo()?;
    let reg = registry_subdir(&root);
    let index = read_index(&reg.join("index.json"))?;
    let entry = index
        .mappings
        .iter()
        .find(|m| m.crate_name == crate_name && m.lang == lang)
        .ok_or_else(|| anyhow::anyhow!("{}", ui.f("crate_not_found", &[crate_name, lang])))?;
    let src = reg.join(&entry.file);
    if !src.is_file() {
        anyhow::bail!("{}", ui.f("crate_file_missing", &[&entry.file]));
    }
    let dest_dir = crate::lang_manager::global_lang_dir().join(lang).join("crates");
    fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join(format!("{}.toml", crate_name));
    if dest.exists() && !force {
        anyhow::bail!("{}", ui.f("crate_already_installed", &[crate_name, lang]));
    }
    fs::copy(&src, &dest)?;
    let mut installed = read_installed();
    installed.insert((crate_name.to_string(), lang.to_string()), entry.clone());
    write_installed(&installed)?;
    println!(
        "{}",
        ui.f("crate_installed", &[crate_name, lang, &dest.display().to_string()])
    );
    Ok(())
}

/// `rzc crate list`：列出已安装的社区映射
pub fn list() -> anyhow::Result<()> {
    let ui = crate::ui::Ui::global();
    let installed = read_installed();
    if installed.is_empty() {
        println!("{}", ui.t("crate_list_empty"));
        return Ok(());
    }
    println!("{}", ui.f("crate_list_header", &[&installed.len().to_string()]));
    for ((crate_name, lang), entry) in &installed {
        println!("  {:<18} {:<6} v{}  {}", crate_name, lang, entry.version, entry.author);
    }
    Ok(())
}

/// `rzc crate remove <crate> --lang <语言>`：删除已安装映射
pub fn remove(crate_name: &str, lang: &str) -> anyhow::Result<()> {
    let ui = crate::ui::Ui::global();
    let dest = crate::lang_manager::global_lang_dir()
        .join(lang)
        .join("crates")
        .join(format!("{}.toml", crate_name));
    if dest.exists() {
        fs::remove_file(&dest)?;
    }
    let mut installed = read_installed();
    if installed
        .remove(&(crate_name.to_string(), lang.to_string()))
        .is_some()
    {
        write_installed(&installed)?;
    }
    println!("{}", ui.f("crate_removed", &[crate_name, lang]));
    Ok(())
}

/// `rzc crate update`：按清单重新拉取所有已安装映射（获取他人更新）
pub fn update() -> anyhow::Result<()> {
    let ui = crate::ui::Ui::global();
    let installed = read_installed();
    if installed.is_empty() {
        println!("{}", ui.t("crate_list_empty"));
        return Ok(());
    }
    let (_temp, root) = fetch_registry_repo()?;
    let reg = registry_subdir(&root);
    let index = read_index(&reg.join("index.json"))?;
    let mut count = 0usize;
    for ((crate_name, lang), _) in &installed {
        if let Some(entry) = index
            .mappings
            .iter()
            .find(|m| &m.crate_name == crate_name && &m.lang == lang)
        {
            let src = reg.join(&entry.file);
            if src.is_file() {
                let dest_dir = crate::lang_manager::global_lang_dir().join(lang).join("crates");
                fs::create_dir_all(&dest_dir)?;
                fs::copy(&src, dest_dir.join(format!("{}.toml", crate_name)))?;
                count += 1;
            }
        }
    }
    println!(
        "{}",
        ui.f("crate_updated", &[&count.to_string(), &installed.len().to_string()])
    );
    Ok(())
}

/// `rzc crate publish <crate> --lang <语言> [--file <路径>] [--author <名>]`：上传映射
pub fn publish(
    crate_name: &str,
    lang: &str,
    file: Option<PathBuf>,
    author: Option<&str>,
) -> anyhow::Result<()> {
    let ui = crate::ui::Ui::global();
    // 1. 定位待发布的映射文件
    let src = resolve_publish_file(crate_name, lang, file)?;
    // 2. 质量门禁：TOML 可解析 + 节名合法（重复键 TOML 解析即失败）
    let content = fs::read_to_string(&src)?;
    validate_mapping_toml(&content)?;
    // 3. 写入注册中心（本地路径直写；远程地址克隆后提交推送）
    let repo_url = registry_repo_url();
    let author = author
        .map(String::from)
        .or_else(git_user_name)
        .unwrap_or_else(|| "anonymous".to_string());
    if Path::new(&repo_url).is_dir() {
        write_to_local_registry(Path::new(&repo_url), crate_name, lang, &content, &author)?;
    } else {
        write_to_remote_registry(&repo_url, crate_name, lang, &content, &author)?;
    }
    println!("{}", ui.f("crate_published", &[crate_name, lang]));
    Ok(())
}

/// 解析待发布文件：--file 优先，否则按常见位置查找
fn resolve_publish_file(
    crate_name: &str,
    lang: &str,
    file: Option<PathBuf>,
) -> anyhow::Result<PathBuf> {
    let ui = crate::ui::Ui::global();
    if let Some(f) = file {
        if f.is_file() {
            return Ok(f);
        }
        anyhow::bail!("{}", ui.f("crate_publish_file_missing", &[&f.display().to_string()]));
    }
    let candidates = [
        crate::lang_manager::global_lang_dir()
            .join(lang)
            .join("crates")
            .join(format!("{}.toml", crate_name)),
        crate::lang_pack_root_of(Path::new("."))
            .join(lang)
            .join("crates")
            .join(format!("{}.toml", crate_name)),
        PathBuf::from(format!("{}.toml", crate_name)),
        PathBuf::from(lang)
            .join("crates")
            .join(format!("{}.toml", crate_name)),
    ];
    for c in &candidates {
        if c.is_file() {
            return Ok(c.clone());
        }
    }
    anyhow::bail!("{}", ui.f("crate_publish_not_found", &[crate_name, lang]));
}

/// 质量门禁：映射文件必须是合法 TOML 表，且仅含允许的节
fn validate_mapping_toml(content: &str) -> anyhow::Result<()> {
    let ui = crate::ui::Ui::global();
    let value: toml::Value = toml::from_str(content)
        .map_err(|e| anyhow::anyhow!("{}", ui.f("crate_publish_invalid_toml", &[&e.to_string()])))?;
    let table = value
        .as_table()
        .ok_or_else(|| anyhow::anyhow!("{}", ui.t("crate_publish_not_table")))?;
    let allowed = ["模块路径", "标识符", "解释"];
    for key in table.keys() {
        if !allowed.contains(&key.as_str()) {
            anyhow::bail!("{}", ui.f("crate_publish_bad_section", &[key]));
        }
    }
    Ok(())
}

/// 取 git user.name（发布署名用）
fn git_user_name() -> Option<String> {
    Command::new("git")
        .arg("config")
        .arg("user.name")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 写入本地路径注册中心并提交（best effort）
fn write_to_local_registry(
    repo: &Path,
    crate_name: &str,
    lang: &str,
    content: &str,
    author: &str,
) -> anyhow::Result<()> {
    let ui = crate::ui::Ui::global();
    let reg = registry_subdir(repo);
    let dest = reg
        .join(lang)
        .join("crates")
        .join(format!("{}.toml", crate_name));
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&dest, content)?;
    let index_path = reg.join("index.json");
    let mut index = read_index(&index_path)?;
    upsert_entry(&mut index, crate_name, lang, author);
    write_index(&index_path, &index)?;
    git_commit(repo, &format!("publish {} ({})", crate_name, lang));
    println!(
        "{}",
        ui.f("crate_publish_wrote_local", &[&repo.display().to_string()])
    );
    Ok(())
}

/// 写入远程注册中心：克隆 → 写文件+索引 → 提交 → 推送
fn write_to_remote_registry(
    url: &str,
    crate_name: &str,
    lang: &str,
    content: &str,
    author: &str,
) -> anyhow::Result<()> {
    let ui = crate::ui::Ui::global();
    let temp = TempDir::new()?;
    let clone_dir = temp.path().join("repo");
    let cloned = Command::new("git")
        .arg("clone")
        .arg(url)
        .arg(&clone_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !cloned {
        anyhow::bail!("{}", ui.f("crate_publish_clone_failed", &[url]));
    }
    let reg = registry_subdir(&clone_dir);
    let dest = reg
        .join(lang)
        .join("crates")
        .join(format!("{}.toml", crate_name));
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&dest, content)?;
    let index_path = reg.join("index.json");
    let mut index = read_index(&index_path)?;
    upsert_entry(&mut index, crate_name, lang, author);
    write_index(&index_path, &index)?;
    git_commit(&clone_dir, &format!("publish {} ({})", crate_name, lang));
    let push = Command::new("git")
        .arg("push")
        .current_dir(&clone_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output();
    match push {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!("{}", ui.f("crate_publish_push_failed", &[err.trim()]));
        }
        Err(e) => anyhow::bail!("{}", ui.f("crate_publish_push_failed", &[&e.to_string()])),
    }
    Ok(())
}

/// 在索引中插入或更新一条映射条目
fn upsert_entry(index: &mut RegistryIndex, crate_name: &str, lang: &str, author: &str) {
    let today = today_utc();
    let file = format!("{}/crates/{}.toml", lang, crate_name);
    if let Some(existing) = index
        .mappings
        .iter_mut()
        .find(|m| m.crate_name == crate_name && m.lang == lang)
    {
        existing.version = bump_version(&existing.version);
        existing.author = author.to_string();
        existing.updated = today.clone();
        existing.file = file;
    } else {
        index.mappings.push(MappingEntry {
            crate_name: crate_name.to_string(),
            lang: lang.to_string(),
            version: "1.0".to_string(),
            author: author.to_string(),
            license: "MIT".to_string(),
            quality: 1.0,
            downloads: 0,
            file,
            updated: today.clone(),
        });
    }
    index.updated = today.clone();
    index
        .mappings
        .sort_by(|a, b| a.crate_name.cmp(&b.crate_name).then_with(|| a.lang.cmp(&b.lang)));
}

/// 版本号末段 +1（1.0 → 1.1；无法解析则回退 1.0）
fn bump_version(v: &str) -> String {
    if let Some((major, minor)) = v.split_once('.') {
        if let Ok(m) = minor.parse::<u32>() {
            return format!("{}.{}", major, m + 1);
        }
    }
    "1.0".to_string()
}

/// 提交改动（best effort，失败不影响文件写入结果）
fn git_commit(repo: &Path, message: &str) {
    let _ = Command::new("git")
        .arg("add")
        .arg("-A")
        .current_dir(repo)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let _ = Command::new("git")
        .arg("commit")
        .arg("-m")
        .arg(message)
        .current_dir(repo)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// 当前 UTC 日期（YYYY-MM-DD），无外部时间依赖
fn today_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut days = (secs / 86_400) as i64;
    let mut y = 1970i64;
    loop {
        let leap = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
            366
        } else {
            365
        };
        if days < leap {
            break;
        }
        days -= leap;
        y += 1;
    }
    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let mut m = 0usize;
    let mut rem = days;
    loop {
        let md = month_days[m] + if m == 1 && leap { 1 } else { 0 };
        if rem < md {
            break;
        }
        rem -= md;
        m += 1;
    }
    format!("{:04}-{:02}-{:02}", y, m + 1, rem + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_today_utc_format() {
        let s = today_utc();
        assert_eq!(s.len(), 10);
        assert!(s.starts_with("20")); // 21 世纪
        assert_eq!(s.as_bytes()[4], b'-');
        assert_eq!(s.as_bytes()[7], b'-');
    }

    #[test]
    fn test_bump_version() {
        assert_eq!(bump_version("1.0"), "1.1");
        assert_eq!(bump_version("2.3"), "2.4");
        assert_eq!(bump_version("garbage"), "1.0");
    }

    #[test]
    fn test_validate_mapping_toml() {
        // 合法：仅含允许的节
        assert!(validate_mapping_toml("[\"标识符\"]\n\"服务器\" = \"Server\"\n").is_ok());
        // 非法节名
        assert!(validate_mapping_toml("[\"未知节\"]\n\"a\" = \"b\"\n").is_err());
        // 重复键：TOML 解析失败
        assert!(validate_mapping_toml("[\"标识符\"]\n\"a\" = \"b\"\n\"a\" = \"c\"\n").is_err());
    }

    #[test]
    fn test_index_roundtrip() {
        let mut index = RegistryIndex::default();
        upsert_entry(&mut index, "serde", "zh", "tan80");
        upsert_entry(&mut index, "tokio", "zh", "alice");
        assert_eq!(index.mappings.len(), 2);
        // 重复 upsert 应更新版本而非新增
        upsert_entry(&mut index, "serde", "zh", "bob");
        assert_eq!(index.mappings.len(), 2);
        let serde = index
            .mappings
            .iter()
            .find(|m| m.crate_name == "serde" && m.lang == "zh")
            .unwrap();
        assert_eq!(serde.version, "1.1");
        assert_eq!(serde.author, "bob");
        // 序列化再反序列化
        let json = serde_json::to_string_pretty(&index).unwrap();
        let back: RegistryIndex = serde_json::from_str(&json).unwrap();
        assert_eq!(back.mappings.len(), 2);
    }
}
