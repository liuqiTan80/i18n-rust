//! 模块路径替换模块
//! 1. 将源代码中 `use` 语句的中文模块路径段替换为英文路径段
//!    （如 `使用 标准集合::哈希映射` → `使用 标准库::集合模块::HashMap`）；
//! 2. 为已知模块路径段添加 `包::` 前缀（LSP 虚拟项目跨文件引用专用，
//!    原实现在 lsp 包，迁入引擎统一维护转译规则）；
//! 3. 文件式 `mod 名字;` 声明的净化（虚拟项目聚合用）与非 ASCII 模块名的
//!    `#[路径模块]` 注解（真实项目产物用，CLI 与 LSP 镜像共享同一实现）。
//!
//! 各阶段均以 token 级替换并产出编辑表，供全管线编辑地图组合。
//!
//! 包 名规范化：Cargo 包名允许连字符（如 `tracing-subscriber`），但 Rust
//! 代码中引用 包 必须写 `_` 形式（`tracing_subscriber`）；use 路径中
//! 模块/类型/函数名不含连字符，`-` 只会出现在 包 名段，故替换时对
//! 映射值做 `-` → `_` 规范化安全。

use crate::缓存::源映射条目;
use rustc_lexer::{TokenKind, tokenize};
use std::collections::{HashMap, HashSet};

/// 单阶段替换结果串：输出与编辑表
///
/// 编辑表以**本阶段输入文本**的字节偏移记录（input_offset 升序、互不重叠），
/// replacement 为本阶段输出文本。
#[derive(Debug, Clone)]
pub struct 替换结果 {
    /// 替换后的文本
    pub 产出: String,
    /// 编辑表
    pub 编辑表: Vec<源映射条目>,
}

/// 将源代码中的中文模块路径段替换为英文
///
/// # 参数
/// - `source`: 待替换的源代码字符串
/// - `path_map`: 中文路径段 → 英文路径段 的映射表
///
/// # 注意
/// 采用 token 级替换（与 `替换别名` 一致）：
/// - 仅替换**完整标识符 token**，避免破坏组合词
///   （如 `读取全部字符串` 中的 `字符串` 不会被误替换）；
/// - 仅替换 **use 语句内**的路径段（含末段与单段路径，如 `使用 文件系统;`），
///   use 内不可能出现用户变量，全量替换安全；
/// - 表达式中的 `类型::关联函数`（如 `引用计数::新建`）不替换，
///   留给别名映射处理成 `Rc::新建`。
pub fn 模块路径替换(源文本: &str, 路径映射表: &HashMap<String, String>) -> String {
    模块路径替换并映射(源文本, 路径映射表).产出
}

/// 同 [`模块路径替换`]，同时产出编辑表（本阶段输入坐标）
pub fn 模块路径替换并映射(
    源文本: &str,
    路径映射表: &HashMap<String, String>,
) -> 替换结果 {
    if 路径映射表.is_empty() {
        return 替换结果 {
            产出: 源文本.to_string(),
            编辑表: Vec::new(),
        };
    }
    let 令牌序列: Vec<_> = tokenize(源文本).collect();
    let mut 产出 = String::new();
    let mut 编辑表 = Vec::new();
    let mut 当前偏移 = 0;
    // use 语句内的路径段才启用模块路径映射；
    // 表达式中的 `类型::关联函数`（如 `引用计数::新建`）应走别名映射，
    // 不能被模块路径映射误替换成 `rc::新建`
    let mut 在使用段内 = false;

    for 令牌 in 令牌序列.iter() {
        let 字节长 = 令牌.len;
        let 文本段 = &源文本[当前偏移..当前偏移 + 字节长];
        match 令牌.kind {
            TokenKind::Ident => {
                // use 语句内所有路径段（含末段/单段）都查映射：
                // use 中不可能出现用户变量，替换安全；
                // 末段若不在路径映射（如 `服务器` 属于标识符别名），
                // 保持原样交给别名替换处理
                if 在使用段内 {
                    if let Some(英文段) = 路径映射表.get(文本段) {
                        // 包 名规范化：`日志订阅` → `tracing_subscriber`
                        //（映射值 `tracing-subscriber` 中的连字符非法）
                        let 规范化值 = 规范化包名连字符(英文段);
                        let 替换值 = 规范化值.as_deref().unwrap_or(英文段);
                        编辑表.push(源映射条目::新建条目(
                            当前偏移,
                            字节长,
                            文本段,
                            替换值,
                        ));
                        产出.push_str(替换值);
                    } else {
                        产出.push_str(文本段);
                    }
                } else {
                    产出.push_str(文本段);
                }
                // 检测 use 语句起点（管线中 `使用` 已被词法转译为 `use`；
                // 直接调用本函数时两种写法均支持）
                if 文本段 == "use" || 文本段 == "使用" {
                    在使用段内 = true;
                }
            }
            TokenKind::Semi => {
                // use 语句以分号结束
                在使用段内 = false;
                产出.push_str(文本段);
            }
            _ => 产出.push_str(文本段),
        }
        当前偏移 += 字节长;
    }
    替换结果 { 产出, 编辑表 }
}

/// 将 use 路径段中的连字符规范化为下划线（包 名规则）
///
/// Cargo 包名允许连字符（如 `tracing-subscriber`、`tokio-util`），但 Rust
/// 路径中 包 段必须写 `_` 形式（`tracing_subscriber`）。use 路径中
/// 模块、类型、函数名均不含连字符，`-` 只会出现在 包 名段，故直接
/// 替换安全；无需替换时返回 无（避免无谓分配）。
/// 别名阶段（别名替换.rs）对 use 之外的模块词回退替换时复用本函数。
pub(crate) fn 规范化包名连字符(段: &str) -> Option<String> {
    段.contains('-').then(|| 段.replace('-', "_"))
}

/// 为已知模块路径段添加 `包::` 前缀
///
/// Rust 2018+ 中，子模块内的裸路径 `模块::项` 无法解析到 包 根的模块，
/// 必须写成 `包::模块::项` 或先 use。LSP 虚拟项目把每个方言文件聚合为
/// 同一 包 的兄弟模块，因此为引用其他模块的路径段自动补全前缀，
/// 使 rust-analyzer 能够解析跨文件引用（references/重命名）。
///
/// 已带前缀的路径（`包::辅助`、`其他::辅助`）不会被重复处理。
///
/// 遮蔽豁免：模块名与文件内名字（类型声明名、use 导入绑定名）重名时，
/// 裸路径首段按 Rust 作用域解析优先命中后者（如 `结构体 项目配置` 与
/// `模块 项目配置` 重名时，`项目配置::缺省()` 指向结构体关联函数），
/// 此时不得加前缀——前缀会把路径钉死为模块路径（E0425）。文件含 glob
/// 导入（`use ...::*;`）时，项目项名集合 `item_names`（跨文件汇总）中的
/// 名字视为可能被 glob 引入作用域，一并豁免；`item_names` 传空集合时
/// 仅按文件内遮蔽判定（无项目上下文的调用方行为不变）。
pub fn 模块路径限定并映射(
    内容: &str,
    模块名集: &HashSet<String>,
    项名集: &HashSet<String>,
) -> 替换结果 {
    if 模块名集.is_empty() || 内容.is_empty() {
        return 替换结果 {
            产出: 内容.to_string(),
            编辑表: Vec::new(),
        };
    }
    // token 序列与字节区间：预扫描（遮蔽名收集）与主循环共用
    let mut 词区间: Vec<(TokenKind, usize, usize)> = Vec::new();
    let mut 偏移 = 0usize;
    for 令牌 in tokenize(内容) {
        let 起始 = 偏移;
        偏移 += 令牌.len;
        词区间.push((令牌.kind, 起始, 偏移));
    }
    let (遮蔽名集, 含通配) = 收集遮蔽名(内容, &词区间);

    let mut 产出 = String::with_capacity(内容.len() + 模块名集.len() * 8);
    let mut 编辑表 = Vec::new();

    for (外索引, &(令牌类型, 起始, 终点)) in 词区间.iter().enumerate() {
        let 文本段 = &内容[起始..终点];

        if 是否为空白符(令牌类型) {
            产出.push_str(文本段);
            continue;
        }

        // 模块路径段：标识符属于已知模块名、后跟 `::`、且不在既有路径段之后
        // （`包::辅助`、`测试甲::辅助` 中的 `辅助` 已处于路径内，跳过）；
        // 文件内遮蔽名与 glob 下命中的项目项名不加前缀（保持本地语义）
        let 需加前缀 = 是否为标识符(令牌类型) && {
            let 裸名 = 文本段.strip_prefix("r#").unwrap_or(文本段);
            模块名集.contains(裸名)
                && 后接路径分隔符(&词区间, 外索引)
                && !前接路径分隔符(&词区间, 外索引)
                && !遮蔽名集.contains(裸名)
                && !(含通配 && 项名集.contains(裸名))
        };
        if 需加前缀 {
            let 加前缀值 = format!("crate::{}", 文本段);
            编辑表.push(源映射条目::新建条目(
                起始,
                文本段.len(),
                文本段,
                &加前缀值,
            ));
            产出.push_str(&加前缀值);
        } else {
            产出.push_str(文本段);
        }
    }
    替换结果 { 产出, 编辑表 }
}

/// 收集文件内遮蔽名与 glob 导入标记（`包::` 前缀豁免判定用）
///
/// 遮蔽名 = 可能让裸路径首段优先解析为本地项的名字：
/// - 类型声明名：`struct`/`enum`/`trait`/`type`/`union` 关键字后的名字
///   （类型命名空间，路径首段解析的优先候选）；
/// - use 导入绑定名：use 语句中每条路径链的末段，`作为` 别名取其别名
///   （容器段与中间段不引入作用域，不收集）。
///
/// 不收集 `mod` 声明名：LSP 虚拟项目会以 1:1 空格替换抹除文件式 `mod`
/// 声明（[`剥离文件模块声明`]），声明消失后裸模块名仍须依赖
/// `包::` 前缀解析，收集会令前缀缺失（回归 E0433 误报）。
///
/// 返回值第二项标记文件是否存在 glob 导入（`use ...::*;`）：glob 引入
/// 的名字不可枚举，调用方回退查项目级项名集合做豁免。
fn 收集遮蔽名<'a>(
    内容: &'a str,
    词区间: &[(TokenKind, usize, usize)],
) -> (HashSet<&'a str>, bool) {
    let mut 名字集: HashSet<&'a str> = HashSet::new();
    let mut 含通配 = false;
    for (外索引, &(令牌类型, 起始, 终点)) in 词区间.iter().enumerate() {
        if !是否为标识符(令牌类型) {
            continue;
        }
        let 文本段 = &内容[起始..终点];
        if matches!(文本段, "struct" | "enum" | "trait" | "type" | "union") {
            // 声明名：关键字后第一个非空白 token 为标识符时收集
            let mut 内索引 = 外索引 + 1;
            while 内索引 < 词区间.len() && 是否为空白符(词区间[内索引].0) {
                内索引 += 1;
            }
            if 内索引 < 词区间.len() && 是否为标识符(词区间[内索引].0) {
                let (_, 字符串项, 错误值) = 词区间[内索引];
                名字集.insert(&内容[字符串项..错误值]);
            }
        } else if 文本段 == "use" {
            收集使用绑定(内容, 词区间, 外索引, &mut 名字集, &mut 含通配);
        }
    }
    (名字集, 含通配)
}

/// 扫描一条 use 语句：收集绑定名（路径链末段/`作为` 别名），标记 glob
///
/// `use` 后的 token 流以 `;` 结束（use 内不出现分号）；花括号容器段
/// （`路径模块::{...}` 的 `路径模块` 末段）不引入作用域，遇 `{` 不提交末段；
/// glob 来源段（`路径::*` 的路径末段）同样不是绑定名。
fn 收集使用绑定<'a>(
    内容: &'a str,
    词区间: &[(TokenKind, usize, usize)],
    使用索引: usize,
    名字集: &mut HashSet<&'a str>,
    含通配: &mut bool,
) {
    let mut 末段: Option<&'a str> = None;
    let mut 期待别名 = false;
    for &(令牌类型, 起始, 终点) in &词区间[(使用索引 + 1)..] {
        if 令牌类型 == TokenKind::Semi {
            break;
        }
        let 文本段 = &内容[起始..终点];
        match 令牌类型 {
            TokenKind::Star => {
                *含通配 = true;
                末段 = None;
            }
            TokenKind::Ident | TokenKind::RawIdent => {
                let 名字 = 文本段.strip_prefix("r#").unwrap_or(文本段);
                if 名字 == "as" {
                    期待别名 = true;
                } else if !matches!(名字, "use" | "pub" | "crate" | "self" | "super") {
                    if 期待别名 {
                        名字集.insert(名字);
                        期待别名 = false;
                        末段 = None;
                    } else {
                        末段 = Some(名字);
                    }
                }
            }
            // 链结束（`测试甲::测试乙,` / `测试甲::测试乙}`）：末段为导入绑定名
            TokenKind::Comma | TokenKind::CloseBrace => {
                if let Some(段) = 末段.take() {
                    名字集.insert(段);
                }
            }
            // 进入成员列表（`路径模块::{...}`）：容器段不是绑定名
            TokenKind::OpenBrace => 末段 = None,
            _ => {}
        }
    }
    if let Some(段) = 末段 {
        名字集.insert(段);
    }
}

/// 抹除文件式模块声明（`mod 名字;`），供 LSP 虚拟项目内容净化使用
///
/// LSP 虚拟项目将每个方言文件以哈希名托管，并在聚合 `main.rs` 中通过
/// `#[路径模块]` 声明为兄弟模块；用户原文中的文件式 `模块 名字;`（转译后为
/// `mod 名字;`）指向的模块文件在虚拟项目中不存在，会被 cargo check 报
/// E0583（找不到模块文件）与 E0754（非 ASCII 标识符名）。因此发送给
/// rust-analyzer 与写入磁盘前须抹除这类声明。
///
/// 抹除范围连带声明紧邻的修饰：属性（`#[cfg(...)]`、`#[路径模块 = ...]`）、
/// 文档注释与可见性（`公开`、`公开(包)`）——修饰必须附着于某个项，
/// 声明被抹除后会变成悬空修饰触发新错误；修饰与声明之间允许空白与注释。
///
/// 抹除采用 1:1 字符替换（换行符保留、其余字符替换为等数 UTF-16 单位的
/// 空格），行号与列号同原文严格一致，列映射无需调整；内联模块
/// （`mod 名字 { ... }`）照常参与编译，不被处理。
pub fn 剥离文件模块声明(内容: &str) -> String {
    // 收集 token 的字节区间：词法解析器 的 token 流连续覆盖整个输入
    let mut 词区间: Vec<(TokenKind, usize, usize)> = Vec::new();
    let mut 偏移 = 0usize;
    for 令牌 in tokenize(内容) {
        let 起始 = 偏移;
        偏移 += 令牌.len;
        词区间.push((令牌.kind, 起始, 偏移));
    }

    // 定位所有文件式模块声明：`mod` + 名字（隔空白/注释）+ `;`
    let mut 区间列表: Vec<(usize, usize)> = Vec::new();
    let mut 外索引 = 0usize;
    while 外索引 < 词区间.len() {
        let (令牌类型, 起始, 终点) = 词区间[外索引];
        if 是否为标识符(令牌类型)
            && &内容[起始..终点] == "mod"
            && let Some(分号索引) = 文件模块结束分号(&词区间, 外索引)
        {
            // 回溯扩展起点：连带声明前的属性/文档注释/可见性修饰
            let 块起始 = 文件模块声明起点(&词区间, 内容, 外索引);
            区间列表.push((词区间[块起始].1, 词区间[分号索引].2));
            外索引 = 分号索引 + 1;
            continue;
        }
        外索引 += 1;
    }
    if 区间列表.is_empty() {
        return 内容.to_string();
    }

    // 1:1 字符替换：换行保留（行号不变），其余字符按等数 UTF-16 单位
    // 替换为空格（列号不变）
    let mut 输出文本 = String::with_capacity(内容.len());
    let mut 已复制 = 0usize;
    for (起始, 终点) in 区间列表 {
        输出文本.push_str(&内容[已复制..起始]);
        for 本字符 in 内容[起始..终点].chars() {
            if 本字符 == '\n' || 本字符 == '\r' {
                输出文本.push(本字符);
            } else {
                for _ in 0..本字符.len_utf16() {
                    输出文本.push(' ');
                }
            }
        }
        已复制 = 终点;
    }
    输出文本.push_str(&内容[已复制..]);
    输出文本
}

/// 检查 `mod` token 是否为文件式声明，是则返回其结束分号的 token 索引
///
/// 文件式：`mod` 后（允许空白/注释）为单个标识符，再后（允许空白/注释）
/// 为 `;`；内联模块（`mod X { ... }`）返回 无。
fn 文件模块结束分号(
    词区间: &[(TokenKind, usize, usize)],
    模块索引: usize,
) -> Option<usize> {
    let mut 已见名字 = false;
    for (索引, &(令牌类型, _, _)) in 词区间.iter().enumerate().skip(模块索引 + 1) {
        if 是否为空白符(令牌类型) {
            continue;
        }
        if !已见名字 {
            // `mod` 后必须紧跟名字（含原始标识符 r#名字）
            if !是否为标识符(令牌类型) {
                return None;
            }
            已见名字 = true;
            continue;
        }
        return matches!(令牌类型, TokenKind::Semi).then_some(索引);
    }
    None
}

/// 回溯文件式模块声明的起点：连带声明紧邻的注释、属性与可见性修饰
///
/// 从 `mod` token 向前回溯：跳过空白（注释与声明之间的空行允许跨越）；
/// 遇到紧邻的注释（如文档注释 `///`）直接纳入——悬空文档注释会触发
/// E0585（缺少文档注释目标）；遇到属性结尾（`]`）时纳入配对的 `#[`；
/// 遇到可见性括号（`)`）时纳入配对的 `(` 并确认前置 `公开`；遇到裸
/// `公开` 直接纳入。任何其他 token（上一项的结尾等）终止回溯。
/// 返回最前的修饰 token 索引；无修饰时返回 `mod` 自身索引。
fn 文件模块声明起点(
    词区间: &[(TokenKind, usize, usize)],
    内容: &str,
    模块索引: usize,
) -> usize {
    let mut 最前 = 模块索引;
    let mut 回溯位 = 模块索引; // 回溯游标：当前已纳入块的第一个 token 索引
    loop {
        // 向前跳过空白（仅空白；注释本身会被纳入或终止回溯）
        let mut 前一个 = 回溯位;
        while 前一个 > 0 && 词区间[前一个 - 1].0 == TokenKind::Whitespace {
            前一个 -= 1;
        }
        if 前一个 == 0 {
            return 最前;
        }
        let (令牌类型, 起始, 终点) = 词区间[前一个 - 1];
        match 令牌类型 {
            // 紧邻注释（含文档注释）随声明一并抹除
            TokenKind::LineComment | TokenKind::BlockComment { .. } => {
                最前 = 前一个 - 1;
                回溯位 = 前一个 - 1;
            }
            // 裸可见性 `公开`
            TokenKind::Ident if &内容[起始..终点] == "pub" => {
                最前 = 前一个 - 1;
                回溯位 = 前一个 - 1;
            }
            // 可见性括号 `公开(包)` 的 `)`
            TokenKind::CloseParen => {
                let Some(开启) = 查找配对开启(
                    词区间,
                    前一个 - 1,
                    TokenKind::OpenParen,
                    TokenKind::CloseParen,
                ) else {
                    return 最前;
                };
                // `(` 前须为非空白 `公开`
                let mut 内索引 = 开启;
                while 内索引 > 0 && 是否为空白符(词区间[内索引 - 1].0) {
                    内索引 -= 1;
                }
                if 内索引 > 0
                    && 是否为标识符(词区间[内索引 - 1].0)
                    && &内容[词区间[内索引 - 1].1..词区间[内索引 - 1].2] == "pub"
                {
                    最前 = 内索引 - 1;
                    回溯位 = 内索引 - 1;
                } else {
                    return 最前;
                }
            }
            // 属性结尾 `]`
            TokenKind::CloseBracket => {
                let Some(开启) = 查找配对开启(
                    词区间,
                    前一个 - 1,
                    TokenKind::OpenBracket,
                    TokenKind::CloseBracket,
                ) else {
                    return 最前;
                };
                // `[` 前须为非空白 `#`
                let mut 内索引 = 开启;
                while 内索引 > 0 && 是否为空白符(词区间[内索引 - 1].0) {
                    内索引 -= 1;
                }
                if 内索引 > 0 && 词区间[内索引 - 1].0 == TokenKind::Pound {
                    最前 = 内索引 - 1;
                    回溯位 = 内索引 - 1;
                } else {
                    return 最前;
                }
            }
            _ => return 最前,
        }
    }
}

/// 向前查找与 `close_idx` 处闭合 token 配对的开启 token 索引
fn 查找配对开启(
    词区间: &[(TokenKind, usize, usize)],
    闭合索引: usize,
    开启类型: TokenKind,
    闭合类型: TokenKind,
) -> Option<usize> {
    let mut 深度 = 0usize;
    for 索引 in (0..=闭合索引).rev() {
        let 令牌类型 = 词区间[索引].0;
        if 令牌类型 == 闭合类型 {
            深度 += 1;
        } else if 令牌类型 == 开启类型 {
            深度 -= 1;
            if 深度 == 0 {
                return Some(索引);
            }
        }
    }
    None
}

/// 是否为空白/注释 token
fn 是否为空白符(令牌类型: TokenKind) -> bool {
    matches!(
        令牌类型,
        TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
    )
}

/// 是否为标识符 token（含原始标识符）
fn 是否为标识符(令牌类型: TokenKind) -> bool {
    matches!(令牌类型, TokenKind::Ident | TokenKind::RawIdent)
}

/// 检查指定 token 之后两个连续的非空白 token 是否为 `::`
///
/// 词法解析器 将 `::` 拆分为两个 `冒号` token。
fn 后接路径分隔符(词区间: &[(TokenKind, usize, usize)], 当前索引: usize) -> bool {
    let mut 冒号计数 = 0;
    for &(令牌类型, _, _) in &词区间[(当前索引 + 1)..] {
        if 是否为空白符(令牌类型) {
            continue;
        }
        if matches!(令牌类型, TokenKind::Colon) {
            冒号计数 += 1;
            if 冒号计数 >= 2 {
                return true;
            }
            continue;
        }
        return false;
    }
    false
}

/// 检查指定 token 之前两个连续的非空白 token 是否为 `::`
fn 前接路径分隔符(词区间: &[(TokenKind, usize, usize)], 当前索引: usize) -> bool {
    let mut 冒号计数 = 0;
    for &(令牌类型, _, _) in 词区间[..当前索引].iter().rev() {
        if 是否为空白符(令牌类型) {
            continue;
        }
        if matches!(令牌类型, TokenKind::Colon) {
            冒号计数 += 1;
            if 冒号计数 >= 2 {
                return true;
            }
        } else {
            return false;
        }
    }
    false
}

/// 为非 ASCII 模块名的文件式声明 `mod 名称;` 补充 `#[路径模块 = "名称.rs"]` 注解。
///
/// rustc 拒绝加载非 ASCII 标识符对应的模块文件（E0754），而方言项目
/// 的模块文件常以母语命名（如 `src/数学.zh` → `src/数学.rs`）；
/// 显式指定 路径模块 后 rustc 可正常加载。仅处理以分号结尾的文件式声明，
/// 内联模块块（`mod 名称 { ... }`）与 ASCII 名不受影响。
///
/// 适用于 包 根文件（`main.rs`/`lib.rs`）：子模块与根文件同处 `src/`。
/// 非根文件的多层目录布局请用 [`标注嵌套模块并行号`]，
/// 虚拟托管布局（平铺 哈希模块 产物）请用 [`标注模块路径并行号`]。
pub fn 标注非西文模块(源串: &str) -> String {
    标注非西文模块并行号(源串).0
}

/// 非根模块文件（多层目录）的 `#[路径模块]` 注解，并返回行映射。
///
/// `self_stem` 为当前文件自身词干（如 `src/领域.rs` 传 `"领域"`）：
/// 其中 `mod 工具;` 的子文件实际位于 `src/领域/工具.rs`，而 `#[路径模块]`
/// 相对当前文件所在目录 `src/` 解析，故注入 `#[路径模块 = "领域/工具.rs"]`。
/// 更深层同理（`src/领域/工具.rs` 传 `"工具"` → `#[路径模块 = "工具/内部.rs"]`，
/// 相对 `src/领域/`）。仅注解非 ASCII 名；ASCII 名遵循 rustc 默认查找。
pub fn 标注嵌套模块并行号(源串: &str, 自身词干: &str) -> (String, Vec<usize>) {
    标注模块路径并行号(源串, |名字| {
        (!名字.is_ascii()).then(|| format!("{自身词干}/{名字}.rs"))
    })
}

/// 通用文件式 `mod` 声明 `#[路径模块]` 注解器，并返回磁盘行 → 引擎直行映射。
///
/// `resolve(模块名) -> 有值(相对路径)` 时在声明前插入
/// `#[路径模块 = "<相对路径>"]`；返回 `无` 不注解（如子文件不存在，
/// 保留 rustc 原生 E0583）。仅处理分号结尾的文件式声明与尚无 `#[路径模块]`
/// 的声明；内联模块块（`mod 名 { ... }`）不受影响。
///
/// 供三类布局复用：根文件（`名.rs`）、多层真实布局（`自词干/名.rs`）、
/// 虚拟托管布局（平铺 哈希模块 产物，`<哈希模块>.rs`）。
pub fn 标注模块路径并行号(
    源串: &str,
    解析闭包: impl Fn(&str) -> Option<String>,
) -> (String, Vec<usize>) {
    注解模块路径实现(源串, &解析闭包)
}

/// 同 [`标注非西文模块`]，并额外返回磁盘产物行 → 引擎直出行映射
///
/// 每处注解在 `mod` 声明所在行前插入一整行 `#[路径模块 = ...]`，其后所有磁盘
/// 行号相对引擎直出产物整体偏移（偏移量 = 该行之前的注解数）。诊断回译时
/// 必须先用本映射把 rustc 报告的磁盘行号换算回引擎直出行号，再走以引擎
/// 直出产物为基准的列映射；否则多模块入口文件（如 `main.zh` 声明 3 个
/// `模块 xxx;`）的诊断行号会系统性偏移 3 行。映射为 0-based：
/// `line_map[磁盘行] = 引擎直出行`（注解行归属其所在 `mod` 声明行）。
pub fn 标注非西文模块并行号(源串: &str) -> (String, Vec<usize>) {
    注解模块路径实现(源串, &|名字| {
        (!名字.is_ascii()).then(|| format!("{名字}.rs"))
    })
}

/// [`标注模块路径并行号`] 的实现体
fn 注解模块路径实现(
    源串: &str,
    解析闭包: &dyn Fn(&str) -> Option<String>,
) -> (String, Vec<usize>) {
    let 令牌序列: Vec<_> = tokenize(源串).collect();
    // 逐 token 的字节偏移（词法解析器 词法流覆盖全源，偏移连续）
    let mut 偏移列表: Vec<usize> = Vec::with_capacity(令牌序列.len());
    let mut 累加 = 0usize;
    for 令牌 in &令牌序列 {
        偏移列表.push(累加);
        累加 += 令牌.len;
    }
    let 跳过修饰 = |mut 索引: usize| {
        while 索引 < 令牌序列.len()
            && matches!(
                令牌序列[索引].kind,
                TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
            )
        {
            索引 += 1;
        }
        索引
    };
    let mut 插入项列表: Vec<(usize, String)> = Vec::new();
    for (外索引, 令牌) in 令牌序列.iter().enumerate() {
        if 令牌.kind != TokenKind::Ident {
            continue;
        }
        let 文本段 = &源串[偏移列表[外索引]..偏移列表[外索引] + 令牌.len];
        if 文本段 != "mod" {
            continue;
        }
        let 名字索引 = 跳过修饰(外索引 + 1);
        let Some(名字令牌) = 令牌序列.get(名字索引) else {
            continue;
        };
        if 名字令牌.kind != TokenKind::Ident {
            continue;
        }
        let 分号索引 = 跳过修饰(名字索引 + 1);
        // 仅文件式声明（分号结尾）需要注解；内联模块块以 `{` 开头
        if !matches!(
            令牌序列.get(分号索引).map(|令牌项| 令牌项.kind),
            Some(TokenKind::Semi)
        ) {
            continue;
        }
        let 名字 = &源串[偏移列表[名字索引]..偏移列表[名字索引] + 名字令牌.len];
        let Some(模块路径) = 解析闭包(名字) else {
            continue;
        };
        // 插入点：若有可见性修饰 `公开`，注解必须在 公开 之前
        let mut 插入点 = 偏移列表[外索引];
        let mut 回溯 = 外索引;
        while 回溯 > 0
            && matches!(
                令牌序列[回溯 - 1].kind,
                TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
            )
        {
            回溯 -= 1;
        }
        if 回溯 > 0
            && 令牌序列[回溯 - 1].kind == TokenKind::Ident
            && &源串[偏移列表[回溯 - 1]..偏移列表[回溯 - 1] + 令牌序列[回溯 - 1].len] == "pub"
        {
            插入点 = 偏移列表[回溯 - 1];
        }
        // 已有 #[路径模块] 注解时不重复添加：注解可独占多行，需向上逐行扫描属性链
        let 行首 = 源串[..插入点]
            .rfind('\n')
            .map(|前索引| 前索引 + 1)
            .unwrap_or(0);
        let mut 扫描起点 = 行首;
        let mut 已有路径属性 = false;
        loop {
            let 行文本 = 源串[扫描起点..插入点].lines().next().unwrap_or("");
            // 首行可能是 mod 所在行的缩进前缀；上移后的行应为属性行
            if 扫描起点 != 行首 || 行文本.trim_start().starts_with("#[") {
                if 行文本.contains("#[path") {
                    已有路径属性 = true;
                    break;
                }
                if !行文本.trim_start().starts_with("#[") {
                    break; // 属性链被普通行/空行打断
                }
            }
            if 扫描起点 == 0 {
                break;
            }
            let 上一行首 = 源串[..扫描起点 - 1]
                .rfind('\n')
                .map(|前索引| 前索引 + 1)
                .unwrap_or(0);
            if 上一行首 == 扫描起点 {
                break;
            }
            扫描起点 = 上一行首;
        }
        if 已有路径属性 {
            continue;
        }
        let 缩进 = &源串[行首..插入点];
        插入项列表.push((插入点, format!("#[path = \"{模块路径}\"]\n{缩进}")));
    }
    // 按插入点升序回放：引擎直出文本的换行推进引擎行号；注解插入文本
    // 的换行不推进（插入行仍归属其所在 `mod` 声明行）
    let mut 结果串文本 = String::with_capacity(
        源串.len()
            + 插入项列表
                .iter()
                .map(|(_, 令牌项)| 令牌项.len())
                .sum::<usize>(),
    );
    let mut 行映射: Vec<usize> = vec![0];
    let mut 引擎行 = 0usize;
    let mut 上一位置 = 0usize;
    for (坐标, 文本段) in &插入项列表 {
        追加并更新行映射(
            &mut 结果串文本,
            &mut 行映射,
            &mut 引擎行,
            &源串[上一位置..*坐标],
            true,
        );
        追加并更新行映射(&mut 结果串文本, &mut 行映射, &mut 引擎行, 文本段, false);
        上一位置 = *坐标;
    }
    追加并更新行映射(
        &mut 结果串文本,
        &mut 行映射,
        &mut 引擎行,
        &源串[上一位置..],
        true,
    );
    (结果串文本, 行映射)
}

/// 追加文本并同步维护行映射：`from_engine=false`（注解插入文本）的换行
/// 不推进引擎行号，其余同普通文本
fn 追加并更新行映射(
    输出串: &mut String,
    行映射: &mut Vec<usize>,
    引擎行: &mut usize,
    文本段: &str,
    来自引擎: bool,
) {
    for 本字符 in 文本段.chars() {
        输出串.push(本字符);
        if 本字符 == '\n' {
            if 来自引擎 {
                *引擎行 += 1;
            }
            行映射.push(*引擎行);
        }
    }
}

#[cfg(test)]
mod 单元测试 {
    use super::*;

    fn 示例路径映射() -> HashMap<String, String> {
        let mut 映射表 = HashMap::new();
        映射表.insert("标准集合".to_string(), "std::collections".to_string());
        映射表.insert("文件系统".to_string(), "std::fs".to_string());
        映射表.insert("字符串".to_string(), "string".to_string());
        映射表
    }

    /// 模块路径段后跟 `::` 时被替换
    #[test]
    fn 测试替换模块路径段() {
        let 映射表 = 示例路径映射();
        let 结果串 = 模块路径替换("使用 标准集合::哈希映射;", &映射表);
        assert_eq!(结果串, "使用 std::collections::哈希映射;");
    }

    /// use 语句外的类型关联调用（如 `引用计数::新建`）不被模块映射替换，
    /// 留给别名映射处理成 `Rc::新建`
    #[test]
    fn 测试使用语句外类型调用不替换() {
        let 映射表 = 示例路径映射();
        let 结果串 = 模块路径替换("标准集合::新建(5);", &映射表);
        assert_eq!(结果串, "标准集合::新建(5);");
    }

    /// use 语句末段（后跟分号）也参与路径映射替换
    #[test]
    fn 测试使用语句末段替换() {
        let 映射表 = 示例路径映射();
        let 结果串 = 模块路径替换("使用 标准集合::字符串;", &映射表);
        assert_eq!(结果串, "使用 std::collections::string;");
    }

    /// use 语句单段路径（无 `::`）直接映射整个模块
    #[test]
    fn 测试使用语句单段替换() {
        let 映射表 = 示例路径映射();
        let 结果串 = 模块路径替换("使用 文件系统;", &映射表);
        assert_eq!(结果串, "使用 std::fs;");
    }

    /// 映射表未覆盖的末段保持原样，交给别名替换处理（如 `服务器` → `Server`）
    #[test]
    fn 测试使用语句末段未映射保留() {
        let 映射表 = 示例路径映射();
        let 结果串 = 模块路径替换("使用 salvo::服务器;", &映射表);
        assert_eq!(结果串, "使用 salvo::服务器;");
    }

    /// 包 名含连字符时（Cargo 名 ≠ Rust 路径名）：替换值规范化为下划线形式
    #[test]
    fn 测试使用语句连字符包名规范化() {
        let 映射表 = HashMap::from([("日志订阅".to_string(), "tracing-subscriber".to_string())]);
        let 结果串 = 模块路径替换并映射("使用 日志订阅 as 日志框架;", &映射表);
        assert_eq!(结果串.产出, "使用 tracing_subscriber as 日志框架;");
        assert_eq!(结果串.编辑表.len(), 1);
        assert_eq!(结果串.编辑表[0].替换文本, "tracing_subscriber");
    }

    /// 无连字符的路径段不受规范化影响
    #[test]
    fn 测试使用语句普通路径不规范() {
        let 映射表 = 示例路径映射();
        let 结果串 = 模块路径替换("使用 标准集合::哈希映射;", &映射表);
        assert_eq!(结果串, "使用 std::collections::哈希映射;");
    }

    /// use 语句结束后（分号后）恢复默认行为
    #[test]
    fn 测试使用语句作用域止于分号() {
        let 映射表 = 示例路径映射();
        let 结果串 = 模块路径替换("使用 标准集合::哈希映射; 标准集合::新建(1);", &映射表);
        assert_eq!(
            结果串,
            "使用 std::collections::哈希映射; 标准集合::新建(1);"
        );
    }

    /// 组合词内部的中文不被破坏（token 级替换）
    #[test]
    fn 测试组合词不被破坏() {
        let 映射表 = 示例路径映射();
        let 结果串 = 模块路径替换("读取全部字符串(\"a.txt\")", &映射表);
        assert_eq!(结果串, "读取全部字符串(\"a.txt\")");
    }

    /// 普通中文变量名（后无 `::`）不被替换
    #[test]
    fn 测试普通标识符不替换() {
        let 映射表 = 示例路径映射();
        let 结果串 = 模块路径替换("让 字符串 = 1;", &映射表);
        assert_eq!(结果串, "让 字符串 = 1;");
    }

    /// 空映射表直接返回原文
    #[test]
    fn 测试空映射表() {
        let 结果串 = 模块路径替换("让 x = 1;", &HashMap::new());
        assert_eq!(结果串, "让 x = 1;");
    }

    /// 编辑表：输入偏移精确指向被替换的路径段
    #[test]
    fn 测试带映射表记录编辑() {
        let 映射表 = 示例路径映射();
        let 结果串 = 模块路径替换并映射("使用 标准集合::哈希映射;", &映射表);
        assert_eq!(结果串.产出, "使用 std::collections::哈希映射;");
        assert_eq!(结果串.编辑表.len(), 1);
        let 编辑项 = &结果串.编辑表[0];
        assert_eq!(编辑项.原文, "标准集合");
        assert_eq!(编辑项.替换文本, "std::collections");
        assert_eq!(
            &"使用 标准集合::哈希映射;"[编辑项.源偏移..编辑项.源偏移 + 编辑项.字节长度],
            "标准集合"
        );
    }

    // ===== qualify（包:: 前缀）测试（自 lsp 包 迁移） =====

    #[test]
    fn 测试补前缀添加前缀() {
        let 集合 = HashSet::from(["辅助".to_string(), "主".to_string()]);
        assert_eq!(
            模块路径限定并映射(
                "fn main() {\n    辅助::辅助函数();\n}",
                &集合,
                &HashSet::new()
            )
            .产出,
            "fn main() {\n    crate::辅助::辅助函数();\n}"
        );
    }

    #[test]
    fn 测试补前缀不重复加前缀() {
        let 集合 = HashSet::from(["辅助".to_string(), "主".to_string()]);
        assert_eq!(
            模块路径限定并映射("crate::辅助::辅助函数()", &集合, &HashSet::new()).产出,
            "crate::辅助::辅助函数()"
        );
        // 其他路径段内的模块名（测试甲::辅助）不重复处理
        assert_eq!(
            模块路径限定并映射("a::辅助::辅助函数()", &集合, &HashSet::new()).产出,
            "a::辅助::辅助函数()"
        );
    }

    #[test]
    fn 测试补前缀非模块不动() {
        let 集合 = HashSet::from(["辅助".to_string(), "主".to_string()]);
        assert_eq!(
            模块路径限定并映射("x::方法()", &集合, &HashSet::new()).产出,
            "x::方法()"
        );
    }

    #[test]
    fn 测试补前缀空集保留原文() {
        assert_eq!(
            模块路径限定并映射("辅助::辅助函数()", &HashSet::new(), &HashSet::new()).产出,
            "辅助::辅助函数()"
        );
    }

    #[test]
    fn 测试补前缀记录编辑() {
        let 集合 = HashSet::from(["辅助".to_string()]);
        let 结果串 = 模块路径限定并映射("fn f() { 辅助::x() }", &集合, &HashSet::new());
        assert_eq!(结果串.产出, "fn f() { crate::辅助::x() }");
        assert_eq!(结果串.编辑表.len(), 1);
        let 编辑项 = &结果串.编辑表[0];
        assert_eq!(编辑项.原文, "辅助");
        assert_eq!(编辑项.替换文本, "crate::辅助");
    }

    /// 同文件类型声明名与模块名重名：裸路径按本地类型解析，不加前缀
    /// （weix-1 `struct 项目配置` 与 `模块 项目配置` 重名的 E0425 回归）
    #[test]
    fn 测试补前缀被类型声明遮蔽() {
        let 集合 = HashSet::from(["项目配置".to_string()]);
        let 源码串 = "struct 项目配置 {}\nfn f() { 项目配置::default() }";
        assert_eq!(
            模块路径限定并映射(源码串, &集合, &HashSet::new()).产出,
            源码串
        );
    }

    /// use 导入的类型名与模块名重名：裸路径解析为导入类型，不加前缀
    #[test]
    fn 测试补前缀被使用导入遮蔽() {
        let 集合 = HashSet::from(["项目配置".to_string()]);
        let 源码串 = "use crate::项目配置::项目配置;\nfn f() { 项目配置::default() }";
        assert_eq!(
            模块路径限定并映射(源码串, &集合, &HashSet::new()).产出,
            源码串
        );
    }

    /// glob 导入 + 项目项名命中：可能经 glob 引入作用域，不加前缀；
    /// 无 glob 时项目项名不影响前缀（模块引用照旧补全）
    #[test]
    fn 测试补前缀通配含项名() {
        let 集合 = HashSet::from(["项目配置".to_string()]);
        let 项集 = HashSet::from(["项目配置".to_string()]);
        let 源码串 = "use super::*;\nfn f() { 项目配置::default() }";
        assert_eq!(模块路径限定并映射(源码串, &集合, &项集).产出, 源码串);
        assert_eq!(
            模块路径限定并映射("fn f() { 项目配置::load() }", &集合, &项集).产出,
            "fn f() { crate::项目配置::load() }"
        );
    }

    /// mod 声明名不豁免：虚拟项目会抹除文件式声明，前缀仍须补全
    #[test]
    fn 测试补前缀模块声明不遮蔽() {
        let 集合 = HashSet::from(["工具".to_string()]);
        assert_eq!(
            模块路径限定并映射(
                "mod 工具;\nfn f() { 工具::加一() }",
                &集合,
                &HashSet::new()
            )
            .产出,
            "mod 工具;\nfn f() { crate::工具::加一() }"
        );
    }

    /// glob 来源段不是绑定名：`use 包::工具::*;` 不遮蔽 `工具`
    /// （glob 场景由项目项名另行判定；此处项名集合为空，照旧加前缀）
    #[test]
    fn 测试补前缀通配来源段非绑定() {
        let 集合 = HashSet::from(["工具".to_string()]);
        assert_eq!(
            模块路径限定并映射(
                "use crate::工具::*;\nfn f() { 工具::加一() }",
                &集合,
                &HashSet::new()
            )
            .产出,
            "use crate::工具::*;\nfn f() { crate::工具::加一() }"
        );
    }

    // ===== strip（文件式模块声明抹除）测试 =====

    /// 文件式模块声明被整体抹除为空格，且行号不变
    #[test]
    fn 测试抹除文件模块声明() {
        let 输入串 = "mod 日志设置;\nfn main() {}";
        let 输出串 = 剥离文件模块声明(输入串);
        assert_eq!(输出串, "         \nfn main() {}");
        assert_eq!(输出串.lines().count(), 输入串.lines().count());
    }

    /// 可见性修饰 `公开` 随声明一并抹除
    #[test]
    fn 测试抹除公开模块声明() {
        let 输入串 = "pub mod 工具;\nfn f() {}";
        let 输出串 = 剥离文件模块声明(输入串);
        assert_eq!(输出串, "           \nfn f() {}");
    }

    /// 可见性括号形式 `公开(包)` 随声明一并抹除
    #[test]
    fn 测试抹除公开包内模块声明() {
        let 输入串 = "pub(crate) mod 工具;\nfn f() {}";
        let 输出串 = 剥离文件模块声明(输入串);
        assert_eq!(输出串, "                  \nfn f() {}");
    }

    /// 属性与文档注释随声明一并抹除（避免悬空修饰）
    #[test]
    fn 测试抹除带属性模块声明() {
        // `#[cfg(test)]`（12 字符）与 `mod 测试;`（7 字符）分别变为等宽空格
        let 输入串 = "#[cfg(test)]\nmod 测试;\nfn f() {}";
        let 输出串 = 剥离文件模块声明(输入串);
        assert_eq!(
            输出串,
            format!("{}\n{}\nfn f() {{}}", " ".repeat(12), " ".repeat(7))
        );

        // `/// 工具模块`（8 字符）与 `mod 工具;`（7 字符）分别变为等宽空格
        let 输入串 = "/// 工具模块\nmod 工具;";
        let 输出串 = 剥离文件模块声明(输入串);
        assert_eq!(输出串, format!("{}\n{}", " ".repeat(8), " ".repeat(7)));
    }

    /// 多行属性随声明一并抹除，且每行 UTF-16 列宽严格不变
    #[test]
    fn 测试抹除多行属性保位置() {
        let 输入串 = "#[cfg(\n    all(test, unix)\n)]\nmod 工具;\nfn f() {}";
        let 输出串 = 剥离文件模块声明(输入串);
        let 输入行: Vec<&str> = 输入串.lines().collect();
        let 输出行: Vec<&str> = 输出串.lines().collect();
        assert_eq!(输入行.len(), 输出行.len());
        for (测试甲, 测试乙) in 输入行.iter().zip(&输出行) {
            assert_eq!(测试甲.encode_utf16().count(), 测试乙.encode_utf16().count());
        }
        assert!(输出串.ends_with("fn f() {}"));
    }

    /// 内联模块（含其内容）不被处理
    #[test]
    fn 测试内联模块保留() {
        let 输入串 = "mod 工具 {\n    fn f() {}\n}\nfn main() {}";
        assert_eq!(剥离文件模块声明(输入串), 输入串);
    }

    /// 内联模块内的文件式声明同样被抹除
    #[test]
    fn 测试抹除内联中嵌套文件模块() {
        let 输入串 = "mod 外层 {\n    mod 内层;\n}";
        let 输出串 = 剥离文件模块声明(输入串);
        assert_eq!(输出串, "mod 外层 {\n           \n}");
    }

    /// 属性属于上一个项时不随声明抹除
    #[test]
    fn 测试归属前项属性保留() {
        let 输入串 = "#[cfg(test)]\nfn f() {}\nmod 工具;";
        let 输出串 = 剥离文件模块声明(输入串);
        assert_eq!(输出串, "#[cfg(test)]\nfn f() {}\n       ");
    }

    /// 含 mod 子串的标识符不被误判
    #[test]
    fn 测试模块子串标识符不抹除() {
        let 输入串 = "fn commode() {}\nfn mode() {}";
        assert_eq!(剥离文件模块声明(输入串), 输入串);
    }

    /// 无文件式声明时原样返回
    #[test]
    fn 测试无声明不改动() {
        let 输入串 = "fn main() {}\nfn f() { let mode = 1; }";
        assert_eq!(剥离文件模块声明(输入串), 输入串);
    }

    /// 非 ASCII 文件式模块声明补充 #[路径模块] 注解；ASCII 名与内联模块不受影响
    #[test]
    fn 测试注解非ascii模块() {
        let 源码串 = "#[path = \"数学.rs\"]\nmod 数学;";
        assert_eq!(标注非西文模块(源码串), 源码串);
        assert_eq!(标注非西文模块("mod ascii;"), "mod ascii;");
        assert_eq!(标注非西文模块("mod 工具 {}"), "mod 工具 {}");
        let 输出串 = 标注非西文模块("fn main() {\n    mod 数学;\n}");
        assert_eq!(
            输出串,
            "fn main() {\n    #[path = \"数学.rs\"]\n    mod 数学;\n}"
        );
    }

    /// 注解行映射：磁盘行 → 引擎直出行（0-based），注解行归属其 mod 声明行，
    /// 多处注解时偏移逐处累积
    #[test]
    fn 测试注解非ascii模块行映射() {
        let (输出串, 行映射) = 标注非西文模块并行号("mod 数学;\nfn main() {}");
        assert_eq!(输出串, "#[path = \"数学.rs\"]\nmod 数学;\nfn main() {}");
        assert_eq!(行映射, vec![0, 0, 1]);

        let (_, 行映射) = 标注非西文模块并行号("fn main() {}\nfn f() {}");
        assert_eq!(行映射, vec![0, 1]);

        let 双声明 = "mod 数学;\nmod 物理;\nfn main() {}";
        let (输出串, 行映射) = 标注非西文模块并行号(双声明);
        assert_eq!(
            输出串,
            "#[path = \"数学.rs\"]\nmod 数学;\n#[path = \"物理.rs\"]\nmod 物理;\nfn main() {}"
        );
        assert_eq!(行映射, vec![0, 0, 1, 1, 2]);
    }
}
