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

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let lang_root = Path::new(&manifest_dir).join("lang-packs");
    let out_dir = env::var("OUT_DIR").unwrap();

    println!("cargo:rerun-if-changed=lang-packs");

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
