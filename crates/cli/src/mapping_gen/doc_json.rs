//! 工具链集成：在临时项目中把目标 crate 作为依赖编译，手动调用 rustdoc
//! 生成 JSON 文档（含薄壳 crate 的 glob 重导出追踪与构建脚本产物注入）。

use anyhow::{Context, anyhow, bail};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::rustdoc_extract::extract_public_api_with_glob_sources;

/// 临时项目目录守卫（Drop 时自动清理）
struct TempProject(PathBuf);

impl TempProject {
    /// 创建临时项目目录
    fn new(crate_name: &str) -> anyhow::Result<Self> {
        // 用户隔离 + 符号链接校验（crate 名已由 run_auto_generate 校验为
        // ASCII 标识符字符；仍拼接用户/PID 段保证路径不可预测）
        let path = crate::temp_guard::secure_temp_path(&format!(
            "rzc-mapping-{}-{}-{}",
            crate_name,
            crate::temp_guard::safe_user_segment(),
            std::process::id()
        ))?;
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join("src")).map_err(|e| {
            anyhow::anyhow!(
                "{}",
                crate::ui::Ui::global().f("mg_err_tempdir", &[&e.to_string()])
            )
        })?;
        Ok(TempProject(path))
    }

    /// 获取临时项目路径
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 提取 crate 及其 glob 重导出链上依赖 crate 的 rustdoc JSON 文本列表
///
/// 首个元素为目标 crate 本身；后续元素为被 glob 重导出（`pub use 依赖::*`）
/// 的依赖 crate——薄壳 crate（如 salvo）的公开 API 全部来自这些 crate。
pub fn extract_crate_doc(crate_name: &str) -> anyhow::Result<Vec<(String, String)>> {
    let temp = TempProject::new(crate_name)?;
    // 1. 临时项目：把目标 crate 作为唯一依赖（* 允许任意已发布版本）
    fs::write(
        temp.path().join("Cargo.toml"),
        format!(
            "[package]\nname = \"rzc-mapping-temp\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n\"{}\" = \"*\"\n\n[workspace]\n",
            crate_name
        ),
    )?;
    fs::write(
        temp.path().join("src/lib.rs"),
        "// 仅供提取依赖 API 的空库\n",
    )?;
    extract_doc_json_internal(&temp, crate_name)
}

/// 工具链核心（对测试开放）：在给定临时项目中定位 crate 并生成其 rustdoc JSON
///
/// 1. `cargo metadata` 定位目标 crate 的源码目录（registry 或本地 path 依赖均可）
/// 2. `cargo build` 编译依赖树，解析每个依赖的 .rlib/.so 路径
/// 3. 手动调用 `rustdoc -Z unstable-options --output-format json` 文档化目标 crate
/// 4. 薄壳 crate（meta crate，如 salvo 仅 `pub use salvo_core::*`）的公开 API
///    来自 glob 重导出：解析每个 crate 的 glob 重导出（inner.use.is_glob）的
///    source 路径，对被重导出的依赖 crate 也生成文档并继续追踪，直至闭环。
///    返回 (crate 名, JSON 文本) 列表，首个为目标 crate 本身。
fn extract_doc_json_internal(
    temp: &TempProject,
    crate_name: &str,
) -> anyhow::Result<Vec<(String, String)>> {
    let project_root = temp.path();
    let ui = crate::ui::Ui::global();

    // 1. metadata：定位目标 crate 的 manifest
    let metadata_output = run_command(
        Command::new(crate::resolve_cargo())
            .arg("metadata")
            .arg("--format-version")
            .arg("1")
            .current_dir(project_root),
        &ui.t("mg_cmd_parse_deps"),
    )?;
    let metadata: Value = serde_json::from_str(&metadata_output)
        .map_err(|e| anyhow!("{}", ui.f("mg_err_parse_meta", &[&e.to_string()])))?;
    // 包元数据索引（manifest_path / features / edition），供 glob 重导出链上的
    // 依赖 crate 查询（薄壳 crate 的 API 在其依赖中）。同时索引 package 名
    // （CLI 参数，如 mini-core）与 lib target 的 crate 名（rustdoc glob 重导出
    // source，如 mini_core），两者在连字符/下划线写法上可能不同。
    let mut pkg_index: HashMap<String, Value> = HashMap::new();
    for p in metadata["packages"].as_array().into_iter().flatten() {
        if let Some(pkg_name) = p.get("name").and_then(Value::as_str) {
            pkg_index.insert(pkg_name.to_string(), p.clone());
        }
        if let Some(lib_name) = p
            .get("targets")
            .and_then(Value::as_array)
            .and_then(|ts| {
                ts.iter().find(|t| {
                    t.get("kind")
                        .and_then(Value::as_array)
                        .map(|k| k.iter().any(|v| v.as_str() == Some("lib")))
                        == Some(true)
                })
            })
            .and_then(|t| t.get("name"))
            .and_then(Value::as_str)
        {
            pkg_index.insert(lib_name.to_string(), p.clone());
        }
    }
    if !pkg_index.contains_key(crate_name) {
        bail!("{}", ui.f("mg_err_crate_not_found", &[crate_name]));
    }

    // resolve.nodes：完整依赖解析图（含多版本共存时的精确解析），按
    // package_id 索引每个 crate 的直接依赖（别名, 依赖 package_id）。
    // --extern 只传直接依赖的精确版本；传递依赖由 rustc 按 rlib 元数据
    // hash 在 -L 目录中自动解析（同名多版本 crate 无法从 -L 手动解析）
    let mut direct_dep_table: HashMap<String, Vec<(String, String)>> = HashMap::new();
    if let Some(nodes) = metadata
        .get("resolve")
        .and_then(|r| r.get("nodes"))
        .and_then(Value::as_array)
    {
        for node in nodes {
            let Some(node_id) = node.get("id").and_then(Value::as_str) else {
                continue;
            };
            let mut deps: Vec<(String, String)> = Vec::new();
            if let Some(dep_list) = node.get("deps").and_then(Value::as_array) {
                for dep in dep_list {
                    let Some(dep_name) = dep.get("name").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some(dep_pkg) = dep.get("pkg").and_then(Value::as_str) else {
                        continue;
                    };
                    // 只保留 normal 依赖（dev/build 依赖不进入 lib 的 extern prelude）；
                    // 目标特定依赖（cfg(...)）无法静态判定，保守保留
                    let is_normal = dep
                        .get("dep_kinds")
                        .and_then(Value::as_array)
                        .map(|kinds| {
                            kinds
                                .iter()
                                .any(|k| k.get("kind").is_none_or(|v| v.is_null()))
                        })
                        .unwrap_or(true);
                    if is_normal {
                        deps.push((dep_name.to_string(), dep_pkg.to_string()));
                    }
                }
            }
            direct_dep_table.insert(node_id.to_string(), deps);
        }
    }

    // 2. cargo build：编译依赖树，解析依赖 .rlib/.so 路径
    let build_output = run_command(
        Command::new(crate::resolve_cargo())
            .arg("build")
            .arg("--message-format=json")
            .current_dir(project_root),
        &ui.t("mg_cmd_build_deps"),
    )?;
    // package_id -> (lib target 名, .rlib/.so 路径, 实际启用的 features)。
    // 按 package_id 索引而非 target 名：依赖树中同名不同版本的 crate 共存时
    // （如 rand 0.8.7 与 0.10.2），按名字覆盖会链接错误版本；features 取 cargo
    // build 的 feature 统一解析结果而非包声明的 default——依赖方可能未启用部分
    // default features（如 salvo 不启用 salvo_core 的 unix），注入不一致的 cfg
    // 会导致编译失败（缺 nix 等依赖）
    let mut artifact_table: HashMap<String, (String, String, Vec<String>)> = HashMap::new();
    // 已编译 lib 的 crate 名集合（glob 重导出链入队检查用）
    let mut compiled_lib_names: HashSet<String> = HashSet::new();
    // package_id -> 构建脚本产物（见模块级 [`BuildScriptInfo`] 类型注释）
    let mut build_script_table: HashMap<String, BuildScriptInfo> = HashMap::new();
    for line in build_output.lines() {
        let Ok(msg) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(reason) = msg.get("reason").and_then(Value::as_str) else {
            continue;
        };
        if reason != "compiler-artifact" && reason != "build-script-executed" {
            continue;
        }
        let Some(pkg_id) = msg.get("package_id").and_then(Value::as_str) else {
            continue;
        };
        if reason == "build-script-executed" {
            let out_dir = msg.get("out_dir").and_then(Value::as_str).map(String::from);
            let cfgs = msg
                .get("cfgs")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            let env_vars = msg
                .get("env")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            build_script_table.insert(pkg_id.to_string(), (out_dir, cfgs, env_vars));
            continue;
        }
        // compiler-artifact：按 package_id 索引（与 metadata packages[].id 一致），
        // 多版本共存时精确关联到具体版本；target.name 仅作为 lib 名记录
        let Some(target_name) = msg
            .get("target")
            .and_then(|t| t.get("name"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        // 只保留 ASCII 合法标识符的 crate 名（本地 workspace 成员如中文名项目会被排除）
        if target_name.is_empty()
            || target_name == "rzc-mapping-temp"
            || !target_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            continue;
        }
        if let Some(filenames) = msg.get("filenames").and_then(Value::as_array)
            && let Some(file) = filenames
                .iter()
                .filter_map(Value::as_str)
                .find(|f| f.ends_with(".rlib") || f.ends_with(".so"))
        {
            let features = msg
                .get("features")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            artifact_table.insert(
                pkg_id.to_string(),
                (target_name.to_string(), file.to_string(), features),
            );
            compiled_lib_names.insert(target_name.to_string());
        }
    }
    if artifact_table.is_empty() {
        bail!("{}", ui.f("mg_err_build_failed", &[crate_name]));
    }

    // 3. rustdoc 队列：目标 crate → glob 重导出链上的依赖 crate。
    //    薄壳 crate（如 salvo）的 index 只有 re-export 节点，公开 API 全部
    //    来自被重导出的依赖 crate（如 salvo_core），逐个生成文档后合并提取。
    let mut queue: Vec<String> = vec![crate_name.to_string()];
    let mut visited: HashSet<String> = HashSet::new();
    let mut results: Vec<(String, String)> = Vec::new();
    while let Some(name) = queue.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let Some(pkg) = pkg_index.get(&name) else {
            continue;
        };
        let pkg_id = pkg
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        // 直接依赖（别名, 精确版本 package_id）：--extern 只传这些
        let direct_deps = direct_dep_table.get(&pkg_id).cloned().unwrap_or_default();
        let json_text = rustdoc_single(
            project_root,
            pkg,
            &name,
            &pkg_id,
            &artifact_table,
            &direct_deps,
            &build_script_table,
        )?;
        // glob 重导出（pub use 依赖::*）→ 被重导出 crate 名（source 首段），
        // 若在依赖树中则入队继续提取；feature 未启用而未编译的依赖自动跳过
        let (_, glob_sources) = extract_public_api_with_glob_sources(&json_text)?;
        for source in glob_sources {
            if compiled_lib_names.contains(&source) && !visited.contains(&source) {
                queue.push(source);
            }
        }
        results.push((name, json_text));
    }
    Ok(results)
}

/// 构建脚本产物：OUT_DIR（include! 生成代码）、cfg（条件编译）、rustc-env 列表。
/// 部分 crate（如 serde）的 build.rs 会生成 include! 的源码或声明 cfg，
/// 手动 rustdoc 时必须注入，否则编译失败（如 OUT_DIR 未定义）。
type BuildScriptInfo = (Option<String>, Vec<String>, Vec<String>);

/// 对单个 crate 手动调用 rustdoc 生成 JSON 文档
///
/// 注入目标 crate 实际启用的 features（cfg）、构建脚本产物（OUT_DIR / cfg /
/// rustc-env）与直接依赖的 .rlib/.so 路径（--extern，按 package_id 精确版本），
/// 保证 cfg(feature) 与 include! 生成的 API 不缺失。
fn rustdoc_single(
    project_root: &Path,
    pkg: &Value,
    crate_name: &str,
    pkg_id: &str,
    artifact_table: &HashMap<String, (String, String, Vec<String>)>,
    direct_deps: &[(String, String)],
    build_script_table: &HashMap<String, BuildScriptInfo>,
) -> anyhow::Result<String> {
    let ui = crate::ui::Ui::global();
    let manifest_path = PathBuf::from(
        pkg.get("manifest_path")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("{}", ui.f("mg_err_no_manifest", &[crate_name])))?,
    );
    let source_dir = manifest_path
        .parent()
        .ok_or_else(|| anyhow!("{}", ui.f("mg_err_no_src_dir", &[crate_name])))?;
    let lib_file = source_dir.join("src/lib.rs");
    if !lib_file.exists() {
        bail!("{}", ui.f("mg_err_no_lib", &[crate_name]));
    }
    // 默认 features：cargo 直接传给 rustc（--cfg feature=...），不经过 build script，
    // 手动 rustdoc 时必须显式补传，否则 cfg(feature) 裁掉的 API 会缺失
    let default_features = pkg
        .get("features")
        .and_then(|f| f.get("default"))
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let manifest_content = fs::read_to_string(&manifest_path).with_context(|| {
        ui.f(
            "mg_err_read_manifest",
            &[&manifest_path.display().to_string()],
        )
    })?;
    let manifest: toml::Value = toml::from_str(&manifest_content).map_err(|e| {
        anyhow!(
            "{}",
            ui.f(
                "mg_err_parse_manifest",
                &[&manifest_path.display().to_string(), &e.to_string()]
            )
        )
    })?;
    let edition = manifest
        .get("package")
        .and_then(|p| p.get("edition"))
        .and_then(toml::Value::as_str)
        .unwrap_or("2015")
        .to_string();
    let proc_macro = manifest
        .get("lib")
        .and_then(|l| l.get("proc-macro"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);

    let crate_name_underscore = crate_name.replace('-', "_");
    let json_dir = project_root.join("mapping-json");
    fs::create_dir_all(&json_dir)?;
    let mut cmd = Command::new("rustdoc");
    cmd.arg(&lib_file)
        .arg("--crate-name")
        .arg(&crate_name_underscore)
        .arg("--crate-type")
        .arg(if proc_macro { "proc-macro" } else { "lib" })
        .arg("--edition")
        .arg(&edition)
        .arg("-L")
        .arg(format!(
            "dependency={}",
            project_root.join("target/debug/deps").display()
        ))
        .env("RUSTC_BOOTSTRAP", "1");
    // --extern 只传直接依赖的精确版本：同名多版本 crate（如 rand 0.8.7 与
    // 0.10.2 共存）无法从 -L 目录自动解析，rustc 可能链接错误版本；传递依赖
    // 由 rustc 按 rlib 元数据 hash 在 -L 中自动解析
    for (dep_name, dep_pkg_id) in direct_deps {
        if let Some((_, path, _)) = artifact_table.get(dep_pkg_id) {
            cmd.arg("--extern").arg(format!("{}={}", dep_name, path));
        }
    }
    // 注入 cargo build 实际启用的 features（feature 统一解析结果）作为 cfg，
    // 与编译产物保持一致；未命中时回退包声明的 default features
    let features = artifact_table
        .get(pkg_id)
        .map(|(_, _, f)| f.clone())
        .unwrap_or(default_features);
    for feature in &features {
        cmd.arg("--cfg").arg(format!("feature=\"{}\"", feature));
    }
    // 注入目标 crate 构建脚本产物：OUT_DIR（include! 生成代码）、cfg（条件编译）、rustc-env
    if let Some((out_dir, cfgs, env_vars)) = build_script_table.get(pkg_id) {
        if let Some(dir) = out_dir {
            cmd.env("OUT_DIR", dir);
        }
        for cfg in cfgs {
            cmd.arg("--cfg").arg(cfg);
        }
        for item in env_vars {
            if let Some((key, value)) = item.split_once('=') {
                cmd.env(key, value);
            }
        }
    }
    cmd.arg("-Z")
        .arg("unstable-options")
        .arg("--output-format")
        .arg("json")
        .arg("--output")
        .arg(&json_dir);
    run_command(&mut cmd, &ui.t("mg_cmd_gen_doc"))?;

    let json_path = json_dir.join(format!("{}.json", crate_name_underscore));
    fs::read_to_string(&json_path)
        .with_context(|| ui.f("mg_err_no_doc_json", &[&json_path.display().to_string()]))
}

/// 运行命令并返回 stdout；失败时附加 stderr 摘要
const ERROR_SUMMARY_LINES: usize = 15;
fn run_command(cmd: &mut Command, description: &str) -> anyhow::Result<String> {
    let output = cmd
        .output()
        .with_context(|| crate::ui::Ui::global().f("mg_err_run_failed", &[description]))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let summary = stderr
            .lines()
            .take(ERROR_SUMMARY_LINES)
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "{}",
            crate::ui::Ui::global().f("mg_err_failed", &[description, &summary])
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::super::rustdoc_extract::{extract_public_api, sample_json};
    use super::*;
    use std::collections::HashSet as TestHashSet;

    #[test]
    fn test_real_toolchain_extraction() {
        let temp = tempfile::tempdir().unwrap();
        let mini = temp.path().join("mini-crate");
        fs::create_dir_all(mini.join("src")).unwrap();
        fs::write(
            mini.join("Cargo.toml"),
            "[package]\nname = \"mini-crate\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            mini.join("src/lib.rs"),
            "pub fn 新建(x: u32) -> Result<Foo, String> { Ok(Foo) }\npub struct Foo;\npub enum 颜色 { 红, 蓝 }\npub trait 行为 {}\npub type 数量 = u32;\npub const 最大值: u32 = 100;\nfn 私有函数() {}\n",
        )
        .unwrap();
        let shell = temp.path().join("外壳");
        fs::create_dir_all(shell.join("src")).unwrap();
        fs::write(
            shell.join("Cargo.toml"),
            "[package]\nname = \"外壳\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-crate = { path = \"../mini-crate\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(shell.join("src/lib.rs"), "// 空库\n").unwrap();

        let temp_guard = TempProject::new("mini-crate").expect("创建临时项目失败");
        // 覆盖临时项目路径为外壳项目
        let _ = fs::remove_dir_all(temp_guard.path());
        fs::create_dir_all(shell.join("src")).unwrap();
        let doc_jsons = extract_doc_json_internal(&TempProject(shell.clone()), "mini-crate")
            .expect("工具链应能提取文档");
        let entries = extract_public_api(&doc_jsons[0].1).unwrap();
        let name_list: Vec<&str> = entries.iter().map(|e| e.english_name.as_str()).collect();
        assert!(
            name_list.contains(&"新建"),
            "应提取到函数 新建: {:?}",
            name_list
        );
        assert!(name_list.contains(&"Foo"));
        assert!(name_list.contains(&"颜色"));
        assert!(name_list.contains(&"行为"));
        assert!(name_list.contains(&"数量"));
        assert!(name_list.contains(&"最大值"));
        assert!(!name_list.contains(&"私有函数"), "私有函数不应被提取");
        // 签名包含类型信息
        let new_fn = entries.iter().find(|e| e.english_name == "新建").unwrap();
        assert!(
            new_fn.signature.contains("u32"),
            "签名应含参数类型: {}",
            new_fn.signature
        );
    }

    /// 不存在的 crate 应报错（含"未找到"提示）
    #[test]
    fn test_nonexistent_crate_error() {
        let temp = TempProject::new("rzc-不存在的crate-xyz-123").expect("创建临时项目失败");
        let result = extract_doc_json_internal(&temp, "rzc-不存在的crate-xyz-123");
        let err = result.expect_err("应报错");
        assert!(
            err.to_string().contains("未找到") || err.to_string().contains("失败"),
            "错误应提示未找到: {}",
            err
        );
    }

    /// 真实工具链 + 构建脚本（build.rs 生成 include! 源码）：
    /// 验证 OUT_DIR 注入，覆盖 serde 等依赖 build.rs 的 crate
    #[test]
    fn test_build_script_out_dir_injection() {
        let temp = tempfile::tempdir().unwrap();
        let mini = temp.path().join("mini-gen");
        fs::create_dir_all(mini.join("src")).unwrap();
        fs::write(
            mini.join("Cargo.toml"),
            "[package]\nname = \"mini-gen\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[build-dependencies]\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            mini.join("build.rs"),
            "fn main() {\n    let dir = std::env::var(\"OUT_DIR\").unwrap();\n    std::fs::write(std::path::Path::new(&dir).join(\"generated.rs\"), \"pub const GENERATED_VALUE: u32 = 42;\\n\").unwrap();\n}\n",
        )
        .unwrap();
        fs::write(
            mini.join("src/lib.rs"),
            "include!(concat!(env!(\"OUT_DIR\"), \"/generated.rs\"));\npub fn use_generated_value() -> u32 { GENERATED_VALUE }\n",
        )
        .unwrap();
        let shell = temp.path().join("外壳2");
        fs::create_dir_all(shell.join("src")).unwrap();
        fs::write(
            shell.join("Cargo.toml"),
            "[package]\nname = \"外壳2\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-gen = { path = \"../mini-gen\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(shell.join("src/lib.rs"), "// 空库\n").unwrap();

        let doc_jsons = extract_doc_json_internal(&TempProject(shell.clone()), "mini-gen")
            .expect("OUT_DIR 注入后应能生成文档");
        let entries = extract_public_api(&doc_jsons[0].1).unwrap();
        let name_list: Vec<&str> = entries.iter().map(|e| e.english_name.as_str()).collect();
        assert!(
            name_list.contains(&"use_generated_value"),
            "应提取到 include! 生成的代码后的函数: {:?}",
            name_list
        );
    }

    /// 默认 feature 注入：cfg(feature) 裁掉的 API 需通过 --cfg feature=... 恢复
    #[test]
    fn test_default_feature_injection() {
        let temp = tempfile::tempdir().unwrap();
        let mini = temp.path().join("mini-feat");
        fs::create_dir_all(mini.join("src")).unwrap();
        fs::write(
            mini.join("Cargo.toml"),
            "[package]\nname = \"mini-feat\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[features]\ndefault = [\"magic\"]\n\"magic\" = []\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            mini.join("src/lib.rs"),
            "pub fn normal_fn() {}\n#[cfg(feature = \"magic\")]\npub fn magic_fn() {}\n",
        )
        .unwrap();
        let shell = temp.path().join("外壳3");
        fs::create_dir_all(shell.join("src")).unwrap();
        fs::write(
            shell.join("Cargo.toml"),
            "[package]\nname = \"外壳3\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-feat = { path = \"../mini-feat\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(shell.join("src/lib.rs"), "// 空库\n").unwrap();

        let doc_jsons = extract_doc_json_internal(&TempProject(shell.clone()), "mini-feat")
            .expect("应能生成文档");
        let entries = extract_public_api(&doc_jsons[0].1).unwrap();
        let name_list: Vec<&str> = entries.iter().map(|e| e.english_name.as_str()).collect();
        assert!(
            name_list.contains(&"magic_fn"),
            "默认 feature 下的 API 应被提取: {:?}",
            name_list
        );
        assert!(name_list.contains(&"normal_fn"));
    }

    /// 薄壳 crate（meta crate）glob 重导出追踪：目标 crate 仅 `pub use 依赖::*`，
    /// 其公开 API 应通过追踪被重导出的依赖 crate 提取到（如 salvo → salvo_core）
    #[test]
    fn test_glob_reexport_tracking() {
        let temp = tempfile::tempdir().unwrap();
        // 真实 crate：mini-core 提供全部 API
        let core = temp.path().join("mini-core");
        fs::create_dir_all(core.join("src")).unwrap();
        fs::write(
            core.join("Cargo.toml"),
            "[package]\nname = \"mini-core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            core.join("src/lib.rs"),
            "pub fn handle_request() -> Result<Response, Error> { Ok(Response) }\npub struct Response;\npub enum Error { E }\npub trait Handler {}\n",
        )
        .unwrap();
        // 薄壳 crate：全部 API 来自 glob 重导出（与 salvo 的 lib.rs 结构一致）
        let facade = temp.path().join("mini-facade");
        fs::create_dir_all(facade.join("src")).unwrap();
        fs::write(
            facade.join("Cargo.toml"),
            "[package]\nname = \"mini-facade\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-core = { path = \"../mini-core\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(
            facade.join("src/lib.rs"),
            "pub use mini_core::*;\npub use mini_core as core;\n",
        )
        .unwrap();
        let shell = temp.path().join("外壳4");
        fs::create_dir_all(shell.join("src")).unwrap();
        fs::write(
            shell.join("Cargo.toml"),
            "[package]\nname = \"外壳4\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nmini-facade = { path = \"../mini-facade\" }\n\n[workspace]\n",
        )
        .unwrap();
        fs::write(shell.join("src/lib.rs"), "// 空库\n").unwrap();

        let doc_jsons = extract_doc_json_internal(&TempProject(shell.clone()), "mini-facade")
            .expect("工具链应能提取文档");
        // 目标 crate + 被 glob 重导出的依赖 crate（mini-core）
        let crate_names: Vec<&str> = doc_jsons.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            doc_jsons.len() >= 2,
            "应追踪到被重导出的依赖 crate: {:?}",
            crate_names
        );
        assert!(
            crate_names.contains(&"mini_core"),
            "glob 重导出链应包含 mini_core（crate 名下划线形式）: {:?}",
            crate_names
        );
        // 合并全部 JSON 提取，薄壳 crate 的 API 应全部到位
        let mut entries = Vec::new();
        let mut seen_names = TestHashSet::new();
        for (_, json_text) in &doc_jsons {
            for entry in extract_public_api(json_text).unwrap() {
                if seen_names.insert(entry.english_name.clone()) {
                    entries.push(entry);
                }
            }
        }
        let name_list: Vec<&str> = entries.iter().map(|e| e.english_name.as_str()).collect();
        assert!(name_list.contains(&"handle_request"));
        assert!(name_list.contains(&"Response"));
        assert!(name_list.contains(&"Error"));
        assert!(name_list.contains(&"Handler"));
    }

    /// 样本 JSON 可解析（样本与提取器在同一 crate，防漂移）
    #[test]
    fn test_sample_json_parses() {
        let entries = extract_public_api(&sample_json()).unwrap();
        assert!(!entries.is_empty());
    }
}
