//! 工具链集成：在临时项目中把目标包作为依赖编译，手动调用 rustdoc
//! 生成 JSON 文档（含薄壳包的整批重导出追踪、构建脚本产物与
//! 包元数据标准环境变量注入）。

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, anyhow};

use super::文档抽取::提取公开接口带通配源;

/// rustdoc JSON 文本列表：(包名, JSON 文档文本)
type 文档列表 = Vec<(String, String)>;

/// 提取结果：(JSON 文本列表, 目标包实际解析版本)
type 包文档提取结果 = (文档列表, Option<String>);

/// 临时项目目录守卫（释放时自动清理）
struct 临时项目(PathBuf);

impl 临时项目 {
    /// 创建临时项目目录
    fn 新建(包名: &str) -> anyhow::Result<Self> {
        // 用户隔离 + 符号链接校验（包名已由运行自动生成校验为
        // 字母数字标识符字符；仍拼接用户/进程号段保证路径不可预测）
        let 目录 = crate::临时守卫::安全临时路径(&format!(
            "rzc-mapping-{}-{}-{}",
            包名,
            crate::临时守卫::安全用户段(),
            std::process::id()
        ))?;
        let _ = fs::remove_dir_all(&目录);
        fs::create_dir_all(目录.join("src")).map_err(|错误值| {
            anyhow!(
                "{}",
                crate::界面::界面消息表::全局表()
                    .取带参消息("mg_err_tempdir", &[&错误值.to_string()])
            )
        })?;
        Ok(临时项目(目录))
    }

    /// 获取临时项目路径
    fn 存放路径(&self) -> &Path {
        &self.0
    }
}

impl Drop for 临时项目 {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 提取目标包及其整批重导出链上依赖包的 rustdoc JSON 文本列表
///
/// 首个元素为目标包本身；后续元素为被整批重导出（`公开 使用 依赖::*`）
/// 的依赖包——薄壳包（如转发层）的公开接口全部来自这些包。
/// 第二个返回值为目标包的实际解析版本（来自 cargo 元数据），
/// 供映射文件头记录生成基准。
///
/// - 版本需求：包管理器版本需求（`=x.y.z` 精确锁定 / `x.y.*` 前缀）；
///   空值时用 `*`（解析到当时最新版，结果不可复现，调用方已提示）
pub fn 提取包文档(
    包名: &str,
    版本需求值: Option<&str>,
) -> anyhow::Result<包文档提取结果> {
    let 临时 = 临时项目::新建(包名)?;
    // 1. 临时项目：把目标包作为唯一依赖；版本需求由目标版本参数转换
    //    （未指定时 * 允许任意已发布版本，生成基准不可复现）
    fs::write(
        临时.存放路径().join("Cargo.toml"),
        format!(
            "[package]\nname = \"rzc-mapping-temp\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n\"{}\" = \"{}\"\n\n[workspace]\n",
            包名,
            版本需求值.unwrap_or("*")
        ),
    )?;
    fs::write(
        临时.存放路径().join("src/lib.rs"),
        "// 仅供提取依赖接口的空库\n",
    )?;
    提取文档结构内部(&临时, 包名)
}

/// 工具链核心（对测试开放）：在给定临时项目中定位包并生成其 rustdoc JSON
///
/// 1. `cargo metadata` 定位目标包的源码目录（注册仓库或本地路径依赖均可）
/// 2. `cargo build` 编译依赖树，解析每个依赖的库文件路径
/// 3. 手动调用 `rustdoc -Z unstable-options --output-format json` 文档化目标包
/// 4. 薄壳包（转发包）的公开接口来自整批重导出：解析每个包的整批重导出
///    来源路径，对被重导出的依赖包也生成文档并继续追踪，直至闭环。
///    返回 ((包名, JSON 文本) 列表（首个为目标包本身）, 实际解析版本)。
fn 提取文档结构内部(
    临时: &临时项目, 包名: &str
) -> anyhow::Result<包文档提取结果> {
    let 项目根 = 临时.存放路径();
    let 界面表 = crate::界面::界面消息表::全局表();

    // 1. 元数据：定位目标包的清单文件
    let 元数据输出 = 运行命令(
        Command::new(crate::解析cargo())
            .arg("metadata")
            .arg("--format-version")
            .arg("1")
            .current_dir(项目根),
        &界面表.取消息("mg_cmd_parse_deps"),
    )?;
    let 元数据: Value = serde_json::from_str(&元数据输出).map_err(|错误值| {
        anyhow!(
            "{}",
            界面表.取带参消息("mg_err_parse_meta", &[&错误值.to_string()])
        )
    })?;
    // 包元数据索引（清单路径 / 特性 / 版次），供整批重导出链上的
    // 依赖包查询（薄壳包的接口在其依赖中）。同时索引包名
    // （命令行参数）与库目标的包名（文档整批重导出来源），
    // 两者在连字符/下划线写法上可能不同。
    let mut 包索引: HashMap<String, Value> = HashMap::new();
    let 空包表: Vec<Value> = Vec::new();
    for 包项 in 元数据["packages"].as_array().unwrap_or(&空包表) {
        if let Some(包名称) = 包项.get("name").and_then(Value::as_str) {
            包索引.insert(包名称.to_string(), 包项.clone());
        }
        if let Some(库名称) = 包项
            .get("targets")
            .and_then(Value::as_array)
            .and_then(|目标表| {
                目标表.iter().find(|目标项| {
                    目标项
                        .get("kind")
                        .and_then(Value::as_array)
                        .map(|种类表| 种类表.iter().any(|值| 值.as_str() == Some("lib")))
                        .unwrap_or(false)
                })
            })
            .and_then(|目标项| 目标项.get("name"))
            .and_then(Value::as_str)
        {
            包索引.insert(库名称.to_string(), 包项.clone());
        }
    }
    if !包索引.contains_key(包名) {
        anyhow::bail!("{}", 界面表.取带参消息("mg_err_crate_not_found", &[包名]));
    }
    // 目标包的实际解析版本（映射文件头基准版本来源；本地路径依赖
    // 取路径包声明版本，注册仓库依赖取本次解析命中版本）
    let 解析版本 = 包索引
        .get(包名)
        .and_then(|包项| 包项.get("version"))
        .and_then(Value::as_str)
        .map(String::from);

    // 解析节点：完整依赖解析图（含多版本共存时的精确解析），按
    // 包标识索引每个包的直接依赖（别名, 依赖包标识）。
    // 外部库参数只传直接依赖的精确版本；传递依赖由编译器按库文件元数据
    // 散列在库目录中自动解析（同名多版本包无法从库目录手动解析）
    let mut 直接依赖表: HashMap<String, Vec<(String, String)>> = HashMap::new();
    if let Some(节点表) = 元数据
        .get("resolve")
        .and_then(|解析值| 解析值.get("nodes"))
        .and_then(Value::as_array)
    {
        for 节点 in 节点表 {
            let Some(节点标识) = 节点.get("id").and_then(Value::as_str) else {
                continue;
            };
            let mut 依赖表: Vec<(String, String)> = Vec::new();
            if let Some(依赖清单) = 节点.get("deps").and_then(Value::as_array) {
                for 依赖项 in 依赖清单 {
                    let Some(依赖名) = 依赖项.get("name").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some(依赖包) = 依赖项.get("pkg").and_then(Value::as_str) else {
                        continue;
                    };
                    // 只保留普通依赖（开发/构建依赖不进入库的外部预导入表）；
                    // 目标特定依赖无法静态判定，保守保留
                    let 是普通依赖 = 依赖项
                        .get("dep_kinds")
                        .and_then(Value::as_array)
                        .map(|种类表| {
                            种类表.iter().any(|种类项| {
                                种类项.get("kind").map(|值| 值.is_null()).unwrap_or(true)
                            })
                        })
                        .unwrap_or(true);
                    if 是普通依赖 {
                        依赖表.push((依赖名.to_string(), 依赖包.to_string()));
                    }
                }
            }
            直接依赖表.insert(节点标识.to_string(), 依赖表);
        }
    }

    // 2. 编译依赖树，解析依赖库文件路径。
    //    统一工具链：显式指定编译器为解析到的同一编译器，保证库文件与后续
    //    手调文档生成（同系统根）版本一致，根治跨版本链接错误。
    let mut 编译命令值 = Command::new(crate::解析cargo());
    编译命令值
        .arg("build")
        .arg("--message-format=json")
        .env("RUSTC", crate::解析rustc())
        .current_dir(项目根);
    let 编译输出 = 运行命令(&mut 编译命令值, &界面表.取消息("mg_cmd_build_deps"))?;
    // 包标识 -> (库目标名, 库文件路径, 实际启用的特性)。
    // 按包标识索引而非目标名：依赖树中同名不同版本的包共存时，
    // 按名字覆盖会链接错误版本；特性取编译时的统一解析结果而非包声明的
    // 默认特性——依赖方可能未启用部分默认特性，注入不一致的条件编译项
    // 会导致编译失败（缺依赖）。
    let mut 产物表: HashMap<String, (String, String, Vec<String>)> = HashMap::new();
    // 已编译库的包名集合（整批重导出链入队检查用）
    let mut 已编译库名: HashSet<String> = HashSet::new();
    // 包标识 -> 构建脚本产物（见模块级构建脚本信息类型注释）
    let mut 构建脚本表: HashMap<String, 构建脚本信息> = HashMap::new();
    for 行文本 in 编译输出.lines() {
        let Ok(消息值) = serde_json::from_str::<Value>(行文本) else {
            continue;
        };
        let Some(原因) = 消息值.get("reason").and_then(Value::as_str) else {
            continue;
        };
        if 原因 != "compiler-artifact" && 原因 != "build-script-executed" {
            continue;
        }
        let Some(包标识) = 消息值.get("package_id").and_then(Value::as_str) else {
            continue;
        };
        if 原因 == "build-script-executed" {
            let 输出目录 = 消息值
                .get("out_dir")
                .and_then(Value::as_str)
                .map(String::from);
            let 条件项表 = 消息值
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
            let 环境项表 = 消息值
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
            构建脚本表.insert(包标识.to_string(), (输出目录, 条件项表, 环境项表));
            continue;
        }
        // 编译产物：按包标识索引（与元数据里的包标识一致），
        // 多版本共存时精确关联到具体版本；目标名仅作为库名记录
        let Some(目标名) = 消息值
            .get("target")
            .and_then(|目标项| 目标项.get("name"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        // 只保留合法标识符的包名（本地工作区成员如中文名项目会被排除）
        if 目标名.is_empty()
            || 目标名 == "rzc-mapping-temp"
            || !目标名
                .chars()
                .all(|本字| 本字.is_ascii_alphanumeric() || 本字 == '_')
        {
            continue;
        }
        if let Some(文件名表) = 消息值.get("filenames").and_then(Value::as_array)
            && let Some(库文件) = 文件名表
                .iter()
                .filter_map(Value::as_str)
                .find(|文件名| 文件名.ends_with(".rlib") || 文件名.ends_with(".so"))
        {
            let 特性表 = 消息值
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
                (目标名.to_string(), 库文件.to_string(), 特性表),
            );
            已编译库名.insert(目标名.to_string());
        }
    }
    if 产物表.is_empty() {
        anyhow::bail!("{}", 界面表.取带参消息("mg_err_build_failed", &[包名]));
    }

    // 3. 文档生成队列：目标包 → 整批重导出链上的依赖包。
    //    薄壳包的索引只有重导出节点，公开接口全部来自被重导出的依赖包，
    //    逐个生成文档后合并提取。
    let mut 待办队列: Vec<String> = vec![包名.to_string()];
    let mut 已访问: HashSet<String> = HashSet::new();
    let mut 结果表: Vec<(String, String)> = Vec::new();
    while let Some(当前名) = 待办队列.pop() {
        if !已访问.insert(当前名.clone()) {
            continue;
        }
        let Some(包项) = 包索引.get(&当前名) else {
            continue;
        };
        let 包标识 = 包项
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        // 直接依赖（别名, 精确版本包标识）：外部库参数只传这些
        let 直接依赖项 = 直接依赖表.get(&包标识).cloned().unwrap_or_default();
        let 文档文本 = 单包文档(
            项目根,
            包项,
            &当前名,
            &包标识,
            &产物表,
            &直接依赖项,
            &构建脚本表,
        )?;
        // 整批重导出（公开 使用 依赖::*）→ 被重导出包名（来源首段），
        // 若在依赖树中则入队继续提取；特性未启用而未编译的依赖自动跳过
        let (_, 通配源表) = 提取公开接口带通配源(&文档文本)?;
        for 来源 in 通配源表 {
            if 已编译库名.contains(&来源) && !已访问.contains(&来源) {
                待办队列.push(来源);
            }
        }
        结果表.push((当前名, 文档文本));
    }
    Ok((结果表, 解析版本))
}

/// 构建脚本产物：输出目录（内联生成代码）、条件编译项、编译期环境变量列表。
/// 部分包的构建脚本会生成被内联的源码或声明条件编译项，
/// 手动文档生成时必须注入，否则编译失败。
type 构建脚本信息 = (Option<String>, Vec<String>, Vec<String>);

/// 包管理器编译时注入的标准包元数据环境变量：(键, 值) 列表
///
/// 手动文档生成不经过包管理器，过程宏在展开期读取这些变量
/// 会因缺失直接恐慌；未声明的包字段与包管理器一致地给出空字符串
/// （最低编译器版本 / 说明文件仅在清单中声明时才提供）。
fn 构建环境变量(
    包项: &Value,
    清单: &toml::Value,
    包名: &str,
    包名下划线: &str,
    源码目录: &Path,
    清单路径: &Path,
) -> Vec<(String, String)> {
    let 包节 = 清单.get("package");
    let 元数据文本 = |键名: &str| -> String {
        包项
            .get(键名)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let 清单文本 = |键名: &str| -> String {
        包节
            .and_then(|包值| 包值.get(键名))
            .and_then(toml::Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let 名称 = {
        let 元数据名称 = 元数据文本("name");
        if 元数据名称.is_empty() {
            包名.to_string()
        } else {
            元数据名称
        }
    };
    let 版本 = 元数据文本("version");
    // 版本分段（先剥离构建元数据，再拆预发布；缺失段为空串）
    let 无构建段 = 版本.split_once('+').map_or(版本.as_str(), |(前段, _)| 前段);
    let (核心段, 预发布段) = 无构建段
        .split_once('-')
        .map_or((无构建段, ""), |(前段, 后段)| (前段, 后段));
    let mut 段迭代器 = 核心段.split('.');
    let 主版本 = 段迭代器.next().unwrap_or("").to_string();
    let 次版本 = 段迭代器.next().unwrap_or("").to_string();
    let 修订版本 = 段迭代器.next().unwrap_or("").to_string();
    let 预发布版本 = 预发布段.to_string();
    // 作者：包管理器以冒号连接多作者
    let 作者 = match 包节.and_then(|包值| 包值.get("authors")) {
        Some(toml::Value::Array(列表)) => 列表
            .iter()
            .filter_map(toml::Value::as_str)
            .collect::<Vec<_>>()
            .join(":"),
        Some(toml::Value::String(文本值)) => 文本值.clone(),
        _ => String::new(),
    };
    let mut 变量表 = vec![
        ("CARGO_PKG_NAME".to_string(), 名称),
        ("CARGO_PKG_VERSION".to_string(), 版本),
        ("CARGO_PKG_VERSION_MAJOR".to_string(), 主版本),
        ("CARGO_PKG_VERSION_MINOR".to_string(), 次版本),
        ("CARGO_PKG_VERSION_PATCH".to_string(), 修订版本),
        ("CARGO_PKG_VERSION_PRE".to_string(), 预发布版本),
        ("CARGO_CRATE_NAME".to_string(), 包名下划线.to_string()),
        (
            "CARGO_MANIFEST_DIR".to_string(),
            源码目录.display().to_string(),
        ),
        (
            "CARGO_MANIFEST_PATH".to_string(),
            清单路径.display().to_string(),
        ),
        ("CARGO_PKG_AUTHORS".to_string(), 作者),
        ("CARGO_PKG_DESCRIPTION".to_string(), 清单文本("description")),
        ("CARGO_PKG_HOMEPAGE".to_string(), 清单文本("homepage")),
        ("CARGO_PKG_REPOSITORY".to_string(), 清单文本("repository")),
        ("CARGO_PKG_LICENSE".to_string(), 清单文本("license")),
        (
            "CARGO_PKG_LICENSE_FILE".to_string(),
            清单文本("license-file"),
        ),
    ];
    if let Some(最低版本) = 包节
        .and_then(|包值| 包值.get("rust-version"))
        .and_then(toml::Value::as_str)
    {
        变量表.push(("CARGO_PKG_RUST_VERSION".to_string(), 最低版本.to_string()));
    }
    if let Some(说明文件) = 包节
        .and_then(|包值| 包值.get("readme"))
        .and_then(toml::Value::as_str)
    {
        变量表.push(("CARGO_PKG_README".to_string(), 说明文件.to_string()));
    }
    变量表
}

/// 对单个包手动调用文档工具生成 JSON 文档
///
/// 注入目标包实际启用的特性、构建脚本产物（输出目录 / 条件编译项 /
/// 编译期环境变量）、标准包元数据环境变量与直接依赖的库文件路径
/// （按包标识精确版本），保证条件编译与内联生成出的接口不缺失。
fn 单包文档(
    项目根: &Path,
    包项: &Value,
    包名: &str,
    包标识: &str,
    产物表: &HashMap<String, (String, String, Vec<String>)>,
    直接依赖项: &[(String, String)],
    构建脚本表: &HashMap<String, 构建脚本信息>,
) -> anyhow::Result<String> {
    let 界面表 = crate::界面::界面消息表::全局表();
    let 清单路径 = PathBuf::from(
        包项
            .get("manifest_path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("{}", 界面表.取带参消息("mg_err_no_manifest", &[包名])))?,
    );
    let 源码目录 = 清单路径
        .parent()
        .ok_or_else(|| anyhow!("{}", 界面表.取带参消息("mg_err_no_src_dir", &[包名])))?;
    let 库文件 = 源码目录.join("src/lib.rs");
    if !库文件.exists() {
        anyhow::bail!("{}", 界面表.取带参消息("mg_err_no_lib", &[包名]));
    }
    // 默认特性：包管理器直接传给编译器，不经过构建脚本，
    // 手动文档生成时必须显式补传，否则被条件编译裁掉的接口会缺失
    let 默认特性 = 包项
        .get("features")
        .and_then(|特性值| 特性值.get("default"))
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
        界面表.取带参消息("mg_err_read_manifest", &[&清单路径.display().to_string()])
    })?;
    let 清单: toml::Value = toml::from_str(&清单内容).map_err(|错误值| {
        anyhow!(
            "{}",
            界面表.取带参消息(
                "mg_err_parse_manifest",
                &[&清单路径.display().to_string(), &错误值.to_string()]
            )
        )
    })?;
    let 版次 = 清单
        .get("package")
        .and_then(|包值| 包值.get("edition"))
        .and_then(toml::Value::as_str)
        .unwrap_or("2015")
        .to_string();
    let 过程宏 = 清单
        .get("lib")
        .and_then(|库节| 库节.get("proc-macro"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);

    let 包名下划线 = 包名.replace('-', "_");
    let 文档目录 = 项目根.join("mapping-json");
    fs::create_dir_all(&文档目录)?;
    let mut 命令值 = Command::new(crate::解析文档工具());
    命令值
        .arg(&库文件)
        .arg("--crate-name")
        .arg(&包名下划线)
        .arg("--crate-type")
        .arg(if 过程宏 { "proc-macro" } else { "lib" })
        .arg("--edition")
        .arg(&版次)
        .arg("-L")
        .arg(format!(
            "dependency={}",
            项目根.join("target/debug/deps").display()
        ))
        .env("RUSTC_BOOTSTRAP", "1");
    // 补全包管理器编译时注入的标准环境变量：过程宏在展开期读取包名，
    // 缺失会直接恐慌
    for (键名, 键值) in 构建环境变量(包项, &清单, 包名, &包名下划线, 源码目录, &清单路径)
    {
        命令值.env(键名, 键值);
    }
    // 外部库参数只传直接依赖的精确版本：同名多版本包无法从库目录自动解析，
    // 编译器可能链接错误版本；传递依赖由编译器按库文件元数据散列自动解析
    for (依赖名, 依赖包标识) in 直接依赖项 {
        if let Some((_, 库路径, _)) = 产物表.get(依赖包标识) {
            命令值.arg("--extern").arg(format!("{}={}", 依赖名, 库路径));
        }
    }
    // 注入编译时实际启用的特性作为条件编译项，与编译产物保持一致；
    // 未命中时回退包声明的默认特性
    let 特性表 = 产物表
        .get(包标识)
        .map(|(_, _, is_enabled)| is_enabled.clone())
        .unwrap_or(默认特性);
    for 特性名 in &特性表 {
        命令值.arg("--cfg").arg(format!("feature=\"{}\"", 特性名));
    }
    // 注入目标包构建脚本产物：输出目录、条件编译项、编译期环境变量
    if let Some((输出目录, 条件项表, 环境项表)) = 构建脚本表.get(包标识) {
        if let Some(目录值) = 输出目录 {
            命令值.env("OUT_DIR", 目录值);
        }
        for 条件项 in 条件项表 {
            命令值.arg("--cfg").arg(条件项);
        }
        for 环境项 in 环境项表 {
            if let Some((键名, 键值)) = 环境项.split_once('=') {
                命令值.env(键名, 键值);
            }
        }
    }
    命令值
        .arg("-Z")
        .arg("unstable-options")
        .arg("--output-format")
        .arg("json")
        .arg("--output")
        .arg(&文档目录);
    运行命令(&mut 命令值, &界面表.取消息("mg_cmd_gen_doc"))?;

    let 文档路径 = 文档目录.join(format!("{}.json", 包名下划线));
    fs::read_to_string(&文档路径).with_context(|| {
        界面表.取带参消息("mg_err_no_doc_json", &[&文档路径.display().to_string()])
    })
}

/// 运行命令并返回标准输出；失败时附加标准错误摘要
const 错误摘要行数: usize = 15;
fn 运行命令(命令值: &mut Command, 说明: &str) -> anyhow::Result<String> {
    let 输出值 = 命令值.output().with_context(|| {
        crate::界面::界面消息表::全局表().取带参消息("mg_err_run_failed", &[说明])
    })?;
    if !输出值.status.success() {
        let 错误输出 = String::from_utf8_lossy(&输出值.stderr);
        let 摘要 = 错误输出
            .lines()
            .take(错误摘要行数)
            .collect::<Vec<_>>()
            .join("\n");
        anyhow::bail!(
            "{}",
            crate::界面::界面消息表::全局表().取带参消息("mg_err_failed", &[说明, &摘要])
        );
    }
    let 标准输出文本 = String::from_utf8_lossy(&输出值.stdout).to_string();
    Ok(标准输出文本)
}

#[cfg(test)]
mod 母语测试 {
    use super::super::文档抽取::{提取公开接口, 示例文档};
    use super::*;

    #[test]
    fn 真实工具链提取() {
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

        let 临时守卫 = 临时项目::新建("mini-crate").expect("创建临时项目失败");
        // 覆盖临时项目路径为外壳项目
        let _ = fs::remove_dir_all(临时守卫.存放路径());
        fs::create_dir_all(外壳.join("src")).unwrap();
        let (文档列表值, 解析版本) =
            提取文档结构内部(&临时项目(外壳.clone()), "mini-crate").expect("工具链应能提取文档");
        // 本地路径依赖 0.1.0：解析版本应被捕获（映射文件头基准来源）
        assert_eq!(解析版本.as_deref(), Some("0.1.0"));
        let 条目表 = 提取公开接口(&文档列表值[0].1).unwrap();
        let 名称清单: Vec<&str> = 条目表.iter().map(|条目| 条目.英文名.as_str()).collect();
        assert!(
            名称清单.contains(&"新建"),
            "应提取到函数 新建: {:?}",
            名称清单
        );
        assert!(名称清单.contains(&"Foo"));
        assert!(名称清单.contains(&"颜色"));
        assert!(名称清单.contains(&"行为"));
        assert!(名称清单.contains(&"数量"));
        assert!(名称清单.contains(&"最大值"));
        assert!(!名称清单.contains(&"私有函数"), "私有函数不应被提取");
        // 签名包含类型信息
        let 新建条目 = 条目表.iter().find(|条目| 条目.英文名 == "新建").unwrap();
        assert!(
            新建条目.签名.contains("u32"),
            "签名应含参数类型: {}",
            新建条目.签名
        );
    }

    /// 不存在的包应报错（含未找到提示）
    #[test]
    fn 不存在包报错() {
        // 错误文案按界面语言渲染，语言又取自环境变量：临时接管清除
        // 区域设置与界面语言，保证命中中文模板
        let _锁 = crate::语言管理::母语测试::环境锁();
        let _环境 = crate::语言管理::母语测试::环境还原::接管(&[
            "RZ_LANG",
            "LC_ALL",
            "LC_MESSAGES",
            "LANG",
        ]);
        let 临时 = 临时项目::新建("rzc-不存在的crate-xyz-123").expect("创建临时项目失败");
        // 写入不含目标包的有效最小项目：让元数据查询成功，
        // 从而命中产品逻辑的包未找到映射（否则包管理器先因缺
        // 清单文件报通用错误，根本走不到该分支）
        fs::write(
            临时.存放路径().join("Cargo.toml"),
            "[package]\nname = \"rzc-mapping-temp\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(临时.存放路径().join("src/lib.rs"), "// 空库\n").unwrap();
        let 提取结果 = 提取文档结构内部(&临时, "rzc-不存在的crate-xyz-123");
        let 错误值 = 提取结果.expect_err("应报错");
        assert!(
            错误值.to_string().contains("未找到"),
            "错误应提示未找到: {}",
            错误值
        );
    }

    /// 真实工具链 + 构建脚本（生成被内联的源码）：
    /// 验证输出目录注入，覆盖依赖构建脚本的包
    #[test]
    fn 构建脚本输出目录注入() {
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

        let (文档列表值, _) = 提取文档结构内部(&临时项目(外壳.clone()), "mini-gen")
            .expect("输出目录注入后应能生成文档");
        let 条目表 = 提取公开接口(&文档列表值[0].1).unwrap();
        let 名称清单: Vec<&str> = 条目表.iter().map(|条目| 条目.英文名.as_str()).collect();
        assert!(
            名称清单.contains(&"use_generated_value"),
            "应提取到内联生成代码后的函数: {:?}",
            名称清单
        );
    }

    /// 默认特性注入：被条件编译裁掉的接口需通过条件编译参数恢复
    #[test]
    fn 默认特性注入() {
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

        let (文档列表值, _) =
            提取文档结构内部(&临时项目(外壳.clone()), "mini-feat").expect("应能生成文档");
        let 条目表 = 提取公开接口(&文档列表值[0].1).unwrap();
        let 名称清单: Vec<&str> = 条目表.iter().map(|条目| 条目.英文名.as_str()).collect();
        assert!(
            名称清单.contains(&"magic_fn"),
            "默认特性下的接口应被提取: {:?}",
            名称清单
        );
        assert!(名称清单.contains(&"normal_fn"));
    }

    /// 薄壳包整批重导出追踪：目标包仅做整批重导出，
    /// 其公开接口应通过追踪被重导出的依赖包提取到
    #[test]
    fn 整批重导出追踪() {
        let 临时 = tempfile::tempdir().unwrap();
        // 真实包：提供全部接口
        let 核心 = 临时.path().join("mini-core");
        fs::create_dir_all(核心.join("src")).unwrap();
        fs::write(
            核心.join("Cargo.toml"),
            "[package]\nname = \"mini-core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            核心.join("src/lib.rs"),
            "pub fn handle_request() -> Result<Response, Error> { Ok(Response) }\npub struct Response;\npub enum Error { E }\npub trait Handler {}\n",
        )
        .unwrap();
        // 薄壳包：全部接口来自整批重导出
        let 外壳包 = 临时.path().join("mini-facade");
        fs::create_dir_all(外壳包.join("src")).unwrap();
        fs::write(
            外壳包.join("Cargo.toml"),
            "[package]\nname = \"mini-facade\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-core = { path = \"../mini-core\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            外壳包.join("src/lib.rs"),
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

        let (文档列表值, _) =
            提取文档结构内部(&临时项目(外壳.clone()), "mini-facade").expect("工具链应能提取文档");
        // 目标包 + 被整批重导出的依赖包
        let 包名清单: Vec<&str> = 文档列表值.iter().map(|(名称, _)| 名称.as_str()).collect();
        assert!(
            文档列表值.len() >= 2,
            "应追踪到被重导出的依赖包: {:?}",
            包名清单
        );
        assert!(
            包名清单.contains(&"mini_core"),
            "整批重导出链应包含 mini_core（包名下划线形式）: {:?}",
            包名清单
        );
        // 合并全部 JSON 提取，薄壳包的接口应全部到位
        let mut 条目表 = Vec::new();
        let mut 已见名称 = HashSet::new();
        for (_, 文档文本) in &文档列表值 {
            for 条目值 in 提取公开接口(文档文本).unwrap() {
                if 已见名称.insert(条目值.英文名.clone()) {
                    条目表.push(条目值);
                }
            }
        }
        let 名称清单: Vec<&str> = 条目表.iter().map(|条目| 条目.英文名.as_str()).collect();
        assert!(名称清单.contains(&"handle_request"));
        assert!(名称清单.contains(&"Response"));
        assert!(名称清单.contains(&"Error"));
        assert!(名称清单.contains(&"Handler"));
    }

    /// 构建环境变量：名称/版本分段/作者拼接与空值兜底（包管理器语义）
    #[test]
    fn 构建环境变量取值() {
        let 包项 = serde_json::json!({ "name": "tauri", "version": "2.11.5" });
        let 清单: toml::Value = toml::from_str(
            "[package]\nname = \"tauri\"\nversion = \"2.11.5\"\nauthors = [\"A\", \"B\"]\nlicense = \"MIT\"\nrust-version = \"1.77\"\n",
        )
        .unwrap();
        let 变量表 = 构建环境变量(
            &包项,
            &清单,
            "tauri",
            "tauri",
            Path::new("/src/tauri-2.11.5"),
            Path::new("/src/tauri-2.11.5/Cargo.toml"),
        );
        let 取值 = |键名: &str| {
            变量表
                .iter()
                .find(|(keys, _)| keys == 键名)
                .map(|(_, 值)| 值.as_str())
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
        // 未声明的字段与包管理器一致给空串；未声明最低版本不提供该变量
        assert_eq!(取值("CARGO_PKG_DESCRIPTION"), Some(""));
        let 包项二 = serde_json::json!({ "name": "x", "version": "1.0.0-alpha.1+build.5" });
        let 清单二: toml::Value =
            toml::from_str("[package]\nname = \"x\"\nversion = \"1.0.0-alpha.1+build.5\"\n")
                .unwrap();
        let 变量表二 = 构建环境变量(
            &包项二,
            &清单二,
            "x",
            "x",
            Path::new("/s"),
            Path::new("/s/Cargo.toml"),
        );
        let 取值二 = |键名: &str| {
            变量表二
                .iter()
                .find(|(keys, _)| keys == 键名)
                .map(|(_, 值)| 值.as_str())
        };
        assert_eq!(取值二("CARGO_PKG_VERSION_PRE"), Some("alpha.1"));
        assert_eq!(取值二("CARGO_PKG_VERSION_PATCH"), Some("0"));
        assert_eq!(取值二("CARGO_PKG_RUST_VERSION"), None);
        assert_eq!(取值二("CARGO_PKG_README"), None);
    }

    /// 真实工具链：文档工具在编译期读取包元数据环境变量，
    /// 注入缺失或值错误时常量断言编译失败——保障提取管线补全这些变量
    #[test]
    fn 包元数据环境注入() {
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

        let (文档列表值, _) = 提取文档结构内部(&临时项目(外壳.clone()), "mini-env")
            .expect("包元数据环境变量注入后应能生成文档");
        let 条目表 = 提取公开接口(&文档列表值[0].1).unwrap();
        let 名称清单: Vec<&str> = 条目表.iter().map(|条目| 条目.英文名.as_str()).collect();
        assert!(
            名称清单.contains(&"env_check"),
            "应提取到环境检查函数: {:?}",
            名称清单
        );
    }

    /// 样本 JSON 可解析（样本与提取器在同一包，防漂移）
    #[test]
    fn 样本可解析() {
        let 条目表 = 提取公开接口(&示例文档()).unwrap();
        assert!(!条目表.is_empty());
    }
}
