//! 后端源码语料覆盖矩阵（`rzc mapping coverage`）
//!
//! 将仓库内后端真实 Rust 源码（engine / cli / lsp 的 src/*.rs）作为真实语料，
//! 对每个语言包检验「用户写代码时会遇到的名字」的覆盖度，自动列出缺失的母语映射：
//! - 关键字（fn/let/...）：缺失时用户无法用母语写出该语法结构（error 级）
//! - API 名（std/core/alloc 路径段、use 导入、首字母大写类型名）：缺失时只能
//!   写英文原名，全母语体验打折（warning 级）
//!
//! 用法：在仓库根目录运行 `rzc mapping coverage [--lang <代码>]`（默认全部内置语言）。
//! 语料来自仓库内 crates/{engine,cli,lsp}/src/*.rs；发布版（无源码）环境下不可用，
//! 此命令面向语言包维护者与 CI 门禁。

use std::collections::HashMap;
use std::path::Path;

/// 一个名字的出现统计（出现次数 + 示例文件）
type 名字统计 = (usize, String);

/// 语料提取结果：四类名字 → (出现次数, 示例文件)
#[derive(Default)]
struct 语料 {
    /// 关键字（fn/let/...）
    关键字表: HashMap<String, 名字统计>,
    /// std/core/alloc 路径段（模块名与类型名）
    标准库段表: HashMap<String, 名字统计>,
    /// use 导入的第三方 crate 名
    外部库表: HashMap<String, 名字统计>,
    /// 首字母大写标识符（类型名，路径链之外）
    类型名表: HashMap<String, 名字统计>,
}

/// Rust 稳定关键字全集（方言母语映射应覆盖；rustc_lexer 不区分关键字，需自行匹配）
const 关键字全集: &[&str] = &[
    "as", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn",
    "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
    "use", "where", "while", "async", "await", "union",
];

impl 语料 {
    fn 记录(表: &mut HashMap<String, 名字统计>, 名字: &str, file: &str) {
        表.entry(名字.to_string())
            .and_modify(|(count, _)| *count += 1)
            .or_insert((1, file.to_string()));
    }
}

/// 收集仓库内后端源码（engine/cli/lsp 的 src/*.rs），返回 (相对路径, 内容)
fn 收集后端源码(根: &Path) -> Vec<(String, String)> {
    let mut 文件表 = Vec::new();
    for 目录名 in ["engine", "cli", "lsp"] {
        let 源目录 = 根.join("crates").join(目录名).join("src");
        let Ok(项序列) = std::fs::read_dir(&源目录) else {
            continue;
        };
        for 项 in 项序列.flatten() {
            let 路径值 = 项.path();
            if 路径值.extension().and_then(|扩展| 扩展.to_str()) != Some("rs") {
                continue;
            }
            if let Ok(内容) = std::fs::read_to_string(&路径值) {
                文件表.push((
                    format!(
                        "crates/{目录名}/src/{}",
                        路径值.file_name().unwrap_or_default().to_string_lossy()
                    ),
                    内容,
                ));
            }
        }
    }
    文件表.sort();
    文件表
}

/// 下一个非空白 token 的种类（rustc_lexer 将 `::` 拆为两个 Colon，中间允许空白）
fn 下个非空白种类(
    令牌: &[rustc_lexer::Token],
    from: usize,
) -> Option<rustc_lexer::TokenKind> {
    令牌[from..]
        .iter()
        .map(|词元项| 词元项.kind)
        .find(|词元类别| !matches!(词元类别, rustc_lexer::TokenKind::Whitespace))
}

/// 前一个非空白 token 的种类（从 为当前 token 索引）
fn 上个非空白种类(
    令牌: &[rustc_lexer::Token],
    from: usize,
) -> Option<rustc_lexer::TokenKind> {
    令牌[..from]
        .iter()
        .rev()
        .map(|词元项| 词元项.kind)
        .find(|词元类别| !matches!(词元类别, rustc_lexer::TokenKind::Whitespace))
}

/// 从单个源文件提取语料名字（单遍 rustc_lexer 扫描，无语法树依赖）
///
/// 状态机：
/// - `处于导入`：use 语句内（到 `;` 结束）
/// - `路径链` / `路径态`：当前 `::` 路径链（含暂存的首段）；
///   标识符先暂存进链，遇到 `::` 才确认是路径（否则按普通标识符处理）
/// - use 内的 `as X` 别名跳过；`crate::`/`super::`/`self::` 内部路径忽略
fn 从源提取名字(file: &str, 源: &str, 语料: &mut 语料) {
    let 令牌流: Vec<rustc_lexer::Token> = rustc_lexer::tokenize(源).collect();
    // 逐 token 的字节偏移（rustc_lexer 词法流覆盖全源，偏移连续）
    let mut 偏移表: Vec<usize> = Vec::with_capacity(令牌流.len());
    let mut 累计 = 0usize;
    for 令牌项 in &令牌流 {
        偏移表.push(累计);
        累计 += 令牌项.len;
    }

    let mut 处于导入 = false;
    let mut 路径链: Vec<String> = Vec::new();
    let mut 路径态 = false;
    let mut 跳过下个导入名 = false;

    for (索引, 令牌) in 令牌流.iter().enumerate() {
        let 字串 = &源[偏移表[索引]..偏移表[索引] + 令牌.len];
        match 令牌.kind {
            rustc_lexer::TokenKind::Ident => {
                if 关键字全集.contains(&字串) {
                    match 字串 {
                        "use" if !处于导入 => {
                            处于导入 = true;
                            路径链.clear();
                            路径态 = false;
                        }
                        "as" if 处于导入 => {
                            跳过下个导入名 = true;
                        }
                        // 内部路径首段（crate::/super::/self::）：入链整体忽略，
                        // 避免 crate 被 flush 后其后的内部模块名（本地化/内置语言 等）
                        // 误入 外部库表；单独出现的 self 仍按普通关键字处理
                        "crate" | "super" | "self" | "Self"
                            if matches!(
                                下个非空白种类(&令牌流, 索引 + 1),
                                Some(rustc_lexer::TokenKind::Colon)
                            ) =>
                        {
                            路径链.push(字串.to_string());
                            路径态 = false;
                        }
                        // 普通关键字：结束当前链
                        _ => {
                            if !处于导入 && !matches!(字串, "crate" | "super" | "self" | "Self")
                            {
                                语料::记录(&mut 语料.关键字表, 字串, file);
                            }
                            冲刷路径(&mut 路径链, 路径态, 处于导入, 语料, file);
                            处于导入 = false;
                            路径态 = false;
                        }
                    }
                } else if 跳过下个导入名 {
                    跳过下个导入名 = false;
                    路径链.clear();
                    路径态 = false;
                } else if 处于导入 || 路径态 {
                    路径链.push(字串.to_string());
                } else {
                    // 暂存：等下一个 token 决定是路径链还是普通标识符
                    路径链.push(字串.to_string());
                    路径态 = false;
                }
            }
            rustc_lexer::TokenKind::Colon => {
                // rustc_lexer 将 `::` 拆为两个 Colon（中间允许空白）：
                // 第一半前瞻下一个非空白仍是 Colon；第二半（前一个非空白
                // 是 Colon）保持路径链，仅单冒号（类型标注等）结束当前链
                let 前一冒号 = matches!(
                    上个非空白种类(&令牌流, 索引),
                    Some(rustc_lexer::TokenKind::Colon)
                );
                let 后一冒号 = matches!(
                    下个非空白种类(&令牌流, 索引 + 1),
                    Some(rustc_lexer::TokenKind::Colon)
                );
                if 前一冒号 {
                    // `::` 第二半：路径链继续（路径态 已由第一半设置）
                } else if 后一冒号 {
                    路径态 = true;
                } else {
                    // 单个冒号（类型标注等）：结束当前链
                    冲刷路径(&mut 路径链, 路径态, 处于导入, 语料, file);
                    路径态 = false;
                }
            }
            rustc_lexer::TokenKind::Semi => {
                冲刷路径(&mut 路径链, 路径态, 处于导入, 语料, file);
                处于导入 = false;
                路径态 = false;
            }
            _ => {
                // 其他 token：结束当前链；若链中只有一个暂存标识符，按普通标识符处理
                冲刷路径(&mut 路径链, 路径态, 处于导入, 语料, file);
                路径态 = false;
            }
        }
    }
    冲刷路径(&mut 路径链, 路径态, 处于导入, 语料, file);
}

/// 结束当前路径链：按 use/非 use 与首段归属分类记录到语料
fn 冲刷路径(
    路径链: &mut Vec<String>,
    路径态: bool,
    处于导入: bool,
    语料: &mut 语料,
    file: &str,
) {
    if 路径链.is_empty() {
        return;
    }
    // 非路径链的单个暂存标识符：首字母大写且非单字符 → 类型名
    if !路径态 && !处于导入 {
        let 名字 = &路径链[0];
        let 首字符 = 名字.chars().next().unwrap_or('a');
        if 首字符.is_uppercase() && 名字.len() > 1 {
            语料::记录(&mut 语料.类型名表, 名字, file);
        }
        路径链.clear();
        return;
    }
    let 首段 = &路径链[0];
    let 是内部 = matches!(首段.as_str(), "crate" | "super" | "self");
    let 是标准库 = matches!(首段.as_str(), "std" | "core" | "alloc");
    if !是内部 {
        if 是标准库 || 处于导入 {
            // std 路径引用 / use 导入：段全部记录（模块名与类型名）
            for 段 in 路径链.iter() {
                if 是标准库 {
                    语料::记录(&mut 语料.标准库段表, 段, file);
                }
            }
        }
        // 首段（crate 名）：use 导入或路径引用的第三方 crate；
        // 惯例上 crate 名小写、类型名大写（如 MyServer::new() 的 MyServer）
        if !是标准库 {
            let 首字符 = 首段.chars().next().unwrap_or('a');
            if 首字符.is_uppercase() {
                语料::记录(&mut 语料.类型名表, 首段, file);
            } else {
                语料::记录(&mut 语料.外部库表, 首段, file);
            }
        }
        // use 内的非首段：大写段是类型名（如 use serde::Serialize 的 Serialize）
        if 处于导入 && !是标准库 {
            for 段 in 路径链.iter().skip(1) {
                let 首字符项 = 段.chars().next().unwrap_or('a');
                if 首字符项.is_uppercase() && 段.len() > 1 {
                    语料::记录(&mut 语料.类型名表, 段, file);
                }
            }
        }
        // 非 use 的第三方路径引用（如 ureq::get）：后续段按类型名/忽略
        if !处于导入 && !是标准库 {
            for 段 in 路径链.iter().skip(1) {
                let 首字符项 = 段.chars().next().unwrap_or('a');
                if 首字符项.is_uppercase() && 段.len() > 1 {
                    语料::记录(&mut 语料.类型名表, 段, file);
                }
            }
        }
    }
    路径链.clear();
}

/// 构建语言包反向索引：英文原名 → 母语键
///
/// 合并 keywords.toml（排除派生特征节，与运行时语义一致）、module_paths.toml、
/// stdlib.toml、crates/*.toml 的全部映射值；同值多键时保留首个（报告无需区分）。
fn 构建反向索引(
    关键字文本: &str,
    模块路径文本: &str,
    标准库文本: &str,
    三方库数据: &[(&str, &str)],
) -> HashMap<String, String> {
    let mut 反向索引: HashMap<String, String> = HashMap::new();
    let 反向插入 = |反向: &mut HashMap<String, String>, 英文名: &str, 母语键: &str| {
        if !英文名.is_empty() {
            反向
                .entry(英文名.to_string())
                .or_insert_with(|| 母语键.to_string());
        }
    };
    if let Ok(节表) = i18n_rust_engine::映射源::解析配置节(关键字文本) {
        for (节名, 映射表) in &节表 {
            if 节名 == "派生特征" {
                continue;
            }
            for (母语键, 英文名) in 映射表 {
                反向插入(&mut 反向索引, 英文名, 母语键);
            }
        }
    }
    for 文本内容 in [模块路径文本, 标准库文本] {
        if let Ok(节表) = i18n_rust_engine::映射源::解析配置节(文本内容) {
            for 映射表 in 节表.values() {
                for (母语键, 英文名) in 映射表 {
                    反向插入(&mut 反向索引, 英文名, 母语键);
                }
            }
        }
    }
    for (_, 内容) in 三方库数据 {
        if let Ok(节表) = i18n_rust_engine::映射源::解析配置节(内容) {
            for 映射表 in 节表.values() {
                for (母语键, 英文名) in 映射表 {
                    反向插入(&mut 反向索引, 英文名, 母语键);
                }
            }
        }
    }
    反向索引
}

/// 单个语言包的覆盖报告
pub struct 覆盖报告 {
    /// 语言代码
    pub 语言代码: String,
    /// 缺失关键字（error 级）：(名字, 出现次数, 示例文件)
    pub 缺失关键字: Vec<(String, usize, String)>,
    /// 缺失 std/core/alloc 段（warning 级，用户写代码最常遇到）
    pub 缺失标准库: Vec<(String, usize, String)>,
    /// 缺失第三方 crate 名（warning 级，用外部库时需要）
    pub 缺失外部库: Vec<(String, usize, String)>,
    /// 缺失类型名（warning 级，多为编译器内部实现，可按需忽略）
    pub 缺失类型: Vec<(String, usize, String)>,
    /// 覆盖统计
    pub 名字总数: usize,
    pub 覆盖数: usize,
}

/// 按出现次数降序排序缺失清单（同次数按名字升序，稳定输出）
fn 排序缺失(清单: &mut [(String, usize, String)]) {
    清单.sort_by(|甲, 乙| 乙.1.cmp(&甲.1).then(甲.0.cmp(&乙.0)));
}

/// 用反向索引检测语料覆盖，产出报告
fn 计算覆盖(
    语料: &语料, 反向索引: &HashMap<String, String>, 语言代码: &str
) -> 覆盖报告 {
    let mut 报告 = 覆盖报告 {
        语言代码: 语言代码.to_string(),
        缺失关键字: Vec::new(),
        缺失标准库: Vec::new(),
        缺失外部库: Vec::new(),
        缺失类型: Vec::new(),
        名字总数: 0,
        覆盖数: 0,
    };
    // 关键字缺失（error 级）
    for (名字, (次数, file)) in &语料.关键字表 {
        报告.名字总数 += 1;
        if !反向索引.contains_key(名字) {
            报告.缺失关键字.push((名字.clone(), *次数, file.clone()));
        } else {
            报告.覆盖数 += 1;
        }
    }
    // API 缺失（warning 级）：std 段 / 外部 crate / 类型名分三组
    for (名字, (次数, file)) in &语料.标准库段表 {
        报告.名字总数 += 1;
        if !反向索引.contains_key(名字) {
            报告.缺失标准库.push((名字.clone(), *次数, file.clone()));
        } else {
            报告.覆盖数 += 1;
        }
    }
    for (名字, (次数, file)) in &语料.外部库表 {
        报告.名字总数 += 1;
        if !反向索引.contains_key(名字) {
            报告.缺失外部库.push((名字.clone(), *次数, file.clone()));
        } else {
            报告.覆盖数 += 1;
        }
    }
    for (名字, (次数, file)) in &语料.类型名表 {
        报告.名字总数 += 1;
        if !反向索引.contains_key(名字) {
            报告.缺失类型.push((名字.clone(), *次数, file.clone()));
        } else {
            报告.覆盖数 += 1;
        }
    }
    排序缺失(&mut 报告.缺失关键字);
    排序缺失(&mut 报告.缺失标准库);
    排序缺失(&mut 报告.缺失外部库);
    排序缺失(&mut 报告.缺失类型);
    报告
}

/// 打印单个语言包的覆盖报告（本地化输出）
fn 打印报告(报告: &覆盖报告, 源文件数: usize) {
    let 界面 = crate::本地化::界面::全局();
    println!("{}", 界面.取文带参("mc_cov_header", &[&报告.语言代码]));
    let 百分比 = 报告
        .覆盖数
        .checked_mul(100)
        .and_then(|比值项| 比值项.checked_div(报告.名字总数))
        .unwrap_or(0);
    println!(
        "{}",
        界面.取文带参(
            "mc_cov_stats",
            &[
                &报告.覆盖数.to_string(),
                &报告.名字总数.to_string(),
                &百分比.to_string(),
                &源文件数.to_string()
            ]
        )
    );
    // 关键字缺失（error 级，最多展示 30 条）
    let 关键字总数 = 报告.缺失关键字.len();
    for (名字, 次数, file) in 报告.缺失关键字.iter().take(30) {
        println!(
            "{}",
            界面.取文带参("mc_cov_kw_missing", &[名字, &次数.to_string(), file])
        );
    }
    if 关键字总数 > 30 {
        println!(
            "{}",
            界面.取文带参("mc_cov_more", &[&(关键字总数 - 30).to_string()])
        );
    }
    // std 段缺失（warning 级，最多展示 25 条）：用户写代码最常遇到，优先补齐
    let 标准库总数 = 报告.缺失标准库.len();
    for (名字, 次数, file) in 报告.缺失标准库.iter().take(25) {
        println!(
            "{}",
            界面.取文带参("mc_cov_std_missing", &[名字, &次数.to_string(), file])
        );
    }
    if 标准库总数 > 25 {
        println!(
            "{}",
            界面.取文带参("mc_cov_more", &[&(标准库总数 - 25).to_string()])
        );
    }
    // 第三方 crate 缺失（warning 级，最多展示 15 条）
    let 库总数 = 报告.缺失外部库.len();
    for (名字, 次数, file) in 报告.缺失外部库.iter().take(15) {
        println!(
            "{}",
            界面.取文带参("mc_cov_crate_missing", &[名字, &次数.to_string(), file])
        );
    }
    if 库总数 > 15 {
        println!(
            "{}",
            界面.取文带参("mc_cov_more", &[&(库总数 - 15).to_string()])
        );
    }
    // 类型名缺失（warning 级，最多展示 15 条；多为编译器内部实现，可按需忽略）
    let 类型总数 = 报告.缺失类型.len();
    for (名字, 次数, file) in 报告.缺失类型.iter().take(15) {
        println!(
            "{}",
            界面.取文带参("mc_cov_type_missing", &[名字, &次数.to_string(), file])
        );
    }
    if 类型总数 > 15 {
        println!(
            "{}",
            界面.取文带参("mc_cov_more", &[&(类型总数 - 15).to_string()])
        );
    }
    if 关键字总数 == 0 {
        println!("{}", 界面.取文("mc_cov_kw_ok"));
    }
    println!();
}

/// `rzc mapping coverage` 入口
///
/// - 语言代码 为 无：检测全部内置语言
/// - 语言代码 为内置语言代码：仅检测该语言
///
/// 返回是否全部语言关键字覆盖（error 级）通过。
pub fn 运行覆盖(语言代码: Option<&str>) -> anyhow::Result<bool> {
    let 界面 = crate::本地化::界面::全局();
    // 语料必须来自源码仓库（发布版无 crates/{engine,cli,lsp}/src 结构）
    let 当前目录 =
        std::env::current_dir().map_err(|首错| anyhow::anyhow!("获取当前目录失败: {首错}"))?;
    let 根 = crate::向上定位项目根(&当前目录)
        .ok_or_else(|| anyhow::anyhow!("{}", 界面.取文("mc_cov_no_repo")))?;
    let 源文件表 = 收集后端源码(&根);
    if 源文件表.is_empty() {
        anyhow::bail!("{}", 界面.取文("mc_cov_no_repo"));
    }
    // 提取语料（一次，供所有语言包复用）
    let mut 语料 = 语料::default();
    for (file, 内容) in &源文件表 {
        从源提取名字(file, 内容, &mut 语料);
    }
    println!("{}", 界面.取文("mc_cov_corpus_intro"));
    let 语言列表: Vec<&str> = match 语言代码 {
        Some(单码) => vec![单码],
        None => crate::内置语言::内置语言代码(),
    };
    let mut 全部通过 = true;
    for 语言 in 语言列表 {
        if !crate::内置语言::拥有内置语言(语言) {
            anyhow::bail!(界面.取文带参("mc_unknown_target", &[语言]));
        }
        let 数据 = crate::内置语言::获取内置数据(语言);
        let 反向索引 = 构建反向索引(
            数据.关键字文本,
            数据.模块路径文本,
            数据.标准库文本,
            数据.三方库数据,
        );
        let 报告 = 计算覆盖(&语料, &反向索引, 语言);
        全部通过 &= 报告.缺失关键字.is_empty();
        打印报告(&报告, 源文件表.len());
    }
    if 全部通过 {
        println!("{}", 界面.取文("mc_cov_all_ok"));
    }
    Ok(全部通过)
}

#[cfg(test)]
mod 单元测试 {
    use super::*;

    /// 单文件语料提取：关键字 / std 路径段 / 第三方 crate / 类型名
    #[test]
    fn 测试提取名字基础() {
        let 源 = r#"
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
        let mut 语料 = 语料::default();
        从源提取名字("t.rs", 源, &mut 语料);
        // 关键字
        for 关键字 in ["fn", "let", "pub", "dyn"] {
            assert!(语料.关键字表.contains_key(关键字), "缺少关键字 {关键字}");
        }
        assert!(!语料.关键字表.contains_key("use"), "use 不应作为关键字语料");
        // std 路径段
        for 段 in ["std", "collections", "HashMap", "fs", "read_to_string"] {
            assert!(语料.标准库段表.contains_key(段), "缺少 std 段 {段}");
        }
        // 第三方 crate 名
        for 包名项 in ["serde", "ureq"] {
            assert!(语料.外部库表.contains_key(包名项), "缺少 crate {包名项}");
        }
        // 内部路径忽略：i18n_rust_engine 是 crate 名（外部），crate:: 内部忽略
        assert!(!语料.外部库表.contains_key("crate"), "crate:: 不应计入");
        // 类型名：use 段中的大写 + 普通大写标识符
        assert!(语料.类型名表.contains_key("Serialize"));
        assert!(语料.类型名表.contains_key("MyServer"));
        assert!(
            语料.类型名表.contains_key("Error"),
            "Box<dyn Error> 的 Error"
        );
        assert!(
            !语料.外部库表.contains_key("HashMap"),
            "裸 HashMap::new() 是类型引用而非 crate"
        );
        assert!(
            语料.标准库段表.contains_key("HashMap"),
            "use 路径链中的 HashMap 计入 std 段"
        );
        // 非 use 的内部路径调用（crate::本地化::界面）整体忽略，内部模块名不入 外部库表
        let 源2 = "fn f() { crate::本地化::界面::全局(); super::helper::run(); self::x::y(); }\n";
        let mut 语料2 = 语料::default();
        从源提取名字("t2.rs", 源2, &mut 语料2);
        for 名字项 in ["ui", "Ui", "helper", "run", "x", "y"] {
            assert!(
                !语料2.外部库表.contains_key(名字项) && !语料2.类型名表.contains_key(名字项),
                "内部路径段 {名字项} 不应计入"
            );
        }
    }

    /// use as 别名与泛型单字符不进入语料
    #[test]
    fn 测试提取名字别名与泛型() {
        let 源 = "use std::fmt::Result as FmtResult;\nfn foo<T>(x: T) -> Result<T> { Ok(x) }\n";
        let mut 语料 = 语料::default();
        从源提取名字("t.rs", 源, &mut 语料);
        assert!(!语料.类型名表.contains_key("FmtResult"), "as 别名应跳过");
        assert!(!语料.类型名表.contains_key("T"), "单字符泛型参数不应计入");
        assert!(语料.标准库段表.contains_key("fmt"));
        assert!(语料.关键字表.contains_key("fn"));
        assert!(
            语料.类型名表.contains_key("Result"),
            "返回类型中的真实类型引用应记录"
        );
    }

    /// 反向索引：合并 keywords/stdlib/module_paths/crates 的值
    #[test]
    fn 测试构建反向索引() {
        let 关键字 = "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n[\"派生特征\"]\n\"调试\" = \"Debug\"\n";
        let 模块路径 = "[\"模块路径\"]\n\"标准库\" = \"std\"\n";
        let 标准库 = "[\"标识符\"]\n\"字符串\" = \"String\"\n";
        let 三方库 = [(
            "serde.toml",
            "[\"模块路径\"]\n\"序列化\" = \"serde\"\n[\"标识符\"]\n\"序列化特征\" = \"Serialize\"\n",
        )];
        let 反转 = 构建反向索引(关键字, 模块路径, 标准库, &三方库);
        assert_eq!(反转.get("fn").map(String::as_str), Some("函数"));
        assert_eq!(反转.get("let").map(String::as_str), Some("让"));
        assert_eq!(反转.get("std").map(String::as_str), Some("标准库"));
        assert_eq!(反转.get("String").map(String::as_str), Some("字符串"));
        assert_eq!(反转.get("serde").map(String::as_str), Some("序列化"));
        assert_eq!(
            反转.get("Serialize").map(String::as_str),
            Some("序列化特征")
        );
        assert!(!反转.contains_key("Debug"), "派生特征节不应并入关键字索引");
    }

    /// 覆盖检测：关键字缺失入 error 列表，API 缺失入 warning 列表
    #[test]
    fn 测试计算覆盖() {
        let 关键字 = "[\"声明\"]\n\"函数\" = \"fn\"\n";
        let 模块路径 = "[\"模块路径\"]\n\"标准库\" = \"std\"\n";
        let 反转 = 构建反向索引(关键字, 模块路径, "", &[]);
        let mut 语料 = 语料::default();
        语料.关键字表.insert("fn".into(), (3, "a.rs".into()));
        语料.关键字表.insert("unsafe".into(), (1, "b.rs".into()));
        语料.标准库段表.insert("std".into(), (9, "a.rs".into()));
        语料
            .类型名表
            .insert("NotCovered".into(), (2, "b.rs".into()));
        let 报告 = 计算覆盖(&语料, &反转, "zh");
        assert!(
            报告
                .缺失关键字
                .iter()
                .any(|(名字项, ..)| 名字项 == "unsafe")
        );
        assert!(!报告.缺失关键字.iter().any(|(名字项, ..)| 名字项 == "fn"));
        assert!(
            报告
                .缺失类型
                .iter()
                .any(|(名字项, ..)| 名字项 == "NotCovered")
        );
        assert!(!报告.缺失标准库.iter().any(|(名字项, ..)| 名字项 == "std"));
        assert_eq!(报告.名字总数, 4);
        assert_eq!(报告.覆盖数, 2);
    }

    /// 关键字门禁：全部内置语言包必须覆盖 Rust 稳定关键字全集（error 级）——
    /// 任一门语言缺任一关键字（用户无法用母语写出该语法结构）都会在此失败
    #[test]
    fn 测试全部内置语言关键字完备() {
        let mut 失败表: Vec<String> = Vec::new();
        for 语言 in crate::内置语言::内置语言代码() {
            let 数据 = crate::内置语言::获取内置数据(语言);
            let 反转 = 构建反向索引(数据.关键字文本, "", "", &[]);
            let 缺失: Vec<&str> = 关键字全集
                .iter()
                .copied()
                .filter(|键名项| !反转.contains_key(*键名项))
                .collect();
            if !缺失.is_empty() {
                失败表.push(format!("{语言} 缺 {缺失:?}",));
            }
        }
        assert!(失败表.is_empty(), "关键字门禁失败: {失败表:?}");
    }

    /// 全内置语言跑一遍覆盖检测（不 panic、有统计）
    #[test]
    fn 测试运行覆盖全语言不崩溃() {
        for 语言 in crate::内置语言::内置语言代码() {
            let 数据 = crate::内置语言::获取内置数据(语言);
            let 反转 = 构建反向索引(
                数据.关键字文本,
                数据.模块路径文本,
                数据.标准库文本,
                数据.三方库数据,
            );
            // 用最小语料冒烟：语料为空时统计为 0 即可
            let 语料 = 语料::default();
            let 报告 = 计算覆盖(&语料, &反转, 语言);
            assert_eq!(报告.名字总数, 0, "{语言} 空语料统计应为 0");
            assert!(报告.缺失关键字.is_empty());
        }
    }
}
