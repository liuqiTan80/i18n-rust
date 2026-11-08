// 构建脚本：扫描 lang-packs/ 目录自动生成内嵌清单
//
// 消除 语言 模块里手写的 `include_str!` 白名单（内置文件查询的语言码分支与消息表分支）：
// 语言包新增/删除文件后无需修改任何 Rust 代码，重新编译即自动纳入。
//
// 生成物（OUT_DIR/内置生成.rs，由 语言.zh 的 `包含文件!` 展开在 语言 模块内；
// 路径经 `cargo:rustc-env` 交给编译期）：
// - 内置清单：(语言码, 相对路径, 文件内容) 全量清单，供 语言::内置文件 查询
// - 消息表_*：每语言 ui.toml 的消息表静态实例，供 语言::消息表路由 查询
//
// 生成项一律用中文命名：消费方是方言源，写英文项名会在全中文门禁上记账
// （中文标识符对 rustc 本身合法，生成物直接参与编译，不经转译）。
//
// 发布兼容性：lang-packs/ 位于库目录内且已列入 Cargo.toml 的包含白名单，
// 发布后的消费者编译时本脚本同样可扫描到完整数据。

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// 语言包文件集合：顶层文件 + 第三方子目录文件
struct 语言包文件 {
    /// (相对路径, 绝对路径)，相对路径形如 "keywords.toml" 或 "crates/序列化.toml"
    file: Vec<(String, PathBuf)>,
}

fn 收集语言包文件(语言目录: &Path) -> 语言包文件 {
    let mut file = Vec::new();
    if let Ok(条目们) = fs::read_dir(语言目录) {
        let mut 列表: Vec<PathBuf> = 条目们
            .flatten()
            .map(|条目| 条目.path())
            .filter(|路径| {
                路径.is_file() && 路径.extension().and_then(|扩展| 扩展.to_str()) == Some("toml")
            })
            .collect();
        列表.sort();
        for 路径 in 列表 {
            let 名称 = 路径.file_name().unwrap().to_string_lossy().into_owned();
            file.push((名称, 路径));
        }
    }
    let 第三方目录 = 语言目录.join("crates");
    if 第三方目录.is_dir()
        && let Ok(条目们) = fs::read_dir(&第三方目录)
    {
        let mut 列表: Vec<PathBuf> = 条目们
            .flatten()
            .map(|条目| 条目.path())
            .filter(|路径| {
                路径.is_file() && 路径.extension().and_then(|扩展| 扩展.to_str()) == Some("toml")
            })
            .collect();
        列表.sort();
        for 路径 in 列表 {
            let 名称 = 路径.file_name().unwrap().to_string_lossy().into_owned();
            file.push((format!("crates/{名称}"), 路径));
        }
    }
    语言包文件 { file }
}

/// FNV-1a 64 位增量哈希（与 缓存 模块的内容哈希同算法）
fn 混入哈希(哈希: &mut u64, 字节们: &[u8]) {
    for u8 in 字节们 {
        *哈希 ^= *u8 as u64;
        *哈希 = 哈希.wrapping_mul(0x100000001b3);
    }
}

/// 引擎源码指纹：对源码目录下全部 .rs 文件（按路径排序，逐个混入相对路径
/// 与内容）计算 FNV-1a 哈希。转译算法任何变化都会改变指纹，使磁盘缓存
/// 自动失效（仅靠内容哈希与映射指纹无法感知引擎升级）。
fn 引擎源码指纹(包根: &Path) -> u64 {
    fn 收集目录(目录: &Path, 文件们: &mut Vec<PathBuf>) {
        let Ok(条目们) = fs::read_dir(目录) else {
            return;
        };
        let mut 列表: Vec<PathBuf> = 条目们.flatten().map(|条目| 条目.path()).collect();
        列表.sort();
        for 路径 in 列表 {
            if 路径.is_dir() {
                收集目录(&路径, 文件们);
            } else if 路径.extension().and_then(|扩展| 扩展.to_str()) == Some("rs") {
                文件们.push(路径);
            }
        }
    }

    let mut 文件们 = Vec::new();
    收集目录(&包根.join("src"), &mut 文件们);
    let mut 哈希: u64 = 0xcbf29ce484222325;
    for 路径 in 文件们 {
        let 相对路径 = 路径.strip_prefix(包根).unwrap_or(&路径).to_string_lossy();
        混入哈希(&mut 哈希, 相对路径.as_bytes());
        if let Ok(内容) = fs::read(&路径) {
            混入哈希(&mut 哈希, &内容);
        }
    }
    哈希
}

fn main() {
    let 清单目录 = env::var("CARGO_MANIFEST_DIR").unwrap();
    let 语言包根 = Path::new(&清单目录).join("lang-packs");
    let 输出目录 = env::var("OUT_DIR").unwrap();

    println!("cargo:rerun-if-changed=lang-packs");
    // 引擎源码变化时重跑本脚本：源码指纹（见下）必须随算法代码更新，
    // 否则磁盘缓存的失效依据会停在旧值
    println!("cargo:rerun-if-changed=src");

    // 语言目录按名称排序，保证生成代码与嵌入顺序确定
    let mut 语言目录们: Vec<PathBuf> = fs::read_dir(&语言包根)
        .expect("语言包目录 lang-packs 不存在")
        .flatten()
        .map(|条目| 条目.path())
        .filter(|路径| 路径.is_dir())
        .collect();
    语言目录们.sort();

    let mut 代码 = String::from("// 由 build.rs 自动生成，勿手工编辑\n\n");

    // ===== 内置清单 全量清单 =====
    代码.push_str("static 内置清单: &[(&str, &str, &str)] = &[\n");
    let mut 消息表项 = Vec::new();
    for 目录 in &语言目录们 {
        let 语言码 = 目录.file_name().unwrap().to_string_lossy().into_owned();
        let 语言文件 = 收集语言包文件(目录);
        for (相对, 绝对) in &语言文件.file {
            let 绝对路径 = 绝对.to_string_lossy();
            writeln!(
                代码,
                "    ({语言码:?}, {相对:?}, include_str!({绝对路径:?})),"
            )
            .unwrap();
            if 相对 == "ui.toml" {
                消息表项.push((语言码.clone(), 绝对路径.into_owned()));
            }
        }
    }
    代码.push_str("];\n\n");

    // ===== 引擎源码指纹（转译缓存失效依据，见 缓存.rs 语境指纹） =====
    // 缓存条目除内容哈希与语言包映射指纹外，还须绑定转译算法自身的身份：
    // 算法变更（如别名替换细则修复）后旧缓存必须失效，否则源文件未变时
    // 旧转译产物继续命中、修复不生效（真实事故：`实现 库特征` 块内方法名
    // 替换修复后，旧产物仍被复用）。
    writeln!(
        代码,
        "pub(crate) static 源码指纹: u64 = {:#x};",
        引擎源码指纹(Path::new(&清单目录))
    )
    .unwrap();
    代码.push('\n');

    // ===== 消息表静态实例（每语言一个，惰性解析一次） =====
    for (序号, (_, 绝对)) in 消息表项.iter().enumerate() {
        writeln!(
            代码,
            "static 消息表_{序号}: std::sync::LazyLock<std::collections::HashMap<String, String>> =\n    \
             std::sync::LazyLock::new(|| crate::界面消息::解析界面消息(include_str!({绝对:?})));"
        )
        .unwrap();
    }
    代码.push('\n');

    // ===== 消息表路由：按语言码路由消息表（未知语言回退中文表） =====
    代码.push_str(
        "fn 消息表路由(语言码: &str) -> &'static std::sync::LazyLock<std::collections::HashMap<String, String>> {\n    match 语言码 {\n",
    );
    for (序号, (语言码, _)) in 消息表项.iter().enumerate() {
        if 语言码 == "zh" {
            continue; // zh 作为回退分支最后输出
        }
        writeln!(代码, "        {语言码:?} => &消息表_{序号},").unwrap();
    }
    let 中文下标 = 消息表项
        .iter()
        .position(|(语言码, _)| 语言码 == "zh")
        .expect("zh 语言包必须存在（消息回退链依赖）");
    writeln!(代码, "        _ => &消息表_{中文下标},").unwrap();
    代码.push_str("    }\n}\n");

    let 生成文件 = Path::new(&输出目录).join("内置生成.rs");
    fs::write(&生成文件, 代码).expect("写入生成的嵌入清单失败");
    // 把生成文件的绝对路径交给编译期：`语言` 模块据此展开 `包含文件!`。
    // 走这条约定而不是在源码里拼输出目录路径，方言源就不必用拼接宏，
    // 也不必为它给全部语言包补词条。
    println!(
        "cargo:rustc-env=BUILTIN_GENERATED_PATH={}",
        生成文件.display()
    );
}
