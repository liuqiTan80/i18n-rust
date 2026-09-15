// 构建脚本：扫描 lang-packs/ 目录自动生成内嵌清单
//
// 消除 语言.rs 中手写的 include_str! 白名单（builtin_file match 与 ui_table! 宏列表）：
// 语言包新增/删除文件后无需修改任何 Rust 代码，重新编译即自动纳入。
//
// 生成物（OUT_DIR/builtin_generated.rs，由 语言.rs include! 引入）：
// - BUILTIN_FILES：(语言代码, 相对路径, 文件内容) 全量清单，供 builtin_file 查询
// - UI_TABLE_*：每语言 ui.toml 的消息表静态实例，供 ui_table_for 查询
//
// 发布兼容性：lang-packs/ 位于 crate 目录内且已列入 Cargo.toml include 白名单，
// crates.io 消费者编译时本脚本同样可扫描到完整数据。

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// 语言包文件集合：顶层文件 + crates/ 子目录文件
struct LangFiles {
    /// (相对路径, 绝对路径)，相对路径形如 "keywords.toml" 或 "crates/序列化.toml"
    files: Vec<(String, PathBuf)>,
}

fn collect_lang_files(lang_dir: &Path) -> LangFiles {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(lang_dir) {
        let mut list: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("toml"))
            .collect();
        list.sort();
        for path in list {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            files.push((name, path));
        }
    }
    let crates_dir = lang_dir.join("crates");
    if crates_dir.is_dir()
        && let Ok(entries) = fs::read_dir(&crates_dir)
    {
        let mut list: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("toml"))
            .collect();
        list.sort();
        for path in list {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            files.push((format!("crates/{name}"), path));
        }
    }
    LangFiles { files }
}

/// FNV-1a 64 位增量哈希（与 cache.rs 的 compute_content_hash 同算法）
fn fnv1a(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= *byte as u64;
        *hash = hash.wrapping_mul(0x100000001b3);
    }
}

/// 引擎源码指纹：对 src/ 下全部 .rs 文件（按路径排序，逐个混入相对路径
/// 与内容）计算 FNV-1a 哈希。转译算法任何变化都会改变指纹，使磁盘缓存
/// 自动失效（仅靠内容哈希与映射指纹无法感知引擎升级）。
fn engine_source_fingerprint(crate_root: &Path) -> u64 {
    fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut list: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        list.sort();
        for path in list {
            if path.is_dir() {
                collect(&path, files);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    collect(&crate_root.join("src"), &mut files);
    let mut hash = 0xcbf29ce484222325u64;
    for path in files {
        let rel = path
            .strip_prefix(crate_root)
            .unwrap_or(&path)
            .to_string_lossy();
        fnv1a(&mut hash, rel.as_bytes());
        if let Ok(content) = fs::read(&path) {
            fnv1a(&mut hash, &content);
        }
    }
    hash
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let lang_root = Path::new(&manifest_dir).join("lang-packs");
    let out_dir = env::var("OUT_DIR").unwrap();

    println!("cargo:rerun-if-changed=lang-packs");
    // 引擎源码变化时重跑本脚本：源码指纹（见下）必须随算法代码更新，
    // 否则磁盘缓存的失效依据会停在旧值
    println!("cargo:rerun-if-changed=src");

    // 语言目录按名称排序，保证生成代码与嵌入顺序确定
    let mut lang_dirs: Vec<PathBuf> = fs::read_dir(&lang_root)
        .expect("语言包目录 lang-packs 不存在")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    lang_dirs.sort();

    let mut code = String::from("// 由 build.rs 自动生成，勿手工编辑\n\n");

    // ===== BUILTIN_FILES 全量清单 =====
    code.push_str("static BUILTIN_FILES: &[(&str, &str, &str)] = &[\n");
    let mut ui_tables = Vec::new();
    for dir in &lang_dirs {
        let lang = dir.file_name().unwrap().to_string_lossy().into_owned();
        let lang_files = collect_lang_files(dir);
        for (rel, abs) in &lang_files.files {
            let abs_str = abs.to_string_lossy();
            writeln!(code, "    ({lang:?}, {rel:?}, include_str!({abs_str:?})),").unwrap();
            if rel == "ui.toml" {
                ui_tables.push((lang.clone(), abs_str.into_owned()));
            }
        }
    }
    code.push_str("];\n\n");

    // ===== 引擎源码指纹（转译缓存失效依据，见 cache.rs 语境指纹） =====
    // 缓存条目除内容哈希与语言包映射指纹外，还须绑定转译算法自身的身份：
    // 算法变更（如别名替换细则修复）后旧缓存必须失效，否则源文件未变时
    // 旧转译产物继续命中、修复不生效（真实事故：`实现 库特征` 块内方法名
    // 替换修复后，旧产物仍被复用）。
    writeln!(
        code,
        "pub(crate) static ENGINE_SOURCE_FINGERPRINT: u64 = {:#x};",
        engine_source_fingerprint(Path::new(&manifest_dir))
    )
    .unwrap();
    code.push('\n');

    // ===== UI 消息表静态实例（每语言一个，惰性解析一次） =====
    for (i, (_, abs)) in ui_tables.iter().enumerate() {
        writeln!(
            code,
            "static UI_TABLE_{i}: std::sync::LazyLock<std::collections::HashMap<String, String>> =\n    \
             std::sync::LazyLock::new(|| parse_ui(include_str!({abs:?})));"
        )
        .unwrap();
    }
    code.push('\n');

    // ===== ui_table_for：按语言代码路由消息表（未知语言回退 zh） =====
    code.push_str(
        "fn ui_table_for(code: &str) -> &'static std::sync::LazyLock<std::collections::HashMap<String, String>> {\n    match code {\n",
    );
    for (i, (lang, _)) in ui_tables.iter().enumerate() {
        if lang == "zh" {
            continue; // zh 作为回退分支最后输出
        }
        writeln!(code, "        {lang:?} => &UI_TABLE_{i},").unwrap();
    }
    let zh_index = ui_tables
        .iter()
        .position(|(lang, _)| lang == "zh")
        .expect("zh 语言包必须存在（消息回退链依赖）");
    writeln!(code, "        _ => &UI_TABLE_{zh_index},").unwrap();
    code.push_str("    }\n}\n");

    fs::write(Path::new(&out_dir).join("builtin_generated.rs"), code)
        .expect("写入生成的嵌入清单失败");
}
