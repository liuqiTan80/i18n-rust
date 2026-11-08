//! 后端源码语料覆盖矩阵（`rzc mapping coverage`）
//!
//! 将仓库内后端真实 Rust 源码（引擎 / 命令行 / 语言服务三方的源码）作为真实语料，
//! 对每个语言包检验「用户写代码时会遇到的名字」的覆盖度，自动列出缺失的母语映射：
//! - 关键字：缺失时用户无法用母语写出该语法结构（错误级）
//! - 接口名（标准库/核心库/分配库路径段、使用导入、首字母大写类型名）：缺失时只能
//!   写英文原名，全母语体验打折（警告级）
//!
//! 用法：在仓库根目录运行 `rzc mapping coverage [--lang <代码>]`（默认全部内置语言）。
//! 语料来自仓库内后端三个包的源码目录；发布版（无源码）环境下不可用，
//! 此命令面向语言包维护者与持续集成门禁。

use std::collections::HashMap;
use std::path::Path;

use i18n_rust_engine::映射源;

/// 一个名字的出现统计（出现次数 + 示例文件）
use anyhow::anyhow;

type 名称统计 = (usize, String);

/// 语料提取结果：四类名字 → （出现次数, 示例文件）
#[derive(Default)]
struct 语料库 {
    /// 关键字
    关键字表: HashMap<String, 名称统计>,
    /// 标准库/核心库/分配库路径段（模块名与类型名）
    标准库段: HashMap<String, 名称统计>,
    /// 使用导入的第三方包名
    外部包名: HashMap<String, 名称统计>,
    /// 首字母大写标识符（类型名，路径链之外）
    类型名: HashMap<String, 名称统计>,
}

/// Rust 稳定关键字全集（方言母语映射应覆盖；词法器不区分关键字，需自行匹配）
const 稳定关键字表: &[&str] = &[
    "as", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn",
    "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
    "use", "where", "while", "async", "await", "union",
];

impl 语料库 {
    fn 记录(映射表: &mut HashMap<String, 名称统计>, 包名: &str, 来源标记: &str) {
        if let Some(统计项) = 映射表.get_mut(&包名.to_string()) {
            统计项.0 += 1;
        } else {
            映射表.insert(包名.to_string(), (1, 来源标记.to_string()));
        }
    }
}

/// 收集仓库内后端源码（引擎、命令行、语言服务三方的源码目录），返回（相对路径, 内容）
fn 收集后端源码(根目录: &Path) -> Vec<(String, String)> {
    let mut 文件清单 = Vec::new();
    for 目录名 in ["engine", "cli", "lsp"] {
        let 源目录 = 根目录.join("crates").join(目录名).join("src");
        let Ok(目录项流) = std::fs::read_dir(&源目录) else {
            continue;
        };
        for 目录条目 in 目录项流.flatten() {
            let 路径值 = 目录条目.path();
            if 路径值.extension().and_then(|后缀| 后缀.to_str()) != Some("rs") {
                continue;
            }
            if let Ok(内容文本) = std::fs::read_to_string(&路径值) {
                文件清单.push((
                    format!(
                        "crates/{}/src/{}",
                        目录名,
                        路径值.file_name().unwrap_or_default().to_string_lossy()
                    ),
                    内容文本,
                ));
            }
        }
    }
    文件清单.sort();
    文件清单
}

/// 下一个非空白词元的种类（词法器将 `::` 拆为两个冒号，中间允许空白）
fn 下一非空白种类(
    令牌们: &[rustc_lexer::Token],
    起点位: usize,
) -> Option<rustc_lexer::TokenKind> {
    令牌们[起点位..]
        .iter()
        .map(|令牌| 令牌.kind)
        .find(|种类值| *种类值 != rustc_lexer::TokenKind::Whitespace)
}

/// 前一个非空白词元的种类（起点位为当前词元索引）
fn 前一非空白种类(
    令牌们: &[rustc_lexer::Token],
    起点位: usize,
) -> Option<rustc_lexer::TokenKind> {
    令牌们[..起点位]
        .iter()
        .rev()
        .map(|令牌| 令牌.kind)
        .find(|种类值| *种类值 != rustc_lexer::TokenKind::Whitespace)
}

/// 从单个源文件提取语料名字（单遍词法器扫描，无语法树依赖）
///
/// 状态机：
/// - `使用中`：使用语句内（到 `;` 结束）
/// - `路径模式`：当前 `::` 路径链（含暂存的首段）；
///   标识符先暂存进链，遇到 `::` 才确认是路径（否则按普通标识符处理）
/// - 使用内的 `as X` 别名跳过；内部路径（包、父模块、自身）忽略
fn 从源码提取名称(来源标记: &str, 源文本: &str, 语料: &mut 语料库) {
    let 令牌们: Vec<rustc_lexer::Token> = rustc_lexer::tokenize(源文本).collect();
    // 逐词元的字节偏移（词法器词法流覆盖全源，偏移连续）
    let mut 偏移们: Vec<usize> = Vec::with_capacity(令牌们.len());
    let mut 累计 = 0;
    for 令牌 in &令牌们 {
        偏移们.push(累计);
        累计 += 令牌.len;
    }

    let mut 使用中 = false;
    let mut 路径链: Vec<String> = Vec::new();
    let mut 路径模式 = false;
    let mut 跳过下一使用标识 = false;

    for (序号, 令牌) in 令牌们.iter().enumerate() {
        let 文本串 = &源文本[偏移们[序号]..偏移们[序号] + 令牌.len];
        match 令牌.kind {
            rustc_lexer::TokenKind::Ident => {
                if 稳定关键字表.contains(&文本串) {
                    match 文本串 {
                        "use" if !使用中 => {
                            使用中 = true;
                            路径链.clear();
                            路径模式 = false;
                        }
                        "as" if 使用中 => {
                            跳过下一使用标识 = true;
                        }
                        // 内部路径首段（包、父模块、自身加双冒号）：入链整体忽略，
                        // 避免包名被冲刷后其后的内部模块名误入外部包名表；
                        // 单独出现的自身仍按普通关键字处理
                        "crate" | "super" | "self" | "Self"
                            if 下一非空白种类(&令牌们, 序号 + 1)
                                == Some(rustc_lexer::TokenKind::Colon) =>
                        {
                            路径链.push(文本串.to_string());
                            路径模式 = false;
                        }
                        // 普通关键字：结束当前链
                        _ => {
                            if !使用中 && !["crate", "super", "self", "Self"].contains(&文本串)
                            {
                                语料库::记录(&mut 语料.关键字表, 文本串, 来源标记);
                            }
                            冲刷路径链(&mut 路径链, 路径模式, 使用中, 语料, 来源标记);
                            使用中 = false;
                            路径模式 = false;
                        }
                    }
                } else if 跳过下一使用标识 {
                    跳过下一使用标识 = false;
                    路径链.clear();
                    路径模式 = false;
                } else if 使用中 || 路径模式 {
                    路径链.push(文本串.to_string());
                } else {
                    // 暂存：等下一个词元决定是路径链还是普通标识符
                    路径链.push(文本串.to_string());
                    路径模式 = false;
                }
            }
            rustc_lexer::TokenKind::Colon => {
                // 词法器将 `::` 拆为两个冒号（中间允许空白）：
                // 第一半前瞻下一个非空白仍是冒号；第二半（前一个非空白
                // 是冒号）保持路径链，仅单冒号（类型标注等）结束当前链
                let 前是冒号 = 前一非空白种类(&令牌们, 序号) == Some(rustc_lexer::TokenKind::Colon);
                let 后是冒号 =
                    下一非空白种类(&令牌们, 序号 + 1) == Some(rustc_lexer::TokenKind::Colon);
                if 前是冒号 {
                    // `::` 第二半：路径链继续（路径模式已由第一半设置）
                } else if 后是冒号 {
                    路径模式 = true;
                } else {
                    // 单个冒号（类型标注等）：结束当前链
                    冲刷路径链(&mut 路径链, 路径模式, 使用中, 语料, 来源标记);
                    路径模式 = false;
                }
            }
            rustc_lexer::TokenKind::Semi => {
                冲刷路径链(&mut 路径链, 路径模式, 使用中, 语料, 来源标记);
                使用中 = false;
                路径模式 = false;
            }
            _ => {
                // 其他词元：结束当前链；若链中只有一个暂存标识符，按普通标识符处理
                冲刷路径链(&mut 路径链, 路径模式, 使用中, 语料, 来源标记);
                路径模式 = false;
            }
        }
    }
    冲刷路径链(&mut 路径链, 路径模式, 使用中, 语料, 来源标记);
}

/// 结束当前路径链：按使用/非使用与首段归属分类记录到语料
fn 冲刷路径链(
    路径链: &mut Vec<String>,
    路径模式: bool,
    使用中: bool,
    语料: &mut 语料库,
    来源标记: &str,
) {
    if 路径链.is_empty() {
        return;
    }
    // 非路径链的单个暂存标识符：首字母大写且非单字符 → 类型名
    if !路径模式 && !使用中 {
        let 包名 = &路径链[0];
        let 首字 = 包名.chars().next().unwrap_or('a');
        if 首字.is_uppercase() && 包名.len() > 1 {
            语料库::记录(&mut 语料.类型名, 包名, 来源标记);
        }
        路径链.clear();
        return;
    }
    let 首段 = &路径链[0];
    let 是内部路径 = ["crate", "super", "self"].contains(&首段.as_str());
    let 是标准库路径 = ["std", "core", "alloc"].contains(&首段.as_str());
    if !是内部路径 {
        if 是标准库路径 || 使用中 {
            // 标准库路径引用 / 使用导入：段全部记录（模块名与类型名）
            for 段 in 路径链.iter() {
                if 是标准库路径 {
                    语料库::记录(&mut 语料.标准库段, 段, 来源标记);
                }
            }
        }
        // 首段（包名）：使用导入或路径引用的第三方包；
        // 惯例上包名小写、类型名大写
        if !是标准库路径 {
            let 首字 = 首段.chars().next().unwrap_or('a');
            if 首字.is_uppercase() {
                语料库::记录(&mut 语料.类型名, 首段, 来源标记);
            } else {
                语料库::记录(&mut 语料.外部包名, 首段, 来源标记);
            }
        }
        // 使用内的非首段：大写段是类型名
        if 使用中 && !是标准库路径 {
            for 段 in 路径链.iter().skip(1) {
                let 首字 = 段.chars().next().unwrap_or('a');
                if 首字.is_uppercase() && 段.len() > 1 {
                    语料库::记录(&mut 语料.类型名, 段, 来源标记);
                }
            }
        }
        // 非使用的第三方路径引用：后续段按类型名/忽略
        if !使用中 && !是标准库路径 {
            for 段 in 路径链.iter().skip(1) {
                let 首字 = 段.chars().next().unwrap_or('a');
                if 首字.is_uppercase() && 段.len() > 1 {
                    语料库::记录(&mut 语料.类型名, 段, 来源标记);
                }
            }
        }
    }
    路径链.clear();
}

/// 构建语言包反向索引：英文原名 → 母语键
///
/// 合并关键字配置（排除派生特征节，与运行时语义一致）、模块路径配置、
/// 标准库配置、三方库映射的全部映射值；同值多键时保留首个（报告无需区分）。
fn 构建反向索引(
    关键字内容: &str,
    模块路径内容: &str,
    标准库内容: &str,
    三方库数据: &[(&str, &str)],
) -> HashMap<String, String> {
    let mut 反向表: HashMap<String, String> = HashMap::new();
    let 置入反向 = |反向表: &mut HashMap<String, String>, 值项: &str, 键名: &str| {
        if !值项.is_empty() {
            反向表
                .entry(值项.to_string())
                .or_insert_with(|| 键名.to_string());
        }
    };
    if let Ok(各节) = 映射源::解析映射节(关键字内容) {
        for (节名, 映射表) in &各节 {
            if 节名 == "派生特征" || 映射源::是否元数据节(节名) {
                continue;
            }
            for (键名, 值项) in 映射表 {
                置入反向(&mut 反向表, 值项, 键名);
            }
        }
    }
    for 配置文本 in [模块路径内容, 标准库内容] {
        if let Ok(各节) = 映射源::解析映射节(配置文本) {
            for (节名, 映射表) in &各节 {
                if 映射源::是否元数据节(节名) {
                    continue;
                }
                for (键名, 值项) in 映射表 {
                    置入反向(&mut 反向表, 值项, 键名);
                }
            }
        }
    }
    for (_, 内容文本) in 三方库数据 {
        if let Ok(各节) = 映射源::解析映射节(内容文本) {
            for (节名, 映射表) in &各节 {
                if 映射源::是否元数据节(节名) {
                    continue;
                }
                for (键名, 值项) in 映射表 {
                    置入反向(&mut 反向表, 值项, 键名);
                }
            }
        }
    }
    反向表
}

/// 单个语言包的覆盖报告
pub struct 覆盖报告 {
    /// 语言代码
    pub 语言代码: String,
    /// 缺失关键字（错误级）：（名字, 出现次数, 示例文件）
    pub 缺失关键字: Vec<(String, usize, String)>,
    /// 缺失标准库段（警告级，用户写代码最常遇到）
    pub 缺失标准库段: Vec<(String, usize, String)>,
    /// 缺失第三方包名（警告级，用外部库时需要）
    pub 缺失第三方包: Vec<(String, usize, String)>,
    /// 缺失类型名（警告级，多为编译器内部实现，可按需忽略）
    pub 缺失类型: Vec<(String, usize, String)>,
    /// 覆盖统计
    pub 总名称数: usize,
    pub 已覆盖数: usize,
}

/// 按出现次数降序排序缺失清单（同次数按名字升序，稳定输出）
fn 排序缺失(列表: &mut [(String, usize, String)]) {
    列表.sort_by(|甲, 乙| 乙.1.cmp(&甲.1).then(甲.0.cmp(&乙.0)));
}

/// 用反向索引检测语料覆盖，产出报告
fn 计算覆盖(
    语料: &语料库, 反向表: &HashMap<String, String>, 语言代码: &str
) -> 覆盖报告 {
    let mut 报告值 = 覆盖报告 {
        语言代码: 语言代码.to_string(),
        缺失关键字: Vec::new(),
        缺失标准库段: Vec::new(),
        缺失第三方包: Vec::new(),
        缺失类型: Vec::new(),
        总名称数: 0,
        已覆盖数: 0,
    };
    // 关键字缺失（错误级）
    for (条目名, (次数, 来源标记)) in &语料.关键字表 {
        报告值.总名称数 += 1;
        if !反向表.contains_key(条目名) {
            报告值
                .缺失关键字
                .push((条目名.clone(), *次数, 来源标记.clone()));
        } else {
            报告值.已覆盖数 += 1;
        }
    }
    // 接口缺失（警告级）：标准库段 / 外部包 / 类型名分三组
    for (条目名, (次数, 来源标记)) in &语料.标准库段 {
        报告值.总名称数 += 1;
        if !反向表.contains_key(条目名) {
            报告值
                .缺失标准库段
                .push((条目名.clone(), *次数, 来源标记.clone()));
        } else {
            报告值.已覆盖数 += 1;
        }
    }
    for (条目名, (次数, 来源标记)) in &语料.外部包名 {
        报告值.总名称数 += 1;
        if !反向表.contains_key(条目名) {
            报告值
                .缺失第三方包
                .push((条目名.clone(), *次数, 来源标记.clone()));
        } else {
            报告值.已覆盖数 += 1;
        }
    }
    for (条目名, (次数, 来源标记)) in &语料.类型名 {
        报告值.总名称数 += 1;
        if !反向表.contains_key(条目名) {
            报告值
                .缺失类型
                .push((条目名.clone(), *次数, 来源标记.clone()));
        } else {
            报告值.已覆盖数 += 1;
        }
    }
    排序缺失(&mut 报告值.缺失关键字);
    排序缺失(&mut 报告值.缺失标准库段);
    排序缺失(&mut 报告值.缺失第三方包);
    排序缺失(&mut 报告值.缺失类型);
    报告值
}

/// 打印单个语言包的覆盖报告（本地化输出）
fn 打印覆盖报告(报告值: &覆盖报告, 源码数: usize) {
    let 界面表 = crate::界面::界面消息表::全局表();
    println!(
        "{}",
        界面表.取带参消息("mc_cov_header", &[&报告值.语言代码])
    );
    let 百分比 = 报告值
        .已覆盖数
        .checked_mul(100)
        .and_then(|数| 数.checked_div(报告值.总名称数))
        .unwrap_or(0);
    println!(
        "{}",
        界面表.取带参消息(
            "mc_cov_stats",
            &[
                &报告值.已覆盖数.to_string(),
                &报告值.总名称数.to_string(),
                &百分比.to_string(),
                &源码数.to_string()
            ]
        )
    );
    // 关键字缺失（错误级，最多展示 30 条）
    let 关键字总数 = 报告值.缺失关键字.len();
    for (条目名, 次数, 示例路径) in 报告值.缺失关键字.iter().take(30) {
        println!(
            "{}",
            界面表.取带参消息("mc_cov_kw_missing", &[条目名, &次数.to_string(), 示例路径])
        );
    }
    if 关键字总数 > 30 {
        println!(
            "{}",
            界面表.取带参消息("mc_cov_more", &[&(关键字总数 - 30).to_string()])
        );
    }
    // 标准库段缺失（警告级，最多展示 25 条）：用户写代码最常遇到，优先补齐
    let 标准库总数 = 报告值.缺失标准库段.len();
    for (条目名, 次数, 示例路径) in 报告值.缺失标准库段.iter().take(25) {
        println!(
            "{}",
            界面表.取带参消息("mc_cov_std_missing", &[条目名, &次数.to_string(), 示例路径])
        );
    }
    if 标准库总数 > 25 {
        println!(
            "{}",
            界面表.取带参消息("mc_cov_more", &[&(标准库总数 - 25).to_string()])
        );
    }
    // 第三方包缺失（警告级，最多展示 15 条）
    let 第三方总数 = 报告值.缺失第三方包.len();
    for (条目名, 次数, 示例路径) in 报告值.缺失第三方包.iter().take(15) {
        println!(
            "{}",
            界面表.取带参消息(
                "mc_cov_crate_missing",
                &[条目名, &次数.to_string(), 示例路径]
            )
        );
    }
    if 第三方总数 > 15 {
        println!(
            "{}",
            界面表.取带参消息("mc_cov_more", &[&(第三方总数 - 15).to_string()])
        );
    }
    // 类型名缺失（警告级，最多展示 15 条；多为编译器内部实现，可按需忽略）
    let 类型总数 = 报告值.缺失类型.len();
    for (条目名, 次数, 示例路径) in 报告值.缺失类型.iter().take(15) {
        println!(
            "{}",
            界面表.取带参消息(
                "mc_cov_type_missing",
                &[条目名, &次数.to_string(), 示例路径]
            )
        );
    }
    if 类型总数 > 15 {
        println!(
            "{}",
            界面表.取带参消息("mc_cov_more", &[&(类型总数 - 15).to_string()])
        );
    }
    if 关键字总数 == 0 {
        println!("{}", 界面表.取消息("mc_cov_kw_ok"));
    }
    println!();
}

/// 「映射覆盖」命令入口
///
/// - 语言参数为空值：检测全部内置语言
/// - 语言参数为内置语言代码：仅检测该语言
///
/// 返回是否全部语言关键字覆盖（错误级）通过。
pub fn 运行覆盖检查(语言参数: Option<&str>) -> anyhow::Result<bool> {
    let 界面表 = crate::界面::界面消息表::全局表();
    // 语料必须来自源码仓库（发布版无后端源码目录结构）
    let 工作目录 =
        std::env::current_dir().map_err(|错误值| anyhow!("获取当前目录失败: {}", 错误值))?;
    let 项目根 = crate::查找项目根向上(&工作目录)
        .ok_or_else(|| anyhow!("{}", 界面表.取消息("mc_cov_no_repo")))?;
    let 源码清单 = 收集后端源码(&项目根);
    if 源码清单.is_empty() {
        anyhow::bail!("{}", 界面表.取消息("mc_cov_no_repo"));
    }
    // 提取语料（一次，供所有语言包复用）
    let mut 语料 = 语料库::default();
    for (来源标记, 内容文本) in &源码清单 {
        从源码提取名称(来源标记, 内容文本, &mut 语料);
    }
    println!("{}", 界面表.取消息("mc_cov_corpus_intro"));
    let 语言清单: Vec<&str> = match 语言参数 {
        Some(单语言) => vec![单语言],
        None => crate::内置语言::内置语言清单(),
    };
    let mut 全部通过 = true;
    for 语言代码 in 语言清单 {
        if !crate::内置语言::有内置语言包(语言代码) {
            anyhow::bail!("{}", 界面表.取带参消息("mc_unknown_target", &[语言代码]));
        }
        let 数据 = crate::内置语言::获取内置数据(语言代码);
        let 反向表 = 构建反向索引(
            数据.关键字内容,
            数据.模块路径内容,
            数据.标准库内容,
            数据.三方库数据,
        );
        let 报告值 = 计算覆盖(&语料, &反向表, 语言代码);
        全部通过 &= 报告值.缺失关键字.is_empty();
        打印覆盖报告(&报告值, 源码清单.len());
    }
    if 全部通过 {
        println!("{}", 界面表.取消息("mc_cov_all_ok"));
    }
    Ok(全部通过)
}

#[cfg(test)]
mod 母语测试 {
    use super::*;

    /// 单文件语料提取：关键字 / 标准库路径段 / 第三方包 / 类型名
    #[test]
    fn 测试提取名称基础() {
        let 源码 = r#"
use std::collections::HashMap;
use serde::Serialize;
use crate::toolchain::find_toolchain;

pub fn main() -> Result<(), Box<dyn Error>> {
    let map: HashMap<String, Vec<u8>> = HashMap::new();
    std::fs::read_to_string("x")?;
    ureq::get("https://x").call()?;
    let server = MyServer::new();
    Ok(())
}
"#;
        let mut 语料 = 语料库::default();
        从源码提取名称("t.rs", 源码, &mut 语料);
        // 关键字
        for 关键字值 in ["fn", "let", "pub", "dyn"] {
            assert!(
                语料.关键字表.contains_key(关键字值),
                "缺少关键字 {关键字值}"
            );
        }
        assert!(!语料.关键字表.contains_key("use"), "use 不应作为关键字语料");
        // 标准库路径段
        for 段 in ["std", "collections", "HashMap", "fs", "read_to_string"] {
            assert!(语料.标准库段.contains_key(段), "缺少标准库段 {段}");
        }
        // 第三方包名
        for 包名 in ["serde", "ureq"] {
            assert!(语料.外部包名.contains_key(包名), "缺少包 {包名}");
        }
        // 内部路径忽略（引擎包名是外部包，包前缀路径忽略）
        assert!(!语料.外部包名.contains_key("crate"), "包前缀路径不应计入");
        // 类型名：使用段中的大写 + 普通大写标识符
        assert!(语料.类型名.contains_key("Serialize"));
        assert!(语料.类型名.contains_key("MyServer"));
        assert!(语料.类型名.contains_key("Error"), "错误特征的类型名");
        assert!(
            !语料.外部包名.contains_key("HashMap"),
            "裸类型构造是类型引用而非包"
        );
        assert!(
            语料.标准库段.contains_key("HashMap"),
            "使用路径链中的 HashMap 计入标准库段"
        );
        // 非使用的内部路径调用整体忽略，内部模块名不入外部包名
        let 源码二 = "fn f() { crate::ui::Ui::global(); super::helper::run(); self::x::y(); }\n";
        let mut 语料二 = 语料库::default();
        从源码提取名称("t2.rs", 源码二, &mut 语料二);
        for 内部段 in ["ui", "Ui", "helper", "run", "x", "y"] {
            assert!(
                !语料二.外部包名.contains_key(内部段) && !语料二.类型名.contains_key(内部段),
                "内部路径段 {内部段} 不应计入"
            );
        }
    }

    /// 使用别名与泛型单字符不进入语料
    #[test]
    fn 测试提取名称别名与泛型() {
        let 源码 = "use std::fmt::Result as FmtResult;\nfn foo<T>(x: T) -> Result<T> { Ok(x) }\n";
        let mut 语料 = 语料库::default();
        从源码提取名称("t.rs", 源码, &mut 语料);
        assert!(!语料.类型名.contains_key("FmtResult"), "别名应跳过");
        assert!(!语料.类型名.contains_key("T"), "单字符泛型参数不应计入");
        assert!(语料.标准库段.contains_key("fmt"));
        assert!(语料.关键字表.contains_key("fn"));
        assert!(
            语料.类型名.contains_key("Result"),
            "返回类型中的真实类型引用应记录"
        );
    }

    /// 反向索引：合并关键字/标准库/模块路径/三方库的值
    #[test]
    fn 测试构建反向索引() {
        let 关键字内容 = "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n[\"派生特征\"]\n\"调试\" = \"Debug\"\n";
        let 模块路径内容 = "[\"模块路径\"]\n\"标准库\" = \"std\"\n";
        let 标准库内容 = "[\"标识符\"]\n\"字符串\" = \"String\"\n";
        let 三方库数据 = [(
            "serde.toml",
            "[\"模块路径\"]\n\"序列化\" = \"serde\"\n[\"标识符\"]\n\"序列化特征\" = \"Serialize\"\n",
        )];
        let 反向表 = 构建反向索引(关键字内容, 模块路径内容, 标准库内容, &三方库数据);
        assert_eq!(反向表.get("fn").map(String::as_str), Some("函数"));
        assert_eq!(反向表.get("let").map(String::as_str), Some("让"));
        assert_eq!(反向表.get("std").map(String::as_str), Some("标准库"));
        assert_eq!(反向表.get("String").map(String::as_str), Some("字符串"));
        assert_eq!(反向表.get("serde").map(String::as_str), Some("序列化"));
        assert_eq!(
            反向表.get("Serialize").map(String::as_str),
            Some("序列化特征")
        );
        assert!(
            !反向表.contains_key("Debug"),
            "派生特征节不应并入关键字索引"
        );
    }

    /// 覆盖检测：关键字缺失入错误列表，接口缺失入警告列表
    #[test]
    fn 测试计算覆盖() {
        let 关键字内容 = "[\"声明\"]\n\"函数\" = \"fn\"\n";
        let 模块路径内容 = "[\"模块路径\"]\n\"标准库\" = \"std\"\n";
        let 反向表 = 构建反向索引(关键字内容, 模块路径内容, "", &[]);
        let mut 语料 = 语料库::default();
        语料
            .关键字表
            .insert("fn".to_string(), (3, "a.rs".to_string()));
        语料
            .关键字表
            .insert("unsafe".to_string(), (1, "b.rs".to_string()));
        语料
            .标准库段
            .insert("std".to_string(), (9, "a.rs".to_string()));
        语料
            .类型名
            .insert("NotCovered".to_string(), (2, "b.rs".to_string()));
        let 报告值 = 计算覆盖(&语料, &反向表, "zh");
        assert!(
            报告值
                .缺失关键字
                .iter()
                .any(|(条目名, ..)| 条目名 == "unsafe")
        );
        assert!(!报告值.缺失关键字.iter().any(|(条目名, ..)| 条目名 == "fn"));
        assert!(
            报告值
                .缺失类型
                .iter()
                .any(|(条目名, ..)| 条目名 == "NotCovered")
        );
        assert!(
            !报告值
                .缺失标准库段
                .iter()
                .any(|(条目名, ..)| 条目名 == "std")
        );
        assert_eq!(报告值.总名称数, 4);
        assert_eq!(报告值.已覆盖数, 2);
    }

    /// 关键字门禁：全部内置语言包必须覆盖 Rust 稳定关键字全集（错误级）——
    /// 任一语言缺任一关键字（用户无法用母语写出该语法结构）都会在此失败
    #[test]
    fn 测试全部内置语言关键字齐全() {
        let mut 失败清单: Vec<String> = Vec::new();
        for 语言代码 in crate::内置语言::内置语言清单() {
            let 数据 = crate::内置语言::获取内置数据(语言代码);
            let 反向表 = 构建反向索引(数据.关键字内容, "", "", &[]);
            let 缺失: Vec<&str> = 稳定关键字表
                .iter()
                .copied()
                .filter(|关键字值| !反向表.contains_key(*关键字值))
                .collect();
            if !缺失.is_empty() {
                失败清单.push(format!("{} 缺 {:?}", 语言代码, 缺失));
            }
        }
        assert!(失败清单.is_empty(), "关键字门禁失败: {:?}", 失败清单);
    }

    /// 全内置语言跑一遍覆盖检测（不崩溃、有统计）
    #[test]
    fn 测试全部语言覆盖不崩溃() {
        for 语言代码 in crate::内置语言::内置语言清单() {
            let 数据 = crate::内置语言::获取内置数据(语言代码);
            let 反向表 = 构建反向索引(
                数据.关键字内容,
                数据.模块路径内容,
                数据.标准库内容,
                数据.三方库数据,
            );
            // 用最小语料冒烟：语料为空时统计为 0 即可
            let 语料 = 语料库::default();
            let 报告值 = 计算覆盖(&语料, &反向表, 语言代码);
            assert_eq!(报告值.总名称数, 0, "{语言代码} 空语料统计应为 0");
            assert!(报告值.缺失关键字.is_empty());
        }
    }
}
