//! 转译产物磁盘布局：非入口方言文件的 `.rs` 产物写入项目内托管目录
//! `.rz/gen/`，镜像源码目录树（`src/数学.zh` → `.rz/gen/src/数学.rs`）。
//!
//! 动机：`src/` 是学习者唯一会去看的目录，旧布局把每个方言文件的英文产物
//! 平铺在源文件旁边（`src/数学.zh` + `src/数学.rs`），母语教学目录里一半
//! 内容是生成物。托管后 `src/` 只剩母语源文件与 Cargo 固定入口产物
//! `src/main.rs`（入口必须留在 Cargo 默认发现位置，无法移出）。
//!
//! 关键不变量：**托管目录镜像源码布局**，所以非入口产物之间的相对位置与
//! 旧布局完全一致，`mod 工具;` 的 `#[path]` 前缀不变；只有入口产物
//! （仍在 `src/`）指向子模块时需要 `../.rz/gen/src/…` 前缀——由
//! [`annotate_product_mod_paths_with_lines`] 统一计算，CLI 与 LSP 镜像
//! 共用同一实现，避免两侧规则漂移。

use std::path::{Path, PathBuf};

/// 产物托管目录（相对项目根）
pub const GEN_DIR: &str = ".rz/gen";

/// 方言源文件 → 非入口产物路径（镜像源码布局，保持项目根相对位置）
///
/// 源文件不在项目根之下（`rzc check ~/示例.zh` 等）时退回旧行为：
/// 产物写在源文件旁边，不硬造 `.rz/gen/` 结构。
pub fn gen_product_path(project_root: &Path, source: &Path) -> PathBuf {
    let Some(rel) = relative_to_root(project_root, source) else {
        return source.with_extension("rs");
    };
    project_root.join(GEN_DIR).join(rel).with_extension("rs")
}

/// 位于托管目录下的产物 → 去掉托管前缀的等价路径（仍为绝对路径）
///
/// `.rz/gen/src/数学.rs` → `<root>/src/数学.rs`；不在托管目录下返回 `None`。
/// 供诊断回译使用：rustc 报的是托管产物路径，母语源文件在源码树里。
pub fn strip_gen_prefix(project_root: &Path, path: &Path) -> Option<PathBuf> {
    let rel = relative_to_root(project_root, path)?;
    let inner = rel.strip_prefix(GEN_DIR).ok()?;
    Some(project_root.join(inner))
}

/// 从目录 `from_dir` 到 `target` 的词法相对路径（`#[path]` 用 `/` 分隔）
///
/// `#[path]` 由 rustc 相对**声明所在文件的目录**解析，故 `from_dir` 传目录。
/// 纯词法计算（不访问文件系统、不解析符号链接）：本项目内产物与源码都在
/// 同一根目录下按字面路径生成，无需真实路径。
pub fn relative_path(from_dir: &Path, target: &Path) -> String {
    let from = normalize_abs(from_dir);
    let to = normalize_abs(target);
    let from_parts: Vec<_> = from.components().collect();
    let to_parts: Vec<_> = to.components().collect();
    let common = from_parts
        .iter()
        .zip(to_parts.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut out: Vec<String> = Vec::new();
    for _ in common..from_parts.len() {
        out.push("..".to_string());
    }
    for comp in &to_parts[common..] {
        out.push(comp.as_os_str().to_string_lossy().into_owned());
    }
    if out.is_empty() {
        ".".to_string()
    } else {
        out.join("/")
    }
}

/// 文件式 `mod 名称;` 声明的 `#[path]` 注解（托管布局版），并返回行映射
///
/// 逐模块名解析实际落点，两类命中者才注解（其余保留 rustc 原生 E0583，
/// 与旧布局同口径）：
/// 1. `child_src_dir/名称.<方言>` 存在 → 指向其托管产物（镜像布局）；
/// 2. `child_src_dir/名称.rs` 存在（手写 `.rs` 子模块）→ 指向它本身。
///
/// 注解路径相对 `product_dir`（当前文件**产物**所在目录）计算：入口产物在
/// `src/` 时得到 `../.rz/gen/src/…`，非入口产物在 `.rz/gen/src/` 时子模块
/// 之间仍是 `领域/工具.rs`，与旧布局逐字一致。
///
/// `extensions` 为可用方言扩展名（可带点），顺序即优先级。
pub fn annotate_product_mod_paths_with_lines(
    code: &str,
    child_src_dir: &Path,
    product_dir: &Path,
    project_root: &Path,
    extensions: &[String],
) -> (String, Vec<usize>) {
    crate::module_path::annotate_mod_paths_with_lines(code, |name| {
        for ext in extensions {
            let src = child_src_dir.join(format!("{name}.{}", ext.trim_start_matches('.')));
            if src.is_file() {
                return Some(relative_path(
                    product_dir,
                    &gen_product_path(project_root, &src),
                ));
            }
        }
        let handwritten = child_src_dir.join(format!("{name}.rs"));
        if handwritten.is_file() {
            return Some(relative_path(product_dir, &handwritten));
        }
        None
    })
}

/// 路径是否落在本项目的托管目录下
///
/// 用于区分写入语义：托管目录内都是机器产物（用户从不编辑），覆盖无需
/// 备份；`src/main.rs` 与项目外源文件旁的产物可能与用户手写文件同名，
/// 必须保留「内容不同先备份」的护栏。
pub fn is_managed_product(project_root: &Path, path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        // 目标还不存在（首次写入）：按字面路径判定，只看父目录是否在托管树下
        return path.starts_with(project_root.join(GEN_DIR));
    };
    // 已存在的真实路径同样要落在本项目根下（符号链接可能指向项目外）
    if !meta.is_file() {
        return false;
    }
    let real = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let root = normalize_abs(&project_root.join(GEN_DIR));
    normalize_abs(&real).starts_with(root)
}

/// 清理托管目录中已失去母语源文件的产物，返回被删除的文件路径（已排序）
///
/// 托管目录是本工具的专属地盘（`src/` 里的手写 `.rs` 从不复制进来），其中
/// 「找不到对应源文件」的 `.rs` 只有一种来历：源文件被删除或改名后留下的
/// 旧产物。留着并非无害——rustc 按默认规则解析 `mod 工具;` 时会在产物同级
/// 目录里找到它（`.rz/gen/src/领域/工具.rs`），于是被删掉的代码还在参与编译，
/// 报错位置指向一个已经没有源文件的生成物。旧布局下不敢做这种清理
/// （`src/` 混着学习者手写 `.rs`，删错就是毁数据），托管后判定是纯本地的：
/// 只看源文件在不在（并顺手清掉旧写入语义留下的 `.rs.bak`），绝不触碰
/// `.rz/gen/` 以外的任何文件；符号链接一律不跟随。
pub fn prune_orphan_products(project_root: &Path, extensions: &[String]) -> Vec<PathBuf> {
    let gen_root = project_root.join(GEN_DIR);
    let Ok(meta) = std::fs::symlink_metadata(&gen_root) else {
        return Vec::new();
    };
    // 托管根被换成符号链接（异常环境）：不顺着走，避免删到目录外的文件
    if !meta.is_dir() {
        return Vec::new();
    }
    let mut orphans: Vec<PathBuf> = Vec::new();
    let mut dirs: Vec<PathBuf> = vec![gen_root.clone()];
    let mut stack = vec![gen_root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(ft) = entry.file_type() else { continue };
            // 符号链接一律不跟随：既不删也不下钻
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                dirs.push(path.clone());
                stack.push(path);
            } else if ft.is_file() && is_junk_product(project_root, &path, &gen_root, extensions) {
                orphans.push(path);
            }
        }
    }
    let mut deleted = Vec::new();
    for path in &orphans {
        if std::fs::remove_file(path).is_ok() {
            deleted.push(path.clone());
        }
    }
    // 产物清空后的空目录自底向上移除（托管根本身保留，下次写入省一次创建）
    dirs.sort_by_key(|b| std::cmp::Reverse(b.components().count()));
    for dir in dirs {
        if dir == gen_root {
            continue;
        }
        if std::fs::read_dir(&dir).is_ok_and(|mut rd| rd.next().is_none()) {
            let _ = std::fs::remove_dir(&dir);
        }
    }
    deleted.sort();
    deleted
}

/// 托管目录下的文件是否属于可安全删除的机器产物
///
/// 两类：失去源文件的 `.rs` 产物，以及旧写入语义在同一产物变更时留下的
/// `.rs.bak` 备份（内容就是本目录里那个 `.rs` 的上一版本，不含用户数据）。
/// 其余文件（用户自己放进来的东西）一律保留。
fn is_junk_product(
    project_root: &Path,
    path: &Path,
    gen_root: &Path,
    extensions: &[String],
) -> bool {
    if is_product_backup(path) {
        return true;
    }
    has_product_extension(path) && !has_source(project_root, path, gen_root, extensions)
}

/// 是否为产物扩展名（`.rs`）的文件
fn has_product_extension(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "rs")
}

/// 是否为产物备份（`.rs.bak`）
fn is_product_backup(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".rs.bak"))
}

/// 托管产物是否还有对应的母语源文件（源文件在源码树里的镜像位置）
fn has_source(project_root: &Path, product: &Path, gen_root: &Path, extensions: &[String]) -> bool {
    let Ok(rel) = product.strip_prefix(gen_root) else {
        return true; // 不在托管根下（理论不可达）：保守视为有源，不删
    };
    let source_base = project_root.join(rel);
    extensions.iter().any(|ext| {
        source_base
            .with_extension(ext.trim_start_matches('.'))
            .is_file()
    })
}

/// 项目根相对路径（源文件/产物可能是相对路径，先按当前目录补齐）
fn relative_to_root(project_root: &Path, path: &Path) -> Option<PathBuf> {
    let root = normalize_abs(project_root);
    let abs = normalize_abs(path);
    abs.strip_prefix(&root).ok().map(PathBuf::from)
}

/// 路径词法规整为绝对形式：相对路径按当前目录补齐，取不到当前目录时
/// 只做词法规整（比较退化为词法相等）
fn normalize_abs(path: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    normalize_lexical(&abs)
}

/// 词法规整路径：就地消解 `.`/`..`（不访问文件系统、不解析符号链接）
///
/// rustc 报告 `#[path]` 子模块时把父文件目录与注解值直接拼接、不做规整
/// （`src/main.rs` + `../.rz/gen/src/数学.rs` → `src/../.rz/gen/src/数学.rs`），
/// 诊断回译要认出托管产物就必须先词法化；`..` 在此只作字面段处理，
/// 符号链接场景下会失真，但本项目内产物与源码均按字面路径生成。
fn normalize_lexical(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    let mut stack: Vec<std::ffi::OsString> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if stack.last().is_some_and(|last| last != "..") {
                    stack.pop();
                } else if !out.exists() {
                    stack.push(std::ffi::OsString::from(".."));
                }
            }
            other => {
                if matches!(other, Component::Normal(_)) {
                    stack.push(other.as_os_str().to_os_string());
                } else {
                    out.push(other.as_os_str());
                }
            }
        }
    }
    for seg in stack {
        out.push(seg);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("/proj")
    }

    #[test]
    fn 非入口产物镜像源码布局() {
        assert_eq!(
            gen_product_path(&root(), Path::new("/proj/src/数学.zh")),
            PathBuf::from("/proj/.rz/gen/src/数学.rs")
        );
        assert_eq!(
            gen_product_path(&root(), Path::new("/proj/src/领域/工具.zh")),
            PathBuf::from("/proj/.rz/gen/src/领域/工具.rs")
        );
    }

    #[test]
    fn 项目外源文件产物留在原地() {
        assert_eq!(
            gen_product_path(&root(), Path::new("/home/u/示例.zh")),
            PathBuf::from("/home/u/示例.rs")
        );
    }

    #[test]
    fn 托管前缀剥离只认托管目录() {
        assert_eq!(
            strip_gen_prefix(&root(), Path::new("/proj/.rz/gen/src/数学.rs")),
            Some(PathBuf::from("/proj/src/数学.rs"))
        );
        assert_eq!(
            strip_gen_prefix(&root(), Path::new("/proj/src/数学.rs")),
            None
        );
        // rustc 报的子模块路径不规整（父文件目录 + `#[path]` 值直接拼接），
        // 词法规整后才能认出托管产物（否则回译失效，诊断指向生成物路径）
        assert_eq!(
            strip_gen_prefix(&root(), Path::new("/proj/src/../.rz/gen/src/数学.rs")),
            Some(PathBuf::from("/proj/src/数学.rs"))
        );
        // 别的项目下的同名目录不误剥（前缀必须落在本项目根之下）
        assert_eq!(
            strip_gen_prefix(&root(), Path::new("/other/.rz/gen/src/数学.rs")),
            None
        );
    }

    #[test]
    fn 相对路径计算覆盖出入口两个方向() {
        // 入口（src/）指向托管子模块
        assert_eq!(
            relative_path(
                Path::new("/proj/src"),
                Path::new("/proj/.rz/gen/src/数学.rs")
            ),
            "../.rz/gen/src/数学.rs"
        );
        // 托管产物之间：与旧布局逐字一致
        assert_eq!(
            relative_path(
                Path::new("/proj/.rz/gen/src"),
                Path::new("/proj/.rz/gen/src/领域/工具.rs")
            ),
            "领域/工具.rs"
        );
        // 托管产物指回手写 .rs（逐层上跳到项目根再进 src/）
        assert_eq!(
            relative_path(
                Path::new("/proj/.rz/gen/src/领域"),
                Path::new("/proj/src/领域/helper.rs")
            ),
            "../../../../src/领域/helper.rs"
        );
        assert_eq!(
            relative_path(Path::new("/proj/src"), Path::new("/proj/src/x.rs")),
            "x.rs"
        );
    }

    /// 端到端：入口产物注解指向托管目录，非 ASCII 与 ASCII 方言子模块都覆盖
    #[test]
    fn 入口注解指向托管产物并按源文件存在性取舍() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/数学.zh"), "fn a() {}\n").unwrap();
        std::fs::write(root.join("src/util.zh"), "fn b() {}\n").unwrap();
        std::fs::write(root.join("src/helper.rs"), "pub fn c() {}\n").unwrap();
        let (out, line_map) = annotate_product_mod_paths_with_lines(
            "mod 数学;\nmod util;\nmod helper;\nmod 不存在;\n",
            &root.join("src"),
            &root.join("src"),
            root,
            &["zh".to_string()],
        );
        assert!(
            out.contains("#[path = \"../.rz/gen/src/数学.rs\"]"),
            "非 ASCII 方言子模块应指向托管产物：{out}"
        );
        assert!(
            out.contains("#[path = \"../.rz/gen/src/util.rs\"]"),
            "ASCII 方言子模块同样要指向托管产物（旧布局的自动查找已失效）：{out}"
        );
        assert!(
            out.contains("#[path = \"helper.rs\"]"),
            "手写 .rs 子模块留在 src/：{out}"
        );
        assert_eq!(
            out.matches("#[path").count(),
            3,
            "无源文件的模块不注解（保留 rustc 原生 E0583）：{out}"
        );
        // 每条注解一行，行映射把注解行归属其 mod 声明行
        assert_eq!(line_map.len(), out.lines().count() + 1);
    }

    /// 托管产物自身注解：子模块相对位置与旧布局一致，手写 .rs 走逃逸路径
    #[test]
    fn 托管内注解与旧布局逐字一致() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/领域")).unwrap();
        std::fs::create_dir_all(root.join(".rz/gen/src")).unwrap();
        std::fs::write(root.join("src/领域/工具.zh"), "fn a() {}\n").unwrap();
        std::fs::write(root.join("src/领域/helper.rs"), "fn b() {}\n").unwrap();
        let (out, _) = annotate_product_mod_paths_with_lines(
            "mod 工具;\nmod helper;\n",
            &root.join("src/领域"),
            &root.join(".rz/gen/src"),
            root,
            &["zh".to_string()],
        );
        assert!(
            out.contains("#[path = \"领域/工具.rs\"]"),
            "方言子模块在托管树内的相对位置不变：{out}"
        );
        assert!(
            out.contains("#[path = \"../../../src/领域/helper.rs\"]"),
            "手写子模块要跨出托管目录回到 src/：{out}"
        );
    }

    /// 清理只看源文件在不在：改名/删除后的残留被收走，在源的产物与
    /// 非产物文件（用户自己往托管目录放的东西）一律不动
    #[test]
    fn 清理失效产物只删无源的产物() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".rz/gen/src/领域")).unwrap();
        std::fs::write(root.join(".rz/gen/src/数学.rs"), "pub fn a() {}\n").unwrap();
        std::fs::write(root.join(".rz/gen/src/领域/工具.rs"), "pub fn b() {}\n").unwrap();
        std::fs::write(root.join(".rz/gen/src/旧名.rs"), "pub fn c() {}\n").unwrap();
        std::fs::write(root.join(".rz/gen/src/笔记.txt"), "keep\n").unwrap();
        // 源文件仍在（含嵌套层与多扩展名候选）
        std::fs::create_dir_all(root.join("src/领域")).unwrap();
        std::fs::write(root.join("src/数学.zh"), "公开 函数 a() {}\n").unwrap();
        std::fs::write(root.join("src/领域/工具.ja"), "公開 関数 a() {}\n").unwrap();
        let deleted = prune_orphan_products(root, &["zh".to_string(), "ja".to_string()]);
        assert_eq!(
            deleted,
            vec![root.join(".rz/gen/src/旧名.rs")],
            "只有无源文件的产物被删"
        );
        assert!(root.join(".rz/gen/src/数学.rs").is_file());
        assert!(root.join(".rz/gen/src/领域/工具.rs").is_file());
        assert!(root.join(".rz/gen/src/笔记.txt").is_file());
        // 源文件一个都没少时，重复清理为空操作
        assert!(prune_orphan_products(root, &["zh".to_string(), "ja".to_string()]).is_empty());
    }

    /// 产物所在的空目录一并移除（嵌套目录整体消失，不留空壳），
    /// 仍有产物的目录与托管根本身保留
    #[test]
    fn 失效产物所在的空目录被移除() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/留着")).unwrap();
        std::fs::create_dir_all(root.join(".rz/gen/src/领域/更深")).unwrap();
        std::fs::write(
            root.join(".rz/gen/src/领域/更深/工具.rs"),
            "pub fn a() {}\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join(".rz/gen/src/留着")).unwrap();
        std::fs::write(root.join(".rz/gen/src/留着/内容.rs"), "pub fn b() {}\n").unwrap();
        // 镜像位置的源文件还在（产物 留着/内容.rs ← 源 src/留着/内容.zh）
        std::fs::write(root.join("src/留着/内容.zh"), "公开 函数 b() {}\n").unwrap();
        let deleted = prune_orphan_products(root, &["zh".to_string()]);
        assert_eq!(deleted, vec![root.join(".rz/gen/src/领域/更深/工具.rs")]);
        assert!(!root.join(".rz/gen/src/领域").exists(), "整个分支已无产物");
        assert!(root.join(".rz/gen/src/留着/内容.rs").is_file());
        assert!(root.join(".rz/gen").is_dir(), "托管根保留");
    }

    /// 无托管目录（纯入口项目 / 从未跑过非入口文件）时静默返回
    #[test]
    fn 没有托管目录时不报错() {
        let dir = tempfile::tempdir().unwrap();
        assert!(prune_orphan_products(dir.path(), &["zh".to_string()]).is_empty());
    }

    /// 旧写入语义在托管目录里留下的 `.rs.bak`（同一产物的上一版本）也被清走，
    /// 而源文件还在的正常产物不受影响
    #[test]
    fn 托管目录内的产物备份一并清理() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/数学.zh"), "公开 函数 a() {}\n").unwrap();
        std::fs::create_dir_all(root.join(".rz/gen/src")).unwrap();
        std::fs::write(root.join(".rz/gen/src/数学.rs"), "pub fn a() {}\n").unwrap();
        std::fs::write(root.join(".rz/gen/src/数学.rs.bak"), "旧版本\n").unwrap();
        let deleted = prune_orphan_products(root, &["zh".to_string()]);
        assert_eq!(deleted, vec![root.join(".rz/gen/src/数学.rs.bak")]);
        assert!(root.join(".rz/gen/src/数学.rs").is_file());
    }

    /// 写入语义分流：只有本项目托管树下的文件算「机器产物」
    #[test]
    fn 托管产物判定不越项目边界() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".rz/gen/src")).unwrap();
        std::fs::write(root.join(".rz/gen/src/数学.rs"), "pub fn a() {}\n").unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        assert!(is_managed_product(root, &root.join(".rz/gen/src/数学.rs")));
        // 首次写入（文件还不存在）同样按字面路径归入托管树
        assert!(is_managed_product(
            root,
            &root.join(".rz/gen/src/新文件.rs")
        ));
        assert!(!is_managed_product(root, &root.join("src/main.rs")));
        // 同名目录属于另一个项目时不算（否则备份护栏被绕过，真风险）
        let 另项目 = Path::new("/other/.rz/gen/src/数学.rs");
        assert!(!is_managed_product(root, 另项目));
    }
}
