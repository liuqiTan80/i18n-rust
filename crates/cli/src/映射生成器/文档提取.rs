//! 工具链集成：在临时项目中把目标 crate 作为依赖编译，手动调用 rustdoc
//! 生成 JSON 文档（含薄壳 crate 的 glob 重导出追踪、构建脚本产物与
//! CARGO_PKG_* 标准环境变量注入）。
//!
//! 本文件是源真相：由引导 rzc 转译为 文档提取.rs 后交 cargo 编译。
//! 再生成方式见 tools/zh-selfhost/regen.sh；请勿手改同目录下的 文档提取.rs。
//!
//! 注：外部 std/三方 ABI 走英文透传——
//! crate 根 `crate::{解析cargo路径, 解析rustc路径, 解析rustdoc路径}`（`main.zh` 已翻转）、`anyhow`/`serde_json`/`toml`/`std`/
//! `tempfile` 及其方法链（get/and_then/as_str/insert/contains/join/collect/map_or/split_once 等）
//! 与 toml/serde 枚举变体（Array/String）；`crate::本地化::界面::全局/取文/取文带参`、
//! `crate::临时守护::安全临时路径/安全用户段`、
//! `crate::语言包工具::单元测试::{环境锁, 环境恢复::接管}`、
//! `超级::{接口条目, 接口种类}`/其字段 `英文原名`/`类型签名`（mod.rs 已翻转）、
//! `接口解析::提取公开接口/提取公开接口含重导出/样例文本` 为已翻转的中文 ABI。
//! 内部类型、函数、局部变量、控制流与测试均已中文化；所有数据串（TOML 夹具、内嵌 lib.rs 源码、
//! CARGO_* 变量名、rustdoc/git 参数、ui key、错误文案）逐字保真。

use anyhow::{Context, anyhow, bail};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::接口解析::提取公开接口含重导出;

/// rustdoc JSON 文本列表：(crate 名, JSON 文档文本)
type 文档列表 = Vec<(String, String)>;

/// 提取结果：(JSON 文本列表, 目标 crate 实际解析版本)
type 库文档提取 = (文档列表, Option<String>);

/// 临时项目目录守卫（Drop 时自动清理）
struct 临时项目守卫(PathBuf);

impl 临时项目守卫 {
    /// 创建临时项目目录
    fn 新建(库名: &str) -> anyhow::Result<Self> {
        // 用户隔离 + 符号链接校验（crate 名已由 运行自动生成 校验为
        // ASCII 标识符字符；仍拼接用户/PID 段保证路径不可预测）
        let 项目路径 = crate::临时守护::安全临时路径(&format!(
            "rzc-mapping-{}-{}-{}",
            库名,
            crate::临时守护::安全用户段(),
            std::process::id()
        ))?;
        let _ = fs::remove_dir_all(&项目路径);
        fs::create_dir_all(项目路径.join("src")).map_err(|错| {
            anyhow::anyhow!(
                "{}",
                crate::本地化::界面::全局().取文带参("mg_err_tempdir", &[&错.to_string()])
            )
        })?;
        Ok(临时项目守卫(项目路径))
    }

    /// 获取临时项目路径
    fn 获取路径(&self) -> &Path {
        &self.0
    }
}

impl Drop for 临时项目守卫 {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 提取 crate 及其 glob 重导出链上依赖 crate 的 rustdoc JSON 文本列表
///
/// 首个元素为目标 crate 本身；后续元素为被 glob 重导出（`pub use 依赖::*`）
/// 的依赖 crate——薄壳 crate（如 salvo）的公开 API 全部来自这些 crate。
/// 第二个返回值为目标 crate 的实际解析版本（来自 cargo metadata），
/// 供映射文件头记录生成基准。
///
/// - `版本需求`：Cargo 版本需求（`=x.y.z` 精确锁定 / `x.y.*` 前缀）；
///   `None` 时用 `*`（解析到当时最新版，结果不可复现，调用方已提示）
pub fn 提取库文档(
    库名: &str, 版本需求: Option<&str>
) -> anyhow::Result<库文档提取> {
    let 守卫 = 临时项目守卫::新建(库名)?;
    // 1. 临时项目：把目标 crate 作为唯一依赖；版本需求由 --target-version 转换
    //    （未指定时 * 允许任意已发布版本，生成基准不可复现）
    fs::write(
        守卫.获取路径().join("Cargo.toml"),
        format!(
            "[package]\nname = \"rzc-mapping-temp\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n\"{}\" = \"{}\"\n\n[workspace]\n",
            库名,
            版本需求.unwrap_or("*")
        ),
    )?;
    fs::write(
        守卫.获取路径().join("src/lib.rs"),
        "// 仅供提取依赖 API 的空库\n",
    )?;
    内部提取库文档(&守卫, 库名)
}

/// 工具链核心（对测试开放）：在给定临时项目中定位 crate 并生成其 rustdoc JSON
///
/// 1. `cargo metadata` 定位目标 crate 的源码目录（registry 或本地 path 依赖均可）
/// 2. `cargo build` 编译依赖树，解析每个依赖的 .rlib/.so 路径
/// 3. 手动调用 `rustdoc -Z unstable-options --output-format json` 文档化目标 crate
/// 4. 薄壳 crate（meta crate，如 salvo 仅 `pub use salvo_core::*`）的公开 API
///    来自 glob 重导出：解析每个 crate 的 glob 重导出（inner.use.is_glob）的
///    source 路径，对被重导出的依赖 crate 也生成文档并继续追踪，直至闭环。
///    返回 ((crate 名, JSON 文本) 列表（首个为目标 crate 本身）, 实际解析版本)。
fn 内部提取库文档(
    守卫: &临时项目守卫, 库名: &str
) -> anyhow::Result<库文档提取> {
    let 项目根 = 守卫.获取路径();
    let 界面 = crate::本地化::界面::全局();

    // 1. metadata：定位目标 crate 的 manifest
    let 元数据输出 = 运行命令(
        Command::new(crate::解析cargo路径())
            .arg("metadata")
            .arg("--format-version")
            .arg("1")
            .current_dir(项目根),
        &界面.取文("mg_cmd_parse_deps"),
    )?;
    let 元数据: Value = serde_json::from_str(&元数据输出)
        .map_err(|错| anyhow!("{}", 界面.取文带参("mg_err_parse_meta", &[&错.to_string()])))?;
    // 包元数据索引（manifest_path / features / edition），供 glob 重导出链上的
    // 依赖 crate 查询（薄壳 crate 的 API 在其依赖中）。同时索引 package 名
    // （CLI 参数，如 mini-core）与 lib target 的 crate 名（rustdoc glob 重导出
    // source，如 mini_core），两者在连字符/下划线写法上可能不同。
    let mut 包索引: HashMap<String, Value> = HashMap::new();
    for 包项 in 元数据["packages"].as_array().into_iter().flatten() {
        if let Some(包名) = 包项.get("name").and_then(Value::as_str) {
            包索引.insert(包名.to_string(), 包项.clone());
        }
        if let Some(目标库名) =
            包项
                .get("targets")
                .and_then(Value::as_array)
                .and_then(|目标列表| {
                    目标列表.iter().find(|目标| {
                        目标.get("kind").and_then(Value::as_array).map(|种类| {
                            种类.iter().any(|值内容项| 值内容项.as_str() == Some("lib"))
                        }) == Some(true)
                    })
                })
                .and_then(|目标| 目标.get("name"))
                .and_then(Value::as_str)
        {
            包索引.insert(目标库名.to_string(), 包项.clone());
        }
    }
    if !包索引.contains_key(库名) {
        bail!("{}", 界面.取文带参("mg_err_crate_not_found", &[库名]));
    }
    // 目标 crate 的实际解析版本（映射文件头「基准版本」来源；本地 path 依赖
    // 取 path 包声明版本，registry 依赖取本次解析命中版本）
    let 已解析版本 = 包索引
        .get(库名)
        .and_then(|包项| 包项.get("version"))
        .and_then(Value::as_str)
        .map(String::from);

    // resolve.nodes：完整依赖解析图（含多版本共存时的精确解析），按
    // package_id 索引每个 crate 的直接依赖（别名, 依赖 package_id）。
    // --extern 只传直接依赖的精确版本；传递依赖由 rustc 按 rlib 元数据
    // hash 在 -L 目录中自动解析（同名多版本 crate 无法从 -L 手动解析）
    let mut 直接依赖表: HashMap<String, Vec<(String, String)>> = HashMap::new();
    if let Some(节点列表) = 元数据
        .get("resolve")
        .and_then(|解析项| 解析项.get("nodes"))
        .and_then(Value::as_array)
    {
        for 节点 in 节点列表 {
            let Some(节点标识) = 节点.get("id").and_then(Value::as_str) else {
                continue;
            };
            let mut 依赖: Vec<(String, String)> = Vec::new();
            if let Some(依赖列表) = 节点.get("deps").and_then(Value::as_array) {
                for 依赖项 in 依赖列表 {
                    let Some(依赖名) = 依赖项.get("name").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some(依赖包) = 依赖项.get("pkg").and_then(Value::as_str) else {
                        continue;
                    };
                    // 只保留 normal 依赖（dev/build 依赖不进入 lib 的 extern prelude）；
                    // 目标特定依赖（cfg(...)）无法静态判定，保守保留
                    let 是普通依赖 = 依赖项
                        .get("dep_kinds")
                        .and_then(Value::as_array)
                        .map(|种类列表| {
                            种类列表.iter().any(|节点项| {
                                节点项.get("kind").is_none_or(|值内容项| 值内容项.is_null())
                            })
                        })
                        .unwrap_or(true);
                    if 是普通依赖 {
                        依赖.push((依赖名.to_string(), 依赖包.to_string()));
                    }
                }
            }
            直接依赖表.insert(节点标识.to_string(), 依赖);
        }
    }

    // 2. cargo build：编译依赖树，解析依赖 .rlib/.so 路径。
    //    统一工具链：显式指定 RUSTC 为解析到的同一编译器，保证 rlib 与后续
    //    手调 rustdoc（同 sysroot）版本一致，根治跨版本链接 E0514。
    let mut 构建进程 = Command::new(crate::解析cargo路径());
    构建进程
        .arg("build")
        .arg("--message-format=json")
        .env("RUSTC", crate::解析rustc路径())
        .current_dir(项目根);
    let 构建输出 = 运行命令(&mut 构建进程, &界面.取文("mg_cmd_build_deps"))?;
    // package_id -> (lib target 名, .rlib/.so 路径, 实际启用的 features)。
    // 按 package_id 索引而非 target 名：依赖树中同名不同版本的 crate 共存时
    // （如 rand 0.8.7 与 0.10.2），按名字覆盖会链接错误版本；features 取 cargo
    // build 的 feature 统一解析结果而非包声明的 default——依赖方可能未启用部分
    // default features（如 salvo 不启用 salvo_core 的 unix），注入不一致的 cfg
    // 会导致编译失败（缺 nix 等依赖）
    let mut 产物表: HashMap<String, (String, String, Vec<String>)> = HashMap::new();
    // 已编译 lib 的 crate 名集合（glob 重导出链入队检查用）
    let mut 已编译库名: HashSet<String> = HashSet::new();
    // package_id -> 构建脚本产物（见模块级 [`构建脚本产物`] 类型注释）
    let mut 构建脚本表: HashMap<String, 构建脚本产物> = HashMap::new();
    for 每行 in 构建输出.lines() {
        let Ok(消息) = serde_json::from_str::<Value>(每行) else {
            continue;
        };
        let Some(缘由) = 消息.get("reason").and_then(Value::as_str) else {
            continue;
        };
        if 缘由 != "compiler-artifact" && 缘由 != "build-script-executed" {
            continue;
        }
        let Some(包标识) = 消息.get("package_id").and_then(Value::as_str) else {
            continue;
        };
        if 缘由 == "build-script-executed" {
            let 输出目录 = 消息
                .get("out_dir")
                .and_then(Value::as_str)
                .map(String::from);
            let 条件配置 = 消息
                .get("cfgs")
                .and_then(Value::as_array)
                .map(|列表| {
                    列表
                        .iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            let 环境变量 = 消息
                .get("env")
                .and_then(Value::as_array)
                .map(|列表| {
                    列表
                        .iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            构建脚本表.insert(包标识.to_string(), (输出目录, 条件配置, 环境变量));
            continue;
        }
        // compiler-artifact：按 package_id 索引（与 metadata packages[].id 一致），
        // 多版本共存时精确关联到具体版本；target.name 仅作为 lib 名记录
        let Some(目标名) = 消息
            .get("target")
            .and_then(|目标| 目标.get("name"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        // 只保留 ASCII 合法标识符的 crate 名（本地 workspace 成员如中文名项目会被排除）
        if 目标名.is_empty()
            || 目标名 == "rzc-mapping-temp"
            || !目标名
                .chars()
                .all(|每字符| 每字符.is_ascii_alphanumeric() || 每字符 == '_')
        {
            continue;
        }
        if let Some(文件名列表) = 消息.get("filenames").and_then(Value::as_array)
            && let Some(产物文件) = 文件名列表
                .iter()
                .filter_map(Value::as_str)
                .find(|文件名项| 文件名项.ends_with(".rlib") || 文件名项.ends_with(".so"))
        {
            let 特征列表 = 消息
                .get("features")
                .and_then(Value::as_array)
                .map(|列表| {
                    列表
                        .iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            产物表.insert(
                包标识.to_string(),
                (目标名.to_string(), 产物文件.to_string(), 特征列表),
            );
            已编译库名.insert(目标名.to_string());
        }
    }
    if 产物表.is_empty() {
        bail!("{}", 界面.取文带参("mg_err_build_failed", &[库名]));
    }

    // 3. rustdoc 队列：目标 crate → glob 重导出链上的依赖 crate。
    //    薄壳 crate（如 salvo）的 index 只有 re-export 节点，公开 API 全部
    //    来自被重导出的依赖 crate（如 salvo_core），逐个生成文档后合并提取。
    let mut 队列: Vec<String> = vec![库名.to_string()];
    let mut 已访问: HashSet<String> = HashSet::new();
    let mut 产出列表: Vec<(String, String)> = Vec::new();
    while let Some(名字) = 队列.pop() {
        if !已访问.insert(名字.clone()) {
            continue;
        }
        let Some(包项) = 包索引.get(&名字) else {
            continue;
        };
        let 包标识 = 包项
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        // 直接依赖（别名, 精确版本 package_id）：--extern 只传这些
        let 直接依赖 = 直接依赖表.get(&包标识).cloned().unwrap_or_default();
        let 文档文本 = 单库文档化(
            项目根,
            包项,
            &名字,
            &包标识,
            &产物表,
            &直接依赖,
            &构建脚本表,
        )?;
        // glob 重导出（pub use 依赖::*）→ 被重导出 crate 名（source 首段），
        // 若在依赖树中则入队继续提取；feature 未启用而未编译的依赖自动跳过
        let (_, 重导出来源) = 提取公开接口含重导出(&文档文本)?;
        for 来源 in 重导出来源 {
            if 已编译库名.contains(&来源) && !已访问.contains(&来源) {
                队列.push(来源);
            }
        }
        产出列表.push((名字, 文档文本));
    }
    Ok((产出列表, 已解析版本))
}

/// 构建脚本产物：OUT_DIR（include! 生成代码）、cfg（条件编译）、rustc-env 列表。
/// 部分 crate（如 serde）的 build.rs 会生成 include! 的源码或声明 cfg，
/// 手动 rustdoc 时必须注入，否则编译失败（如 OUT_DIR 未定义）。
type 构建脚本产物 = (Option<String>, Vec<String>, Vec<String>);

/// cargo 编译 crate 时注入的标准 CARGO_* 环境变量：(键, 值) 列表
///
/// 手动 rustdoc 不经过 cargo，proc macro 在展开期读取这些变量（如 tauri 的
/// `#[command(root = "crate")]` 读 CARGO_PKG_NAME）会因缺失直接 panic；
/// 未声明的包字段与 cargo 一致地给出空字符串（rust-version / readme
/// 仅在清单中声明时才提供）。
fn 标准环境变量(
    包项: &Value,
    清单: &toml::Value,
    库名: &str,
    库名下划线: &str,
    源目录: &Path,
    清单路径: &Path,
) -> Vec<(String, String)> {
    let 包声明 = 清单.get("package");
    let 取元字段 = |键名: &str| -> String {
        包项
            .get(键名)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let 取清单字段 = |键名: &str| -> String {
        包声明
            .and_then(|表项| 表项.get(键名))
            .and_then(toml::Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let 名称 = {
        let 元名称 = 取元字段("name");
        if 元名称.is_empty() {
            库名.to_string()
        } else {
            元名称
        }
    };
    let 版本串 = 取元字段("version");
    // 版本分段（先剥离 +构建 元数据，再拆 -预发布；缺失段为空串）
    let 剥离构建 = 版本串
        .split_once('+')
        .map_or(版本串.as_str(), |(值内容项, _)| 值内容项);
    let (核心版本, 预发布) = 剥离构建
        .split_once('-')
        .map_or((剥离构建, ""), |(核心段, 预发布段)| (核心段, 预发布段));
    let mut 版本段 = 核心版本.split('.');
    let 主版本 = 版本段.next().unwrap_or("").to_string();
    let 次版本 = 版本段.next().unwrap_or("").to_string();
    let 补丁版本 = 版本段.next().unwrap_or("").to_string();
    let 预发布串 = 预发布.to_string();
    // authors：cargo 以冒号连接多作者
    let 作者串 = match 包声明.and_then(|表项| 表项.get("authors")) {
        Some(toml::Value::Array(列表)) => 列表
            .iter()
            .filter_map(toml::Value::as_str)
            .collect::<Vec<_>>()
            .join(":"),
        Some(toml::Value::String(串)) => 串.clone(),
        _ => String::new(),
    };
    let mut 变量列表 = vec![
        ("CARGO_PKG_NAME".to_string(), 名称),
        ("CARGO_PKG_VERSION".to_string(), 版本串),
        ("CARGO_PKG_VERSION_MAJOR".to_string(), 主版本),
        ("CARGO_PKG_VERSION_MINOR".to_string(), 次版本),
        ("CARGO_PKG_VERSION_PATCH".to_string(), 补丁版本),
        ("CARGO_PKG_VERSION_PRE".to_string(), 预发布串),
        ("CARGO_CRATE_NAME".to_string(), 库名下划线.to_string()),
        (
            "CARGO_MANIFEST_DIR".to_string(),
            源目录.display().to_string(),
        ),
        (
            "CARGO_MANIFEST_PATH".to_string(),
            清单路径.display().to_string(),
        ),
        ("CARGO_PKG_AUTHORS".to_string(), 作者串),
        (
            "CARGO_PKG_DESCRIPTION".to_string(),
            取清单字段("description"),
        ),
        ("CARGO_PKG_HOMEPAGE".to_string(), 取清单字段("homepage")),
        ("CARGO_PKG_REPOSITORY".to_string(), 取清单字段("repository")),
        ("CARGO_PKG_LICENSE".to_string(), 取清单字段("license")),
        (
            "CARGO_PKG_LICENSE_FILE".to_string(),
            取清单字段("license-file"),
        ),
    ];
    if let Some(rust版本) = 包声明
        .and_then(|表项| 表项.get("rust-version"))
        .and_then(toml::Value::as_str)
    {
        变量列表.push(("CARGO_PKG_RUST_VERSION".to_string(), rust版本.to_string()));
    }
    if let Some(readme) = 包声明
        .and_then(|表项| 表项.get("readme"))
        .and_then(toml::Value::as_str)
    {
        变量列表.push(("CARGO_PKG_README".to_string(), readme.to_string()));
    }
    变量列表
}

/// 对单个 crate 手动调用 rustdoc 生成 JSON 文档
///
/// 注入目标 crate 实际启用的 features（cfg）、构建脚本产物（OUT_DIR / cfg /
/// rustc-env）、标准 CARGO_PKG_* 环境变量与直接依赖的 .rlib/.so 路径
/// （--extern，按 package_id 精确版本），保证 cfg(feature) 与 include!
/// 生成的 API 不缺失。
fn 单库文档化(
    项目根: &Path,
    包项: &Value,
    库名: &str,
    包标识: &str,
    产物表: &HashMap<String, (String, String, Vec<String>)>,
    直接依赖: &[(String, String)],
    构建脚本表: &HashMap<String, 构建脚本产物>,
) -> anyhow::Result<String> {
    let 界面 = crate::本地化::界面::全局();
    let 清单路径 = PathBuf::from(
        包项
            .get("manifest_path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("{}", 界面.取文带参("mg_err_no_manifest", &[库名])))?,
    );
    let 源目录 = 清单路径
        .parent()
        .ok_or_else(|| anyhow!("{}", 界面.取文带参("mg_err_no_src_dir", &[库名])))?;
    let 库文件 = 源目录.join("src/lib.rs");
    if !库文件.exists() {
        bail!("{}", 界面.取文带参("mg_err_no_lib", &[库名]));
    }
    // 默认 features：cargo 直接传给 rustc（--cfg feature=...），不经过 build script，
    // 手动 rustdoc 时必须显式补传，否则 cfg(feature) 裁掉的 API 会缺失
    let 默认特征 = 包项
        .get("features")
        .and_then(|特征表项| 特征表项.get("default"))
        .and_then(Value::as_array)
        .map(|列表| {
            列表
                .iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let 清单内容 = fs::read_to_string(&清单路径).with_context(|| {
        界面.取文带参("mg_err_read_manifest", &[&清单路径.display().to_string()])
    })?;
    let 清单: toml::Value = toml::from_str(&清单内容).map_err(|错| {
        anyhow!(
            "{}",
            界面.取文带参(
                "mg_err_parse_manifest",
                &[&清单路径.display().to_string(), &错.to_string()]
            )
        )
    })?;
    let 编辑版本 = 清单
        .get("package")
        .and_then(|表项| 表项.get("edition"))
        .and_then(toml::Value::as_str)
        .unwrap_or("2015")
        .to_string();
    let 过程宏 = 清单
        .get("lib")
        .and_then(|表项| 表项.get("proc-macro"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);

    let 库名下划线 = 库名.replace('-', "_");
    let 文档目录 = 项目根.join("mapping-json");
    fs::create_dir_all(&文档目录)?;
    let mut 进程 = Command::new(crate::解析rustdoc路径());
    进程
        .arg(&库文件)
        .arg("--crate-name")
        .arg(&库名下划线)
        .arg("--crate-type")
        .arg(if 过程宏 { "proc-macro" } else { "lib" })
        .arg("--edition")
        .arg(&编辑版本)
        .arg("-L")
        .arg(format!(
            "dependency={}",
            项目根.join("target/debug/deps").display()
        ))
        .env("RUSTC_BOOTSTRAP", "1");
    // 补全 cargo 编译时注入的标准环境变量：proc macro（如 tauri 的
    // `#[command]`）在展开期读取 CARGO_PKG_NAME，缺失会直接 panic
    for (键名, 值项) in 标准环境变量(包项, &清单, 库名, &库名下划线, 源目录, &清单路径)
    {
        进程.env(键名, 值项);
    }
    // --extern 只传直接依赖的精确版本：同名多版本 crate（如 rand 0.8.7 与
    // 0.10.2 共存）无法从 -L 目录自动解析，rustc 可能链接错误版本；传递依赖
    // 由 rustc 按 rlib 元数据 hash 在 -L 中自动解析
    for (依赖名, 依赖包标识) in 直接依赖 {
        if let Some((_, 产物文件路径, _)) = 产物表.get(依赖包标识) {
            进程
                .arg("--extern")
                .arg(format!("{}={}", 依赖名, 产物文件路径));
        }
    }
    // 注入 cargo build 实际启用的 features（feature 统一解析结果）作为 cfg，
    // 与编译产物保持一致；未命中时回退包声明的 default features
    let 特征列表 = 产物表
        .get(包标识)
        .map(|(_, _, 特征串)| 特征串.clone())
        .unwrap_or(默认特征);
    for 特征项 in &特征列表 {
        进程.arg("--cfg").arg(format!("feature=\"{}\"", 特征项));
    }
    // 注入目标 crate 构建脚本产物：OUT_DIR（include! 生成代码）、cfg（条件编译）、rustc-env
    if let Some((输出目录, 条件配置, 环境变量)) = 构建脚本表.get(包标识) {
        if let Some(构建目录) = 输出目录 {
            进程.env("OUT_DIR", 构建目录);
        }
        for 配置项 in 条件配置 {
            进程.arg("--cfg").arg(配置项);
        }
        for 项 in 环境变量 {
            if let Some((键名, 值项)) = 项.split_once('=') {
                进程.env(键名, 值项);
            }
        }
    }
    进程
        .arg("-Z")
        .arg("unstable-options")
        .arg("--output-format")
        .arg("json")
        .arg("--output")
        .arg(&文档目录);
    运行命令(&mut 进程, &界面.取文("mg_cmd_gen_doc"))?;

    let 文档路径 = 文档目录.join(format!("{}.json", 库名下划线));
    fs::read_to_string(&文档路径).with_context(|| {
        界面.取文带参("mg_err_no_doc_json", &[&文档路径.display().to_string()])
    })
}

/// 运行命令并返回 stdout；失败时附加 stderr 摘要
const 错误摘要行数: usize = 15;
fn 运行命令(进程: &mut Command, 命令描述: &str) -> anyhow::Result<String> {
    let 命令输出 = 进程.output().with_context(|| {
        crate::本地化::界面::全局().取文带参("mg_err_run_failed", &[命令描述])
    })?;
    if !命令输出.status.success() {
        let 错误流 = String::from_utf8_lossy(&命令输出.stderr);
        let 摘要串 = 错误流
            .lines()
            .take(错误摘要行数)
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "{}",
            crate::本地化::界面::全局().取文带参("mg_err_failed", &[命令描述, &摘要串])
        );
    }
    Ok(String::from_utf8_lossy(&命令输出.stdout).to_string())
}

#[cfg(test)]
mod 单元测试 {
    use super::super::接口解析::{提取公开接口, 样例文本};
    use super::*;

    #[test]
    fn 测试真实工具链提取() {
        let 临时 = tempfile::tempdir().unwrap();
        let 迷你 = 临时.path().join("mini-crate");
        fs::create_dir_all(迷你.join("src")).unwrap();
        fs::write(
            迷你.join("Cargo.toml"),
            "[package]\nname = \"mini-crate\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            迷你.join("src/lib.rs"),
            "pub fn 新建(x: u32) -> Result<Foo, String> { Ok(Foo) }\npub struct Foo;\npub enum 颜色 { 红, 蓝 }\npub trait 行为 {}\npub type 数量 = u32;\npub const 最大值: u32 = 100;\nfn 私有函数() {}\n",
        )
        .unwrap();
        let 外壳 = 临时.path().join("外壳");
        fs::create_dir_all(外壳.join("src")).unwrap();
        fs::write(
            外壳.join("Cargo.toml"),
            "[package]\nname = \"外壳\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-crate = { path = \"../mini-crate\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(外壳.join("src/lib.rs"), "// 空库\n").unwrap();

        let 守卫实例 = 临时项目守卫::新建("mini-crate").expect("创建临时项目失败");
        // 覆盖临时项目路径为外壳项目
        let _ = fs::remove_dir_all(守卫实例.获取路径());
        fs::create_dir_all(外壳.join("src")).unwrap();
        let (文档集, 已解析版本) =
            内部提取库文档(&临时项目守卫(外壳.clone()), "mini-crate").expect("工具链应能提取文档");
        // 本地 path 依赖 0.1.0：解析版本应被捕获（映射文件头基准来源）
        assert_eq!(已解析版本.as_deref(), Some("0.1.0"));
        let 接口列表 = 提取公开接口(&文档集[0].1).unwrap();
        let 名称列表: Vec<&str> = 接口列表
            .iter()
            .map(|接口项| 接口项.英文原名.as_str())
            .collect();
        assert!(
            名称列表.contains(&"新建"),
            "应提取到函数 新建: {:?}",
            名称列表
        );
        assert!(名称列表.contains(&"Foo"));
        assert!(名称列表.contains(&"颜色"));
        assert!(名称列表.contains(&"行为"));
        assert!(名称列表.contains(&"数量"));
        assert!(名称列表.contains(&"最大值"));
        assert!(!名称列表.contains(&"私有函数"), "私有函数不应被提取");
        // 签名包含类型信息
        let 新函数 = 接口列表
            .iter()
            .find(|接口项| 接口项.英文原名 == "新建")
            .unwrap();
        assert!(
            新函数.类型签名.contains("u32"),
            "签名应含参数类型: {}",
            新函数.类型签名
        );
    }

    /// 不存在的 crate 应报错（含"未找到"提示）
    #[test]
    fn 测试不存在的库报错() {
        // 错误文案按界面语言渲染，语言又取自环境变量：临时接管清除
        // 区域设置 / RZ_LANG，保证命中中文模板（macos runner 预设 en
        // 会渲染出英文文案，导致此前断言假失败）
        let _锁 = crate::语言包工具::单元测试::环境锁();
        let _环境 = crate::语言包工具::单元测试::环境恢复::接管(&[
            "RZ_LANG",
            "LC_ALL",
            "LC_MESSAGES",
            "LANG",
        ]);
        let 守卫 = 临时项目守卫::新建("rzc-不存在的crate-xyz-123").expect("创建临时项目失败");
        // 写入不含目标 crate 的有效最小项目：让 `cargo metadata` 成功，
        // 从而命中产品逻辑的 "crate 未找到" 映射（否则 cargo 先因缺
        // Cargo.toml 报通用错误，根本走不到该分支）
        fs::write(
            守卫.获取路径().join("Cargo.toml"),
            "[package]\nname = \"rzc-mapping-temp\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(守卫.获取路径().join("src/lib.rs"), "// 空库\n").unwrap();
        let 提取产出 = 内部提取库文档(&守卫, "rzc-不存在的crate-xyz-123");
        let 错误信息 = 提取产出.expect_err("应报错");
        assert!(
            错误信息.to_string().contains("未找到"),
            "错误应提示未找到: {}",
            错误信息
        );
    }

    /// 真实工具链 + 构建脚本（build.rs 生成 include! 源码）：
    /// 验证 OUT_DIR 注入，覆盖 serde 等依赖 build.rs 的 crate
    #[test]
    fn 测试构建脚本产物注入() {
        let 临时 = tempfile::tempdir().unwrap();
        let 迷你 = 临时.path().join("mini-gen");
        fs::create_dir_all(迷你.join("src")).unwrap();
        fs::write(
            迷你.join("Cargo.toml"),
            "[package]\nname = \"mini-gen\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[build-dependencies]\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            迷你.join("build.rs"),
            "fn main() {\n    let dir = std::env::var(\"OUT_DIR\").unwrap();\n    std::fs::write(std::path::Path::new(&dir).join(\"generated.rs\"), \"pub const GENERATED_VALUE: u32 = 42;\\n\").unwrap();\n}\n",
        )
        .unwrap();
        fs::write(
            迷你.join("src/lib.rs"),
            "include!(concat!(env!(\"OUT_DIR\"), \"/generated.rs\"));\npub fn use_generated_value() -> u32 { GENERATED_VALUE }\n",
        )
        .unwrap();
        let 外壳 = 临时.path().join("外壳2");
        fs::create_dir_all(外壳.join("src")).unwrap();
        fs::write(
            外壳.join("Cargo.toml"),
            "[package]\nname = \"外壳2\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-gen = { path = \"../mini-gen\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(外壳.join("src/lib.rs"), "// 空库\n").unwrap();

        let (文档集, _) = 内部提取库文档(&临时项目守卫(外壳.clone()), "mini-gen")
            .expect("OUT_DIR 注入后应能生成文档");
        let 接口列表 = 提取公开接口(&文档集[0].1).unwrap();
        let 名称列表: Vec<&str> = 接口列表
            .iter()
            .map(|接口项| 接口项.英文原名.as_str())
            .collect();
        assert!(
            名称列表.contains(&"use_generated_value"),
            "应提取到 include! 生成的代码后的函数: {:?}",
            名称列表
        );
    }

    /// 默认 feature 注入：cfg(feature) 裁掉的 API 需通过 --cfg feature=... 恢复
    #[test]
    fn 测试默认特征注入() {
        let 临时 = tempfile::tempdir().unwrap();
        let 迷你 = 临时.path().join("mini-feat");
        fs::create_dir_all(迷你.join("src")).unwrap();
        fs::write(
            迷你.join("Cargo.toml"),
            "[package]\nname = \"mini-feat\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[features]\ndefault = [\"magic\"]\n\"magic\" = []\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            迷你.join("src/lib.rs"),
            "pub fn normal_fn() {}\n#[cfg(feature = \"magic\")]\npub fn magic_fn() {}\n",
        )
        .unwrap();
        let 外壳 = 临时.path().join("外壳3");
        fs::create_dir_all(外壳.join("src")).unwrap();
        fs::write(
            外壳.join("Cargo.toml"),
            "[package]\nname = \"外壳3\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-feat = { path = \"../mini-feat\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(外壳.join("src/lib.rs"), "// 空库\n").unwrap();

        let (文档集, _) =
            内部提取库文档(&临时项目守卫(外壳.clone()), "mini-feat").expect("应能生成文档");
        let 接口列表 = 提取公开接口(&文档集[0].1).unwrap();
        let 名称列表: Vec<&str> = 接口列表
            .iter()
            .map(|接口项| 接口项.英文原名.as_str())
            .collect();
        assert!(
            名称列表.contains(&"magic_fn"),
            "默认 feature 下的 API 应被提取: {:?}",
            名称列表
        );
        assert!(名称列表.contains(&"normal_fn"));
    }

    /// 薄壳 crate（meta crate）glob 重导出追踪：目标 crate 仅 `pub use 依赖::*`，
    /// 其公开 API 应通过追踪被重导出的依赖 crate 提取到（如 salvo → salvo_core）
    #[test]
    fn 测试全局重导出追踪() {
        let 临时 = tempfile::tempdir().unwrap();
        // 真实 crate：mini-core 提供全部 API
        let 核 = 临时.path().join("mini-core");
        fs::create_dir_all(核.join("src")).unwrap();
        fs::write(
            核.join("Cargo.toml"),
            "[package]\nname = \"mini-core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            核.join("src/lib.rs"),
            "pub fn handle_request() -> Result<Response, Error> { Ok(Response) }\npub struct Response;\npub enum Error { E }\npub trait Handler {}\n",
        )
        .unwrap();
        // 薄壳 crate：全部 API 来自 glob 重导出（与 salvo 的 lib.rs 结构一致）
        let 壳 = 临时.path().join("mini-facade");
        fs::create_dir_all(壳.join("src")).unwrap();
        fs::write(
            壳.join("Cargo.toml"),
            "[package]\nname = \"mini-facade\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-core = { path = \"../mini-core\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            壳.join("src/lib.rs"),
            "pub use mini_core::*;\npub use mini_core as core;\n",
        )
        .unwrap();
        let 外壳 = 临时.path().join("外壳4");
        fs::create_dir_all(外壳.join("src")).unwrap();
        fs::write(
            外壳.join("Cargo.toml"),
            "[package]\nname = \"外壳4\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-facade = { path = \"../mini-facade\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(外壳.join("src/lib.rs"), "// 空库\n").unwrap();

        let (文档集, _) =
            内部提取库文档(&临时项目守卫(外壳.clone()), "mini-facade").expect("工具链应能提取文档");
        // 目标 crate + 被 glob 重导出的依赖 crate（mini-core）
        let 库名列表: Vec<&str> = 文档集.iter().map(|(名称, _)| 名称.as_str()).collect();
        assert!(
            文档集.len() >= 2,
            "应追踪到被重导出的依赖 crate: {:?}",
            库名列表
        );
        assert!(
            库名列表.contains(&"mini_core"),
            "glob 重导出链应包含 mini_core（crate 名下划线形式）: {:?}",
            库名列表
        );
        // 合并全部 JSON 提取，薄壳 crate 的 API 应全部到位
        let mut 接口列表 = Vec::new();
        let mut 已见名称 = HashSet::new();
        for (_, 文档文本) in &文档集 {
            for 接口项 in 提取公开接口(文档文本).unwrap() {
                if 已见名称.insert(接口项.英文原名.clone()) {
                    接口列表.push(接口项);
                }
            }
        }
        let 名称列表: Vec<&str> = 接口列表
            .iter()
            .map(|接口项| 接口项.英文原名.as_str())
            .collect();
        assert!(名称列表.contains(&"handle_request"));
        assert!(名称列表.contains(&"Response"));
        assert!(名称列表.contains(&"Error"));
        assert!(名称列表.contains(&"Handler"));
    }

    /// cargo_env_vars：名称/版本分段/作者拼接与空值兜底（cargo 语义）
    #[test]
    fn 测试标准环境变量取值() {
        let 包值 = serde_json::json!({ "name": "tauri", "version": "2.11.5" });
        let 清单: toml::Value = toml::from_str(
            "[package]\nname = \"tauri\"\nversion = \"2.11.5\"\nauthors = [\"A\", \"B\"]\nlicense = \"MIT\"\nrust-version = \"1.77\"\n",
        )
        .unwrap();
        let 变量列表 = 标准环境变量(
            &包值,
            &清单,
            "tauri",
            "tauri",
            Path::new("/src/tauri-2.11.5"),
            Path::new("/src/tauri-2.11.5/Cargo.toml"),
        );
        let 取值 = |键名: &str| {
            变量列表
                .iter()
                .find(|(键项, _)| 键项 == 键名)
                .map(|(_, 值项)| 值项.as_str())
        };
        assert_eq!(取值("CARGO_PKG_NAME"), Some("tauri"));
        assert_eq!(取值("CARGO_PKG_VERSION"), Some("2.11.5"));
        assert_eq!(取值("CARGO_PKG_VERSION_MAJOR"), Some("2"));
        assert_eq!(取值("CARGO_PKG_VERSION_MINOR"), Some("11"));
        assert_eq!(取值("CARGO_PKG_VERSION_PATCH"), Some("5"));
        assert_eq!(取值("CARGO_PKG_VERSION_PRE"), Some(""));
        assert_eq!(取值("CARGO_CRATE_NAME"), Some("tauri"));
        assert_eq!(取值("CARGO_MANIFEST_DIR"), Some("/src/tauri-2.11.5"));
        assert_eq!(取值("CARGO_PKG_AUTHORS"), Some("A:B"));
        assert_eq!(取值("CARGO_PKG_LICENSE"), Some("MIT"));
        assert_eq!(取值("CARGO_PKG_RUST_VERSION"), Some("1.77"));
        // 未声明的字段与 cargo 一致给空串；未声明 rust-version 不提供该变量
        assert_eq!(取值("CARGO_PKG_DESCRIPTION"), Some(""));
        let 包2 = serde_json::json!({ "name": "x", "version": "1.0.0-alpha.1+build.5" });
        let 清单2: toml::Value =
            toml::from_str("[package]\nname = \"x\"\nversion = \"1.0.0-alpha.1+build.5\"\n")
                .unwrap();
        let 变量列表2 = 标准环境变量(
            &包2,
            &清单2,
            "x",
            "x",
            Path::new("/s"),
            Path::new("/s/Cargo.toml"),
        );
        let 取值2 = |键名: &str| {
            变量列表2
                .iter()
                .find(|(键项, _)| 键项 == 键名)
                .map(|(_, 值项)| 值项.as_str())
        };
        assert_eq!(取值2("CARGO_PKG_VERSION_PRE"), Some("alpha.1"));
        assert_eq!(取值2("CARGO_PKG_VERSION_PATCH"), Some("0"));
        assert_eq!(取值2("CARGO_PKG_RUST_VERSION"), None);
        assert_eq!(取值2("CARGO_PKG_README"), None);
    }

    /// 真实工具链：rustdoc 编译期读取 CARGO_PKG_*（tauri 的 command 宏同款行为），
    /// 注入缺失或值错误时 const 断言编译失败——保障提取管线补全这些变量
    #[test]
    fn 测试环境变量注入() {
        let 临时 = tempfile::tempdir().unwrap();
        let 迷你 = 临时.path().join("mini-env");
        fs::create_dir_all(迷你.join("src")).unwrap();
        fs::write(
            迷你.join("Cargo.toml"),
            "[package]\nname = \"mini-env\"\nversion = \"0.2.3\"\nedition = \"2024\"\ndescription = \"迷你环境测试包\"\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            迷你.join("src/lib.rs"),
            "const fn eq(a: &str, b: &str) -> bool {\n\
             let (a, b) = (a.as_bytes(), b.as_bytes());\n\
             if a.len() != b.len() {\n\
             return false;\n\
             }\n\
             let mut i = 0;\n\
             while i < a.len() {\n\
             if a[i] != b[i] {\n\
             return false;\n\
             }\n\
             i += 1;\n\
             }\n\
             true\n\
             }\n\
             const _: () = assert!(eq(env!(\"CARGO_PKG_NAME\"), \"mini-env\"));\n\
             const _: () = assert!(eq(env!(\"CARGO_PKG_VERSION\"), \"0.2.3\"));\n\
             const _: () = assert!(eq(env!(\"CARGO_PKG_VERSION_MINOR\"), \"2\"));\n\
             const _: () = assert!(eq(env!(\"CARGO_CRATE_NAME\"), \"mini_env\"));\n\
             pub fn env_check() -> u8 { 1 }\n",
        )
        .unwrap();
        let 外壳 = 临时.path().join("外壳5");
        fs::create_dir_all(外壳.join("src")).unwrap();
        fs::write(
            外壳.join("Cargo.toml"),
            "[package]\nname = \"外壳5\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-env = { path = \"../mini-env\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(外壳.join("src/lib.rs"), "// 空库\n").unwrap();

        let (文档集, _) = 内部提取库文档(&临时项目守卫(外壳.clone()), "mini-env")
            .expect("CARGO_PKG_* 注入后应能生成文档");
        let 接口列表 = 提取公开接口(&文档集[0].1).unwrap();
        let 名称列表: Vec<&str> = 接口列表
            .iter()
            .map(|接口项| 接口项.英文原名.as_str())
            .collect();
        assert!(
            名称列表.contains(&"env_check"),
            "应提取到 env_check: {:?}",
            名称列表
        );
    }

    /// 样本 JSON 可解析（样本与提取器在同一 crate，防漂移）
    #[test]
    fn 测试样本文档可解析() {
        let 接口列表 = 提取公开接口(&样例文本()).unwrap();
        assert!(!接口列表.is_empty());
    }
}
