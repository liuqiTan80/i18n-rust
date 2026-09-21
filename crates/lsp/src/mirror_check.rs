//! 真实项目镜像 cargo check（权威诊断）
//!
//! 虚拟项目只聚合方言模块、无第三方依赖，与真实工程结构偏差大；
//! 本模块把整个项目树复制为「镜像」（排除 target/.git/node_modules/dist），
//! 按 rzc 的规则把 src/ 下的方言文件转译为 .rs 产物：
//! - 入口方言源（src/main.zh，或与现有 src/main.rs 最相似的方言文件）
//!   转译后写入镜像的 src/main.rs，非 ASCII 文件式模块声明补 `#[path]` 注解；
//! - 其余方言文件转译为同名 .rs 产物（与 rzc 的 transpile_project_files 一致）。
//!
//! 镜像目录位于系统临时目录顶层（`i18n_lsp_mirror_<用户>_<PID>_<项目哈希>`），
//! 与虚拟项目目录同层隔离：虚拟项目本身是 cargo 包，镜像嵌入其下会被
//! cargo 判定为“在 workspace 中却未被声明”而拒绝检查（父目录 Cargo.toml
//! 构成 workspace 根）；顶层目录同时避免 rust-analyzer 把镜像树当作
//! 工作区文件扫描。残留目录由下次启动的虚拟目录清理机制一并清扫。
//!
//! 在镜像中执行 `cargo check --offline --message-format=json`，
//! `--target-dir` 指向真实项目的 target 目录，依赖编译产物零重复（镜像
//! 本地 crate 的 fingerprint 含项目路径，与真实项目互不干扰）。
//! rustc 诊断经行/列回译（含 `#[path]` 注解行的行号换算）还原为方言坐标，
//! 消息翻译与所有权详情提取后按文件发布；镜像不可用（无 Cargo.toml、
//! 复制失败、cargo 无法启动）时返回 false，由调用方回退虚拟项目检查。
//!
//! 打开中的方言文档以编辑器缓冲区内容为准（内存最新），其余取磁盘内容。
//! 与 rzc 构建的规则唯一来源为引擎（转译管线、`#[path]` 注解、列映射）。

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::response_map::diag_text::{
    extract_ownership_details, inject_teaching_diags, translate_diagnostic_message,
};
use crate::translation_cache::{TranslationCache, path_to_uri};

/// 镜像树复制时排除的目录名（构建产物/版本库/依赖缓存）
const EXCLUDED_DIRS: [&str; 4] = ["target", ".git", "node_modules", "dist"];

/// 镜像检查轮询超时：600s（含真实依赖编译，首轮可能数分钟）
const CHECK_TIMEOUT_MS: u64 = 600_000;
/// 轮询间隔
const POLL_INTERVAL_MS: u64 = 200;

/// 单个镜像转译文件：方言源 ↔ 镜像产物坐标映射
struct MirrorFile {
    /// 镜像中的产物路径（src/main.rs 或同名 .rs；诊断归位用）
    product_path: PathBuf,
    /// 真实项目中的方言源 URI（发布诊断用）
    original_uri: String,
    /// 方言源内容（打开文档取缓冲区，否则取磁盘；同行列换算用）
    zh_content: String,
    /// 引擎列映射（产物字符列 → 方言字符列；本模块自行转译，与 CLI 同源）
    column_map: i18n_rust_engine::column_map::ColumnMap,
    /// 入口产物行（磁盘行，0-based 索引）→ 引擎直出行映射；
    /// 仅入口文件有 `#[path]` 注解插入行，其余为恒等（None）
    entry_line_map: Option<Vec<usize>>,
}

/// 待写盘的镜像转译产物（入口识别需全部产物就绪，故先在内存中转译）
struct Pending {
    /// 镜像中的方言源路径（产物路径由它推导）
    mirror_path: PathBuf,
    /// 引擎转译输出（未补 `#[path]` 注解）
    output: String,
    /// 引擎列映射（产物字符列 → 方言字符列）
    column_map: i18n_rust_engine::column_map::ColumnMap,
    /// 方言源内容（打开文档取缓冲区，否则取磁盘）
    zh_content: String,
}

/// 执行一次镜像检查并发布权威诊断
///
/// 返回 true 表示镜像检查确实执行（无论项目有无诊断）；
/// false 表示镜像不可用，调用方回退虚拟项目检查（原有行为）。
pub(crate) fn run_mirror_check(
    cache: &Arc<TranslationCache>,
    sender: &crossbeam_channel::Sender<lsp_server::Message>,
    builtin_diags: &Arc<Mutex<HashMap<String, Vec<Value>>>>,
    extensions: &[String],
    origin_uri: &str,
    known_words: &HashSet<String>,
) -> bool {
    // 触发文件须在缓存中（didOpen/didSave 均已入库），据此定位项目根
    let Some(origin_path) = cache
        .query_original(origin_uri)
        .map(|e| e.original_path.clone())
    else {
        return false;
    };
    let Some(project_root) = find_project_root(&origin_path) else {
        return false; // 非 cargo 项目（单文件教学场景）
    };
    if !origin_path.starts_with(&project_root) {
        return false;
    }

    // 镜像目录：系统临时目录顶层、按 用户+PID+项目哈希 隔离；每次全量重建
    // （删除/改名等磁盘变化自动反映，无需增量跟踪）
    let mirror_dir = mirror_dir_for(&project_root);
    let _ = std::fs::remove_dir_all(&mirror_dir);
    if let Err(e) = copy_project_tree(&project_root, &mirror_dir) {
        log::warn!(
            "镜像复制失败（回退虚拟检查）：{}：{e}",
            project_root.display()
        );
        let _ = std::fs::remove_dir_all(&mirror_dir);
        return false;
    }

    // 方言源转译：入口检测 → 写产物 → 返回产物坐标映射
    let Some(files) = transpile_into_mirror(cache, extensions, &project_root, &mirror_dir) else {
        let _ = std::fs::remove_dir_all(&mirror_dir);
        return false;
    };

    // cargo check：--offline 防索引网络访问卡死；--target-dir 复用真实项目
    // 的依赖编译产物（镜像仅重编本地 crate）
    let target_dir = project_root.join("target");
    let Some((stdout, ok)) = run_cargo_check(&mirror_dir, &target_dir) else {
        return false;
    };

    let (seen, published) = publish_rustc_diagnostics(
        &stdout,
        &mirror_dir,
        &files,
        cache,
        known_words,
        builtin_diags,
        sender,
    );
    if !ok && seen == 0 {
        // 无任何可归位诊断却异常退出：cargo 层失败（清单/离线缺依赖等），
        // 镜像结论不可信，回退虚拟检查
        log::warn!("镜像检查异常退出且无诊断，回退虚拟检查");
        return false;
    }
    log::info!(
        "镜像检查完成：{} 个方言文件，{} 条编译消息，发布 {} 个文件",
        files.len(),
        seen,
        published
    );
    true
}

/// 定位项目根：触发文件最近的上层 Cargo.toml 所在目录
fn find_project_root(origin: &Path) -> Option<PathBuf> {
    let mut dir = origin.parent()?;
    loop {
        if dir.join("Cargo.toml").is_file() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

/// 项目根路径哈希（镜像目录名；同项目复用同一目录，跨会话互不串扰）
fn path_hash(path: &Path) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    path.hash(&mut h);
    h.finish()
}

/// 镜像目录：系统临时目录下、用户名+PID+项目哈希隔离的独立顶层目录
///
/// 不能放在虚拟项目目录之下：虚拟项目本身是 cargo 包（含 Cargo.toml），
/// 嵌套其中的镜像包会被 cargo 判定为“在 workspace 中却未被声明”而拒绝
/// 检查（父目录 Cargo.toml 构成 workspace 根）；顶层独立目录同时避免
/// rust-analyzer 把镜像树当作工作区文件扫描。
fn mirror_dir_for(project_root: &Path) -> PathBuf {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "default".to_string());
    let safe_user: String = user
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    std::env::temp_dir().join(format!(
        "i18n_lsp_mirror_{}_{}_{:x}",
        safe_user,
        std::process::id(),
        path_hash(project_root)
    ))
}

/// 递归复制项目树（排除构建产物/版本库；目录符号链接跳过防逃逸与循环，
/// 文件符号链接跟随复制单文件）
fn copy_project_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for dent in std::fs::read_dir(from)?.flatten() {
        let path = dent.path();
        let Ok(ft) = dent.file_type() else { continue };
        let Ok(meta) = dent.metadata() else { continue };
        let name = dent.file_name();
        let name = name.to_string_lossy();
        if meta.is_dir() {
            if ft.is_symlink() || EXCLUDED_DIRS.contains(&name.as_ref()) {
                continue;
            }
            copy_project_tree(&path, &to.join(name.as_ref()))?;
        } else if meta.is_file() {
            std::fs::copy(&path, to.join(name.as_ref()))?;
        }
    }
    Ok(())
}

/// 收集目录下全部方言源文件（递归；路径排序保证确定性）
fn collect_dialect_paths(dir: &Path, extensions: &[String], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for dent in entries.flatten() {
        let path = dent.path();
        let Ok(ft) = dent.file_type() else { continue };
        if ft.is_dir() {
            collect_dialect_paths(&path, extensions, out);
        } else if ft.is_file() && file_matches_extensions(&path, extensions) {
            out.push(path);
        }
    }
}

/// 路径扩展名是否属于支持的方言扩展名（如 `.zh`）
fn file_matches_extensions(path: &Path, extensions: &[String]) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let dotted = format!(".{ext}");
    extensions.iter().any(|e| e == &dotted || e == ext)
}

/// 把镜像 src/ 下的方言源转译为 .rs 产物（入口 → main.rs），返回产物映射表
///
/// 转译与 rzc 同规则：模块名集合 = src/ 顶层方言文件词干（`crate::` 前缀
/// 限定用）、项目声明上下文来自全部方言源（跨文件声明豁免）、入口产物补
/// 非 ASCII 模块 `#[path]` 注解。打开中的文档以缓冲区内容为准。
fn transpile_into_mirror(
    cache: &Arc<TranslationCache>,
    extensions: &[String],
    project_root: &Path,
    mirror_dir: &Path,
) -> Option<Vec<MirrorFile>> {
    let mirror_src = mirror_dir.join("src");
    if !mirror_src.is_dir() {
        return None;
    }
    // 方言源清单：src/ 递归（覆盖 rzc 的顶层扫描；嵌套文件经 #[path] 引用）
    let mut dialect_paths: Vec<PathBuf> = Vec::new();
    collect_dialect_paths(&mirror_src, extensions, &mut dialect_paths);
    dialect_paths.sort();
    if dialect_paths.is_empty() {
        return None;
    }

    // 模块名集合：src/ 顶层方言文件词干（与 CLI collect_project_context 同规则）
    let mut module_names: HashSet<String> = HashSet::new();
    // 方言源内容：打开文档取缓冲区，其余取磁盘
    let mut sources: HashMap<PathBuf, String> = HashMap::new();
    for mirror_path in &dialect_paths {
        let Ok(rel) = mirror_path.strip_prefix(mirror_dir) else {
            continue;
        };
        // 打开文档取缓冲区内容：按路径查询（容忍客户端 URI 与规范化
        // 路径的表示差异——Windows 上字符串键匹配会失效）
        let content = cache
            .query_by_path(&project_root.join(rel))
            .filter(|e| e.is_open)
            .map(|e| e.zh_content.clone())
            .or_else(|| std::fs::read_to_string(mirror_path).ok());
        let Some(content) = content else { continue };
        if mirror_path.parent() == Some(mirror_src.as_path())
            && let Some(stem) = mirror_path.file_stem().and_then(|s| s.to_str())
        {
            module_names.insert(stem.to_string());
        }
        sources.insert(mirror_path.clone(), content);
    }
    if sources.is_empty() {
        return None;
    }

    let project = i18n_rust_engine::alias::ProjectContext::from_sources(
        module_names.clone(),
        sources.values().map(String::as_str),
        cache.manager(),
    );

    // 逐文件转译（内存中）：入口识别需要全部产物先就绪
    let mut pending: Vec<Pending> = Vec::new();
    for mirror_path in &dialect_paths {
        let Some(content) = sources.get(mirror_path) else {
            continue;
        };
        let output = i18n_rust_engine::transpile_pipeline_with_map_and_project(
            content,
            cache.manager(),
            Some(&module_names),
            Some(&project),
        );
        let column_map =
            i18n_rust_engine::column_map::ColumnMap::build(content, &output.pipeline_map);
        pending.push(Pending {
            mirror_path: mirror_path.clone(),
            output: output.output,
            column_map,
            zh_content: content.clone(),
        });
    }

    // 入口识别：词干 main 优先（其天然产物即 src/main.rs）；否则取与
    // 现有 src/main.rs 相似度最高的方言源（rzc 允许任意入口文件名，
    // 产物统一写入 main.rs；相似度不足时保留原 main.rs 不动）
    let main_rs_content = std::fs::read_to_string(mirror_src.join("main.rs")).ok();
    let entry_path = detect_entry(&pending, main_rs_content.as_deref());

    // 写产物 + 构造映射表
    let mut files: Vec<MirrorFile> = Vec::new();
    for p in pending {
        let Ok(rel) = p.mirror_path.strip_prefix(mirror_dir) else {
            continue;
        };
        let source_path = project_root.join(rel);
        // 诊断发布 URI 复用缓存条目的客户端原样 URI：与编辑器打开的
        // 文档严格一致（Windows 上规范化形式与客户端形式不同，直接用
        // 反推 URI 会导致诊断发布到用户不可见的文档上）；未登记条目
        // （缓存无路径匹配）回退规范化形式
        let original_uri = cache
            .query_by_path(&source_path)
            .map(|e| e.original_uri.clone())
            .unwrap_or_else(|| path_to_uri(&source_path));
        let is_entry = entry_path.as_deref() == Some(p.mirror_path.as_path());
        let (content, product_path, entry_line_map) = if is_entry {
            // 入口：补 #[path] 注解（rustc 拒绝非 ASCII 模块名的文件式
            // 声明，E0754），产物固定写入 src/main.rs
            let (annotated, line_map) =
                i18n_rust_engine::module_path::annotate_non_ascii_mods_with_lines(&p.output);
            (annotated, mirror_src.join("main.rs"), Some(line_map))
        } else {
            // 其余文件与 rzc 的 transpile_project_files 一致：不注解，
            // 天然产物路径（同名 .rs）
            (p.output.clone(), p.mirror_path.with_extension("rs"), None)
        };
        if let Err(e) = std::fs::write(&product_path, &content) {
            log::warn!("镜像产物写入失败：{}：{e}", product_path.display());
            continue;
        }
        files.push(MirrorFile {
            product_path,
            original_uri,
            zh_content: p.zh_content,
            column_map: p.column_map,
            entry_line_map,
        });
    }
    if files.is_empty() { None } else { Some(files) }
}

/// 入口识别：返回镜像中作为 main.rs 来源的方言源路径
///
/// 词干为 main 的方言文件优先（rzc 语义：其产物即入口产物）；
/// 否则在所有方言产物（含 `#[path]` 注解）中找与现有 src/main.rs
/// 相似度最高者，相似度须达到阈值（初次构建即精确匹配，入口编辑后
/// 仍高度相似；其他模块与入口的相似度接近零）。
fn detect_entry(pending: &[Pending], main_rs: Option<&str>) -> Option<PathBuf> {
    for p in pending {
        if p.mirror_path.file_stem().and_then(|s| s.to_str()) == Some("main") {
            return Some(p.mirror_path.clone());
        }
    }
    let main_rs = main_rs?;
    let mut best: Option<(PathBuf, f64)> = None;
    for p in pending {
        let (annotated, _) =
            i18n_rust_engine::module_path::annotate_non_ascii_mods_with_lines(&p.output);
        let score = line_similarity(&annotated, main_rs);
        if score >= 0.6 && best.as_ref().is_none_or(|(_, b)| score > *b) {
            best = Some((p.mirror_path.clone(), score));
        }
    }
    best.map(|(path, _)| path)
}

/// 逐行相似度：实质行（≥8 字符）命中率（命中数 / 两侧较大行数）
///
/// 分母取 max(候选行数, 参照行数)，小文件的局部巧合命中无法抬高分数。
fn line_similarity(candidate: &str, reference: &str) -> f64 {
    let substantial = |l: &str| l.trim().chars().count() >= 8;
    let ref_lines: HashSet<&str> = reference
        .lines()
        .map(str::trim)
        .filter(|l| substantial(l))
        .collect();
    if ref_lines.is_empty() {
        return 0.0;
    }
    let mut total = 0usize;
    let mut hit = 0usize;
    for line in candidate.lines().map(str::trim).filter(|l| substantial(l)) {
        total += 1;
        if ref_lines.contains(line) {
            hit += 1;
        }
    }
    if total == 0 {
        return 0.0;
    }
    hit as f64 / total.max(ref_lines.len()) as f64
}

/// 启动 cargo check 并等待退出（持续排空管道防写满阻塞，超时强杀）
///
/// 返回 (stdout, 退出码是否成功)；进程无法启动/超时被强杀时返回 None。
fn run_cargo_check(mirror_dir: &Path, target_dir: &Path) -> Option<(String, bool)> {
    let mut child = match std::process::Command::new("cargo")
        .args(["check", "--offline", "--message-format=json"])
        .arg("--target-dir")
        .arg(target_dir)
        .current_dir(mirror_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            log::warn!("镜像 cargo check 启动失败：{e}");
            return None;
        }
    };
    let mut stdout_pipe = child.stdout.take()?;
    let mut stderr_pipe = child.stderr.take()?;
    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf);
        buf
    });

    let mut status = None;
    for _ in 0..CHECK_TIMEOUT_MS / POLL_INTERVAL_MS {
        match child.try_wait() {
            Ok(Some(s)) => {
                status = Some(s);
                break;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS)),
            Err(_) => break,
        }
    }
    let status = match status {
        Some(s) => s,
        None => {
            log::warn!("镜像 cargo check 超时/异常，已终止（回退虚拟检查）");
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };
    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();
    if !status.success() {
        // 编译错误属正常情况（诊断在 stdout）时同样以非零码退出；stderr
        // 末尾才是 cargo 层的真正错误（清单/锁/依赖解析），首行往往只是
        // "Checking ..." 进度行，取末尾若干行输出便于排查
        let stderr_text = String::from_utf8_lossy(&stderr);
        if stderr_text.contains("failed to") || stderr_text.contains("error: could not") {
            let tail: Vec<&str> = stderr_text.lines().rev().take(6).collect();
            let tail: Vec<&str> = tail.into_iter().rev().collect();
            log::warn!(
                "镜像 cargo check 非零退出（{:?}）：{}",
                status.code(),
                tail.join(" / ")
            );
        }
    }
    Some((
        String::from_utf8_lossy(&stdout).to_string(),
        status.success(),
    ))
}

/// 解析 rustc JSON 诊断流：坐标回译 → 消息翻译 → 所有权提取 → 合并发布
///
/// 返回 (可归位的编译消息数, 发布文件数)。
fn publish_rustc_diagnostics(
    stdout: &str,
    mirror_dir: &Path,
    files: &[MirrorFile],
    cache: &Arc<TranslationCache>,
    known_words: &HashSet<String>,
    builtin_diags: &Arc<Mutex<HashMap<String, Vec<Value>>>>,
    sender: &crossbeam_channel::Sender<lsp_server::Message>,
) -> (usize, usize) {
    // 产物路径 → 方言源映射：canonical 键归一 rustc 的绝对/相对路径
    let index: HashMap<PathBuf, &MirrorFile> = files
        .iter()
        .filter_map(|f| std::fs::canonicalize(&f.product_path).ok().map(|p| (p, f)))
        .collect();

    let mut by_uri: HashMap<String, Vec<Value>> = HashMap::new();
    let mut seen = 0usize;
    for line in stdout.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["reason"].as_str() != Some("compiler-message") {
            continue;
        }
        let msg = &v["message"];
        let Some(spans) = msg["spans"].as_array() else {
            continue;
        };
        // 主 span 优先（rustc 的 spans 可能把次级位置排在前面）
        let Some(span) = spans
            .iter()
            .find(|s| s["is_primary"] == Value::Bool(true))
            .or_else(|| spans.first())
        else {
            continue;
        };
        let Some(file) = resolve_span_file(span, mirror_dir, &index) else {
            continue; // 非方言文件（手写 .rs、标准库等）：不发布
        };
        seen += 1;

        let orig_message = msg["message"].as_str().unwrap_or("");
        let code = msg["code"]["code"].as_str().unwrap_or("").to_string();
        let severity = match msg["level"].as_str() {
            Some("error") => 1,
            Some("warning") => 2,
            _ => 3,
        };
        let Some((sl, sc, el, ec)) = map_span_range(file, span) else {
            continue;
        };
        let range = json!({
            "start": { "line": sl, "character": sc },
            "end": { "line": el, "character": ec }
        });

        // 次级 span → relatedInformation（所有权可视化等）；消息先保留英文，
        // 供 extract_ownership_details 关键词判定，发布前再翻译
        let mut raw_related: Vec<Value> = Vec::new();
        for s in spans {
            if s["is_primary"] == Value::Bool(true) {
                continue;
            }
            let Some(label) = s["label"].as_str().filter(|l| !l.is_empty()) else {
                continue;
            };
            let Some(rf) = resolve_span_file(s, mirror_dir, &index) else {
                continue;
            };
            let Some((a, b, c, d)) = map_span_range(rf, s) else {
                continue;
            };
            raw_related.push(json!({
                "location": {
                    "uri": rf.original_uri,
                    "range": {
                        "start": { "line": a, "character": b },
                        "end": { "line": c, "character": d }
                    }
                },
                "message": label
            }));
        }

        let raw = json!({ "message": orig_message, "code": code });
        let restored = json!({ "range": range, "relatedInformation": raw_related });
        let ownership = extract_ownership_details(&raw, &restored, &file.original_uri);

        let mut diag = json!({
            "range": range,
            "severity": severity,
            "code": code,
            "message": translate_diagnostic_message(orig_message),
        });
        if !raw_related.is_empty() {
            let translated: Vec<Value> = raw_related
                .iter()
                .map(|r| {
                    let mut item = r.clone();
                    item["message"] = Value::String(translate_diagnostic_message(
                        r["message"].as_str().unwrap_or(""),
                    ));
                    item
                })
                .collect();
            diag["relatedInformation"] = Value::Array(translated);
        }
        if let Some(details) = ownership
            && let Ok(details_value) = serde_json::to_value(&details)
        {
            diag["data"] = details_value;
        }
        by_uri
            .entry(file.original_uri.clone())
            .or_default()
            .push(diag);
    }

    // 合并内置诊断（RA 最近一次映射后的方言坐标诊断 + 教学提示）：
    // 同 code 且同起始行视为重复（与虚拟检查的合并规则一致），
    // 避免镜像结果覆盖语法/类型实时诊断
    let mut published = 0usize;
    for (uri, mut diags) in by_uri {
        if let Ok(guard) = builtin_diags.lock()
            && let Some(builtin) = guard.get(&uri)
        {
            for e in builtin.clone() {
                let dup = diags.iter().any(|d| {
                    d["code"] == e["code"]
                        && d["range"]["start"]["line"] == e["range"]["start"]["line"]
                });
                if !dup {
                    diags.push(e);
                }
            }
        }
        // 教学诊断注入（全角标点 + 教学 lint）：由 entry 内容直接计算，
        // 不依赖 builtin 缓存（镜像检查先于 RA 首批发布时缓存为空）
        if let Some(entry) = cache.query_original(&uri) {
            inject_teaching_diags(&mut diags, &entry, known_words);
        }
        let notification = lsp_server::Notification {
            method: "textDocument/publishDiagnostics".to_string(),
            params: json!({ "uri": uri, "diagnostics": diags }),
        };
        let _ = sender.send(lsp_server::Message::Notification(notification));
        published += 1;
    }
    (seen, published)
}

/// 解析 span 归属的方言文件（镜像产物路径 → MirrorFile）
fn resolve_span_file<'a>(
    span: &Value,
    mirror_dir: &Path,
    index: &HashMap<PathBuf, &'a MirrorFile>,
) -> Option<&'a MirrorFile> {
    let name = span["file_name"].as_str()?;
    let path = Path::new(name);
    // rustc 输出相对路径（cwd 为镜像项目根）或绝对路径，统一绝对化
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        mirror_dir.join(path)
    };
    let canon = std::fs::canonicalize(abs).ok()?;
    index.get(&canon).copied()
}

/// span 的起止位置回译：方言 0-based 行 + UTF-16 列（LSP 协议）
fn map_span_range(file: &MirrorFile, span: &Value) -> Option<(u32, u32, u32, u32)> {
    let line = span["line_start"].as_u64()? as u32;
    let col = span["column_start"].as_u64()? as u32;
    let end_line = span["line_end"].as_u64().unwrap_or(line as u64) as u32;
    let end_col = span["column_end"].as_u64().unwrap_or(col as u64) as u32;
    let (sl, sc) = map_position(file, line, col);
    let (el, ec) = map_position(file, end_line, end_col);
    Some((sl, sc, el, ec))
}

/// 单点回译：产物 1-based (行, 字符列) → 方言 0-based (行, UTF-16 列)
///
/// 行号：入口产物含 `#[path]` 注解插入行，先经入口行映射换算回引擎直出
/// 行号（列映射以引擎直出产物为基准；注解行归属其 mod 声明行）。
/// 列号：引擎列映射输出方言字符列（与 rustc 列口径一致），再按方言文本
/// 换算为 LSP 的 UTF-16 列。
fn map_position(file: &MirrorFile, line_1based: u32, col_1based: u32) -> (u32, u32) {
    let engine_line = match &file.entry_line_map {
        Some(map) => map
            .get(line_1based.saturating_sub(1) as usize)
            .map(|l| *l as u32 + 1)
            .unwrap_or(line_1based),
        None => line_1based,
    };
    let (zh_line, zh_char_col) = file.column_map.map_position(engine_line, col_1based);
    (
        zh_line.saturating_sub(1),
        zh_char_col_to_utf16(&file.zh_content, zh_line, zh_char_col),
    )
}

/// 方言行内字符列（1-based）→ UTF-16 列（0-based）
///
/// 引擎列映射的列口径为字符数（与 rustc JSON 一致）；LSP 协议要求
/// UTF-16 代码单元列，非 BMP 字符（emoji 等）占 2 个单元，须逐字符累加。
fn zh_char_col_to_utf16(zh_content: &str, line_1based: u32, char_col_1based: u32) -> u32 {
    let Some(line_text) = zh_content
        .lines()
        .nth(line_1based.saturating_sub(1) as usize)
    else {
        return char_col_1based.saturating_sub(1);
    };
    line_text
        .chars()
        .take(char_col_1based.saturating_sub(1) as usize)
        .map(|c| c.len_utf16() as u32)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(name: &str, output: &str) -> Pending {
        Pending {
            mirror_path: PathBuf::from("/mirror/src").join(name),
            output: output.to_string(),
            column_map: i18n_rust_engine::column_map::ColumnMap::build("", &[]),
            zh_content: String::new(),
        }
    }

    /// 小于 8 字符的短行（`}`、空行等）不参与相似度，避免结构行虚高命中
    #[test]
    fn test_line_similarity_ignores_short_lines() {
        assert_eq!(line_similarity("}\n{\n", "}\n{\n"), 0.0);
        assert_eq!(line_similarity("", ""), 0.0);
        // 参照侧全是短行 → 实质行为空 → 0.0
        assert_eq!(line_similarity("fn main() {}", "}\n)\n"), 0.0);
    }

    /// 完全相同为 1.0；无交集为 0.0
    #[test]
    fn test_line_similarity_bounds() {
        let 文本 = "fn main() {\n    println!(\"hi\");\n}\n";
        assert_eq!(line_similarity(文本, 文本), 1.0);
        assert_eq!(
            line_similarity(文本, "fn other() {\n    let 甲 = 1;\n}\n"),
            0.0
        );
    }

    /// 分母取两侧较大者：小文件与参照的局部巧合命中无法抬高分数
    ///
    /// 候选仅 1 条实质行且命中，参照有 3 条实质行 → 1/3 而非 1.0。
    #[test]
    fn test_line_similarity_denominator_uses_larger_side() {
        let 参照 = "fn 甲() {\nfn 乙() {\nfn 丙() {\n";
        let 分数 = line_similarity("fn 甲() {\n", 参照);
        assert!(
            (分数 - 1.0 / 3.0).abs() < 1e-9,
            "分母应为 3（较大侧），实际 {分数}"
        );
    }

    /// 分词干为 main 的方言文件优先作为入口（不看相似度）
    #[test]
    fn test_detect_entry_prefers_main_stem() {
        let 候选 = vec![
            pending("甲.zh", "fn 甲() {\n    let 甲值 = 1;\n}\n"),
            pending("main.zh", "fn 主函数() {\n}\n"),
            pending("乙.zh", "fn 乙() {\n    let 乙值 = 2;\n}\n"),
        ];
        assert_eq!(
            detect_entry(&候选, None).map(|p| p.file_name().unwrap().to_owned()),
            Some("main.zh".into()),
            "词干为 main 时应无视相似度直接选中"
        );
    }

    /// 无 main 时按与现有 main.rs 的相似度选取（须达阈值 0.6）
    #[test]
    fn test_detect_entry_by_similarity_with_threshold() {
        let 入口文本 = "fn 主函数() {\n    println!(\"你好\");\n}\n";
        let 候选 = vec![
            pending("甲.zh", "fn 甲() {\n    let 甲值 = 1;\n}\n"),
            pending("入口.zh", 入口文本),
        ];
        assert_eq!(
            detect_entry(&候选, Some(入口文本)).map(|p| p.file_name().unwrap().to_owned()),
            Some("入口.zh".into()),
            "相似度达阈值者应被选为入口"
        );
        // 全部候选都与参照不相似 → None（低于阈值宁可不识别）
        assert!(
            detect_entry(
                &[pending("甲.zh", "fn 甲() {\n    let 甲值 = 1;\n}\n")],
                Some("fn 乙() {\n    let 乙值 = 2;\n}\n")
            )
            .is_none(),
            "相似度不足阈值时不应误判入口"
        );
    }

    /// 无 main 词干且无参照 main.rs 时无法识别入口
    #[test]
    fn test_detect_entry_none_without_reference() {
        let 候选 = vec![pending("甲.zh", "fn 甲() {\n    let 甲值 = 1;\n}\n")];
        assert!(detect_entry(&候选, None).is_none());
        assert!(detect_entry(&[], Some("fn 主函数() {\n}\n")).is_none());
    }

    /// 方言扩展名匹配：大小写与后缀精确性
    #[test]
    fn test_file_matches_extensions() {
        let 扩展名 = vec!["zh".to_string(), "ja".to_string()];
        assert!(file_matches_extensions(Path::new("/p/main.zh"), &扩展名));
        assert!(file_matches_extensions(Path::new("/p/模块.ja"), &扩展名));
        assert!(!file_matches_extensions(Path::new("/p/main.rs"), &扩展名));
        // 仅匹配完整后缀：`xzh` 不是 `.zh`
        assert!(!file_matches_extensions(Path::new("/p/main.zhx"), &扩展名));
        // 无扩展名
        assert!(!file_matches_extensions(Path::new("/p/main"), &扩展名));
    }

    /// 项目根哈希：同路径稳定、不同路径不同（镜像目录隔离的依据）
    #[test]
    fn test_path_hash_is_stable_and_distinct() {
        let 甲 = path_hash(Path::new("/a/b"));
        assert_eq!(甲, path_hash(Path::new("/a/b")), "同路径应稳定");
        assert_ne!(甲, path_hash(Path::new("/a/c")), "不同路径应不同");
    }

    /// 字符列 → UTF-16 列：非 BMP 字符占 2 个单元，BMP 中文占 1
    #[test]
    fn test_zh_char_col_to_utf16_counts_units() {
        // "字符😀甲"：字符列 1-based → 取前 n 个字符累加 len_utf16
        let 文本 = "字符😀甲";
        assert_eq!(zh_char_col_to_utf16(文本, 1, 1), 0);
        assert_eq!(zh_char_col_to_utf16(文本, 1, 2), 1);
        // 含 emoji：到第 4 列时已计入 emoji 的 2 个单元
        assert_eq!(zh_char_col_to_utf16(文本, 1, 4), 4);
        // 越界行回退为 0-based 字符列
        assert_eq!(zh_char_col_to_utf16(文本, 9, 3), 2);
    }
}
