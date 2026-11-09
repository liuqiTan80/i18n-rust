//! 真实项目镜像编译检查（权威诊断）
//!
//! 虚拟项目只聚合方言模块、无第三方依赖，与真实工程结构偏差大；
//! 本模块把整个项目树复制为「镜像」（排除 target/.git/node_modules/dist），
//! 按 rzc 的规则把源码目录下的方言文件转译为 .rs 产物：
//! - 入口方言源（入口文件，或与现有入口产物的最相似方言文件）
//!   转译后写入镜像的入口产物，非 ASCII 文件式模块声明补 `#[path]` 注解；
//! - 其余方言文件转译为同名 .rs 产物（与 rzc 的项目文件转译规则一致）。
//!
//! 镜像目录位于系统临时目录顶层（`i18n_lsp_mirror_<用户>_<进程号>_<项目哈希>`），
//! 与虚拟项目目录同层隔离：虚拟项目本身是编译包，镜像嵌入其下会被
//! 包管理器判定为“在工作区中却未被声明”而拒绝检查（父目录清单文件
//! 构成工作区根）；顶层目录同时避免分析器把镜像树当作
//! 工作区文件扫描。残留目录由下次启动的虚拟目录清理机制一并清扫。
//!
//! 在镜像中执行 `cargo check --offline --message-format=json`，
//! `--target-dir` 指向真实项目的构建目录，依赖编译产物零重复（镜像
//! 本地编译单元的指纹含项目路径，与真实项目互不干扰）。
//! 编译器诊断经行/列回译（含 `#[path]` 注解行的行号换算）还原为方言坐标，
//! 消息翻译与所有权详情提取后按文件发布；镜像不可用（无清单文件、
//! 复制失败、包管理器无法启动）时返回假，由调用方回退虚拟项目检查。
//!
//! 打开中的方言文档以编辑器缓冲区内容为准（内存最新），其余取磁盘内容。
//! 与 rzc 构建的规则唯一来源为引擎（转译管线、`#[path]` 注解、列映射）。

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::响应映射::诊断文本::{
    提取所有权详情, 注入教学诊断, 翻译诊断消息
};
use crate::翻译缓存::{路径转统一资源标识, 转译缓存};

/// 镜像树复制时排除的目录名（构建产物/版本库/依赖缓存/项目级 cargo 配置）
///
/// `.cargo` 必须排除：其中的配置文件可指定编译器包装器与构建脚本
/// 等键，复制进镜像后 `cargo check` 会执行仓库自带的命令。
const 排除目录表: [&str; 6] = ["target", ".git", "node_modules", "dist", ".rz", ".cargo"];

/// 镜像树复制时排除的文件名（工具链选择与 cargo 包装器配置）
///
/// `rust-toolchain*` 会让 cargo 尝试切换/下载工具链；与 `.cargo` 同理，
/// 不受信仓库不应通过镜像检查影响本机工具链选择。
const 排除文件表: [&str; 2] = ["rust-toolchain.toml", "rust-toolchain"];

/// 镜像检查轮询超时：600s（含真实依赖编译，首轮可能数分钟）
const 检查超时毫秒: u64 = 600_000;
/// 轮询间隔
const 轮询间隔毫秒: u64 = 200;

/// 单个镜像转译文件：方言源 ↔ 镜像产物坐标映射
struct 镜像文件 {
    /// 镜像中的产物路径（src/main.rs 或同名 .rs；诊断归位用）
    产物路径: PathBuf,
    /// 真实项目中的方言源统一资源标识（发布诊断用）
    原始资源标识: String,
    /// 方言源内容（打开文档取缓冲区，否则取磁盘；同行列换算用）
    母语内容: String,
    /// 引擎列映射（产物字符列 → 方言字符列；本模块自行转译，与命令行同源）
    列位映射数据: i18n_rust_engine::列位映射::列位映射表,
    /// 入口产物行（磁盘行，从 0 开始的索引）→ 引擎直出行映射；
    /// 仅入口文件有 `#[path]` 注解插入行，其余为恒等（无）
    入口行映射: Option<Vec<usize>>,
}

/// 待写盘的镜像转译产物（入口识别需全部产物就绪，故先在内存中转译）
struct 待写产物 {
    /// 镜像中的方言源路径（产物路径由它推导）
    镜像路径: PathBuf,
    /// 引擎转译文本（未补 `#[path]` 注解）
    转译文本: String,
    /// 引擎列映射（产物字符列 → 方言字符列）
    列位映射数据: i18n_rust_engine::列位映射::列位映射表,
    /// 方言源内容（打开文档取缓冲区，否则取磁盘）
    母语内容: String,
}

/// 执行一次镜像检查并发布权威诊断
///
/// 返回真表示镜像检查确实执行（无论项目有无诊断）；
/// 假表示镜像不可用，调用方回退虚拟项目检查（原有行为）。
pub(crate) fn 运行镜像检查(
    缓存: &Arc<转译缓存>,
    消息发送方: &crossbeam_channel::Sender<lsp_server::Message>,
    内置诊断表: &Arc<Mutex<HashMap<String, Vec<Value>>>>,
    扩展名表: &[String],
    原始资源标识: &str,
    管理器: &i18n_rust_engine::映射管理器::映射管理器,
) -> bool {
    // 触发文件须在缓存中（打开/保存均已入库），据此定位项目根
    let Some(原始路径) = 缓存
        .查询原始(原始资源标识)
        .map(|条目值| 条目值.原始路径.clone())
    else {
        return false;
    };
    let Some(项目根) = 查找项目根(&原始路径) else {
        return false; // 非编译项目（单文件教学场景）
    };
    if !原始路径.starts_with(&项目根) {
        return false;
    }

    // 镜像目录：系统临时目录顶层、按 用户+进程号+项目哈希 隔离。
    // 采用增量同步而非每次全量重建：未改动文件按「大小 + 修改时间」跳过，
    // 只清理已不存在的条目；删除/改名等磁盘变化仍会在同步阶段反映。
    //
    // 安全：目录名可预测（用户名已知、进程号可枚举、路径哈希用确定性
    // 默认哈希器），多用户共享临时目录下可被预创建为符号链接。创建所有目录
    // 会跟随末段链接写到任意位置，故创建前后各查一次符号链接；任一命中即
    // 回退虚拟检查（复用返回假的降级契约），正常路径行为不变。
    let 镜像目录值 = 镜像目录路径(&项目根);
    if 是符号链接(&镜像目录值) {
        log::warn!(
            "镜像目录已被符号链接占用，跳过镜像检查（回退虚拟检查）：{}",
            镜像目录值.display()
        );
        return false;
    }
    if let Err(错误值) = std::fs::create_dir_all(&镜像目录值) {
        log::warn!(
            "镜像目录创建失败（回退虚拟检查）：{}：{错误值}",
            项目根.display()
        );
        return false;
    }
    if 是符号链接(&镜像目录值) {
        log::warn!(
            "镜像目录创建后检测到符号链接（并发竞态），回退虚拟检查：{}",
            镜像目录值.display()
        );
        return false;
    }
    if let Err(错误值) = 同步项目树(&项目根, &镜像目录值) {
        log::warn!(
            "镜像同步失败（回退虚拟检查）：{}：{错误值}",
            项目根.display()
        );
        let _ = std::fs::remove_dir_all(&镜像目录值);
        return false;
    }

    // 方言源转译：入口检测 → 写产物 → 返回产物坐标映射
    let Some(文件表) = 转译进镜像(缓存, 扩展名表, &项目根, &镜像目录值)
    else {
        let _ = std::fs::remove_dir_all(&镜像目录值);
        return false;
    };

    // 编译检查：`--offline` 防索引网络访问卡死；`--target-dir` 复用真实项目
    // 的依赖编译产物（镜像仅重编本地编译单元）
    let 目标目录 = 项目根.join("target");
    let Some((标准输出文本, 是否成功)) = 运行编译检查(&镜像目录值, &目标目录)
    else {
        return false;
    };

    let (可归位数, 发布数) = 发布编译器诊断(
        &标准输出文本,
        &镜像目录值,
        &文件表,
        缓存,
        管理器,
        内置诊断表,
        消息发送方,
    );
    if !是否成功 && 可归位数 == 0 {
        // 无任何可归位诊断却异常退出：包管理器层失败（清单/离线缺依赖等），
        // 镜像结论不可信，回退虚拟检查
        log::warn!("镜像检查异常退出且无诊断，回退虚拟检查");
        return false;
    }
    log::info!(
        "镜像检查完成：{} 个方言文件，{} 条编译消息，发布 {} 个文件",
        文件表.len(),
        可归位数,
        发布数
    );
    true
}

/// 定位项目根：触发文件最近的上层清单文件所在目录
fn 查找项目根(起点: &Path) -> Option<PathBuf> {
    let mut 目录值 = 起点.parent()?;
    loop {
        if 目录值.join("Cargo.toml").is_file() {
            return Some(目录值.to_path_buf());
        }
        目录值 = 目录值.parent()?;
    }
}

/// 项目根路径哈希（镜像目录名；同项目复用同一目录，跨会话互不串扰）
fn 路径哈希(路径值: &Path) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut 哈希器值 = DefaultHasher::new();
    路径值.hash(&mut 哈希器值);
    哈希器值.finish()
}

/// 镜像目录：系统临时目录下、用户名+进程号+项目哈希隔离的独立顶层目录
///
/// 不能放在虚拟项目目录之下：虚拟项目本身是编译包（含清单文件），
/// 嵌套其中的镜像包会被包管理器判定为“在工作区中却未被声明”而拒绝
/// 检查（父目录清单文件构成工作区根）；顶层独立目录同时避免
/// 分析器把镜像树当作工作区文件扫描。
fn 镜像目录路径(项目根: &Path) -> PathBuf {
    let 用户名 = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "default".to_string());
    let 安全用户名: String = 用户名
        .chars()
        .map(|本字| {
            if 本字.is_alphanumeric() || 本字 == '_' {
                本字
            } else {
                '_'
            }
        })
        .collect();
    std::env::temp_dir().join(format!(
        "i18n_lsp_mirror_{}_{}_{:x}",
        安全用户名,
        std::process::id(),
        路径哈希(项目根)
    ))
}

/// 路径自身是否为符号链接（不跟随末段链接）——与服务器模块
/// 虚拟临时目录同一防御水位，供镜像目录预检与并发复查使用。
fn 是符号链接(路径值: &Path) -> bool {
    路径值
        .symlink_metadata()
        .map(|元数据值| 元数据值.file_type().is_symlink())
        .unwrap_or(false)
}

/// 增量同步项目树到镜像（排除构建产物/版本库；目录符号链接跳过防逃逸与循环，
/// 文件符号链接跟随复制单文件）
///
/// 未改动文件按「大小 + 修改时间」跳过；镜像中已不存在、或已列入排除表的
/// 条目会被清理。`.rz` 是转译产物目录，`src/main.rs` 是入口产物，
/// 两者都保留给后续写盘与入口识别。
fn 同步项目树(来源: &Path, 目标: &Path) -> std::io::Result<()> {
    同步项目树带保留(来源, 目标, &PathBuf::new())
}

/// 带相对路径的递归实现：`相对` 为当前目录相对镜像根的路径
fn 同步项目树带保留(来源: &Path, 目标: &Path, 相对: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(目标)?;
    // 1) 收集来源名字，清理镜像中多余/不再允许的条目
    let mut 源名字集: HashSet<String> = HashSet::new();
    if let Ok(源条目集) = std::fs::read_dir(来源) {
        for 目录项 in 源条目集.flatten() {
            源名字集.insert(目录项.file_name().to_string_lossy().to_string());
        }
    }
    if let Ok(目标条目集) = std::fs::read_dir(目标) {
        for 目录项 in 目标条目集.flatten() {
            let 名称文本 = 目录项.file_name().to_string_lossy().to_string();
            if 名称文本 == ".rz" {
                continue;
            }
            // 入口产物由转译写盘，来源树里没有同名 .rs，不能当陈旧条目删掉
            let 子相对 = 相对.join(名称文本.as_str());
            if 子相对 == PathBuf::from("src").join("main.rs") {
                continue;
            }
            // cargo 在镜像里生成的锁文件：来源没有时保留，避免每次重新解析依赖
            if 名称文本 == "Cargo.lock" && !源名字集.contains(&名称文本) {
                continue;
            }
            let 排除项 =
                排除目录表.contains(&名称文本.as_str()) || 排除文件表.contains(&名称文本.as_str());
            if !排除项 && 源名字集.contains(&名称文本) {
                continue;
            }
            let 路径值 = 目录项.path();
            let Ok(条目类型) = 目录项.file_type() else {
                continue;
            };
            if 条目类型.is_dir() && !条目类型.is_symlink() {
                let _ = std::fs::remove_dir_all(&路径值);
            } else {
                let _ = std::fs::remove_file(&路径值);
            }
        }
    }
    // 2) 复制新增或改动的文件
    for 目录项 in std::fs::read_dir(来源)?.flatten() {
        let 路径值 = 目录项.path();
        let Ok(条目类型) = 目录项.file_type() else {
            continue;
        };
        let Ok(元数据值) = 目录项.metadata() else {
            continue;
        };
        let 名称值 = 目录项.file_name();
        let 名称文本 = 名称值.to_string_lossy();
        if 元数据值.is_dir() {
            if 条目类型.is_symlink() || 排除目录表.contains(&名称文本.as_ref()) {
                continue;
            }
            let 子相对 = 相对.join(名称文本.as_ref());
            同步项目树带保留(&路径值, &目标.join(名称文本.as_ref()), &子相对)?;
        } else if 元数据值.is_file() {
            if 排除文件表.contains(&名称文本.as_ref()) {
                continue;
            }
            let 目标路径 = 目标.join(名称文本.as_ref());
            // 大小相同且镜像文件不早于来源 → 视为未改动，跳过复制
            let mut 需要复制 = true;
            if let Ok(目标元数据) = std::fs::metadata(&目标路径) {
                if 目标元数据.is_file() && 目标元数据.len() == 元数据值.len() {
                    if let (Ok(源时间), Ok(目标时间)) = (元数据值.modified(), 目标元数据.modified())
                    {
                        if 目标时间 >= 源时间 {
                            需要复制 = false;
                        }
                    }
                }
            }
            if 需要复制 {
                std::fs::copy(&路径值, &目标路径)?;
            }
        }
    }
    Ok(())
}

/// 收集目录下全部方言源文件（递归；路径排序保证确定性）
fn 收集方言路径(目录值: &Path, 扩展名表: &[String], 输出表: &mut Vec<PathBuf>) {
    let Ok(条目集) = std::fs::read_dir(目录值) else {
        return;
    };
    for 目录项 in 条目集.flatten() {
        let 路径值 = 目录项.path();
        let Ok(条目类型) = 目录项.file_type() else {
            continue;
        };
        if 条目类型.is_dir() {
            收集方言路径(&路径值, 扩展名表, 输出表);
        } else if 条目类型.is_file() && 文件匹配扩展名(&路径值, 扩展名表) {
            输出表.push(路径值);
        }
    }
}

/// 路径扩展名是否属于支持的方言扩展名（如 `.zh`）
fn 文件匹配扩展名(路径值: &Path, 扩展名表: &[String]) -> bool {
    let Some(扩展名值) = 路径值.extension().and_then(|值| 值.to_str()) else {
        return false;
    };
    let 带点扩展名 = format!(".{}", 扩展名值);
    扩展名表
        .iter()
        .any(|单项| 单项 == &带点扩展名 || 单项 == 扩展名值)
}

/// 把镜像源码目录下的方言源转译为 .rs 产物（入口 → 入口产物），返回产物映射表
///
/// 转译与 rzc 同规则：模块名集合 = 源码目录顶层方言文件词干（`crate::` 前缀
/// 限定用）、项目声明上下文来自全部方言源（跨文件声明豁免）、入口产物补
/// 非 ASCII 模块 `#[path]` 注解。打开中的文档以缓冲区内容为准。
fn 转译进镜像(
    缓存: &Arc<转译缓存>,
    扩展名表: &[String],
    项目根: &Path,
    镜像目录值: &Path,
) -> Option<Vec<镜像文件>> {
    let 镜像源目录 = 镜像目录值.join("src");
    if !镜像源目录.is_dir() {
        return None;
    }
    // 方言源清单：源码目录递归（覆盖 rzc 的顶层扫描；嵌套文件经 `#[path]` 引用）
    let mut 方言路径表: Vec<PathBuf> = Vec::new();
    收集方言路径(&镜像源目录, 扩展名表, &mut 方言路径表);
    方言路径表.sort();
    if 方言路径表.is_empty() {
        return None;
    }

    // 模块名集合：源码目录全层级方言文件词干（与命令行收集项目上下文
    // 同规则）；嵌套模块（领域子目录下工具.zh 的「工具」）也纳入，供
    // `crate::领域::工具::成员` 多层路径链豁免
    let mut 模块名集: HashSet<String> = HashSet::new();
    // 方言源内容：打开文档取缓冲区，其余取磁盘
    let mut 源表: HashMap<PathBuf, String> = HashMap::new();
    for 镜像路径 in &方言路径表 {
        let Ok(相对路径) = 镜像路径.strip_prefix(镜像目录值) else {
            continue;
        };
        // 打开文档取缓冲区内容：按路径查询（容忍客户端资源标识与规范化
        // 路径的表示差异——Windows 上字符串键匹配会失效）
        let 内容选项 = 缓存
            .按路径查询(&项目根.join(相对路径))
            .filter(|条目值| 条目值.已打开)
            .map(|条目值| 条目值.母语内容.clone())
            .or_else(|| std::fs::read_to_string(镜像路径).ok());
        let Some(内容值) = 内容选项 else {
            continue;
        };
        if let Some(文件名干) = 镜像路径.file_stem().and_then(|值| 值.to_str()) {
            模块名集.insert(文件名干.to_string());
        }
        源表.insert(镜像路径.clone(), 内容值);
    }
    if 源表.is_empty() {
        return None;
    }

    let 源内容们: Vec<&str> = 源表.values().map(String::as_str).collect();
    let 项目上下文值 = 缓存.从内容集构建项目上下文(&模块名集, &源内容们);

    // 逐文件转译（内存中）：入口识别需要全部产物先就绪
    //
    // 走**静默**入口：本模块自己把全角标点与教学检查算成诊断按文件归属
    // 发布（见 `注入教学诊断`），而非静默入口会把项目里每个文件的每
    // 条教学告警再往标准错误无归属地打一遍（`[时间] [警告] [教学检查] 第 N 行…`，
    // 看不出是哪个文件）——与问题面板完全重复。除刷屏外还有实际代价：
    // 教学诊断输出会跑一整轮 Unicode 混淆/全角/教学检查扫描并逐条
    // `格式化!()`，结果全部丢弃；而镜像检查逐项目文件循环，开销随文件数线性增长。
    // 语言服务侧转译入口早先已统一为静默（翻译缓存），此处为残留的非静默点。
    let mut 待写表: Vec<待写产物> = Vec::new();
    for 镜像路径 in &方言路径表 {
        let Some(内容值) = 源表.get(镜像路径) else {
            continue;
        };
        let 转译成果 = i18n_rust_engine::静默带映射与项目转译管线(
            内容值,
            缓存.映射管理器引用(),
            Some(&模块名集),
            Some(&项目上下文值),
        );
        let 列位映射数据 = i18n_rust_engine::列位映射::列位映射表::回放生成(
            内容值,
            &转译成果.流程映射,
        );
        待写表.push(待写产物 {
            镜像路径: 镜像路径.clone(),
            转译文本: 转译成果.输出,
            列位映射数据,
            母语内容: 内容值.clone(),
        });
    }

    // 入口识别：入口词干优先（其天然产物即入口产物）；否则取与
    // 现有入口产物最相似的方言源（rzc 允许任意入口文件名，
    // 产物统一写入入口产物；相似度不足时保留原入口产物不动）
    let 主文件内容 = std::fs::read_to_string(镜像源目录.join("main.rs")).ok();
    let 入口路径 = 识别入口(&待写表, 主文件内容.as_deref());

    // 写产物 + 构造映射表
    let mut 文件表: Vec<镜像文件> = Vec::new();
    for 待写项 in 待写表 {
        let Ok(相对路径) = 待写项.镜像路径.strip_prefix(镜像目录值) else {
            continue;
        };
        let 源路径 = 项目根.join(相对路径);
        // 诊断发布资源标识复用缓存条目的客户端原样标识：与编辑器打开的
        // 文档严格一致（Windows 上规范化形式与客户端形式不同，直接用
        // 反推标识会导致诊断发布到用户不可见的文档上）；未登记条目
        // （缓存无路径匹配）回退规范化形式
        let 原始资源标识 = 缓存
            .按路径查询(&源路径)
            .map(|条目值| 条目值.原始资源标识.clone())
            .unwrap_or_else(|| 路径转统一资源标识(&源路径));
        let 是入口 = 入口路径.as_deref() == Some(待写项.镜像路径.as_path());
        // 产物落点与 rzc 同规则：入口固定写镜像的 src/main.rs（包管理器只认这一
        // 默认发现位置），其余进托管目录 `.rz/gen/` 并镜像源码布局——真实
        // 项目与镜像同构，`#[path]` 解析与诊断归位才能共用一套逻辑
        let 产物路径 = if 是入口 {
            镜像源目录.join("main.rs")
        } else {
            i18n_rust_engine::产物布局::产物路径(镜像目录值, &待写项.镜像路径)
        };
        // 子模块源文件所在目录：编译单元根与源文件同目录，非根文件位于
        // `<自身词干>/`（编译器文件式布局）
        let 子源目录 = if 是入口 {
            match 待写项.镜像路径.parent() {
                Some(父级) => 父级.to_path_buf(),
                None => PathBuf::new(),
            }
        } else {
            match 待写项.镜像路径.parent() {
                Some(父级) => match 待写项.镜像路径.file_stem().and_then(|值| 值.to_str())
                {
                    Some(文件名干) => 父级.join(文件名干),
                    None => 父级.to_path_buf(),
                },
                None => PathBuf::new(),
            }
        };
        // `#[path]` 注解按托管布局解析落点（方言子模块 → 托管产物，
        // 手写 `.rs` 子模块 → 跨出托管目录指回源码目录）；行映射供诊断行号回译
        let (内容, 行映射) =
            i18n_rust_engine::产物布局::注解产物模块路径并返回行映射(
                &待写项.转译文本,
                &子源目录,
                产物路径.parent().unwrap_or(&镜像源目录),
                镜像目录值,
                扩展名表,
            );
        if let Some(父级) = 产物路径.parent() {
            // 托管目录随首个非入口产物创建（镜像同步时已排除 `.rz`）
            let _ = std::fs::create_dir_all(父级);
        }
        // 内容未变则不写盘：cargo 以修改时间判定改动，重复写入会让整包重编译
        let 已有相同 = std::fs::read_to_string(&产物路径)
            .map(|旧内容| 旧内容 == 内容)
            .unwrap_or(false);
        if !已有相同 {
            if let Err(错误值) = std::fs::write(&产物路径, &内容) {
                log::warn!("镜像产物写入失败：{}：{错误值}", 产物路径.display());
                continue;
            }
        }
        文件表.push(镜像文件 {
            产物路径,
            原始资源标识,
            母语内容: 待写项.母语内容,
            列位映射数据: 待写项.列位映射数据,
            入口行映射: Some(行映射),
        });
    }
    // 增量镜像不整体删除，已删除源文件的旧产物要按命令行同一规则清掉，
    // 否则 rustc 仍可能按默认模块查找命中它们
    let _ = i18n_rust_engine::产物布局::清理失效产物(镜像目录值, 扩展名表);
    if 文件表.is_empty() {
        None
    } else {
        Some(文件表)
    }
}

/// 入口识别：返回镜像中作为入口产物来源的方言源路径
///
/// 词干为入口名的方言文件优先（rzc 语义：其产物即入口产物）；
/// 否则在所有方言产物（含 `#[path]` 注解）中找与现有入口产物
/// 相似度最高者，相似度须达到阈值（初次构建即精确匹配，入口编辑后
/// 仍高度相似；其他模块与入口的相似度接近零）。
fn 识别入口(待写表: &[待写产物], 主文件: Option<&str>) -> Option<PathBuf> {
    for 待写项 in 待写表 {
        if 待写项.镜像路径.file_stem().and_then(|值| 值.to_str()) == Some("main") {
            return Some(待写项.镜像路径.clone());
        }
    }
    let 主文件 = 主文件?;
    let mut 最佳: Option<(PathBuf, f64)> = None;
    for 待写项 in 待写表 {
        let (加注解, _) = i18n_rust_engine::模块路径::注解非母语模块并返回行映射(
            &待写项.转译文本,
        );
        let 分数 = 行相似度(&加注解, 主文件);
        if 分数 >= 0.6 && 最佳.as_ref().is_none_or(|(_, 旧分)| 分数 > *旧分) {
            最佳 = Some((待写项.镜像路径.clone(), 分数));
        }
    }
    最佳.map(|(路径值, _)| 路径值)
}

/// 逐行相似度：实质行（≥8 字符）命中率（命中数 / 两侧较大行数）
///
/// 分母取 候选行数与参照行数的较大者，小文件的局部巧合命中无法抬高分数。
fn 行相似度(候选: &str, 参照: &str) -> f64 {
    let 是实质行 = |行文本: &str| 行文本.trim().chars().count() >= 8;
    let 参照行集: HashSet<&str> = 参照
        .lines()
        .map(|行文本| 行文本.trim())
        .filter(|行文本| 是实质行(行文本))
        .collect();
    if 参照行集.is_empty() {
        return 0.0;
    }
    let mut 总数 = 0usize;
    let mut 命中数 = 0usize;
    for 行文本 in 候选
        .lines()
        .map(|行文本| 行文本.trim())
        .filter(|行文本| 是实质行(行文本))
    {
        总数 += 1;
        if 参照行集.contains(行文本) {
            命中数 += 1;
        }
    }
    if 总数 == 0 {
        return 0.0;
    }
    命中数 as f64 / 总数.max(参照行集.len()) as f64
}

/// 启动编译检查并等待退出（持续排空管道防写满阻塞，超时强杀）
///
/// 环境隔离：清空 `RUSTC_WRAPPER`/`RUSTFLAGS` 等可由宿主环境注入的编译钩子，
/// 并强制离线；镜像已排除项目级 `.cargo` 与 `rust-toolchain*`，这些清理是
/// 「不受信仓库不通过镜像检查改变本机编译行为」的第二道防线。
/// 返回 (标准输出, 退出码是否成功)；进程无法启动/超时被强杀时返回无。
fn 运行编译检查(镜像目录值: &Path, 目标目录: &Path) -> Option<(String, bool)> {
    let mut 子进程实例 = match std::process::Command::new("cargo")
        .args(["check", "--offline", "--message-format=json"])
        .arg("--target-dir")
        .arg(目标目录)
        .current_dir(镜像目录值)
        .env("CARGO_NET_OFFLINE", "true")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("CARGO_BUILD_RUSTC_WRAPPER")
        .env_remove("CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(子进程值) => 子进程值,
        Err(错误值) => {
            log::warn!("镜像编译检查启动失败：{错误值}");
            return None;
        }
    };
    let mut 标准输出管道 = 子进程实例.stdout.take()?;
    let mut 标准错误管道 = 子进程实例.stderr.take()?;
    let 标准输出线程 = std::thread::spawn(move || {
        let mut 缓冲值 = Vec::new();
        let _ = 标准输出管道.read_to_end(&mut 缓冲值);
        缓冲值
    });
    let 标准错误线程 = std::thread::spawn(move || {
        let mut 缓冲值 = Vec::new();
        let _ = 标准错误管道.read_to_end(&mut 缓冲值);
        缓冲值
    });

    let mut 状态值 = None;
    for _ in 0..检查超时毫秒 / 轮询间隔毫秒 {
        match 子进程实例.try_wait() {
            Ok(Some(状态项)) => {
                状态值 = Some(状态项);
                break;
            }
            Ok(状态选项) => {
                let _ = 状态选项;
                std::thread::sleep(std::time::Duration::from_millis(轮询间隔毫秒));
            }
            Err(_) => break,
        }
    }
    let 状态值 = match 状态值 {
        Some(状态项) => 状态项,
        None => {
            log::warn!("镜像编译检查超时/异常，已终止（回退虚拟检查）");
            let _ = 子进程实例.kill();
            let _ = 子进程实例.wait();
            return None;
        }
    };
    let 标准输出文本 = 标准输出线程.join().unwrap_or_default();
    let 标准错误数据 = 标准错误线程.join().unwrap_or_default();
    if !状态值.success() {
        // 编译错误属正常情况（诊断在标准输出）时同样以非零码退出；标准错误
        // 末尾才是包管理器层的真正错误（清单/锁/依赖解析），首行往往只是
        // `Checking ...` 进度行，取末尾若干行输出便于排查
        let 标准错误文本 = String::from_utf8_lossy(&标准错误数据);
        if 标准错误文本.contains("failed to") || 标准错误文本.contains("error: could not")
        {
            let 尾部: Vec<&str> = 标准错误文本.lines().rev().take(6).collect();
            let 尾部: Vec<&str> = 尾部.into_iter().rev().collect();
            log::warn!(
                "镜像编译检查非零退出（{:?}）：{}",
                状态值.code(),
                尾部.join(" / ")
            );
        }
    }
    Some((
        String::from_utf8_lossy(&标准输出文本).to_string(),
        状态值.success(),
    ))
}

/// 解析编译器 JSON 诊断流：坐标回译 → 消息翻译 → 所有权提取 → 合并发布
///
/// 返回 (可归位的编译消息数, 发布文件数)。
fn 发布编译器诊断(
    标准输出文本: &str,
    镜像目录值: &Path,
    文件表: &[镜像文件],
    缓存: &Arc<转译缓存>,
    管理器: &i18n_rust_engine::映射管理器::映射管理器,
    内置诊断表: &Arc<Mutex<HashMap<String, Vec<Value>>>>,
    消息发送方: &crossbeam_channel::Sender<lsp_server::Message>,
) -> (usize, usize) {
    // 产物路径 → 方言源映射：规范路径键归一编译器的绝对/相对路径
    let 索引表: HashMap<PathBuf, &镜像文件> = 文件表
        .iter()
        .filter_map(|文件项| {
            std::fs::canonicalize(&文件项.产物路径)
                .ok()
                .map(|路径值| (路径值, 文件项))
        })
        .collect();

    let mut 按资源标识: HashMap<String, Vec<Value>> = HashMap::new();
    let mut 可归位数 = 0usize;
    for 行文本 in 标准输出文本.lines() {
        let Ok(值项) = serde_json::from_str::<Value>(行文本) else {
            continue;
        };
        if 值项["reason"].as_str() != Some("compiler-message") {
            continue;
        }
        let 消息项 = &值项["message"];
        let Some(跨度表) = 消息项["spans"].as_array() else {
            continue;
        };
        // 主跨度优先（编译器的跨度表可能把次级位置排在前面）
        let Some(跨度项) = 跨度表
            .iter()
            .find(|单项| 单项["is_primary"] == Value::Bool(true))
            .or_else(|| 跨度表.first())
        else {
            continue;
        };
        let Some(文件项) = 解析跨度文件(跨度项, 镜像目录值, &索引表) else {
            continue; // 非方言文件（手写 .rs、标准库等）：不发布
        };
        可归位数 += 1;

        let 原始消息 = 消息项["message"].as_str().unwrap_or("");
        let 错误码值 = 消息项["code"]["code"].as_str().unwrap_or("").to_string();
        let 严重级 = match 消息项["level"].as_str() {
            Some("error") => 1,
            Some("warning") => 2,
            _ => 3,
        };
        let Some((起行, 起列, 止行, 止列)) = 映射跨度范围(文件项, 跨度项)
        else {
            continue;
        };
        let 范围值 = serde_json::json!({
            "start": { "line": 起行, "character": 起列 },
            "end": { "line": 止行, "character": 止列 }
        });

        // 次级跨度 → 关联信息（所有权可视化等）；消息先保留英文，
        // 供 提取所有权详情 关键词判定，发布前再翻译
        let mut 原始关联: Vec<Value> = Vec::new();
        for 单项 in 跨度表 {
            if 单项["is_primary"] == Value::Bool(true) {
                continue;
            }
            let Some(标签) = 单项["label"]
                .as_str()
                .filter(|标签文本| !标签文本.is_empty())
            else {
                continue;
            };
            let Some(关联文件) = 解析跨度文件(单项, 镜像目录值, &索引表) else {
                continue;
            };
            let Some((甲, 乙, 丙, 丁)) = 映射跨度范围(关联文件, 单项) else {
                continue;
            };
            原始关联.push(serde_json::json!({
                "location": {
                    "uri": 关联文件.原始资源标识,
                    "range": {
                        "start": { "line": 甲, "character": 乙 },
                        "end": { "line": 丙, "character": 丁 }
                    }
                },
                "message": 标签
            }));
        }

        let 原始 = serde_json::json!({ "message": 原始消息, "code": 错误码值 });
        let 还原后 = serde_json::json!({ "range": 范围值, "relatedInformation": 原始关联 });
        let 所有权详情值 = 提取所有权详情(&原始, &还原后, &文件项.原始资源标识);

        // 主消息与命令行同口径：传错误码 + 主跨度标签（编译器 JSON 携带
        // 期望类型与实际类型），使类型不匹配等能回填期望/实际并中文化类型名
        let 主消息 = 翻译诊断消息(
            if 错误码值.is_empty() {
                None
            } else {
                Some(&错误码值)
            },
            原始消息,
            跨度项["label"].as_str(),
        );
        let mut 诊断项 = serde_json::json!({
            "range": 范围值,
            "severity": 严重级,
            "code": 错误码值,
            "message": 主消息,
        });
        if !原始关联.is_empty() {
            let 已翻译: Vec<Value> = 原始关联
                .iter()
                .map(|关联项| {
                    let mut 单项 = 关联项.clone();
                    单项["message"] = Value::String(翻译诊断消息(
                        None,
                        关联项["message"].as_str().unwrap_or(""),
                        None,
                    ));
                    单项
                })
                .collect();
            诊断项["relatedInformation"] = Value::Array(已翻译);
        }
        if let Some(详情值) = 所有权详情值
            && let Ok(详情条目) = serde_json::to_value(&详情值)
        {
            诊断项["data"] = 详情条目;
        }
        按资源标识
            .entry(文件项.原始资源标识.clone())
            .or_default()
            .push(诊断项);
    }

    // 合并内置诊断（分析器最近一次映射后的方言坐标诊断 + 教学提示）：
    // 同错误码且同起始行视为重复（与虚拟检查的合并规则一致），
    // 避免镜像结果覆盖语法/类型实时诊断
    let mut 发布数 = 0usize;
    for (资源标识, mut 诊断表) in 按资源标识 {
        if let Ok(守卫) = 内置诊断表.lock()
            && let Some(内置项) = 守卫.get(&资源标识)
        {
            for 内置诊断 in 内置项.clone() {
                let 重复 = 诊断表.iter().any(|诊断项| {
                    诊断项["code"] == 内置诊断["code"]
                        && 诊断项["range"]["start"]["line"] == 内置诊断["range"]["start"]["line"]
                });
                if !重复 {
                    诊断表.push(内置诊断);
                }
            }
        }
        // 教学诊断注入（全角标点 + 教学检查）：由条目内容直接计算，
        // 不依赖内置缓存（镜像检查先于分析器首批发布时缓存为空）
        if let Some(条目值) = 缓存.查询原始(&资源标识) {
            注入教学诊断(
                &mut 诊断表,
                &条目值,
                管理器.取校验分桶(),
                缓存.歧义构造器词(),
                缓存.英文回落映射(),
                缓存.关键词映射(),
            );
        }
        let 通知项 = lsp_server::Notification {
            method: "textDocument/publishDiagnostics".to_string(),
            params: serde_json::json!({ "uri": 资源标识, "diagnostics": 诊断表 }),
        };
        let _ = 消息发送方.send(lsp_server::Message::Notification(通知项));
        发布数 += 1;
    }
    (可归位数, 发布数)
}

/// 解析跨度归属的方言文件（镜像产物路径 → 镜像文件）
fn 解析跨度文件<'甲>(
    跨度项: &Value,
    镜像目录值: &Path,
    索引表: &HashMap<PathBuf, &'甲 镜像文件>,
) -> Option<&'甲 镜像文件> {
    let 名称值 = 跨度项["file_name"].as_str()?;
    let 路径值 = Path::new(名称值);
    // 编译器输出相对路径（工作目录为镜像项目根）或绝对路径，统一绝对化
    let 绝对路径 = if 路径值.is_absolute() {
        路径值.to_path_buf()
    } else {
        镜像目录值.join(路径值)
    };
    let 规范值 = std::fs::canonicalize(绝对路径).ok()?;
    索引表.get(&规范值).copied()
}

/// 跨度的起止位置回译：方言从 0 开始的行 + UTF-16 列（语言服务协议）
fn 映射跨度范围(文件项: &镜像文件, 跨度项: &Value) -> Option<(u32, u32, u32, u32)> {
    let 行号 = 跨度项["line_start"].as_u64()? as u32;
    let 列号 = 跨度项["column_start"].as_u64()? as u32;
    let 末行号 = 跨度项["line_end"].as_u64().unwrap_or(行号 as u64) as u32;
    let 末列号 = 跨度项["column_end"].as_u64().unwrap_or(列号 as u64) as u32;
    let (起行, 起列) = 映射位置(文件项, 行号, 列号);
    let (止行, 止列) = 映射位置(文件项, 末行号, 末列号);
    Some((起行, 起列, 止行, 止列))
}

/// 单点回译：产物从 1 开始的行与字符列 → 方言从 0 开始的行与 UTF-16 列
///
/// 行号：入口产物含 `#[path]` 注解插入行，先经入口行映射换算回引擎直出
/// 行号（列映射以引擎直出产物为基准；注解行归属其模块声明行）。
/// 列号：引擎列映射输出方言字符列（与编译器列口径一致），再按方言文本
/// 换算为语言服务协议的 UTF-16 列。
fn 映射位置(文件项: &镜像文件, 行号从1: u32, 列号从1: u32) -> (u32, u32) {
    let 引擎行号 = match &文件项.入口行映射 {
        Some(映射值) => 映射值
            .get(行号从1.saturating_sub(1) as usize)
            .map(|行值| *行值 as u32 + 1)
            .unwrap_or(行号从1),
        None => 行号从1,
    };
    let (母语行号, 母语字符列) = 文件项.列位映射数据.回译坐标(引擎行号, 列号从1);
    (
        母语行号.saturating_sub(1),
        母语字符列转编码单元(&文件项.母语内容, 母语行号, 母语字符列),
    )
}

/// 方言行内字符列（从 1 开始）→ UTF-16 列（从 0 开始）
///
/// 引擎列映射的列口径为字符数（与编译器 JSON 一致）；语言服务协议要求
/// UTF-16 代码单元列，非基本平面字符（表情符号等）占 2 个单元，须逐字符累加。
fn 母语字符列转编码单元(母语内容: &str, 行号从1: u32, 字符列从1: u32) -> u32 {
    let Some(行文本) = 母语内容.lines().nth(行号从1.saturating_sub(1) as usize) else {
        return 字符列从1.saturating_sub(1);
    };
    行文本
        .chars()
        .take(字符列从1.saturating_sub(1) as usize)
        .map(|字符值| 字符值.len_utf16() as u32)
        .sum()
}

#[cfg(test)]
mod 母语测试 {
    use super::*;

    fn 构造待写项(名称值: &str, 转译文本: &str) -> 待写产物 {
        待写产物 {
            镜像路径: PathBuf::from("/mirror/src").join(名称值),
            转译文本: 转译文本.to_string(),
            列位映射数据: i18n_rust_engine::列位映射::列位映射表::回放生成(
                "",
                &[],
            ),
            母语内容: String::new(),
        }
    }

    /// 小于 8 字符的短行（`}`、空行等）不参与相似度，避免结构行虚高命中
    #[test]
    fn 行相似度忽略短行() {
        assert_eq!(行相似度("}\n{\n", "}\n{\n"), 0.0);
        assert_eq!(行相似度("", ""), 0.0);
        // 参照侧全是短行 → 实质行为空 → 0.0
        assert_eq!(行相似度("fn main() {}", "}\n)\n"), 0.0);
    }

    /// 完全相同为 1.0；无交集为 0.0
    #[test]
    fn 行相似度边界() {
        let 样本文本 = "fn main() {\n    println!(\"hi\");\n}\n";
        assert_eq!(行相似度(样本文本, 样本文本), 1.0);
        assert_eq!(
            行相似度(样本文本, "fn other() {\n    let 甲 = 1;\n}\n"),
            0.0
        );
    }

    /// 分母取两侧较大者：小文件与参照的局部巧合命中无法抬高分数
    ///
    /// 候选仅 1 条实质行且命中，参照有 3 条实质行 → 1/3 而非 1.0。
    #[test]
    fn 行相似度分母取较大侧() {
        let 参照文本 = "fn 甲() {\nfn 乙() {\nfn 丙() {\n";
        let 分数 = 行相似度("fn 甲() {\n", 参照文本);
        assert!(
            (分数 - 1.0 / 3.0).abs() < 1e-9,
            "分母应为 3（较大侧），实际 {分数}"
        );
    }

    /// 分词干为入口名的方言文件优先作为入口（不看相似度）
    #[test]
    fn 识别入口优先主词干() {
        let 候选表 = vec![
            构造待写项("甲.zh", "fn 甲() {\n    let 甲值 = 1;\n}\n"),
            构造待写项("main.zh", "fn 主函数() {\n}\n"),
            构造待写项("乙.zh", "fn 乙() {\n    let 乙值 = 2;\n}\n"),
        ];
        assert_eq!(
            识别入口(&候选表, None).map(|路径值| 路径值
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()),
            Some("main.zh".to_string()),
            "词干为入口名时应无视相似度直接选中"
        );
    }

    /// 无入口词干时按与现有入口产物的相似度选取（须达阈值 0.6）
    #[test]
    fn 识别入口按相似度阈值() {
        let 入口文本 = "fn 主函数() {\n    println!(\"你好\");\n}\n";
        let 候选表 = vec![
            构造待写项("甲.zh", "fn 甲() {\n    let 甲值 = 1;\n}\n"),
            构造待写项("入口.zh", 入口文本),
        ];
        assert_eq!(
            识别入口(&候选表, Some(入口文本)).map(|路径值| 路径值
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()),
            Some("入口.zh".to_string()),
            "相似度达阈值者应被选为入口"
        );
        // 全部候选都与参照不相似 → 无（低于阈值宁可不识别）
        assert!(
            识别入口(
                &[构造待写项(
                    "甲.zh",
                    "fn 甲() {\n    let 甲值 = 1;\n}\n"
                )],
                Some("fn 乙() {\n    let 乙值 = 2;\n}\n")
            )
            .is_none(),
            "相似度不足阈值时不应误判入口"
        );
    }

    /// 无入口词干且无参照入口产物时无法识别入口
    #[test]
    fn 识别入口无参照() {
        let 候选表 = vec![构造待写项(
            "甲.zh",
            "fn 甲() {\n    let 甲值 = 1;\n}\n",
        )];
        assert!(识别入口(&候选表, None).is_none());
        assert!(识别入口(&[], Some("fn 主函数() {\n}\n")).is_none());
    }

    /// 方言扩展名匹配：大小写与后缀精确性
    #[test]
    fn 判定文件匹配扩展名() {
        let 扩展名列表 = vec!["zh".to_string(), "ja".to_string()];
        assert!(文件匹配扩展名(Path::new("/p/main.zh"), &扩展名列表));
        assert!(文件匹配扩展名(Path::new("/p/模块.ja"), &扩展名列表));
        assert!(!文件匹配扩展名(Path::new("/p/main.rs"), &扩展名列表));
        // 仅匹配完整后缀：多一个字符不算 `.zh`
        assert!(!文件匹配扩展名(
            Path::new("/p/main.zhx"),
            &扩展名列表
        ));
        // 无扩展名
        assert!(!文件匹配扩展名(Path::new("/p/main"), &扩展名列表));
    }

    /// 项目根哈希：同路径稳定、不同路径不同（镜像目录隔离的依据）
    #[test]
    fn 路径哈希稳定且不同() {
        let 甲 = 路径哈希(Path::new("/a/b"));
        assert_eq!(甲, 路径哈希(Path::new("/a/b")), "同路径应稳定");
        assert_ne!(甲, 路径哈希(Path::new("/a/c")), "不同路径应不同");
    }

    /// 字符列 → UTF-16 列：非基本平面字符占 2 个单元，基本平面中文占 1
    #[test]
    fn 母语字符列转编码单元计数() {
        // "字符😀甲"：字符列从 1 开始 → 取前 n 个字符累加编码单元
        let 样本文本 = "字符😀甲";
        assert_eq!(母语字符列转编码单元(样本文本, 1, 1), 0);
        assert_eq!(母语字符列转编码单元(样本文本, 1, 2), 1);
        // 含表情符号：到第 4 列时已计入表情符号的 2 个单元
        assert_eq!(母语字符列转编码单元(样本文本, 1, 4), 4);
        // 越界行回退为从 0 开始的字符列
        assert_eq!(母语字符列转编码单元(样本文本, 9, 3), 2);
    }

    /// 项目根定位：沿目录上行找最近的清单文件；全程没有则无
    #[test]
    fn 查找项目根最近且缺失() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根目录 = 临时.path().join("proj");
        let 深层 = 根目录.join("src/sub/deep");
        std::fs::create_dir_all(&深层).unwrap();
        std::fs::write(根目录.join("Cargo.toml"), "[package]\n").unwrap();
        let 触发文件 = 深层.join("main.zh");
        assert_eq!(查找项目根(&触发文件).as_deref(), Some(根目录.as_path()));
        // 内层另有清单文件时取最近者
        std::fs::write(深层.join("Cargo.toml"), "[package]\n").unwrap();
        assert_eq!(查找项目根(&触发文件).as_deref(), Some(深层.as_path()));
        // 无清单文件的目录树（单文件教学场景）→ 无
        let 其他目录 = 临时.path().join("noproject/a");
        std::fs::create_dir_all(&其他目录).unwrap();
        assert!(查找项目根(&其他目录.join("main.zh")).is_none());
    }

    /// 镜像目录名：固定前缀 + 用户 + 进程号 + 项目哈希；用户名非法字符清洗为下划线
    #[test]
    fn 镜像目录形状与用户名净化() {
        let 根路径 = Path::new("/tmp/proj-alpha");
        let 目录值 = 镜像目录路径(根路径);
        let 名称值 = 目录值.file_name().unwrap().to_string_lossy().into_owned();
        assert!(名称值.starts_with("i18n_lsp_mirror_"), "实际：{名称值}");
        assert!(
            名称值.ends_with(&format!("_{:x}", 路径哈希(根路径))),
            "目录名应以项目哈希结尾：{名称值}"
        );

        // 用户名含空格/斜杠等非法字符时清洗（保存/恢复用户名环境变量，避免污染其他测试）
        let 已保存 = std::env::var("USER").ok();
        unsafe {
            std::env::set_var("USER", "a b/c");
        }
        let 目录二 = 镜像目录路径(根路径);
        let 名称二 = 目录二.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            名称二.contains("a_b_c"),
            "用户名非法字符应替换为下划线：{名称二}"
        );
        unsafe {
            match 已保存 {
                Some(值文本) => std::env::set_var("USER", 值文本),
                None => std::env::remove_var("USER"),
            }
        }
    }

    /// 是符号链接识别符号链接（含悬空链接），非链接路径返回假—与
    /// 服务器模块同名测试同风格，仅 Unix 跑（Windows 建符号链接需特权，避免抖动）。
    #[cfg(unix)]
    #[test]
    fn 是符号链接识别链接与普通文件() {
        let 临时 = tempfile::tempdir().unwrap();
        let 文件值 = 临时.path().join("f");
        std::fs::write(&文件值, b"x").unwrap();
        assert!(!是符号链接(&文件值), "普通文件不是符号链接");
        assert!(
            !是符号链接(&临时.path().join("absent")),
            "不存在路径不是符号链接"
        );
        let 链接值 = 临时.path().join("link");
        std::os::unix::fs::symlink(&文件值, &链接值).unwrap();
        assert!(是符号链接(&链接值), "符号链接应被识别");
        let 悬空值 = 临时.path().join("dangling");
        std::os::unix::fs::symlink(临时.path().join("gone"), &悬空值).unwrap();
        assert!(是符号链接(&悬空值), "悬空符号链接仍须识别为链接");
    }

    /// 增量同步：排除项不复制；未改动文件跳过；删除源后镜像清理；
    /// `.rz` 与入口产物 `src/main.rs` 保留
    #[test]
    fn 同步项目树增量与清理() {
        let 临时 = tempfile::tempdir().unwrap();
        let 来源值 = 临时.path().join("proj");
        std::fs::create_dir_all(来源值.join("src/sub")).unwrap();
        std::fs::write(来源值.join("Cargo.toml"), "[package]\n").unwrap();
        std::fs::write(来源值.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        std::fs::write(来源值.join("src/sub/util.zh"), "公开 函数 工具() {}\n").unwrap();
        for 排除项 in [
            "target/debug/x",
            ".git/config",
            "node_modules/lib/a",
            "dist/bundle",
            ".cargo/config.toml",
            // 托管生成物不镜像：镜像里由转译重新产出，带上旧产物只会多余字节
            ".rz/gen/src/数学.rs",
        ] {
            let 路径值 = 来源值.join(排除项);
            std::fs::create_dir_all(路径值.parent().unwrap()).unwrap();
            std::fs::write(路径值, "应被排除").unwrap();
        }
        let 目标值 = 临时.path().join("mirror");
        同步项目树(&来源值, &目标值).unwrap();
        assert!(目标值.join("Cargo.toml").is_file());
        assert!(目标值.join("src/main.zh").is_file());
        assert!(目标值.join("src/sub/util.zh").is_file());
        assert!(!目标值.join("target").exists());
        assert!(!目标值.join(".git").exists());
        assert!(!目标值.join("node_modules").exists());
        assert!(!目标值.join("dist").exists());
        assert!(!目标值.join(".rz").exists());
        assert!(!目标值.join(".cargo").exists());

        // 预置生成物与旧版本遗留的 .cargo
        std::fs::write(目标值.join("src/main.rs"), "// 入口产物\n").unwrap();
        std::fs::create_dir_all(目标值.join(".rz/gen/src")).unwrap();
        std::fs::write(目标值.join(".rz/gen/src/旧.rs"), "// 旧产物\n").unwrap();
        std::fs::create_dir_all(目标值.join(".cargo")).unwrap();
        std::fs::write(目标值.join(".cargo/config.toml"), "").unwrap();
        let 目标主文件 = 目标值.join("src/main.zh");
        let 首次修改时间 = std::fs::metadata(&目标主文件).unwrap().modified().unwrap();
        // 未改动：第二次同步不重写文件（修改时间不变）
        同步项目树(&来源值, &目标值).unwrap();
        assert_eq!(
            std::fs::metadata(&目标主文件).unwrap().modified().unwrap(),
            首次修改时间
        );
        assert!(目标值.join("src/main.rs").is_file(), "入口产物应保留");
        assert!(
            目标值.join(".rz/gen/src/旧.rs").is_file(),
            "托管产物目录应保留"
        );
        assert!(!目标值.join(".cargo").exists(), "旧遗留 .cargo 应清掉");

        // 删除源文件：镜像对应条目被清理
        std::fs::remove_file(来源值.join("src/sub/util.zh")).unwrap();
        同步项目树(&来源值, &目标值).unwrap();
        assert!(!目标值.join("src/sub/util.zh").exists());
    }

    /// 方言源收集：递归、仅匹配给定扩展名
    #[test]
    fn 收集方言路径递归() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根目录 = 临时.path().join("proj/src");
        std::fs::create_dir_all(根目录.join("sub")).unwrap();
        std::fs::write(根目录.join("main.zh"), "").unwrap();
        std::fs::write(根目录.join("main.rs"), "").unwrap();
        std::fs::write(根目录.join("sub/util.zh"), "").unwrap();
        let mut 收集表 = Vec::new();
        收集方言路径(&根目录, &[".zh".to_string()], &mut 收集表);
        收集表.sort();
        let 名称清单: Vec<String> = 收集表
            .iter()
            .map(|路径值| 路径值.strip_prefix(&根目录).unwrap().display().to_string())
            .collect();
        // 期望值按平台构造分隔符：去前缀后再展示会带上平台分隔符，
        // 硬写固定分隔符只会测到平台差异而非收集逻辑
        let 期望清单: Vec<String> = [
            PathBuf::from("main.zh"),
            PathBuf::from("sub").join("util.zh"),
        ]
        .iter()
        .map(|路径值| 路径值.to_string_lossy().into_owned())
        .collect();
        assert_eq!(名称清单, 期望清单);
        // 目录不存在不报错（调用方依赖此容错）
        let mut 空表 = Vec::new();
        收集方言路径(&临时.path().join("absent"), &[".zh".to_string()], &mut 空表);
        assert!(空表.is_empty());
    }

    /// 构造一个带列映射的镜像文件（母语「让 x = 1;」→ 英文 `let x = 1;`）
    fn 构造映射文件(入口行映射: Option<Vec<usize>>) -> 镜像文件 {
        let 母语源 = "让 x = 1;\n";
        let 编辑表 = [i18n_rust_engine::缓存::源码映射条目::新建(
            0, 3, "让", "let",
        )];
        镜像文件 {
            产物路径: PathBuf::from("/mirror/src/main.rs"),
            原始资源标识: "file:///proj/src/main.zh".to_string(),
            母语内容: 母语源.to_string(),
            列位映射数据: i18n_rust_engine::列位映射::列位映射表::回放生成(
                母语源, &编辑表,
            ),
            入口行映射,
        }
    }

    /// 跨度回译：列经列映射换算，行转从 0 开始；无 `#[path]` 行映射时行号恒等
    #[test]
    fn 映射跨度范围列与行基础() {
        let 文件项 = 构造映射文件(None);
        // 产物 `let x = 1;` 第 1 行第 5 列（x 的首列，从 1 开始）
        // → 方言「让 x = 1;」第 3 列；输出为从 0 开始的行 + UTF-16 列
        let 跨度项 = serde_json::json!({"line_start": 1, "column_start": 5});
        assert_eq!(映射跨度范围(&文件项, &跨度项), Some((0, 2, 0, 2)));
        // 缺 行起点 → 无（无法回译）
        let 缺失项 = serde_json::json!({"column_start": 5});
        assert_eq!(映射跨度范围(&文件项, &缺失项), None);
    }

    /// 入口产物含 `#[path]` 注解插入行：产物行先经入口行映射回引擎直出行号
    #[test]
    fn 映射位置换算入口插入行() {
        // 方言 3 行；产物在顶部插入 1 行注解：产物 1 行 ↔ 引擎 3 行（映射值 2 → 行 3）
        let 母语源 = "函数 主函数() {\n    打印行!(\"hi\");\n}\n";
        let 编辑表 = [
            i18n_rust_engine::缓存::源码映射条目::新建(0, 6, "函数", "fn"),
            // 第 1 行 21 字节 + 4 个前导空格 → 「打印行」字节偏移 25
            i18n_rust_engine::缓存::源码映射条目::新建(25, 9, "打印行", "println"),
        ];
        let 文件项 = 镜像文件 {
            产物路径: PathBuf::from("/mirror/src/main.rs"),
            原始资源标识: "file:///proj/src/main.zh".to_string(),
            母语内容: 母语源.to_string(),
            列位映射数据: i18n_rust_engine::列位映射::列位映射表::回放生成(
                母语源, &编辑表,
            ),
            入口行映射: Some(vec![2, 0, 1]),
        };
        // 产物第 1 行 → 引擎第 3 行（"}" 行）→ 方言从 0 开始的第 2 行
        let (起行, _起列, 止行, _止列) = 映射跨度范围(
            &文件项,
            &serde_json::json!({"line_start": 1, "column_start": 1}),
        )
        .unwrap();
        assert_eq!((起行, 止行), (2, 2));
        // 产物第 2 行 → 引擎第 1 行（映射值 0）
        let (起行二, _, _, _) = 映射跨度范围(
            &文件项,
            &serde_json::json!({"line_start": 2, "column_start": 1}),
        )
        .unwrap();
        assert_eq!(起行二, 0);
    }

    /// 全链路非基本平面回译（列映射字符列 → UTF-16 列 的组合边界）：
    /// 诊断列落在非基本平面字符（表情符号）之后时，回译必须先把表情符号当作
    /// 1 个字符参与列映射、再在转 UTF-16 时计为 2 个单元——两步口径不能混用。
    /// 母语「让😀x = 1;」→ 产物 `let😀x = 1;`：x 在产物第 5 字符列，回译出
    /// 方言字符列 3（让=1、😀=2、x=3），转 UTF-16 为 3（让占 1 单元 + 😀占 2）。
    /// 与纯基本平面基线同列只给 2 形对比——若误用字符数口径会少算表情符号的额外
    /// 单元（返回 2），本测试把模块文档「混用会在非基本平面字符上产生偏移」钉死。
    #[test]
    fn 映射跨度范围非基本平面列前() {
        let 母语源 = "让😀x = 1;\n";
        let 编辑表 = [i18n_rust_engine::缓存::源码映射条目::新建(
            0, 3, "让", "let",
        )];
        let 文件项 = 镜像文件 {
            产物路径: PathBuf::from("/mirror/src/main.rs"),
            原始资源标识: "file:///proj/src/main.zh".to_string(),
            母语内容: 母语源.to_string(),
            列位映射数据: i18n_rust_engine::列位映射::列位映射表::回放生成(
                母语源, &编辑表,
            ),
            入口行映射: None,
        };
        // 产物 `let😀x = 1;` 中 x 在第 5 字符列 → 方言 UTF-16 列 3
        let 跨度项 = serde_json::json!({"line_start": 1, "column_start": 5});
        assert_eq!(映射跨度范围(&文件项, &跨度项), Some((0, 3, 0, 3)));
    }
}
