//! 转译产物磁盘布局：非入口方言文件的 `.rs` 产物写入项目内托管目录
//! `.rz/gen/`，镜像源码目录树（`src/数学.zh` → `.rz/gen/src/数学.rs`）。
//!
//! 动机：源码目录是学习者唯一会去看的目录，旧布局把每个方言文件的英文产物
//! 平铺在源文件旁边（`src/数学.zh` + `src/数学.rs`），母语教学目录里一半
//! 内容是生成物。托管后源码目录只剩母语源文件与 Cargo 固定入口产物
//! `src/main.rs`（入口必须留在 Cargo 默认发现位置，无法移出）。
//!
//! 关键不变量：**托管目录镜像源码布局**，所以非入口产物之间的相对位置与
//! 旧布局完全一致，`mod 工具;` 的 `#[path]` 前缀不变；只有入口产物
//! （仍在源码根目录）指向子模块时需要 `../.rz/gen/src/…` 前缀——由
//! [`注解产物模块路径并返回行映射`] 统一计算，CLI 与 LSP 镜像
//! 共用同一实现，避免两侧规则漂移。

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// 产物托管目录（相对项目根）
pub const 托管目录: &str = ".rz/gen";

/// 方言源文件 → 非入口产物路径（镜像源码布局，保持项目根相对位置）
///
/// 源文件不在项目根之下（`rzc check ~/示例.zh` 等）时退回旧行为：
/// 产物写在源文件旁边，不硬造 `.rz/gen/` 结构。
pub fn 产物路径(项目根: &Path, 源文件: &Path) -> PathBuf {
    if let Some(相对路径值) = 相对项目根路径(项目根, 源文件) {
        项目根.join(托管目录).join(相对路径值).with_extension("rs")
    } else {
        源文件.with_extension("rs")
    }
}

/// 位于托管目录下的产物 → 去掉托管前缀的等价路径（仍为绝对路径）
///
/// `.rz/gen/src/数学.rs` → `<root>/src/数学.rs`；不在托管目录下返回「无」。
/// 供诊断回译使用：rustc 报的是托管产物路径，母语源文件在源码树里。
pub fn 去托管前缀(项目根: &Path, 入口路径: &Path) -> Option<PathBuf> {
    let 相对路径值 = 相对项目根路径(项目根, 入口路径)?;
    let 内部路径 = 相对路径值.strip_prefix(托管目录).ok()?;
    Some(项目根.join(内部路径))
}

/// 从起始目录到目标路径的词法相对路径（`#[path]` 用 `/` 分隔）
///
/// `#[path]` 由 rustc 相对**声明所在文件的目录**解析，故起始目录参数传目录。
/// 纯词法计算（不访问文件系统、不解析符号链接）：本项目内产物与源码都在
/// 同一根目录下按字面路径生成，无需真实路径。
pub fn 相对路径(起始目录: &Path, 目标路径: &Path) -> String {
    let 起点 = 规整为绝对路径(起始目录);
    let 终点 = 规整为绝对路径(目标路径);
    let 起段: Vec<_> = 起点.components().collect();
    let 终段: Vec<_> = 终点.components().collect();
    let 公共段数 = 起段
        .iter()
        .zip(终段.iter())
        .take_while(|(甲, 乙)| 甲 == 乙)
        .count();
    let mut 拼接串: Vec<String> = Vec::new();
    for _ in 公共段数..起段.len() {
        拼接串.push("..".to_string());
    }
    for 段 in &终段[公共段数..] {
        拼接串.push(段.as_os_str().to_string_lossy().into_owned());
    }
    if 拼接串.is_empty() {
        ".".to_string()
    } else {
        拼接串.join("/")
    }
}

/// 文件式 `mod 名称;` 声明的 `#[path]` 注解（托管布局版），并返回行映射
///
/// 逐模块名解析实际落点，两类命中者才注解（其余保留 rustc 原生 E0583，
/// 与旧布局同口径）：
/// 1. `child_src_dir/名称.<方言>` 存在 → 指向其托管产物（镜像布局）；
/// 2. `child_src_dir/名称.rs` 存在（手写 `.rs` 子模块）→ 指向它本身。
///
/// 注解路径相对产物目录（当前文件**产物**所在目录）计算：入口产物在
/// 源码根目录时得到 `../.rz/gen/src/…`，非入口产物在 `.rz/gen/src/` 时子模块
/// 之间仍是 `领域/工具.rs`，与旧布局逐字一致。
///
/// 扩展名序列为可用方言扩展名（可带点），顺序即优先级。
pub fn 注解产物模块路径并返回行映射(
    代码: &str,
    子源码目录: &Path,
    产物目录: &Path,
    项目根: &Path,
    扩展名序列: &[String],
) -> (String, Vec<usize>) {
    crate::模块路径::注解模块路径并返回行映射(代码, |名称| {
        for 后缀 in 扩展名序列 {
            let 源路径 = 子源码目录.join(format!("{}.{}", 名称, 后缀.trim_start_matches('.')));
            if 源路径.is_file() {
                return Some(相对路径(产物目录, &产物路径(项目根, &源路径)));
            }
        }
        let 手写路径 = 子源码目录.join(format!("{}.rs", 名称));
        if 手写路径.is_file() {
            return Some(相对路径(产物目录, &手写路径));
        }
        None
    })
}

/// 路径是否落在本项目的托管目录下
///
/// 用于区分写入语义：托管目录内都是机器产物（用户从不编辑），覆盖无需
/// 备份；`src/main.rs` 与项目外源文件旁的产物可能与用户手写文件同名，
/// 必须保留「内容不同先备份」的护栏。
pub fn 是否为托管产物(项目根: &Path, 目标路径: &Path) -> bool {
    let Ok(元数据) = std::fs::symlink_metadata(目标路径) else {
        // 目标还不存在（首次写入）：按字面路径判定，只看父目录是否在托管树下
        return 目标路径.starts_with(项目根.join(托管目录));
    };
    // 已存在的真实路径同样要落在本项目根下（符号链接可能指向项目外）
    if !元数据.is_file() {
        return false;
    }
    let 真实路径 = 目标路径
        .canonicalize()
        .unwrap_or_else(|_| 目标路径.to_path_buf());
    let 根路径 = 规整为绝对路径(&项目根.join(托管目录));
    规整为绝对路径(&真实路径).starts_with(根路径)
}

/// 清理托管目录中已失去母语源文件的产物，返回被删除的文件路径（已排序）
///
/// 托管目录是本工具的专属地盘（源码目录里的手写 `.rs` 从不复制进来），其中
/// 「找不到对应源文件」的 `.rs` 只有一种来历：源文件被删除或改名后留下的
/// 旧产物。留着并非无害——rustc 按默认规则解析 `mod 工具;` 时会在产物同级
/// 目录里找到它（`.rz/gen/src/领域/工具.rs`），于是被删掉的代码还在参与编译，
/// 报错位置指向一个已经没有源文件的生成物。旧布局下不敢做这种清理
/// （源码目录混着学习者手写 `.rs`，删错就是毁数据），托管后判定是纯本地的：
/// 只看源文件在不在（并顺手清掉旧写入语义留下的 `.rs.bak`），绝不触碰
/// `.rz/gen/` 以外的任何文件；符号链接一律不跟随。
pub fn 清理失效产物(项目根: &Path, 扩展名序列: &[String]) -> Vec<PathBuf> {
    let 托管根 = 项目根.join(托管目录);
    let Ok(元数据) = std::fs::symlink_metadata(&托管根) else {
        return Vec::new();
    };
    // 托管根被换成符号链接（异常环境）：不顺着走，避免删到目录外的文件
    if !元数据.is_dir() {
        return Vec::new();
    }
    let mut 失效产物: Vec<PathBuf> = Vec::new();
    let mut 目录集: Vec<PathBuf> = vec![托管根.clone()];
    let mut 待走访: Vec<PathBuf> = vec![托管根.clone()];
    while let Some(目录) = 待走访.pop() {
        let Ok(目录项) = std::fs::read_dir(&目录) else {
            continue;
        };
        for 条目 in 目录项.flatten() {
            let 条目路径 = 条目.path();
            let Ok(文件类型) = 条目.file_type() else {
                continue;
            };
            // 符号链接一律不跟随：既不删也不下钻
            if 文件类型.is_symlink() {
                continue;
            }
            if 文件类型.is_dir() {
                目录集.push(条目路径.clone());
                待走访.push(条目路径);
            } else if 文件类型.is_file()
                && 是否为废弃产物(项目根, &条目路径, &托管根, 扩展名序列)
            {
                失效产物.push(条目路径);
            }
        }
    }
    let mut 已删除 = Vec::new();
    for 条目路径 in &失效产物 {
        if std::fs::remove_file(条目路径).is_ok() {
            已删除.push(条目路径.clone());
        }
    }
    // 产物清空后的空目录自底向上移除（托管根本身保留，下次写入省一次创建）：
    // 按键升序后整体反转，等价于按组件数降序（同层目录的先后不影响结果）
    目录集.sort_by_key(|目录| 目录.components().count());
    目录集.reverse();
    for 目录 in 目录集 {
        if 目录 == 托管根 {
            continue;
        }
        if std::fs::read_dir(&目录).is_ok_and(|mut 读取器| 读取器.next().is_none()) {
            let _ = std::fs::remove_dir(&目录);
        }
    }
    已删除.sort();
    已删除
}

/// 托管目录下的文件是否属于可安全删除的机器产物
///
/// 两类：失去源文件的 `.rs` 产物，以及旧写入语义在同一产物变更时留下的
/// `.rs.bak` 备份（内容就是本目录里那个 `.rs` 的上一版本，不含用户数据）。
/// 其余文件（用户自己放进来的东西）一律保留。
fn 是否为废弃产物(
    项目根: &Path,
    目标路径: &Path,
    托管根: &Path,
    扩展名序列: &[String],
) -> bool {
    if 是否为产物备份(目标路径) {
        return true;
    }
    是否有产物扩展名(目标路径) && !是否有源文件(项目根, 目标路径, 托管根, 扩展名序列)
}

/// 是否为产物扩展名（`.rs`）的文件
fn 是否有产物扩展名(目标路径: &Path) -> bool {
    目标路径.extension().is_some_and(|扩展名| 扩展名 == "rs")
}

/// 是否为产物备份（`.rs.bak`）
fn 是否为产物备份(目标路径: &Path) -> bool {
    目标路径
        .file_name()
        .and_then(|名称| 名称.to_str())
        .is_some_and(|名称| 名称.ends_with(".rs.bak"))
}

/// 托管产物是否还有对应的母语源文件（源文件在源码树里的镜像位置）
fn 是否有源文件(
    项目根: &Path,
    产物路径值: &Path,
    托管根: &Path,
    扩展名序列: &[String],
) -> bool {
    if let Ok(相对路径值) = 产物路径值.strip_prefix(托管根) {
        let 源基 = 项目根.join(相对路径值);
        扩展名序列
            .iter()
            .any(|后缀| 源基.with_extension(后缀.trim_start_matches('.')).is_file())
    } else {
        true // 不在托管根下（理论不可达）：保守视为有源，不删
    }
}

/// 项目根相对路径（源文件/产物可能是相对路径，先按当前目录补齐）
pub fn 相对项目根路径(项目根: &Path, 目标路径: &Path) -> Option<PathBuf> {
    let 根路径 = 规整为绝对路径(项目根);
    let 绝对路径 = 规整为绝对路径(目标路径);
    绝对路径.strip_prefix(&根路径).ok().map(PathBuf::from)
}

/// 路径词法规整为绝对形式：相对路径按当前目录补齐，取不到当前目录时
/// 只做词法规整（比较退化为词法相等）
fn 规整为绝对路径(目标路径: &Path) -> PathBuf {
    if 目标路径.is_absolute() {
        return 规整词法路径(目标路径, true);
    }
    if let Ok(当前目录) = std::env::current_dir() {
        规整词法路径(&当前目录.join(目标路径), true)
    } else {
        // 取不到当前目录：按原样做词法规整（首个非 `.`/`..` 段不算根）
        规整词法路径(目标路径, false)
    }
}

/// 词法规整路径：就地消解 `.`/`..`（不访问文件系统、不解析符号链接）
///
/// rustc 报告 `#[path]` 子模块时把父文件目录与注解值直接拼接、不做规整
/// （`src/main.rs` + `../.rz/gen/src/数学.rs` → `src/../.rz/gen/src/数学.rs`），
/// 诊断回译要认出托管产物就必须先词法化；`..` 在此只作字面段处理，
/// 符号链接场景下会失真，但本项目内产物与源码均按字面路径生成。
///
/// `已有根` 为真表示输入是绝对路径（或由当前目录补齐），首个非 `.`/`..`
/// 段就是根/前缀段，落到输出并阻止后续 `..` 越过根；为假时全部按普通段处理。
fn 规整词法路径(目标路径: &Path, 已有根: bool) -> PathBuf {
    let mut 输出 = PathBuf::new();
    let mut 段栈: Vec<OsString> = Vec::new();
    let mut 根已落位 = !已有根;
    for 组件 in 目标路径.components() {
        let 原始段 = 组件.as_os_str();
        let 文案 = 原始段.to_string_lossy();
        if 文案 == "." {
            continue;
        }
        if 文案 == ".." {
            if 段栈.last().is_some_and(|末段| 末段 != "..") {
                段栈.pop();
            } else if !输出.exists() {
                段栈.push(OsString::from(".."));
            }
            continue;
        }
        if !根已落位 {
            // 根/前缀段（首个非普通段）落到输出：后续 `..` 不会越过它
            输出.push(原始段);
            根已落位 = true;
        } else {
            段栈.push(OsString::from(原始段));
        }
    }
    for 段 in 段栈 {
        输出.push(段);
    }
    输出
}

#[cfg(test)]
mod 母语测试 {
    use super::*;

    fn 根目录() -> PathBuf {
        PathBuf::from("/proj")
    }

    #[test]
    fn 非入口产物镜像源码布局() {
        assert_eq!(
            产物路径(&根目录(), Path::new("/proj/src/数学.zh")),
            PathBuf::from("/proj/.rz/gen/src/数学.rs")
        );
        assert_eq!(
            产物路径(&根目录(), Path::new("/proj/src/领域/工具.zh")),
            PathBuf::from("/proj/.rz/gen/src/领域/工具.rs")
        );
    }

    #[test]
    fn 项目外源文件产物留在原地() {
        assert_eq!(
            产物路径(&根目录(), Path::new("/home/u/示例.zh")),
            PathBuf::from("/home/u/示例.rs")
        );
    }

    #[test]
    fn 托管前缀剥离只认托管目录() {
        assert_eq!(
            去托管前缀(&根目录(), Path::new("/proj/.rz/gen/src/数学.rs")),
            Some(PathBuf::from("/proj/src/数学.rs"))
        );
        assert_eq!(去托管前缀(&根目录(), Path::new("/proj/src/数学.rs")), None);
        // rustc 报的子模块路径不规整（父文件目录 + `#[path]` 值直接拼接），
        // 词法规整后才能认出托管产物（否则回译失效，诊断指向生成物路径）
        assert_eq!(
            去托管前缀(&根目录(), Path::new("/proj/src/../.rz/gen/src/数学.rs")),
            Some(PathBuf::from("/proj/src/数学.rs"))
        );
        // 别的项目下的同名目录不误剥（前缀必须落在本项目根之下）
        assert_eq!(
            去托管前缀(&根目录(), Path::new("/other/.rz/gen/src/数学.rs")),
            None
        );
    }

    #[test]
    fn 相对路径计算覆盖出入口两个方向() {
        // 入口（源码根目录）指向托管子模块
        assert_eq!(
            相对路径(
                Path::new("/proj/src"),
                Path::new("/proj/.rz/gen/src/数学.rs")
            ),
            "../.rz/gen/src/数学.rs"
        );
        // 托管产物之间：与旧布局逐字一致
        assert_eq!(
            相对路径(
                Path::new("/proj/.rz/gen/src"),
                Path::new("/proj/.rz/gen/src/领域/工具.rs")
            ),
            "领域/工具.rs"
        );
        // 托管产物指回手写 .rs（逐层上跳到项目根再进源码目录）
        assert_eq!(
            相对路径(
                Path::new("/proj/.rz/gen/src/领域"),
                Path::new("/proj/src/领域/helper.rs")
            ),
            "../../../../src/领域/helper.rs"
        );
        assert_eq!(
            相对路径(Path::new("/proj/src"), Path::new("/proj/src/x.rs")),
            "x.rs"
        );
    }

    /// 端到端：入口产物注解指向托管目录，非 ASCII 与 ASCII 方言子模块都覆盖
    #[test]
    fn 入口注解指向托管产物并按源文件存在性取舍() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根路径 = 临时.path();
        std::fs::create_dir_all(根路径.join("src")).unwrap();
        std::fs::write(根路径.join("src/数学.zh"), "fn a() {}\n").unwrap();
        std::fs::write(根路径.join("src/util.zh"), "fn b() {}\n").unwrap();
        std::fs::write(根路径.join("src/helper.rs"), "pub fn c() {}\n").unwrap();
        let (输出, 行映射) = 注解产物模块路径并返回行映射(
            "mod 数学;\nmod util;\nmod helper;\nmod 不存在;\n",
            &根路径.join("src"),
            &根路径.join("src"),
            根路径,
            &["zh".to_string()],
        );
        assert!(
            输出.contains("#[path = \"../.rz/gen/src/数学.rs\"]"),
            "非 ASCII 方言子模块应指向托管产物：{输出}"
        );
        assert!(
            输出.contains("#[path = \"../.rz/gen/src/util.rs\"]"),
            "ASCII 方言子模块同样要指向托管产物（旧布局的自动查找已失效）：{输出}"
        );
        assert!(
            输出.contains("#[path = \"helper.rs\"]"),
            "手写 .rs 子模块留在 src/：{输出}"
        );
        assert_eq!(
            输出.matches("#[path").count(),
            3,
            "无源文件的模块不注解（保留 rustc 原生 E0583）：{输出}"
        );
        // 每条注解一行，行映射把注解行归属其模块声明行
        assert_eq!(行映射.len(), 输出.lines().count() + 1);
    }

    /// 托管产物自身注解：子模块相对位置与旧布局一致，手写 .rs 走逃逸路径
    #[test]
    fn 托管内注解与旧布局逐字一致() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根路径 = 临时.path();
        std::fs::create_dir_all(根路径.join("src/领域")).unwrap();
        std::fs::create_dir_all(根路径.join(".rz/gen/src")).unwrap();
        std::fs::write(根路径.join("src/领域/工具.zh"), "fn a() {}\n").unwrap();
        std::fs::write(根路径.join("src/领域/helper.rs"), "fn b() {}\n").unwrap();
        let (输出, _) = 注解产物模块路径并返回行映射(
            "mod 工具;\nmod helper;\n",
            &根路径.join("src/领域"),
            &根路径.join(".rz/gen/src"),
            根路径,
            &["zh".to_string()],
        );
        assert!(
            输出.contains("#[path = \"领域/工具.rs\"]"),
            "方言子模块在托管树内的相对位置不变：{输出}"
        );
        assert!(
            输出.contains("#[path = \"../../../src/领域/helper.rs\"]"),
            "手写子模块要跨出托管目录回到 src/：{输出}"
        );
    }

    /// 清理只看源文件在不在：改名/删除后的残留被收走，在源的产物与
    /// 非产物文件（用户自己往托管目录放的东西）一律不动
    #[test]
    fn 清理失效产物只删无源的产物() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根路径 = 临时.path();
        std::fs::create_dir_all(根路径.join(".rz/gen/src/领域")).unwrap();
        std::fs::write(根路径.join(".rz/gen/src/数学.rs"), "pub fn a() {}\n").unwrap();
        std::fs::write(根路径.join(".rz/gen/src/领域/工具.rs"), "pub fn b() {}\n").unwrap();
        std::fs::write(根路径.join(".rz/gen/src/旧名.rs"), "pub fn c() {}\n").unwrap();
        std::fs::write(根路径.join(".rz/gen/src/笔记.txt"), "keep\n").unwrap();
        // 源文件仍在（含嵌套层与多扩展名候选）
        std::fs::create_dir_all(根路径.join("src/领域")).unwrap();
        std::fs::write(根路径.join("src/数学.zh"), "公开 函数 a() {}\n").unwrap();
        std::fs::write(根路径.join("src/领域/工具.ja"), "公開 関数 a() {}\n").unwrap();
        let 已删除 = 清理失效产物(根路径, &["zh".to_string(), "ja".to_string()]);
        assert_eq!(
            已删除,
            vec![根路径.join(".rz/gen/src/旧名.rs")],
            "只有无源文件的产物被删"
        );
        assert!(根路径.join(".rz/gen/src/数学.rs").is_file());
        assert!(根路径.join(".rz/gen/src/领域/工具.rs").is_file());
        assert!(根路径.join(".rz/gen/src/笔记.txt").is_file());
        // 源文件一个都没少时，重复清理为空操作
        assert!(清理失效产物(根路径, &["zh".to_string(), "ja".to_string()]).is_empty());
    }

    /// 产物所在的空目录一并移除（嵌套目录整体消失，不留空壳），
    /// 仍有产物的目录与托管根本身保留
    #[test]
    fn 失效产物所在的空目录被移除() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根路径 = 临时.path();
        std::fs::create_dir_all(根路径.join("src/留着")).unwrap();
        std::fs::create_dir_all(根路径.join(".rz/gen/src/领域/更深")).unwrap();
        std::fs::write(
            根路径.join(".rz/gen/src/领域/更深/工具.rs"),
            "pub fn a() {}\n",
        )
        .unwrap();
        std::fs::create_dir_all(根路径.join(".rz/gen/src/留着")).unwrap();
        std::fs::write(根路径.join(".rz/gen/src/留着/内容.rs"), "pub fn b() {}\n").unwrap();
        // 镜像位置的源文件还在（产物 留着/内容.rs ← 源 src/留着/内容.zh）
        std::fs::write(根路径.join("src/留着/内容.zh"), "公开 函数 b() {}\n").unwrap();
        let 已删除 = 清理失效产物(根路径, &["zh".to_string()]);
        assert_eq!(已删除, vec![根路径.join(".rz/gen/src/领域/更深/工具.rs")]);
        assert!(
            !根路径.join(".rz/gen/src/领域").exists(),
            "整个分支已无产物"
        );
        assert!(根路径.join(".rz/gen/src/留着/内容.rs").is_file());
        assert!(根路径.join(".rz/gen").is_dir(), "托管根保留");
    }

    /// 无托管目录（纯入口项目 / 从未跑过非入口文件）时静默返回
    #[test]
    fn 没有托管目录时不报错() {
        let 临时 = tempfile::tempdir().unwrap();
        assert!(清理失效产物(临时.path(), &["zh".to_string()]).is_empty());
    }

    /// 旧写入语义在托管目录里留下的 `.rs.bak`（同一产物的上一版本）也被清走，
    /// 而源文件还在的正常产物不受影响
    #[test]
    fn 托管目录内的产物备份一并清理() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根路径 = 临时.path();
        std::fs::create_dir_all(根路径.join("src")).unwrap();
        std::fs::write(根路径.join("src/数学.zh"), "公开 函数 a() {}\n").unwrap();
        std::fs::create_dir_all(根路径.join(".rz/gen/src")).unwrap();
        std::fs::write(根路径.join(".rz/gen/src/数学.rs"), "pub fn a() {}\n").unwrap();
        std::fs::write(根路径.join(".rz/gen/src/数学.rs.bak"), "旧版本\n").unwrap();
        let 已删除 = 清理失效产物(根路径, &["zh".to_string()]);
        assert_eq!(已删除, vec![根路径.join(".rz/gen/src/数学.rs.bak")]);
        assert!(根路径.join(".rz/gen/src/数学.rs").is_file());
    }

    /// 写入语义分流：只有本项目托管树下的文件算「机器产物」
    #[test]
    fn 托管产物判定不越项目边界() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根路径 = 临时.path();
        std::fs::create_dir_all(根路径.join(".rz/gen/src")).unwrap();
        std::fs::write(根路径.join(".rz/gen/src/数学.rs"), "pub fn a() {}\n").unwrap();
        std::fs::create_dir_all(根路径.join("src")).unwrap();
        std::fs::write(根路径.join("src/main.rs"), "fn main() {}\n").unwrap();
        assert!(是否为托管产物(
            根路径,
            &根路径.join(".rz/gen/src/数学.rs")
        ));
        // 首次写入（文件还不存在）同样按字面路径归入托管树
        assert!(是否为托管产物(
            根路径,
            &根路径.join(".rz/gen/src/新文件.rs")
        ));
        assert!(!是否为托管产物(根路径, &根路径.join("src/main.rs")));
        // 同名目录属于另一个项目时不算（否则备份护栏被绕过，真风险）
        let 另项目 = Path::new("/other/.rz/gen/src/数学.rs");
        assert!(!是否为托管产物(根路径, 另项目));
    }
}
