//! 第三方库映射校验与脚手架模块
//!
//! 提供 `rzc mapping check` 与 `rzc mapping scaffold` 的核心逻辑：
//! - check：校验单个语言包的 crates/*.toml 映射质量
//!   （TOML 可解析 / 无重复键 / 关键字避让 / 跨文件同键冲突 / 条目数统计），
//!   以及跨内置语言的条目数一致性对比。
//! - scaffold：从源语言 crates 生成目标语言的翻译骨架（保留英文值，母语键留待翻译）。
//!
//! 校验规则来自翻译实践（见记忆 ff7678c2）：
//! 1. 关键字避让：crates 键与 keywords.toml 键相撞时，关键字先替换，crates 键永不生效 → error
//! 2. crates 文件之间同键不同值：read_dir 顺序未定义，合并非确定 → error
//! 3. crates 键与 stdlib 标识符同键不同值：stdlib 最后加载优先，crates 键被覆盖失效 → warning

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// “母语词 → 英文”映射表
type 词条映射表 = HashMap<String, String>;

/// 语言包映射数据的统一视图（内置数据与外部目录两种来源归一化）
pub struct 语言包视图 {
    /// 语言代码（如 zh / ru）
    pub 语言代码: String,
    /// keywords.toml 内容
    pub 关键字内容: String,
    /// stdlib.toml 内容
    pub 标准库内容: String,
    /// errors.toml 内容（错误码/消息翻译）
    pub 错误内容: String,
    /// 第三方库映射（文件名, TOML 内容），按文件名排序保证确定性
    pub 三方库文件: Vec<(String, String)>,
}

impl 语言包视图 {
    /// 从内置语言包数据构造视图
    pub fn 自内置构造(语言代码: &str) -> Self {
        let 内置数据 = crate::内置语言::获取内置数据(语言代码);
        let mut 三方库文件: Vec<(String, String)> = 内置数据
            .三方库数据
            .iter()
            .map(|(基名, 内容)| (基名.to_string(), 内容.to_string()))
            .collect();
        三方库文件.sort_by(|甲, 乙| 甲.0.cmp(&乙.0));
        语言包视图 {
            语言代码: 语言代码.to_string(),
            关键字内容: 内置数据.关键字文本.to_string(),
            标准库内容: 内置数据.标准库文本.to_string(),
            错误内容: 内置数据.错误文本.to_string(),
            三方库文件,
        }
    }

    /// 从外部语言包目录构造视图（读取 keywords.toml / stdlib.toml / crates/*.toml）
    pub fn 自目录构造(目录: &Path) -> anyhow::Result<Self> {
        let 语言代码 = 目录
            .file_name()
            .and_then(|原始名| 原始名.to_str())
            .unwrap_or("unknown")
            .to_string();
        let 关键字内容 = std::fs::read_to_string(目录.join("keywords.toml"))
            .map_err(|错| anyhow::anyhow!("读取 keywords.toml 失败: {错}"))?;
        // stdlib.toml 可选（部分语言包可能未提供）
        let 标准库内容 = std::fs::read_to_string(目录.join("stdlib.toml")).unwrap_or_default();
        // errors.toml 可选（en 等母语即英文的语言无此文件）
        let 错误内容 = std::fs::read_to_string(目录.join("errors.toml")).unwrap_or_default();
        let mut 三方库文件 = Vec::new();
        let 库目录 = 目录.join("crates");
        if 库目录.is_dir() {
            let mut 配置清单: Vec<PathBuf> = std::fs::read_dir(&库目录)
                .map_err(|错| anyhow::anyhow!("读取 crates 目录失败: {错}"))?
                .filter_map(|条目| 条目.ok())
                .map(|条目| 条目.path())
                .filter(|每路径| 每路径.extension().and_then(|后缀| 后缀.to_str()) == Some("toml"))
                .collect();
            配置清单.sort();
            for 库路径 in 配置清单 {
                let 库文件基名 = 库路径
                    .file_name()
                    .and_then(|原始名| 原始名.to_str())
                    .unwrap_or("unknown.toml")
                    .to_string();
                let 内容文本 = std::fs::read_to_string(&库路径)
                    .map_err(|错| anyhow::anyhow!("读取 {} 失败: {错}", 库路径.display()))?;
                三方库文件.push((库文件基名, 内容文本));
            }
        }
        Ok(语言包视图 {
            语言代码,
            关键字内容,
            标准库内容,
            错误内容,
            三方库文件,
        })
    }
}

/// 校验统计信息
#[derive(Debug, Default, Clone)]
pub struct 校验统计 {
    /// crates 文件数
    pub 库文件数: usize,
    /// 模块路径节条目总数
    pub 模块路径条目数: usize,
    /// 标识符节条目总数
    pub 标识符条目数: usize,
}

/// 校验报告
#[derive(Debug, Default)]
pub struct 校验报告 {
    /// 必须修复的错误
    pub 错误清单: Vec<String>,
    /// 建议修复的警告
    pub 警告清单: Vec<String>,
    /// 统计信息
    pub 统计项: 校验统计,
}

impl 校验报告 {
    /// 是否通过（无错误；警告不阻断）
    pub fn 是否通过(&self) -> bool {
        self.错误清单.is_empty()
    }
}

/// 提取 TOML 中 `["模块路径"]` 与 `["标识符"]` 两节的键值对
///
/// 返回 (模块路径表, 标识符表)。TOML 解析失败（含重复键）时返回 Err。
fn 提取双节(内容文本: &str) -> Result<(词条映射表, 词条映射表), String> {
    let 解析值: toml::Value = toml::from_str(内容文本).map_err(|错| 错.to_string())?;
    let mut 模块路径表 = HashMap::new();
    let mut 标识符表 = HashMap::new();
    if let toml::Value::Table(表内容) = 解析值 {
        if let Some(toml::Value::Table(模块路径子表)) = 表内容.get("模块路径") {
            for (母语键, 值内容) in 模块路径子表 {
                if let toml::Value::String(文本值) = 值内容 {
                    模块路径表.insert(母语键.clone(), 文本值.clone());
                }
            }
        }
        if let Some(toml::Value::Table(标识符子表)) = 表内容.get("标识符") {
            for (母语键, 值内容) in 标识符子表 {
                if let toml::Value::String(文本值) = 值内容 {
                    标识符表.insert(母语键.clone(), 文本值.clone());
                }
            }
        }
    }
    Ok((模块路径表, 标识符表))
}

/// 提取 keywords.toml 中所有节的“母语词 → 英文”映射
///
/// 复用引擎权威语义（摊平节表：按节名升序合并，后到覆盖），
/// 与运行时实际生效的关键字映射保持一致，避免校验误报。
/// `["派生特征"]` 节仅在 `#[派生(...)]` 属性内生效（不并入关键字表），
/// 此处同步排除，否则与 stdlib/crates 的同键异值（如 `调试`: Debug/debug）
/// 会被误报为冲突。
///
/// 用于关键字避让检测：需比较 crates 键与关键字的**值**是否一致，
/// 同值视为安全冗余（关键字替换与 crates 替换结果相同），不同值才是真冲突。
fn 提取关键字映射(内容文本: &str) -> HashMap<String, String> {
    match i18n_rust_engine::映射源::解析配置节(内容文本) {
        Ok(节表) => {
            let mut 映射表 = i18n_rust_engine::映射源::摊平节表(&节表);
            if let Some(派生节) = 节表.get("派生特征") {
                for 母语键 in 派生节.keys() {
                    映射表.remove(母语键);
                }
            }
            映射表
        }
        Err(_) => HashMap::new(),
    }
}

/// keywords.toml 必需节（引擎按节名取表，缺节不会报错，只会让整类词条静默失效）
///
/// - `宏`：宏名 → 英文宏名，供宏调用自动补 `!`。缺失则 `取宏映射表()`
///   为空、宏调用不再补 `!`，产物非法却仍报「转译成功」，最难排查；
/// - `派生特征`：`#[派生(...)]` 参数。缺失则派生参数不替换；
/// - 其余节经 `摊平节表` 合并进关键字表，缺节即整类方言词不可用。
const 必需关键字节: &[&str] = &[
    "声明",
    "控制流",
    "类型",
    "逻辑值",
    "特殊值",
    "内存",
    "不安全",
    "错误处理",
    "宏",
    "派生特征",
    "标准库成员",
];

/// 校验单个语言包的 crates 映射质量
///
/// 检查项（按严重级别）：
/// - error：TOML 解析失败（含重复键）
/// - error：stdlib.toml 解析失败（重复键等——stdlib 整体加载失败，
///   运行时标准库映射全部丢失，此前静默吞掉导致 de/pt 重复键长期潜伏）
/// - error：crates 键与 keywords.toml 键相撞（关键字先替换，crates 键永不生效）
/// - error：crates 文件之间模块路径/标识符同键不同值（合并非确定）
/// - warning：crates 标识符键与 stdlib 标识符同键不同值（stdlib 优先，crates 键失效）
///
/// 设计约定（不检查）：同一文件内 `["模块路径"]` 与 `["标识符"]`
/// 双节同键不同值是合法模式（模块路径=crate 名小写如 `rocket`，
/// 标识符=类型名 `Rocket`，两表在词法/语义阶段独立查询，互不干扰），
/// 不视为冲突；同键同值则为安全冗余。
pub fn 校验语言包(视图: &语言包视图) -> 校验报告 {
    let mut 检查报告 = 校验报告::default();
    let 关键词映射表 = 提取关键字映射(&视图.关键字内容);

    // 0. keywords.toml 可解析性与必需节完整性。
    // 解析失败（如重复键）会让关键字表整体失效（全部方言词不可用）；
    // 缺节不会报错——引擎按节名取表，缺节返回空表，整类词条静默失效。
    // 其中缺 `["宏"]` 最危险：宏调用不再自动补 `!`，产物非法却仍报「转译成功」。
    match toml::from_str::<toml::Table>(&视图.关键字内容) {
        Ok(关键字表) => {
            for 节 in 必需关键字节 {
                if !关键字表.contains_key(*节) {
                    检查报告
                        .错误清单
                        .push(format!("mc_missing_section|{}|{节}", 视图.语言代码));
                }
            }
        }
        Err(错) => 检查报告
            .错误清单
            .push(format!("mc_parse_failed|keywords.toml|{错}")),
    }

    // stdlib 解析失败必须报错（重复键等会让 stdlib 整体加载失败）；
    // 空内容（无 stdlib 文件）解析为空表，不会误报
    let (_, 标准库标识符) = match 提取双节(&视图.标准库内容) {
        Ok(节对) => 节对,
        Err(错) => {
            检查报告
                .错误清单
                .push(format!("mc_stdlib_parse_failed|{错}"));
            (HashMap::new(), HashMap::new())
        }
    };

    // 跨文件键追踪（模块路径与标识符分别维护）：键 -> (值, 首次出现的文件)
    let mut 已见模块路径: HashMap<String, (String, String)> = HashMap::new();
    let mut 已见标识符: HashMap<String, (String, String)> = HashMap::new();

    for (库文件基名, 内容文本) in &视图.三方库文件 {
        检查报告.统计项.库文件数 += 1;
        // 1. TOML 解析（重复键 / 格式错误在此暴露）
        let (模块路径表, 标识符表) = match 提取双节(内容文本) {
            Ok(节对) => 节对,
            Err(错) => {
                检查报告
                    .错误清单
                    .push(format!("mc_parse_failed|{库文件基名}|{错}"));
                continue;
            }
        };
        检查报告.统计项.模块路径条目数 += 模块路径表.len();
        检查报告.统计项.标识符条目数 += 标识符表.len();

        // 2. 关键字避让（对模块路径节与标识符节的键都检查）
        // 3. 跨文件同键不同值（两节对称：合并结果由文件名排序决定，
        //    同键不同值意味着覆盖语义取决于排序，属确定性歧义）
        for 节名 in ["模块路径", "标识符"] {
            let (条目集, 已见, 错误码模板) = if 节名 == "模块路径" {
                (&模块路径表, &mut 已见模块路径, "mc_cross_conflict_mp")
            } else {
                (&标识符表, &mut 已见标识符, "mc_cross_conflict")
            };
            for (母语键, 英文值) in 条目集 {
                // 关键字避让：仅当 crates 键与关键字**值不同**时报错；
                // 同值时关键字替换与 crates 替换结果一致，属安全冗余不报错
                if 关键词映射表
                    .get(母语键)
                    .is_some_and(|关键字值| 关键字值 != 英文值)
                {
                    检查报告.错误清单.push(format!(
                        "mc_keyword_collision|{库文件基名}|{节名}|{母语键}|{英文值}|{}",
                        关键词映射表[母语键]
                    ));
                }
                // 跨文件同键不同值
                if let Some((已有值, 已有文件)) = 已见.get(母语键) {
                    if 已有值 != 英文值 {
                        检查报告.错误清单.push(format!(
                            "{错误码模板}|{母语键}|{已有文件}|{已有值}|{库文件基名}|{英文值}"
                        ));
                    }
                } else {
                    已见.insert(母语键.clone(), (英文值.clone(), 库文件基名.clone()));
                }
                // stdlib 覆盖检测（标识符节；stdlib 后加载优先）
                if 节名 == "标识符"
                    && 标准库标识符
                        .get(母语键)
                        .is_some_and(|标准库值| 标准库值 != 英文值)
                {
                    检查报告.警告清单.push(format!(
                        "mc_stdlib_shadow|{库文件基名}|{母语键}|{英文值}|{}",
                        标准库标识符[母语键]
                    ));
                }
            }
        }
    }
    检查报告
}

/// 从源语言 crates 生成目标语言的翻译骨架
///
/// 保留 TOML 结构与英文值，在每个含母语键的行后追加 TODO 注释提示翻译。
/// 返回生成的文件数。目标目录不存在时自动创建。
pub fn 生成骨架(
    源视图: &语言包视图,
    目标语言代码: &str,
    产出目录: &Path,
) -> anyhow::Result<(usize, usize)> {
    std::fs::create_dir_all(产出目录)
        .map_err(|错| anyhow::anyhow!("创建目录 {} 失败: {错}", 产出目录.display()))?;
    let mut 已新建数 = 0;
    let mut 已跳过数 = 0;
    for (库文件基名, 内容文本) in &源视图.三方库文件 {
        let 目标路径 = 产出目录.join(库文件基名);
        // 已存在的文件跳过：保留既有翻译成果，保证重跑幂等
        if 目标路径.exists() {
            已跳过数 += 1;
            continue;
        }
        let 骨架文本 = 追加待译注释(内容文本, &源视图.语言代码, 目标语言代码);
        std::fs::write(&目标路径, 骨架文本)
            .map_err(|错| anyhow::anyhow!("写入 {} 失败: {错}", 目标路径.display()))?;
        已新建数 += 1;
    }
    Ok((已新建数, 已跳过数))
}

/// 为 TOML 内容中每个 `母语键 = 英文值` 行追加 TODO 翻译注释
///
/// 仅处理 `["模块路径"]` 与 `["标识符"]` 节内的键值行；节标题、注释、空行原样保留。
fn 追加待译注释(
    内容文本: &str, 源语言代码: &str, 目标语言代码: &str
) -> String {
    let mut 输出文本 = String::new();
    let mut 处于目标节 = false;
    for 每行 in 内容文本.lines() {
        let 去空格 = 每行.trim();
        // 节标题检测：进入/离开目标节
        if 去空格.starts_with('[') {
            处于目标节 = 去空格 == "[\"模块路径\"]" || 去空格 == "[\"标识符\"]";
            输出文本.push_str(每行);
            输出文本.push('\n');
            continue;
        }
        // 目标节内的键值行（含 `=` 且非注释）追加 TODO
        if 处于目标节 && 去空格.contains('=') && !去空格.starts_with('#') {
            // 已有行尾注释时不重复追加
            if !去空格.contains("TODO") {
                输出文本.push_str(每行);
                输出文本.push_str(&format!(
                    "  # TODO({目标语言代码}): 将键从 {源语言代码} 翻译"
                ));
                输出文本.push('\n');
                continue;
            }
        }
        输出文本.push_str(每行);
        输出文本.push('\n');
    }
    输出文本
}

/// 打印单个语言包的校验报告（本地化输出）
///
/// 报告条目格式为 `code|arg1|arg2...`，按 code 查 ui.toml 模板渲染。
fn 打印报告(视图语言: &str, 检查报告: &校验报告) {
    let 界面 = crate::本地化::界面::全局();
    println!("{}", 界面.取文带参("mc_header", &[视图语言]));
    println!(
        "{}",
        界面.取文带参(
            "mc_stats",
            &[
                &检查报告.统计项.库文件数.to_string(),
                &检查报告.统计项.模块路径条目数.to_string(),
                &检查报告.统计项.标识符条目数.to_string()
            ]
        )
    );
    for 错误项 in &检查报告.错误清单 {
        println!("{}", 渲染问题(错误项));
    }
    for 警告项 in &检查报告.警告清单 {
        println!("{}", 渲染问题(警告项));
    }
    if 检查报告.是否通过() {
        println!(
            "{}",
            if 检查报告.警告清单.is_empty() {
                界面.取文("mc_ok")
            } else {
                界面.取文带参(
                    "mc_ok_with_warnings",
                    &[&检查报告.警告清单.len().to_string()],
                )
            }
        );
    } else {
        println!(
            "{}",
            界面.取文带参(
                "mc_failed",
                &[
                    &检查报告.错误清单.len().to_string(),
                    &检查报告.警告清单.len().to_string()
                ]
            )
        );
    }
}

/// 将 `code|arg1|arg2...` 格式的问题条目渲染为本地化消息
fn 渲染问题(问题串: &str) -> String {
    let 界面 = crate::本地化::界面::全局();
    let mut 拆分片段 = 问题串.split('|');
    let 问题码 = 拆分片段.next().unwrap_or(问题串);
    let 参数串: Vec<&str> = 拆分片段.collect();
    界面.取文带参(问题码, &参数串)
}

/// `rzc mapping check` 入口
///
/// - 目标 为 None：校验全部内置语言并输出跨语言条目数一致性对比
/// - 目标 为已存在的目录路径：按外部语言包目录校验
/// - 否则按内置语言代码校验
pub fn 运行校验(目标参数: Option<&str>) -> anyhow::Result<bool> {
    let 界面 = crate::本地化::界面::全局();
    let Some(目标) = 目标参数 else {
        // 全部内置语言 + 跨语言一致性对比 + 跨语言完整性门禁
        let mut 全部通过 = true;
        for 语言代码 in crate::内置语言::内置语言代码() {
            let 视图 = 语言包视图::自内置构造(语言代码);
            let 检查报告 = 校验语言包(&视图);
            全部通过 = 全部通过 && 检查报告.是否通过();
            打印报告(语言代码, &检查报告);
        }
        打印跨语言计数();
        全部通过 = 全部通过 && 打印跨语言完整性();
        return Ok(全部通过);
    };
    let 路径 = Path::new(目标);
    if 路径.is_dir() {
        let 视图 = 语言包视图::自目录构造(路径)?;
        let 检查报告 = 校验语言包(&视图);
        let 通过 = 检查报告.是否通过();
        打印报告(&视图.语言代码, &检查报告);
        Ok(通过)
    } else if crate::内置语言::拥有内置语言(目标) {
        let 视图 = 语言包视图::自内置构造(目标);
        let 检查报告 = 校验语言包(&视图);
        let 通过 = 检查报告.是否通过();
        打印报告(目标, &检查报告);
        Ok(通过)
    } else {
        anyhow::bail!(界面.取文带参("mc_unknown_target", &[目标]));
    }
}

/// 跨语言完整性检查报告
#[derive(Debug, Default)]
pub struct 跨语言报告 {
    /// 必须修复的错误（方言词缺失，功能缺口）
    pub 错误清单: Vec<String>,
    /// 建议补齐的警告（有回退机制，不阻断）
    pub 警告清单: Vec<String>,
}

impl 跨语言报告 {
    /// 是否通过（无错误；警告不阻断）
    pub fn 是否通过(&self) -> bool {
        self.错误清单.is_empty()
    }
}

/// 提取 keywords.toml 的扁平化关键字值集合与派生特征值集合
///
/// 与引擎运行时语义一致（摊平节表 + 派生特征节单独存放），
/// 按**英文值**对齐而非母语键：各语言母语词不同（ja 用日语、de 用德语），
/// 只有英文值是跨语言可比的"方言能力"。
fn 关键字值集合(内容文本: &str) -> (HashSet<String>, HashSet<String>) {
    match i18n_rust_engine::映射源::解析配置节(内容文本) {
        Ok(节表) => {
            let mut 关键字集 = HashSet::new();
            let mut 派生集 = HashSet::new();
            for (节名, 子表) in &节表 {
                let 目标集 = if 节名 == "派生特征" {
                    &mut 派生集
                } else {
                    &mut 关键字集
                };
                for 值内容 in 子表.values() {
                    目标集.insert(值内容.clone());
                }
            }
            (关键字集, 派生集)
        }
        Err(_) => (HashSet::new(), HashSet::new()),
    }
}

/// 提取 errors.toml 的错误码节集合（如 E0425）与消息翻译键集合
fn 错误组成(内容文本: &str) -> (HashSet<String>, HashSet<String>) {
    let Ok(解析值) = 内容文本.parse::<toml::Value>() else {
        return (HashSet::new(), HashSet::new());
    };
    let Some(表内容) = 解析值.as_table() else {
        return (HashSet::new(), HashSet::new());
    };
    let mut 错误码集 = HashSet::new();
    let mut 消息键集 = HashSet::new();
    for (母语键, 值内容) in 表内容 {
        if 母语键.starts_with('E')
            && 母语键.len() == 5
            && 母语键[1..].chars().all(|每字符| 每字符.is_ascii_digit())
        {
            错误码集.insert(母语键.clone());
        } else if 母语键 == "消息翻译"
            && let toml::Value::Table(消息子表) = 值内容
        {
            for 消息键 in 消息子表.keys() {
                消息键集.insert(消息键.clone());
            }
        }
    }
    (错误码集, 消息键集)
}

/// 单对基准/目标语言包的跨语言完整性检查
///
/// 规则（基准 = 语言包维护语言，当前为 zh）：
/// - error：目标语言缺少基准语言拥有的关键字英文值（方言词不可用，功能缺口）
/// - error：目标语言缺少派生特征值（`#[派生(...)]` 属性参数不可用）
/// - error：目标语言缺少基准语言的错误码节（该错误码无母语教学提示）
/// - warning：目标语言缺少消息翻译键（无错误码时回退英文原文，可工作）
pub fn 校验跨语言对(基准: &语言包视图, 目标: &语言包视图) -> 跨语言报告 {
    let mut 检查报告 = 跨语言报告::default();
    let (基准关键字, 基准派生) = 关键字值集合(&基准.关键字内容);
    let (目标关键字, 目标派生) = 关键字值集合(&目标.关键字内容);
    let (基准错误码, 基准消息) = 错误组成(&基准.错误内容);
    let (目标错误码, 目标消息) = 错误组成(&目标.错误内容);

    // 关键字值缺失：合并输出，避免每个词一条噪音
    let mut 缺失关键字: Vec<String> = 基准关键字.difference(&目标关键字).cloned().collect();
    缺失关键字.sort();
    if !缺失关键字.is_empty() {
        检查报告.错误清单.push(format!(
            "mc_cross_kw_missing|{}|{}|{}",
            目标.语言代码,
            缺失关键字.len(),
            缺失关键字.join("/")
        ));
    }
    // 派生特征值缺失（语法上在 keywords.toml 的 [派生特征] 节）
    let mut 缺失派生: Vec<String> = 基准派生.difference(&目标派生).cloned().collect();
    缺失派生.sort();
    if !缺失派生.is_empty() {
        检查报告.错误清单.push(format!(
            "mc_cross_derive_missing|{}|{}|{}",
            目标.语言代码,
            缺失派生.len(),
            缺失派生.join("/")
        ));
    }
    // 错误码节缺失
    let mut 缺失错误码: Vec<String> = 基准错误码.difference(&目标错误码).cloned().collect();
    缺失错误码.sort();
    if !缺失错误码.is_empty() {
        检查报告.错误清单.push(format!(
            "mc_cross_err_missing|{}|{}|{}",
            目标.语言代码,
            缺失错误码.len(),
            缺失错误码.join("/")
        ));
    }
    // 消息翻译键缺失（warning：无错误码匹配时回退英文原文）
    let mut 缺失消息: Vec<String> = 基准消息.difference(&目标消息).cloned().collect();
    缺失消息.sort();
    if !缺失消息.is_empty() {
        检查报告.警告清单.push(format!(
            "mc_cross_msg_missing|{}|{}|{}",
            目标.语言代码,
            缺失消息.len(),
            缺失消息
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(" / ")
        ));
    }
    检查报告
}

/// 打印跨内置语言的完整性检查（以 zh 为基准，错误阻断）
///
/// en 等母语即英文的语言包无 keywords/errors 文件，跳过不参与比较。
/// 返回是否全部通过（供 运行校验 汇总退出码）。
pub fn 打印跨语言完整性() -> bool {
    let 界面 = crate::本地化::界面::全局();
    println!("{}", 界面.取文("mc_cross_integrity_header"));
    let 基准 = 语言包视图::自内置构造("zh");
    let mut 全部通过 = true;
    let mut 已检查任 = false;
    for 语言代码 in crate::内置语言::内置语言代码() {
        if 语言代码 == "zh" {
            continue;
        }
        let 视图 = 语言包视图::自内置构造(语言代码);
        // 无 keywords/errors 数据（母语即英文）的语言不参与比较
        if 视图.关键字内容.is_empty() && 视图.错误内容.is_empty() {
            continue;
        }
        已检查任 = true;
        let 检查报告 = 校验跨语言对(&基准, &视图);
        全部通过 = 全部通过 && 检查报告.是否通过();
        for 错误项 in &检查报告.错误清单 {
            println!("  {}", 渲染问题(错误项));
        }
        for 警告项 in &检查报告.警告清单 {
            println!("  {}", 渲染问题(警告项));
        }
        if 检查报告.是否通过() && 检查报告.警告清单.is_empty() {
            println!("  {}: {}", 语言代码, 界面.取文("mc_cross_integrity_ok"));
        } else if 检查报告.是否通过() {
            println!(
                "  {}: {}",
                语言代码,
                界面.取文带参(
                    "mc_cross_integrity_warn",
                    &[&检查报告.警告清单.len().to_string()]
                )
            );
        }
    }
    if !已检查任 {
        println!("  {}", 界面.取文("mc_cross_integrity_none"));
    }
    全部通过
}

fn 打印跨语言计数() {
    let 界面 = crate::本地化::界面::全局();
    let 语言列表 = crate::内置语言::内置语言代码();
    let mut 计数列表: Vec<(String, usize, usize)> = Vec::new();
    for 语言代码 in 语言列表 {
        let 视图 = 语言包视图::自内置构造(语言代码);
        let 检查报告 = 校验语言包(&视图);
        计数列表.push((
            语言代码.to_string(),
            检查报告.统计项.库文件数,
            检查报告.统计项.标识符条目数,
        ));
    }
    计数列表.sort_by(|甲, 乙| 甲.0.cmp(&乙.0));
    println!("{}", 界面.取文("mc_cross_lang_header"));
    for (语言代码, _, 条目数) in &计数列表 {
        println!("  {}: {}", 语言代码, 条目数);
    }
    // 仅对有 crates 文件的语言比较条目数
    let 含库条目: Vec<usize> = 计数列表
        .iter()
        .filter(|(_, 库数, _)| *库数 > 0)
        .map(|(_, _, 条目数)| *条目数)
        .collect();
    let 首个数 = 含库条目.first().copied();
    let 不一致 = 含库条目.iter().any(|条目数| Some(*条目数) != 首个数);
    if 不一致 {
        println!("{}", 界面.取文("mc_cross_lang_inconsistent"));
    } else {
        println!("{}", 界面.取文("mc_cross_lang_consistent"));
    }
}

/// `rzc mapping scaffold` 入口
///
/// source 必须是内置语言代码；output 为 None 时默认写入
/// 项目语言包根 `<target>/crates/`（主仓库内为 crates/engine/lang-packs/，
/// 用户项目为 lang-packs/）。
/// provider：`rule`（默认，生成 TODO 骨架待人工翻译）或
/// `deepseek`（AI 自动翻译键名，需 DEEPSEEK_API_KEY）。
pub fn 运行脚手架(
    源语言代码: &str,
    目标语言代码: &str,
    产出: Option<&Path>,
    提供方: &str,
) -> anyhow::Result<()> {
    let 界面 = crate::本地化::界面::全局();
    if !crate::内置语言::拥有内置语言(源语言代码) {
        anyhow::bail!(界面.取文带参("mc_unknown_source", &[源语言代码]));
    }
    let 视图 = 语言包视图::自内置构造(源语言代码);
    if 视图.三方库文件.is_empty() {
        anyhow::bail!(界面.取文带参("mc_no_crates", &[源语言代码]));
    }
    let 产出目录 = match 产出 {
        Some(目录) => 目录.to_path_buf(),
        None => {
            let 基础目录 = std::env::current_dir()
                .ok()
                .and_then(|工作目录| crate::向上定位项目根(&工作目录))
                .unwrap_or_else(|| PathBuf::from("."));
            crate::语言包根目录(&基础目录).join(format!("{}/crates", 目标语言代码))
        }
    };
    let (已新建数, 已跳过数) = 生成骨架(&视图, 目标语言代码, &产出目录)?;
    if 已新建数 > 0 {
        println!(
            "{}",
            界面.取文带参(
                "mc_scaffold_generated",
                &[&已新建数.to_string(), &产出目录.display().to_string()]
            )
        );
    }
    if 已跳过数 > 0 {
        println!(
            "{}",
            界面.取文带参("mc_scaffold_skip_existing", &[&已跳过数.to_string()])
        );
    }
    if 提供方 == "deepseek" {
        翻译骨架(&视图, 源语言代码, 目标语言代码, &产出目录)?;
    } else {
        println!("{}", 界面.取文带参("mc_scaffold_hint", &[目标语言代码]));
    }
    Ok(())
}

/// 每批送 AI 的键数上限
const 批量大小: usize = 60;
/// 冲突改名最大重试轮数
const 最大重试轮数: usize = 2;

/// AI 翻译脚手架：提取 TODO 键 → 批量翻译 → 写回 → 校验冲突 → 改名重试
fn 翻译骨架(
    源视图: &语言包视图,
    源语言代码: &str,
    目标语言代码: &str,
    产出目录: &Path,
) -> anyhow::Result<()> {
    let 界面 = crate::本地化::界面::全局();
    // 禁用词：源包 keywords 键（译名撞关键字会永不生效）
    let 禁用词: Vec<String> = 提取关键字映射(&源视图.关键字内容).keys().cloned().collect();

    let 待译项 = 收集待译键(产出目录, 目标语言代码)?;
    if 待译项.is_empty() {
        return Ok(());
    }
    // 按 (键, 英文值) 去重，保证跨文件同键翻译一致
    let mut 去重键表: Vec<(String, String)> = Vec::new();
    for (_, 母语键, 英文值) in &待译项 {
        if !去重键表.iter().any(|(甲, 乙)| 甲 == 母语键 && 乙 == 英文值) {
            去重键表.push((母语键.clone(), 英文值.clone()));
        }
    }

    // 批量翻译
    let mut 翻译表: HashMap<String, String> = HashMap::new();
    let 批次总数 = 去重键表.len().div_ceil(批量大小);
    for (序号, 每批) in 去重键表.chunks(批量大小).enumerate() {
        println!(
            "{}",
            界面.取文带参(
                "mc_scaffold_ai_batch",
                &[&(序号 + 1).to_string(), &批次总数.to_string()]
            )
        );
        let 已用词: Vec<String> = 翻译表.values().cloned().collect();
        let 批次映射 = 翻译键名(源语言代码, 目标语言代码, 每批, &禁用词, &已用词)?;
        翻译表.extend(批次映射);
    }
    let 已替换数 = 应用翻译(产出目录, &翻译表, 目标语言代码)?;
    println!(
        "{}",
        界面.取文带参(
            "mc_scaffold_ai_done",
            &[&已替换数.to_string(), &待译项.len().to_string()]
        )
    );
    if 已替换数 < 待译项.len() {
        println!(
            "{}",
            界面.取文带参(
                "mc_scaffold_ai_partial",
                &[&(待译项.len() - 已替换数).to_string()]
            )
        );
    }

    // 校验回环：对生成的 crates 目录重新校验，冲突键送 AI 改名重试
    for 轮次 in 1..=最大重试轮数 {
        let 检查报告 = 校验语言包(&视图自库目录(产出目录, 源视图));
        let 冲突项集 = 收集冲突项(&检查报告, 产出目录);
        if 冲突项集.is_empty() {
            if !检查报告.警告清单.is_empty() {
                println!(
                    "{}",
                    界面.取文带参(
                        "mc_ok_with_warnings",
                        &[&检查报告.警告清单.len().to_string()]
                    )
                );
            } else {
                println!("{}", 界面.取文("mc_ok"));
            }
            return Ok(());
        }
        println!(
            "{}",
            界面.取文带参(
                "mc_scaffold_ai_retry",
                &[
                    &冲突项集.len().to_string(),
                    &轮次.to_string(),
                    &最大重试轮数.to_string()
                ]
            )
        );
        let 改名表 = 冲突改名(目标语言代码, &冲突项集, &禁用词)?;
        应用改名(产出目录, &改名表, 目标语言代码)?;
    }
    // 重试后仍可能有残留冲突：输出最终报告供人工处理
    let 视图 = 视图自库目录(产出目录, 源视图);
    let 检查报告 = 校验语言包(&视图);
    if !检查报告.是否通过() {
        打印报告(目标语言代码, &检查报告);
        println!(
            "{}",
            界面.取文带参("mc_scaffold_ai_retry_left", &[目标语言代码])
        );
    } else {
        println!("{}", 界面.取文("mc_ok"));
    }
    Ok(())
}

/// 扫描输出目录中含 `TODO(<target>)` 标记的行，返回 (文件名, 键, 英文值)
fn 收集待译键(
    产出目录: &Path,
    目标语言代码: &str,
) -> anyhow::Result<Vec<(String, String, String)>> {
    let mut 待译项 = Vec::new();
    let 待译标记 = format!("TODO({目标语言代码})");
    for 目录项 in 列出配置文件(产出目录)? {
        let 库文件基名 = 目录项
            .file_name()
            .and_then(|原始名| 原始名.to_str())
            .unwrap_or_default()
            .to_string();
        let 内容文本 = std::fs::read_to_string(&目录项)?;
        for 每行 in 内容文本.lines() {
            if !每行.contains(&待译标记) {
                continue;
            }
            if let Some((母语键, 英文值)) = 解析键值行(每行) {
                待译项.push((库文件基名.clone(), 母语键, 英文值));
            }
        }
    }
    Ok(待译项)
}

/// 列出目录下的 .toml 文件（按名排序保证确定性）
fn 列出配置文件(目录: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut 配置清单: Vec<PathBuf> = std::fs::read_dir(目录)?
        .filter_map(|每项| 每项.ok())
        .map(|每项| 每项.path())
        .filter(|每路径| 每路径.extension().and_then(|后缀| 后缀.to_str()) == Some("toml"))
        .collect();
    配置清单.sort();
    Ok(配置清单)
}

/// 解析 `"键" = "值" ...` 行，返回 (键, 值)
///
/// 键与值均支持转义引号（`\"`），避免在转义处错误截断。
fn 解析键值行(行号: &str) -> Option<(String, String)> {
    let 去空格 = 行号.trim();
    let mut 部分 = 去空格.splitn(2, '=');
    let 键侧 = 部分.next()?.trim();
    let 值侧 = 部分.next()?.trim();
    // 键与值共用同一转义感知解析（键侧此前只剥外层引号，
    // 转义引号会残留在键中导致后续查表静默失败）
    let 母语键 = 解析引号内(键侧)?;
    let 英文值 = 解析引号内(值侧)?;
    Some((母语键, 英文值))
}

/// 解析带转义的引号内容：剥离前缀引号后逐字符扫描，
/// 转义序列取下一字符原样保留，遇未转义引号即结束
fn 解析引号内(片段: &str) -> Option<String> {
    let 内部 = 片段.strip_prefix('"')?;
    let mut 输出文本 = String::new();
    let mut 字符流 = 内部.chars();
    while let Some(当前字符) = 字符流.next() {
        if 当前字符 == '\\' {
            // 转义序列：取下一个字符原样保留
            if let Some(下一字符) = 字符流.next() {
                输出文本.push(下一字符);
            }
        } else if 当前字符 == '"' {
            // 未转义的引号：内容结束
            break;
        } else {
            输出文本.push(当前字符);
        }
    }
    Some(输出文本)
}

/// 调用 AI 批量翻译键名，返回 源键→目标语言键
fn 翻译键名(
    源语言代码: &str,
    目标语言代码: &str,
    项名集: &[(String, String)],
    禁用词: &[String],
    已用词: &[String],
) -> anyhow::Result<HashMap<String, String>> {
    let 系统提示 = "You are a programming terminology translation engine for a \
Rust teaching dialect. Translate mapping keys from the source language into the \
target language. Rules:\n\
1. Output ONLY a JSON object {\"source_key\": \"translated_key\"}, no explanation.\n\
2. Translations must be natural programming terms in the target language.\n\
3. Never reuse words from the forbidden list (language keywords) or the used list.\n\
4. Different source keys must map to different translations.\n\
5. Keep the same style as existing pack keys (single word or short phrase).";
    let 键清单 = 项名集
        .iter()
        .map(|(甲, 乙)| format!("{} = {}", 甲, 乙))
        .collect::<Vec<_>>()
        .join("\n");
    let 用户提示 = format!(
        "source language: {}\ntarget language: {}\nforbidden words: {}\n\
used words: {}\nkeys to translate (key = its English API):\n{}",
        源语言代码,
        目标语言代码,
        禁用词.join(", "),
        已用词.join(", "),
        键清单
    );
    let 内容文本 = crate::映射生成器::深度求索对话(系统提示, &用户提示)?;
    解析映射(&内容文本)
}

/// 从校验报告提取冲突项 (文件名, 键)，供改名重试
fn 收集冲突项(检查报告: &校验报告, 产出目录: &Path) -> Vec<(String, String)> {
    let mut 项名集: Vec<(String, String)> = Vec::new();
    for 错误项 in &检查报告.错误清单 {
        let 片段: Vec<&str> = 错误项.split('|').collect();
        match 片段.first().copied() {
            Some("mc_keyword_collision") if 片段.len() >= 4 => {
                项名集.push((片段[1].to_string(), 片段[3].to_string()));
            }
            Some("mc_cross_conflict") if 片段.len() >= 6 => {
                let 冲突键 = 片段[1];
                项名集.push((片段[2].to_string(), 冲突键.to_string()));
                项名集.push((片段[4].to_string(), 冲突键.to_string()));
            }
            Some("mc_cross_conflict_mp") if 片段.len() >= 6 => {
                let 冲突键 = 片段[1];
                项名集.push((片段[2].to_string(), 冲突键.to_string()));
                项名集.push((片段[4].to_string(), 冲突键.to_string()));
            }
            _ => {}
        }
    }
    // 去重并确认文件仍存在
    项名集.retain(|(库文件, _)| 产出目录.join(库文件).is_file());
    项名集.sort();
    项名集.dedup();
    项名集
}

/// 调用 AI 为冲突键改名，返回 (文件名, 旧键)→新键
fn 冲突改名(
    目标语言代码: &str,
    项名集: &[(String, String)],
    禁用词: &[String],
) -> anyhow::Result<HashMap<(String, String), String>> {
    let 系统提示 = "You rename conflicting mapping keys of a Rust teaching \
dialect language pack. Rules:\n\
1. Output ONLY a JSON object {\"file|key\": \"new_key\"}, no explanation.\n\
2. New keys must be multi-word terms in the target language, avoiding the forbidden \
list and all other existing keys.\n\
3. Keep the technical meaning of the original key.";
    let 清单 = 项名集
        .iter()
        .map(|(甲, 乙)| format!("{}|{}", 甲, 乙))
        .collect::<Vec<_>>()
        .join("\n");
    let 用户提示 = format!(
        "target language: {}\nforbidden words: {}\n\
conflicting entries (file|key) to rename:\n{}",
        目标语言代码,
        禁用词.join(", "),
        清单
    );
    let 内容文本 = crate::映射生成器::深度求索对话(系统提示, &用户提示)?;
    let 原始映射 = 解析映射(&内容文本)?;
    let mut 改名表 = HashMap::new();
    for (复合键, 新键) in 原始映射 {
        if let Some((库文件, 母语键)) = 复合键.split_once('|') {
            改名表.insert((库文件.to_string(), 母语键.to_string()), 新键);
        }
    }
    Ok(改名表)
}

/// 解析 AI 返回文本中的 JSON 对象（容忍 ```json 围栏与前后杂讯）
fn 解析映射(相关文本: &str) -> anyhow::Result<HashMap<String, String>> {
    let 起始处 = 相关文本
        .find('{')
        .ok_or_else(|| anyhow::anyhow!("AI 返回中未找到 JSON 对象"))?;
    let 终点 = 相关文本[起始处..]
        .rfind('}')
        .map(|偏移量| 起始处 + 偏移量 + 1)
        .ok_or_else(|| anyhow::anyhow!("AI 返回的 JSON 不完整"))?;
    let 解析值: serde_json::Value = serde_json::from_str(&相关文本[起始处..终点])?;
    let mut 映射表 = HashMap::new();
    if let serde_json::Value::Object(对象) = 解析值 {
        for (母语键, 值内容) in 对象 {
            if let serde_json::Value::String(文本值) = 值内容
                && !母语键.is_empty()
                && !文本值.is_empty()
            {
                映射表.insert(母语键, 文本值);
            }
        }
    }
    Ok(映射表)
}

/// 将翻译结果写回：替换 TODO 行的键并移除 TODO 注释；未命中键保留待人工
fn 应用翻译(
    产出目录: &Path,
    翻译表: &HashMap<String, String>,
    目标语言代码: &str,
) -> anyhow::Result<usize> {
    let mut 已替换数 = 0;
    let 待译标记 = format!("TODO({目标语言代码})");
    for 目录项 in 列出配置文件(产出目录).unwrap_or_default() {
        let 内容文本 = match std::fs::read_to_string(&目录项) {
            Ok(内容) => 内容,
            Err(_) => continue,
        };
        let mut 输出文本 = String::new();
        for 每行 in 内容文本.lines() {
            if 每行.contains(&待译标记)
                && let Some((母语键, 英文值)) = 解析键值行(每行)
                && let Some(新键) = 翻译表.get(&母语键)
            {
                // 重写为无 TODO 的行（丢弃原行尾注释中的 TODO 部分）
                输出文本.push_str(&format!("\"{}\" = \"{}\"\n", 新键, 英文值));
                已替换数 += 1;
                continue;
            }
            输出文本.push_str(每行);
            输出文本.push('\n');
        }
        // 写回失败必须上抛（此前静默吞掉会让用户看到"成功"但文件未更新）
        std::fs::write(&目录项, 输出文本)
            .map_err(|错| anyhow::anyhow!("翻译结果写回失败: {}: {错}", 目录项.display()))?;
    }
    Ok(已替换数)
}

/// 按 (文件名, 旧键) 精确改名（冲突重试轮次用）
fn 应用改名(
    产出目录: &Path,
    改名表: &HashMap<(String, String), String>,
    目标语言代码: &str,
) -> anyhow::Result<()> {
    for 目录项 in 列出配置文件(产出目录).unwrap_or_default() {
        let 库文件基名 = 目录项
            .file_name()
            .and_then(|原始名| 原始名.to_str())
            .unwrap_or_default()
            .to_string();
        let 内容文本 = match std::fs::read_to_string(&目录项) {
            Ok(内容) => 内容,
            Err(_) => continue,
        };
        let mut 输出文本 = String::new();
        for 每行 in 内容文本.lines() {
            let mut 已写入 = false;
            if let Some((母语键, 英文值)) = 解析键值行(每行)
                && let Some(新键) = 改名表.get(&(库文件基名.clone(), 母语键.clone()))
            {
                let 后缀 = if 每行.contains(&format!("TODO({目标语言代码})")) {
                    String::new()
                } else {
                    // 保留原有行尾注释（非 TODO 部分）
                    每行
                        .split('#')
                        .nth(1)
                        .map(|注释| format!("  # {}", 注释.trim()))
                        .unwrap_or_default()
                };
                输出文本.push_str(&format!("\"{}\" = \"{}\"{}\n", 新键, 英文值, 后缀));
                已写入 = true;
            }
            if !已写入 {
                输出文本.push_str(每行);
                输出文本.push('\n');
            }
        }
        std::fs::write(&目录项, 输出文本)
            .map_err(|错| anyhow::anyhow!("改名结果写回失败: {}: {错}", 目录项.display()))?;
    }
    Ok(())
}

/// 从已生成的 crates 目录构造校验视图（keywords/stdlib 取自源包）
fn 视图自库目录(库目录: &Path, 源视图: &语言包视图) -> 语言包视图 {
    let mut 三方库文件 = Vec::new();
    for 目录项 in 列出配置文件(库目录).unwrap_or_default() {
        let 库文件基名 = 目录项
            .file_name()
            .and_then(|原始名| 原始名.to_str())
            .unwrap_or("unknown.toml")
            .to_string();
        if let Ok(内容文本) = std::fs::read_to_string(&目录项) {
            三方库文件.push((库文件基名, 内容文本));
        }
    }
    // 目标语言包目录（crates 的父目录）若已有 keywords/stdlib 则优先用它们
    let 语言目录 = 库目录.parent().map(|父目录| 父目录.to_path_buf());
    let 关键字内容 = 语言目录
        .as_ref()
        .map(|目录| std::fs::read_to_string(目录.join("keywords.toml")).unwrap_or_default())
        .filter(|每串| !每串.is_empty())
        .unwrap_or_else(|| 源视图.关键字内容.clone());
    let 标准库内容 = 语言目录
        .as_ref()
        .map(|目录| std::fs::read_to_string(目录.join("stdlib.toml")).unwrap_or_default())
        .filter(|每串| !每串.is_empty())
        .unwrap_or_else(|| 源视图.标准库内容.clone());
    let 错误内容 = 语言目录
        .as_ref()
        .map(|目录| std::fs::read_to_string(目录.join("errors.toml")).unwrap_or_default())
        .filter(|每串| !每串.is_empty())
        .unwrap_or_else(|| 源视图.错误内容.clone());
    语言包视图 {
        语言代码: 库目录
            .parent()
            .and_then(|父目录| 父目录.file_name())
            .and_then(|原始名| 原始名.to_str())
            .unwrap_or("unknown")
            .to_string(),
        关键字内容,
        标准库内容,
        错误内容,
        三方库文件,
    }
}

#[cfg(test)]
mod 单元测试 {
    use super::*;

    /// 构造最小视图用于测试
    fn 视图含库(三方库文件: Vec<(&str, &str)>) -> 语言包视图 {
        语言包视图 {
            语言代码: "zh".to_string(),
            // 含全部必需节：本组用例聚焦 crates 冲突检测，
            // 缺节会额外触发 mc_missing_section 干扰断言
            关键字内容: concat!(
                "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n",
                "[\"控制流\"]\n[\"类型\"]\n[\"逻辑值\"]\n[\"特殊值\"]\n[\"内存\"]\n",
                "[\"不安全\"]\n[\"错误处理\"]\n[\"宏\"]\n[\"派生特征\"]\n[\"标准库成员\"]\n",
            )
            .to_string(),
            标准库内容: "[\"标识符\"]\n\"字符串\" = \"String\"\n".to_string(),
            错误内容: String::new(),
            三方库文件: 三方库文件
                .into_iter()
                .map(|(基名, 内容)| (基名.to_string(), 内容.to_string()))
                .collect(),
        }
    }

    /// 正常映射通过校验
    #[test]
    fn 测试干净映射通过() {
        let 视图 = 视图含库(vec![(
            "a.toml",
            "[\"标识符\"]\n\"服务器\" = \"Server\"\n\"路由\" = \"Router\"\n",
        )]);
        let 检查报告 = 校验语言包(&视图);
        assert!(
            检查报告.是否通过(),
            "干净映射应通过: {:?}",
            检查报告.错误清单
        );
        assert_eq!(检查报告.统计项.标识符条目数, 2);
        assert_eq!(检查报告.统计项.库文件数, 1);
    }

    /// 缺少必需节报 error（缺节不报错但整类词条静默失效）
    #[test]
    fn 测试缺少必需节被检出() {
        let 视图 = 语言包视图 {
            语言代码: "xx".to_string(),
            // 仅保留「声明」与「宏」，缺少其余必需节
            关键字内容: "[\"声明\"]\n\"函数\" = \"fn\"\n[\"宏\"]\n\"打印行\" = \"println\"\n"
                .to_string(),
            标准库内容: String::new(),
            错误内容: String::new(),
            三方库文件: Vec::new(),
        };
        let 检查报告 = 校验语言包(&视图);
        assert!(!检查报告.是否通过(), "缺必需节应校验失败");
        let 缺节: Vec<&String> = 检查报告
            .错误清单
            .iter()
            .filter(|每错| 每错.starts_with("mc_missing_section"))
            .collect();
        assert_eq!(缺节.len(), 必需关键字节.len() - 2, "应报出全部缺失节");
        assert!(
            缺节.iter().any(|每错| 每错.ends_with("|派生特征")),
            "缺 [\"派生特征\"] 应被检出: {缺节:?}"
        );
        // 含全部必需节的视图不应报缺节
        let 完整 = 视图含库(Vec::new());
        assert!(
            !校验语言包(&完整)
                .错误清单
                .iter()
                .any(|每错| 每错.starts_with("mc_missing_section")),
            "节齐全时不应报缺节"
        );
    }

    /// keywords.toml 解析失败（重复键）报 error：关键字表整体失效
    #[test]
    fn 测试关键字解析失败被检出() {
        let 视图 = 语言包视图 {
            语言代码: "xx".to_string(),
            // 同一节内重复键 → TOML 解析失败
            关键字内容: "[\"声明\"]\n\"函数\" = \"fn\"\n\"函数\" = \"fn2\"\n".to_string(),
            标准库内容: String::new(),
            错误内容: String::new(),
            三方库文件: Vec::new(),
        };
        let 检查报告 = 校验语言包(&视图);
        assert!(
            检查报告
                .错误清单
                .iter()
                .any(|每错| 每错.starts_with("mc_parse_failed|keywords.toml")),
            "keywords.toml 重复键应报解析失败: {:?}",
            检查报告.错误清单
        );
    }

    /// crates 键与 keywords 键相撞报 error
    #[test]
    fn 测试关键字冲突被检出() {
        let 视图 = 视图含库(vec![(
            "a.toml",
            "[\"标识符\"]\n\"函数\" = \"some_fn\"\n", // "函数" 是关键字
        )]);
        let 检查报告 = 校验语言包(&视图);
        assert!(!检查报告.是否通过());
        assert!(
            检查报告
                .错误清单
                .iter()
                .any(|每错| 每错.starts_with("mc_keyword_collision"))
        );
    }

    /// crates 文件之间同键不同值报 error
    #[test]
    fn 测试跨文件冲突被检出() {
        let 视图 = 视图含库(vec![
            ("a.toml", "[\"标识符\"]\n\"连接\" = \"connect_a\"\n"),
            ("b.toml", "[\"标识符\"]\n\"连接\" = \"connect_b\"\n"),
        ]);
        let 检查报告 = 校验语言包(&视图);
        assert!(!检查报告.是否通过());
        assert!(
            检查报告
                .错误清单
                .iter()
                .any(|每错| 每错.starts_with("mc_cross_conflict"))
        );
    }

    /// crates 文件之间同键同值不报错
    #[test]
    fn 测试跨文件同值通过() {
        let 视图 = 视图含库(vec![
            ("a.toml", "[\"标识符\"]\n\"连接\" = \"connect\"\n"),
            ("b.toml", "[\"标识符\"]\n\"连接\" = \"connect\"\n"),
        ]);
        let 检查报告 = 校验语言包(&视图);
        assert!(
            !检查报告
                .错误清单
                .iter()
                .any(|每错| 每错.starts_with("mc_cross_conflict")),
            "同键同值不应报冲突: {:?}",
            检查报告.错误清单
        );
    }

    /// crates 键与 stdlib 标识符同键不同值报 warning（不阻断）
    #[test]
    fn 测试标准库覆盖警告() {
        let 视图 = 视图含库(vec![(
            "a.toml",
            "[\"标识符\"]\n\"字符串\" = \"MyString\"\n", // stdlib 中 "字符串" = "String"
        )]);
        let 检查报告 = 校验语言包(&视图);
        assert!(检查报告.是否通过(), "stdlib 覆盖只是 warning 不阻断");
        assert!(
            检查报告
                .警告清单
                .iter()
                .any(|每警| 每警.starts_with("mc_stdlib_shadow"))
        );
    }

    /// 重复键导致 TOML 解析失败，报 error
    #[test]
    fn 测试重复键解析错误() {
        let 视图 = 视图含库(vec![(
            "a.toml",
            "[\"标识符\"]\n\"连接\" = \"connect\"\n\"连接\" = \"link\"\n",
        )]);
        let 检查报告 = 校验语言包(&视图);
        assert!(!检查报告.是否通过());
        assert!(
            检查报告
                .错误清单
                .iter()
                .any(|每错| 每错.starts_with("mc_parse_failed"))
        );
    }

    /// stdlib.toml 解析失败（重复键）报 error，不再静默吞掉
    #[test]
    fn 测试标准库解析失败被检出() {
        let 视图 = 语言包视图 {
            语言代码: "zh".to_string(),
            关键字内容: "[\"声明\"]\n\"函数\" = \"fn\"\n".to_string(),
            标准库内容: "[\"标识符\"]\n\"连接\" = \"link\"\n\"连接\" = \"net\"\n".to_string(),
            错误内容: String::new(),
            三方库文件: vec![(
                "a.toml".to_string(),
                "[\"标识符\"]\n\"服务器\" = \"Server\"\n".to_string(),
            )],
        };
        let 检查报告 = 校验语言包(&视图);
        assert!(
            检查报告
                .错误清单
                .iter()
                .any(|每错| 每错.starts_with("mc_stdlib_parse_failed")),
            "stdlib 解析失败应报 error: {:?}",
            检查报告.错误清单
        );
    }

    /// 无 stdlib 文件（空内容）不应误报解析失败
    #[test]
    fn 测试标准库空内容不误报() {
        let 视图 = 语言包视图 {
            语言代码: "zh".to_string(),
            关键字内容: "[\"声明\"]\n\"函数\" = \"fn\"\n".to_string(),
            标准库内容: String::new(),
            错误内容: String::new(),
            三方库文件: vec![(
                "a.toml".to_string(),
                "[\"标识符\"]\n\"服务器\" = \"Server\"\n".to_string(),
            )],
        };
        let 检查报告 = 校验语言包(&视图);
        assert!(
            !检查报告
                .错误清单
                .iter()
                .any(|每错| 每错.starts_with("mc_stdlib_parse_failed")),
            "空 stdlib 不应误报: {:?}",
            检查报告.错误清单
        );
    }

    /// 模块路径节跨文件同键不同值报 error（与标识符节对称）
    #[test]
    fn 测试模块路径跨文件冲突被检出() {
        let 视图 = 视图含库(vec![
            ("a.toml", "[\"模块路径\"]\n\"网络库\" = \"reqwest\"\n"),
            ("b.toml", "[\"模块路径\"]\n\"网络库\" = \"hyper\"\n"),
        ]);
        let 检查报告 = 校验语言包(&视图);
        assert!(
            检查报告
                .错误清单
                .iter()
                .any(|每错| 每错.starts_with("mc_cross_conflict_mp")),
            "模块路径跨文件冲突应报 error: {:?}",
            检查报告.错误清单
        );
    }

    /// 模块路径节跨文件同键同值不报错
    #[test]
    fn 测试模块路径跨文件同值通过() {
        let 视图 = 视图含库(vec![
            ("a.toml", "[\"模块路径\"]\n\"网络库\" = \"reqwest\"\n"),
            ("b.toml", "[\"模块路径\"]\n\"网络库\" = \"reqwest\"\n"),
        ]);
        let 检查报告 = 校验语言包(&视图);
        assert!(
            !检查报告
                .错误清单
                .iter()
                .any(|每错| 每错.starts_with("mc_cross_conflict_mp")),
            "模块路径同键同值不应报冲突: {:?}",
            检查报告.错误清单
        );
    }

    /// 同一文件内双节同键不同值（模块路径=crate 名、标识符=类型名）
    /// 是合法设计，不应报错
    #[test]
    fn 测试双节同键异值通过() {
        let 视图 = 视图含库(vec![(
            "a.toml",
            "[\"模块路径\"]\n\"火箭\" = \"rocket\"\n[\"标识符\"]\n\"火箭\" = \"Rocket\"\n",
        )]);
        let 检查报告 = 校验语言包(&视图);
        assert!(
            检查报告.是否通过(),
            "双节同键不同值（crate 名/类型名）应通过: {:?}",
            检查报告.错误清单
        );
    }

    /// scaffold 为键值行追加 TODO 注释，节标题与注释保留
    #[test]
    fn 测试骨架追加待译标记() {
        let 内容文本 = "[\"标识符\"]\n# 注释行\n\"服务器\" = \"Server\"\n";
        let 生成文本 = 追加待译注释(内容文本, "zh", "vi");
        assert!(生成文本.contains("TODO(vi)"));
        assert!(生成文本.contains("# 注释行"));
        assert!(生成文本.contains("\"服务器\" = \"Server\""));
    }

    /// 内置中文包自检：历史冲突已清理，校验应完全通过。
    #[test]
    fn 测试内置中文包干净通过() {
        let 视图 = 语言包视图::自内置构造("zh");
        let 检查报告 = 校验语言包(&视图);
        // 工具正常运行，统计数据合理（crate 数动态取内置清单，随语言包目录自动纳入）
        assert_eq!(
            检查报告.统计项.库文件数,
            crate::内置语言::获取内置数据("zh").三方库数据.len()
        );
        assert!(检查报告.统计项.标识符条目数 > 0);
        // 历史冲突（关键字避让/跨文件同键不同值）已清理完毕
        assert!(
            检查报告.是否通过(),
            "内置 zh 包应无错误: {:?}",
            检查报告.错误清单
        );
    }

    /// 各内置语言的标识符条目数均可统计
    #[test]
    fn 测试各语言条目数统计() {
        for 语言代码 in crate::内置语言::内置语言代码() {
            let 视图 = 语言包视图::自内置构造(语言代码);
            let 检查报告 = 校验语言包(&视图);
            assert!(检查报告.统计项.标识符条目数 > 0, "{语言代码} 应有条目");
        }
    }

    /// 全部内置语言均应通过校验（CI 门禁的测试层基线）
    #[test]
    fn 测试全部内置语言通过() {
        for 语言代码 in crate::内置语言::内置语言代码() {
            let 视图 = 语言包视图::自内置构造(语言代码);
            let 检查报告 = 校验语言包(&视图);
            assert!(
                检查报告.是否通过(),
                "内置包 {语言代码} 应通过校验: {:?}",
                检查报告.错误清单
            );
        }
    }

    /// 键值行解析：提取引号内键与值，忽略行尾注释
    #[test]
    fn 测试解析键值行() {
        let (母语键, 英文值) =
            解析键值行("\"服务器\" = \"Server\"  # TODO(vi): 将键从 zh 翻译").unwrap();
        assert_eq!(母语键, "服务器");
        assert_eq!(英文值, "Server");
        assert!(解析键值行("# 注释行").is_none());
        assert!(解析键值行("[\"标识符\"]").is_none());
    }

    /// 键与值两侧的转义引号都应正确还原（此前键侧不处理转义）
    #[test]
    fn 测试解析键值行转义引号() {
        let (母语键, 英文值) = 解析键值行("\"foo\\\"bar\" = \"va\\\"lue\"").unwrap();
        assert_eq!(母语键, "foo\"bar");
        assert_eq!(英文值, "va\"lue");
    }

    /// AI 返回 JSON 解析：容忍围栏与杂讯，丢弃空条目
    #[test]
    fn 测试解析映射() {
        let 样例文本 =
            "好的，以下是翻译结果：\n```json\n{\"服务器\": \"Máy chủ\", \"空\": \"\"}\n```\n";
        let 映射结果 = 解析映射(样例文本).unwrap();
        assert_eq!(映射结果.get("服务器").unwrap(), "Máy chủ");
        assert!(!映射结果.contains_key("空"), "空值条目应丢弃");
        assert!(解析映射("没有任何 JSON").is_err());
    }

    /// TODO 键提取 + 翻译写回：替换键并移除 TODO 标记，未命中键保留
    #[test]
    fn 测试收集并应用翻译() {
        let 测试目录 = tempfile::tempdir().unwrap();
        let 测试文件 = 测试目录.path().join("a.toml");
        std::fs::write(
            &测试文件,
            "[\"标识符\"]\n\"服务器\" = \"Server\"  # TODO(vi): 将键从 zh 翻译\n\
             \"路由\" = \"Router\"  # TODO(vi): 将键从 zh 翻译\n",
        )
        .unwrap();

        let 待译项 = 收集待译键(测试目录.path(), "vi").unwrap();
        assert_eq!(待译项.len(), 2);
        assert_eq!(待译项[0].1, "服务器");

        let mut 翻译表 = HashMap::new();
        翻译表.insert("服务器".to_string(), "Máy chủ".to_string());
        let 已替换数 = 应用翻译(测试目录.path(), &翻译表, "vi").unwrap();
        assert_eq!(已替换数, 1);

        let 内容文本 = std::fs::read_to_string(&测试文件).unwrap();
        assert!(内容文本.contains("\"Máy chủ\" = \"Server\""));
        assert!(
            !内容文本.contains("Máy chủ\" = \"Server\"  # TODO"),
            "已译键不应带 TODO"
        );
        assert!(内容文本.contains("TODO(vi)"), "未命中键应保留 TODO");
    }

    /// 冲突项提取：keyword_collision 与 cross_conflict 均能定位到 (文件, 键)
    #[test]
    fn 测试收集冲突项() {
        let 测试目录 = tempfile::tempdir().unwrap();
        std::fs::write(测试目录.path().join("a.toml"), "").unwrap();
        std::fs::write(测试目录.path().join("b.toml"), "").unwrap();
        let mut 检查报告 = 校验报告::default();
        检查报告
            .错误清单
            .push("mc_keyword_collision|a.toml|标识符|错误|bail|Err".to_string());
        检查报告
            .错误清单
            .push("mc_cross_conflict|连接|a.toml|join|b.toml|Connection".to_string());
        let 冲突结果 = 收集冲突项(&检查报告, 测试目录.path());
        assert!(冲突结果.contains(&("a.toml".to_string(), "错误".to_string())));
        assert!(冲突结果.contains(&("a.toml".to_string(), "连接".to_string())));
        assert!(冲突结果.contains(&("b.toml".to_string(), "连接".to_string())));
    }

    /// 按文件精确改名：保留行尾注释，仅替换目标键
    #[test]
    fn 测试应用改名() {
        let 测试目录 = tempfile::tempdir().unwrap();
        let 测试文件 = 测试目录.path().join("a.toml");
        std::fs::write(
            &测试文件,
            "\"连接\" = \"join\"\n\"其他\" = \"other\"  # 注释保留\n",
        )
        .unwrap();
        let mut 改名表 = HashMap::new();
        改名表.insert(
            ("a.toml".to_string(), "连接".to_string()),
            "连接等待".to_string(),
        );
        应用改名(测试目录.path(), &改名表, "vi").unwrap();
        let 内容文本 = std::fs::read_to_string(&测试文件).unwrap();
        assert!(内容文本.contains("\"连接等待\" = \"join\""));
        assert!(内容文本.contains("\"其他\" = \"other\"  # 注释保留"));
    }

    // ---------- 跨语言完整性检查 ----------

    /// 构造含完整 keywords/errors 的视图（zh 基准样）
    fn 视图含语言数据(
        语言代码: &str, 关键字内容: &str, 错误内容: &str
    ) -> 语言包视图 {
        语言包视图 {
            语言代码: 语言代码.to_string(),
            关键字内容: 关键字内容.to_string(),
            标准库内容: String::new(),
            错误内容: 错误内容.to_string(),
            三方库文件: Vec::new(),
        }
    }

    const 基准关键字: &str =
        "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n[\"派生特征\"]\n\"克隆\" = \"Clone\"\n";
    const 基准错误: &str = "[E0425]\n\"消息模板\" = \"x\"\n\"教学提示\" = \"y\"\n[\"消息翻译\"]\n\"mismatched types\" = { \"消息模板\" = \"t\" }\n";

    /// 语言包完整时跨语言检查通过
    #[test]
    fn 测试跨语言对完整通过() {
        let 基准视图 = 视图含语言数据("zh", 基准关键字, 基准错误);
        let 目标视图 = 视图含语言数据("ja", 基准关键字, 基准错误);
        let 检查报告 = 校验跨语言对(&基准视图, &目标视图);
        assert!(检查报告.是否通过(), "完整包应通过: {:?}", 检查报告.错误清单);
        assert!(
            检查报告.警告清单.is_empty(),
            "无警告: {:?}",
            检查报告.警告清单
        );
    }

    /// 缺少派生特征节报 error（方言特性不可用）
    #[test]
    fn 测试跨语言缺派生被检出() {
        let 基准视图 = 视图含语言数据("zh", 基准关键字, 基准错误);
        let 目标视图 = 视图含语言数据(
            "ja",
            "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n",
            基准错误,
        );
        let 检查报告 = 校验跨语言对(&基准视图, &目标视图);
        assert!(
            检查报告
                .错误清单
                .iter()
                .any(|每错| 每错.starts_with("mc_cross_derive_missing")),
            "缺派生特征应报 error: {:?}",
            检查报告.错误清单
        );
    }

    /// 缺少关键字值报 error（方言词不可用）
    #[test]
    fn 测试跨语言缺关键字被检出() {
        let 基准视图 = 视图含语言数据("zh", 基准关键字, 基准错误);
        let 目标视图 = 视图含语言数据(
            "ja",
            "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n[\"派生特征\"]\n\"克隆\" = \"Clone\"\n",
            基准错误,
        );
        // 与基准语言相同的母语键（值相同），故意改成不同的值
        let 检查报告 = 校验跨语言对(&基准视图, &目标视图);
        // 该用例下目标与基准值集合一致，应通过
        assert!(检查报告.是否通过());
        // 缺 "let" 值的情况
        let 缺失视图 = 视图含语言数据(
            "ja",
            "[\"声明\"]\n\"函数\" = \"fn\"\n[\"派生特征\"]\n\"克隆\" = \"Clone\"\n",
            基准错误,
        );
        let 检查报告 = 校验跨语言对(&基准视图, &缺失视图);
        assert!(
            检查报告
                .错误清单
                .iter()
                .any(|每错| 每错.starts_with("mc_cross_kw_missing")),
            "缺关键字值应报 error: {:?}",
            检查报告.错误清单
        );
    }

    /// 缺少错误码节报 error，缺消息翻译键报 warning
    #[test]
    fn 测试跨语言缺错误被检出() {
        let 基准视图 = 视图含语言数据("zh", 基准关键字, 基准错误);
        // 缺错误码节 E0425
        let 目标视图 = 视图含语言数据(
            "ja",
            基准关键字,
            "[\"消息翻译\"]\n\"mismatched types\" = { \"消息模板\" = \"t\" }\n",
        );
        let 检查报告 = 校验跨语言对(&基准视图, &目标视图);
        assert!(
            检查报告
                .错误清单
                .iter()
                .any(|每错| 每错.starts_with("mc_cross_err_missing")),
            "缺错误码节应报 error: {:?}",
            检查报告.错误清单
        );
        // 缺消息翻译键
        let 目标视图 = 视图含语言数据(
            "ja",
            基准关键字,
            "[E0425]\n\"消息模板\" = \"x\"\n\"教学提示\" = \"y\"\n",
        );
        let 检查报告 = 校验跨语言对(&基准视图, &目标视图);
        assert!(
            检查报告.是否通过(),
            "缺消息翻译键不应阻断: {:?}",
            检查报告.错误清单
        );
        assert!(
            检查报告
                .警告清单
                .iter()
                .any(|每警| 每警.starts_with("mc_cross_msg_missing")),
            "缺消息翻译键应报 warning: {:?}",
            检查报告.警告清单
        );
    }

    /// 内置全部语言通过跨语言完整性检查（zh 基准）
    #[test]
    fn 测试全部内置跨语言完整性() {
        let 基准 = 语言包视图::自内置构造("zh");
        for 语言代码 in crate::内置语言::内置语言代码() {
            if 语言代码 == "zh" {
                continue;
            }
            let 视图 = 语言包视图::自内置构造(语言代码);
            if 视图.关键字内容.is_empty() && 视图.错误内容.is_empty() {
                continue;
            }
            let 检查报告 = 校验跨语言对(&基准, &视图);
            assert!(
                检查报告.是否通过(),
                "内置包 {语言代码} 跨语言完整性应通过: {:?}",
                检查报告.错误清单
            );
        }
    }

    /// 提取双节：两节正常提取；TOML 非法（含重复键）返回 Err
    #[test]
    fn 测试提取双节基本与错误() {
        let (模块路径, 标识项) = 提取双节(
            "[\"模块路径\"]\n\"序列化\" = \"serde\"\n[\"标识符\"]\n\"向量\" = \"Vec\"\n",
        )
        .expect("合法 TOML 应解析成功");
        assert_eq!(模块路径.get("序列化").map(String::as_str), Some("serde"));
        assert_eq!(标识项.get("向量").map(String::as_str), Some("Vec"));
        assert!(提取双节("[\"标识符\"]\n\"a\" = \"1\"\n\"a\" = \"2\"\n").is_err());
    }

    /// 报告渲染三分支：干净通过 / 仅警告通过 / 错误失败；
    /// 渲染问题 对无参数 code 与未知 code 均不 panic
    #[test]
    fn 测试打印报告全分支() {
        let 干净报告 = 校验报告::default();
        打印报告("zh", &干净报告);
        let mut 仅警告报告 = 校验报告::default();
        仅警告报告
            .警告清单
            .push("mc_stdlib_shadow|a.toml|键|a|b".to_string());
        打印报告("zh", &仅警告报告);
        let mut 失败报告 = 校验报告::default();
        失败报告
            .错误清单
            .push("mc_parse_failed|a.toml|重复键".to_string());
        失败报告
            .警告清单
            .push("mc_stdlib_shadow|a.toml|键|a|b".to_string());
        打印报告("zh", &失败报告);
        // 无 `|` 参数的 code：整串作为 code 查模板
        let _ = 渲染问题("mc_ok");
        // 未知 code：回退键本身，不 panic
        let _ = 渲染问题("not_a_real_code|x");
    }

    /// 生成骨架：首次写入并带目标语 TODO 注释（英文值保留）；重跑跳过已存在文件
    #[test]
    fn 测试骨架写入后跳过() {
        let 测试目录 = tempfile::tempdir().unwrap();
        let 产出目录 = 测试目录.path().join("fr");
        let 源视图 = 视图含库(vec![("a.toml", "[\"标识符\"]\n\"服务器\" = \"Server\"\n")]);
        let (已新建数, 已跳过数) = 生成骨架(&源视图, "fr", &产出目录).expect("首次生成应成功");
        assert_eq!((已新建数, 已跳过数), (1, 0));
        let 内容文本 = std::fs::read_to_string(产出目录.join("a.toml")).unwrap();
        assert!(
            内容文本.contains("TODO(fr)"),
            "应追加目标语 TODO：{内容文本}"
        );
        assert!(
            内容文本.contains("\"Server\""),
            "英文值必须保留：{内容文本}"
        );
        let (已新建数二, 已跳过数二) = 生成骨架(&源视图, "fr", &产出目录).expect("重跑应成功");
        assert_eq!((已新建数二, 已跳过数二), (0, 1), "重跑必须幂等跳过");
    }

    /// 视图自库目录：目录内 keywords/stdlib 优先；
    /// 缺省时回退源视图（crates 内容始终从目标目录读取）
    #[test]
    fn 测试视图自库目录覆盖与回退() {
        let 源视图 = 视图含库(vec![("src.toml", "[\"标识符\"]\n\"源\" = \"Src\"\n")]);
        // 覆盖路径：目标目录自带 keywords.toml
        let 测试目录 = tempfile::tempdir().unwrap();
        let 语言目录 = 测试目录.path().join("de");
        let 库目录 = 语言目录.join("crates");
        std::fs::create_dir_all(&库目录).unwrap();
        std::fs::write(
            库目录.join("a.toml"),
            "[\"标识符\"]\n\"服务器\" = \"Server\"\n",
        )
        .unwrap();
        std::fs::write(语言目录.join("keywords.toml"), "# 自定义关键字\n").unwrap();
        let 视图 = 视图自库目录(&库目录, &源视图);
        assert_eq!(视图.语言代码, "de");
        assert_eq!(视图.三方库文件.len(), 1);
        assert_eq!(视图.关键字内容, "# 自定义关键字\n");
        // 回退路径：空 语言目录（无 crates、无 keywords）→ 回退源视图语言表
        let 测试目录二 = tempfile::tempdir().unwrap();
        let 语言目录二 = 测试目录二.path().join("xx");
        let 库目录二 = 语言目录二.join("crates");
        std::fs::create_dir_all(&库目录二).unwrap();
        let 回退视图 = 视图自库目录(&库目录二, &源视图);
        assert_eq!(回退视图.语言代码, "xx");
        assert!(回退视图.三方库文件.is_empty());
        assert!(回退视图.关键字内容.contains("函数"));
    }

    /// 运行校验：内置语言代码走内置视图（zh 干净 → 真）；
    /// 未知目标（非目录非内置）报错；外部目录缺 keywords.toml 报错
    #[test]
    fn 测试运行校验目标分支() {
        assert!(
            运行校验(Some("zh")).expect("内置 zh 应可校验"),
            "内置 zh 基线必须通过"
        );
        assert!(运行校验(Some("no-such-lang-xyz")).is_err());
        let 测试目录 = tempfile::tempdir().unwrap();
        assert!(
            运行校验(测试目录.path().to_str()).is_err(),
            "缺 keywords.toml 应报错"
        );
    }
}
