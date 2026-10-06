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

use crate::响应映射::诊断文本::{
    提取所有权详情, 注入教学诊断, 翻译诊断消息
};
use crate::翻译缓存::{路径转定位, 转译缓存};

/// 镜像树复制时排除的目录名（构建产物/版本库/依赖缓存）
const 排除目录名: [&str; 4] = ["target", ".git", "node_modules", "dist"];

/// 镜像检查轮询超时：600s（含真实依赖编译，首轮可能数分钟）
const 检查超时毫秒: u64 = 600_000;
/// 轮询间隔
const 轮询间隔毫秒: u64 = 200;

/// 单个镜像转译文件：方言源 ↔ 镜像产物坐标映射
struct 镜像文件 {
    /// 镜像中的产物路径（src/main.rs 或同名 .rs；诊断归位用）
    产物路径: PathBuf,
    /// 真实项目中的方言源 URI（发布诊断用）
    原始资源定位: String,
    /// 方言源内容（打开文档取缓冲区，否则取磁盘；同行列换算用）
    方言内容: String,
    /// 引擎列映射（产物字符列 → 方言字符列；本模块自行转译，与 CLI 同源）
    列映射: i18n_rust_engine::列映射::列映射表,
    /// 入口产物行（磁盘行，0-based 索引）→ 引擎直出行映射；
    /// 仅入口文件有 `#[path]` 注解插入行，其余为恒等（None）
    入口行映射: Option<Vec<usize>>,
}

/// 待写盘的镜像转译产物（入口识别需全部产物就绪，故先在内存中转译）
struct 待写产物 {
    /// 镜像中的方言源路径（产物路径由它推导）
    镜像路径: PathBuf,
    /// 引擎转译输出（未补 `#[path]` 注解）
    产出: String,
    /// 引擎列映射（产物字符列 → 方言字符列）
    列映射: i18n_rust_engine::列映射::列映射表,
    /// 方言源内容（打开文档取缓冲区，否则取磁盘）
    方言内容: String,
}

/// 执行一次镜像检查并发布权威诊断
///
/// 返回 true 表示镜像检查确实执行（无论项目有无诊断）；
/// false 表示镜像不可用，调用方回退虚拟项目检查（原有行为）。
pub(crate) fn 执行镜像检查(
    缓存: &Arc<转译缓存>,
    发送端: &crossbeam_channel::Sender<lsp_server::Message>,
    内置诊断: &Arc<Mutex<HashMap<String, Vec<Value>>>>,
    扩展名列表: &[String],
    触发资源定位: &str,
    已知词集: &HashSet<String>,
) -> bool {
    // 触发文件须在缓存中（didOpen/didSave 均已入库），据此定位项目根
    let Some(触发路径) = 缓存
        .查询原文(触发资源定位)
        .map(|条目| 条目.原始路径.clone())
    else {
        return false;
    };
    let Some(项目根) = 定位项目根(&触发路径) else {
        return false; // 非 cargo 项目（单文件教学场景）
    };
    if !触发路径.starts_with(&项目根) {
        return false;
    }

    // 镜像目录：系统临时目录顶层、按 用户+PID+项目哈希 隔离；每次全量重建
    // （删除/改名等磁盘变化自动反映，无需增量跟踪）
    let 镜像目录 = 镜像目录路径(&项目根);
    let _ = std::fs::remove_dir_all(&镜像目录);
    if let Err(错误值) = 复制项目树(&项目根, &镜像目录) {
        log::warn!(
            "镜像复制失败（回退虚拟检查）：{}：{错误值}",
            项目根.display()
        );
        let _ = std::fs::remove_dir_all(&镜像目录);
        return false;
    }

    // 方言源转译：入口检测 → 写产物 → 返回产物坐标映射
    let Some(文件集) = 转译入镜像(缓存, 扩展名列表, &项目根, &镜像目录)
    else {
        let _ = std::fs::remove_dir_all(&镜像目录);
        return false;
    };

    // cargo check：--offline 防索引网络访问卡死；--target-dir 复用真实项目
    // 的依赖编译产物（镜像仅重编本地 crate）
    let 目标目录 = 项目根.join("target");
    let Some((标准输出, 成功标志)) = 运行cargo检查(&镜像目录, &目标目录) else {
        return false;
    };

    let (已见, 已发布) = 发布rustc诊断(
        &标准输出,
        &镜像目录,
        &文件集,
        缓存,
        已知词集,
        内置诊断,
        发送端,
    );
    if !成功标志 && 已见 == 0 {
        // 无任何可归位诊断却异常退出：cargo 层失败（清单/离线缺依赖等），
        // 镜像结论不可信，回退虚拟检查
        log::warn!("镜像检查异常退出且无诊断，回退虚拟检查");
        return false;
    }
    log::info!(
        "镜像检查完成：{} 个方言文件，{} 条编译消息，发布 {} 个文件",
        文件集.len(),
        已见,
        已发布
    );
    true
}

/// 定位项目根：触发文件最近的上层 Cargo.toml 所在目录
fn 定位项目根(起点: &Path) -> Option<PathBuf> {
    let mut 目录 = 起点.parent()?;
    loop {
        if 目录.join("Cargo.toml").is_file() {
            return Some(目录.to_path_buf());
        }
        目录 = 目录.parent()?;
    }
}

/// 项目根路径哈希（镜像目录名；同项目复用同一目录，跨会话互不串扰）
fn 路径哈希(路径: &Path) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut 哈希器项 = DefaultHasher::new();
    路径.hash(&mut 哈希器项);
    哈希器项.finish()
}

/// 镜像目录：系统临时目录下、用户名+PID+项目哈希隔离的独立顶层目录
///
/// 不能放在虚拟项目目录之下：虚拟项目本身是 cargo 包（含 Cargo.toml），
/// 嵌套其中的镜像包会被 cargo 判定为“在 workspace 中却未被声明”而拒绝
/// 检查（父目录 Cargo.toml 构成 workspace 根）；顶层独立目录同时避免
/// rust-analyzer 把镜像树当作工作区文件扫描。
fn 镜像目录路径(项目根: &Path) -> PathBuf {
    let 用户 = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "default".to_string());
    let 安全用户: String = 用户
        .chars()
        .map(|字符项| {
            if 字符项.is_alphanumeric() || 字符项 == '_' {
                字符项
            } else {
                '_'
            }
        })
        .collect();
    std::env::temp_dir().join(format!(
        "i18n_lsp_mirror_{}_{}_{:x}",
        安全用户,
        std::process::id(),
        路径哈希(项目根)
    ))
}

/// 递归复制项目树（排除构建产物/版本库；目录符号链接跳过防逃逸与循环，
/// 文件符号链接跟随复制单文件）
fn 复制项目树(来自: &Path, 到: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(到)?;
    for 目录项 in std::fs::read_dir(来自)?.flatten() {
        let 路径 = 目录项.path();
        let Ok(文件类型项) = 目录项.file_type() else {
            continue;
        };
        let Ok(元数据) = 目录项.metadata() else {
            continue;
        };
        let 名字 = 目录项.file_name();
        let 名字 = 名字.to_string_lossy();
        if 元数据.is_dir() {
            if 文件类型项.is_symlink() || 排除目录名.contains(&名字.as_ref()) {
                continue;
            }
            复制项目树(&路径, &到.join(名字.as_ref()))?;
        } else if 元数据.is_file() {
            std::fs::copy(&路径, 到.join(名字.as_ref()))?;
        }
    }
    Ok(())
}

/// 收集目录下全部方言源文件（递归；路径排序保证确定性）
fn 收集方言路径(目录: &Path, 扩展名列表: &[String], 输出: &mut Vec<PathBuf>) {
    let Ok(条目) = std::fs::read_dir(目录) else {
        return;
    };
    for 目录项 in 条目.flatten() {
        let 路径 = 目录项.path();
        let Ok(文件类型项) = 目录项.file_type() else {
            continue;
        };
        if 文件类型项.is_dir() {
            收集方言路径(&路径, 扩展名列表, 输出);
        } else if 文件类型项.is_file() && 匹配方言扩展名(&路径, 扩展名列表) {
            输出.push(路径);
        }
    }
}

/// 路径扩展名是否属于支持的方言扩展名（如 `.zh`）
fn 匹配方言扩展名(路径: &Path, 扩展名列表: &[String]) -> bool {
    let Some(后缀) = 路径.extension().and_then(|后缀项| 后缀项.to_str()) else {
        return false;
    };
    let 带点 = format!(".{后缀}");
    扩展名列表
        .iter()
        .any(|后缀项| 后缀项 == &带点 || 后缀项 == 后缀)
}

/// 把镜像 src/ 下的方言源转译为 .rs 产物（入口 → main.rs），返回产物映射表
///
/// 转译与 rzc 同规则：模块名集合 = src/ 顶层方言文件词干（`crate::` 前缀
/// 限定用）、项目声明上下文来自全部方言源（跨文件声明豁免）、入口产物补
/// 非 ASCII 模块 `#[path]` 注解。打开中的文档以缓冲区内容为准。
fn 转译入镜像(
    缓存: &Arc<转译缓存>,
    扩展名列表: &[String],
    项目根: &Path,
    镜像目录: &Path,
) -> Option<Vec<镜像文件>> {
    let 镜像src = 镜像目录.join("src");
    if !镜像src.is_dir() {
        return None;
    }
    // 方言源清单：src/ 递归（覆盖 rzc 的顶层扫描；嵌套文件经 #[path] 引用）
    let mut 方言路径: Vec<PathBuf> = Vec::new();
    收集方言路径(&镜像src, 扩展名列表, &mut 方言路径);
    方言路径.sort();
    if 方言路径.is_empty() {
        return None;
    }

    // 模块名集合：src/ 全层级方言文件词干（与 CLI collect_project_context
    // 同规则）；嵌套模块（src/领域/工具.zh 的「工具」）也纳入，供
    // `crate::领域::工具::成员` 多层路径链豁免
    let mut 模块名集: HashSet<String> = HashSet::new();
    // 方言源内容：打开文档取缓冲区，其余取磁盘
    let mut 源内容: HashMap<PathBuf, String> = HashMap::new();
    for 镜像路径 in &方言路径 {
        let Ok(相对) = 镜像路径.strip_prefix(镜像目录) else {
            continue;
        };
        // 打开文档取缓冲区内容：按路径查询（容忍客户端 URI 与规范化
        // 路径的表示差异——Windows 上字符串键匹配会失效）
        let 内容 = 缓存
            .按路径查询(&项目根.join(相对))
            .filter(|条目| 条目.是否打开)
            .map(|条目| 条目.中文原文.clone())
            .or_else(|| std::fs::read_to_string(镜像路径).ok());
        let Some(内容) = 内容 else { continue };
        if let Some(词干) = 镜像路径.file_stem().and_then(|扩展名项| 扩展名项.to_str())
        {
            模块名集.insert(词干.to_string());
        }
        源内容.insert(镜像路径.clone(), 内容);
    }
    if 源内容.is_empty() {
        return None;
    }

    let 项目 = i18n_rust_engine::别名替换::项目上下文::自源文件新建(
        模块名集.clone(),
        源内容.values().map(String::as_str),
        缓存.词表管理器(),
    );

    // 逐文件转译（内存中）：入口识别需要全部产物先就绪
    let mut 待写: Vec<待写产物> = Vec::new();
    for 镜像路径 in &方言路径 {
        let Some(内容) = 源内容.get(镜像路径) else {
            continue;
        };
        let 产出 = i18n_rust_engine::转译管线并映射并项目(
            内容,
            缓存.词表管理器(),
            Some(&模块名集),
            Some(&项目),
        );
        let 列映射 = i18n_rust_engine::列映射::列映射表::r#构建(内容, &产出.管线映射);
        待写.push(待写产物 {
            镜像路径: 镜像路径.clone(),
            产出: 产出.产出,
            列映射,
            方言内容: 内容.clone(),
        });
    }

    // 入口识别：词干 main 优先（其天然产物即 src/main.rs）；否则取与
    // 现有 src/main.rs 相似度最高的方言源（rzc 允许任意入口文件名，
    // 产物统一写入 main.rs；相似度不足时保留原 main.rs 不动）
    let 现有主产物 = std::fs::read_to_string(镜像src.join("main.rs")).ok();
    let 入口路径 = 识别入口(&待写, 现有主产物.as_deref());

    // 写产物 + 构造映射表
    let mut 文件集: Vec<镜像文件> = Vec::new();
    for 路径项 in 待写 {
        let Ok(相对) = 路径项.镜像路径.strip_prefix(镜像目录) else {
            continue;
        };
        let 源路径 = 项目根.join(相对);
        // 诊断发布 URI 复用缓存条目的客户端原样 URI：与编辑器打开的
        // 文档严格一致（Windows 上规范化形式与客户端形式不同，直接用
        // 反推 URI 会导致诊断发布到用户不可见的文档上）；未登记条目
        // （缓存无路径匹配）回退规范化形式
        let 原始资源定位 = 缓存
            .按路径查询(&源路径)
            .map(|条目| 条目.原始资源定位.clone())
            .unwrap_or_else(|| 路径转定位(&源路径));
        let 是入口 = 入口路径.as_deref() == Some(路径项.镜像路径.as_path());
        let (内容, 产物路径, 入口行映射) = if 是入口 {
            // 入口：补 #[path] 注解（rustc 拒绝非 ASCII 模块名的文件式
            // 声明，E0754），产物固定写入 src/main.rs
            let (带注解, 行映射) =
                i18n_rust_engine::模块路径::标注非西文模块并行号(&路径项.产出);
            (带注解, 镜像src.join("main.rs"), Some(行映射))
        } else if let Some(词干) = 路径项
            .镜像路径
            .file_stem()
            .and_then(|扩展名项| 扩展名项.to_str())
        {
            // 非入口模块（含多层）：按自身词干目录补 #[path]，
            // 与 rzc transpile_project_files 同规则（src/领域.rs 的
            // `mod 工具;` → #[path = "领域/工具.rs"]）；行映射同样
            // 供诊断行号回译
            let (带注解, 行映射) =
                i18n_rust_engine::模块路径::标注嵌套模块并行号(&路径项.产出, 词干);
            (带注解, 路径项.镜像路径.with_extension("rs"), Some(行映射))
        } else {
            (
                路径项.产出.clone(),
                路径项.镜像路径.with_extension("rs"),
                None,
            )
        };
        if let Err(错误值) = std::fs::write(&产物路径, &内容) {
            log::warn!("镜像产物写入失败：{}：{错误值}", 产物路径.display());
            continue;
        }
        文件集.push(镜像文件 {
            产物路径,
            原始资源定位,
            方言内容: 路径项.方言内容,
            列映射: 路径项.列映射,
            入口行映射,
        });
    }
    if 文件集.is_empty() {
        None
    } else {
        Some(文件集)
    }
}

/// 入口识别：返回镜像中作为 main.rs 来源的方言源路径
///
/// 词干为 main 的方言文件优先（rzc 语义：其产物即入口产物）；
/// 否则在所有方言产物（含 `#[path]` 注解）中找与现有 src/main.rs
/// 相似度最高者，相似度须达到阈值（初次构建即精确匹配，入口编辑后
/// 仍高度相似；其他模块与入口的相似度接近零）。
fn 识别入口(待写: &[待写产物], 现有主产物: Option<&str>) -> Option<PathBuf> {
    for 路径项 in 待写 {
        if 路径项
            .镜像路径
            .file_stem()
            .and_then(|扩展名项| 扩展名项.to_str())
            == Some("main")
        {
            return Some(路径项.镜像路径.clone());
        }
    }
    let 现有主产物 = 现有主产物?;
    let mut 最佳: Option<(PathBuf, f64)> = None;
    for 路径项 in 待写 {
        let (带注解, _) =
            i18n_rust_engine::模块路径::标注非西文模块并行号(&路径项.产出);
        let 分数 = 逐行相似度(&带注解, 现有主产物);
        if 分数 >= 0.6 && 最佳.as_ref().is_none_or(|(_, 已选)| 分数 > *已选) {
            最佳 = Some((路径项.镜像路径.clone(), 分数));
        }
    }
    最佳.map(|(路径, _)| 路径)
}

/// 逐行相似度：实质行（≥8 字符）命中率（命中数 / 两侧较大行数）
///
/// 分母取 max(候选行数, 参照行数)，小文件的局部巧合命中无法抬高分数。
fn 逐行相似度(候选: &str, 参照: &str) -> f64 {
    let 实质判定 = |line: &str| line.trim().chars().count() >= 8;
    let 参照行: HashSet<&str> = 参照
        .lines()
        .map(str::trim)
        .filter(|line| 实质判定(line))
        .collect();
    if 参照行.is_empty() {
        return 0.0;
    }
    let mut 总数 = 0usize;
    let mut 命中 = 0usize;
    for line in 候选.lines().map(str::trim).filter(|line| 实质判定(line)) {
        总数 += 1;
        if 参照行.contains(line) {
            命中 += 1;
        }
    }
    if 总数 == 0 {
        return 0.0;
    }
    命中 as f64 / 总数.max(参照行.len()) as f64
}

/// 启动 cargo check 并等待退出（持续排空管道防写满阻塞，超时强杀）
///
/// 返回 (stdout, 退出码是否成功)；进程无法启动/超时被强杀时返回 None。
fn 运行cargo检查(镜像目录: &Path, 目标目录: &Path) -> Option<(String, bool)> {
    let mut 子进程 = match std::process::Command::new("cargo")
        .args(["check", "--offline", "--message-format=json"])
        .arg("--target-dir")
        .arg(目标目录)
        .current_dir(镜像目录)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(子进程项) => 子进程项,
        Err(错误值) => {
            log::warn!("镜像 cargo check 启动失败：{错误值}");
            return None;
        }
    };
    let mut 标准输出管道 = 子进程.stdout.take()?;
    let mut 标准错误管道 = 子进程.stderr.take()?;
    let 标准输出句柄 = std::thread::spawn(move || {
        let mut 缓冲 = Vec::new();
        let _ = 标准输出管道.read_to_end(&mut 缓冲);
        缓冲
    });
    let 标准错误句柄 = std::thread::spawn(move || {
        let mut 缓冲 = Vec::new();
        let _ = 标准错误管道.read_to_end(&mut 缓冲);
        缓冲
    });

    let mut 状态 = None;
    for _ in 0..检查超时毫秒 / 轮询间隔毫秒 {
        match 子进程.try_wait() {
            Ok(Some(等待结果项)) => {
                状态 = Some(等待结果项);
                break;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(轮询间隔毫秒)),
            Err(_) => break,
        }
    }
    let 状态 = match 状态 {
        Some(等待结果项) => 等待结果项,
        None => {
            log::warn!("镜像 cargo check 超时/异常，已终止（回退虚拟检查）");
            let _ = 子进程.kill();
            let _ = 子进程.wait();
            return None;
        }
    };
    let 标准输出 = 标准输出句柄.join().unwrap_or_default();
    let 标准错误 = 标准错误句柄.join().unwrap_or_default();
    if !状态.success() {
        // 编译错误属正常情况（诊断在 stdout）时同样以非零码退出；stderr
        // 末尾才是 cargo 层的真正错误（清单/锁/依赖解析），首行往往只是
        // "Checking ..." 进度行，取末尾若干行输出便于排查
        let 标准错误文本 = String::from_utf8_lossy(&标准错误);
        if 标准错误文本.contains("failed to") || 标准错误文本.contains("error: could not")
        {
            let 尾部: Vec<&str> = 标准错误文本.lines().rev().take(6).collect();
            let 尾部: Vec<&str> = 尾部.into_iter().rev().collect();
            log::warn!(
                "镜像 cargo check 非零退出（{:?}）：{}",
                状态.code(),
                尾部.join(" / ")
            );
        }
    }
    Some((
        String::from_utf8_lossy(&标准输出).to_string(),
        状态.success(),
    ))
}

/// 解析 rustc JSON 诊断流：坐标回译 → 消息翻译 → 所有权提取 → 合并发布
///
/// 返回 (可归位的编译消息数, 发布文件数)。
fn 发布rustc诊断(
    标准输出: &str,
    镜像目录: &Path,
    文件集: &[镜像文件],
    缓存: &Arc<转译缓存>,
    已知词集: &HashSet<String>,
    内置诊断: &Arc<Mutex<HashMap<String, Vec<Value>>>>,
    发送端: &crossbeam_channel::Sender<lsp_server::Message>,
) -> (usize, usize) {
    // 产物路径 → 方言源映射：canonical 键归一 rustc 的绝对/相对路径
    let 索引: HashMap<PathBuf, &镜像文件> = 文件集
        .iter()
        .filter_map(|文件项| {
            std::fs::canonicalize(&文件项.产物路径)
                .ok()
                .map(|路径项| (路径项, 文件项))
        })
        .collect();

    let mut 按资源定位: HashMap<String, Vec<Value>> = HashMap::new();
    let mut 已见 = 0usize;
    for line in 标准输出.lines() {
        let Ok(值) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if 值["reason"].as_str() != Some("compiler-message") {
            continue;
        }
        let 消息 = &值["message"];
        let Some(跨度列表) = 消息["spans"].as_array() else {
            continue;
        };
        // 主 span 优先（rustc 的 spans 可能把次级位置排在前面）
        let Some(跨度) = 跨度列表
            .iter()
            .find(|跨度项| 跨度项["is_primary"] == Value::Bool(true))
            .or_else(|| 跨度列表.first())
        else {
            continue;
        };
        let Some(file) = 解析跨度归属文件(跨度, 镜像目录, &索引) else {
            continue; // 非方言文件（手写 .rs、标准库等）：不发布
        };
        已见 += 1;

        let 原始消息 = 消息["message"].as_str().unwrap_or("");
        let 错误码 = 消息["code"]["code"].as_str().unwrap_or("").to_string();
        let 严重度 = match 消息["level"].as_str() {
            Some("error") => 1,
            Some("warning") => 2,
            _ => 3,
        };
        let Some((起始行, 起始列, 结束行, 结束列)) = 回译跨度范围(file, 跨度)
        else {
            continue;
        };
        let 范围 = json!({
            "start": { "line": 起始行, "character": 起始列 },
            "end": { "line": 结束行, "character": 结束列 }
        });

        // 次级 span → relatedInformation（所有权可视化等）；消息先保留英文，
        // 供 提取所有权详情 关键词判定，发布前再翻译
        let mut 原始关联: Vec<Value> = Vec::new();
        for 跨度项 in 跨度列表 {
            if 跨度项["is_primary"] == Value::Bool(true) {
                continue;
            }
            let Some(标签) = 跨度项["label"].as_str().filter(|标签项| !标签项.is_empty())
            else {
                continue;
            };
            let Some(关联文件) = 解析跨度归属文件(跨度项, 镜像目录, &索引)
            else {
                continue;
            };
            let Some((起点行, 起点列, 终点行, 终点列)) = 回译跨度范围(关联文件, 跨度项)
            else {
                continue;
            };
            原始关联.push(json!({
                "location": {
                    "uri": 关联文件.原始资源定位,
                    "range": {
                        "start": { "line": 起点行, "character": 起点列 },
                        "end": { "line": 终点行, "character": 终点列 }
                    }
                },
                "message": 标签
            }));
        }

        let 原始值 = json!({ "message": 原始消息, "code": 错误码 });
        let 还原值 = json!({ "range": 范围, "relatedInformation": 原始关联 });
        let 所有权 = 提取所有权详情(&原始值, &还原值, &file.原始资源定位);

        // 主消息与 CLI 同口径：传错误码 + 主 span 标签（rustc JSON 携带
        // “expected X, found Y”），使类型不匹配等能回填期望/实际并中文化类型名
        let 主消息 = 翻译诊断消息(
            if 错误码.is_empty() {
                None
            } else {
                Some(&错误码)
            },
            原始消息,
            跨度["label"].as_str(),
        );
        let mut 诊断 = json!({
            "range": 范围,
            "severity": 严重度,
            "code": 错误码,
            "message": 主消息,
        });
        if !原始关联.is_empty() {
            let 译文列表: Vec<Value> = 原始关联
                .iter()
                .map(|关联项| {
                    let mut 项 = 关联项.clone();
                    项["message"] = Value::String(翻译诊断消息(
                        None,
                        关联项["message"].as_str().unwrap_or(""),
                        None,
                    ));
                    项
                })
                .collect();
            诊断["relatedInformation"] = Value::Array(译文列表);
        }
        if let Some(详情) = 所有权
            && let Ok(详情值) = serde_json::to_value(&详情)
        {
            诊断["data"] = 详情值;
        }
        按资源定位
            .entry(file.原始资源定位.clone())
            .or_default()
            .push(诊断);
    }

    // 合并内置诊断（RA 最近一次映射后的方言坐标诊断 + 教学提示）：
    // 同 code 且同起始行视为重复（与虚拟检查的合并规则一致），
    // 避免镜像结果覆盖语法/类型实时诊断
    let mut 已发布 = 0usize;
    for (资源定位, mut 诊断列表) in 按资源定位 {
        if let Ok(守卫) = 内置诊断.lock()
            && let Some(内置项) = 守卫.get(&资源定位)
        {
            for 条目 in 内置项.clone() {
                let 重复 = 诊断列表.iter().any(|诊断项| {
                    诊断项["code"] == 条目["code"]
                        && 诊断项["range"]["start"]["line"] == 条目["range"]["start"]["line"]
                });
                if !重复 {
                    诊断列表.push(条目);
                }
            }
        }
        // 教学诊断注入（全角标点 + 教学 lint）：由 entry 内容直接计算，
        // 不依赖 builtin 缓存（镜像检查先于 RA 首批发布时缓存为空）
        if let Some(条目) = 缓存.查询原文(&资源定位) {
            注入教学诊断(&mut 诊断列表, &条目, 已知词集, &缓存.歧义构造词集());
        }
        let 通知 = lsp_server::Notification {
            method: "textDocument/publishDiagnostics".to_string(),
            params: json!({ "uri": 资源定位, "diagnostics": 诊断列表 }),
        };
        let _ = 发送端.send(lsp_server::Message::Notification(通知));
        已发布 += 1;
    }
    (已见, 已发布)
}

/// 解析 span 归属的方言文件（镜像产物路径 → 镜像文件）
fn 解析跨度归属文件<'a>(
    跨度: &Value,
    镜像目录: &Path,
    索引: &HashMap<PathBuf, &'a 镜像文件>,
) -> Option<&'a 镜像文件> {
    let 名字 = 跨度["file_name"].as_str()?;
    let 路径 = Path::new(名字);
    // rustc 输出相对路径（cwd 为镜像项目根）或绝对路径，统一绝对化
    let 绝对 = if 路径.is_absolute() {
        路径.to_path_buf()
    } else {
        镜像目录.join(路径)
    };
    let 规范 = std::fs::canonicalize(绝对).ok()?;
    索引.get(&规范).copied()
}

/// span 的起止位置回译：方言 0-based 行 + UTF-16 列（LSP 协议）
fn 回译跨度范围(file: &镜像文件, 跨度: &Value) -> Option<(u32, u32, u32, u32)> {
    let line = 跨度["line_start"].as_u64()? as u32;
    let column = 跨度["column_start"].as_u64()? as u32;
    let 末行 = 跨度["line_end"].as_u64().unwrap_or(line as u64) as u32;
    let 末列 = 跨度["column_end"].as_u64().unwrap_or(column as u64) as u32;
    let (起始行, 起始列) = 回译单点位置(file, line, column);
    let (结束行, 结束列) = 回译单点位置(file, 末行, 末列);
    Some((起始行, 起始列, 结束行, 结束列))
}

/// 单点回译：产物 1-based (行, 字符列) → 方言 0-based (行, UTF-16 列)
///
/// 行号：入口产物含 `#[path]` 注解插入行，先经入口行映射换算回引擎直出
/// 行号（列映射以引擎直出产物为基准；注解行归属其 mod 声明行）。
/// 列号：引擎列映射输出方言字符列（与 rustc 列口径一致），再按方言文本
/// 换算为 LSP 的 UTF-16 列。
fn 回译单点位置(file: &镜像文件, 行1基: u32, 列1基: u32) -> (u32, u32) {
    let 引擎行 = match &file.入口行映射 {
        Some(映射表) => 映射表
            .get(行1基.saturating_sub(1) as usize)
            .map(|行号项| *行号项 as u32 + 1)
            .unwrap_or(行1基),
        None => 行1基,
    };
    let (方言行, 方言字符列) = file.列映射.映射位置(引擎行, 列1基);
    (
        方言行.saturating_sub(1),
        方言字符列转utf16列(&file.方言内容, 方言行, 方言字符列),
    )
}

/// 方言行内字符列（1-based）→ UTF-16 列（0-based）
///
/// 引擎列映射的列口径为字符数（与 rustc JSON 一致）；LSP 协议要求
/// UTF-16 代码单元列，非 BMP 字符（emoji 等）占 2 个单元，须逐字符累加。
fn 方言字符列转utf16列(方言内容: &str, 行1基: u32, 字符列1基: u32) -> u32 {
    let Some(行文本) = 方言内容.lines().nth(行1基.saturating_sub(1) as usize) else {
        return 字符列1基.saturating_sub(1);
    };
    行文本
        .chars()
        .take(字符列1基.saturating_sub(1) as usize)
        .map(|字符项| 字符项.len_utf16() as u32)
        .sum()
}

#[cfg(test)]
mod 单元测试 {
    use super::*;

    fn 构造待写产物(名字: &str, 产出: &str) -> 待写产物 {
        待写产物 {
            镜像路径: PathBuf::from("/mirror/src").join(名字),
            产出: 产出.to_string(),
            列映射: i18n_rust_engine::列映射::列映射表::r#构建("", &[]),
            方言内容: String::new(),
        }
    }

    /// 小于 8 字符的短行（`}`、空行等）不参与相似度，避免结构行虚高命中
    #[test]
    fn 测试相似度忽略短行() {
        assert_eq!(逐行相似度("}\n{\n", "}\n{\n"), 0.0);
        assert_eq!(逐行相似度("", ""), 0.0);
        // 参照侧全是短行 → 实质行为空 → 0.0
        assert_eq!(逐行相似度("fn main() {}", "}\n)\n"), 0.0);
    }

    /// 完全相同为 1.0；无交集为 0.0
    #[test]
    fn 测试相似度边界() {
        let str = "fn main() {\n    println!(\"hi\");\n}\n";
        assert_eq!(逐行相似度(str, str), 1.0);
        assert_eq!(逐行相似度(str, "fn other() {\n    let 甲 = 1;\n}\n"), 0.0);
    }

    /// 分母取两侧较大者：小文件与参照的局部巧合命中无法抬高分数
    ///
    /// 候选仅 1 条实质行且命中，参照有 3 条实质行 → 1/3 而非 1.0。
    #[test]
    fn 测试相似度分母取较大侧() {
        let 参照 = "fn 甲() {\nfn 乙() {\nfn 丙() {\n";
        let 分数 = 逐行相似度("fn 甲() {\n", 参照);
        assert!(
            (分数 - 1.0 / 3.0).abs() < 1e-9,
            "分母应为 3（较大侧），实际 {分数}"
        );
    }

    /// 词干为 main 的方言文件优先作为入口（不看相似度）
    #[test]
    fn 测试入口优先选main词干() {
        let 候选 = vec![
            构造待写产物("甲.zh", "fn 甲() {\n    let 甲值 = 1;\n}\n"),
            构造待写产物("main.zh", "fn 主函数() {\n}\n"),
            构造待写产物("乙.zh", "fn 乙() {\n    let 乙值 = 2;\n}\n"),
        ];
        assert_eq!(
            识别入口(&候选, None).map(|路径项| 路径项.file_name().unwrap().to_owned()),
            Some("main.zh".into()),
            "词干为 main 时应无视相似度直接选中"
        );
    }

    /// 无 main 时按与现有 main.rs 的相似度选取（须达阈值 0.6）
    #[test]
    fn 测试入口按相似度达阈值() {
        let 入口文本 = "fn 主函数() {\n    println!(\"你好\");\n}\n";
        let 候选 = vec![
            构造待写产物("甲.zh", "fn 甲() {\n    let 甲值 = 1;\n}\n"),
            构造待写产物("入口.zh", 入口文本),
        ];
        assert_eq!(
            识别入口(&候选, Some(入口文本)).map(|路径项| 路径项.file_name().unwrap().to_owned()),
            Some("入口.zh".into()),
            "相似度达阈值者应被选为入口"
        );
        // 全部候选都与参照不相似 → None（低于阈值宁可不识别）
        assert!(
            识别入口(
                &[构造待写产物(
                    "甲.zh",
                    "fn 甲() {\n    let 甲值 = 1;\n}\n"
                )],
                Some("fn 乙() {\n    let 乙值 = 2;\n}\n")
            )
            .is_none(),
            "相似度不足阈值时不应误判入口"
        );
    }

    /// 无 main 词干且无参照 main.rs 时无法识别入口
    #[test]
    fn 测试无参照时识别不到入口() {
        let 候选 = vec![构造待写产物(
            "甲.zh",
            "fn 甲() {\n    let 甲值 = 1;\n}\n",
        )];
        assert!(识别入口(&候选, None).is_none());
        assert!(识别入口(&[], Some("fn 主函数() {\n}\n")).is_none());
    }

    /// 方言扩展名匹配：大小写与后缀精确性
    #[test]
    fn 测试方言扩展名匹配() {
        let 扩展名 = vec!["zh".to_string(), "ja".to_string()];
        assert!(匹配方言扩展名(Path::new("/p/main.zh"), &扩展名));
        assert!(匹配方言扩展名(Path::new("/p/模块.ja"), &扩展名));
        assert!(!匹配方言扩展名(Path::new("/p/main.rs"), &扩展名));
        // 仅匹配完整后缀：`xzh` 不是 `.zh`
        assert!(!匹配方言扩展名(Path::new("/p/main.zhx"), &扩展名));
        // 无扩展名
        assert!(!匹配方言扩展名(Path::new("/p/main"), &扩展名));
    }

    /// 项目根哈希：同路径稳定、不同路径不同（镜像目录隔离的依据）
    #[test]
    fn 测试路径哈希稳定且区分() {
        let 甲 = 路径哈希(Path::new("/a/b"));
        assert_eq!(甲, 路径哈希(Path::new("/a/b")), "同路径应稳定");
        assert_ne!(甲, 路径哈希(Path::new("/a/c")), "不同路径应不同");
    }

    /// 字符列 → UTF-16 列：非 BMP 字符占 2 个单元，BMP 中文占 1
    #[test]
    fn 测试方言字符列转utf16统计单元() {
        // "字符😀甲"：字符列 1-based → 取前 n 个字符累加 len_utf16
        let str = "字符😀甲";
        assert_eq!(方言字符列转utf16列(str, 1, 1), 0);
        assert_eq!(方言字符列转utf16列(str, 1, 2), 1);
        // 含 emoji：到第 4 列时已计入 emoji 的 2 个单元
        assert_eq!(方言字符列转utf16列(str, 1, 4), 4);
        // 越界行回退为 0-based 字符列
        assert_eq!(方言字符列转utf16列(str, 9, 3), 2);
    }

    /// 项目根定位：沿目录上行找最近的 Cargo.toml；全程没有则 None
    #[test]
    fn 测试项目根定位最近与缺失() {
        let 测试临时目录 = tempfile::tempdir().unwrap();
        let 项目根目录 = 测试临时目录.path().join("proj");
        let 深层目录 = 项目根目录.join("src/sub/deep");
        std::fs::create_dir_all(&深层目录).unwrap();
        std::fs::write(项目根目录.join("Cargo.toml"), "[package]\n").unwrap();
        let 触发文件 = 深层目录.join("main.zh");
        assert_eq!(定位项目根(&触发文件).as_deref(), Some(项目根目录.as_path()));
        // 内层另有 Cargo.toml 时取最近者
        std::fs::write(深层目录.join("Cargo.toml"), "[package]\n").unwrap();
        assert_eq!(定位项目根(&触发文件).as_deref(), Some(深层目录.as_path()));
        // 无 Cargo.toml 的目录树（单文件教学场景）→ None
        let 其他目录 = 测试临时目录.path().join("noproject/a");
        std::fs::create_dir_all(&其他目录).unwrap();
        assert!(定位项目根(&其他目录.join("main.zh")).is_none());
    }

    /// 镜像目录名：固定前缀 + 用户 + PID + 项目哈希；用户名非法字符清洗为下划线
    #[test]
    fn 测试镜像目录名与用户名清洗() {
        let 项目根目录 = Path::new("/tmp/proj-alpha");
        let 镜像目录 = 镜像目录路径(项目根目录);
        let 名字 = 镜像目录.file_name().unwrap().to_string_lossy().into_owned();
        assert!(名字.starts_with("i18n_lsp_mirror_"), "实际：{名字}");
        assert!(
            名字.ends_with(&format!("_{:x}", 路径哈希(项目根目录))),
            "目录名应以项目哈希结尾：{名字}"
        );

        // 用户名含空格/斜杠等非法字符时清洗（保存/恢复 USER，避免污染其他测试）
        let 已存 = std::env::var("USER").ok();
        unsafe {
            std::env::set_var("USER", "a b/c");
        }
        let 镜像目录二 = 镜像目录路径(项目根目录);
        let 名字2 = 镜像目录二
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(
            名字2.contains("a_b_c"),
            "用户名非法字符应替换为下划线：{名字2}"
        );
        unsafe {
            match 已存 {
                Some(值项) => std::env::set_var("USER", 值项),
                None => std::env::remove_var("USER"),
            }
        }
    }

    /// 项目树复制：源文件递归复制；target/.git/node_modules/dist 一律排除
    #[test]
    fn 测试复制项目树排除目录() {
        let 测试临时目录 = tempfile::tempdir().unwrap();
        let 来自 = 测试临时目录.path().join("proj");
        std::fs::create_dir_all(来自.join("src/sub")).unwrap();
        std::fs::write(来自.join("Cargo.toml"), "[package]\n").unwrap();
        std::fs::write(来自.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        std::fs::write(来自.join("src/sub/util.zh"), "公开 函数 工具() {}\n").unwrap();
        for 排除项 in [
            "target/debug/x",
            ".git/config",
            "node_modules/lib/a",
            "dist/bundle",
        ] {
            let 路径项 = 来自.join(排除项);
            std::fs::create_dir_all(路径项.parent().unwrap()).unwrap();
            std::fs::write(路径项, "应被排除").unwrap();
        }
        let 到 = 测试临时目录.path().join("mirror");
        复制项目树(&来自, &到).unwrap();
        assert!(到.join("Cargo.toml").is_file());
        assert!(到.join("src/main.zh").is_file());
        assert!(到.join("src/sub/util.zh").is_file());
        assert!(!到.join("target").exists());
        assert!(!到.join(".git").exists());
        assert!(!到.join("node_modules").exists());
        assert!(!到.join("dist").exists());
    }

    /// 方言源收集：递归、仅匹配给定扩展名
    #[test]
    fn 测试收集方言路径递归() {
        let 测试临时目录 = tempfile::tempdir().unwrap();
        let 项目根目录 = 测试临时目录.path().join("proj/src");
        std::fs::create_dir_all(项目根目录.join("sub")).unwrap();
        std::fs::write(项目根目录.join("main.zh"), "").unwrap();
        std::fs::write(项目根目录.join("main.rs"), "").unwrap();
        std::fs::write(项目根目录.join("sub/util.zh"), "").unwrap();
        let mut 输出 = Vec::new();
        收集方言路径(&项目根目录, &[".zh".to_string()], &mut 输出);
        输出.sort();
        let 声明名: Vec<String> = 输出
            .iter()
            .map(|路径项| {
                路径项
                    .strip_prefix(&项目根目录)
                    .unwrap()
                    .display()
                    .to_string()
            })
            .collect();
        assert_eq!(声明名, vec!["main.zh", "sub/util.zh"]);
        // 目录不存在不报错（调用方依赖此容错）
        let mut 空列表 = Vec::new();
        收集方言路径(
            &测试临时目录.path().join("absent"),
            &[".zh".to_string()],
            &mut 空列表,
        );
        assert!(空列表.is_empty());
    }

    /// 构造一个带列映射的镜像文件（zh「让 x = 1;」→ en「let x = 1;」）
    fn 构造映射文件(入口行映射: Option<Vec<usize>>) -> 镜像文件 {
        let 方言源码 = "让 x = 1;\n";
        let 编辑列表 = [i18n_rust_engine::缓存::源映射条目::新建条目(
            0, 3, "让", "let",
        )];
        镜像文件 {
            产物路径: PathBuf::from("/mirror/src/main.rs"),
            原始资源定位: "file:///proj/src/main.zh".to_string(),
            方言内容: 方言源码.to_string(),
            列映射: i18n_rust_engine::列映射::列映射表::r#构建(方言源码, &编辑列表),
            入口行映射,
        }
    }

    /// span 回译：列经列映射换算，行转 0-based；无 #[path] 行映射时行号恒等
    #[test]
    fn 测试跨度回译列与行基础() {
        let file = 构造映射文件(None);
        // 产物 `let x = 1;` 第 1 行第 5 列（x 的首列，1-based）
        // → 方言「让 x = 1;」第 3 列；输出为 0-based 行 + UTF-16 列
        let 跨度 = serde_json::json!({"line_start": 1, "column_start": 5});
        assert_eq!(回译跨度范围(&file, &跨度), Some((0, 2, 0, 2)));
        // 缺 line_start → None（无法回译）
        let 坏值 = serde_json::json!({"column_start": 5});
        assert_eq!(回译跨度范围(&file, &坏值), None);
    }

    /// 入口产物含 #[path] 注解插入行：产物行先经入口行映射回引擎直出行号
    #[test]
    fn 测试单点回译入口插入行() {
        // 方言 3 行；产物在顶部插入 1 行注解：产物 1 行 ↔ 引擎 3 行（映射值 2 → 行 3）
        let 方言源码 = "函数 主函数() {\n    打印行!(\"hi\");\n}\n";
        let 编辑列表 = [
            i18n_rust_engine::缓存::源映射条目::新建条目(0, 6, "函数", "fn"),
            // 第 1 行 21 字节 + 4 个前导空格 → 「打印行」字节偏移 25
            i18n_rust_engine::缓存::源映射条目::新建条目(25, 9, "打印行", "println"),
        ];
        let file = 镜像文件 {
            产物路径: PathBuf::from("/mirror/src/main.rs"),
            原始资源定位: "file:///proj/src/main.zh".to_string(),
            方言内容: 方言源码.to_string(),
            列映射: i18n_rust_engine::列映射::列映射表::r#构建(方言源码, &编辑列表),
            入口行映射: Some(vec![2, 0, 1]),
        };
        // 产物第 1 行 → 引擎第 3 行（"}" 行）→ 方言 0-based 第 2 行
        let (起始行, _, 结束行, _) = 回译跨度范围(
            &file,
            &serde_json::json!({"line_start": 1, "column_start": 1}),
        )
        .unwrap();
        assert_eq!((起始行, 结束行), (2, 2));
        // 产物第 2 行 → 引擎第 1 行（映射值 0）
        let (起始行2, _, _, _) = 回译跨度范围(
            &file,
            &serde_json::json!({"line_start": 2, "column_start": 1}),
        )
        .unwrap();
        assert_eq!(起始行2, 0);
    }
}
