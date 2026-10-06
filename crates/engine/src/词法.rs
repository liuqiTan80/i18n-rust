//! 词法处理模块 - 将母语源码根据关键字映射转译为标准 Rust 源码
//!
//! 基于 词法解析器 的 token 级转译，保证注释与字符串字面量内容不被误改。
//! 支持宏感叹号自动补充、原始标识符（r#）处理、反向转译（Rust → 母语）。

use crate::缓存::源映射条目;
use rustc_lexer::{TokenKind, tokenize};
use std::collections::{HashMap, HashSet};

/// 转译结果：标准 Rust 输出、词法阶段源映射与全管线编辑表
#[derive(Debug, Clone)]
pub struct 词法产出 {
    /// 转译后的代码文本
    pub 产出: String,
    /// 词法阶段源映射条目列表（记录被替换的标识符，replacement 不含补的 `!`）
    pub 源映射: Vec<源映射条目>,
    /// 全管线编辑表：与 源映射 同偏移，但 replacement 为**最终输出文本**
    ///（宏调用自动补充的 `!` 已计入 replacement），供全管线编辑地图组合使用
    pub 最终编辑表: Vec<源映射条目>,
}

/// 将母语 Rust 源代码转换为标准 Rust 源代码字符串
///
/// 参数：
///   source: 母语源代码（如 .zh 文件内容）
///   关键词映射表: 母语关键字到英文关键字的映射表
/// 返回：标准 Rust 源代码
pub fn 词法转译(源码: &str, 关键字映射表: &HashMap<String, String>) -> String {
    let 空映射 = HashMap::new();
    词法转译并宏映射(源码, 关键字映射表, &空映射, &HashMap::new())
}

/// 将母语 Rust 源代码转换为标准 Rust 源代码字符串（支持宏感叹号自动补充）
///
/// 参数：
///   source: 母语源代码（如 .zh 文件内容）
///   关键词映射表: 母语关键字到英文关键字的映射表
///   macro_names: 所有中文宏名的集合（不含感叹号），用于自动补充 `!`
/// 返回：标准 Rust 源代码
///
/// 注意：宏名的英文替换优先使用宏映射（见 [`词法转译并宏映射`]）；
/// 本函数以 关键词映射表 兜底（宏名在 关键词映射表 中被类型节覆盖时值可能不准，
/// 如 `向量` 在类型节映射为 `新建向量`、在宏节映射为 `向量宏`）。
pub fn 词法转译并宏(
    源码: &str,
    关键字映射表: &HashMap<String, String>,
    宏名集: &HashSet<String>,
) -> String {
    let 宏映射表: HashMap<String, String> = 宏名集
        .iter()
        .map(|名字| {
            (
                名字.clone(),
                关键字映射表
                    .get(名字)
                    .cloned()
                    .unwrap_or_else(|| 名字.clone()),
            )
        })
        .collect();
    词法转译并映射(
        源码,
        关键字映射表,
        &宏映射表,
        &HashMap::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
    )
    .产出
}

/// 同 [`词法转译并宏`]，但宏名英文替换来自宏映射表
/// （宏名 → 英文宏名，如 `向量` → `向量宏`），可避免宏名在类型节与宏节
/// 重复时被类型值覆盖（如 `向量` 类型节为 `新建向量`、宏节为 `向量宏`）。
pub fn 词法转译并宏映射(
    源码: &str,
    关键字映射表: &HashMap<String, String>,
    宏映射表: &HashMap<String, String>,
    派生映射表: &HashMap<String, String>,
) -> String {
    词法转译并映射(
        源码,
        关键字映射表,
        宏映射表,
        派生映射表,
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
    )
    .产出
}

/// 同 [`词法转译并宏`]，同时产出源映射（被替换标识符的源偏移与翻译前后文本）
///
/// 源映射在标识符实际被替换时记录；宏感叹号自动补充不产生映射条目。
///
/// `defer_in_use`：use 语句内的让位词集合（模块路径/别名映射键，见
/// [`包::映射管理::映射管理器::取使用延迟词`]）。
/// use 段内命中该集合的词跳过关键字/宏替换，原样保留给后续阶段处理。
///
/// `method_defer`：方法位让位词集合（见
/// [`包::映射管理::映射管理器::取方法延迟词`]）——
/// 词法替换值是保留关键字的词（`枚举`→enum）在方法位（前面是 `.`）原样
/// 保留：`.枚举()` 直译会成非法的 `.enum()`，让位后由别名阶段替换为
/// `.枚举()`。
///
/// `literal_suffix_map`：数字字面量后缀的兜底查表（标准库标识符映射，见
/// [`包::映射管理::映射管理器::取别名映射表`]）——标准库层
/// 数值类型词（如 `无符号机器整数` = 无符号机器整数）不在 关键词映射表，黏连数字后
/// 同样需要拆分替换。
pub fn 词法转译并映射(
    源码: &str,
    关键字映射表: &HashMap<String, String>,
    宏映射表: &HashMap<String, String>,
    派生映射表: &HashMap<String, String>,
    使用段让位词: &HashSet<String>,
    方法位让位词: &HashSet<String>,
    字面量后缀表: &HashMap<String, String>,
) -> 词法产出 {
    // 收集所有 token 以便前瞻/后顾
    let 令牌流: Vec<_> = tokenize(源码).collect();
    // 预计算每个 token 的文本（派生属性上下文回溯需要前一个标识符的内容）
    let 令牌文本: Vec<&str> = {
        let mut 偏移 = 0;
        令牌流
            .iter()
            .map(|令牌项| {
                let 字符串项 = &源码[偏移..偏移 + 令牌项.len];
                偏移 += 令牌项.len;
                字符串项
            })
            .collect()
    };
    let mut 产出 = String::new();
    let mut 当前偏移 = 0;
    let mut 源映射 = Vec::new();
    // 全管线编辑表：replacement 为最终输出文本（宏补的 `!` 计入），与 源映射 同偏移
    let mut 最终编辑表 = Vec::new();
    // 派生参数态：`#[派生(` 之后的参数（直到 `)`）内的标识符
    // 优先查派生特征映射（`克隆` → `Clone`），避免与方法名别名（小写 克隆值）冲突
    let mut 派生参数态 = false;
    // use 语句态：use 段内的路径段命中"让位词"（模块路径/别名映射键）时
    // 跳过关键字/宏替换，交由后续模块路径与别名阶段按标准库语义替换——
    // 同名词在宏表与标准库表重复时 use 段内标准库优先（`文件`=file/File，
    // `使用 标准库::文件系统::文件` 必须产出 `use 标准库::文件系统::File` 而非小写 file）
    let mut 使用语句态 = false;

    for 下标 in 0..令牌流.len() {
        let 令牌 = &令牌流[下标];
        let 令牌长度 = 令牌.len as usize;
        let 文本段 = &源码[当前偏移..当前偏移 + 令牌长度];

        match 令牌.kind {
            TokenKind::Ident | TokenKind::RawIdent => {
                let 裸名 = 文本段.strip_prefix("r#").unwrap_or(文本段);

                // 宏调用上下文优先：宏名后跟 `!` 或 `(/[/{`（且前面不是 `::`）时，
                // 使用宏映射替换为英文宏名并确保感叹号，避免宏名在类型节与宏节
                // 重复时被类型值覆盖（如 `向量` 类型节为 `新建向量`、宏节为 `向量宏`）。
                // 方法调用位（前面是 `.`）不适用：宏不经 `.` 访问，
                // `.格式化(` 是方法调用，不补 `!`（后续走关键字映射 → `.格式化提示(`）
                let mut 已处理 = false;
                if 宏映射表.contains_key(裸名)
                    && !前接双冒号(&令牌流, 下标)
                    && !前接点号(&令牌流, 下标)
                    && !是否为属性名(&令牌流, 下标)
                    && !(使用语句态 && 使用段让位词.contains(裸名))
                    && let Some(后继形态) = 查找后继非空白形态(&令牌流, 下标 + 1)
                {
                    let 是否叹号 = matches!(后继形态, TokenKind::Not);
                    if 是否开括号(后继形态) || 是否叹号 {
                        let 英文宏名 = 宏映射表.get(裸名).cloned().unwrap_or_else(|| {
                            关键字映射表
                                .get(裸名)
                                .cloned()
                                .unwrap_or_else(|| 文本段.to_string())
                        });
                        let 最终文本 = if 文本段.starts_with("r#") && !英文宏名.starts_with("r#")
                        {
                            format!("r#{}", 英文宏名)
                        } else {
                            英文宏名
                        };
                        if 最终文本 != 文本段 {
                            源映射.push(源映射条目::新建条目(
                                当前偏移,
                                令牌长度,
                                文本段,
                                &最终文本,
                            ));
                        }
                        // 最终输出文本（含自动补充的 `!`）：
                        // 与 源映射 语义不同——replacement 即输出原文，供列映射回放
                        let 追加文本 = if 是否开括号(后继形态) {
                            format!("{}!", 最终文本)
                        } else {
                            最终文本.clone()
                        };
                        if 追加文本 != 文本段 {
                            最终编辑表.push(源映射条目::新建条目(
                                当前偏移,
                                令牌长度,
                                文本段,
                                &追加文本,
                            ));
                        }
                        产出.push_str(&最终文本);
                        // 后跟开括号但无 `!` 时自动补 `!`
                        if 是否开括号(后继形态) {
                            产出.push('!');
                        }
                        已处理 = true;
                    }
                }

                if !已处理 {
                    // 派生参数态内：优先查派生特征映射（如 `克隆` → `Clone`）
                    let 派生替换 = if 派生参数态 {
                        派生映射表.get(裸名).cloned()
                    } else {
                        None
                    };
                    let 替换值 = if 使用语句态 && 使用段让位词.contains(裸名) {
                        // use 段让位：保持原文，由模块路径/别名阶段替换
                        文本段.to_string()
                    } else if 方法位让位词.contains(裸名) && 前接点号(&令牌流, 下标)
                    {
                        // 方法位让位：替换值是保留关键字，直译非法（`.枚举()` →
                        // `.enum()`）；保持原文由别名阶段替换为有效词条（枚举）
                        文本段.to_string()
                    } else if let Some(英文值) = 派生替换 {
                        英文值
                    } else if let Some(内层) = 文本段.strip_prefix("r#") {
                        关键字映射表
                            .get(内层)
                            .map(|英文词| format!("r#{}", 英文词))
                            .unwrap_or_else(|| 文本段.to_string())
                    } else {
                        关键字映射表
                            .get(文本段)
                            .cloned()
                            .unwrap_or_else(|| 文本段.to_string())
                    };

                    // `宏规则`（macro_rules）声明形式：后跟宏名时必须带 `!`，
                    // 如 `宏规则 创建向量 { ... }` → `macro_rules! 创建向量 { ... }`
                    let 宏声明 = 替换值 == "macro_rules"
                        && matches!(
                            查找后继非空白形态(&令牌流, 下标 + 1),
                            Some(TokenKind::Ident)
                        );
                    let 最终替换 = if 宏声明 {
                        "macro_rules!"
                    } else {
                        替换值.as_str()
                    };

                    // 记录实际发生替换的标识符映射
                    if 最终替换 != 文本段 {
                        源映射.push(源映射条目::新建条目(
                            当前偏移,
                            令牌长度,
                            文本段,
                            最终替换,
                        ));
                        最终编辑表.push(源映射条目::新建条目(
                            当前偏移,
                            令牌长度,
                            文本段,
                            最终替换,
                        ));
                    }
                    产出.push_str(最终替换);
                }
                // use 语句态推进：本语言 use 关键字词形开启让位态，由 `;` 结束。
                // 词形经关键字映射反查（zh/ja `使用`、es/pt `usar`、ru `используй`
                // 等全部本地词形），不硬编码字面 `use`/`使用`——历史实现只识别
                // 这两种，其余语言 use 段让位态从未开启，冲突词（如 es `formato`
                // 宏 format 与模块 fmt 同名）被词法替换后残留英文宏名 `format::`
                if 文本段 == "use" || 关键字映射表.get(文本段).is_some_and(|值项| 值项 == "use")
                {
                    使用语句态 = true;
                }
            }
            TokenKind::OpenParen => {
                // 派生属性：`派生`（转译为 derive）后跟 `(` 进入参数态
                if !派生参数态
                    && 前一标识符是否(&令牌流, &令牌文本, 下标, 关键字映射表, "derive")
                {
                    派生参数态 = true;
                }
                产出.push_str(文本段);
            }
            TokenKind::CloseParen => {
                // 离开派生参数态
                派生参数态 = false;
                产出.push_str(文本段);
            }
            // use 语句以分号结束，退出让位态
            TokenKind::Semi => {
                使用语句态 = false;
                产出.push_str(文本段);
            }
            // 数字字面量：数字与中文类型"黏连"（`0无符号微整数`）时，
            // 词法解析器 把整串（含 Unicode 后缀）切为单个 字面量记号 token，
            // 后缀是完整映射词（keywords 表或标准库表：`无符号微整数` → 无符号微整数、
            // `无符号机器整数` → 无符号机器整数）时替换后缀（产出 `0u8`/`0usize`）；
            // 未命中保持原样（编译错误交由编译器报告）
            TokenKind::Literal { suffix_start, .. } => {
                let 后缀 = &文本段[suffix_start..];
                if !后缀.is_empty()
                    && !后缀.is_ascii()
                    && let Some(英文值) = 关键字映射表.get(后缀).or_else(|| 字面量后缀表.get(后缀))
                {
                    let 数字串 = &文本段[..suffix_start];
                    let 替换后 = format!("{}{}", 数字串, 英文值);
                    let 编辑条目 = 源映射条目::新建条目(
                        当前偏移 + suffix_start,
                        后缀.len(),
                        后缀,
                        英文值,
                    );
                    源映射.push(编辑条目.clone());
                    最终编辑表.push(编辑条目);
                    产出.push_str(&替换后);
                } else {
                    产出.push_str(文本段);
                }
            }
            // 其他所有 token 直接原样输出
            _ => 产出.push_str(文本段),
        }
        当前偏移 += 令牌长度;
    }

    词法产出 {
        产出,
        源映射,
        最终编辑表,
    }
}

/// 查找从指定位置开始的第一个非空白 token 的 令牌类别
fn 查找后继非空白形态(
    令牌流: &[rustc_lexer::Token],
    起点下标: usize,
) -> Option<TokenKind> {
    for 令牌 in &令牌流[起点下标..] {
        if !是否为空白符(令牌.kind) {
            return Some(令牌.kind);
        }
    }
    None
}

/// 判断当前位置之前的第一个非空白标识符（经关键字映射后）是否等于指定英文值
///
/// 用于识别派生属性：`#[派生(...)]` 中 `派生` 的映射值为 `derive`。
fn 前一标识符是否(
    令牌流: &[rustc_lexer::Token],
    令牌文本: &[&str],
    当前位置: usize,
    关键字映射表: &HashMap<String, String>,
    英文值: &str,
) -> bool {
    let mut 内索引 = 当前位置;
    while 内索引 > 0 {
        内索引 -= 1;
        let 形态 = 令牌流[内索引].kind;
        if 是否为空白符(形态) {
            continue;
        }
        if matches!(形态, TokenKind::Ident | TokenKind::RawIdent) {
            let 名字 = 令牌文本[内索引]
                .strip_prefix("r#")
                .unwrap_or(令牌文本[内索引]);
            return 关键字映射表
                .get(名字)
                .map(|值项| 值项 == 英文值)
                .unwrap_or(false);
        }
        return false;
    }
    false
}

/// 将标准 Rust 源码反向转译为母语源码
///
/// 参数：
///   source: 标准 Rust 源代码（如 rustfmt 格式化后的虚拟 .rs 内容）
///   reverse_map: 英文关键字到母语关键字的映射表（由正向映射反转得到）
///   module_names: 当前已打开 .zh 文件的模块名集合。
///              代理为跨文件引用插入的 `包::` 前缀在此被删除，
///              还原为母语中的裸路径（`辅助::`），与正向翻译的
///              `模块路径替换` 互为逆操作。
/// 返回：母语源代码
///
/// 以 token 为单位匹配，天然避免子串误替换
/// （如 `i32` 不会被更短的 `i3` 错误替换），
/// 注释与字符串字面量内容保持原样，与正向翻译一一对应。
///
/// `added_crate_tokens` 为代理（LSP 虚拟项目）添加的 `包::` 前缀在英文
/// 输出中的非空白 token 序号（见 [`补包标记下标`]）：仅删除这些
/// 位置的前缀；用户显式书写的 `包::`（不在集合中）原样保留，反向转译
/// 不再无差别删除，避免还原后代码丢失用户手写前缀。
pub fn 逆向转译(
    源码: &str,
    反向映射: &HashMap<String, String>,
    模块名集: &HashSet<String>,
    代理前缀序号集: &HashSet<usize>,
) -> String {
    // 收集 (token 种类, 文本) 对以便前瞻/后顾
    let 令牌流: Vec<(TokenKind, &str)> = {
        let mut 列表 = Vec::new();
        let mut 偏移 = 0;
        for 令牌 in tokenize(源码) {
            列表.push((令牌.kind, &源码[偏移..偏移 + 令牌.len]));
            偏移 += 令牌.len;
        }
        列表
    };
    // 预处理：每个 token 的非空白序号（rustfmt 等空白变化不影响序号，
    // 序号与 [`补包标记下标`] 的计数口径一致）
    let mut 非空白序号表 = Vec::with_capacity(令牌流.len());
    let mut 非空白计数 = 0usize;
    for (形态, _) in &令牌流 {
        非空白序号表.push(非空白计数);
        if !是否为空白符(*形态) {
            非空白计数 += 1;
        }
    }
    let mut 输出文本 = String::with_capacity(源码.len());
    let mut 跳过令牌数 = 0usize; // 删除 包:: 前缀 / macro_rules! 感叹号时跳过的 token 数

    for 下标 in 0..令牌流.len() {
        if 跳过令牌数 > 0 {
            跳过令牌数 -= 1;
            continue;
        }
        let (令牌, 文本段) = &令牌流[下标];
        match 令牌 {
            TokenKind::Ident | TokenKind::RawIdent => {
                let 裸名 = if let Some(内层) = 文本段.strip_prefix("r#") {
                    内层
                } else {
                    文本段
                };
                // 代理为跨文件引用插入的 `包::` 前缀：仅当序号命中
                // 正向翻译记录（added_crate_tokens）时才整体删除；
                // 用户手写的 `包::` 不在记录中，原样保留
                // （跳过数含中间空白，兼容 `包 :: 模块` 等带空格写法）
                if 裸名 == "crate"
                    && 代理前缀序号集.contains(&非空白序号表[下标])
                    && let Some(跳过数) = 包前缀跳过数(&令牌流, 下标, 模块名集)
                {
                    跳过令牌数 = 跳过数;
                    continue;
                }
                // `macro_rules! 名称` 声明形式反向转译为 `宏规则 名称`（省略感叹号）
                if 裸名 == "macro_rules"
                    && let Some(跳过数) = 感叹后缀跳过数(&令牌流, 下标)
                {
                    跳过令牌数 = 跳过数;
                }
                let 母语名 = 反向映射
                    .get(裸名)
                    .map(|字符串项| 字符串项.as_str())
                    .unwrap_or(裸名);
                if 文本段.starts_with("r#") && !母语名.starts_with("r#") {
                    输出文本.push_str("r#");
                }
                输出文本.push_str(母语名);
            }
            _ => 输出文本.push_str(文本段),
        }
    }
    输出文本
}

/// 计算全管线编辑地图中“代理添加的 `包::` 前缀”在英文输出中的非空白 token 序号
///
/// 输入为最终英文输出文本与其全管线编辑地图（replacement 为最终输出文本）。
/// 净长度增量线性累积后，把 replacement 以 `包::` 开头的条目（仅来自 LSP
/// 虚拟项目的跨文件引用前缀重写，用户手写前缀不产生编辑条目）的母语源偏移
/// 换算为英文输出偏移，再转为非空白 token 序号。
///
/// rustfmt 格式化不改变非空白 token 序列（只改变空白），序号在格式化前后
/// 保持一致，供 [`逆向转译`] 对格式化后的文本精确删除前缀。
pub fn 补包标记下标(
    英文内容: &str, 管线映射表: &[源映射条目]
) -> HashSet<usize> {
    // 1. 英文输出偏移 = 母语源偏移 + 之前全部条目的净长度增量
    //（编辑地图以母语源偏移升序记录且条目互不重叠，线性累积即可）
    let mut 英文偏移集 = HashSet::new();
    let mut 增量 = 0i64;
    for 错误值 in 管线映射表 {
        if 错误值.替换文本.starts_with("crate::") {
            英文偏移集.insert((错误值.源偏移 as i64 + 增量) as usize);
        }
        增量 += 错误值.替换文本.len() as i64 - 错误值.字节长度 as i64;
    }
    if 英文偏移集.is_empty() {
        return HashSet::new();
    }
    // 2. 偏移 → 非空白 token 序号（与 [`逆向转译`] 计数口径一致）
    let mut 序号集 = HashSet::new();
    let mut 偏移 = 0usize;
    let mut 非空白计数 = 0usize;
    for 令牌 in tokenize(英文内容) {
        if !是否为空白符(令牌.kind) {
            if 英文偏移集.contains(&偏移) {
                序号集.insert(非空白计数);
            }
            非空白计数 += 1;
        }
        偏移 += 令牌.len;
    }
    序号集
}

/// `包::模块名` 前缀成立时，返回 `包` 之后需跳过的 token 数（空白 + 两个冒号）；
/// 不成立（冒号不足/后非已知模块名）返回 无
fn 包前缀跳过数(
    令牌流: &[(TokenKind, &str)],
    当前位置: usize,
    模块名集: &HashSet<String>,
) -> Option<usize> {
    let mut 序号 = 当前位置 + 1;
    let mut 跳过数 = 0usize;
    let mut 冒号数 = 0usize;
    while 序号 < 令牌流.len() {
        let (形态, 文本段) = 令牌流[序号];
        if 是否为空白符(形态) {
            跳过数 += 1;
            序号 += 1;
            continue;
        }
        if 冒号数 < 2 && matches!(形态, TokenKind::Colon) {
            冒号数 += 1;
            跳过数 += 1;
            序号 += 1;
            continue;
        }
        // 两个冒号后的第一个非空白 token 必须是已知模块名才视为代理插入的前缀
        if 冒号数 == 2 && matches!(形态, TokenKind::Ident) && 模块名集.contains(文本段)
        {
            return Some(跳过数);
        }
        return None;
    }
    None
}

/// `macro_rules` 后跟感叹号（可隔空白）时，返回需跳过的 token 数（空白 + 感叹号）
fn 感叹后缀跳过数(令牌流: &[(TokenKind, &str)], 当前位置: usize) -> Option<usize> {
    let mut 跳过数 = 0usize;
    for (形态, _) in &令牌流[当前位置 + 1..] {
        if 是否为空白符(*形态) {
            跳过数 += 1;
            continue;
        }
        return if matches!(形态, TokenKind::Not) {
            Some(跳过数 + 1)
        } else {
            None
        };
    }
    None
}

/// 判断 token 是否为空白
fn 是否为空白符(形态: TokenKind) -> bool {
    matches!(
        形态,
        TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
    )
}

/// 判断 token 是否为开括号（( [ {）
fn 是否开括号(形态: TokenKind) -> bool {
    matches!(
        形态,
        TokenKind::OpenParen | TokenKind::OpenBracket | TokenKind::OpenBrace
    )
}

/// 检查指定位置之前的非空白 token 是否构成 ::（双冒号）
fn 前接双冒号(令牌流: &[rustc_lexer::Token], 当前位置: usize) -> bool {
    // 从当前位置往前找，跳过空白，找前两个非空白 token
    let mut 前一形态 = None;
    let mut 前二形态 = None;

    let mut 内索引 = 当前位置;
    while 内索引 > 0 {
        内索引 -= 1;
        if !是否为空白符(令牌流[内索引].kind) {
            if 前一形态.is_none() {
                前一形态 = Some(令牌流[内索引].kind);
            } else if 前二形态.is_none() {
                前二形态 = Some(令牌流[内索引].kind);
                break;
            }
        }
    }

    // 前一个和前两个都是冒号 → 前面是 ::
    matches!(前一形态, Some(TokenKind::Colon)) && matches!(前二形态, Some(TokenKind::Colon))
}

/// 判断指定位置的标识符前是否为 `.`（方法调用/字段访问位）
///
/// 宏不能经 `.` 访问，`标识符(...)` 前是 `.` 时必为方法调用：
/// `.格式化(...)` 不应按宏补 `!`（否则生成 `.format!(...)`，方法调用
/// 语法非法）。修复「宏映射词在方法调用位被追补感叹号」的行为。
fn 前接点号(令牌流: &[rustc_lexer::Token], 当前位置: usize) -> bool {
    let mut 内索引 = 当前位置;
    while 内索引 > 0 {
        内索引 -= 1;
        if !是否为空白符(令牌流[内索引].kind) {
            return matches!(令牌流[内索引].kind, TokenKind::Dot);
        }
    }
    false
}

/// 判断指定位置的标识符是否为属性名（紧跟在 `#[` 或 `#![` 之后的标识符）
///
/// 属性名不应触发宏感叹号自动补充：`#[配置(测试)]` 中的 `配置` 是属性
/// （对应 `cfg`），而非宏调用，若误补 `!` 会生成非法的 `#[cfg!(test)]`。
fn 是否为属性名(令牌流: &[rustc_lexer::Token], 当前位置: usize) -> bool {
    // 往前收集最多三个非空白 token（最近的在前）
    let mut 前序: Vec<TokenKind> = Vec::new();
    let mut 内索引 = 当前位置;
    while 内索引 > 0 && 前序.len() < 3 {
        内索引 -= 1;
        if !是否为空白符(令牌流[内索引].kind) {
            前序.push(令牌流[内索引].kind);
        }
    }

    // `#[标识符`：最近的 token 是 `[`，再前一个是 `#`
    // `#![标识符`：最近的 token 是 `[`，再前两个是 `!` 与 `#`
    matches!(
        前序.as_slice(),
        [TokenKind::OpenBracket, TokenKind::Pound, ..]
    ) || matches!(
        前序.as_slice(),
        [TokenKind::OpenBracket, TokenKind::Not, TokenKind::Pound]
    )
}

#[cfg(test)]
mod 单元测试 {
    use super::*;
    use std::collections::HashMap;
    use std::collections::HashSet;

    fn 建测试映射() -> HashMap<String, String> {
        HashMap::from([
            ("函数".to_string(), "fn".to_string()),
            ("让".to_string(), "let".to_string()),
            ("可变".to_string(), "mut".to_string()),
            ("如果".to_string(), "if".to_string()),
            ("否则".to_string(), "else".to_string()),
            ("打印行".to_string(), "println".to_string()),
            ("打印".to_string(), "print".to_string()),
            ("格式化".to_string(), "format".to_string()),
            ("断言".to_string(), "assert".to_string()),
            ("断言相等".to_string(), "assert_eq".to_string()),
            ("向量".to_string(), "vec".to_string()),
        ])
    }

    fn 建宏映射() -> HashMap<String, String> {
        HashMap::from([
            ("打印行".to_string(), "println".to_string()),
            ("打印".to_string(), "print".to_string()),
            ("格式化".to_string(), "format".to_string()),
            ("断言".to_string(), "assert".to_string()),
            ("断言相等".to_string(), "assert_eq".to_string()),
            ("向量".to_string(), "vec".to_string()),
        ])
    }

    #[test]
    fn 测试简单替换() {
        let 映射表 = 建测试映射();
        let 源码 = "让 可变 x = 5;";
        let 期望串 = "let mut x = 5;";
        assert_eq!(词法转译(源码, &映射表), 期望串);
    }

    #[test]
    fn 测试未映射标识符保留() {
        let 映射表 = 建测试映射();
        let 源码 = "让 变量名 = 42;";
        let 期望串 = "let 变量名 = 42;";
        assert_eq!(词法转译(源码, &映射表), 期望串);
    }

    #[test]
    fn 测试注释与字符串保留() {
        let 映射表 = 建测试映射();
        let 源码 = "// 这是注释 函数\n让 s = \"这是字符串 函数\";";
        let 期望串 = "// 这是注释 函数\nlet s = \"这是字符串 函数\";";
        assert_eq!(词法转译(源码, &映射表), 期望串);
    }

    #[test]
    fn 测试原始标识符处理() {
        // 原始标识符用于保留关键字（如 match 是 Rust 保留字）
        let mut 映射表 = 建测试映射();
        映射表.insert("匹配".to_string(), "match".to_string());
        let 源码 = "让 r#匹配 = 1;";
        let 期望串 = "let r#match = 1;";
        assert_eq!(词法转译(源码, &映射表), 期望串);
    }

    #[test]
    fn 测试未映射标识符原样() {
        let 映射表 = 建测试映射();
        let 源码 = "函数 主函数() { }";
        let 期望串 = "fn 主函数() { }";
        assert_eq!(词法转译(源码, &映射表), 期望串);
    }

    // ===== 宏感叹号自动补充测试 =====

    #[test]
    fn 测试宏自动补叹号() {
        let 映射表 = 建测试映射();
        let 宏表 = 建宏映射();
        let 源码 = "打印行(\"你好\")";
        let 期望串 = "println!(\"你好\")";
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &宏表, &HashMap::new()),
            期望串
        );
    }

    #[test]
    fn 测试宏已有叹号不重复() {
        let 映射表 = 建测试映射();
        let 宏表 = 建宏映射();
        let 源码 = "打印行!(\"你好\")";
        let 期望串 = "println!(\"你好\")";
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &宏表, &HashMap::new()),
            期望串
        );
    }

    #[test]
    fn 测试整程序宏自动补叹号() {
        let 映射表 = 建测试映射();
        let 宏表 = 建宏映射();
        let 源码 = "函数 主函数() { 打印行(\"你好\") }";
        // 注意：主函数 不在映射中，所以保持原样
        // 但 函数 → fn，打印行 → println!
        let 实测 = 词法转译并宏映射(源码, &映射表, &宏表, &HashMap::new());
        assert!(实测.contains("fn"));
        assert!(实测.contains("println!(\"你好\")"));
    }

    #[test]
    fn 测试普通函数调用不加叹号() {
        let 映射表 = 建测试映射();
        let 宏表 = 建宏映射();
        // "从" 不在宏集合中，不应被加 !
        let 源码 = "字符串::从(\"x\")";
        let 期望串 = "字符串::从(\"x\")";
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &宏表, &HashMap::new()),
            期望串
        );
    }

    #[test]
    fn 测试属性名不补叹号() {
        let mut 映射表 = 建测试映射();
        let 宏表 = 建宏映射();
        // 属性上下文中的宏名（如 配置→cfg、断言）不应被补 !
        映射表.insert("配置".to_string(), "cfg".to_string());
        let 源码 = "#[配置(测试)]\n#[断言]";
        let 期望串 = "#[cfg(测试)]\n#[assert]";
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &宏表, &HashMap::new()),
            期望串
        );
    }

    #[test]
    fn 测试内部属性名不补叹号() {
        let mut 映射表 = 建测试映射();
        let 宏表 = 建宏映射();
        映射表.insert("配置".to_string(), "cfg".to_string());
        // 内部属性 #![...] 同样不应补 !
        let 源码 = "#![配置(测试)]";
        let 期望串 = "#![cfg(测试)]";
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &宏表, &HashMap::new()),
            期望串
        );
    }

    #[test]
    fn 测试方法位宏词不补叹号() {
        // 方法调用位的宏词不补 `!`：`.格式化(` → `.格式化提示(`（方法调用）
        let 映射表 = 建测试映射();
        let 宏表 = 建宏映射();
        let 源码 = "记录器.格式化(\"{}\", 值);";
        let 期望串 = "记录器.format(\"{}\", 值);";
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &宏表, &HashMap::new()),
            期望串
        );
    }

    #[test]
    fn 测试点号后带叹号宏词保留() {
        // 方法位显式写了 `!`：保持用户书写形式（不按宏补逻辑处理）
        let 映射表 = 建测试映射();
        let 宏表 = 建宏映射();
        let 源码 = "记录器.打印行!(\"你好\");";
        let 期望串 = "记录器.println!(\"你好\");";
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &宏表, &HashMap::new()),
            期望串
        );
    }

    #[test]
    fn 测试普通代码宏仍补叹号() {
        let 映射表 = 建测试映射();
        let 宏表 = 建宏映射();
        // 普通代码中的宏调用不受影响
        let 源码 = "断言(5 > 3)";
        let 期望串 = "assert!(5 > 3)";
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &宏表, &HashMap::new()),
            期望串
        );
    }

    #[test]
    fn 测试双冒号后宏不补叹号() {
        let 映射表 = 建测试映射();
        let 宏表 = 建宏映射();
        // 打印行 在宏集合中，但前面是 ::，不应加 !
        let 源码 = "std::打印行(\"你好\")";
        let 期望串 = "std::println(\"你好\")";
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &宏表, &HashMap::new()),
            期望串
        );
    }

    #[test]
    fn 测试宏后跟方括号() {
        let 映射表 = 建测试映射();
        let 宏表 = 建宏映射();
        let 源码 = "向量![1, 2, 3]";
        let 期望串 = "vec![1, 2, 3]";
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &宏表, &HashMap::new()),
            期望串
        );
    }

    #[test]
    fn 测试宏后跟方括号补叹号() {
        let 映射表 = 建测试映射();
        let 宏表 = 建宏映射();
        let 源码 = "向量[1, 2, 3]";
        let 期望串 = "vec![1, 2, 3]";
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &宏表, &HashMap::new()),
            期望串
        );
    }

    #[test]
    fn 测试宏映射覆盖类型值() {
        // 模拟真实语言包：宏名同时出现在类型节与宏节（如 向量→新建向量 与 向量→向量宏），
        // 关键词映射表 被类型节覆盖为 新建向量，宏映射应保证宏调用输出 向量宏!
        let mut 映射表 = 建测试映射();
        映射表.insert("向量".to_string(), "Vec".to_string());
        let 宏表 = 建宏映射();
        let 源码 = "让 v = 向量![1, 2, 3];";
        let 期望串 = "let v = vec![1, 2, 3];";
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &宏表, &HashMap::new()),
            期望串
        );
    }

    #[test]
    fn 测试派生参数用派生映射() {
        // 派生属性内的特征名走派生特征映射（`克隆` → `Clone` 大写）
        let mut 映射表 = 建测试映射();
        映射表.insert("派生".to_string(), "derive".to_string());
        let mut 派生表 = HashMap::new();
        派生表.insert("克隆".to_string(), "Clone".to_string());
        派生表.insert("调试".to_string(), "Debug".to_string());
        let 空表 = HashMap::new();
        let 源码 = "#[派生(克隆, 调试)]\n结构体 点 {";
        let 期望串 = "#[derive(Clone, Debug)]\n结构体 点 {";
        assert_eq!(词法转译并宏映射(源码, &映射表, &空表, &派生表), 期望串);
        // 方法调用 `值.克隆()` 不受派生映射影响（无上下文匹配，走关键字表）
        let 源码二 = "值.克隆();";
        assert_eq!(
            词法转译并宏映射(源码二, &映射表, &空表, &派生表),
            "值.克隆();"
        );
    }

    #[test]
    fn 测试派生参数连同关键字替换() {
        // 派生属性本身的关键字（派生→derive）与参数（克隆→Clone）同时替换
        let mut 映射表 = 建测试映射();
        映射表.insert("派生".to_string(), "derive".to_string());
        let mut 派生表 = HashMap::new();
        派生表.insert("克隆".to_string(), "Clone".to_string());
        let 空表 = HashMap::new();
        let 源码 = "#[派生(克隆)]";
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &空表, &派生表),
            "#[derive(Clone)]"
        );
    }

    #[test]
    fn 测试空宏集不补叹号() {
        let 映射表 = 建测试映射();
        let 空表 = HashMap::new();
        let 源码 = "打印行(\"你好\")";
        let 期望串 = "println(\"你好\")"; // 不补 !
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &空表, &HashMap::new()),
            期望串
        );
    }

    #[test]
    fn 测试多个宏调用() {
        let 映射表 = 建测试映射();
        let 宏表 = 建宏映射();
        let 源码 = "打印行(\"甲\"); 打印(\"乙\")";
        let 期望串 = "println!(\"甲\"); print!(\"乙\")";
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &宏表, &HashMap::new()),
            期望串
        );
    }

    #[test]
    fn 测试宏规则声明补叹号() {
        // `宏规则` 声明形式：后跟宏名时自动补 `!`（macro_rules! 名称）
        let mut 映射表 = 建测试映射();
        映射表.insert("宏规则".to_string(), "macro_rules".to_string());
        let 空表 = HashMap::new();
        let 源码 = "宏规则 创建向量 { () => { } }";
        let 期望串 = "macro_rules! 创建向量 { () => { } }";
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &空表, &HashMap::new()),
            期望串
        );
    }

    #[test]
    fn 测试宏规则单独不补叹号() {
        // 宏规则后不跟宏名（如注释性用法）时不补 `!`
        let mut 映射表 = 建测试映射();
        映射表.insert("宏规则".to_string(), "macro_rules".to_string());
        let 空表 = HashMap::new();
        let 源码 = "让 x = 宏规则;";
        let 期望串 = "let x = macro_rules;";
        assert_eq!(
            词法转译并宏映射(源码, &映射表, &空表, &HashMap::new()),
            期望串
        );
    }

    // ===== 源映射测试 =====

    #[test]
    fn 测试带映射转译输出一致() {
        let 映射表 = 建测试映射();
        let 宏表 = 建宏映射();
        let 源码 = "函数 主函数() { 让 x = 5; 打印行(\"你好\") }";
        let 转译产出 = 词法转译并映射(
            源码,
            &映射表,
            &宏表,
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashMap::new(),
        );
        assert_eq!(
            转译产出.产出,
            词法转译并宏映射(源码, &映射表, &宏表, &HashMap::new())
        );
    }

    #[test]
    fn 测试带映射转译记录关键字替换() {
        let 映射表 = 建测试映射();
        let 宏表 = 建宏映射();
        let 源码 = "函数 主函数() { 让 x = 5; 打印行(\"你好\") }";
        let 转译产出 = 词法转译并映射(
            源码,
            &映射表,
            &宏表,
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashMap::new(),
        );

        // 函数/让/打印行 被替换，主函数 未命中映射不记录
        let 函数条目 = 转译产出
            .源映射
            .iter()
            .find(|标记项| 标记项.原文 == "函数")
            .expect("应有 函数 映射");
        assert_eq!(函数条目.替换文本, "fn");
        assert_eq!(
            &源码[函数条目.源偏移..函数条目.源偏移 + 函数条目.字节长度],
            "函数"
        );

        let 让条目 = 转译产出
            .源映射
            .iter()
            .find(|标记项| 标记项.原文 == "让")
            .expect("应有 让 映射");
        assert_eq!(让条目.替换文本, "let");
        assert_eq!(&源码[让条目.源偏移..让条目.源偏移 + 让条目.字节长度], "让");

        // 宏名映射记录翻译文本（println），感叹号补充不产生额外条目
        let 打印行条目 = 转译产出
            .源映射
            .iter()
            .find(|标记项| 标记项.原文 == "打印行")
            .expect("应有 打印行 映射");
        assert_eq!(打印行条目.替换文本, "println");

        assert!(!转译产出.源映射.iter().any(|标记项| 标记项.原文 == "主函数"));
    }

    #[test]
    fn 测试带映射转译原始标识符() {
        let mut 映射表 = 建测试映射();
        映射表.insert("匹配".to_string(), "match".to_string());
        let 空表 = HashMap::new();
        let 源码 = "让 r#匹配 = 1;";
        let 转译产出 = 词法转译并映射(
            源码,
            &映射表,
            &空表,
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashMap::new(),
        );
        assert_eq!(转译产出.产出, "let r#match = 1;");
        let 条目项 = 转译产出
            .源映射
            .iter()
            .find(|标记项| 标记项.原文 == "r#匹配")
            .expect("应有原始标识符映射");
        assert_eq!(条目项.替换文本, "r#match");
    }

    // ===== 反向转译测试 =====

    fn 建反向映射(正向表: &HashMap<String, String>) -> HashMap<String, String> {
        正向表
            .iter()
            .map(|(次索引, 值项)| (值项.clone(), 次索引.clone()))
            .collect()
    }

    #[test]
    fn 测试反向转译基础() {
        // 关键字、类型名、宏名均被还原为母语，宏感叹号保留
        let 正向表 = HashMap::from([
            ("函数".to_string(), "fn".to_string()),
            ("主函数".to_string(), "main".to_string()),
            ("让".to_string(), "let".to_string()),
            ("可变".to_string(), "mut".to_string()),
            ("整数".to_string(), "i32".to_string()),
            ("打印行".to_string(), "println".to_string()),
        ]);
        let 反向表 = 建反向映射(&正向表);
        let 空集 = HashSet::new();
        let 源码 = "fn main() { let mut x: i32 = 5; println!(\"你好\"); }";
        let 期望串 = "函数 主函数() { 让 可变 x: 整数 = 5; 打印行!(\"你好\"); }";
        assert_eq!(逆向转译(源码, &反向表, &空集, &HashSet::new()), 期望串);
    }

    #[test]
    fn 测试反向转译保留注释字符串自定义标识符() {
        // 注释、字符串字面量、中文/英文自定义标识符均保持原样
        let 反向表 = HashMap::from([
            ("fn".to_string(), "函数".to_string()),
            ("let".to_string(), "让".to_string()),
        ]);
        let 空集 = HashSet::new();
        let 源码 = "// fn 是关键字\nlet s = \"let\";\nlet 计数 = fn_value;";
        let 期望串 = "// fn 是关键字\n让 s = \"let\";\n让 计数 = fn_value;";
        assert_eq!(逆向转译(源码, &反向表, &空集, &HashSet::new()), 期望串);
    }

    #[test]
    fn 测试反向转译长词优先() {
        // token 级匹配：i32 与 i3x 是完整 token，互不干扰（无子串误替换）
        let 反向表 = HashMap::from([
            ("i32".to_string(), "整数".to_string()),
            ("i3".to_string(), "三".to_string()),
        ]);
        let 空集 = HashSet::new();
        let 源码 = "let a: i32 = 1; let b: i3x = 2; let c = i3;";
        // let 不在反向表中保持原样；i32→整数、i3→三，i3x 是完整 token 不受影响
        let 期望串 = "let a: 整数 = 1; let b: i3x = 2; let c = 三;";
        assert_eq!(逆向转译(源码, &反向表, &空集, &HashSet::new()), 期望串);
    }

    #[test]
    fn 测试反向转译宏规则声明() {
        // `macro_rules! 名称` 反向转译为 `宏规则 名称`（感叹号省略）
        let 正向表 = HashMap::from([
            ("宏规则".to_string(), "macro_rules".to_string()),
            ("函数".to_string(), "fn".to_string()),
        ]);
        let 反向表 = 建反向映射(&正向表);
        let 空集 = HashSet::new();
        let 源码 = "macro_rules! 创建向量 { () => { } }";
        let 期望串 = "宏规则 创建向量 { () => { } }";
        assert_eq!(逆向转译(源码, &反向表, &空集, &HashSet::new()), 期望串);
    }

    #[test]
    fn 测试反向转译宏规则带叹号调用() {
        // 宏调用 `名称!(...)` 的感叹号属于调用方，反向转译时保留
        let 正向表 = HashMap::from([
            ("打印行".to_string(), "println".to_string()),
            ("宏规则".to_string(), "macro_rules".to_string()),
            ("函数".to_string(), "fn".to_string()),
        ]);
        let 反向表 = 建反向映射(&正向表);
        let 空集 = HashSet::new();
        let 源码 = "macro_rules! 创建向量 { () => { } }\nfn main() { 创建向量!() }";
        let 期望串 = "宏规则 创建向量 { () => { } }\n函数 main() { 创建向量!() }";
        assert_eq!(逆向转译(源码, &反向表, &空集, &HashSet::new()), 期望串);
    }

    #[test]
    fn 测试反向转译模块前缀() {
        let 正向表 = HashMap::from([
            ("函数".to_string(), "fn".to_string()),
            ("包".to_string(), "crate".to_string()),
        ]);
        let 反向表 = 建反向映射(&正向表);
        let 模块集 = HashSet::from(["辅助".to_string()]);
        // 序号命中正向记录（代理添加的 包::）：整体删除，还原为裸路径
        let 已加集 = HashSet::from([5usize]);
        assert_eq!(
            逆向转译(
                "fn main() { crate::辅助::辅助函数(); }",
                &反向表,
                &模块集,
                &已加集
            ),
            "函数 main() { 辅助::辅助函数(); }"
        );
        // 序号未命中（用户手写的 包::，非代理添加）：前缀保留不误删
        assert_eq!(
            逆向转译(
                "fn main() { crate::辅助::辅助函数(); }",
                &反向表,
                &模块集,
                &HashSet::new()
            ),
            "函数 main() { 包::辅助::辅助函数(); }"
        );
        // 后跟非模块名时还原为 包::（不受序号影响）
        assert_eq!(
            逆向转译(
                "fn main() { crate::外部函数(); }",
                &反向表,
                &模块集,
                &已加集
            ),
            "函数 main() { 包::外部函数(); }"
        );
    }

    #[test]
    fn 测试反向转译原始标识符() {
        let 反向表 = HashMap::from([
            ("let".to_string(), "让".to_string()),
            ("match".to_string(), "匹配".to_string()),
        ]);
        let 空集 = HashSet::new();
        assert_eq!(
            逆向转译("let r#match = 1;", &反向表, &空集, &HashSet::new()),
            "让 r#匹配 = 1;"
        );
    }

    #[test]
    fn 测试反向转译包前缀带空白() {
        // 包 与 :: 之间存在空白时前缀仍应被完整删除（不能残留孤立冒号）
        let 反向表 = HashMap::from([("fn".to_string(), "函数".to_string())]);
        let 模块集 = HashSet::from(["辅助".to_string()]);
        let 已加集 = HashSet::from([5usize]);
        assert_eq!(
            逆向转译(
                "fn f() { crate :: 辅助::辅助函数(); }",
                &反向表,
                &模块集,
                &已加集
            ),
            "函数 f() { 辅助::辅助函数(); }"
        );
    }

    #[test]
    fn 测试反向转译宏规则带空白叹号() {
        // macro_rules 与 ! 之间存在空白时感叹号仍应被省略
        let 反向表 = HashMap::from([("macro_rules".to_string(), "宏规则".to_string())]);
        let 空集 = HashSet::new();
        assert_eq!(
            逆向转译(
                "macro_rules ! 创建向量 { () => { } }",
                &反向表,
                &空集,
                &HashSet::new()
            ),
            "宏规则 创建向量 { () => { } }"
        );
    }

    /// use 段让位：`文件` 同时在宏表（file）与让位集合（标准库 File）中时，
    /// use 段内跳过词法替换（交后续模块路径/别名阶段处理）；
    /// 普通代码位置的 `文件!` 宏调用照常替换
    #[test]
    fn 测试使用段让位冲突词() {
        let 映射表 = HashMap::from([
            ("使用".to_string(), "use".to_string()),
            ("文件".to_string(), "file".to_string()),
        ]);
        let 让位集 = HashSet::from(["文件".to_string()]);
        let 输出串 = 词法转译并映射(
            "使用 标准库::文件系统::文件 as 库文件;\n文件!()",
            &映射表,
            &HashMap::new(),
            &HashMap::new(),
            &让位集,
            &HashSet::new(),
            &HashMap::new(),
        )
        .产出;
        assert_eq!(输出串, "use 标准库::文件系统::文件 as 库文件;\nfile!()");
        // 对照组：无让位集合时 use 段内照旧替换（旧行为，验证让位集合确为开关）
        let 无让位输出 = 词法转译并映射(
            "使用 标准库::文件系统::文件 as 库文件;",
            &映射表,
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashMap::new(),
        )
        .产出;
        assert_eq!(无让位输出, "use 标准库::文件系统::file as 库文件;");
    }

    /// use 段让位（本地化 use 词形）：非 `use`/`使用` 词形的语言（如 es `usar`）
    /// 也需开启让位态——词形经关键字映射反查（历史硬编码只识别 `use`/`使用`，
    /// es 等语言 use 段的冲突词被词法替换后残留 `format::`，模块路径阶段无法修复）
    #[test]
    fn 测试使用段让位本地化使用词形() {
        let 映射表 = HashMap::from([
            ("usar".to_string(), "use".to_string()),
            ("formato".to_string(), "format".to_string()),
        ]);
        let 让位集 = HashSet::from(["formato".to_string()]);
        let 输出串 = 词法转译并映射(
            "usar estandar::formato::Mostrar;",
            &映射表,
            &HashMap::new(),
            &HashMap::new(),
            &让位集,
            &HashSet::new(),
            &HashMap::new(),
        )
        .产出;
        // `usar` 照常译为 use；`formato` 让位保留（交模块路径阶段替换为 fmt）
        assert_eq!(输出串, "use estandar::formato::Mostrar;");
    }

    /// 方法位让位：词法替换值是保留关键字的词（`枚举`→enum）在 `.` 后
    /// 原样保留（交别名阶段替成 枚举）；未命中让位集合时照旧替换
    #[test]
    fn 测试方法位让位冲突词() {
        let 映射表 = HashMap::from([
            ("让".to_string(), "let".to_string()),
            ("迭代".to_string(), "iter".to_string()),
            ("枚举".to_string(), "enum".to_string()),
        ]);
        let 方法让位集 = HashSet::from(["枚举".to_string()]);
        let 输出串 = 词法转译并映射(
            "让 n = 列.迭代().枚举();",
            &映射表,
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &方法让位集,
            &HashMap::new(),
        )
        .产出;
        assert_eq!(输出串, "let n = 列.iter().枚举();");
        // 对照组：无让位集合时方法位照旧替换（旧行为，验证让位集合确为开关）
        let 无让位输出 = 词法转译并映射(
            "让 n = 列.迭代().枚举();",
            &映射表,
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashMap::new(),
        )
        .产出;
        assert_eq!(无让位输出, "let n = 列.iter().enum();");
    }

    /// 数字与中文类型黏连（`0无符号微整数`）：词法解析器 视为单个
    /// 字面量记号 token，后缀命中映射表时替换（→ `0u8`）；未命中/ASCII 后缀原样
    #[test]
    fn 测试数字字面量中文后缀() {
        let 映射表 = HashMap::from([("无符号微整数".to_string(), "u8".to_string())]);
        let 空表 = HashMap::new();
        // 后缀命中：替换为英文数值类型，并记录编辑条目（仅后缀部分）
        let 转译产出 = 词法转译并映射(
            "vec![0无符号微整数; 16]",
            &映射表,
            &空表,
            &空表,
            &HashSet::new(),
            &HashSet::new(),
            &空表,
        );
        assert_eq!(转译产出.产出, "vec![0u8; 16]");
        assert_eq!(转译产出.源映射.len(), 1);
        assert_eq!(转译产出.源映射[0].原文, "无符号微整数");
        assert_eq!(转译产出.源映射[0].替换文本, "u8");
        // 后缀未命中：原样保留（编译错误交给编译器）
        let 输出串 = 词法转译并映射(
            "let x = 0未知词;",
            &映射表,
            &空表,
            &空表,
            &HashSet::new(),
            &HashSet::new(),
            &空表,
        )
        .产出;
        assert_eq!(输出串, "let x = 0未知词;");
        // ASCII 后缀不受影响
        let 输出串 = 词法转译并映射(
            "let x = 0u8;",
            &映射表,
            &空表,
            &空表,
            &HashSet::new(),
            &HashSet::new(),
            &空表,
        )
        .产出;
        assert_eq!(输出串, "let x = 0u8;");
    }

    /// 标准库层数值类型词（`无符号机器整数` = 无符号机器整数，不在 关键词映射表）
    /// 黏连数字后由兜底表拆分：`0无符号机器整数` → `0usize`；
    /// 两表同词冲突时 关键词映射表 优先
    #[test]
    fn 测试数字字面量后缀兜底标准库表() {
        let 空表 = HashMap::new();
        let 标准库表 = HashMap::from([("无符号机器整数".to_string(), "usize".to_string())]);
        let 转译产出 = 词法转译并映射(
            "let x = 0无符号机器整数;",
            &空表,
            &空表,
            &空表,
            &HashSet::new(),
            &HashSet::new(),
            &标准库表,
        );
        assert_eq!(转译产出.产出, "let x = 0usize;");
        assert_eq!(转译产出.源映射.len(), 1);
        assert_eq!(转译产出.源映射[0].原文, "无符号机器整数");
        assert_eq!(转译产出.源映射[0].替换文本, "usize");
        // 两表同词冲突：关键词映射表 优先（匹配其它阶段的查找次序）
        let 映射表 = HashMap::from([("微整数".to_string(), "u8".to_string())]);
        let 兜底表 = HashMap::from([("微整数".to_string(), "usize".to_string())]);
        let 输出串 = 词法转译并映射(
            "0微整数",
            &映射表,
            &空表,
            &空表,
            &HashSet::new(),
            &HashSet::new(),
            &兜底表,
        )
        .产出;
        assert_eq!(输出串, "0u8");
    }
}
