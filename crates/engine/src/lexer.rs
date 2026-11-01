//! 词法处理模块 - 将母语源码根据关键字映射转译为标准 Rust 源码
//!
//! 基于 rustc_lexer 的 token 级转译，保证注释与字符串字面量内容不被误改。
//! 支持宏感叹号自动补充、原始标识符（r#）处理、反向转译（Rust → 母语）。

use crate::cache::{MapText, SourceMapEntry};
use crate::name_cache::NameCache;
use rustc_lexer::{TokenKind, tokenize};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

/// 转译结果：标准 Rust 输出、词法阶段源映射与全管线编辑表
#[derive(Debug, Clone)]
pub struct TranspileResult {
    /// 转译后的代码文本
    pub output: String,
    /// 词法阶段源映射条目列表（记录被替换的标识符；replacement 是词法阶段自己的
    /// 产物，因此不含宏调用自动补的 `!`）
    pub source_map: Vec<SourceMapEntry>,
    /// 全管线编辑表：与 source_map 同偏移，但 replacement 为**最终输出文本**
    ///（宏调用自动补充的 `!` 已计入 replacement），供全管线编辑地图组合使用
    pub final_edits: Vec<SourceMapEntry>,
}

/// 标识符映射事实：一次解析出词法阶段无条件要问的那两张表的答案
///
/// 词法阶段对每个标识符都要问关键字表与宏表，每次都是一次随机内存访问——
/// 2000 行样例（3930 个标识符）实测约 22 ns/次。同一份源码里标识符高度重复
/// （局部变量、字段名被反复引用），因此按名字直映缓存解析结果：重复名字只付
/// 一次廉价探测，首次出现才查表。
///
/// 只缓存无条件的这两张表：派生特征表、use 让位词、方法位让位词都只在特定
/// 上下文（`#[派生(` 内、use 段内、`.` 之后）才需要，现场查一次比塞进缓存更
/// 便宜——早先每个未命中的名字都要连查五张表，实测占词法阶段 16%（1192 份
/// 语料 0.84 ms）。
#[derive(Clone, Copy)]
struct IdentFacts<'a> {
    /// 关键字表命中值（按去掉 `r#` 的名字查）
    keyword: Option<&'a str>,
    /// 宏表命中值（按去掉 `r#` 的名字查）
    macro_en: Option<&'a str>,
}

/// 将母语 Rust 源代码转换为标准 Rust 源代码字符串
///
/// 参数：
///   source: 母语源代码（如 .zh 文件内容）
///   keyword_map: 母语关键字到英文关键字的映射表
/// 返回：标准 Rust 源代码
pub fn transpile_source(source: &str, keyword_map: &HashMap<String, String>) -> String {
    let empty_map = HashMap::new();
    transpile_source_with_macro_map(source, keyword_map, &empty_map, &HashMap::new())
}

/// 将母语 Rust 源代码转换为标准 Rust 源代码字符串（支持宏感叹号自动补充）
///
/// 参数：
///   source: 母语源代码（如 .zh 文件内容）
///   keyword_map: 母语关键字到英文关键字的映射表
///   macro_names: 所有中文宏名的集合（不含感叹号），用于自动补充 `!`
/// 返回：标准 Rust 源代码
///
/// 注意：宏名的英文替换优先使用宏映射（见 [`transpile_source_with_macro_map`]）；
/// 本函数以 keyword_map 兜底（宏名在 keyword_map 中被类型节覆盖时值可能不准，
/// 如 `向量` 在类型节映射为 `Vec`、在宏节映射为 `vec`）。
pub fn transpile_source_with_macros(
    source: &str,
    keyword_map: &HashMap<String, String>,
    macro_names: &HashSet<String>,
) -> String {
    let macro_map: HashMap<String, String> = macro_names
        .iter()
        .map(|name| {
            (
                name.clone(),
                keyword_map
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| name.clone()),
            )
        })
        .collect();
    transpile_with_map(
        source,
        keyword_map,
        &macro_map,
        &HashMap::new(),
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
    )
    .output
}

/// 同 [`transpile_source_with_macros`]，但宏名英文替换来自宏映射表
/// （宏名 → 英文宏名，如 `向量` → `vec`），可避免宏名在类型节与宏节
/// 重复时被类型值覆盖（如 `向量` 类型节为 `Vec`、宏节为 `vec`）。
pub fn transpile_source_with_macro_map(
    source: &str,
    keyword_map: &HashMap<String, String>,
    macro_map: &HashMap<String, String>,
    derive_map: &HashMap<String, String>,
) -> String {
    transpile_with_map(
        source,
        keyword_map,
        macro_map,
        derive_map,
        &HashSet::new(),
        &HashSet::new(),
        &HashMap::new(),
    )
    .output
}

/// 同 [`transpile_source_with_macros`]，同时产出源映射（被替换标识符的源偏移与翻译前后文本）
///
/// 源映射在标识符实际被替换时记录；宏感叹号自动补充不产生映射条目。
///
/// `defer_in_use`：use 语句内的让位词集合（模块路径/别名映射键，见
/// [`crate::mapping_manager::MappingManager::get_use_defer_words`]）。
/// use 段内命中该集合的词跳过关键字/宏替换，原样保留给后续阶段处理。
///
/// `method_defer`：方法位让位词集合（见
/// [`crate::mapping_manager::MappingManager::get_method_defer_words`]）——
/// 词法替换值是保留关键字的词（`枚举`→enum）在方法位（前面是 `.`）原样
/// 保留：`.枚举()` 直译会成非法的 `.enum()`，让位后由别名阶段替换为
/// `.enumerate()`。
///
/// `literal_suffix_map`：数字字面量后缀的兜底查表（标准库标识符映射，见
/// [`crate::mapping_manager::MappingManager::get_alias_map`]）——标准库层
/// 数值类型词（如 `无符号机器整数` = usize）不在 keyword_map，黏连数字后
/// 同样需要拆分替换。
pub fn transpile_with_map(
    source: &str,
    keyword_map: &HashMap<String, String>,
    macro_map: &HashMap<String, String>,
    derive_map: &HashMap<String, String>,
    defer_in_use: &HashSet<String>,
    method_defer: &HashSet<String>,
    literal_suffix_map: &HashMap<String, String>,
) -> TranspileResult {
    transpile_with_map_tokens(
        source,
        keyword_map,
        macro_map,
        derive_map,
        defer_in_use,
        method_defer,
        literal_suffix_map,
    )
    .0
}

/// 同 [`transpile_with_map`]，另把本阶段自己分好的**输入** token 流交给调用方
///
/// 转译需要随机访问整条 token 流（前瞻宏调用的 `!`、后顾声明位），因此本
/// 阶段无论如何都要把源码分词一次并留一份向量。转译之后的管线还需要在
/// **词法产物**上分词一次（`lex.output` 的 token 流），而那份流可由「本阶段
/// 的输入 token 流 + 本阶段的编辑表」派生（见 [`crate::rebase_tokens`]）：
/// 于是这里把手里的向量顺手交出去，省掉管线对输出的整趟重新分词（2000 行
/// 样例实测 421 µs）。
///
/// [`crate::rebase_tokens`]: crate#rebase_tokens
pub(crate) fn transpile_with_map_tokens(
    source: &str,
    keyword_map: &HashMap<String, String>,
    macro_map: &HashMap<String, String>,
    derive_map: &HashMap<String, String>,
    defer_in_use: &HashSet<String>,
    method_defer: &HashSet<String>,
    literal_suffix_map: &HashMap<String, String>,
) -> (TranspileResult, Vec<rustc_lexer::Token>) {
    // 收集所有 token 以便前瞻/后顾。token 原文按需切片（见 [`text_at`]）：
    // 早先为整条 token 流预建 `Vec<&str>`，2000 行文件因此多付一次全量分配、
    // 一趟遍历和每 token 16 字节的临时表，而实际回溯只跨紧邻的几个 token
    let mut token_stream: Vec<rustc_lexer::Token> = Vec::with_capacity(source.len() / 4 + 8);
    token_stream.extend(tokenize(source));
    // 输出只在替换点刷出：未替换的 token 不逐个 `push_str`，替换时才把
    // 「上次刷出位置 → 本 token 起点」的原始片段整段拷过去（2000 行样例
    // 1.5 万个 token 只有一百多个替换点）
    let mut output = String::with_capacity(source.len());
    let mut copied = 0usize;
    let mut current_offset = 0;
    // 本文件声明的名字（`函数 测试`/`结构体 测试`… 后的标识符）：调用位不是宏调用。
    // 与宏同名的用户函数被调用时（`测试(41)`），若不排除会被宏糖补 `!`
    // 产出不存在的宏 `test!(41)`。惰性构建：文件里没有宏调用形态时零开销。
    let mut declared_names: Option<HashSet<&str>> = None;
    // 全管线编辑表：replacement 为最终输出文本（宏补的 `!` 计入）
    let mut final_edits: Vec<SourceMapEntry> = Vec::new();
    // 词法表（`source_map`）**不在主循环里收**：它与 `final_edits` 逐条同偏移、同原文，
    // 只有「词法自动补 `!`」的条目不同（词法表记不含 `!` 的替换值，或压根不记）。
    // 两条表在循环里各写一份是词法阶段里最大的一项重复写入：2000 行样例（两条表
    // 各 2525 条、每条 80 字节）上改成「只收一张 + 末尾整表复制」（复制那一份实测
    // 28 µs）后，criterion 配对 A/B（改动前后交替构建、stderr 分流）测得词法阶段
    // 906.1 → 841.6 µs（−6.0%～−12.7%）。这里因此只记下少数差异条目（`final_edits`
    // 下标 + 词法阶段的替换值，`None` = 该条不进词法表），循环后由
    // [`build_lex_source_map`] 复制出词法表。
    let mut lex_overrides: Vec<(usize, Option<MapText>)> = Vec::new();
    // 派生参数态：`#[派生(` 之后的参数（直到 `)`）内的标识符
    // 优先查派生特征映射（`克隆` → `Clone`），避免与方法名别名（小写 clone）冲突
    let mut in_derive_params = false;
    // use 语句态：use 段内的路径段命中"让位词"（模块路径/别名映射键）时
    // 跳过关键字/宏替换，交由后续模块路径与别名阶段按标准库语义替换——
    // 同名词在宏表与标准库表重复时 use 段内标准库优先（`文件`=file/File，
    // `使用 标准库::文件系统::文件` 必须产出 `use std::fs::File` 而非小写 file ）
    let mut in_use_stmt = false;
    // 标识符事实缓存（见 [`IdentFacts`] 与 [`crate::name_cache`]）：按名字直映
    let mut ident_cache: NameCache<'_, IdentFacts<'_>> = NameCache::new();
    // 上一个有意义 token 是否为 `.`（方法位判定）：空白/注释不改变该状态，
    // 与早先逐位回溯 `is_preceded_by_dot` 逐字等价，但不必再回扫 token 流
    let mut prev_is_dot = false;

    for i in 0..token_stream.len() {
        let token = &token_stream[i];
        let length = token.len;
        let text = &source[current_offset..current_offset + length];

        match token.kind {
            TokenKind::Ident | TokenKind::RawIdent => {
                let raw_name = text.strip_prefix("r#").unwrap_or(text);
                // 原始标识符（`r#匹配`）是用户显式转义的名字：关键字查表口径
                // 与普通标识符不同（宏名不能是原始标识符，故永不补 `!`）
                let is_raw = raw_name.len() != text.len();
                // 名字级缓存：命中则复用上次解析的几张表答案，未命中才逐表查
                let facts = ident_cache.get_or(text, || IdentFacts {
                    keyword: keyword_map.get(raw_name).map(String::as_str),
                    macro_en: macro_map.get(raw_name).map(String::as_str),
                });

                // 宏调用上下文优先：宏名后跟 `!` 或 `(/[/{`（且前面不是 `::`）时，
                // 使用宏映射替换为英文宏名并确保感叹号，避免宏名在类型节与宏节
                // 重复时被类型值覆盖（如 `向量` 类型节为 `Vec`、宏节为 `vec`）。
                // 方法调用位（前面是 `.`）不适用：宏不经 `.` 访问，
                // `.格式化(` 是方法调用，不补 `!`（后续走关键字映射 → `.format(`）
                let mut handled = false;
                if facts.macro_en.is_some()
                    // 原始标识符（`r#匹配`）是用户显式转义的名字：宏名不能是
                    // 原始标识符，故永不补 `!`（`r#匹配(3)` → `r#match(3)`）
                    && !is_raw
                    && !is_preceded_by_double_colon(&token_stream, i)
                    && !prev_is_dot
                    && !is_attribute_name(&token_stream, i)
                    && !(in_use_stmt && defer_in_use.contains(raw_name))
                    // 声明位豁免：声明关键字后的名字不是宏调用（`模块 测试 {` 的
                    // `测试` 不得补 `!`），修复 `mod test!` 一类非法产物。
                    && !is_preceded_by_decl_introducer(
                        source,
                        &token_stream,
                        i,
                        current_offset,
                        keyword_map,
                    )
                    // 本文件已声明同名函数/类型时，调用位按普通标识符处理
                    && !declared_names
                        .get_or_insert_with(|| {
                            collect_declared_names(source, &token_stream, keyword_map, macro_map)
                        })
                        .contains(raw_name)
                    && let Some(next_kind) = find_next_non_ws_kind(&token_stream, i + 1)
                {
                    let is_bang = matches!(next_kind, TokenKind::Not);
                    if is_open_bracket(next_kind) || is_bang {
                        // 借用映射表里的值而非 clone：宏名替换是热路径上少数
                        // 需要字符串的场合，未命中时直接沿用原文切片
                        let en_macro: &str = facts.macro_en.or(facts.keyword).unwrap_or(text);
                        // 最终输出文本（含自动补充的 `!`）：
                        // 与词法表语义不同——replacement 即输出原文，供列映射回放
                        let needs_bang = is_open_bracket(next_kind);
                        let appended: Cow<'_, str> = if needs_bang {
                            Cow::Owned(format!("{en_macro}!"))
                        } else {
                            Cow::Borrowed(en_macro)
                        };
                        if appended != text {
                            let index = final_edits.len();
                            final_edits.push(SourceMapEntry::new(
                                current_offset,
                                length,
                                text,
                                &appended,
                            ));
                            // 补了 `!`：词法表要的是不含 `!` 的替换值；替换值本就等于
                            // 原文（只补了 `!`）时词法表不记这一条
                            if needs_bang {
                                lex_overrides.push((
                                    index,
                                    (en_macro != text).then(|| MapText::from(en_macro)),
                                ));
                            }
                        }
                        output.push_str(&source[copied..current_offset]);
                        output.push_str(en_macro);
                        // 后跟开括号但无 `!` 时自动补 `!`
                        if needs_bang {
                            output.push('!');
                        }
                        copied = current_offset + length;
                        handled = true;
                    }
                }

                if !handled {
                    // 全走借用（Cow）：热路径上绝大多数标识符未命中任何映射表，
                    // 逐 token `to_string()` 是纯分配损耗（2000 行文件数万次）
                    let replacement: Cow<'_, str> =
                        if in_use_stmt && defer_in_use.contains(raw_name) {
                            // use 段让位：保持原文，由模块路径/别名阶段替换
                            Cow::Borrowed(text)
                        } else if prev_is_dot && method_defer.contains(raw_name) {
                            // 方法位让位：替换值是保留关键字，直译非法（`.枚举()` →
                            // `.enum()`）；保持原文由别名阶段替换为有效词条（enumerate）
                            Cow::Borrowed(text)
                        } else if in_derive_params
                            && let Some(en) = derive_map.get(raw_name).map(String::as_str)
                        {
                            // 派生参数态内：优先查派生特征映射（如 `克隆` → `Clone`）
                            Cow::Borrowed(en)
                        } else if is_raw {
                            match facts.keyword {
                                Some(en) => Cow::Owned(format!("r#{}", en)),
                                None => Cow::Borrowed(text),
                            }
                        } else if let Some(en) = facts.keyword {
                            Cow::Borrowed(en)
                        } else {
                            Cow::Borrowed(text)
                        };

                    // `宏规则`（macro_rules）声明形式：后跟宏名时必须带 `!`，
                    // 如 `宏规则 创建向量 { ... }` → `macro_rules! 创建向量 { ... }`
                    let macro_rules_decl = replacement == "macro_rules"
                        && matches!(
                            find_next_non_ws_kind(&token_stream, i + 1),
                            Some(TokenKind::Ident)
                        );
                    let final_replacement: &str = if macro_rules_decl {
                        "macro_rules!"
                    } else {
                        replacement.as_ref()
                    };

                    // 记录实际发生替换的标识符映射（两张表此处同值，只收一份）
                    if final_replacement != text {
                        final_edits.push(SourceMapEntry::new(
                            current_offset,
                            length,
                            text,
                            final_replacement,
                        ));
                        output.push_str(&source[copied..current_offset]);
                        output.push_str(final_replacement);
                        copied = current_offset + length;
                    }
                }
                // use 语句态推进：本语言 use 关键字词形开启让位态，由 `;` 结束。
                // 词形经关键字映射反查（zh/ja `使用`、es/pt `usar`、ru `используй`
                // 等全部本地词形），不硬编码字面 `use`/`使用`——历史实现只识别
                // 这两种，其余语言 use 段让位态从未开启，冲突词（如 es `formato`
                // 宏 format 与模块 fmt 同名）被词法替换后残留英文宏名 `format::`
                //
                // 原始标识符按完整原文查（与上面缓存里的原名口径不同）
                let use_word = if is_raw {
                    keyword_map.get(text).map(String::as_str)
                } else {
                    facts.keyword
                };
                if text == "use" || use_word.is_some_and(|v| v == "use") {
                    in_use_stmt = true;
                }
            }
            TokenKind::OpenParen => {
                // 派生属性：`派生`（转译为 derive）后跟 `(` 进入参数态
                if !in_derive_params
                    && prev_ident_is(
                        source,
                        &token_stream,
                        i,
                        current_offset,
                        keyword_map,
                        "derive",
                    )
                {
                    in_derive_params = true;
                }
            }
            TokenKind::CloseParen => {
                // 离开派生参数态
                in_derive_params = false;
            }
            // use 语句以分号结束，退出让位态
            TokenKind::Semi => {
                in_use_stmt = false;
            }
            // 数字字面量：数字与中文类型"黏连"（`0无符号微整数`）时，
            // rustc_lexer 把整串（含 Unicode 后缀）切为单个 Literal token，
            // 后缀是完整映射词（keywords 表或标准库表：`无符号微整数` → u8、
            // `无符号机器整数` → usize）时替换后缀（产出 `0u8`/`0usize`）；
            // 未命中保持原样（编译错误交由编译器报告）
            TokenKind::Literal { suffix_start, .. } => {
                let suffix = &text[suffix_start..];
                if !suffix.is_empty()
                    && !suffix.is_ascii()
                    && let Some(en) = keyword_map
                        .get(suffix)
                        .or_else(|| literal_suffix_map.get(suffix))
                {
                    let number = &text[..suffix_start];
                    let replaced = format!("{}{}", number, en);
                    let entry = SourceMapEntry::new(
                        current_offset + suffix_start,
                        suffix.len(),
                        suffix,
                        en,
                    );
                    final_edits.push(entry);
                    output.push_str(&source[copied..current_offset]);
                    output.push_str(&replaced);
                    copied = current_offset + length;
                }
            }
            // 其他所有 token 原样输出（随片段拷出，无需逐个 push）
            _ => {}
        }
        // 空白/注释不改变「上一个有意义 token」的状态
        if !is_whitespace(token.kind) {
            prev_is_dot = matches!(token.kind, TokenKind::Dot);
        }
        current_offset += length;
    }
    output.push_str(&source[copied..]);

    let source_map = build_lex_source_map(&final_edits, &lex_overrides);
    (
        TranspileResult {
            output,
            source_map,
            final_edits,
        },
        token_stream,
    )
}

/// 由全管线编辑表复制出词法阶段的源映射表
///
/// 两条表逐条同偏移、同原文（见 [`transpile_with_map_tokens`] 里的说明），只有
/// 词法自动补 `!` 的条目不同：`overrides` 按 `final_edits` 下标升序给出这些条目
/// 在词法阶段的替换值，`None` 表示该条不进词法表（替换值就是原文、只补了 `!`）。
/// 与「在循环里各 push 一份」逐条等价，但只付一次整表复制。
fn build_lex_source_map(
    final_edits: &[SourceMapEntry],
    overrides: &[(usize, Option<MapText>)],
) -> Vec<SourceMapEntry> {
    let mut out: Vec<SourceMapEntry> = Vec::with_capacity(final_edits.len());
    let mut next = 0usize;
    for (index, entry) in final_edits.iter().enumerate() {
        while next < overrides.len() && overrides[next].0 < index {
            next += 1;
        }
        if next < overrides.len() && overrides[next].0 == index {
            next += 1;
            if let Some(lex_text) = &overrides[next - 1].1 {
                out.push(SourceMapEntry {
                    source_offset: entry.source_offset,
                    length: entry.length,
                    original: entry.original.clone(),
                    replacement: lex_text.clone(),
                });
            }
            continue;
        }
        out.push(entry.clone());
    }
    out
}

/// 查找从指定位置开始的第一个非空白 token 的 kind
fn find_next_non_ws_kind(token_stream: &[rustc_lexer::Token], start: usize) -> Option<TokenKind> {
    for token in &token_stream[start..] {
        if !is_whitespace(token.kind) {
            return Some(token.kind);
        }
    }
    None
}

/// 取 token 流中序号 `idx` 的原文切片
///
/// `current_offset` 是序号 `current`（要求 `idx <= current`）token 的起始偏移。
/// 回溯只跨过紧邻的少量 token（空白/标点），因此是 O(1) 且零分配——取代
/// 早先为整条 token 流预建 `Vec<&str>` 的做法。
fn text_at<'a>(
    source: &'a str,
    token_stream: &[rustc_lexer::Token],
    current: usize,
    current_offset: usize,
    idx: usize,
) -> &'a str {
    let mut start = current_offset;
    let mut k = current;
    while k > idx {
        k -= 1;
        start = start.saturating_sub(token_stream[k].len);
    }
    let len = token_stream[idx].len;
    &source[start..start + len]
}

/// 判断当前位置之前的第一个非空白标识符（经关键字映射后）是否等于指定英文值
///
/// 用于识别派生属性：`#[派生(...)]` 中 `派生` 的映射值为 `derive`。
fn prev_ident_is(
    source: &str,
    token_stream: &[rustc_lexer::Token],
    current: usize,
    current_offset: usize,
    keyword_map: &HashMap<String, String>,
    en_value: &str,
) -> bool {
    prev_ident_text(source, token_stream, current, current_offset)
        .and_then(|name| keyword_map.get(name))
        .is_some_and(|value| value == en_value)
}

/// 返回当前位置之前第一个非空白标识符的原文（含 `r#` 前缀则剔除）
///
/// 中间遇到非空白 token 时返回 None（非紧邻）。
fn prev_ident_text<'a>(
    source: &'a str,
    token_stream: &[rustc_lexer::Token],
    current: usize,
    current_offset: usize,
) -> Option<&'a str> {
    let mut j = current;
    while j > 0 {
        j -= 1;
        let kind = token_stream[j].kind;
        if is_whitespace(kind) {
            continue;
        }
        if matches!(kind, TokenKind::Ident | TokenKind::RawIdent) {
            let raw = text_at(source, token_stream, current, current_offset, j);
            return Some(raw.strip_prefix("r#").unwrap_or(raw));
        }
        return None;
    }
    None
}

/// 声明引导词（对应母语词条的英文映射值）：其后紧跟的名字是「被声明的标识符」，
/// 不是宏调用——即便该名字恰好也是宏名（如 `测试` = `test`），也不得补 `!`。
///
/// 覆盖 `模块 X {` / `结构体 X {` / `函数 X(` 等「声明关键字 + 名字」形态；
/// 修复 `模块 测试 { … }` 被误转译为非法 `mod test! { … }` 的缺陷（`{` 被
/// 当作宏调用的花括号定界符）。
const DECL_INTRODUCER_VALUES: &[&str] = &[
    "fn", "let", "const", "static", "struct", "enum", "union", "type", "trait", "impl", "mod",
];

/// 判断指定位置的标识符是否紧跟声明引导词（如 `函数 测试` 中的 `测试`）。
///
/// 命中时该标识符处于声明位，应跳过宏感叹号自动补充，交由关键字映射照常替换。
fn is_preceded_by_decl_introducer(
    source: &str,
    token_stream: &[rustc_lexer::Token],
    current: usize,
    current_offset: usize,
    keyword_map: &HashMap<String, String>,
) -> bool {
    prev_ident_text(source, token_stream, current, current_offset)
        .and_then(|name| keyword_map.get(name))
        .is_some_and(|value| DECL_INTRODUCER_VALUES.contains(&value.as_str()))
}

/// 收集本文件声明的标识符名字（声明引导词之后紧跟的那个标识符）。
///
/// 用于抑制宏糖：`函数 测试` 已声明 `测试` 后，`测试(41)` 是该函数的调用，
/// 不应被当作宏调用补 `!`（否则产出不存在的 `test!(41)`）。只收集同时
/// 存在于宏表的名字——调用方仅在宏名分支上问该集合。
fn collect_declared_names<'a>(
    source: &'a str,
    token_stream: &[rustc_lexer::Token],
    keyword_map: &HashMap<String, String>,
    macro_map: &HashMap<String, String>,
) -> HashSet<&'a str> {
    let mut names = HashSet::new();
    // 顺序扫描并自维护运行偏移（避免为全文回溯重建 token 索引表）
    let mut offset = 0usize;
    for i in 0..token_stream.len() {
        let token_start = offset;
        let len = token_stream[i].len;
        offset += len;
        if !matches!(token_stream[i].kind, TokenKind::Ident | TokenKind::RawIdent) {
            continue;
        }
        // 先取前一个标识符再做一次查表（避免对 11 个引导词重复回溯）
        let Some(prev) = prev_ident_text(source, token_stream, i, token_start) else {
            continue;
        };
        let is_declaration = keyword_map
            .get(prev)
            .is_some_and(|value| DECL_INTRODUCER_VALUES.contains(&value.as_str()));
        if !is_declaration {
            continue;
        }
        let text = &source[token_start..token_start + len];
        let name = text.strip_prefix("r#").unwrap_or(text);
        if macro_map.contains_key(name) {
            names.insert(name);
        }
    }
    names
}

/// 将标准 Rust 源码反向转译为母语源码
///
/// 参数：
///   source: 标准 Rust 源代码（如 rustfmt 格式化后的虚拟 .rs 内容）
///   reverse_map: 英文关键字到母语关键字的映射表（由正向映射反转得到）
///   module_names: 当前已打开 .zh 文件的模块名集合。
///              代理为跨文件引用插入的 `crate::` 前缀在此被删除，
///              还原为母语中的裸路径（`辅助::`），与正向翻译的
///              `replace_module_paths` 互为逆操作。
/// 返回：母语源代码
///
/// 以 token 为单位匹配，天然避免子串误替换
/// （如 `i32` 不会被更短的 `i3` 错误替换），
/// 注释与字符串字面量内容保持原样，与正向翻译一一对应。
///
/// `added_crate_tokens` 为代理（LSP 虚拟项目）添加的 `crate::` 前缀在英文
/// 输出中的非空白 token 序号（见 [`added_crate_token_indices`]）：仅删除这些
/// 位置的前缀；用户显式书写的 `crate::`（不在集合中）原样保留，反向转译
/// 不再无差别删除，避免还原后代码丢失用户手写前缀。
pub fn reverse_transpile(
    source: &str,
    reverse_map: &HashMap<String, String>,
    module_names: &HashSet<String>,
    added_crate_tokens: &HashSet<usize>,
) -> String {
    // 收集 (token 种类, 文本) 对以便前瞻/后顾
    let token_stream: Vec<(TokenKind, &str)> = {
        let mut list = Vec::new();
        let mut offset = 0;
        for token in tokenize(source) {
            list.push((token.kind, &source[offset..offset + token.len]));
            offset += token.len;
        }
        list
    };
    // 预处理：每个 token 的非空白序号（rustfmt 等空白变化不影响序号，
    // 序号与 [`added_crate_token_indices`] 的计数口径一致）
    let mut nonspace_idxs = Vec::with_capacity(token_stream.len());
    let mut nonspace = 0usize;
    for (kind, _) in &token_stream {
        nonspace_idxs.push(nonspace);
        if !is_whitespace(*kind) {
            nonspace += 1;
        }
    }
    let mut output = String::with_capacity(source.len());
    let mut skip_tokens = 0usize; // 删除 crate:: 前缀 / macro_rules! 感叹号时跳过的 token 数

    for i in 0..token_stream.len() {
        if skip_tokens > 0 {
            skip_tokens -= 1;
            continue;
        }
        let (token, text) = &token_stream[i];
        match token {
            TokenKind::Ident | TokenKind::RawIdent => {
                let raw_name = if let Some(inner) = text.strip_prefix("r#") {
                    inner
                } else {
                    text
                };
                // 代理为跨文件引用插入的 `crate::` 前缀：仅当序号命中
                // 正向翻译记录（added_crate_tokens）时才整体删除；
                // 用户手写的 `crate::` 不在记录中，原样保留
                // （跳过数含中间空白，兼容 `crate :: 模块` 等带空格写法）
                if raw_name == "crate"
                    && added_crate_tokens.contains(&nonspace_idxs[i])
                    && let Some(skip) = crate_prefix_skip_count(&token_stream, i, module_names)
                {
                    skip_tokens = skip;
                    continue;
                }
                // `macro_rules! 名称` 声明形式反向转译为 `宏规则 名称`（省略感叹号）
                if raw_name == "macro_rules"
                    && let Some(skip) = bang_suffix_skip_count(&token_stream, i)
                {
                    skip_tokens = skip;
                }
                let zh = reverse_map
                    .get(raw_name)
                    .map(|s| s.as_str())
                    .unwrap_or(raw_name);
                if text.starts_with("r#") && !zh.starts_with("r#") {
                    output.push_str("r#");
                }
                output.push_str(zh);
            }
            _ => output.push_str(text),
        }
    }
    output
}

/// 计算全管线编辑地图中“代理添加的 `crate::` 前缀”在英文输出中的非空白 token 序号
///
/// 输入为最终英文输出文本与其全管线编辑地图（replacement 为最终输出文本）。
/// 净长度增量线性累积后，把 replacement 以 `crate::` 开头的条目（仅来自 LSP
/// 虚拟项目的跨文件引用前缀重写，用户手写前缀不产生编辑条目）的母语源偏移
/// 换算为英文输出偏移，再转为非空白 token 序号。
///
/// rustfmt 格式化不改变非空白 token 序列（只改变空白），序号在格式化前后
/// 保持一致，供 [`reverse_transpile`] 对格式化后的文本精确删除前缀。
pub fn added_crate_token_indices(
    en_content: &str,
    pipeline_map: &[SourceMapEntry],
) -> HashSet<usize> {
    // 1. 英文输出偏移 = 母语源偏移 + 之前全部条目的净长度增量
    //（编辑地图以母语源偏移升序记录且条目互不重叠，线性累积即可）
    let mut en_offsets = HashSet::new();
    let mut delta = 0i64;
    for e in pipeline_map {
        if e.replacement.starts_with("crate::") {
            en_offsets.insert((e.source_offset as i64 + delta) as usize);
        }
        delta += e.replacement.len() as i64 - e.length as i64;
    }
    if en_offsets.is_empty() {
        return HashSet::new();
    }
    // 2. 偏移 → 非空白 token 序号（与 [`reverse_transpile`] 计数口径一致）
    let mut result = HashSet::new();
    let mut offset = 0usize;
    let mut nonspace = 0usize;
    for token in tokenize(en_content) {
        if !is_whitespace(token.kind) {
            if en_offsets.contains(&offset) {
                result.insert(nonspace);
            }
            nonspace += 1;
        }
        offset += token.len;
    }
    result
}

/// `crate::模块名` 前缀成立时，返回 `crate` 之后需跳过的 token 数（空白 + 两个冒号）；
/// 不成立（冒号不足/后非已知模块名）返回 None
fn crate_prefix_skip_count(
    token_stream: &[(TokenKind, &str)],
    current: usize,
    module_names: &HashSet<String>,
) -> Option<usize> {
    let mut idx = current + 1;
    let mut skip = 0usize;
    let mut colons = 0usize;
    while idx < token_stream.len() {
        let (kind, text) = token_stream[idx];
        if is_whitespace(kind) {
            skip += 1;
            idx += 1;
            continue;
        }
        if colons < 2 && matches!(kind, TokenKind::Colon) {
            colons += 1;
            skip += 1;
            idx += 1;
            continue;
        }
        // 两个冒号后的第一个非空白 token 必须是已知模块名才视为代理插入的前缀
        if colons == 2 && matches!(kind, TokenKind::Ident) && module_names.contains(text) {
            return Some(skip);
        }
        return None;
    }
    None
}

/// `macro_rules` 后跟感叹号（可隔空白）时，返回需跳过的 token 数（空白 + 感叹号）
fn bang_suffix_skip_count(token_stream: &[(TokenKind, &str)], current: usize) -> Option<usize> {
    let mut skip = 0usize;
    for (kind, _) in &token_stream[current + 1..] {
        if is_whitespace(*kind) {
            skip += 1;
            continue;
        }
        return if matches!(kind, TokenKind::Not) {
            Some(skip + 1)
        } else {
            None
        };
    }
    None
}

/// 判断 token 是否为空白
fn is_whitespace(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
    )
}

/// 判断 token 是否为开括号（( [ {）
fn is_open_bracket(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::OpenParen | TokenKind::OpenBracket | TokenKind::OpenBrace
    )
}

/// 检查指定位置之前的非空白 token 是否构成 ::（双冒号）
fn is_preceded_by_double_colon(token_stream: &[rustc_lexer::Token], current_pos: usize) -> bool {
    // 从当前位置往前找，跳过空白，找前两个非空白 token
    let mut prev_one = None;
    let mut prev_two = None;

    let mut j = current_pos;
    while j > 0 {
        j -= 1;
        if !is_whitespace(token_stream[j].kind) {
            if prev_one.is_none() {
                prev_one = Some(token_stream[j].kind);
            } else if prev_two.is_none() {
                prev_two = Some(token_stream[j].kind);
                break;
            }
        }
    }

    // 前一个和前两个都是冒号 → 前面是 ::
    matches!(prev_one, Some(TokenKind::Colon)) && matches!(prev_two, Some(TokenKind::Colon))
}

/// 判断指定位置的标识符是否为属性名（紧跟在 `#[` 或 `#![` 之后的标识符）
///
/// 属性名不应触发宏感叹号自动补充：`#[配置(测试)]` 中的 `配置` 是属性
/// （对应 `cfg`），而非宏调用，若误补 `!` 会生成非法的 `#[cfg!(test)]`。
fn is_attribute_name(token_stream: &[rustc_lexer::Token], current_pos: usize) -> bool {
    // 往前收集最多三个非空白 token（最近的在前）
    let mut prev: Vec<TokenKind> = Vec::new();
    let mut j = current_pos;
    while j > 0 && prev.len() < 3 {
        j -= 1;
        if !is_whitespace(token_stream[j].kind) {
            prev.push(token_stream[j].kind);
        }
    }

    // `#[标识符`：最近的 token 是 `[`，再前一个是 `#`
    // `#![标识符`：最近的 token 是 `[`，再前两个是 `!` 与 `#`
    matches!(
        prev.as_slice(),
        [TokenKind::OpenBracket, TokenKind::Pound, ..]
    ) || matches!(
        prev.as_slice(),
        [TokenKind::OpenBracket, TokenKind::Not, TokenKind::Pound]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::collections::HashSet;

    fn create_test_map() -> HashMap<String, String> {
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

    fn create_macro_map() -> HashMap<String, String> {
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
    fn test_simple_replacement() {
        let map = create_test_map();
        let source = "让 可变 x = 5;";
        let expected = "let mut x = 5;";
        assert_eq!(transpile_source(source, &map), expected);
    }

    #[test]
    fn test_non_mapped_identifiers_preserved() {
        let map = create_test_map();
        let source = "让 变量名 = 42;";
        let expected = "let 变量名 = 42;";
        assert_eq!(transpile_source(source, &map), expected);
    }

    #[test]
    fn test_comments_and_strings_preserved() {
        let map = create_test_map();
        let source = "// 这是注释 函数\n让 s = \"这是字符串 函数\";";
        let expected = "// 这是注释 函数\nlet s = \"这是字符串 函数\";";
        assert_eq!(transpile_source(source, &map), expected);
    }

    #[test]
    fn test_raw_identifier_handling() {
        // 原始标识符用于保留关键字（如 match 是 Rust 保留字）
        let mut map = create_test_map();
        map.insert("匹配".to_string(), "match".to_string());
        let source = "让 r#匹配 = 1;";
        let expected = "let r#match = 1;";
        assert_eq!(transpile_source(source, &map), expected);
    }

    #[test]
    fn test_non_mapped_identifiers_kept() {
        let map = create_test_map();
        let source = "函数 主函数() { }";
        let expected = "fn 主函数() { }";
        assert_eq!(transpile_source(source, &map), expected);
    }

    /// 标识符事实缓存按名字直映：槽位冲突时两个名字必须各得其事实
    ///
    /// 槽内保存名字全文并逐字节比较；若误按槽位直接取用，后写入的未映射
    /// 名字会拿到前一个已映射名字的译文（或反之）。
    #[test]
    fn test_ident_cache_collision_keeps_both_names() {
        let mut map = create_test_map();
        map.insert("甲".to_string(), "one".to_string());
        let want = crate::name_cache::name_cache_slot("甲");
        let collide = (0..5000)
            .map(|i| format!("甲{i}"))
            .find(|name| crate::name_cache::name_cache_slot(name) == want)
            .expect("应能找到同槽位的另一个名字");
        let source = format!("让 甲 = 1;\n让 {collide} = 2;\n");
        let expected = format!("let one = 1;\nlet {collide} = 2;\n");
        assert_eq!(transpile_source(&source, &map), expected);
    }

    /// 名字数远超缓存槽数（必然反复冲突覆盖），逐个仍按自己的事实转译
    #[test]
    fn test_ident_cache_many_distinct_names() {
        let map = create_test_map();
        let mut source = String::from("函数 主函数() {\n");
        let mut expected = String::from("fn 主函数() {\n");
        for i in 0..600 {
            source.push_str(&format!("    让 变量_{i} = {i};\n"));
            expected.push_str(&format!("    let 变量_{i} = {i};\n"));
        }
        // 重复引用 + 关键字词交替出现：命中与未命中混在一起
        source.push_str(
            "    让 结果 = 变量_5 + 变量_257;\n    如果 结果 > 0 { 打印行!(\"正\"); }\n}\n",
        );
        expected.push_str(
            "    let 结果 = 变量_5 + 变量_257;\n    if 结果 > 0 { println!(\"正\"); }\n}\n",
        );
        let macros = create_macro_map();
        assert_eq!(
            transpile_source_with_macro_map(&source, &map, &macros, &HashMap::new()),
            expected
        );
    }

    // ===== 宏感叹号自动补充测试 =====

    #[test]
    fn test_macro_auto_exclamation() {
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "打印行(\"你好\")";
        let expected = "println!(\"你好\")";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros, &HashMap::new()),
            expected
        );
    }

    #[test]
    fn test_macro_existing_exclamation_not_duplicated() {
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "打印行!(\"你好\")";
        let expected = "println!(\"你好\")";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros, &HashMap::new()),
            expected
        );
    }

    #[test]
    fn test_full_program_macro_auto_exclamation() {
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "函数 主函数() { 打印行(\"你好\") }";
        // 注意：主函数 不在映射中，所以保持原样
        // 但 函数 → fn，打印行 → println!
        let actual = transpile_source_with_macro_map(source, &map, &macros, &HashMap::new());
        assert!(actual.contains("fn"));
        assert!(actual.contains("println!(\"你好\")"));
    }

    #[test]
    fn test_regular_function_call_not_exclamation() {
        let map = create_test_map();
        let macros = create_macro_map();
        // "从" 不在宏集合中，不应被加 !
        let source = "字符串::从(\"x\")";
        let expected = "字符串::从(\"x\")";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros, &HashMap::new()),
            expected
        );
    }

    #[test]
    fn test_attribute_name_no_exclamation() {
        let mut map = create_test_map();
        let macros = create_macro_map();
        // 属性上下文中的宏名（如 配置→cfg、断言）不应被补 !
        map.insert("配置".to_string(), "cfg".to_string());
        let source = "#[配置(测试)]\n#[断言]";
        let expected = "#[cfg(测试)]\n#[assert]";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros, &HashMap::new()),
            expected
        );
    }

    #[test]
    fn test_inner_attribute_name_no_exclamation() {
        let mut map = create_test_map();
        let macros = create_macro_map();
        map.insert("配置".to_string(), "cfg".to_string());
        // 内部属性 #![...] 同样不应补 !
        let source = "#![配置(测试)]";
        let expected = "#![cfg(测试)]";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros, &HashMap::new()),
            expected
        );
    }

    #[test]
    fn test_macro_word_at_method_position_not_exclamation() {
        // 方法调用位的宏词不补 `!`：`.格式化(` → `.format(`（方法调用）
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "记录器.格式化(\"{}\", 值);";
        let expected = "记录器.format(\"{}\", 值);";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros, &HashMap::new()),
            expected
        );
    }

    #[test]
    fn test_macro_word_after_dot_with_bang_kept() {
        // 方法位显式写了 `!`：保持用户书写形式（不按宏补逻辑处理）
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "记录器.打印行!(\"你好\");";
        let expected = "记录器.println!(\"你好\");";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros, &HashMap::new()),
            expected
        );
    }

    #[test]
    fn test_macro_in_normal_code_still_exclamation() {
        let map = create_test_map();
        let macros = create_macro_map();
        // 普通代码中的宏调用不受影响
        let source = "断言(5 > 3)";
        let expected = "assert!(5 > 3)";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros, &HashMap::new()),
            expected
        );
    }

    #[test]
    fn test_macro_after_double_colon_no_exclamation() {
        let map = create_test_map();
        let macros = create_macro_map();
        // 打印行 在宏集合中，但前面是 ::，不应加 !
        let source = "std::打印行(\"你好\")";
        let expected = "std::println(\"你好\")";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros, &HashMap::new()),
            expected
        );
    }

    #[test]
    fn test_macro_followed_by_brackets() {
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "向量![1, 2, 3]";
        let expected = "vec![1, 2, 3]";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros, &HashMap::new()),
            expected
        );
    }

    #[test]
    fn test_macro_word_at_declaration_position_not_exclamation() {
        // 声明位豁免：`模块 测试 {` 的 `测试` 是模块名（恰为宏名 test），
        // 不得补 `!` 产出非法的 `mod test! {`；`函数 测试(` 同理。
        let mut map = create_test_map();
        map.insert("模块".to_string(), "mod".to_string());
        map.insert("测试".to_string(), "test".to_string());
        let mut macros = create_macro_map();
        macros.insert("测试".to_string(), "test".to_string());

        let source = "模块 测试 {\n    函数 f() {}\n}";
        let expected = "mod test {\n    fn f() {}\n}";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros, &HashMap::new()),
            expected
        );

        // 非声明位的同名宏调用仍照常补 `!`（不误伤）
        assert_eq!(
            transpile_source_with_macro_map("让 x = 测试();", &map, &macros, &HashMap::new()),
            "let x = test!();"
        );
    }

    #[test]
    fn test_declared_macro_named_function_call_not_exclamation() {
        // 用户定义与宏同名的函数后，调用位不得补 `!`：
        // `测试(1)` 是函数调用（→ `test(1)`），不是宏调用（`test!(1)` 不存在）
        let mut map = create_test_map();
        map.insert("测试".to_string(), "test".to_string());
        let mut macros = create_macro_map();
        macros.insert("测试".to_string(), "test".to_string());

        let source = "函数 测试() {}\n\n函数 主函数() {\n    测试(1);\n}";
        let out = transpile_source_with_macro_map(source, &map, &macros, &HashMap::new());
        assert!(out.contains("fn test()"), "{out}");
        assert!(out.contains("test(1);"), "{out}");
        assert!(!out.contains("test!(1)"), "声明后的调用不应补 `!`：{out}");

        // 未声明同名函数时仍按宏处理（不误伤真宏调用）
        let other =
            transpile_source_with_macro_map("让 x = 测试(1);", &map, &macros, &HashMap::new());
        assert!(other.contains("test!(1)"), "{other}");
    }

    #[test]
    fn test_raw_identifier_macro_word_call_keeps_bare_parens() {
        // `r#匹配` 转义声明后，调用位 `r#匹配(3)` 也不得补 `!`
        // （否则产出 `r#match!(3)`）：撞名的兜底写法规那么依赖本行为
        let mut map = create_test_map();
        map.insert("匹配".to_string(), "match".to_string());
        let mut macros = create_macro_map();
        macros.insert("匹配".to_string(), "match".to_string());

        let source = "函数 r#匹配(值: 整数) -> 整数 {\n    返回 值\n}";
        let out = transpile_source_with_macro_map(source, &map, &macros, &HashMap::new());
        assert!(out.contains("fn r#match("), "{out}");

        // 同文件内的转义调用
        let call = "函数 主函数() {\n    r#匹配(3);\n}";
        let out = transpile_source_with_macro_map(call, &map, &macros, &HashMap::new());
        assert!(out.contains("r#match(3);"), "{out}");
        assert!(!out.contains("r#match!("), "转义调用不应补 `!`：{out}");
    }

    #[test]
    fn test_macro_followed_by_brackets_no_exclamation() {
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "向量[1, 2, 3]";
        let expected = "vec![1, 2, 3]";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros, &HashMap::new()),
            expected
        );
    }

    #[test]
    fn test_macro_map_overrides_type_value() {
        // 模拟真实语言包：宏名同时出现在类型节与宏节（如 向量→Vec 与 向量→vec），
        // keyword_map 被类型节覆盖为 Vec，宏映射应保证宏调用输出 vec!
        let mut map = create_test_map();
        map.insert("向量".to_string(), "Vec".to_string());
        let macros = create_macro_map();
        let source = "让 v = 向量![1, 2, 3];";
        let expected = "let v = vec![1, 2, 3];";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros, &HashMap::new()),
            expected
        );
    }

    #[test]
    fn test_derive_params_use_derive_map() {
        // 派生属性内的特征名走派生特征映射（`克隆` → `Clone` 大写）
        let mut map = create_test_map();
        map.insert("派生".to_string(), "derive".to_string());
        let mut derive = HashMap::new();
        derive.insert("克隆".to_string(), "Clone".to_string());
        derive.insert("调试".to_string(), "Debug".to_string());
        let empty = HashMap::new();
        let source = "#[派生(克隆, 调试)]\n结构体 点 {";
        let expected = "#[derive(Clone, Debug)]\n结构体 点 {";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &empty, &derive),
            expected
        );
        // 方法调用 `值.克隆()` 不受派生映射影响（无上下文匹配，走关键字表）
        let source2 = "值.克隆();";
        assert_eq!(
            transpile_source_with_macro_map(source2, &map, &empty, &derive),
            "值.克隆();"
        );
    }

    #[test]
    fn test_derive_params_with_keyword_replacement() {
        // 派生属性本身的关键字（派生→derive）与参数（克隆→Clone）同时替换
        let mut map = create_test_map();
        map.insert("派生".to_string(), "derive".to_string());
        let mut derive = HashMap::new();
        derive.insert("克隆".to_string(), "Clone".to_string());
        let empty = HashMap::new();
        let source = "#[派生(克隆)]";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &empty, &derive),
            "#[derive(Clone)]"
        );
    }

    #[test]
    fn test_empty_macro_set_no_exclamation() {
        let map = create_test_map();
        let empty = HashMap::new();
        let source = "打印行(\"你好\")";
        let expected = "println(\"你好\")"; // 不补 !
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &empty, &HashMap::new()),
            expected
        );
    }

    #[test]
    fn test_multiple_macro_calls() {
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "打印行(\"甲\"); 打印(\"乙\")";
        let expected = "println!(\"甲\"); print!(\"乙\")";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &macros, &HashMap::new()),
            expected
        );
    }

    #[test]
    fn test_macro_rules_declaration_gets_exclamation() {
        // `宏规则` 声明形式：后跟宏名时自动补 `!`（macro_rules! 名称）
        let mut map = create_test_map();
        map.insert("宏规则".to_string(), "macro_rules".to_string());
        let empty = HashMap::new();
        let source = "宏规则 创建向量 { () => { } }";
        let expected = "macro_rules! 创建向量 { () => { } }";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &empty, &HashMap::new()),
            expected
        );
    }

    #[test]
    fn test_macro_rules_keyword_alone_no_exclamation() {
        // 宏规则后不跟宏名（如注释性用法）时不补 `!`
        let mut map = create_test_map();
        map.insert("宏规则".to_string(), "macro_rules".to_string());
        let empty = HashMap::new();
        let source = "让 x = 宏规则;";
        let expected = "let x = macro_rules;";
        assert_eq!(
            transpile_source_with_macro_map(source, &map, &empty, &HashMap::new()),
            expected
        );
    }

    // ===== 源映射测试 =====

    #[test]
    fn test_transpile_with_map_output_consistent() {
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "函数 主函数() { 让 x = 5; 打印行(\"你好\") }";
        let result = transpile_with_map(
            source,
            &map,
            &macros,
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashMap::new(),
        );
        assert_eq!(
            result.output,
            transpile_source_with_macro_map(source, &map, &macros, &HashMap::new())
        );
    }

    #[test]
    fn test_transpile_with_map_records_keyword_replacements() {
        let map = create_test_map();
        let macros = create_macro_map();
        let source = "函数 主函数() { 让 x = 5; 打印行(\"你好\") }";
        let result = transpile_with_map(
            source,
            &map,
            &macros,
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashMap::new(),
        );

        // 函数/让/打印行 被替换，主函数 未命中映射不记录
        let fn_entry = result
            .source_map
            .iter()
            .find(|m| m.original == "函数")
            .expect("应有 函数 映射");
        assert_eq!(fn_entry.replacement, "fn");
        assert_eq!(
            &source[fn_entry.source_offset..fn_entry.source_offset + fn_entry.length],
            "函数"
        );

        let let_entry = result
            .source_map
            .iter()
            .find(|m| m.original == "让")
            .expect("应有 让 映射");
        assert_eq!(let_entry.replacement, "let");
        assert_eq!(
            &source[let_entry.source_offset..let_entry.source_offset + let_entry.length],
            "让"
        );

        // 宏名映射记录翻译文本（println），感叹号补充不产生额外条目
        let println_entry = result
            .source_map
            .iter()
            .find(|m| m.original == "打印行")
            .expect("应有 打印行 映射");
        assert_eq!(println_entry.replacement, "println");

        assert!(!result.source_map.iter().any(|m| m.original == "主函数"));
    }

    #[test]
    fn test_transpile_with_map_raw_identifier() {
        let mut map = create_test_map();
        map.insert("匹配".to_string(), "match".to_string());
        let empty = HashMap::new();
        let source = "让 r#匹配 = 1;";
        let result = transpile_with_map(
            source,
            &map,
            &empty,
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashMap::new(),
        );
        assert_eq!(result.output, "let r#match = 1;");
        let entry = result
            .source_map
            .iter()
            .find(|m| m.original == "r#匹配")
            .expect("应有原始标识符映射");
        assert_eq!(entry.replacement, "r#match");
    }

    /// 编辑表投影成（原文，替换值）序列，便于逐条比两张表的内容
    fn table_pairs(entries: &[SourceMapEntry]) -> Vec<(&str, &str)> {
        entries
            .iter()
            .map(|e| (e.original.as_str(), e.replacement.as_str()))
            .collect()
    }

    /// 只给关键字表与宏表的最小入参转译（其余表为空，不影响这里的形态）
    fn transpile_two_maps(
        source: &str,
        keyword_map: &HashMap<String, String>,
        macro_map: &HashMap<String, String>,
    ) -> TranspileResult {
        transpile_with_map(
            source,
            keyword_map,
            macro_map,
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashMap::new(),
        )
    }

    /// 两张表的等价关系（回归锁，口径见 [`build_lex_source_map`]）
    ///
    /// 主循环只收全管线表，词法表由它复制、少数条目按补 `!` 规则改写或删除。
    /// 这里钉住两个后果：① 全管线表回放到源码必须逐字等于输出——漏收条目、
    /// 替换值写错都会先在这里炸；② 两张表逐条同偏移同原文，替换值只可能差一个
    /// 自动补的 `!`，且不进词法表的条目必须正是「原文 + `!`」那一种。
    fn assert_tables_in_lockstep(source: &str, result: &TranspileResult) {
        let mut replay = String::new();
        let mut copied = 0usize;
        for edit in &result.final_edits {
            assert!(
                edit.source_offset >= copied,
                "全管线表必须按偏移升序且不重叠：{edit:?}"
            );
            replay.push_str(&source[copied..edit.source_offset]);
            replay.push_str(edit.replacement.as_str());
            copied = edit.source_offset + edit.length;
        }
        replay.push_str(&source[copied..]);
        assert_eq!(replay, result.output, "全管线表回放必须逐字等于输出");

        let mut lex = result.source_map.iter();
        let mut pending = lex.next();
        for edit in &result.final_edits {
            if pending.is_some_and(|l| l.source_offset == edit.source_offset) {
                let entry = pending.take().expect("上面 is_some 判定的就是这个条目");
                assert_eq!(
                    (entry.length, entry.original.as_str()),
                    (edit.length, edit.original.as_str()),
                    "偏移 {} 的两表条目必须同原文同长度",
                    edit.source_offset
                );
                let diff = edit
                    .replacement
                    .as_str()
                    .strip_prefix(entry.replacement.as_str());
                assert!(
                    diff == Some("") || diff == Some("!"),
                    "偏移 {}：词法表 {:?} 与全管线表 {:?} 只能差一个补的 `!`",
                    edit.source_offset,
                    entry.replacement,
                    edit.replacement
                );
                pending = lex.next();
            } else {
                let original = &source[edit.source_offset..edit.source_offset + edit.length];
                assert_eq!(
                    edit.replacement.as_str(),
                    format!("{original}!"),
                    "不进词法表的条目只能是「原文只补 `!`」"
                );
            }
        }
        assert!(pending.is_none(), "词法表不能有全管线表之外的条目");
    }

    /// 各形态下两条表各自的内容：宏调用补的 `!` 只进全管线表，其余同值
    #[test]
    fn test_lex_and_final_tables_per_shape() {
        let mut keywords = create_test_map();
        keywords.insert("无符号微整数".to_string(), "u8".to_string());
        keywords.insert("宏规则".to_string(), "macro_rules".to_string());
        keywords.insert("匹配".to_string(), "match".to_string());
        let source = "函数 示例() {\n    打印行(\"补叹号\");\n    打印行!(\"已带叹号\");\n    让 值 = 0无符号微整数;\n    让 r#匹配 = 1;\n    宏规则 建向量 { () => {} }\n}\n";
        let result = transpile_two_maps(source, &keywords, &create_macro_map());
        assert_eq!(
            result.output,
            "fn 示例() {\n    println!(\"补叹号\");\n    println!(\"已带叹号\");\n    let 值 = 0u8;\n    let r#match = 1;\n    macro_rules! 建向量 { () => {} }\n}\n"
        );
        // 词法表记本阶段产物：两条宏调用都是 println（第一条的 `!` 由本阶段
        // 补在输出里，不进表），`宏规则` 的 `!` 是替换值自带、照常进表
        assert_eq!(
            table_pairs(&result.source_map),
            [
                ("函数", "fn"),
                ("打印行", "println"),
                ("打印行", "println"),
                ("让", "let"),
                ("无符号微整数", "u8"),
                ("让", "let"),
                ("r#匹配", "r#match"),
                ("宏规则", "macro_rules!"),
            ]
        );
        // 全管线表记输出原文：第一条补了 `!`，第二条源码自带 `!` 不重复补
        assert_eq!(
            table_pairs(&result.final_edits),
            [
                ("函数", "fn"),
                ("打印行", "println!"),
                ("打印行", "println"),
                ("让", "let"),
                ("无符号微整数", "u8"),
                ("让", "let"),
                ("r#匹配", "r#match"),
                ("宏规则", "macro_rules!"),
            ]
        );
        assert_tables_in_lockstep(source, &result);
    }

    /// 宏名译文与母语词形相同（两表差别只有补的 `!`）：词法表不记该条
    ///
    /// 这是「词法阶段没改动、输出却多了 `!`」的那一侧边界，对应
    /// `lex_overrides` 里的 `None` 分支。
    #[test]
    fn test_lex_table_skips_bang_only_edit() {
        let keywords = HashMap::from([("打印行".to_string(), "打印行".to_string())]);
        let macros = HashMap::from([("打印行".to_string(), "打印行".to_string())]);
        let source = "打印行(\"甲\");\n";
        let result = transpile_two_maps(source, &keywords, &macros);
        assert_eq!(result.output, "打印行!(\"甲\");\n");
        assert!(result.source_map.is_empty(), "只补 `!` 的条目不进词法表");
        assert_eq!(table_pairs(&result.final_edits), [("打印行", "打印行!")]);
        assert_tables_in_lockstep(source, &result);
    }

    // ===== 反向转译测试 =====

    fn create_reverse_map(forward: &HashMap<String, String>) -> HashMap<String, String> {
        forward
            .iter()
            .map(|(k, v)| (v.clone(), k.clone()))
            .collect()
    }

    #[test]
    fn test_reverse_transpile_basic() {
        // 关键字、类型名、宏名均被还原为母语，宏感叹号保留
        let forward = HashMap::from([
            ("函数".to_string(), "fn".to_string()),
            ("主函数".to_string(), "main".to_string()),
            ("让".to_string(), "let".to_string()),
            ("可变".to_string(), "mut".to_string()),
            ("整数".to_string(), "i32".to_string()),
            ("打印行".to_string(), "println".to_string()),
        ]);
        let reverse = create_reverse_map(&forward);
        let empty = HashSet::new();
        let source = "fn main() { let mut x: i32 = 5; println!(\"你好\"); }";
        let expected = "函数 主函数() { 让 可变 x: 整数 = 5; 打印行!(\"你好\"); }";
        assert_eq!(
            reverse_transpile(source, &reverse, &empty, &HashSet::new()),
            expected
        );
    }

    #[test]
    fn test_reverse_transpile_preserves_comments_strings_custom_idents() {
        // 注释、字符串字面量、中文/英文自定义标识符均保持原样
        let reverse = HashMap::from([
            ("fn".to_string(), "函数".to_string()),
            ("let".to_string(), "让".to_string()),
        ]);
        let empty = HashSet::new();
        let source = "// fn 是关键字\nlet s = \"let\";\nlet 计数 = fn_value;";
        let expected = "// fn 是关键字\n让 s = \"let\";\n让 计数 = fn_value;";
        assert_eq!(
            reverse_transpile(source, &reverse, &empty, &HashSet::new()),
            expected
        );
    }

    #[test]
    fn test_reverse_transpile_longer_word_priority() {
        // token 级匹配：i32 与 i3x 是完整 token，互不干扰（无子串误替换）
        let reverse = HashMap::from([
            ("i32".to_string(), "整数".to_string()),
            ("i3".to_string(), "三".to_string()),
        ]);
        let empty = HashSet::new();
        let source = "let a: i32 = 1; let b: i3x = 2; let c = i3;";
        // let 不在反向表中保持原样；i32→整数、i3→三，i3x 是完整 token 不受影响
        let expected = "let a: 整数 = 1; let b: i3x = 2; let c = 三;";
        assert_eq!(
            reverse_transpile(source, &reverse, &empty, &HashSet::new()),
            expected
        );
    }

    #[test]
    fn test_reverse_transpile_macro_rules_declaration() {
        // `macro_rules! 名称` 反向转译为 `宏规则 名称`（感叹号省略）
        let forward = HashMap::from([
            ("宏规则".to_string(), "macro_rules".to_string()),
            ("函数".to_string(), "fn".to_string()),
        ]);
        let reverse = create_reverse_map(&forward);
        let empty = HashSet::new();
        let source = "macro_rules! 创建向量 { () => { } }";
        let expected = "宏规则 创建向量 { () => { } }";
        assert_eq!(
            reverse_transpile(source, &reverse, &empty, &HashSet::new()),
            expected
        );
    }

    #[test]
    fn test_reverse_transpile_macro_rules_with_bang_string() {
        // 宏调用 `名称!(...)` 的感叹号属于调用方，反向转译时保留
        let forward = HashMap::from([
            ("打印行".to_string(), "println".to_string()),
            ("宏规则".to_string(), "macro_rules".to_string()),
            ("函数".to_string(), "fn".to_string()),
        ]);
        let reverse = create_reverse_map(&forward);
        let empty = HashSet::new();
        let source = "macro_rules! 创建向量 { () => { } }\nfn main() { 创建向量!() }";
        let expected = "宏规则 创建向量 { () => { } }\n函数 main() { 创建向量!() }";
        assert_eq!(
            reverse_transpile(source, &reverse, &empty, &HashSet::new()),
            expected
        );
    }

    #[test]
    fn test_reverse_transpile_module_prefix() {
        let forward = HashMap::from([
            ("函数".to_string(), "fn".to_string()),
            ("包".to_string(), "crate".to_string()),
        ]);
        let reverse = create_reverse_map(&forward);
        let set = HashSet::from(["辅助".to_string()]);
        // 序号命中正向记录（代理添加的 crate::）：整体删除，还原为裸路径
        let added = HashSet::from([5usize]);
        assert_eq!(
            reverse_transpile(
                "fn main() { crate::辅助::辅助函数(); }",
                &reverse,
                &set,
                &added
            ),
            "函数 main() { 辅助::辅助函数(); }"
        );
        // 序号未命中（用户手写的 crate::，非代理添加）：前缀保留不误删
        assert_eq!(
            reverse_transpile(
                "fn main() { crate::辅助::辅助函数(); }",
                &reverse,
                &set,
                &HashSet::new()
            ),
            "函数 main() { 包::辅助::辅助函数(); }"
        );
        // 后跟非模块名时还原为 包::（不受序号影响）
        assert_eq!(
            reverse_transpile("fn main() { crate::外部函数(); }", &reverse, &set, &added),
            "函数 main() { 包::外部函数(); }"
        );
    }

    #[test]
    fn test_reverse_transpile_raw_identifier() {
        let reverse = HashMap::from([
            ("let".to_string(), "让".to_string()),
            ("match".to_string(), "匹配".to_string()),
        ]);
        let empty = HashSet::new();
        assert_eq!(
            reverse_transpile("let r#match = 1;", &reverse, &empty, &HashSet::new()),
            "让 r#匹配 = 1;"
        );
    }

    #[test]
    fn test_reverse_transpile_crate_prefix_with_whitespace() {
        // crate 与 :: 之间存在空白时前缀仍应被完整删除（不能残留孤立冒号）
        let reverse = HashMap::from([("fn".to_string(), "函数".to_string())]);
        let set = HashSet::from(["辅助".to_string()]);
        let added = HashSet::from([5usize]);
        assert_eq!(
            reverse_transpile(
                "fn f() { crate :: 辅助::辅助函数(); }",
                &reverse,
                &set,
                &added
            ),
            "函数 f() { 辅助::辅助函数(); }"
        );
    }

    #[test]
    fn test_reverse_transpile_macro_rules_with_whitespace_bang() {
        // macro_rules 与 ! 之间存在空白时感叹号仍应被省略
        let reverse = HashMap::from([("macro_rules".to_string(), "宏规则".to_string())]);
        let empty = HashSet::new();
        assert_eq!(
            reverse_transpile(
                "macro_rules ! 创建向量 { () => { } }",
                &reverse,
                &empty,
                &HashSet::new()
            ),
            "宏规则 创建向量 { () => { } }"
        );
    }

    /// use 段让位：`文件` 同时在宏表（file）与让位集合（标准库 File）中时，
    /// use 段内跳过词法替换（交后续模块路径/别名阶段处理）；
    /// 普通代码位置的 `文件!` 宏调用照常替换
    #[test]
    fn test_use_stmt_defers_conflicting_word() {
        let map = HashMap::from([
            ("使用".to_string(), "use".to_string()),
            ("文件".to_string(), "file".to_string()),
        ]);
        let defer = HashSet::from(["文件".to_string()]);
        let out = transpile_with_map(
            "使用 标准库::文件系统::文件 as 库文件;\n文件!()",
            &map,
            &HashMap::new(),
            &HashMap::new(),
            &defer,
            &HashSet::new(),
            &HashMap::new(),
        )
        .output;
        assert_eq!(out, "use 标准库::文件系统::文件 as 库文件;\nfile!()");
        // 对照组：无让位集合时 use 段内照旧替换（旧行为，验证让位集合确为开关）
        let out_no_defer = transpile_with_map(
            "使用 标准库::文件系统::文件 as 库文件;",
            &map,
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashMap::new(),
        )
        .output;
        assert_eq!(out_no_defer, "use 标准库::文件系统::file as 库文件;");
    }

    /// use 段让位（本地化 use 词形）：非 `use`/`使用` 词形的语言（如 es `usar`）
    /// 也需开启让位态——词形经关键字映射反查（历史硬编码只识别 `use`/`使用`，
    /// es 等语言 use 段的冲突词被词法替换后残留 `format::`，模块路径阶段无法修复）
    #[test]
    fn test_use_stmt_defers_with_localized_use_keyword() {
        let map = HashMap::from([
            ("usar".to_string(), "use".to_string()),
            ("formato".to_string(), "format".to_string()),
        ]);
        let defer = HashSet::from(["formato".to_string()]);
        let out = transpile_with_map(
            "usar estandar::formato::Mostrar;",
            &map,
            &HashMap::new(),
            &HashMap::new(),
            &defer,
            &HashSet::new(),
            &HashMap::new(),
        )
        .output;
        // `usar` 照常译为 use；`formato` 让位保留（交模块路径阶段替换为 fmt）
        assert_eq!(out, "use estandar::formato::Mostrar;");
    }

    /// 方法位判定跨空白与注释：`.` 与名字之间夹什么都是方法位
    ///
    /// 方法位改用「上一个有意义 token 是否为 `.`」的状态变量（早先对每个
    /// 标识符回扫 token 流），空白与注释不重置该状态正是两条实现等价的关键
    /// 边界；同时确认非方法位照常替换，状态没有被粘住。
    #[test]
    fn test_method_pos_defers_across_trivia() {
        let map = HashMap::from([("枚举".to_string(), "enum".to_string())]);
        let method_defer = HashSet::from(["枚举".to_string()]);
        for src in [
            "列.枚举();",
            "列 . 枚举();",
            "列.\n    枚举();",
            "列. /* c */ 枚举();",
            "列. // c\n    枚举();",
        ] {
            let out = transpile_with_map(
                src,
                &map,
                &HashMap::new(),
                &HashMap::new(),
                &HashSet::new(),
                &method_defer,
                &HashMap::new(),
            )
            .output;
            assert_eq!(out, src, "方法位让位失效：{src:?}");
        }
        // 对照：不在 `.` 之后的同名标识符照常替换
        let out = transpile_with_map(
            "枚举();",
            &map,
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &method_defer,
            &HashMap::new(),
        )
        .output;
        assert_eq!(out, "enum();");
    }

    /// 方法位让位：词法替换值是保留关键字的词（`枚举`→enum）在 `.` 后
    /// 原样保留（交别名阶段替成 enumerate）；未命中让位集合时照旧替换
    #[test]
    fn test_method_pos_defers_conflicting_word() {
        let map = HashMap::from([
            ("让".to_string(), "let".to_string()),
            ("迭代".to_string(), "iter".to_string()),
            ("枚举".to_string(), "enum".to_string()),
        ]);
        let method_defer = HashSet::from(["枚举".to_string()]);
        let out = transpile_with_map(
            "让 n = 列.迭代().枚举();",
            &map,
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &method_defer,
            &HashMap::new(),
        )
        .output;
        assert_eq!(out, "let n = 列.iter().枚举();");
        // 对照组：无让位集合时方法位照旧替换（旧行为，验证让位集合确为开关）
        let out_no_defer = transpile_with_map(
            "让 n = 列.迭代().枚举();",
            &map,
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
            &HashMap::new(),
        )
        .output;
        assert_eq!(out_no_defer, "let n = 列.iter().enum();");
    }

    /// 数字与中文类型黏连（`0无符号微整数`）：rustc_lexer 视为单个
    /// Literal token，后缀命中映射表时替换（→ `0u8`）；未命中/ASCII 后缀原样
    #[test]
    fn test_number_literal_with_chinese_suffix() {
        let map = HashMap::from([("无符号微整数".to_string(), "u8".to_string())]);
        let empty = HashMap::new();
        // 后缀命中：替换为英文数值类型，并记录编辑条目（仅后缀部分）
        let result = transpile_with_map(
            "vec![0无符号微整数; 16]",
            &map,
            &empty,
            &empty,
            &HashSet::new(),
            &HashSet::new(),
            &empty,
        );
        assert_eq!(result.output, "vec![0u8; 16]");
        assert_eq!(result.source_map.len(), 1);
        assert_eq!(result.source_map[0].original, "无符号微整数");
        assert_eq!(result.source_map[0].replacement, "u8");
        // 后缀未命中：原样保留（编译错误交给编译器）
        let out = transpile_with_map(
            "let x = 0未知词;",
            &map,
            &empty,
            &empty,
            &HashSet::new(),
            &HashSet::new(),
            &empty,
        )
        .output;
        assert_eq!(out, "let x = 0未知词;");
        // ASCII 后缀不受影响
        let out = transpile_with_map(
            "let x = 0u8;",
            &map,
            &empty,
            &empty,
            &HashSet::new(),
            &HashSet::new(),
            &empty,
        )
        .output;
        assert_eq!(out, "let x = 0u8;");
    }

    /// 标准库层数值类型词（`无符号机器整数` = usize，不在 keyword_map）
    /// 黏连数字后由兜底表拆分：`0无符号机器整数` → `0usize`；
    /// 两表同词冲突时 keyword_map 优先
    #[test]
    fn test_number_literal_suffix_falls_back_to_stdlib_map() {
        let empty = HashMap::new();
        let stdlib = HashMap::from([("无符号机器整数".to_string(), "usize".to_string())]);
        let result = transpile_with_map(
            "let x = 0无符号机器整数;",
            &empty,
            &empty,
            &empty,
            &HashSet::new(),
            &HashSet::new(),
            &stdlib,
        );
        assert_eq!(result.output, "let x = 0usize;");
        assert_eq!(result.source_map.len(), 1);
        assert_eq!(result.source_map[0].original, "无符号机器整数");
        assert_eq!(result.source_map[0].replacement, "usize");
        // 两表同词冲突：keyword_map 优先（匹配其它阶段的查找次序）
        let map = HashMap::from([("微整数".to_string(), "u8".to_string())]);
        let fallback = HashMap::from([("微整数".to_string(), "usize".to_string())]);
        let out = transpile_with_map(
            "0微整数",
            &map,
            &empty,
            &empty,
            &HashSet::new(),
            &HashSet::new(),
            &fallback,
        )
        .output;
        assert_eq!(out, "0u8");
    }
}
