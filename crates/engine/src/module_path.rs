//! 模块路径替换模块
//! 1. 将源代码中 `use` 语句的中文模块路径段替换为英文路径段
//!    （如 `使用 标准集合::哈希映射` → `使用 std::collections::HashMap`）；
//! 2. 为已知模块路径段添加 `crate::` 前缀（LSP 虚拟项目跨文件引用专用，
//!    原实现在 lsp crate，迁入引擎统一维护转译规则）；
//! 3. 文件式 `mod 名字;` 声明的净化（虚拟项目聚合用）与非 ASCII 模块名的
//!    `#[path]` 注解（真实项目产物用，CLI 与 LSP 镜像共享同一实现）。
//!
//! 各阶段均以 token 级替换并产出编辑表，供全管线编辑地图组合。
//!
//! crate 名规范化：Cargo 包名允许连字符（如 `tracing-subscriber`），但 Rust
//! 代码中引用 crate 必须写 `_` 形式（`tracing_subscriber`）；use 路径中
//! 模块/类型/函数名不含连字符，`-` 只会出现在 crate 名段，故替换时对
//! 映射值做 `-` → `_` 规范化安全。

use crate::缓存::源码映射条目;
use rustc_lexer::{TokenKind, tokenize};
use std::collections::{HashMap, HashSet};

/// 单阶段替换结果：输出与编辑表
///
/// 编辑表以**本阶段输入文本**的字节偏移记录（input_offset 升序、互不重叠），
/// replacement 为本阶段输出文本。
#[derive(Debug, Clone)]
pub struct ReplaceResult {
    /// 替换后的文本
    pub output: String,
    /// 编辑表
    pub edits: Vec<源码映射条目>,
}

/// 将源代码中的中文模块路径段替换为英文
///
/// # 参数
/// - `source`: 待替换的源代码字符串
/// - `path_map`: 中文路径段 → 英文路径段 的映射表
///
/// # 注意
/// 采用 token 级替换（与 `replace_aliases` 一致）：
/// - 仅替换**完整标识符 token**，避免破坏组合词
///   （如 `读取全部字符串` 中的 `字符串` 不会被误替换）；
/// - 仅替换 **use 语句内**的路径段（含末段与单段路径，如 `使用 文件系统;`），
///   use 内不可能出现用户变量，全量替换安全；
/// - 表达式中的 `类型::关联函数`（如 `引用计数::新建`）不替换，
///   留给别名映射处理成 `Rc::new`。
pub fn replace_module_paths(source: &str, path_map: &HashMap<String, String>) -> String {
    replace_module_paths_with_map(source, path_map).output
}

/// 同 [`replace_module_paths`]，同时产出编辑表（本阶段输入坐标）
pub fn replace_module_paths_with_map(
    source: &str,
    path_map: &HashMap<String, String>,
) -> ReplaceResult {
    if path_map.is_empty() {
        return ReplaceResult {
            output: source.to_string(),
            edits: Vec::new(),
        };
    }
    // use 语句是唯一的替换场合：整份源码里两种词形都不出现时无需分词
    // （直接调用本函数时母语与已转译的 `use` 都可能出现）
    if !source.contains("use") && !source.contains("使用") {
        return ReplaceResult {
            output: source.to_string(),
            edits: Vec::new(),
        };
    }
    // 不收集 token 向量：主循环只顺序消费一次（每个 use 行只命中少数段），
    // 收集一份全文件 token 表是纯分配开销
    replace_module_paths_iter(source, path_map, tokenize(source).map(|t| (t.kind, t.len)))
}

/// 同 [`replace_module_paths_with_map`]，但复用调用方已分好的 token 流
///
/// 引擎的延迟项目上下文入口在 `lex.output` 上已经分词一次（为了收集声明名
/// 建项目上下文），而本阶段恰好也是在这份文本上跑，可直接借用那份 token 流，
/// 省掉一次全文件分词。两条入口的输出与编辑表逐字相同（同一序列）。
pub(crate) fn replace_module_paths_in_tokens(
    source: &str,
    path_map: &HashMap<String, String>,
    tokens: &[rustc_lexer::Token],
) -> ReplaceResult {
    if path_map.is_empty() {
        return ReplaceResult {
            output: source.to_string(),
            edits: Vec::new(),
        };
    }
    // 与现分词入口同一道早退：本阶段与它逐字等价，只是 token 来源不同
    if !source.contains("use") && !source.contains("使用") {
        return ReplaceResult {
            output: source.to_string(),
            edits: Vec::new(),
        };
    }
    replace_module_paths_iter(source, path_map, tokens.iter().map(|t| (t.kind, t.len)))
}

/// [`replace_module_paths_with_map`] 的实现：token 流由调用方决定是现分词
/// （流式）还是复用已有向量
fn replace_module_paths_iter<I>(
    source: &str,
    path_map: &HashMap<String, String>,
    tokens: I,
) -> ReplaceResult
where
    I: Iterator<Item = (TokenKind, usize)>,
{
    let mut output = String::with_capacity(source.len());
    let mut edits = Vec::new();
    let mut current_offset = 0;
    // 已刷出的源偏移：未命中的 token 不逐个 push_str，命中时才把「上次刷出
    // 位置 → 本 token 起点」的原始片段整段拷过去。大文件里 token 数万而命中
    // 只在 use 行，逐 token 复制是纯开销（2000 行样例本阶段 515.7 → 473.9 µs）
    let mut copied = 0usize;
    // use 语句内的路径段才启用模块路径映射；
    // 表达式中的 `类型::关联函数`（如 `引用计数::新建`）应走别名映射，
    // 不能被模块路径映射误替换成 `rc::新建`
    let mut in_use_stmt = false;

    for (kind, len) in tokens {
        let text = &source[current_offset..current_offset + len];
        match kind {
            TokenKind::Ident => {
                // use 语句内所有路径段（含末段/单段）都查映射：
                // use 中不可能出现用户变量，替换安全；
                // 末段若不在路径映射（如 `服务器` 属于标识符别名），
                // 保持原样交给别名替换处理
                if in_use_stmt && let Some(english) = path_map.get(text) {
                    // crate 名规范化：`日志订阅` → `tracing_subscriber`
                    //（映射值 `tracing-subscriber` 中的连字符非法）
                    let normalized = normalize_crate_hyphen(english);
                    let replacement = normalized.as_deref().unwrap_or(english);
                    edits.push(源码映射条目::new(
                        current_offset,
                        len,
                        text,
                        replacement,
                    ));
                    output.push_str(&source[copied..current_offset]);
                    output.push_str(replacement);
                    copied = current_offset + len;
                }
                // 检测 use 语句起点（管线中 `使用` 已被词法转译为 `use`；
                // 直接调用本函数时两种写法均支持）
                if text == "use" || text == "使用" {
                    in_use_stmt = true;
                }
            }
            TokenKind::Semi => {
                // use 语句以分号结束
                in_use_stmt = false;
            }
            _ => {}
        }
        current_offset += len;
    }
    output.push_str(&source[copied..]);
    ReplaceResult { output, edits }
}

/// 将 use 路径段中的连字符规范化为下划线（crate 名规则）
///
/// Cargo 包名允许连字符（如 `tracing-subscriber`、`tokio-util`），但 Rust
/// 路径中 crate 段必须写 `_` 形式（`tracing_subscriber`）。use 路径中
/// 模块、类型、函数名均不含连字符，`-` 只会出现在 crate 名段，故直接
/// 替换安全；无需替换时返回 None（避免无谓分配）。
/// 别名阶段（alias.rs）对 use 之外的模块词回退替换时复用本函数。
pub(crate) fn normalize_crate_hyphen(segment: &str) -> Option<String> {
    segment.contains('-').then(|| segment.replace('-', "_"))
}

/// 可能被加 `crate::` 前缀的路径段：模块名之后（跳过空白）紧跟 `::`
///
/// 供 [`qualify_module_paths_with_map`] 的快速通道使用：这是「该名字会被加
/// 前缀」的**必要条件**（[`is_path_separator_after`] 允许名与 `::` 之间
/// 夹空白/注释）。注释以 `/` 开头，遇到 `/` 时同样视为可能（否则
/// `甲 /*x*/ :: 乙` 会被误判为不可能）；其余后续字节（`(`、`;`、`_` 等）
/// 都足以否定该处出现。
fn could_be_path_segment(content: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    // 名字可能自重叠（`aa` 在 `aaa::` 中），每次只前进一个字符
    let step = name.chars().next().map_or(1, char::len_utf8);
    let mut from = 0usize;
    while let Some(pos) = content[from..].find(name) {
        let after_name = from + pos + name.len();
        let rest = content[after_name..].trim_start_matches([' ', '\t', '\r', '\n']);
        match rest.as_bytes().first() {
            Some(b':') | Some(b'/') => return true,
            _ => {}
        }
        from += pos + step;
    }
    false
}

/// 为已知模块路径段添加 `crate::` 前缀
///
/// Rust 2018+ 中，子模块内的裸路径 `模块::项` 无法解析到 crate 根的模块，
/// 必须写成 `crate::模块::项` 或先 use。LSP 虚拟项目把每个方言文件聚合为
/// 同一 crate 的兄弟模块，因此为引用其他模块的路径段自动补全前缀，
/// 使 rust-analyzer 能够解析跨文件引用（references/rename）。
///
/// 已带前缀的路径（`crate::辅助`、`其他::辅助`）不会被重复处理。
///
/// 遮蔽豁免：模块名与文件内名字（类型声明名、use 导入绑定名）重名时，
/// 裸路径首段按 Rust 作用域解析优先命中后者（如 `结构体 项目配置` 与
/// `模块 项目配置` 重名时，`项目配置::缺省()` 指向结构体关联函数），
/// 此时不得加前缀——前缀会把路径钉死为模块路径（E0425）。文件含 glob
/// 导入（`use ...::*;`）时，项目项名集合 `item_names`（跨文件汇总）中的
/// 名字视为可能被 glob 引入作用域，一并豁免；`item_names` 传空集合时
/// 仅按文件内遮蔽判定（无项目上下文的调用方行为不变）。
pub fn qualify_module_paths_with_map(
    content: &str,
    module_names: &HashSet<String>,
    item_names: &HashSet<String>,
) -> ReplaceResult {
    if module_names.is_empty() || content.is_empty() {
        return ReplaceResult {
            output: content.to_string(),
            edits: Vec::new(),
        };
    }
    // 快速通道：本条只给「与模块名逐字节相同的标识符、且后面紧跟 `::`」
    // 加前缀，所以只要每个模块名的每处出现后面都不可能跟 `::`，就一定没有
    // 编辑（无编辑时逐 token 重建的输出恒等于原文）。比「全文包含模块名」更准：
    // 单文件项目的模块名就是文件词干 `main`，而 `主函数` 会被词法阶段译成
    // `main`（`fn main()`）——只看子串会永远命中，等于没有早退。
    if !module_names
        .iter()
        .any(|name| could_be_path_segment(content, name))
    {
        return ReplaceResult {
            output: content.to_string(),
            edits: Vec::new(),
        };
    }
    // token 序列与字节区间：预扫描（遮蔽名收集）与主循环共用
    let mut spans: Vec<(TokenKind, usize, usize)> = Vec::new();
    let mut offset = 0usize;
    for token in tokenize(content) {
        let start = offset;
        offset += token.len;
        spans.push((token.kind, start, offset));
    }
    let (shadow_names, has_glob) = collect_shadow_names(content, &spans);

    let mut output = String::with_capacity(content.len() + module_names.len() * 8);
    let mut edits = Vec::new();

    for (i, &(kind, start, end)) in spans.iter().enumerate() {
        let text = &content[start..end];

        if is_ws(kind) {
            output.push_str(text);
            continue;
        }

        // 模块路径段：标识符属于已知模块名、后跟 `::`、且不在既有路径段之后
        // （`crate::辅助`、`a::辅助` 中的 `辅助` 已处于路径内，跳过）；
        // 文件内遮蔽名与 glob 下命中的项目项名不加前缀（保持本地语义）
        let needs_prefix = is_ident(kind) && {
            let raw_name = text.strip_prefix("r#").unwrap_or(text);
            module_names.contains(raw_name)
                && is_path_separator_after(&spans, i)
                && !is_path_separator_before(&spans, i)
                && !shadow_names.contains(raw_name)
                && !(has_glob && item_names.contains(raw_name))
        };
        if needs_prefix {
            let prefixed = format!("crate::{}", text);
            edits.push(源码映射条目::new(start, text.len(), text, &prefixed));
            output.push_str(&prefixed);
        } else {
            output.push_str(text);
        }
    }
    ReplaceResult { output, edits }
}

/// 收集文件内遮蔽名与 glob 导入标记（`crate::` 前缀豁免判定用）
///
/// 遮蔽名 = 可能让裸路径首段优先解析为本地项的名字：
/// - 类型声明名：`struct`/`enum`/`trait`/`type`/`union` 关键字后的名字
///   （类型命名空间，路径首段解析的优先候选）；
/// - use 导入绑定名：use 语句中每条路径链的末段，`as` 别名取其别名
///   （容器段与中间段不引入作用域，不收集）。
///
/// 不收集 `mod` 声明名：LSP 虚拟项目会以 1:1 空格替换抹除文件式 `mod`
/// 声明（[`strip_file_module_decls`]），声明消失后裸模块名仍须依赖
/// `crate::` 前缀解析，收集会令前缀缺失（回归 E0433 误报）。
///
/// 返回值第二项标记文件是否存在 glob 导入（`use ...::*;`）：glob 引入
/// 的名字不可枚举，调用方回退查项目级项名集合做豁免。
fn collect_shadow_names<'a>(
    content: &'a str,
    spans: &[(TokenKind, usize, usize)],
) -> (HashSet<&'a str>, bool) {
    let mut names: HashSet<&'a str> = HashSet::new();
    let mut has_glob = false;
    for (i, &(kind, start, end)) in spans.iter().enumerate() {
        if !is_ident(kind) {
            continue;
        }
        let text = &content[start..end];
        if matches!(text, "struct" | "enum" | "trait" | "type" | "union") {
            // 声明名：关键字后第一个非空白 token 为标识符时收集
            let mut j = i + 1;
            while j < spans.len() && is_ws(spans[j].0) {
                j += 1;
            }
            if j < spans.len() && is_ident(spans[j].0) {
                let (_, s, e) = spans[j];
                names.insert(&content[s..e]);
            }
        } else if text == "use" {
            collect_use_bindings(content, spans, i, &mut names, &mut has_glob);
        }
    }
    (names, has_glob)
}

/// 扫描一条 use 语句：收集绑定名（路径链末段/`as` 别名），标记 glob
///
/// `use` 后的 token 流以 `;` 结束（use 内不出现分号）；花括号容器段
/// （`path::{...}` 的 `path` 末段）不引入作用域，遇 `{` 不提交末段；
/// glob 来源段（`路径::*` 的路径末段）同样不是绑定名。
fn collect_use_bindings<'a>(
    content: &'a str,
    spans: &[(TokenKind, usize, usize)],
    use_idx: usize,
    names: &mut HashSet<&'a str>,
    has_glob: &mut bool,
) {
    let mut last_seg: Option<&'a str> = None;
    let mut expect_alias = false;
    for &(kind, start, end) in &spans[(use_idx + 1)..] {
        if kind == TokenKind::Semi {
            break;
        }
        let text = &content[start..end];
        match kind {
            TokenKind::Star => {
                *has_glob = true;
                last_seg = None;
            }
            TokenKind::Ident | TokenKind::RawIdent => {
                let name = text.strip_prefix("r#").unwrap_or(text);
                if name == "as" {
                    expect_alias = true;
                } else if !matches!(name, "use" | "pub" | "crate" | "self" | "super") {
                    if expect_alias {
                        names.insert(name);
                        expect_alias = false;
                        last_seg = None;
                    } else {
                        last_seg = Some(name);
                    }
                }
            }
            // 链结束（`a::b,` / `a::b}`）：末段为导入绑定名
            TokenKind::Comma | TokenKind::CloseBrace => {
                if let Some(seg) = last_seg.take() {
                    names.insert(seg);
                }
            }
            // 进入成员列表（`path::{...}`）：容器段不是绑定名
            TokenKind::OpenBrace => last_seg = None,
            _ => {}
        }
    }
    if let Some(seg) = last_seg {
        names.insert(seg);
    }
}

/// 抹除文件式模块声明（`mod 名字;`），供 LSP 虚拟项目内容净化使用
///
/// LSP 虚拟项目将每个方言文件以哈希名托管，并在聚合 `main.rs` 中通过
/// `#[path]` 声明为兄弟模块；用户原文中的文件式 `模块 名字;`（转译后为
/// `mod 名字;`）指向的模块文件在虚拟项目中不存在，会被 cargo check 报
/// E0583（找不到模块文件）与 E0754（非 ASCII 标识符名）。因此发送给
/// rust-analyzer 与写入磁盘前须抹除这类声明。
///
/// 抹除范围连带声明紧邻的修饰：属性（`#[cfg(...)]`、`#[path = ...]`）、
/// 文档注释与可见性（`pub`、`pub(crate)`）——修饰必须附着于某个项，
/// 声明被抹除后会变成悬空修饰触发新错误；修饰与声明之间允许空白与注释。
///
/// 抹除采用 1:1 字符替换（换行符保留、其余字符替换为等数 UTF-16 单位的
/// 空格），行号与列号同原文严格一致，列映射无需调整；内联模块
/// （`mod 名字 { ... }`）照常参与编译，不被处理。
pub fn strip_file_module_decls(content: &str) -> String {
    // 快速通道：判定一个 token 是否文件式声明，要求其文本恰为 `mod`，
    // 因此全文连子串 `mod` 都不含时不可能有声明。LSP 每次按键都会调用
    // 本函数，省掉整趟词法与 token 区间表的分配（绝大多数文件没有 `mod`）。
    if !content.contains("mod") {
        return content.to_string();
    }

    // 收集 token 的字节区间：rustc_lexer 的 token 流连续覆盖整个输入
    let mut spans: Vec<(TokenKind, usize, usize)> = Vec::new();
    let mut offset = 0usize;
    for token in tokenize(content) {
        let start = offset;
        offset += token.len;
        spans.push((token.kind, start, offset));
    }

    // 定位所有文件式模块声明：`mod` + 名字（隔空白/注释）+ `;`
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    while i < spans.len() {
        let (kind, start, end) = spans[i];
        if is_ident(kind)
            && &content[start..end] == "mod"
            && let Some(semi_idx) = file_module_semi(&spans, i)
        {
            // 回溯扩展起点：连带声明前的属性/文档注释/可见性修饰
            let begin = file_module_decl_start(&spans, content, i);
            ranges.push((spans[begin].1, spans[semi_idx].2));
            i = semi_idx + 1;
            continue;
        }
        i += 1;
    }
    if ranges.is_empty() {
        return content.to_string();
    }

    // 1:1 字符替换：换行保留（行号不变），其余字符按等数 UTF-16 单位
    // 替换为空格（列号不变）
    let mut output = String::with_capacity(content.len());
    let mut copied = 0usize;
    for (start, end) in ranges {
        output.push_str(&content[copied..start]);
        for c in content[start..end].chars() {
            if c == '\n' || c == '\r' {
                output.push(c);
            } else {
                for _ in 0..c.len_utf16() {
                    output.push(' ');
                }
            }
        }
        copied = end;
    }
    output.push_str(&content[copied..]);
    output
}

/// 检查 `mod` token 是否为文件式声明，是则返回其结束分号的 token 索引
///
/// 文件式：`mod` 后（允许空白/注释）为单个标识符，再后（允许空白/注释）
/// 为 `;`；内联模块（`mod X { ... }`）返回 None。
fn file_module_semi(spans: &[(TokenKind, usize, usize)], mod_idx: usize) -> Option<usize> {
    let mut name_seen = false;
    for (idx, &(kind, _, _)) in spans.iter().enumerate().skip(mod_idx + 1) {
        if is_ws(kind) {
            continue;
        }
        if !name_seen {
            // `mod` 后必须紧跟名字（含原始标识符 r#名字）
            if !is_ident(kind) {
                return None;
            }
            name_seen = true;
            continue;
        }
        return matches!(kind, TokenKind::Semi).then_some(idx);
    }
    None
}

/// 回溯文件式模块声明的起点：连带声明紧邻的注释、属性与可见性修饰
///
/// 从 `mod` token 向前回溯：跳过空白（注释与声明之间的空行允许跨越）；
/// 遇到紧邻的注释（如文档注释 `///`）直接纳入——悬空文档注释会触发
/// E0585（缺少文档注释目标）；遇到属性结尾（`]`）时纳入配对的 `#[`；
/// 遇到可见性括号（`)`）时纳入配对的 `(` 并确认前置 `pub`；遇到裸
/// `pub` 直接纳入。任何其他 token（上一项的结尾等）终止回溯。
/// 返回最前的修饰 token 索引；无修饰时返回 `mod` 自身索引。
fn file_module_decl_start(
    spans: &[(TokenKind, usize, usize)],
    content: &str,
    mod_idx: usize,
) -> usize {
    let mut earliest = mod_idx;
    let mut cursor = mod_idx; // 回溯游标：当前已纳入块的第一个 token 索引
    loop {
        // 向前跳过空白（仅空白；注释本身会被纳入或终止回溯）
        let mut prev = cursor;
        while prev > 0 && spans[prev - 1].0 == TokenKind::Whitespace {
            prev -= 1;
        }
        if prev == 0 {
            return earliest;
        }
        let (kind, start, end) = spans[prev - 1];
        match kind {
            // 紧邻注释（含文档注释）随声明一并抹除
            TokenKind::LineComment | TokenKind::BlockComment { .. } => {
                earliest = prev - 1;
                cursor = prev - 1;
            }
            // 裸可见性 `pub`
            TokenKind::Ident if &content[start..end] == "pub" => {
                earliest = prev - 1;
                cursor = prev - 1;
            }
            // 可见性括号 `pub(crate)` 的 `)`
            TokenKind::CloseParen => {
                let Some(open) =
                    matching_open(spans, prev - 1, TokenKind::OpenParen, TokenKind::CloseParen)
                else {
                    return earliest;
                };
                // `(` 前须为非空白 `pub`
                let mut j = open;
                while j > 0 && is_ws(spans[j - 1].0) {
                    j -= 1;
                }
                if j > 0
                    && is_ident(spans[j - 1].0)
                    && &content[spans[j - 1].1..spans[j - 1].2] == "pub"
                {
                    earliest = j - 1;
                    cursor = j - 1;
                } else {
                    return earliest;
                }
            }
            // 属性结尾 `]`
            TokenKind::CloseBracket => {
                let Some(open) = matching_open(
                    spans,
                    prev - 1,
                    TokenKind::OpenBracket,
                    TokenKind::CloseBracket,
                ) else {
                    return earliest;
                };
                // `[` 前须为非空白 `#`
                let mut j = open;
                while j > 0 && is_ws(spans[j - 1].0) {
                    j -= 1;
                }
                if j > 0 && spans[j - 1].0 == TokenKind::Pound {
                    earliest = j - 1;
                    cursor = j - 1;
                } else {
                    return earliest;
                }
            }
            _ => return earliest,
        }
    }
}

/// 向前查找与 `close_idx` 处闭合 token 配对的开启 token 索引
fn matching_open(
    spans: &[(TokenKind, usize, usize)],
    close_idx: usize,
    open_kind: TokenKind,
    close_kind: TokenKind,
) -> Option<usize> {
    let mut depth = 0usize;
    for idx in (0..=close_idx).rev() {
        let kind = spans[idx].0;
        if kind == close_kind {
            depth += 1;
        } else if kind == open_kind {
            depth -= 1;
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

/// 是否为空白/注释 token
fn is_ws(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
    )
}

/// 是否为标识符 token（含原始标识符）
fn is_ident(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::Ident | TokenKind::RawIdent)
}

/// 检查指定 token 之后两个连续的非空白 token 是否为 `::`
///
/// rustc_lexer 将 `::` 拆分为两个 `Colon` token。
fn is_path_separator_after(spans: &[(TokenKind, usize, usize)], current: usize) -> bool {
    let mut colon_count = 0;
    for &(kind, _, _) in &spans[(current + 1)..] {
        if is_ws(kind) {
            continue;
        }
        if matches!(kind, TokenKind::Colon) {
            colon_count += 1;
            if colon_count >= 2 {
                return true;
            }
            continue;
        }
        return false;
    }
    false
}

/// 检查指定 token 之前两个连续的非空白 token 是否为 `::`
fn is_path_separator_before(spans: &[(TokenKind, usize, usize)], current: usize) -> bool {
    let mut colon_count = 0;
    for &(kind, _, _) in spans[..current].iter().rev() {
        if is_ws(kind) {
            continue;
        }
        if matches!(kind, TokenKind::Colon) {
            colon_count += 1;
            if colon_count >= 2 {
                return true;
            }
        } else {
            return false;
        }
    }
    false
}

/// 为非 ASCII 模块名的文件式声明 `mod 名称;` 补充 `#[path = "名称.rs"]` 注解。
///
/// rustc 拒绝加载非 ASCII 标识符对应的模块文件（E0754），而方言项目
/// 的模块文件常以母语命名（如 `src/数学.zh` → `src/数学.rs`）；
/// 显式指定 path 后 rustc 可正常加载。仅处理以分号结尾的文件式声明，
/// 内联模块块（`mod 名称 { ... }`）与 ASCII 名不受影响。
///
/// 适用于 crate 根文件（`main.rs`/`lib.rs`）：子模块与根文件同处 `src/`。
/// 非根文件的多层目录布局请用 [`annotate_nested_mods_with_lines`]，
/// 虚拟托管布局（平铺 hash 产物）请用 [`annotate_mod_paths_with_lines`]。
pub fn annotate_non_ascii_mods(code: &str) -> String {
    annotate_non_ascii_mods_with_lines(code).0
}

/// 非根模块文件（多层目录）的 `#[path]` 注解，并返回行映射。
///
/// `self_stem` 为当前文件自身词干（如 `src/领域.rs` 传 `"领域"`）：
/// 其中 `mod 工具;` 的子文件实际位于 `src/领域/工具.rs`，而 `#[path]`
/// 相对当前文件所在目录 `src/` 解析，故注入 `#[path = "领域/工具.rs"]`。
/// 更深层同理（`src/领域/工具.rs` 传 `"工具"` → `#[path = "工具/内部.rs"]`，
/// 相对 `src/领域/`）。仅注解非 ASCII 名；ASCII 名遵循 rustc 默认查找。
pub fn annotate_nested_mods_with_lines(code: &str, self_stem: &str) -> (String, Vec<usize>) {
    annotate_mod_paths_with_lines(code, |name| {
        (!name.is_ascii()).then(|| format!("{self_stem}/{name}.rs"))
    })
}

/// 通用文件式 `mod` 声明 `#[path]` 注解器，并返回磁盘行 → 引擎直行映射。
///
/// `resolve(模块名) -> Some(相对路径)` 时在声明前插入
/// `#[path = "<相对路径>"]`；返回 `None` 不注解（如子文件不存在，
/// 保留 rustc 原生 E0583）。仅处理分号结尾的文件式声明与尚无 `#[path]`
/// 的声明；内联模块块（`mod 名 { ... }`）不受影响。
///
/// 供三类布局复用：根文件（`名.rs`）、多层真实布局（`自词干/名.rs`）、
/// 虚拟托管布局（平铺 hash 产物，`<hash>.rs`）。
pub fn annotate_mod_paths_with_lines(
    code: &str,
    resolve: impl Fn(&str) -> Option<String>,
) -> (String, Vec<usize>) {
    annotate_mod_paths_impl(code, &resolve)
}

/// 同 [`annotate_non_ascii_mods`]，并额外返回磁盘产物行 → 引擎直出行映射
///
/// 每处注解在 `mod` 声明所在行前插入一整行 `#[path = ...]`，其后所有磁盘
/// 行号相对引擎直出产物整体偏移（偏移量 = 该行之前的注解数）。诊断回译时
/// 必须先用本映射把 rustc 报告的磁盘行号换算回引擎直出行号，再走以引擎
/// 直出产物为基准的列映射；否则多模块入口文件（如 `main.zh` 声明 3 个
/// `模块 xxx;`）的诊断行号会系统性偏移 3 行。映射为 0-based：
/// `line_map[磁盘行] = 引擎直出行`（注解行归属其所在 `mod` 声明行）。
pub fn annotate_non_ascii_mods_with_lines(code: &str) -> (String, Vec<usize>) {
    annotate_mod_paths_impl(code, &|name| {
        (!name.is_ascii()).then(|| format!("{name}.rs"))
    })
}

/// [`annotate_mod_paths_with_lines`] 的实现体
fn annotate_mod_paths_impl(
    code: &str,
    resolve: &dyn Fn(&str) -> Option<String>,
) -> (String, Vec<usize>) {
    // 文件首部 BOM：rustc 仅接受文件第一个字符位置的 BOM。首行恰好是
    // `mod 名;` 时，注解行插在 `mod` 前会把 BOM 挤到行内（
    // 「unknown start of token: \u{feff}」，编译失败）。剥离后统一前置，
    // BOM 不占行，行映射不受影响。
    let (bom, code) = match code.strip_prefix('\u{FEFF}') {
        Some(rest) => ("\u{FEFF}", rest),
        None => ("", code),
    };
    let tokens: Vec<_> = tokenize(code).collect();
    // 逐 token 的字节偏移（rustc_lexer 词法流覆盖全源，偏移连续）
    let mut offsets: Vec<usize> = Vec::with_capacity(tokens.len());
    let mut acc = 0usize;
    for t in &tokens {
        offsets.push(acc);
        acc += t.len;
    }
    let skip_trivia = |mut idx: usize| {
        while idx < tokens.len()
            && matches!(
                tokens[idx].kind,
                TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
            )
        {
            idx += 1;
        }
        idx
    };
    let mut insertions: Vec<(usize, String)> = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Ident {
            continue;
        }
        let text = &code[offsets[i]..offsets[i] + token.len];
        if text != "mod" {
            continue;
        }
        let name_idx = skip_trivia(i + 1);
        let Some(name_t) = tokens.get(name_idx) else {
            continue;
        };
        if name_t.kind != TokenKind::Ident {
            continue;
        }
        let semi_idx = skip_trivia(name_idx + 1);
        // 仅文件式声明（分号结尾）需要注解；内联模块块以 `{` 开头
        if !matches!(tokens.get(semi_idx).map(|t| t.kind), Some(TokenKind::Semi)) {
            continue;
        }
        let name = &code[offsets[name_idx]..offsets[name_idx] + name_t.len];
        let Some(mod_path) = resolve(name) else {
            continue;
        };
        // 插入点：若有可见性修饰 `pub`，注解必须在 pub 之前
        let mut insert_at = offsets[i];
        let mut back = i;
        while back > 0
            && matches!(
                tokens[back - 1].kind,
                TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
            )
        {
            back -= 1;
        }
        if back > 0
            && tokens[back - 1].kind == TokenKind::Ident
            && &code[offsets[back - 1]..offsets[back - 1] + tokens[back - 1].len] == "pub"
        {
            insert_at = offsets[back - 1];
        }
        // 已有 #[path] 注解时不重复添加：注解可独占多行，需向上逐行扫描属性链
        let line_start = code[..insert_at].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let mut scan_start = line_start;
        let mut has_path_attr = false;
        loop {
            let line = code[scan_start..insert_at].lines().next().unwrap_or("");
            // 首行可能是 mod 所在行的缩进前缀；上移后的行应为属性行
            if scan_start != line_start || line.trim_start().starts_with("#[") {
                if line.contains("#[path") {
                    has_path_attr = true;
                    break;
                }
                if !line.trim_start().starts_with("#[") {
                    break; // 属性链被普通行/空行打断
                }
            }
            if scan_start == 0 {
                break;
            }
            let prev_start = code[..scan_start - 1]
                .rfind('\n')
                .map(|p| p + 1)
                .unwrap_or(0);
            if prev_start == scan_start {
                break;
            }
            scan_start = prev_start;
        }
        if has_path_attr {
            continue;
        }
        let indent = &code[line_start..insert_at];
        insertions.push((insert_at, format!("#[path = \"{mod_path}\"]\n{indent}")));
    }
    // 按插入点升序回放：引擎直出文本的换行推进引擎行号；注解插入文本
    // 的换行不推进（插入行仍归属其所在 `mod` 声明行）
    let mut result =
        String::with_capacity(code.len() + insertions.iter().map(|(_, t)| t.len()).sum::<usize>());
    result.push_str(bom);
    let mut line_map: Vec<usize> = vec![0];
    let mut engine_line = 0usize;
    let mut last = 0usize;
    for (pos, text) in &insertions {
        append_with_line_map(
            &mut result,
            &mut line_map,
            &mut engine_line,
            &code[last..*pos],
            true,
        );
        append_with_line_map(&mut result, &mut line_map, &mut engine_line, text, false);
        last = *pos;
    }
    append_with_line_map(
        &mut result,
        &mut line_map,
        &mut engine_line,
        &code[last..],
        true,
    );
    (result, line_map)
}

/// 追加文本并同步维护行映射：`from_engine=false`（注解插入文本）的换行
/// 不推进引擎行号，其余同普通文本
fn append_with_line_map(
    out: &mut String,
    line_map: &mut Vec<usize>,
    engine_line: &mut usize,
    text: &str,
    from_engine: bool,
) {
    for c in text.chars() {
        out.push(c);
        if c == '\n' {
            if from_engine {
                *engine_line += 1;
            }
            line_map.push(*engine_line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_path_map() -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("标准集合".to_string(), "std::collections".to_string());
        map.insert("文件系统".to_string(), "std::fs".to_string());
        map.insert("字符串".to_string(), "string".to_string());
        map
    }

    /// 模块路径段后跟 `::` 时被替换
    #[test]
    fn test_replace_module_path_segment() {
        let map = sample_path_map();
        let result = replace_module_paths("使用 标准集合::哈希映射;", &map);
        assert_eq!(result, "使用 std::collections::哈希映射;");
    }

    /// use 语句外的类型关联调用（如 `引用计数::新建`）不被模块映射替换，
    /// 留给别名映射处理成 `Rc::new`
    #[test]
    fn test_type_call_outside_use_not_replaced() {
        let map = sample_path_map();
        let result = replace_module_paths("标准集合::新建(5);", &map);
        assert_eq!(result, "标准集合::新建(5);");
    }

    /// use 语句末段（后跟分号）也参与路径映射替换
    #[test]
    fn test_use_tail_segment_replaced() {
        let map = sample_path_map();
        let result = replace_module_paths("使用 标准集合::字符串;", &map);
        assert_eq!(result, "使用 std::collections::string;");
    }

    /// use 语句单段路径（无 `::`）直接映射整个模块
    #[test]
    fn test_use_single_segment_replaced() {
        let map = sample_path_map();
        let result = replace_module_paths("使用 文件系统;", &map);
        assert_eq!(result, "使用 std::fs;");
    }

    /// 映射表未覆盖的末段保持原样，交给别名替换处理（如 `服务器` → `Server`）
    #[test]
    fn test_use_tail_unmapped_kept() {
        let map = sample_path_map();
        let result = replace_module_paths("使用 salvo::服务器;", &map);
        assert_eq!(result, "使用 salvo::服务器;");
    }

    /// 复用外部 token 流的入口与现分词入口逐字等价
    ///
    /// 两条路径同一实现，差别只在 token 从哪来（现分词 vs 调用方已分的向量）。
    /// 覆盖：use 段内/段外、跨行 use、注释与空白穿插、无 use 文件（字符串早退）、
    /// 未映射末段、连字符 crate 名、空文件。
    #[test]
    fn test_tokens_entry_matches_fresh_tokenize() {
        let map = HashMap::from([
            ("标准集合".to_string(), "std::collections".to_string()),
            ("文件系统".to_string(), "std::fs".to_string()),
            ("字符串".to_string(), "string".to_string()),
            ("日志订阅".to_string(), "tracing-subscriber".to_string()),
        ]);
        for src in [
            "使用 标准集合::哈希映射;\n",
            "使用 文件系统;\n",
            "使用 标准集合::字符串 as 映射;\n使用 文件系统::文件;\n",
            "使用 标准集合 :: 字符串;\n",
            "使用 日志订阅 as 日志框架;\n",
            "使用 salvo::服务器;\n",
            "fn main() {\n    let x = 标准集合::新建(5);\n}\n",
            "使用 文件系统::文件;\nfn f() { 文件系统::读('a'); }\n",
            "// 使用 标准集合::哈希映射;\nlet x = 1;\n",
            "fn main() {}\n",
            "",
        ] {
            let fresh = replace_module_paths_with_map(src, &map);
            let tokens = crate::collect_tokens(src);
            let reused = replace_module_paths_in_tokens(src, &map, &tokens);
            assert_eq!(fresh.output, reused.output, "{src:?}：译文不一致");
            assert_eq!(fresh.edits.len(), reused.edits.len(), "{src:?}：编辑条数");
            for (i, (a, b)) in fresh.edits.iter().zip(&reused.edits).enumerate() {
                assert_eq!(a.源偏移, b.源偏移, "{src:?} 第 {i} 条偏移");
                assert_eq!(a.length, b.length, "{src:?} 第 {i} 条长度");
                assert_eq!(a.original, b.original, "{src:?} 第 {i} 条原文");
                assert_eq!(a.替换文字, b.替换文字, "{src:?} 第 {i} 条替换文本");
            }
        }
    }

    /// crate 名含连字符时（Cargo 名 ≠ Rust 路径名）：替换值规范化为下划线形式
    #[test]
    fn test_use_hyphen_crate_name_normalized() {
        let map = HashMap::from([("日志订阅".to_string(), "tracing-subscriber".to_string())]);
        let result = replace_module_paths_with_map("使用 日志订阅 as 日志框架;", &map);
        assert_eq!(result.output, "使用 tracing_subscriber as 日志框架;");
        assert_eq!(result.edits.len(), 1);
        assert_eq!(result.edits[0].替换文字, "tracing_subscriber");
    }

    /// 无连字符的路径段不受规范化影响
    #[test]
    fn test_use_plain_path_not_normalized() {
        let map = sample_path_map();
        let result = replace_module_paths("使用 标准集合::哈希映射;", &map);
        assert_eq!(result, "使用 std::collections::哈希映射;");
    }

    /// use 语句结束后（分号后）恢复默认行为
    #[test]
    fn test_use_scope_ends_at_semicolon() {
        let map = sample_path_map();
        let result = replace_module_paths("使用 标准集合::哈希映射; 标准集合::新建(1);", &map);
        assert_eq!(
            result,
            "使用 std::collections::哈希映射; 标准集合::新建(1);"
        );
    }

    /// 组合词内部的中文不被破坏（token 级替换）
    #[test]
    fn test_compound_word_not_broken() {
        let map = sample_path_map();
        let result = replace_module_paths("读取全部字符串(\"a.txt\")", &map);
        assert_eq!(result, "读取全部字符串(\"a.txt\")");
    }

    /// 普通中文变量名（后无 `::`）不被替换
    #[test]
    fn test_plain_ident_not_replaced() {
        let map = sample_path_map();
        let result = replace_module_paths("让 字符串 = 1;", &map);
        assert_eq!(result, "让 字符串 = 1;");
    }

    /// 空映射表直接返回原文
    #[test]
    fn test_empty_map() {
        let result = replace_module_paths("让 x = 1;", &HashMap::new());
        assert_eq!(result, "让 x = 1;");
    }

    /// 编辑表：输入偏移精确指向被替换的路径段
    #[test]
    fn test_with_map_records_edits() {
        let map = sample_path_map();
        let result = replace_module_paths_with_map("使用 标准集合::哈希映射;", &map);
        assert_eq!(result.output, "使用 std::collections::哈希映射;");
        assert_eq!(result.edits.len(), 1);
        let e = &result.edits[0];
        assert_eq!(e.original, "标准集合");
        assert_eq!(e.替换文字, "std::collections");
        assert_eq!(
            &"使用 标准集合::哈希映射;"[e.源偏移..e.源偏移 + e.length],
            "标准集合"
        );
    }

    // ===== qualify（crate:: 前缀）测试（自 lsp crate 迁移） =====

    #[test]
    fn test_qualify_adds_prefix() {
        let set = HashSet::from(["辅助".to_string(), "主".to_string()]);
        assert_eq!(
            qualify_module_paths_with_map(
                "fn main() {\n    辅助::辅助函数();\n}",
                &set,
                &HashSet::new()
            )
            .output,
            "fn main() {\n    crate::辅助::辅助函数();\n}"
        );
    }

    #[test]
    fn test_qualify_no_double_prefix() {
        let set = HashSet::from(["辅助".to_string(), "主".to_string()]);
        assert_eq!(
            qualify_module_paths_with_map("crate::辅助::辅助函数()", &set, &HashSet::new()).output,
            "crate::辅助::辅助函数()"
        );
        // 其他路径段内的模块名（a::辅助）不重复处理
        assert_eq!(
            qualify_module_paths_with_map("a::辅助::辅助函数()", &set, &HashSet::new()).output,
            "a::辅助::辅助函数()"
        );
    }

    #[test]
    fn test_qualify_non_module_untouched() {
        let set = HashSet::from(["辅助".to_string(), "主".to_string()]);
        assert_eq!(
            qualify_module_paths_with_map("x::方法()", &set, &HashSet::new()).output,
            "x::方法()"
        );
    }

    #[test]
    fn test_qualify_empty_set_keeps_original() {
        assert_eq!(
            qualify_module_paths_with_map("辅助::辅助函数()", &HashSet::new(), &HashSet::new())
                .output,
            "辅助::辅助函数()"
        );
    }

    #[test]
    fn test_qualify_records_edits() {
        let set = HashSet::from(["辅助".to_string()]);
        let result = qualify_module_paths_with_map("fn f() { 辅助::x() }", &set, &HashSet::new());
        assert_eq!(result.output, "fn f() { crate::辅助::x() }");
        assert_eq!(result.edits.len(), 1);
        let e = &result.edits[0];
        assert_eq!(e.original, "辅助");
        assert_eq!(e.替换文字, "crate::辅助");
    }

    /// 同文件类型声明名与模块名重名：裸路径按本地类型解析，不加前缀
    /// （weix-1 `struct 项目配置` 与 `模块 项目配置` 重名的 E0425 回归）
    #[test]
    fn test_qualify_shadowed_by_type_decl() {
        let set = HashSet::from(["项目配置".to_string()]);
        let src = "struct 项目配置 {}\nfn f() { 项目配置::default() }";
        assert_eq!(
            qualify_module_paths_with_map(src, &set, &HashSet::new()).output,
            src
        );
    }

    /// use 导入的类型名与模块名重名：裸路径解析为导入类型，不加前缀
    #[test]
    fn test_qualify_shadowed_by_use_import() {
        let set = HashSet::from(["项目配置".to_string()]);
        let src = "use crate::项目配置::项目配置;\nfn f() { 项目配置::default() }";
        assert_eq!(
            qualify_module_paths_with_map(src, &set, &HashSet::new()).output,
            src
        );
    }

    /// glob 导入 + 项目项名命中：可能经 glob 引入作用域，不加前缀；
    /// 无 glob 时项目项名不影响前缀（模块引用照旧补全）
    #[test]
    fn test_qualify_glob_with_item_names() {
        let set = HashSet::from(["项目配置".to_string()]);
        let items = HashSet::from(["项目配置".to_string()]);
        let src = "use super::*;\nfn f() { 项目配置::default() }";
        assert_eq!(qualify_module_paths_with_map(src, &set, &items).output, src);
        assert_eq!(
            qualify_module_paths_with_map("fn f() { 项目配置::load() }", &set, &items).output,
            "fn f() { crate::项目配置::load() }"
        );
    }

    /// mod 声明名不豁免：虚拟项目会抹除文件式声明，前缀仍须补全
    #[test]
    fn test_qualify_mod_decl_not_shadowed() {
        let set = HashSet::from(["工具".to_string()]);
        assert_eq!(
            qualify_module_paths_with_map(
                "mod 工具;\nfn f() { 工具::加一() }",
                &set,
                &HashSet::new()
            )
            .output,
            "mod 工具;\nfn f() { crate::工具::加一() }"
        );
    }

    /// glob 来源段不是绑定名：`use crate::工具::*;` 不遮蔽 `工具`
    /// （glob 场景由项目项名另行判定；此处项名集合为空，照旧加前缀）
    #[test]
    fn test_qualify_glob_source_segment_not_bound() {
        let set = HashSet::from(["工具".to_string()]);
        assert_eq!(
            qualify_module_paths_with_map(
                "use crate::工具::*;\nfn f() { 工具::加一() }",
                &set,
                &HashSet::new()
            )
            .output,
            "use crate::工具::*;\nfn f() { crate::工具::加一() }"
        );
    }

    // ===== strip（文件式模块声明抹除）测试 =====

    /// 文件式模块声明被整体抹除为空格，且行号不变
    #[test]
    fn test_strip_file_mod_decl() {
        let input = "mod 日志设置;\nfn main() {}";
        let output = strip_file_module_decls(input);
        assert_eq!(output, "         \nfn main() {}");
        assert_eq!(output.lines().count(), input.lines().count());
    }

    /// 可见性修饰 `pub` 随声明一并抹除
    #[test]
    fn test_strip_pub_mod_decl() {
        let input = "pub mod 工具;\nfn f() {}";
        let output = strip_file_module_decls(input);
        assert_eq!(output, "           \nfn f() {}");
    }

    /// 可见性括号形式 `pub(crate)` 随声明一并抹除
    #[test]
    fn test_strip_pub_crate_mod_decl() {
        let input = "pub(crate) mod 工具;\nfn f() {}";
        let output = strip_file_module_decls(input);
        assert_eq!(output, "                  \nfn f() {}");
    }

    /// 属性与文档注释随声明一并抹除（避免悬空修饰）
    #[test]
    fn test_strip_attributed_mod_decl() {
        // `#[cfg(test)]`（12 字符）与 `mod 测试;`（7 字符）分别变为等宽空格
        let input = "#[cfg(test)]\nmod 测试;\nfn f() {}";
        let output = strip_file_module_decls(input);
        assert_eq!(
            output,
            format!("{}\n{}\nfn f() {{}}", " ".repeat(12), " ".repeat(7))
        );

        // `/// 工具模块`（8 字符）与 `mod 工具;`（7 字符）分别变为等宽空格
        let input = "/// 工具模块\nmod 工具;";
        let output = strip_file_module_decls(input);
        assert_eq!(output, format!("{}\n{}", " ".repeat(8), " ".repeat(7)));
    }

    /// 多行属性随声明一并抹除，且每行 UTF-16 列宽严格不变
    #[test]
    fn test_strip_multiline_attr_keeps_position() {
        let input = "#[cfg(\n    all(test, unix)\n)]\nmod 工具;\nfn f() {}";
        let output = strip_file_module_decls(input);
        let input_lines: Vec<&str> = input.lines().collect();
        let output_lines: Vec<&str> = output.lines().collect();
        assert_eq!(input_lines.len(), output_lines.len());
        for (a, b) in input_lines.iter().zip(&output_lines) {
            assert_eq!(a.encode_utf16().count(), b.encode_utf16().count());
        }
        assert!(output.ends_with("fn f() {}"));
    }

    /// 内联模块（含其内容）不被处理
    #[test]
    fn test_inline_mod_kept() {
        let input = "mod 工具 {\n    fn f() {}\n}\nfn main() {}";
        assert_eq!(strip_file_module_decls(input), input);
    }

    /// 内联模块内的文件式声明同样被抹除
    #[test]
    fn test_strip_nested_file_mod_inside_inline() {
        let input = "mod 外层 {\n    mod 内层;\n}";
        let output = strip_file_module_decls(input);
        assert_eq!(output, "mod 外层 {\n           \n}");
    }

    /// 属性属于上一个项时不随声明抹除
    #[test]
    fn test_attr_belonging_to_prev_item_kept() {
        let input = "#[cfg(test)]\nfn f() {}\nmod 工具;";
        let output = strip_file_module_decls(input);
        assert_eq!(output, "#[cfg(test)]\nfn f() {}\n       ");
    }

    /// 含 mod 子串的标识符不被误判
    #[test]
    fn test_mod_substring_ident_not_stripped() {
        let input = "fn commode() {}\nfn mode() {}";
        assert_eq!(strip_file_module_decls(input), input);
    }

    /// 无文件式声明时原样返回
    #[test]
    fn test_strip_no_decl_unchanged() {
        let input = "fn main() {}\nfn f() { let mode = 1; }";
        assert_eq!(strip_file_module_decls(input), input);
    }

    /// 非 ASCII 文件式模块声明补充 #[path] 注解；ASCII 名与内联模块不受影响
    #[test]
    fn test_annotate_non_ascii_mods() {
        let src = "#[path = \"数学.rs\"]\nmod 数学;";
        assert_eq!(annotate_non_ascii_mods(src), src);
        assert_eq!(annotate_non_ascii_mods("mod ascii;"), "mod ascii;");
        assert_eq!(annotate_non_ascii_mods("mod 工具 {}"), "mod 工具 {}");
        let out = annotate_non_ascii_mods("fn main() {\n    mod 数学;\n}");
        assert_eq!(
            out,
            "fn main() {\n    #[path = \"数学.rs\"]\n    mod 数学;\n}"
        );
    }

    /// 注解行映射：磁盘行 → 引擎直出行（0-based），注解行归属其 mod 声明行，
    /// 多处注解时偏移逐处累积
    #[test]
    fn test_annotate_non_ascii_mod_line_map() {
        let (out, line_map) = annotate_non_ascii_mods_with_lines("mod 数学;\nfn main() {}");
        assert_eq!(out, "#[path = \"数学.rs\"]\nmod 数学;\nfn main() {}");
        assert_eq!(line_map, vec![0, 0, 1]);

        let (_, line_map) = annotate_non_ascii_mods_with_lines("fn main() {}\nfn f() {}");
        assert_eq!(line_map, vec![0, 1]);

        let two = "mod 数学;\nmod 物理;\nfn main() {}";
        let (out, line_map) = annotate_non_ascii_mods_with_lines(two);
        assert_eq!(
            out,
            "#[path = \"数学.rs\"]\nmod 数学;\n#[path = \"物理.rs\"]\nmod 物理;\nfn main() {}"
        );
        assert_eq!(line_map, vec![0, 0, 1, 1, 2]);
    }

    /// 首部 BOM 回归：首行恰为 `mod 名;` 时注解行不得把 BOM 挤到行内
    /// （rustc 仅接受文件第一个字符位置的 BOM，否则 unknown start of token）
    #[test]
    fn test_annotate_preserves_leading_bom() {
        let (out, line_map) = annotate_non_ascii_mods_with_lines("\u{FEFF}mod 数学;\nfn main() {}");
        assert!(
            out.starts_with("\u{FEFF}#[path = \"数学.rs\"]\nmod 数学;"),
            "{out}"
        );
        // BOM 不占行：行映射与无 BOM 时一致
        assert_eq!(line_map, vec![0, 0, 1]);
        // 无 BOM 时行为不变
        assert_eq!(
            annotate_non_ascii_mods_with_lines("mod 数学;"),
            ("#[path = \"数学.rs\"]\nmod 数学;".to_string(), vec![0, 0])
        );
    }
}
