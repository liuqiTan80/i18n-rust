//! 词法差分测试（声明位启发式的判定基准）
//!
//! 把 `alias.rs`/`校验.rs` 里的"声明位保护"从经验规则升级为带判定基准的规格：
//! 随机生成大体符合语法的母语方言词元流，正向转译后用 Rust 词法器重新
//! 词法化，对源与产物断言四类结构不变量，并对不含模块路径的子集做反译回环。
//!
//! 断言（管线在 `module_names=None` 下运行，故 `crate::` 阶段被跳过，
//! 唯一的受控标点增量是宏自动补的 `!`）：
//! 1. 词元流结构同构：标点/运算符/字面量（除 `!`）的文本序列源与产物一致；
//!    `!` 的净增数恰等于本用例书写时省略感叹号的宏调用数；标识符词元数守恒。
//! 2. 空白不变量：词元间空白段与注释词元在产物中逐一原样保留（管线
//!    只做词元级替换，不增删空白/注释）。
//! 3. 声明位保真（核心）：声明关键字后紧跟的自定义命名（取自别名/标识符词表——
//!    这些词在非声明位会被别名阶段改写）必须在产物中逐字节原样出现，
//!    直接固化"声明位启发式"的契约。
//! 4. 幂等复核：`transpile(output) == output`（英文产物再转译不变）。
//!
//! 反译回环判定基准（独立属性）：对不含模块路径、宏一律带感叹号的生成子集，
//! `反译(output, reverse_map(kw+macro+alias+derive), ∅, ∅)` 还原源。

use std::collections::HashMap;

use i18n_rust_engine::带映射转译管线;
use i18n_rust_engine::映射管理器::映射管理器;
use i18n_rust_engine::词法器::反译;
use proptest::collection as 属性向量;
use proptest::prelude::*;
use rustc_lexer::{TokenKind, tokenize};

/// 别名阶段视为"声明关键字"的英文形式（见 `别名::声明关键字表`）：
/// 其后的首个标识符为用户自定义命名，须受声明位保护。
const 声明关键字英文: &[&str] = &[
    "fn", "struct", "enum", "trait", "type", "mod", "let", "const", "static",
];

/// 受控测试语言包：关键字/类型/宏/派生特征/模块路径/标识符别名全覆盖，
/// 且所有替换目标均为单标识符（不含 `::`/`.`/空格），便于结构同构断言。
fn 测试管理器() -> 映射管理器 {
    let 关键词配置 = r#"
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
    let 模块路径配置 = r#"
["模块路径"]
"标准集合" = "std::collections"
"#;
    let 标识符配置 = r#"
["标识符"]
"新建" = "new"
"长度" = "len"
"迭代" = "iter"
"字符串" = "String"
"获取" = "get"
"#;
    映射管理器::从内置载入(关键词配置, 模块路径配置, 标识符配置, &[])
        .expect("加载受控测试语言包失败")
}

/// `en` 是否为合法的单 Rust 标识符（首字母/下划线，余字母数字/下划线）：
/// 只有这样的替换目标才不会引入额外标点，保证结构同构断言成立。
fn 是否为单个标识符(str: &str) -> bool {
    let mut 字符们 = str.chars();
    match 字符们.next() {
        Some(char) if char.is_ascii_alphabetic() || char == '_' => {}
        _ => return false,
    }
    字符们.all(|char| char.is_ascii_alphanumeric() || char == '_')
}

/// 与反向表生成规则一致：英文 → 母语，同英文键取"排序最小"的母语词，
/// 保证反译候选确定。返回 (英文→母语表, 唯一映射的母语词集合)。
fn 构建反向表与唯一集(
    映射表们: &[&HashMap<String, String>],
) -> (HashMap<String, String>, std::collections::HashSet<String>) {
    let mut 按英文: HashMap<String, Vec<String>> = HashMap::new();
    for 映射表 in 映射表们 {
        for (原生, 英文) in *映射表 {
            if 原生 == 英文 || !是否为单个标识符(英文) {
                continue;
            }
            按英文.entry(英文.clone()).or_default().push(原生.clone());
        }
    }
    let mut 反向表 = HashMap::new();
    let mut 唯一集 = std::collections::HashSet::new();
    for (英文, mut 原生词们) in 按英文 {
        原生词们.sort();
        原生词们.dedup();
        if 原生词们.len() == 1 {
            let 原生 = 原生词们.into_iter().next().expect("非空");
            反向表.insert(英文.clone(), 原生.clone());
            唯一集.insert(原生);
        }
    }
    (反向表, 唯一集)
}

/// 词法词元的粗分类
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum 分类 {
    空白,
    注释,
    标识符项,
    其他,
}

fn 分类令牌(种类: TokenKind) -> 分类 {
    match 种类 {
        TokenKind::Whitespace => 分类::空白,
        TokenKind::LineComment | TokenKind::BlockComment { .. } => 分类::注释,
        TokenKind::Ident | TokenKind::RawIdent => 分类::标识符项,
        _ => 分类::其他,
    }
}

/// 词法化为 (分类, 文本) 序列（Rust 词法器的标点均为单词元，`!` 归其他）
fn 词法化(源码: &str) -> Vec<(分类, String)> {
    let mut 收集结果 = Vec::new();
    let mut 偏移 = 0usize;
    for 令牌 in tokenize(源码) {
        收集结果.push((分类令牌(令牌.kind), 源码[偏移..偏移 + 令牌.len].to_string()));
        偏移 += 令牌.len;
    }
    收集结果
}

/// 取指定分类的文本序列
fn 取文本(令牌们: &[(分类, String)], 类别: 分类) -> Vec<String> {
    令牌们
        .iter()
        .filter(|(分类, _)| *分类 == 类别)
        .map(|(_, str)| str.clone())
        .collect()
}

/// 取"结构骨架"：其他类词元文本序列，剔除 `!`（宏自动补的感叹号是唯一受控增量）
fn 骨架排除感叹号(令牌们: &[(分类, String)]) -> Vec<String> {
    令牌们
        .iter()
        .filter(|(分类, str)| *分类 == 分类::其他 && str != "!")
        .map(|(_, str)| str.clone())
        .collect()
}

/// 统计 `!` 词元数
fn 统计感叹号(令牌们: &[(分类, String)]) -> usize {
    令牌们
        .iter()
        .filter(|(分类, str)| *分类 == 分类::其他 && str == "!")
        .count()
}

/// 在产物词元流中验证：英文声明关键字之后（跳过空白/注释）的首个标识符
/// 恰为受保护的原生命名（逐字节）。
fn 声明名保留(令牌们: &[(分类, String)], 英文声明: &str, 原生: &str) -> bool {
    let 有效令牌: Vec<&(分类, String)> = 令牌们
        .iter()
        .filter(|(分类, _)| *分类 == 分类::标识符项 || *分类 == 分类::其他)
        .collect();
    for 窗口 in 有效令牌.windows(2) {
        if 窗口[0].1 == 英文声明 && 窗口[1].1 == 原生 {
            return true;
        }
    }
    false
}

/// 生成用的片段指令：种类 选择构造模板，甲/乙 为池内索引（对池长取模）
#[derive(Clone, Copy, Debug)]
struct 片段 {
    种类: u8,
    甲: u32,
    乙: u32,
}

/// 池：随语言包派生的原生词与固定的自定义命名/字面量/标点/注释
struct 词池 {
    /// 声明关键字（原生词, 英文）
    声明: Vec<(String, String)>,
    /// 别名/标识符原生词（非声明关键字，非模块路径；用于声明位保真与方法调用）
    别名: Vec<String>,
    /// 宏原生词
    宏词: Vec<String>,
    /// 自定义命名：不与任何映射表的原生键或英文值冲突，逐字节安全
    自定义: Vec<String>,
}

const 字面量表: &[&str] = &[
    "42",
    "0",
    "3.14",
    "\"字符串内容\"",
    "\"\"",
    "'a'",
    "b'x'",
    "r#\"原始串\"#",
];
const 注释表: &[&str] = &["// 行注释", "/* 块注释 */", "// 另一注释"];
const 标点表: &[&str] = &[
    ";", ",", "{", "}", "(", ")", "[", "]", "=", "+", "->", "<", ">",
];

impl 词池 {
    fn 新建(映射: &映射管理器) -> 词池 {
        let 关键词表 = 映射.取关键词表();
        let 别名表 = 映射.取别名表();
        let 宏表 = 映射.取宏表();
        let 模块路径表 = 映射.取模块路径表();

        // 反向唯一集：关键词+宏+别名+派生特征（排除模块路径——其英文含 `::` 非单标识符）
        let (_反向, 唯一集) =
            构建反向表与唯一集(&[关键词表, 宏表, 别名表, 映射.取派生特征表()]);

        // 英文值全集（用于自定义命名排雷：自定义名不得是任何表的英文值，避免反译误改）
        let mut 英文值集: std::collections::HashSet<String> = std::collections::HashSet::new();
        for 表 in [关键词表, 宏表, 别名表, 映射.取派生特征表(), 模块路径表] {
            for 英文 in 表.values() {
                英文值集.insert(英文.clone());
            }
        }
        let 原生键集: std::collections::HashSet<&String> = 关键词表
            .keys()
            .chain(宏表.keys())
            .chain(别名表.keys())
            .collect();

        let 声明词: Vec<(String, String)> = 关键词表
            .iter()
            .filter(|(原生, 英文)| {
                声明关键字英文.contains(&英文.as_str())
                    && 是否为单个标识符(英文)
                    && 唯一集.contains(*原生)
            })
            .map(|(原生, 英文)| (原生.clone(), 英文.clone()))
            .collect();

        // 别名词：来自标识符别名表，替换目标为单标识符且反向唯一
        let 别名词: Vec<String> = 别名表
            .keys()
            .filter(|原生| {
                是否为单个标识符(&别名表[*原生])
                    && 唯一集.contains(*原生)
                    && !声明关键字英文.contains(&别名表[*原生].as_str())
            })
            .cloned()
            .collect();

        let 宏词: Vec<String> = 宏表
            .keys()
            .filter(|原生| 是否为单个标识符(&宏表[*原生]) && 唯一集.contains(*原生))
            .cloned()
            .collect();

        // 固定候选自定义命名，过滤任何可能冲突者
        let 自定义名: Vec<String> = [
            "甲",
            "乙",
            "数据",
            "项目名",
            "我的变量",
            "辅助函数",
            "绝对值",
        ]
        .iter()
        .map(|str| str.to_string())
        .filter(|候选| !英文值集.contains(候选) && !原生键集.contains(候选))
        .collect();

        assert!(
            !声明词.is_empty() && !别名词.is_empty() && !自定义名.is_empty(),
            "受控词表不完整：decl={:?} alias={:?} custom={:?}",
            声明词,
            别名词,
            自定义名
        );
        词池 {
            声明: 声明词,
            别名: 别名词,
            宏词,
            自定义: 自定义名,
        }
    }
}

/// 片段模板编号
const 声明片段: u8 = 0;
const 自定义片段: u8 = 1;
const 方法片段: u8 = 2;
const 宏片段: u8 = 3;
const 字面量片段: u8 = 4;
const 标点片段: u8 = 5;
const 注释片段: u8 = 6;
const 路径片段: u8 = 7;
const 涡轮鱼片段: u8 = 8;

/// 渲染片段为源码，同时收集：期望的声明位保真对 (英文声明词, 原生名) 与
/// 期望的宏补感叹号数（省略感叹号书写的宏调用数）。
fn 渲染(
    片段列表: &[片段],
    词池: &词池,
    强制感叹号宏: bool,
) -> (String, Vec<(String, String)>, usize) {
    let mut 源码 = String::new();
    let mut 保留对: Vec<(String, String)> = Vec::new();
    let mut 期望感叹号 = 0usize;
    // 行注释会吞掉同行其后内容：若上一片段为行注释，则以换行分隔，
    // 保证后续片段仍处于代码位（否则宏 `!` 等替换不生效，破坏判定基准前提）
    let mut 上一为行注释 = false;
    for (序号, 片) in 片段列表.iter().enumerate() {
        if 序号 > 0 {
            源码.push(if 上一为行注释 { '\n' } else { ' ' });
        }
        上一为行注释 = false;
        match 片.种类 {
            声明片段 => {
                let (原生声明, 英文声明) = &词池.声明[片.甲 as usize % 词池.声明.len()];
                // 声明名取自别名表（这些词在非声明位会被别名阶段改写）或自定义命名，
                // 二者都应在声明位被逐字节保留
                let 名称 = if 片.乙 % 2 == 0 {
                    词池.别名[片.乙 as usize % 词池.别名.len()].clone()
                } else {
                    词池.自定义[(片.乙 as usize) % 词池.自定义.len()].clone()
                };
                保留对.push((英文声明.clone(), 名称.clone()));
                源码.push_str(原生声明);
                源码.push(' ');
                源码.push_str(&名称);
            }
            自定义片段 => {
                源码.push_str(&词池.自定义[片.甲 as usize % 词池.自定义.len()]);
            }
            方法片段 => {
                源码.push_str(&词池.自定义[片.甲 as usize % 词池.自定义.len()]);
                源码.push('.');
                源码.push_str(&词池.别名[片.乙 as usize % 词池.别名.len()]);
                源码.push_str("()");
            }
            宏片段 => {
                let 宏词项 = &词池.宏词[片.甲 as usize % 词池.宏词.len()];
                源码.push_str(宏词项);
                let 带感叹号 = 强制感叹号宏 || (片.乙 % 2 == 0);
                if 带感叹号 {
                    源码.push('!');
                } else {
                    期望感叹号 += 1;
                }
                源码.push_str("(\"参数\")");
            }
            字面量片段 => 源码.push_str(字面量表[片.甲 as usize % 字面量表.len()]),
            标点片段 => 源码.push_str(标点表[片.甲 as usize % 标点表.len()]),
            注释片段 => {
                let 注释 = 注释表[片.甲 as usize % 注释表.len()];
                上一为行注释 = 注释.starts_with("//");
                源码.push_str(注释);
            }
            路径片段 => {
                源码.push_str(&词池.自定义[片.甲 as usize % 词池.自定义.len()]);
                源码.push_str("::");
                源码.push_str(&词池.自定义[片.乙 as usize % 词池.自定义.len()]);
            }
            涡轮鱼片段 => {
                源码.push_str(&词池.自定义[片.甲 as usize % 词池.自定义.len()]);
                源码.push_str("::<");
                源码.push_str(&词池.自定义[片.乙 as usize % 词池.自定义.len()]);
                源码.push('>');
            }
            _ => {}
        }
    }
    (源码, 保留对, 期望感叹号)
}

fn 片段策略() -> impl Strategy<Value = 片段> {
    (0u8..9, any::<u32>(), any::<u32>()).prop_map(|(种类, 甲, 乙)| 片段 { 种类, 甲, 乙 })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// 正向管线的四类结构不变量 + 幂等
    #[test]
    fn 性质_结构不变量(片段列表 in 属性向量::vec(片段策略(), 1..=12)) {
        let 管理器 = 测试管理器();
        let 词池 = 词池::新建(&管理器);
        let (源码, 保留对, 期望感叹号) = 渲染(&片段列表, &词池, false);

        let 转译产物 = 带映射转译管线(&源码, &管理器, None);
        let 输出 = &转译产物.输出;

        let 源令牌 = 词法化(&源码);
        let 输出令牌 = 词法化(输出);

        // 2. 空白不变量：空白段与注释词元逐一原样保留
        prop_assert_eq!(取文本(&源令牌, 分类::空白), 取文本(&输出令牌, 分类::空白),
            "空白 token 序列应逐字节保留\nsource={:?}\noutput={:?}", 源码, 输出);
        prop_assert_eq!(取文本(&源令牌, 分类::注释), 取文本(&输出令牌, 分类::注释),
            "注释 token 序列应逐字节保留");

        // 1a. 结构骨架同构：标点/运算符/字面量（除 `!`）文本序列源与产物一致
        prop_assert_eq!(骨架排除感叹号(&源令牌), 骨架排除感叹号(&输出令牌),
            "除宏 `!` 外标点/字面量应守恒\nsource={:?}", 源码);

        // 1b. `!` 净增恰等于省略感叹号的宏调用数
        let 新增感叹号 = 统计感叹号(&输出令牌).saturating_sub(统计感叹号(&源令牌));
        prop_assert_eq!(新增感叹号, 期望感叹号,
            "宏补 `!` 增量应等于省略感叹号的宏调用数\nsource={:?}", 源码);

        // 1c. 标识符词元数守恒（原生词→单标识符英文，1:1；无路径前缀插入）
        prop_assert_eq!(取文本(&源令牌, 分类::标识符项).len(), 取文本(&输出令牌, 分类::标识符项).len(),
            "标识符 token 数应守恒\noutput={:?}", 输出);

        // 3. 声明位保真（核心）
        for (英文声明, 名称) in &保留对 {
            prop_assert!(声明名保留(&输出令牌, 英文声明, 名称),
                "声明位命名 {:?} 应在输出中逐字节保留（位于 {:?} 之后）\noutput={:?}",
                名称, 英文声明, 输出);
        }

        // 4. 幂等复核：英文产物再转译不变
        let 再次 = 带映射转译管线(输出, &管理器, None).输出;
        prop_assert_eq!(&再次, 输出, "对输出再转译应保持不变\noutput={:?}", 输出);
    }

    /// 反译回环判定基准：不含模块路径、宏一律带感叹号的子集，反译还原源
    #[test]
    fn 性质_反译回环(片段列表 in 属性向量::vec(片段策略(), 1..=10)) {
        let 管理器 = 测试管理器();
        let 词池 = 词池::新建(&管理器);
        let (源码, _保留对, _感叹号) = 渲染(&片段列表, &词池, true);

        let 转译产物 = 带映射转译管线(&源码, &管理器, None);
        let 输出 = &转译产物.输出;

        // 合并关键字 + 宏 + 别名 + 派生特征并反转（英文 → 母语，排序最小者优先）
        let (反向表, _唯一集) = 构建反向表与唯一集(&[
            管理器.取关键词表(),
            管理器.取宏表(),
            管理器.取别名表(),
            管理器.取派生特征表(),
        ]);

        let 还原 = 反译(
            输出,
            &反向表,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
        );
        prop_assert_eq!(&还原, &源码,
            "反译应精确还原母语源\nsource={:?}\noutput={:?}\nrestored={:?}",
            源码, 输出, 还原);
    }
}
