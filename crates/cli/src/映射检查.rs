//! 第三方库映射校验与脚手架模块
//!
//! 提供 `rzc mapping check` 与 `rzc mapping scaffold` 的核心逻辑：
//! - 校验：校验单个语言包的三方库映射文本质量
//!   （配置格式可解析 / 无重复键 / 关键字避让 / 跨文件同键冲突 / 条目数统计），
//!   以及跨内置语言的条目数一致性对比。
//! - 脚手架：从源语言的三方库映射生成目标语言的翻译骨架（保留英文值，母语键留待翻译）。
//!
//! 校验规则来自翻译实践：
//! 1. 关键字避让：三方库键与关键字配置的键相撞时，关键字先替换，三方库键永不生效 → 错误
//! 2. 三方库文件之间同键不同值：读取目录顺序未定义，合并非确定 → 错误
//! 3. 三方库键与标准库标识符同键不同值：标准库最后加载优先，三方库键被覆盖失效 → 警告

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use i18n_rust_engine::映射源;

/// 「母语词 → 英文」映射表
use anyhow::anyhow;

type 名称映射 = HashMap<String, String>;

/// 语言包映射数据的统一视图（内置数据与外部目录两种来源归一化）
pub struct 语言包视图 {
    /// 语言代码（如 zh / ru）
    pub 语言代码: String,
    /// 关键字配置文本
    pub 关键字内容: String,
    /// 标准库配置文本
    pub 标准库内容: String,
    /// 错误配置文本（错误码/消息翻译）
    pub 错误内容: String,
    /// 三方库映射（文件名, 配置文本），按文件名排序保证确定性
    pub 三方库表: Vec<(String, String)>,
}

impl 语言包视图 {
    /// 从内置语言包数据构造视图
    pub fn 从内置(语言代码: &str) -> Self {
        let 数据 = crate::内置语言::获取内置数据(语言代码);
        let mut 三方库表: Vec<(String, String)> = 数据
            .三方库数据
            .iter()
            .map(|(包名, 内容串)| (包名.to_string(), 内容串.to_string()))
            .collect();
        三方库表.sort();
        Self {
            语言代码: 语言代码.to_string(),
            关键字内容: 数据.关键字内容.to_string(),
            标准库内容: 数据.标准库内容.to_string(),
            错误内容: 数据.错误内容.to_string(),
            三方库表,
        }
    }

    /// 从外部语言包目录构造视图（读取关键字、标准库与三方库映射）
    pub fn 从目录(目录值: &Path) -> anyhow::Result<Self> {
        let 语言代码 = 目录值
            .file_name()
            .and_then(|本名| 本名.to_str())
            .unwrap_or("unknown")
            .to_string();
        let 关键字内容 = std::fs::read_to_string(目录值.join("keywords.toml"))
            .map_err(|错误值| anyhow!("读取 keywords.toml 失败: {}", 错误值))?;
        // 标准库文件可选（部分语言包可能未提供）
        let 标准库内容 = std::fs::read_to_string(目录值.join("stdlib.toml")).unwrap_or_default();
        // 错误文件可选（英文等母语即英文的语言无此文件）
        let 错误内容 = std::fs::read_to_string(目录值.join("errors.toml")).unwrap_or_default();
        let mut 三方库表 = Vec::new();
        let 三方库目录 = 目录值.join("crates");
        if 三方库目录.is_dir() {
            let mut 文件清单: Vec<PathBuf> = std::fs::read_dir(&三方库目录)
                .map_err(|错误值| anyhow!("读取 crates 目录失败: {}", 错误值))?
                .filter_map(|目录条目| 目录条目.ok())
                .map(|目录条目| 目录条目.path())
                .filter(|路径值| 路径值.extension().and_then(|后缀| 后缀.to_str()) == Some("toml"))
                .collect();
            文件清单.sort();
            for 路径值 in 文件清单 {
                let 文件本名 = 路径值
                    .file_name()
                    .and_then(|本名| 本名.to_str())
                    .unwrap_or("unknown.toml")
                    .to_string();
                let 内容串 = std::fs::read_to_string(&路径值)
                    .map_err(|错误值| anyhow!("读取 {} 失败: {}", 路径值.display(), 错误值))?;
                三方库表.push((文件本名, 内容串));
            }
        }
        Ok(Self {
            语言代码,
            关键字内容,
            标准库内容,
            错误内容,
            三方库表,
        })
    }
}

/// 校验统计信息
#[derive(Debug, Default, Clone)]
pub struct 校验统计 {
    /// 三方库文件数
    pub 包文件数: usize,
    /// 模块路径节条目总数
    pub 模块路径条目数: usize,
    /// 标识符节条目总数
    pub 标识符条目数: usize,
}

/// 校验报告
#[derive(Debug, Default)]
pub struct 校验报告 {
    /// 必须修复的错误
    pub 错误列表: Vec<String>,
    /// 建议修复的警告
    pub 警告列表: Vec<String>,
    /// 统计信息
    pub 统计: 校验统计,
}

impl 校验报告 {
    /// 是否通过（无错误；警告不阻断）
    pub fn 是否通过(&self) -> bool {
        self.错误列表.is_empty()
    }
}

/// 提取配置文本中「模块路径」与「标识符」两节的键值对
///
/// 返回（模块路径表, 标识符表）。配置解析失败（含重复键）时返回错误。
fn 提取各节(内容文本: &str) -> Result<(名称映射, 名称映射), String> {
    let 节表值 = 映射源::解析映射节(内容文本).map_err(|错误值| 错误值.to_string())?;
    let 模块路径表 = 节表值.get("模块路径").cloned().unwrap_or(HashMap::new());
    let 标识符表 = 节表值.get("标识符").cloned().unwrap_or(HashMap::new());
    Ok((模块路径表, 标识符表))
}

/// 提取关键字配置中所有节的「母语词 → 英文」映射
///
/// 复用引擎权威语义（摊平节表：按节名升序合并，后到覆盖），
/// 与运行时实际生效的关键字映射保持一致，避免校验误报。
/// 「派生特征」节仅在派生属性内生效（不并入关键字表），
/// 此处同步排除，否则与标准库/三方库的同键异值（如调试在派生特征与
/// 标准库中的取值不同）会被误报为冲突。
///
/// 用于关键字避让检测：需比较三方库键与关键字的值是否一致，
/// 同值视为安全冗余（关键字替换与三方库替换结果相同），不同值才是真冲突。
fn 提取关键字映射(内容文本: &str) -> HashMap<String, String> {
    match 映射源::解析映射节(内容文本) {
        Ok(各节) => {
            let mut 映射表 = 映射源::摊平节表(&各节);
            if let Some(派生节) = 各节.get("派生特征") {
                for 键名 in 派生节.keys() {
                    映射表.remove(键名);
                }
            }
            映射表
        }
        Err(_) => HashMap::new(),
    }
}

/// 关键字配置必需节（引擎按节名取表，缺节不会报错，只会让整类词条静默失效）
///
/// - `宏`：宏名 → 英文宏名，供宏调用自动补 `!`。缺失则宏映射为空、
///   宏调用不再补 `!`，产物非法却仍报「转译成功」，最难排查；
/// - `派生特征`：派生属性参数。缺失则派生参数不替换；
/// - 其余节经摊平节表合并进关键字表，缺节即整类方言词不可用。
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

/// 校验单个映射文件的危险等级元数据
///
/// 规则（均为错误，防止回归为「隐式崩溃 / 误标纯安全」）：
/// - 等级值只允许会恐慌、会改写、纯安全（多风险用 `+` 连接）；
/// - 元数据键必须对应本文件已声明的词条（防止改名后元数据失效）；
/// - 映射目标属于一定会恐慌的集合时，显式标注不得漏掉恐慌；
/// - 关键字/宏词条带改写风险，显式标注不得漏掉改写。
fn 检查危险元数据(
    报告值: &mut 校验报告,
    文件本名: &str,
    内容文本: &str,
    目标表: &名称映射,
    是关键字: bool,
) {
    use i18n_rust_engine::映射源::{
        危险备注节, 危险等级, 危险等级节, 恒恐慌目标
    };

    let Ok(各节) = 映射源::解析映射节(内容文本) else {
        return; // 配置解析错误已由其他检查项报告
    };
    let mut 已声明 = HashSet::new();
    for (词条, 原始文本) in 各节.get(危险等级节).into_iter().flatten() {
        已声明.insert(词条.as_str());
        let Some(等级值) = 危险等级::解析等级(原始文本) else {
            报告值.错误列表.push(format!(
                "mc_danger_invalid|{}|{}|{}",
                文件本名, 词条, 原始文本
            ));
            continue;
        };
        let Some(目标) = 目标表.get(词条) else {
            报告值
                .错误列表
                .push(format!("mc_danger_orphan|{}|{}", 文件本名, 词条));
            continue;
        };
        if 恒恐慌目标(目标) && !等级值.会恐慌 {
            报告值.错误列表.push(format!(
                "mc_danger_understated|{}|{}|{}",
                文件本名, 词条, 目标
            ));
        }
        if 是关键字 && !等级值.会改写 {
            报告值.错误列表.push(format!(
                "mc_danger_understated_rewrite|{}|{}",
                文件本名, 词条
            ));
        }
    }
    // 只写备注不写等级同样算孤立元数据（两节必须成对维护）
    for 词条 in 各节
        .get(危险备注节)
        .into_iter()
        .flatten()
        .map(|(词条名, _)| 词条名)
    {
        if !已声明.contains(词条.as_str()) && !目标表.contains_key(词条) {
            报告值
                .错误列表
                .push(format!("mc_danger_orphan|{}|{}", 文件本名, 词条));
        }
    }
}

/// 校验单个语言包的三方库映射质量
///
/// 检查项（按严重级别）：
/// - 错误：配置解析失败（含重复键）
/// - 错误：标准库配置解析失败（重复键等——标准库整体加载失败，
///   运行时标准库映射全部丢失，此前静默吞掉导致德语与葡萄牙语重复键长期潜伏）
/// - 错误：三方库键与关键字配置的键相撞（关键字先替换，三方库键永不生效）
/// - 错误：三方库文件之间模块路径/标识符同键不同值（合并非确定）
/// - 警告：三方库标识符键与标准库标识符同键不同值（标准库优先，三方库键失效）
///
/// 设计约定（不检查）：同一文件内「模块路径」与「标识符」两节
/// 双节同键不同值是合法模式（模块路径为三方库名的小写形式，
/// 标识符为类型名的大写形式，两表在词法/语义阶段独立查询，互不干扰），
/// 不视为冲突；同键同值则为安全冗余。
pub fn 校验语言包(视图: &语言包视图) -> 校验报告 {
    let mut 报告值 = 校验报告::default();
    let 关键字映射 = 提取关键字映射(&视图.关键字内容);

    // 0. 关键字配置的可解析性与必需节完整性。
    // 解析失败（如重复键）会让关键字表整体失效（全部方言词不可用）；
    // 缺节不会报错——引擎按节名取表，缺节返回空表，整类词条静默失效。
    // 其中缺「宏」最危险：宏调用不再自动补 `!`，产物非法却仍报「转译成功」。
    match toml::from_str::<toml::Table>(&视图.关键字内容) {
        Ok(表值) => {
            for 节名 in 必需关键字节 {
                if !表值.contains_key(*节名) {
                    报告值
                        .错误列表
                        .push(format!("mc_missing_section|{}|{}", 视图.语言代码, 节名));
                }
            }
        }
        Err(错误值) => 报告值
            .错误列表
            .push(format!("mc_parse_failed|keywords.toml|{}", 错误值)),
    }

    // 标准库解析失败必须报错（重复键等会让标准库整体加载失败）；
    // 空内容（无标准库文件）解析为空表，不会误报
    let (_, 标准库标识符表) = match 提取各节(&视图.标准库内容) {
        Ok(各节) => 各节,
        Err(错误值) => {
            报告值
                .错误列表
                .push(format!("mc_stdlib_parse_failed|{}", 错误值));
            (HashMap::new(), HashMap::new())
        }
    };

    // 危险等级元数据：值合法性 / 孤立键 / 漏标恐慌与改写
    // （关键字默认改写、解包类目标默认恐慌，显式标注不得降级）
    {
        let mut 关键字目标表 = 关键字映射.clone();
        if let Ok(各节) = 映射源::解析映射节(&视图.关键字内容)
            && let Some(派生节) = 各节.get("派生特征")
        {
            关键字目标表.extend(
                派生节
                    .iter()
                    .map(|(键名, 值项)| (键名.clone(), 值项.clone())),
            );
        }
        检查危险元数据(
            &mut 报告值,
            "keywords.toml",
            &视图.关键字内容,
            &关键字目标表,
            true,
        );
        let (mut 标准库目标表, 标准库标识符目标表) = 提取各节(&视图.标准库内容).unwrap_or_default();
        标准库目标表.extend(标准库标识符目标表);
        检查危险元数据(
            &mut 报告值,
            "stdlib.toml",
            &视图.标准库内容,
            &标准库目标表,
            false,
        );
    }

    // 跨文件键追踪（模块路径与标识符分别维护）：键 -> (值, 首次出现的文件)
    let mut 已见模块路径: HashMap<String, (String, String)> = HashMap::new();
    let mut 已见标识符: HashMap<String, (String, String)> = HashMap::new();

    for (文件本名, 内容文本) in &视图.三方库表 {
        报告值.统计.包文件数 += 1;
        // 1. 配置解析（重复键 / 格式错误在此暴露）
        let (模块路径表, 标识符表) = match 提取各节(内容文本) {
            Ok(各节) => 各节,
            Err(错误值) => {
                报告值
                    .错误列表
                    .push(format!("mc_parse_failed|{}|{}", 文件本名, 错误值));
                continue;
            }
        };
        报告值.统计.模块路径条目数 += 模块路径表.len();
        报告值.统计.标识符条目数 += 标识符表.len();
        // 危险等级元数据：键必须对应本文件词条，且不得漏标恐慌
        let mut 危险目标表 = 模块路径表.clone();
        危险目标表.extend(
            标识符表
                .iter()
                .map(|(键名, 值项)| (键名.clone(), 值项.clone())),
        );
        检查危险元数据(&mut 报告值, 文件本名, 内容文本, &危险目标表, false);

        // 2. 关键字避让（对模块路径节与标识符节的键都检查）
        // 3. 跨文件同键不同值（两节对称：合并结果由文件名排序决定，
        //    同键不同值意味着覆盖语义取决于排序，属确定性歧义）
        for 节名 in ["模块路径", "标识符"] {
            let (条目表, 已见表, 错误码) = if 节名 == "模块路径" {
                (&模块路径表, &mut 已见模块路径, "mc_cross_conflict_mp")
            } else {
                (&标识符表, &mut 已见标识符, "mc_cross_conflict")
            };
            for (键名, 值项) in 条目表 {
                // 关键字避让：仅当三方库键与关键字值不同时报错；
                // 同值时关键字替换与三方库替换结果一致，属安全冗余不报错
                if 关键字映射
                    .get(键名)
                    .is_some_and(|关键字值| 关键字值 != 值项)
                {
                    报告值.错误列表.push(format!(
                        "mc_keyword_collision|{}|{}|{}|{}|{}",
                        文件本名, 节名, 键名, 值项, 关键字映射[键名]
                    ));
                }
                // 跨文件同键不同值
                if let Some((前值, 前来源)) = 已见表.get(键名) {
                    if 前值 != 值项 {
                        报告值.错误列表.push(format!(
                            "{}|{}|{}|{}|{}|{}",
                            错误码, 键名, 前来源, 前值, 文件本名, 值项
                        ));
                    }
                } else {
                    已见表.insert(键名.clone(), (值项.clone(), 文件本名.clone()));
                }
                // 标准库覆盖检测（标识符节；标准库后加载优先）
                if 节名 == "标识符"
                    && 标准库标识符表
                        .get(键名)
                        .is_some_and(|标准库值| 标准库值 != 值项)
                {
                    报告值.警告列表.push(format!(
                        "mc_stdlib_shadow|{}|{}|{}|{}",
                        文件本名, 键名, 值项, 标准库标识符表[键名]
                    ));
                }
            }
        }
    }
    报告值
}

/// 从源语言的三方库映射生成目标语言的翻译骨架
///
/// 保留配置结构与英文值，在每个含母语键的行后追加待办注释提示翻译。
/// 返回生成的文件数。目标目录不存在时自动创建。
pub fn 生成骨架(
    源视图: &语言包视图,
    目标语言: &str,
    输出目录: &Path,
) -> anyhow::Result<(usize, usize)> {
    std::fs::create_dir_all(输出目录)
        .map_err(|错误值| anyhow!("创建目录 {} 失败: {}", 输出目录.display(), 错误值))?;
    let mut 已创建数 = 0;
    let mut 已跳过数 = 0;
    for (文件本名, 内容文本) in &源视图.三方库表 {
        let 目标路径 = 输出目录.join(文件本名);
        // 已存在的文件跳过：保留既有翻译成果，保证重跑幂等
        if 目标路径.exists() {
            已跳过数 += 1;
            continue;
        }
        let 骨架文本 = 追加骨架待办(内容文本, &源视图.语言代码, 目标语言);
        std::fs::write(&目标路径, 骨架文本)
            .map_err(|错误值| anyhow!("写入 {} 失败: {}", 目标路径.display(), 错误值))?;
        已创建数 += 1;
    }
    Ok((已创建数, 已跳过数))
}

/// 为配置文本中每个「母语键 = 英文值」行追加待办翻译注释
///
/// 仅处理「模块路径」与「标识符」节内的键值行；节标题、注释、空行原样保留。
fn 追加骨架待办(内容文本: &str, 源语言: &str, 目标语言: &str) -> String {
    let mut 输出文本 = String::new();
    let mut 是否目标节 = false;
    for 行文本 in 内容文本.lines() {
        let 裁剪行 = 行文本.trim();
        // 节标题检测：进入/离开目标节
        if 裁剪行.starts_with('[') {
            是否目标节 = 裁剪行 == "[\"模块路径\"]" || 裁剪行 == "[\"标识符\"]";
            输出文本.push_str(行文本);
            输出文本.push('\n');
            continue;
        }
        // 目标节内的键值行（含 `=` 且非注释）追加待办
        if 是否目标节 && 裁剪行.contains('=') && !裁剪行.starts_with('#') {
            // 已有行尾注释时不重复追加
            if !裁剪行.contains("TODO") {
                输出文本.push_str(行文本);
                输出文本.push_str(&format!("  # TODO({}): 将键从 {} 翻译", 目标语言, 源语言));
                输出文本.push('\n');
                continue;
            }
        }
        输出文本.push_str(行文本);
        输出文本.push('\n');
    }
    输出文本
}

/// 打印单个语言包的校验报告（本地化输出）
///
/// 报告条目格式为 `code|arg1|arg2...`，按消息码查界面模板渲染。
fn 打印报告(语言代码: &str, 报告值: &校验报告) {
    let 界面表 = crate::界面::界面消息表::全局表();
    println!("{}", 界面表.取带参消息("mc_header", &[语言代码]));
    println!(
        "{}",
        界面表.取带参消息(
            "mc_stats",
            &[
                &报告值.统计.包文件数.to_string(),
                &报告值.统计.模块路径条目数.to_string(),
                &报告值.统计.标识符条目数.to_string()
            ]
        )
    );
    for 错误项 in &报告值.错误列表 {
        println!("{}", 渲染问题(错误项));
    }
    for 警告项 in &报告值.警告列表 {
        println!("{}", 渲染问题(警告项));
    }
    if 报告值.是否通过() {
        println!(
            "{}",
            if 报告值.警告列表.is_empty() {
                界面表.取消息("mc_ok")
            } else {
                界面表.取带参消息("mc_ok_with_warnings", &[&报告值.警告列表.len().to_string()])
            }
        );
    } else {
        println!(
            "{}",
            界面表.取带参消息(
                "mc_failed",
                &[
                    &报告值.错误列表.len().to_string(),
                    &报告值.警告列表.len().to_string()
                ]
            )
        );
    }
}

/// 将 `code|arg1|arg2...` 格式的问题条目渲染为本地化消息
fn 渲染问题(问题文本: &str) -> String {
    let 界面表 = crate::界面::界面消息表::全局表();
    let mut 片段表 = 问题文本.split('|');
    let 消息码 = 片段表.next().unwrap_or(问题文本);
    let 参数表: Vec<&str> = 片段表.collect();
    界面表.取带参消息(消息码, &参数表)
}

/// 「映射校验」命令入口
///
/// - 目标为空值：校验全部内置语言并输出跨语言条目数一致性对比
/// - 目标为已存在的目录路径：按外部语言包目录校验
/// - 否则按内置语言代码校验
pub fn 运行检查(目标: Option<&str>) -> anyhow::Result<bool> {
    let 界面表 = crate::界面::界面消息表::全局表();
    let Some(目标值) = 目标 else {
        // 全部内置语言 + 跨语言一致性对比 + 跨语言完整性门禁
        let mut 全部通过 = true;
        for 语言代码 in crate::内置语言::内置语言清单() {
            let 视图 = 语言包视图::从内置(语言代码);
            let 报告值 = 校验语言包(&视图);
            全部通过 &= 报告值.是否通过();
            打印报告(语言代码, &报告值);
        }
        打印跨语言数量();
        全部通过 &= 打印跨语言完整性();
        return Ok(全部通过);
    };
    let 路径值 = Path::new(目标值);
    if 路径值.is_dir() {
        let 视图 = 语言包视图::从目录(路径值)?;
        let 报告值 = 校验语言包(&视图);
        let 通过 = 报告值.是否通过();
        打印报告(&视图.语言代码, &报告值);
        Ok(通过)
    } else if crate::内置语言::有内置语言包(目标值) {
        let 视图 = 语言包视图::从内置(目标值);
        let 报告值 = 校验语言包(&视图);
        let 通过 = 报告值.是否通过();
        打印报告(目标值, &报告值);
        Ok(通过)
    } else {
        anyhow::bail!("{}", 界面表.取带参消息("mc_unknown_target", &[目标值]));
    }
}

/// 跨语言完整性检查报告
#[derive(Debug, Default)]
pub struct 跨语言报告 {
    /// 必须修复的错误（方言词缺失，功能缺口）
    pub 错误列表: Vec<String>,
    /// 建议补齐的警告（有回退机制，不阻断）
    pub 警告列表: Vec<String>,
}

impl 跨语言报告 {
    /// 是否通过（无错误；警告不阻断）
    pub fn 是否通过(&self) -> bool {
        self.错误列表.is_empty()
    }
}

/// 提取关键字配置的扁平化关键字值集合与派生特征值集合
///
/// 与引擎运行时语义一致（摊平节表 + 派生特征节单独存放），
/// 按英文值对齐而非母语键：各语言母语词不同（日语用日语、德语用德语），
/// 只有英文值是跨语言可比的方言能力。
fn 提取关键字值集合(内容文本: &str) -> (HashSet<String>, HashSet<String>) {
    match 映射源::解析映射节(内容文本) {
        Ok(各节) => {
            let mut 关键字集 = HashSet::new();
            let mut 派生集 = HashSet::new();
            for (节名, 映射表) in &各节 {
                // 危险等级/备注是元数据节（值不是 Rust 名称），
                // 并入值集合会把恐慌等标记当成缺失词条误报
                if 映射源::是否元数据节(节名) {
                    continue;
                }
                let 目标集 = if 节名 == "派生特征" {
                    &mut 派生集
                } else {
                    &mut 关键字集
                };
                for 值项 in 映射表.values() {
                    目标集.insert(值项.clone());
                }
            }
            (关键字集, 派生集)
        }
        Err(_) => (HashSet::new(), HashSet::new()),
    }
}

/// 提取错误配置的错误码节集合（如 E0425）与消息翻译键集合
fn 错误组成(内容文本: &str) -> (HashSet<String>, HashSet<String>) {
    // 配置解析入口必须是整文档，不能按单个值解析，否则会静默退化为空集合
    let Ok(值对象) = toml::from_str::<toml::Value>(内容文本) else {
        return (HashSet::new(), HashSet::new());
    };
    let Some(表值) = 值对象.as_table() else {
        return (HashSet::new(), HashSet::new());
    };
    let mut 错误码集 = HashSet::new();
    let mut 消息键集 = HashSet::new();
    for (键名, 值项) in 表值 {
        if 键名.starts_with('E')
            && 键名.len() == 5
            && 键名[1..].chars().all(|单字| 单字.is_ascii_digit())
        {
            错误码集.insert(键名.clone());
        } else if 键名 == "消息翻译"
            && let Some(消息表) = 值项.as_table()
        {
            for 消息键 in 消息表.keys() {
                消息键集.insert(消息键.clone());
            }
        }
    }
    (错误码集, 消息键集)
}

/// 单对基准/目标语言包的跨语言完整性检查
///
/// 规则（基准 = 语言包维护语言，当前为 zh）：
/// - 错误：目标语言缺少基准语言拥有的关键字英文值（方言词不可用，功能缺口）
/// - 错误：目标语言缺少派生特征值（派生属性参数不可用）
/// - 错误：目标语言缺少基准语言的错误码节（该错误码无母语教学提示）
/// - 警告：目标语言缺少消息翻译键（无错误码时回退英文原文，可工作）
pub fn 校验跨语言对(基准: &语言包视图, 目标: &语言包视图) -> 跨语言报告 {
    let mut 报告值 = 跨语言报告::default();
    let (基准关键字, 基准派生) = 提取关键字值集合(&基准.关键字内容);
    let (目标关键字, 目标派生) = 提取关键字值集合(&目标.关键字内容);
    let (基准错误码, 基准消息键) = 错误组成(&基准.错误内容);
    let (目标错误码, 目标消息键) = 错误组成(&目标.错误内容);

    // 关键字值缺失：合并输出，避免每个词一条噪音
    let mut 缺失关键字: Vec<String> = 基准关键字
        .iter()
        .filter(|值项| !目标关键字.contains(*值项))
        .cloned()
        .collect();
    缺失关键字.sort();
    if !缺失关键字.is_empty() {
        报告值.错误列表.push(format!(
            "mc_cross_kw_missing|{}|{}|{}",
            目标.语言代码,
            缺失关键字.len(),
            缺失关键字.join("/")
        ));
    }
    // 派生特征值缺失（语法上在关键字配置的派生特征节）
    let mut 缺失派生: Vec<String> = 基准派生
        .iter()
        .filter(|值项| !目标派生.contains(*值项))
        .cloned()
        .collect();
    缺失派生.sort();
    if !缺失派生.is_empty() {
        报告值.错误列表.push(format!(
            "mc_cross_derive_missing|{}|{}|{}",
            目标.语言代码,
            缺失派生.len(),
            缺失派生.join("/")
        ));
    }
    // 错误码节缺失
    let mut 缺失错误码: Vec<String> = 基准错误码
        .iter()
        .filter(|值项| !目标错误码.contains(*值项))
        .cloned()
        .collect();
    缺失错误码.sort();
    if !缺失错误码.is_empty() {
        报告值.错误列表.push(format!(
            "mc_cross_err_missing|{}|{}|{}",
            目标.语言代码,
            缺失错误码.len(),
            缺失错误码.join("/")
        ));
    }
    // 消息翻译键缺失（警告：无错误码匹配时回退英文原文）
    let mut 缺失消息: Vec<String> = 基准消息键
        .iter()
        .filter(|值项| !目标消息键.contains(*值项))
        .cloned()
        .collect();
    缺失消息.sort();
    if !缺失消息.is_empty() {
        报告值.警告列表.push(format!(
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
    报告值
}

/// 打印跨内置语言的完整性检查（以 zh 为基准，错误阻断）
///
/// 英文等母语即英文的语言包无关键字/错误文件，跳过不参与比较。
/// 返回是否全部通过（供运行检查汇总退出码）。
pub fn 打印跨语言完整性() -> bool {
    let 界面表 = crate::界面::界面消息表::全局表();
    println!("{}", 界面表.取消息("mc_cross_integrity_header"));
    let 基准 = 语言包视图::从内置("zh");
    let mut 全部通过 = true;
    let mut 有检查 = false;
    for 语言代码 in crate::内置语言::内置语言清单() {
        if 语言代码 == "zh" {
            continue;
        }
        let 视图 = 语言包视图::从内置(语言代码);
        // 无关键字/错误数据（母语即英文）的语言不参与比较
        if 视图.关键字内容.is_empty() && 视图.错误内容.is_empty() {
            continue;
        }
        有检查 = true;
        let 报告值 = 校验跨语言对(&基准, &视图);
        全部通过 &= 报告值.是否通过();
        for 错误项 in &报告值.错误列表 {
            println!("  {}", 渲染问题(错误项));
        }
        for 警告项 in &报告值.警告列表 {
            println!("  {}", 渲染问题(警告项));
        }
        if 报告值.是否通过() && 报告值.警告列表.is_empty() {
            println!("  {}: {}", 语言代码, 界面表.取消息("mc_cross_integrity_ok"));
        } else if 报告值.是否通过() {
            println!(
                "  {}: {}",
                语言代码,
                界面表.取带参消息(
                    "mc_cross_integrity_warn",
                    &[&报告值.警告列表.len().to_string()]
                )
            );
        }
    }
    if !有检查 {
        println!("  {}", 界面表.取消息("mc_cross_integrity_none"));
    }
    全部通过
}

fn 打印跨语言数量() {
    let 界面表 = crate::界面::界面消息表::全局表();
    let 语言清单 = crate::内置语言::内置语言清单();
    let mut 数量表: Vec<(String, usize, usize)> = Vec::new();
    for 语言代码 in 语言清单 {
        let 视图 = 语言包视图::从内置(语言代码);
        let 报告值 = 校验语言包(&视图);
        数量表.push((
            语言代码.to_string(),
            报告值.统计.包文件数,
            报告值.统计.标识符条目数,
        ));
    }
    数量表.sort_by(|甲, 乙| 甲.0.cmp(&乙.0));
    println!("{}", 界面表.取消息("mc_cross_lang_header"));
    for (语言代码, _, 条目数) in &数量表 {
        println!("  {}: {}", 语言代码, 条目数);
    }
    // 仅对有包文件的语言比较条目数
    let 有包清单: Vec<usize> = 数量表
        .iter()
        .filter(|(_, 包数, _)| *包数 > 0)
        .map(|(_, _, 条目数)| *条目数)
        .collect();
    let 首值 = 有包清单.first().copied();
    let 不一致 = 有包清单.iter().any(|条目数| Some(*条目数) != 首值);
    if 不一致 {
        println!("{}", 界面表.取消息("mc_cross_lang_inconsistent"));
    } else {
        println!("{}", 界面表.取消息("mc_cross_lang_consistent"));
    }
}

/// 「映射脚手架」命令入口
///
/// 源语言必须是内置语言代码；输出为空值时默认写入
/// 项目语言包根 `<目标>/crates/`（主仓库内为 crates/engine/lang-packs/，
/// 用户项目为 lang-packs/）。
/// 服务商：规则（默认，生成待办骨架待人工翻译）或
/// 深度求索 / 开放人工智能 / 兼容端点（智能服务自动翻译键名，需配好密钥环境变量）。
pub fn 运行生成骨架(
    源语言: &str,
    目标语言: &str,
    输出参数: Option<&Path>,
    服务商: &str,
) -> anyhow::Result<()> {
    let 界面表 = crate::界面::界面消息表::全局表();
    if !crate::内置语言::有内置语言包(源语言) {
        anyhow::bail!("{}", 界面表.取带参消息("mc_unknown_source", &[源语言]));
    }
    // 先判定服务商与凭据再动手：拼错的服务商选项、没配的密钥都不该
    // 先生成一堆骨架文件再报错
    let mut 用智能 = false;
    match crate::映射生成::解析服务商(服务商) {
        Some(服务商模式) => match 服务商模式 {
            crate::映射生成::服务商::智能 => {
                crate::映射生成::确保凭据()?;
                用智能 = true;
            }
            crate::映射生成::服务商::规则 => {}
        },
        None => anyhow::bail!(
            "{}",
            界面表.取带参消息("mapping_unknown_provider", &[服务商])
        ),
    };
    let 视图 = 语言包视图::从内置(源语言);
    if 视图.三方库表.is_empty() {
        anyhow::bail!("{}", 界面表.取带参消息("mc_no_crates", &[源语言]));
    }
    let 输出目录 = match 输出参数 {
        Some(目录值) => 目录值.to_path_buf(),
        None => {
            let 基准目录 = std::env::current_dir()
                .ok()
                .and_then(|工作目录| crate::查找项目根向上(&工作目录))
                .unwrap_or_else(|| PathBuf::from("."));
            crate::语言包根目录(&基准目录).join(format!("{}/crates", 目标语言))
        }
    };
    let (已创建数, 已跳过数) = 生成骨架(&视图, 目标语言, &输出目录)?;
    if 已创建数 > 0 {
        println!(
            "{}",
            界面表.取带参消息(
                "mc_scaffold_generated",
                &[&已创建数.to_string(), &输出目录.display().to_string()]
            )
        );
    }
    if 已跳过数 > 0 {
        println!(
            "{}",
            界面表.取带参消息("mc_scaffold_skip_existing", &[&已跳过数.to_string()])
        );
    }
    if 用智能 {
        智能翻译骨架(&视图, 源语言, 目标语言, &输出目录)?;
    } else {
        println!("{}", 界面表.取带参消息("mc_scaffold_hint", &[目标语言]));
    }
    Ok(())
}

// ==================== 智能翻译（服务商：深度求索 / 开放人工智能 / 兼容端点） ====================

/// 每批送智能服务的键数上限
const 智能批量上限: usize = 60;
/// 冲突改名最大重试轮数
const 智能最大重试轮数: usize = 2;

/// 智能翻译脚手架：提取待办键 → 批量翻译 → 写回 → 校验冲突 → 改名重试
fn 智能翻译骨架(
    源视图: &语言包视图,
    源语言: &str,
    目标语言: &str,
    输出目录: &Path,
) -> anyhow::Result<()> {
    let 界面表 = crate::界面::界面消息表::全局表();
    // 禁用词：源包关键字配置的键（译名撞关键字会永不生效）
    let 禁用词表: Vec<String> = 提取关键字映射(&源视图.关键字内容).keys().cloned().collect();

    let 项列表 = 收集待办键(输出目录, 目标语言)?;
    if 项列表.is_empty() {
        return Ok(());
    }
    // 按（键, 英文值）去重，保证跨文件同键翻译一致
    let mut 唯一项: Vec<(String, String)> = Vec::new();
    for (_, 键名, 值项) in &项列表 {
        if !唯一项
            .iter()
            .any(|(已有键, 已有值)| 已有键 == 键名 && 已有值 == 值项)
        {
            唯一项.push((键名.clone(), 值项.clone()));
        }
    }

    // 批量翻译
    let mut 翻译表: HashMap<String, String> = HashMap::new();
    let 总批数 = 唯一项.chunks(智能批量上限).count();
    for (批序, 批次) in 唯一项.chunks(智能批量上限).enumerate() {
        println!(
            "{}",
            界面表.取带参消息(
                "mc_scaffold_ai_batch",
                &[&(批序 + 1).to_string(), &总批数.to_string()]
            )
        );
        let 已用词: Vec<String> = 翻译表.values().cloned().collect();
        let 批次映射 = 智能翻译键(源语言, 目标语言, 批次, &禁用词表, &已用词)?;
        翻译表.extend(批次映射);
    }
    let 替换数 = 应用翻译(输出目录, &翻译表, 目标语言)?;
    println!(
        "{}",
        界面表.取带参消息(
            "mc_scaffold_ai_done",
            &[&替换数.to_string(), &项列表.len().to_string()]
        )
    );
    if 替换数 < 项列表.len() {
        println!(
            "{}",
            界面表.取带参消息(
                "mc_scaffold_ai_partial",
                &[&(项列表.len() - 替换数).to_string()]
            )
        );
    }

    // 校验回环：对生成的三方库目录重新校验，冲突键送智能服务改名重试
    for 轮次 in 1..=智能最大重试轮数 {
        let 报告值 = 校验语言包(&从三方库目录构建视图(输出目录, 源视图));
        let 冲突项 = 收集冲突项(&报告值, 输出目录);
        if 冲突项.is_empty() {
            if !报告值.警告列表.is_empty() {
                println!(
                    "{}",
                    界面表.取带参消息(
                        "mc_ok_with_warnings",
                        &[&报告值.警告列表.len().to_string()]
                    )
                );
            } else {
                println!("{}", 界面表.取消息("mc_ok"));
            }
            return Ok(());
        }
        println!(
            "{}",
            界面表.取带参消息(
                "mc_scaffold_ai_retry",
                &[
                    &冲突项.len().to_string(),
                    &轮次.to_string(),
                    &智能最大重试轮数.to_string()
                ]
            )
        );
        let 改名表 = 智能重命名冲突(目标语言, &冲突项, &禁用词表)?;
        应用改名(输出目录, &改名表, 目标语言)?;
    }
    // 重试后仍可能有残留冲突：输出最终报告供人工处理
    let 视图 = 从三方库目录构建视图(输出目录, 源视图);
    let 报告值 = 校验语言包(&视图);
    if !报告值.是否通过() {
        打印报告(目标语言, &报告值);
        println!(
            "{}",
            界面表.取带参消息("mc_scaffold_ai_retry_left", &[目标语言])
        );
    } else {
        println!("{}", 界面表.取消息("mc_ok"));
    }
    Ok(())
}

/// 扫描输出目录中含目标语言待办标记的行，返回（文件名, 键, 英文值）
fn 收集待办键(
    输出目录: &Path,
    目标语言: &str,
) -> anyhow::Result<Vec<(String, String, String)>> {
    let mut 项列表 = Vec::new();
    let 待办标记 = format!("TODO({})", 目标语言);
    for 目录条目 in 列出配置清单(输出目录)? {
        let 文件本名 = 目录条目
            .file_name()
            .and_then(|本名| 本名.to_str())
            .unwrap_or_default()
            .to_string();
        let 内容文本 = std::fs::read_to_string(&目录条目)?;
        for 行文本 in 内容文本.lines() {
            if !行文本.contains(&待办标记) {
                continue;
            }
            if let Some((键名, 值项)) = 解析键值行(行文本) {
                项列表.push((文件本名.clone(), 键名, 值项));
            }
        }
    }
    Ok(项列表)
}

/// 列出目录下的配置文本（按名排序保证确定性）
fn 列出配置清单(目录值: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut 文件清单: Vec<PathBuf> = std::fs::read_dir(目录值)?
        .filter_map(|目录条目| 目录条目.ok())
        .map(|目录条目| 目录条目.path())
        .filter(|路径值| 路径值.extension().and_then(|后缀| 后缀.to_str()) == Some("toml"))
        .collect();
    文件清单.sort();
    Ok(文件清单)
}

/// 解析 `"键" = "值" ...` 行，返回（键, 值）
///
/// 键与值均支持转义引号（`\"`），避免在转义处错误截断。
fn 解析键值行(行文本: &str) -> Option<(String, String)> {
    let 裁剪行 = 行文本.trim();
    let (键部分, 值部分) = 裁剪行.split_once('=')?;
    let 键名 = 解析引号内容(键部分.trim())?;
    let 值项 = 解析引号内容(值部分.trim())?;
    Some((键名, 值项))
}

/// 解析带转义的引号内容：剥离前缀引号后逐字符扫描，
/// 转义序列取下一字符原样保留，遇未转义引号即结束
fn 解析引号内容(片段: &str) -> Option<String> {
    let 内部文本 = 片段.strip_prefix('"')?;
    let mut 输出文本 = String::new();
    let mut 字符流 = 内部文本.chars();
    while let Some(单字) = 字符流.next() {
        if 单字 == '\\' {
            // 转义序列：取下一个字符原样保留
            if let Some(下一字) = 字符流.next() {
                输出文本.push(下一字);
            }
        } else if 单字 == '"' {
            // 未转义的引号：内容结束
            break;
        } else {
            输出文本.push(单字);
        }
    }
    Some(输出文本)
}

/// 调用智能服务批量翻译键名，返回 源键→目标语言键
fn 智能翻译键(
    源语言: &str,
    目标语言: &str,
    项列表: &[(String, String)],
    禁用词表: &[String],
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
    let 键清单 = 项列表
        .iter()
        .map(|(键名, 值项)| format!("{} = {}", 键名, 值项))
        .collect::<Vec<_>>()
        .join("\n");
    let 用户提示 = format!(
        "source language: {}\ntarget language: {}\nforbidden words: {}\n\
used words: {}\nkeys to translate (key = its English API):\n{}",
        源语言,
        目标语言,
        禁用词表.join(", "),
        已用词.join(", "),
        键清单
    );
    let 内容文本 = crate::映射生成::智能对话(系统提示, &用户提示)?;
    解析结构化映射(&内容文本)
}

/// 从校验报告提取冲突项（文件名, 键），供改名重试
fn 收集冲突项(报告值: &校验报告, 输出目录: &Path) -> Vec<(String, String)> {
    let mut 项列表: Vec<(String, String)> = Vec::new();
    for 错误项 in &报告值.错误列表 {
        let 片段表: Vec<&str> = 错误项.split('|').collect();
        match 片段表.first().copied() {
            Some("mc_keyword_collision") if 片段表.len() >= 4 => {
                项列表.push((片段表[1].to_string(), 片段表[3].to_string()));
            }
            Some("mc_cross_conflict") if 片段表.len() >= 6 => {
                let 键名 = 片段表[1];
                项列表.push((片段表[2].to_string(), 键名.to_string()));
                项列表.push((片段表[4].to_string(), 键名.to_string()));
            }
            Some("mc_cross_conflict_mp") if 片段表.len() >= 6 => {
                let 键名 = 片段表[1];
                项列表.push((片段表[2].to_string(), 键名.to_string()));
                项列表.push((片段表[4].to_string(), 键名.to_string()));
            }
            _ => {}
        }
    }
    // 去重并确认文件仍存在
    项列表.retain(|(文件本名, _)| 输出目录.join(文件本名).is_file());
    项列表.sort();
    项列表.dedup();
    项列表
}

/// 调用智能服务为冲突键改名，返回（文件名, 旧键）→新键
fn 智能重命名冲突(
    目标语言: &str,
    项列表: &[(String, String)],
    禁用词表: &[String],
) -> anyhow::Result<HashMap<(String, String), String>> {
    let 系统提示 = "You rename conflicting mapping keys of a Rust teaching \
dialect language pack. Rules:\n\
1. Output ONLY a JSON object {\"file|key\": \"new_key\"}, no explanation.\n\
2. New keys must be multi-word terms in the target language, avoiding the forbidden \
list and all other existing keys.\n\
3. Keep the technical meaning of the original key.";
    let 清单 = 项列表
        .iter()
        .map(|(文件本名, 键名)| format!("{}|{}", 文件本名, 键名))
        .collect::<Vec<_>>()
        .join("\n");
    let 用户提示 = format!(
        "target language: {}\nforbidden words: {}\n\
conflicting entries (file|key) to rename:\n{}",
        目标语言,
        禁用词表.join(", "),
        清单
    );
    let 内容文本 = crate::映射生成::智能对话(系统提示, &用户提示)?;
    let 原始映射 = 解析结构化映射(&内容文本)?;
    let mut 改名表 = HashMap::new();
    for (组合键, 新键名) in 原始映射 {
        if let Some((文件本名, 旧键名)) = 组合键.split_once('|') {
            改名表.insert((文件本名.to_string(), 旧键名.to_string()), 新键名);
        }
    }
    Ok(改名表)
}

/// 解析智能服务返回文本中的 JSON 对象（容忍 ```json 围栏与前后杂讯）
fn 解析结构化映射(文本串: &str) -> anyhow::Result<HashMap<String, String>> {
    let 起始位 = 文本串
        .find('{')
        .ok_or_else(|| anyhow!("AI 返回中未找到 JSON 对象"))?;
    let 终位 = 文本串[起始位..]
        .rfind('}')
        .map(|位置值| 起始位 + 位置值 + 1)
        .ok_or_else(|| anyhow!("AI 返回的 JSON 不完整"))?;
    let 值对象: serde_json::Value = serde_json::from_str(&文本串[起始位..终位])?;
    let mut 映射表 = HashMap::new();
    if let Some(对象表) = 值对象.as_object() {
        for (键名, 值项) in 对象表 {
            if let Some(内容值) = 值项.as_str()
                && !键名.is_empty()
                && !内容值.is_empty()
            {
                映射表.insert(键名.clone(), 内容值.to_string());
            }
        }
    }
    Ok(映射表)
}

/// 将翻译结果写回：替换待办行的键并移除待办注释；未命中键保留待人工
fn 应用翻译(
    输出目录: &Path,
    翻译表: &HashMap<String, String>,
    目标语言: &str,
) -> anyhow::Result<usize> {
    let mut 替换数 = 0;
    let 待办标记 = format!("TODO({})", 目标语言);
    for 目录条目 in 列出配置清单(输出目录).unwrap_or_default() {
        let 内容文本 = match std::fs::read_to_string(&目录条目) {
            Ok(文本串) => 文本串,
            Err(_) => continue,
        };
        let mut 输出文本 = String::new();
        for 行文本 in 内容文本.lines() {
            if 行文本.contains(&待办标记)
                && let Some((键名, 值项)) = 解析键值行(行文本)
                && let Some(新键名) = 翻译表.get(&键名)
            {
                // 重写为无待办的行（丢弃原行尾注释中的待办部分）
                输出文本.push_str(&format!("\"{}\" = \"{}\"\n", 新键名, 值项));
                替换数 += 1;
                continue;
            }
            输出文本.push_str(行文本);
            输出文本.push('\n');
        }
        // 写回失败必须上抛（此前静默吞掉会让用户看到成功但文件未更新）
        std::fs::write(&目录条目, 输出文本)
            .map_err(|错误值| anyhow!("翻译结果写回失败: {}: {}", 目录条目.display(), 错误值))?;
    }
    Ok(替换数)
}

/// 按（文件名, 旧键）精确改名（冲突重试轮次用）
fn 应用改名(
    输出目录: &Path,
    改名表: &HashMap<(String, String), String>,
    目标语言: &str,
) -> anyhow::Result<()> {
    for 目录条目 in 列出配置清单(输出目录).unwrap_or_default() {
        let 文件本名 = 目录条目
            .file_name()
            .and_then(|本名| 本名.to_str())
            .unwrap_or_default()
            .to_string();
        let 内容文本 = match std::fs::read_to_string(&目录条目) {
            Ok(文本串) => 文本串,
            Err(_) => continue,
        };
        let mut 输出文本 = String::new();
        for 行文本 in 内容文本.lines() {
            let mut 已写入 = false;
            if let Some((键名, 值项)) = 解析键值行(行文本)
                && let Some(新键名) = 改名表.get(&(文件本名.clone(), 键名.clone()))
            {
                let 后缀 = if 行文本.contains(&format!("TODO({})", 目标语言)) {
                    String::new()
                } else {
                    // 保留原有行尾注释（非待办部分）
                    行文本
                        .split('#')
                        .nth(1)
                        .map(|注释文本| format!("  # {}", 注释文本.trim()))
                        .unwrap_or_default()
                };
                输出文本.push_str(&format!("\"{}\" = \"{}\"{}\n", 新键名, 值项, 后缀));
                已写入 = true;
            }
            if !已写入 {
                输出文本.push_str(行文本);
                输出文本.push('\n');
            }
        }
        std::fs::write(&目录条目, 输出文本)
            .map_err(|错误值| anyhow!("改名结果写回失败: {}: {}", 目录条目.display(), 错误值))?;
    }
    Ok(())
}

/// 从已生成的三方库目录构造校验视图（关键字/标准库取自源包）
fn 从三方库目录构建视图(
    三方库目录: &Path, 源视图: &语言包视图
) -> 语言包视图 {
    let mut 三方库表 = Vec::new();
    for 目录条目 in 列出配置清单(三方库目录).unwrap_or_default() {
        let 文件本名 = 目录条目
            .file_name()
            .and_then(|本名| 本名.to_str())
            .unwrap_or("unknown.toml")
            .to_string();
        if let Ok(内容文本) = std::fs::read_to_string(&目录条目) {
            三方库表.push((文件本名, 内容文本));
        }
    }
    // 目标语言包目录（三方库的父目录）若已有关键字/标准库则优先用它们
    let 语言包目录 = 三方库目录.parent().map(|路径值| 路径值.to_path_buf());
    let 关键字内容 = 语言包目录
        .as_ref()
        .map(|目录值| std::fs::read_to_string(目录值.join("keywords.toml")).unwrap_or_default())
        .filter(|内容值| !内容值.is_empty())
        .unwrap_or_else(|| 源视图.关键字内容.clone());
    let 标准库内容 = 语言包目录
        .as_ref()
        .map(|目录值| std::fs::read_to_string(目录值.join("stdlib.toml")).unwrap_or_default())
        .filter(|内容值| !内容值.is_empty())
        .unwrap_or_else(|| 源视图.标准库内容.clone());
    let 错误内容 = 语言包目录
        .as_ref()
        .map(|目录值| std::fs::read_to_string(目录值.join("errors.toml")).unwrap_or_default())
        .filter(|内容值| !内容值.is_empty())
        .unwrap_or_else(|| 源视图.错误内容.clone());
    语言包视图 {
        语言代码: 三方库目录
            .parent()
            .and_then(|路径值| 路径值.file_name())
            .and_then(|本名| 本名.to_str())
            .unwrap_or("unknown")
            .to_string(),
        关键字内容,
        标准库内容,
        错误内容,
        三方库表,
    }
}

#[cfg(test)]
mod 母语测试 {
    use super::*;

    /// 构造最小视图用于测试
    fn 视图含三方库(三方库表: Vec<(&str, &str)>) -> 语言包视图 {
        语言包视图 {
            语言代码: "zh".to_string(),
            // 含全部必需节：本组用例聚焦三方库冲突检测，
            // 缺节会额外触发缺节错误干扰断言
            关键字内容: concat!(
                "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n",
                "[\"控制流\"]\n[\"类型\"]\n[\"逻辑值\"]\n[\"特殊值\"]\n[\"内存\"]\n",
                "[\"不安全\"]\n[\"错误处理\"]\n[\"宏\"]\n[\"派生特征\"]\n[\"标准库成员\"]\n",
            )
            .to_string(),
            标准库内容: "[\"标识符\"]\n\"字符串\" = \"String\"\n".to_string(),
            错误内容: String::new(),
            三方库表: 三方库表
                .into_iter()
                .map(|(包名, 内容串)| (包名.to_string(), 内容串.to_string()))
                .collect(),
        }
    }

    /// 正常映射通过校验
    #[test]
    fn 测试干净映射通过() {
        let 视图 = 视图含三方库(vec![(
            "a.toml",
            "[\"标识符\"]\n\"服务器\" = \"Server\"\n\"路由\" = \"Router\"\n",
        )]);
        let 报告值 = 校验语言包(&视图);
        assert!(报告值.是否通过(), "干净映射应通过: {:?}", 报告值.错误列表);
        assert_eq!(报告值.统计.标识符条目数, 2);
        assert_eq!(报告值.统计.包文件数, 1);
    }

    /// 危险等级校验：会恐慌的目标标成纯安全必须报错
    ///
    /// 回归保护（「要恐慌必须显式写」）：映射到解包类目标的词条
    /// 若标成纯安全，调用方会当默认路径使用，运行即崩。
    #[test]
    fn 测试危险等级漏标恐慌() {
        let 视图 = 视图含三方库(vec![(
            "a.toml",
            r#"
["标识符"]
"取列" = "get_unwrap"

["危险等级"]
"取列" = "safe"
"#,
        )]);
        let 报告值 = 校验语言包(&视图);
        assert!(
            报告值
                .错误列表
                .iter()
                .any(|错误项| 错误项.starts_with("mc_danger_understated|a.toml|取列")),
            "恐慌目标标成纯安全必须报错: {:?}",
            报告值.错误列表
        );
    }

    /// 危险等级校验：关键字/宏词条默认改写，显式标纯安全也不得降级
    #[test]
    fn 测试危险等级关键字漏标改写() {
        let 视图 = 语言包视图 {
            语言代码: "zh".to_string(),
            关键字内容: r#"
["声明"]
"函数" = "fn"
["控制流"]
["类型"]
["逻辑值"]
["特殊值"]
["内存"]
["不安全"]
["错误处理"]
["宏"]
["派生特征"]
["标准库成员"]

["危险等级"]
"函数" = "safe"
"#
            .to_string(),
            标准库内容: String::new(),
            错误内容: String::new(),
            三方库表: Vec::new(),
        };
        let 报告值 = 校验语言包(&视图);
        assert!(
            报告值.错误列表.iter().any(
                |错误项| 错误项.starts_with("mc_danger_understated_rewrite|keywords.toml|函数")
            ),
            "关键字标成纯安全必须报错: {:?}",
            报告值.错误列表
        );
    }

    /// 危险等级校验：非法值与孤立元数据都要报错
    ///
    /// 拼写错误会让整条标注静默失效；词条改名后旧标注同样
    /// 静默失效——两者都不能只靠人工审查。
    #[test]
    fn 测试危险等级非法与孤立() {
        let 视图 = 视图含三方库(vec![(
            "a.toml",
            r#"
["标识符"]
"服务器" = "Server"

["危险等级"]
"服务器" = "panik"
"已改名" = "panic"
"#,
        )]);
        let 报告值 = 校验语言包(&视图);
        assert!(
            报告值
                .错误列表
                .iter()
                .any(|错误项| 错误项.starts_with("mc_danger_invalid|a.toml|服务器|panik")),
            "非法等级值应报错: {:?}",
            报告值.错误列表
        );
        assert!(
            报告值
                .错误列表
                .iter()
                .any(|错误项| 错误项.starts_with("mc_danger_orphan|a.toml|已改名")),
            "孤立元数据应报错: {:?}",
            报告值.错误列表
        );
    }

    /// 缺少必需节报错误（缺节不报错但整类词条静默失效）
    ///
    /// 回归：缺「宏」节时宏映射返回空表，宏调用不再自动补 `!`，
    /// 产物非法却仍显示「转译成功」——最难排查的一类静默降级。
    #[test]
    fn 测试缺必需节() {
        let 视图 = 语言包视图 {
            语言代码: "xx".to_string(),
            // 仅保留「声明」与「宏」，缺少其余必需节
            关键字内容: "[\"声明\"]\n\"函数\" = \"fn\"\n[\"宏\"]\n\"打印行\" = \"println\"\n"
                .to_string(),
            标准库内容: String::new(),
            错误内容: String::new(),
            三方库表: Vec::new(),
        };
        let 报告值 = 校验语言包(&视图);
        assert!(!报告值.是否通过(), "缺必需节应校验失败");
        let 缺失节: Vec<&String> = 报告值
            .错误列表
            .iter()
            .filter(|错误项| 错误项.starts_with("mc_missing_section"))
            .collect();
        assert_eq!(缺失节.len(), 必需关键字节.len() - 2, "应报出全部缺失节");
        assert!(
            缺失节.iter().any(|错误项| 错误项.ends_with("|派生特征")),
            "缺「派生特征」应被检出: {缺失节:?}"
        );
        // 含全部必需节的视图不应报缺节
        let 完整 = 视图含三方库(Vec::new());
        assert!(
            !校验语言包(&完整)
                .错误列表
                .iter()
                .any(|错误项| 错误项.starts_with("mc_missing_section")),
            "节齐全时不应报缺节"
        );
    }

    /// 关键字配置解析失败（重复键）报错误：关键字表整体失效
    #[test]
    fn 测试关键字解析失败() {
        let 视图 = 语言包视图 {
            语言代码: "xx".to_string(),
            // 同一节内重复键 → 配置解析失败
            关键字内容: "[\"声明\"]\n\"函数\" = \"fn\"\n\"函数\" = \"fn2\"\n".to_string(),
            标准库内容: String::new(),
            错误内容: String::new(),
            三方库表: Vec::new(),
        };
        let 报告值 = 校验语言包(&视图);
        assert!(
            报告值
                .错误列表
                .iter()
                .any(|错误项| 错误项.starts_with("mc_parse_failed|keywords.toml")),
            "关键字配置重复键应报解析失败: {:?}",
            报告值.错误列表
        );
    }

    /// 三方库键与关键字键相撞报错误
    #[test]
    fn 测试关键字冲突() {
        let 视图 = 视图含三方库(vec![(
            "a.toml",
            "[\"标识符\"]\n\"函数\" = \"some_fn\"\n", // 「函数」是关键字
        )]);
        let 报告值 = 校验语言包(&视图);
        assert!(!报告值.是否通过());
        assert!(
            报告值
                .错误列表
                .iter()
                .any(|错误项| 错误项.starts_with("mc_keyword_collision"))
        );
    }

    /// 三方库文件之间同键不同值报错误
    #[test]
    fn 测试跨文件冲突() {
        let 视图 = 视图含三方库(vec![
            ("a.toml", "[\"标识符\"]\n\"连接\" = \"connect_a\"\n"),
            ("b.toml", "[\"标识符\"]\n\"连接\" = \"connect_b\"\n"),
        ]);
        let 报告值 = 校验语言包(&视图);
        assert!(!报告值.是否通过());
        assert!(
            报告值
                .错误列表
                .iter()
                .any(|错误项| 错误项.starts_with("mc_cross_conflict"))
        );
    }

    /// 三方库文件之间同键同值不报错
    #[test]
    fn 测试跨文件同值通过() {
        let 视图 = 视图含三方库(vec![
            ("a.toml", "[\"标识符\"]\n\"连接\" = \"connect\"\n"),
            ("b.toml", "[\"标识符\"]\n\"连接\" = \"connect\"\n"),
        ]);
        let 报告值 = 校验语言包(&视图);
        assert!(
            !报告值
                .错误列表
                .iter()
                .any(|错误项| 错误项.starts_with("mc_cross_conflict")),
            "同键同值不应报冲突: {:?}",
            报告值.错误列表
        );
    }

    /// 三方库键与标准库标识符同键不同值报警告（不阻断）
    #[test]
    fn 测试标准库覆盖警告() {
        let 视图 = 视图含三方库(vec![(
            "a.toml",
            "[\"标识符\"]\n\"字符串\" = \"MyString\"\n", // 标准库中「字符串」词条指向字符串类型名
        )]);
        let 报告值 = 校验语言包(&视图);
        assert!(报告值.是否通过(), "标准库覆盖只是警告不阻断");
        assert!(
            报告值
                .警告列表
                .iter()
                .any(|警告项| 警告项.starts_with("mc_stdlib_shadow"))
        );
    }

    /// 重复键导致配置解析失败，报错误
    #[test]
    fn 测试重复键解析错误() {
        let 视图 = 视图含三方库(vec![(
            "a.toml",
            "[\"标识符\"]\n\"连接\" = \"connect\"\n\"连接\" = \"link\"\n",
        )]);
        let 报告值 = 校验语言包(&视图);
        assert!(!报告值.是否通过());
        assert!(
            报告值
                .错误列表
                .iter()
                .any(|错误项| 错误项.starts_with("mc_parse_failed"))
        );
    }

    /// 标准库配置解析失败（重复键）报错误，不再静默吞掉
    #[test]
    fn 测试标准库解析失败() {
        let 视图 = 语言包视图 {
            语言代码: "zh".to_string(),
            关键字内容: "[\"声明\"]\n\"函数\" = \"fn\"\n".to_string(),
            标准库内容: "[\"标识符\"]\n\"连接\" = \"link\"\n\"连接\" = \"net\"\n".to_string(),
            错误内容: String::new(),
            三方库表: vec![(
                "a.toml".to_string(),
                "[\"标识符\"]\n\"服务器\" = \"Server\"\n".to_string(),
            )],
        };
        let 报告值 = 校验语言包(&视图);
        assert!(
            报告值
                .错误列表
                .iter()
                .any(|错误项| 错误项.starts_with("mc_stdlib_parse_failed")),
            "标准库解析失败应报错误: {:?}",
            报告值.错误列表
        );
    }

    /// 无标准库文件（空内容）不应误报解析失败
    #[test]
    fn 测试空标准库不误报() {
        let 视图 = 语言包视图 {
            语言代码: "zh".to_string(),
            关键字内容: "[\"声明\"]\n\"函数\" = \"fn\"\n".to_string(),
            标准库内容: String::new(),
            错误内容: String::new(),
            三方库表: vec![(
                "a.toml".to_string(),
                "[\"标识符\"]\n\"服务器\" = \"Server\"\n".to_string(),
            )],
        };
        let 报告值 = 校验语言包(&视图);
        assert!(
            !报告值
                .错误列表
                .iter()
                .any(|错误项| 错误项.starts_with("mc_stdlib_parse_failed")),
            "空标准库不应误报: {:?}",
            报告值.错误列表
        );
    }

    /// 模块路径节跨文件同键不同值报错误（与标识符节对称）
    #[test]
    fn 测试模块路径跨文件冲突() {
        let 视图 = 视图含三方库(vec![
            ("a.toml", "[\"模块路径\"]\n\"网络库\" = \"reqwest\"\n"),
            ("b.toml", "[\"模块路径\"]\n\"网络库\" = \"hyper\"\n"),
        ]);
        let 报告值 = 校验语言包(&视图);
        assert!(
            报告值
                .错误列表
                .iter()
                .any(|错误项| 错误项.starts_with("mc_cross_conflict_mp")),
            "模块路径跨文件冲突应报错误: {:?}",
            报告值.错误列表
        );
    }

    /// 模块路径节跨文件同键同值不报错
    #[test]
    fn 测试模块路径跨文件同值通过() {
        let 视图 = 视图含三方库(vec![
            ("a.toml", "[\"模块路径\"]\n\"网络库\" = \"reqwest\"\n"),
            ("b.toml", "[\"模块路径\"]\n\"网络库\" = \"reqwest\"\n"),
        ]);
        let 报告值 = 校验语言包(&视图);
        assert!(
            !报告值
                .错误列表
                .iter()
                .any(|错误项| 错误项.starts_with("mc_cross_conflict_mp")),
            "模块路径同键同值不应报冲突: {:?}",
            报告值.错误列表
        );
    }

    /// 同一文件内双节同键不同值（模块路径为三方库名、标识符为类型名）
    /// 是合法设计，不应报错
    #[test]
    fn 测试双节同键不同值通过() {
        let 视图 = 视图含三方库(vec![(
            "a.toml",
            "[\"模块路径\"]\n\"火箭\" = \"rocket\"\n[\"标识符\"]\n\"火箭\" = \"Rocket\"\n",
        )]);
        let 报告值 = 校验语言包(&视图);
        assert!(
            报告值.是否通过(),
            "双节同键不同值（包名/类型名）应通过: {:?}",
            报告值.错误列表
        );
    }

    /// 骨架为键值行追加待办注释，节标题与注释保留
    #[test]
    fn 测试骨架追加待办() {
        let 内容文本 = "[\"标识符\"]\n# 注释行\n\"服务器\" = \"Server\"\n";
        let 结果值 = 追加骨架待办(内容文本, "zh", "vi");
        assert!(结果值.contains("TODO(vi)"));
        assert!(结果值.contains("# 注释行"));
        assert!(结果值.contains("\"服务器\" = \"Server\""));
    }

    /// 内置中文包自检：历史冲突已清理，校验应完全通过。
    #[test]
    fn 测试内置中文包通过() {
        let 视图 = 语言包视图::从内置("zh");
        let 报告值 = 校验语言包(&视图);
        // 工具正常运行，统计数据合理（包数动态取内置清单，随语言包目录自动纳入）
        assert_eq!(
            报告值.统计.包文件数,
            crate::内置语言::获取内置数据("zh").三方库数据.len()
        );
        assert!(报告值.统计.标识符条目数 > 0);
        // 历史冲突（关键字避让/跨文件同键不同值）已清理完毕
        assert!(
            报告值.是否通过(),
            "内置 zh 包应无错误: {:?}",
            报告值.错误列表
        );
    }

    /// 各内置语言的标识符条目数均可统计
    #[test]
    fn 测试各语言条目数() {
        for 语言代码 in ["zh", "ru", "de"] {
            let 视图 = 语言包视图::从内置(语言代码);
            let 报告值 = 校验语言包(&视图);
            assert!(报告值.统计.标识符条目数 > 0, "{语言代码} 应有条目");
        }
    }

    /// 全部内置语言均应通过校验（持续集成门禁的测试层基线）
    #[test]
    fn 测试全部内置语言通过() {
        for 语言代码 in crate::内置语言::内置语言清单() {
            let 视图 = 语言包视图::从内置(语言代码);
            let 报告值 = 校验语言包(&视图);
            assert!(
                报告值.是否通过(),
                "内置包 {语言代码} 应通过校验: {:?}",
                报告值.错误列表
            );
        }
    }

    /// 键值行解析：提取引号内键与值，忽略行尾注释
    #[test]
    fn 测试解析键值行() {
        let (键名, 值项) =
            解析键值行("\"服务器\" = \"Server\"  # TODO(vi): 将键从 zh 翻译").unwrap();
        assert_eq!(键名, "服务器");
        assert_eq!(值项, "Server");
        assert!(解析键值行("# 注释行").is_none());
        assert!(解析键值行("[\"标识符\"]").is_none());
    }

    /// 键与值两侧的转义引号都应正确还原（此前键侧不处理转义）
    #[test]
    fn 测试解析键值行转义引号() {
        let (键名, 值项) = 解析键值行("\"foo\\\"bar\" = \"va\\\"lue\"").unwrap();
        assert_eq!(键名, "foo\"bar");
        assert_eq!(值项, "va\"lue");
    }

    /// 智能服务返回 JSON 解析：容忍围栏与杂讯，丢弃空条目
    #[test]
    fn 测试解析结构化映射() {
        let 文本串 =
            "好的，以下是翻译结果：\n```json\n{\"服务器\": \"Máy chủ\", \"空\": \"\"}\n```\n";
        let 映射表 = 解析结构化映射(文本串).unwrap();
        assert_eq!(映射表.get("服务器").unwrap(), "Máy chủ");
        assert!(!映射表.contains_key("空"), "空值条目应丢弃");
        assert!(解析结构化映射("没有任何 JSON").is_err());
    }

    /// 待办键提取 + 翻译写回：替换键并移除待办标记，未命中键保留
    #[test]
    fn 测试收集与应用翻译() {
        let 目录值 = tempfile::tempdir().unwrap();
        let 文件路径 = 目录值.path().join("a.toml");
        std::fs::write(
            &文件路径,
            "[\"标识符\"]\n\"服务器\" = \"Server\"  # TODO(vi): 将键从 zh 翻译\n\
             \"路由\" = \"Router\"  # TODO(vi): 将键从 zh 翻译\n",
        )
        .unwrap();

        let 项列表 = 收集待办键(目录值.path(), "vi").unwrap();
        assert_eq!(项列表.len(), 2);
        assert_eq!(项列表[0].1, "服务器");

        let mut 翻译表 = HashMap::new();
        翻译表.insert("服务器".to_string(), "Máy chủ".to_string());
        let 替换数 = 应用翻译(目录值.path(), &翻译表, "vi").unwrap();
        assert_eq!(替换数, 1);

        let 内容文本 = std::fs::read_to_string(&文件路径).unwrap();
        assert!(内容文本.contains("\"Máy chủ\" = \"Server\""));
        assert!(
            !内容文本.contains("Máy chủ\" = \"Server\"  # TODO"),
            "已译键不应带待办"
        );
        assert!(内容文本.contains("TODO(vi)"), "未命中键应保留待办");
    }

    /// 冲突项提取：关键字冲突与跨文件冲突均能定位到（文件, 键）
    #[test]
    fn 测试收集冲突项() {
        let 目录值 = tempfile::tempdir().unwrap();
        std::fs::write(目录值.path().join("a.toml"), "").unwrap();
        std::fs::write(目录值.path().join("b.toml"), "").unwrap();
        let mut 报告值 = 校验报告::default();
        报告值
            .错误列表
            .push("mc_keyword_collision|a.toml|标识符|错误|bail|Err".to_string());
        报告值
            .错误列表
            .push("mc_cross_conflict|连接|a.toml|join|b.toml|Connection".to_string());
        let 项列表 = 收集冲突项(&报告值, 目录值.path());
        assert!(项列表.contains(&("a.toml".to_string(), "错误".to_string())));
        assert!(项列表.contains(&("a.toml".to_string(), "连接".to_string())));
        assert!(项列表.contains(&("b.toml".to_string(), "连接".to_string())));
    }

    /// 按文件精确改名：保留行尾注释，仅替换目标键
    #[test]
    fn 测试应用改名() {
        let 目录值 = tempfile::tempdir().unwrap();
        let 文件路径 = 目录值.path().join("a.toml");
        std::fs::write(
            &文件路径,
            "\"连接\" = \"join\"\n\"其他\" = \"other\"  # 注释保留\n",
        )
        .unwrap();
        let mut 改名表 = HashMap::new();
        改名表.insert(
            ("a.toml".to_string(), "连接".to_string()),
            "连接等待".to_string(),
        );
        应用改名(目录值.path(), &改名表, "vi").unwrap();
        let 内容文本 = std::fs::read_to_string(&文件路径).unwrap();
        assert!(内容文本.contains("\"连接等待\" = \"join\""));
        assert!(内容文本.contains("\"其他\" = \"other\"  # 注释保留"));
    }

    // ---------- 跨语言完整性检查 ----------

    /// 构造含完整关键字/错误数据的视图（zh 基准样）
    fn 视图含语言数据(
        语言代码: &str, 关键字文本: &str, 错误文本: &str
    ) -> 语言包视图 {
        语言包视图 {
            语言代码: 语言代码.to_string(),
            关键字内容: 关键字文本.to_string(),
            标准库内容: String::new(),
            错误内容: 错误文本.to_string(),
            三方库表: Vec::new(),
        }
    }

    const 基础关键字: &str =
        "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n[\"派生特征\"]\n\"克隆\" = \"Clone\"\n";
    const 基础错误: &str = "[E0425]\n\"消息模板\" = \"x\"\n\"教学提示\" = \"y\"\n[\"消息翻译\"]\n\"mismatched types\" = { \"消息模板\" = \"t\" }\n";

    /// 语言包完整时跨语言检查通过
    #[test]
    fn 测试跨语言对完整() {
        let 基准 = 视图含语言数据("zh", 基础关键字, 基础错误);
        let 目标 = 视图含语言数据("ja", 基础关键字, 基础错误);
        let 报告值 = 校验跨语言对(&基准, &目标);
        assert!(报告值.是否通过(), "完整包应通过: {:?}", 报告值.错误列表);
        assert!(报告值.警告列表.is_empty(), "无警告: {:?}", 报告值.警告列表);
    }

    /// 缺少派生特征节报错误（方言特性不可用）
    #[test]
    fn 测试跨语言缺派生() {
        let 基准 = 视图含语言数据("zh", 基础关键字, 基础错误);
        let 目标 = 视图含语言数据(
            "ja",
            "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n",
            基础错误,
        );
        let 报告值 = 校验跨语言对(&基准, &目标);
        assert!(
            报告值
                .错误列表
                .iter()
                .any(|错误项| 错误项.starts_with("mc_cross_derive_missing")),
            "缺派生特征应报错误: {:?}",
            报告值.错误列表
        );
    }

    /// 缺少关键字值报错误（方言词不可用）
    #[test]
    fn 测试跨语言缺关键字() {
        let 基准 = 视图含语言数据("zh", 基础关键字, 基础错误);
        let 目标 = 视图含语言数据(
            "ja",
            "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n[\"派生特征\"]\n\"克隆\" = \"Clone\"\n",
            基础错误,
        );
        // 与基准语言相同的母语键（值相同），故意改成不同的值
        let 报告值 = 校验跨语言对(&基准, &目标);
        // 该用例下目标与基准值集合一致，应通过
        assert!(报告值.是否通过());
        // 缺少某个关键字值的情况
        let 贫乏 = 视图含语言数据(
            "ja",
            "[\"声明\"]\n\"函数\" = \"fn\"\n[\"派生特征\"]\n\"克隆\" = \"Clone\"\n",
            基础错误,
        );
        let 报告值 = 校验跨语言对(&基准, &贫乏);
        assert!(
            报告值
                .错误列表
                .iter()
                .any(|错误项| 错误项.starts_with("mc_cross_kw_missing")),
            "缺关键字值应报错误: {:?}",
            报告值.错误列表
        );
    }

    /// 缺少错误码节报错误，缺消息翻译键报警告
    #[test]
    fn 测试跨语言缺错误() {
        let 基准 = 视图含语言数据("zh", 基础关键字, 基础错误);
        // 缺错误码节 E0425
        let 目标 = 视图含语言数据(
            "ja",
            基础关键字,
            "[\"消息翻译\"]\n\"mismatched types\" = { \"消息模板\" = \"t\" }\n",
        );
        let 报告值 = 校验跨语言对(&基准, &目标);
        assert!(
            报告值
                .错误列表
                .iter()
                .any(|错误项| 错误项.starts_with("mc_cross_err_missing")),
            "缺错误码节应报错误: {:?}",
            报告值.错误列表
        );
        // 缺消息翻译键
        let 目标 = 视图含语言数据(
            "ja",
            基础关键字,
            "[E0425]\n\"消息模板\" = \"x\"\n\"教学提示\" = \"y\"\n",
        );
        let 报告值 = 校验跨语言对(&基准, &目标);
        assert!(
            报告值.是否通过(),
            "缺消息翻译键不应阻断: {:?}",
            报告值.错误列表
        );
        assert!(
            报告值
                .警告列表
                .iter()
                .any(|警告项| 警告项.starts_with("mc_cross_msg_missing")),
            "缺消息翻译键应报警告: {:?}",
            报告值.警告列表
        );
    }

    /// 内置全部语言通过跨语言完整性检查（zh 基准）
    #[test]
    fn 测试全部内置跨语言完整性() {
        let 基准 = 语言包视图::从内置("zh");
        for 语言代码 in crate::内置语言::内置语言清单() {
            if 语言代码 == "zh" {
                continue;
            }
            let 视图 = 语言包视图::从内置(语言代码);
            if 视图.关键字内容.is_empty() && 视图.错误内容.is_empty() {
                continue;
            }
            let 报告值 = 校验跨语言对(&基准, &视图);
            assert!(
                报告值.是否通过(),
                "内置包 {语言代码} 跨语言完整性应通过: {:?}",
                报告值.错误列表
            );
        }
    }

    /// 提取各节：两节正常提取；配置非法（含重复键）返回错误
    #[test]
    fn 测试提取各节基础与错误() {
        let (模块路径表, 标识符表) = 提取各节(
            "[\"模块路径\"]\n\"序列化\" = \"serde\"\n[\"标识符\"]\n\"向量\" = \"Vec\"\n",
        )
        .expect("合法配置应解析成功");
        assert_eq!(模块路径表.get("序列化").map(String::as_str), Some("serde"));
        assert_eq!(标识符表.get("向量").map(String::as_str), Some("Vec"));
        assert!(提取各节("[\"标识符\"]\n\"a\" = \"1\"\n\"a\" = \"2\"\n").is_err());
    }

    /// 报告渲染三分支：干净通过 / 仅警告通过 / 错误失败；
    /// 渲染问题对无参数消息码与未知消息码均不崩溃
    ///
    /// 打印报告三调用只验证「不崩溃」（输出到标准输出，本单元不做捕获），
    /// 真正的不变量在渲染问题的消息码→模板链路上，逐条钉住。
    #[test]
    fn 测试打印报告全分支() {
        // 渲染问题走全局界面表，语言取自环境变量：这里持环境锁并钉住
        // RZ_LANG=zh，免得用户目录里的界面配置文件或持续集成预设的英文让结果时好时坏
        let _锁 = crate::语言管理::tests::环境锁();
        let _环境 =
            crate::语言管理::tests::环境还原::接管(
                &["RZ_LANG", "LC_ALL", "LC_MESSAGES", "LANG"],
            );
        // Rust 2024：进程级环境变量写入为不安全，由上面的环境锁串行化
        unsafe {
            std::env::set_var("RZ_LANG", "zh");
        }

        let 干净 = 校验报告::default();
        打印报告("zh", &干净);
        let mut 仅警告 = 校验报告::default();
        仅警告
            .警告列表
            .push("mc_stdlib_shadow|a.toml|键|a|b".to_string());
        打印报告("zh", &仅警告);
        let mut 失败 = 校验报告::default();
        失败
            .错误列表
            .push("mc_parse_failed|a.toml|重复键".to_string());
        失败
            .警告列表
            .push("mc_stdlib_shadow|a.toml|键|a|b".to_string());
        打印报告("zh", &失败);

        // 以下断言只看「结构」，不比对译文文字：界面文案随语言与系统语言变化，
        // 钉死中文串会让用例在别的区域设置下随机失败。
        // 无 `|` 参数：整串作为消息码查模板，命中时必须拿到译文而不是键名
        assert_ne!(渲染问题("mc_ok"), "mc_ok", "mc_ok 应有界面模板");
        assert!(!渲染问题("mc_ok").is_empty(), "模板不得渲染成空串");
        // 带参数：`|` 后的各段要真的进到渲染结果里，否则等于参数被默默丢掉
        let 带参结果 = 渲染问题("mc_ok_with_warnings|3");
        assert_ne!(带参结果, "mc_ok_with_warnings", "应有模板而非回退键名");
        assert!(带参结果.contains("3"), "警告数未注入模板：{带参结果:?}");
        let 阴影结果 = 渲染问题("mc_stdlib_shadow|a.toml|键|a|b");
        for 参数项 in ["a.toml", "键", "a", "b"] {
            assert!(
                阴影结果.contains(参数项),
                "参数 {参数项:?} 被丢弃：{阴影结果:?}"
            );
        }
        // 未知消息码：回退键本身（既不崩溃，也不渲染成空串吞掉整行）
        assert_eq!(渲染问题("not_a_real_code|x"), "not_a_real_code");
        // 空串：不崩溃，按模板回退链返回空（分割的首段就是整串）
        assert_eq!(渲染问题(""), "");
    }

    /// 骨架：首次写入并带目标语待办注释（英文值保留）；重跑跳过已存在文件
    #[test]
    fn 测试骨架写入后跳过() {
        let 目录值 = tempfile::tempdir().unwrap();
        let 输出目录 = 目录值.path().join("fr");
        let 源视图 =
            视图含三方库(vec![("a.toml", "[\"标识符\"]\n\"服务器\" = \"Server\"\n")]);
        let (已创建数, 已跳过数) = 生成骨架(&源视图, "fr", &输出目录).expect("首次生成应成功");
        assert_eq!((已创建数, 已跳过数), (1, 0));
        let 内容文本 = std::fs::read_to_string(输出目录.join("a.toml")).unwrap();
        assert!(
            内容文本.contains("TODO(fr)"),
            "应追加目标语待办：{内容文本}"
        );
        assert!(
            内容文本.contains("\"Server\""),
            "英文值必须保留：{内容文本}"
        );
        let (已创建二, 已跳过二) = 生成骨架(&源视图, "fr", &输出目录).expect("重跑应成功");
        assert_eq!((已创建二, 已跳过二), (0, 1), "重跑必须幂等跳过");
    }

    /// 从三方库目录构建视图：目录内关键字/标准库优先；
    /// 缺省时回退源视图（三方库内容始终从目标目录读取）
    #[test]
    fn 测试视图从三方库目录覆盖与回退() {
        let 源视图 = 视图含三方库(vec![("src.toml", "[\"标识符\"]\n\"源\" = \"Src\"\n")]);
        // 覆盖路径：目标目录自带关键字配置
        let 目录值 = tempfile::tempdir().unwrap();
        let 语言包目录 = 目录值.path().join("de");
        let 三方库目录 = 语言包目录.join("crates");
        std::fs::create_dir_all(&三方库目录).unwrap();
        std::fs::write(
            三方库目录.join("a.toml"),
            "[\"标识符\"]\n\"服务器\" = \"Server\"\n",
        )
        .unwrap();
        std::fs::write(语言包目录.join("keywords.toml"), "# 自定义关键字\n").unwrap();
        let 视图 = 从三方库目录构建视图(&三方库目录, &源视图);
        assert_eq!(视图.语言代码, "de");
        assert_eq!(视图.三方库表.len(), 1);
        assert_eq!(视图.关键字内容, "# 自定义关键字\n");
        // 回退路径：空语言包目录（无三方库、无关键字）→ 回退源视图语言表
        let 目录二 = tempfile::tempdir().unwrap();
        let 语言包目录二 = 目录二.path().join("xx");
        let 三方库目录二 = 语言包目录二.join("crates");
        std::fs::create_dir_all(&三方库目录二).unwrap();
        let 回退 = 从三方库目录构建视图(&三方库目录二, &源视图);
        assert_eq!(回退.语言代码, "xx");
        assert!(回退.三方库表.is_empty());
        assert!(回退.关键字内容.contains("函数"));
    }

    /// 运行检查：内置语言代码走内置视图（zh 干净 → 真）；
    /// 未知目标（非目录非内置）报错；外部目录缺关键字配置报错
    #[test]
    fn 测试运行检查目标分支() {
        assert!(
            运行检查(Some("zh")).expect("内置 zh 应可校验"),
            "内置 zh 基线必须通过"
        );
        assert!(运行检查(Some("no-such-lang-xyz")).is_err());
        let 目录值 = tempfile::tempdir().unwrap();
        assert!(
            运行检查(目录值.path().to_str()).is_err(),
            "缺关键字配置应报错"
        );
    }
}
