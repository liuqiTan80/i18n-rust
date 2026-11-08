//! 词法差分测试（声明位启发式的 oracle）
//!
//! 把 `alias.rs`/`校验.rs` 里的"声明位保护"从经验规则升级为有 oracle 的规格：
//! 随机生成"语法良好-ish"的母语方言 token 流，正向转译后用 `rustc_lexer` 重新
//! 词法化，对 source↔output 断言四类结构不变量，并对不含模块路径的子集做反译回环。
//!
//! 断言（管线在 `module_names=None` 下运行，故 `crate::` 阶段被跳过，
//! 唯一的受控标点增量是宏自动补的 `!`）：
//! 1. token 流结构同构：标点/运算符/字面量（除 `!`）的文本序列 source==output；
//!    `!` 的净增数恰等于本用例书写时省略感叹号的宏调用数；标识符 token 数守恒。
//! 2. 空白不变量：token 间空白 run 与注释 token 在 output 中逐一原样保留（管线
//!    只做 token 级替换，不增删空白/注释）。
//! 3. 声明位保真（核心）：声明关键字后紧跟的自定义命名（取自别名/标识符词表——
//!    这些词在非声明位会被别名阶段改写）必须在 output 中逐字节原样出现，
//!    直接固化"声明位启发式"的契约。
//! 4. 幂等复核：`transpile(output) == output`（英文产物再转译不变）。
//!
//! 反译回环 oracle（独立属性）：对不含模块路径、宏一律带感叹号的生成子集，
//! `反译(output, reverse_map(kw+macro+alias+derive), ∅, ∅)` 还原 source。

use std::collections::HashMap;

use i18n_rust_engine::带映射转译管线;
use i18n_rust_engine::映射管理器::映射管理器;
use i18n_rust_engine::词法器::反译;
use proptest::prelude::*;
use rustc_lexer::{TokenKind, tokenize};

/// 别名阶段视为"声明关键字"的英文形式（见 `别名::声明关键字表`）：
/// 其后的首个标识符为用户自定义命名，须受声明位保护。
const DECL_EN: &[&str] = &[
    "fn", "struct", "enum", "trait", "type", "mod", "let", "const", "static",
];

/// 受控测试语言包：关键字/类型/宏/派生特征/模块路径/标识符别名全覆盖，
/// 且所有替换目标均为单标识符（不含 `::`/`.`/空格），便于结构同构断言。
fn test_manager() -> 映射管理器 {
    let keywords = r#"
["声明"]
"函数" = "fn"
"让" = "let"
"结构体" = "struct"
"枚举体" = "enum"
"特征" = "trait"
"常量" = "const"
"静态" = "static"
"类型别名" = "type"
"模块" = "mod"
"可变" = "mut"
"使用" = "use"
["类型"]
"整数" = "i32"
"布尔" = "bool"
["宏"]
"打印行" = "println"
"向量" = "vec"
["派生特征"]
"调试" = "Debug"
"克隆" = "Clone"
"#;
    let module_paths = r#"
["模块路径"]
"标准集合" = "std::collections"
"#;
    let stdlib = r#"
["标识符"]
"新建" = "new"
"长度" = "len"
"迭代" = "iter"
"字符串" = "String"
"获取" = "get"
"#;
    映射管理器::从内置载入(keywords, module_paths, stdlib, &[]).expect("加载受控测试语言包失败")
}

/// `en` 是否为合法的单 Rust 标识符（首字母/下划线，余字母数字/下划线）：
/// 只有这样的替换目标才不会引入额外标点，保证结构同构断言成立。
fn is_single_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 与 `reverse_map` 生成规则一致：英文 → 母语，同英文键取"排序最小"的母语词，
/// 保证反译候选确定。返回 (英文→母语表, 唯一映射的母语词集合)。
fn build_reverse_and_unique(
    maps: &[&HashMap<String, String>],
) -> (HashMap<String, String>, std::collections::HashSet<String>) {
    let mut by_en: HashMap<String, Vec<String>> = HashMap::new();
    for m in maps {
        for (native, en) in *m {
            if native == en || !is_single_ident(en) {
                continue;
            }
            by_en.entry(en.clone()).or_default().push(native.clone());
        }
    }
    let mut reverse = HashMap::new();
    let mut unique = std::collections::HashSet::new();
    for (en, mut natives) in by_en {
        natives.sort();
        natives.dedup();
        if natives.len() == 1 {
            let native = natives.into_iter().next().expect("非空");
            reverse.insert(en.clone(), native.clone());
            unique.insert(native);
        }
    }
    (reverse, unique)
}

/// 词法 token 的粗分类
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum Cat {
    Ws,
    Comment,
    Ident,
    Other,
}

fn classify(kind: TokenKind) -> Cat {
    match kind {
        TokenKind::Whitespace => Cat::Ws,
        TokenKind::LineComment | TokenKind::BlockComment { .. } => Cat::Comment,
        TokenKind::Ident | TokenKind::RawIdent => Cat::Ident,
        _ => Cat::Other,
    }
}

/// 词法化为 (分类, 文本) 序列（rustc_lexer 的标点均为单 token，`!` 归 Other）
fn lex(s: &str) -> Vec<(Cat, String)> {
    let mut out = Vec::new();
    let mut off = 0usize;
    for t in tokenize(s) {
        out.push((classify(t.kind), s[off..off + t.len].to_string()));
        off += t.len;
    }
    out
}

/// 取指定分类的文本序列
fn texts(toks: &[(Cat, String)], cat: Cat) -> Vec<String> {
    toks.iter()
        .filter(|(c, _)| *c == cat)
        .map(|(_, t)| t.clone())
        .collect()
}

/// 取"结构骨架"：Other 类 token 文本序列，剔除 `!`（宏自动补的感叹号是唯一受控增量）
fn skeleton_excl_bang(toks: &[(Cat, String)]) -> Vec<String> {
    toks.iter()
        .filter(|(c, t)| *c == Cat::Other && t != "!")
        .map(|(_, t)| t.clone())
        .collect()
}

/// 统计 `!` token 数
fn count_bang(toks: &[(Cat, String)]) -> usize {
    toks.iter()
        .filter(|(c, t)| *c == Cat::Other && t == "!")
        .count()
}

/// 在 output token 流中验证：英文声明关键字之后（跳过空白/注释）的首个标识符
/// 恰为受保护的原生命名（逐字节）。
fn decl_name_preserved(toks: &[(Cat, String)], decl_en: &str, native: &str) -> bool {
    let meaningful: Vec<&(Cat, String)> = toks
        .iter()
        .filter(|(c, _)| *c == Cat::Ident || *c == Cat::Other)
        .collect();
    for w in meaningful.windows(2) {
        if w[0].1 == decl_en && w[1].1 == native {
            return true;
        }
    }
    false
}

/// 生成用的片段指令：kind 选择构造模板，a/b 为池内索引（对池长取模）
#[derive(Clone, Copy, Debug)]
struct Frag {
    kind: u8,
    a: u32,
    b: u32,
}

/// 池：随语言包派生的原生词与固定的自定义命名/字面量/标点/注释
struct Pools {
    /// 声明关键字（原生词, 英文）
    decl: Vec<(String, String)>,
    /// 别名/标识符原生词（非声明关键字，非模块路径；用于声明位保真与方法调用）
    alias: Vec<String>,
    /// 宏原生词
    macro_words: Vec<String>,
    /// 自定义命名：不与任何映射表的原生键或英文值冲突，逐字节安全
    custom: Vec<String>,
}

const LITERALS: &[&str] = &[
    "42",
    "0",
    "3.14",
    "\"字符串内容\"",
    "\"\"",
    "'a'",
    "b'x'",
    "r#\"原始串\"#",
];
const COMMENTS: &[&str] = &["// 行注释", "/* 块注释 */", "// 另一注释"];
const PUNCTS: &[&str] = &[
    ";", ",", "{", "}", "(", ")", "[", "]", "=", "+", "->", "<", ">",
];

impl Pools {
    fn new(m: &映射管理器) -> Pools {
        let kw = m.取关键词表();
        let alias_map = m.取别名表();
        let macro_map = m.取宏表();
        let module_map = m.取模块路径表();

        // 反向唯一集：kw+macro+alias+derive（排除模块路径——其英文含 `::` 非单标识符）
        let (_rev, unique) =
            build_reverse_and_unique(&[kw, macro_map, alias_map, m.取派生特征表()]);

        // 英文值全集（用于自定义命名排雷：自定义名不得是任何表的英文值，避免反译误改）
        let mut english_values: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for tbl in [kw, macro_map, alias_map, m.取派生特征表(), module_map] {
            for en in tbl.values() {
                english_values.insert(en.clone());
            }
        }
        let native_keys: std::collections::HashSet<&String> = kw
            .keys()
            .chain(macro_map.keys())
            .chain(alias_map.keys())
            .collect();

        let decl: Vec<(String, String)> = kw
            .iter()
            .filter(|(native, en)| {
                DECL_EN.contains(&en.as_str()) && is_single_ident(en) && unique.contains(*native)
            })
            .map(|(n, e)| (n.clone(), e.clone()))
            .collect();

        // 别名词：来自标识符别名表，替换目标为单标识符且反向唯一
        let alias: Vec<String> = alias_map
            .keys()
            .filter(|native| {
                is_single_ident(&alias_map[*native])
                    && unique.contains(*native)
                    && !DECL_EN.contains(&alias_map[*native].as_str())
            })
            .cloned()
            .collect();

        let macro_words: Vec<String> = macro_map
            .keys()
            .filter(|native| is_single_ident(&macro_map[*native]) && unique.contains(*native))
            .cloned()
            .collect();

        // 固定候选自定义命名，过滤任何可能冲突者
        let custom: Vec<String> = [
            "甲",
            "乙",
            "数据",
            "项目名",
            "我的变量",
            "辅助函数",
            "绝对值",
        ]
        .iter()
        .map(|s| s.to_string())
        .filter(|c| !english_values.contains(c) && !native_keys.contains(c))
        .collect();

        assert!(
            !decl.is_empty() && !alias.is_empty() && !custom.is_empty(),
            "受控词表不完整：decl={:?} alias={:?} custom={:?}",
            decl,
            alias,
            custom
        );
        Pools {
            decl,
            alias,
            macro_words,
            custom,
        }
    }
}

/// 片段模板编号
const K_DECL: u8 = 0;
const K_CUSTOM: u8 = 1;
const K_METHOD: u8 = 2;
const K_MACRO: u8 = 3;
const K_LIT: u8 = 4;
const K_PUNCT: u8 = 5;
const K_COMMENT: u8 = 6;
const K_PATH: u8 = 7;
const K_TURBOFISH: u8 = 8;

/// 渲染片段为源码，同时收集：期望的声明位保真对 (英文声明词, 原生名) 与
/// 期望的宏补感叹号数（省略感叹号书写的宏调用数）。
fn render(
    frags: &[Frag],
    pools: &Pools,
    force_bang_macro: bool,
) -> (String, Vec<(String, String)>, usize) {
    let mut src = String::new();
    let mut preserved: Vec<(String, String)> = Vec::new();
    let mut expected_bang = 0usize;
    // 行注释会吞掉同行其后内容：若上一片段为行注释，则以换行分隔，
    // 保证后续片段仍处于代码位（否则宏 `!` 等替换不生效，破坏 oracle 前提）
    let mut prev_line_comment = false;
    for (i, f) in frags.iter().enumerate() {
        if i > 0 {
            src.push(if prev_line_comment { '\n' } else { ' ' });
        }
        prev_line_comment = false;
        match f.kind {
            K_DECL => {
                let (dnative, den) = &pools.decl[f.a as usize % pools.decl.len()];
                // 声明名取自别名表（这些词在非声明位会被别名阶段改写）或自定义命名，
                // 二者都应在声明位被逐字节保留
                let name = if f.b % 2 == 0 {
                    pools.alias[f.b as usize % pools.alias.len()].clone()
                } else {
                    pools.custom[(f.b as usize) % pools.custom.len()].clone()
                };
                preserved.push((den.clone(), name.clone()));
                src.push_str(dnative);
                src.push(' ');
                src.push_str(&name);
            }
            K_CUSTOM => {
                src.push_str(&pools.custom[f.a as usize % pools.custom.len()]);
            }
            K_METHOD => {
                src.push_str(&pools.custom[f.a as usize % pools.custom.len()]);
                src.push('.');
                src.push_str(&pools.alias[f.b as usize % pools.alias.len()]);
                src.push_str("()");
            }
            K_MACRO => {
                let m = &pools.macro_words[f.a as usize % pools.macro_words.len()];
                src.push_str(m);
                let with_bang = force_bang_macro || (f.b % 2 == 0);
                if with_bang {
                    src.push('!');
                } else {
                    expected_bang += 1;
                }
                src.push_str("(\"参数\")");
            }
            K_LIT => src.push_str(LITERALS[f.a as usize % LITERALS.len()]),
            K_PUNCT => src.push_str(PUNCTS[f.a as usize % PUNCTS.len()]),
            K_COMMENT => {
                let c = COMMENTS[f.a as usize % COMMENTS.len()];
                prev_line_comment = c.starts_with("//");
                src.push_str(c);
            }
            K_PATH => {
                src.push_str(&pools.custom[f.a as usize % pools.custom.len()]);
                src.push_str("::");
                src.push_str(&pools.custom[f.b as usize % pools.custom.len()]);
            }
            K_TURBOFISH => {
                src.push_str(&pools.custom[f.a as usize % pools.custom.len()]);
                src.push_str("::<");
                src.push_str(&pools.custom[f.b as usize % pools.custom.len()]);
                src.push('>');
            }
            _ => {}
        }
    }
    (src, preserved, expected_bang)
}

fn frag_strategy() -> impl Strategy<Value = Frag> {
    (0u8..9, any::<u32>(), any::<u32>()).prop_map(|(kind, a, b)| Frag { kind, a, b })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// 正向管线的四类结构不变量 + 幂等
    #[test]
    fn prop_structural_invariants(frags in prop::collection::vec(frag_strategy(), 1..=12)) {
        let manager = test_manager();
        let pools = Pools::new(&manager);
        let (source, preserved, expected_bang) = render(&frags, &pools, false);

        let out = 带映射转译管线(&source, &manager, None);
        let output = &out.输出;

        let s_toks = lex(&source);
        let o_toks = lex(output);

        // 2. 空白不变量：空白 run 与注释 token 逐一原样保留
        prop_assert_eq!(texts(&s_toks, Cat::Ws), texts(&o_toks, Cat::Ws),
            "空白 token 序列应逐字节保留\nsource={:?}\noutput={:?}", source, output);
        prop_assert_eq!(texts(&s_toks, Cat::Comment), texts(&o_toks, Cat::Comment),
            "注释 token 序列应逐字节保留");

        // 1a. 结构骨架同构：标点/运算符/字面量（除 `!`）文本序列 source==output
        prop_assert_eq!(skeleton_excl_bang(&s_toks), skeleton_excl_bang(&o_toks),
            "除宏 `!` 外标点/字面量应守恒\nsource={:?}", source);

        // 1b. `!` 净增恰等于省略感叹号的宏调用数
        let added = count_bang(&o_toks).saturating_sub(count_bang(&s_toks));
        prop_assert_eq!(added, expected_bang,
            "宏补 `!` 增量应等于省略感叹号的宏调用数\nsource={:?}", source);

        // 1c. 标识符 token 数守恒（原生词→单标识符英文，1:1；无 crate:: 插入）
        prop_assert_eq!(texts(&s_toks, Cat::Ident).len(), texts(&o_toks, Cat::Ident).len(),
            "标识符 token 数应守恒\noutput={:?}", output);

        // 3. 声明位保真（核心）
        for (den, name) in &preserved {
            prop_assert!(decl_name_preserved(&o_toks, den, name),
                "声明位命名 {:?} 应在输出中逐字节保留（位于 {:?} 之后）\noutput={:?}",
                name, den, output);
        }

        // 4. 幂等复核：英文产物再转译不变
        let again = 带映射转译管线(output, &manager, None).输出;
        prop_assert_eq!(&again, output, "对输出再转译应保持不变\noutput={:?}", output);
    }

    /// 反译回环 oracle：不含模块路径、宏一律带感叹号的子集，反译 还原 source
    #[test]
    fn prop_reverse_roundtrip(frags in prop::collection::vec(frag_strategy(), 1..=10)) {
        let manager = test_manager();
        let pools = Pools::new(&manager);
        let (source, _preserved, _bang) = render(&frags, &pools, true);

        let out = 带映射转译管线(&source, &manager, None);
        let output = &out.输出;

        // 合并关键字 + 宏 + 别名 + 派生特征并反转（英文 → 母语，排序最小者优先）
        let (reverse_map, _unique) = build_reverse_and_unique(&[
            manager.取关键词表(),
            manager.取宏表(),
            manager.取别名表(),
            manager.取派生特征表(),
        ]);

        let restored = 反译(
            output,
            &reverse_map,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        );
        prop_assert_eq!(&restored, &source,
            "反译应精确还原母语源\nsource={:?}\noutput={:?}\nrestored={:?}",
            source, output, restored);
    }
}
