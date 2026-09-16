// 别名替换模块
// 将源代码中的中文标识符别名（如第三方库的中文名称）替换为英文标识符。
// 仅替换标识符类型的 token，不触碰字符串字面量和注释等内容。
// 声明位保护：紧跟在声明关键字（fn/struct/let 等）后的标识符是用户自己的定义，
// 不是库 API 引用，不参与别名替换；且用户声明的名字在整个文件内的
// 裸使用处都豁免（两遍扫描：先收集声明名，再逐 token 替换），
// 避免 `let 新建 = 5` 声明位受保护而后续使用处被误替换成 new。
// 三条精细规则：
// 1. `::` 限定后的路径段按「链根」分级处理：根是项目符号
//    （crate/self/super/项目模块名/项目项名）时链内**与项目声明名
//    同名**的段豁免——`平台Linux::新建` 的定义侧与调用侧一致
//    （避免 E0599）；链内非项目名与根非项目符号的段都照常替换
//    （`源码映射项::新建` 的 `新建` 未定义于项目、`盒子::新建`
//    的 `新建` 是库 API）；无项目上下文时退化为「本文件项名豁免」
//    （`接口错误::错误请求`）；
// 2. 库特征实现块（`impl <映射词特征> for 类型 {}`）内的方法名是库 API
//    规定的名称（用户从映射表抄写而来），不是用户自定义名字，不受声明位
//    保护——`实现 抄写器 对于 类型 { 函数 渲染（...） }` 中的 `渲染`
//    必须转译为 `render`，否则无法匹配 `Scribe::render`（E0407/E0046）；
// 3. 函数参数与闭包参数是「值绑定」（用户命名）：`fn 完整网址(路径: &str)`
//    的 `路径` 在声明与使用处都豁免替换（参数「类型」位置的字照常替换），
//    与格式化串中的 `{路径}` 保持一致，避免 E0425（找不到名称 `路径`）；
// 4. 结构体字段名与枚举变体名同样是用户命名，纳入声明收集：
//    字段 `struct 容器 { 值: i32 }` 的 `值`（撞 `值`=values 映射键）
//    在定义/构造/访问处（`实例.值`）全豁免；枚举变体
//    `enum 判定结果 { 保留, 丢弃 }` 的 `保留`/`丢弃`（撞
//    retain/drop 映射键）同理；变体可经 `::` 访问
//    （`判定结果::保留`）故收集为项，字段收集为值绑定。
//    方法调用位（`实例.方法()`）不查字段名：`编辑表.长度()` 的
//    `长度` 是 `Vec::len` 调用，只查「项目方法名集合」（项）；
// 5. 模式绑定（match 臂、let 解构、for 模式）同样是值绑定：
//    `匹配 有值(入参) { 有值(连接) => … }` 的 `连接`（撞
//    `连接`=join 映射键）全文件豁免（见 collect_pattern_bindings）。

use rustc_lexer::{TokenKind, tokenize};
use std::collections::{HashMap, HashSet};

use crate::cache::SourceMapEntry;

/// 单阶段替换结果：输出与编辑表
///
/// 编辑表以**本阶段输入文本**的字节偏移记录（input_offset 升序、互不重叠），
/// replacement 为本阶段输出文本。
#[derive(Debug, Clone)]
pub struct ReplaceResult {
    /// 替换后的文本
    pub output: String,
    /// 编辑表
    pub edits: Vec<SourceMapEntry>,
}

/// 声明位关键字（转译后的英文形式）：其后紧跟的标识符为用户定义
///
/// 不含 `mut`：仅 `让 mut 名称` 场景下透明传递声明状态（见替换循环），
/// `&mut 类型` 等非声明位的 mut 不传递，避免误保护库类型引用。
/// 不含 `impl`：`impl 特征 for 类型` 中的特征/类型多为库 API，仍需替换。
/// LSP 列映射模拟别名替换时复用同一张表，保证两侧行为一致。
pub const DECL_KEYWORDS: &[&str] = &[
    "fn", "struct", "enum", "trait", "type", "mod", "let", "const", "static",
];

/// 变量声明关键字：其后名字是「变量」（不可经 `::` 访问）；
/// [`DECL_KEYWORDS`] 的其余关键字（fn/struct/enum/trait/type/mod）声明的
/// 是「项」，项可经 `::` 访问（见模块文档第 1 条规则）
const VAR_DECL_KEYWORDS: &[&str] = &["let", "const", "static"];

/// 用户声明名与库特征方法名的收集结果（第一遍扫描）
#[derive(Debug, Default)]
pub struct DeclaredNames {
    /// 项声明名（fn/struct/enum/trait/type/mod）+ 枚举变体名：
    /// `::` 后的同名标识符仍豁免
    pub items: HashSet<String>,
    /// 变量声明名（let/const/static）+ 函数/闭包参数：
    /// `::` 后的同名标识符照常替换
    pub variables: HashSet<String>,
    /// 结构体命名字段：裸使用处豁免（`实例.字段`/结构体字面量）；
    /// `::` 后照常替换（字段不能经 `::` 访问）。
    /// 与 variables 分开收集，供项目级合并（绑定名是文件局部的）
    pub members: HashSet<String>,
    /// 库特征实现块内的方法名：声明位保护对这些名字失效（库 API 名称）
    pub library_fns: HashSet<String>,
}

/// 项目级声明上下文（跨文件声明豁免）
///
/// 单文件的声明豁免无法覆盖跨文件调用：A 文件声明 `pub 函数 新建()`，
/// B 文件调用 `包::A::新建()` 时 B 的转译单元看不到 A 的声明，
/// `新建` 被当作库别名替换出 `new`（E0599）。转译前扫描同 crate
/// 全部方言源文件汇总本结构，把使用处豁免升级到项目级。
///
/// 保守近似与单文件声明豁免一致：不做作用域/可见性分析，同名即豁免。
#[derive(Debug, Default, Clone)]
pub struct ProjectContext {
    /// 项目模块名（方言文件名词干）：`::` 路径链根
    pub modules: HashSet<String>,
    /// 项目声明名（项 + 结构体字段）：裸使用处豁免
    ///（let/参数等文件内绑定不参与合并——绑定名是文件局部的）
    pub names: HashSet<String>,
    /// 项目项名（fn/struct/enum/trait/type/mod + 枚举变体）：
    /// `::` 路径段豁免与路径链根判定（字段不参与：不可经 `::` 访问）
    pub items: HashSet<String>,
}

impl ProjectContext {
    /// 从项目源文件集合构建：`modules` 为项目模块名词干，
    /// `sources` 为全部方言源文件内容（母语原文）
    ///
    /// 声明收集工作在关键字转译后的文本上进行（声明关键字需英文形式），
    /// 内部对每个源文件先跑词法转译阶段再收集。
    pub fn from_sources<'a>(
        modules: HashSet<String>,
        sources: impl IntoIterator<Item = &'a str>,
        manager: &crate::mapping_manager::MappingManager,
    ) -> Self {
        let alias_map = manager.get_alias_map();
        let macro_map = manager.get_macro_map();
        let derive_map = manager.get_derive_map();
        let mut names = HashSet::new();
        let mut items = HashSet::new();
        for source in sources {
            let lex = crate::lexer::transpile_with_map(
                source,
                manager.get_keyword_map(),
                &macro_map,
                &derive_map,
                manager.get_use_defer_words(),
                manager.get_method_defer_words(),
                alias_map,
            );
            let declared = collect_declared_names(&lex.output, alias_map);
            for name in declared.items {
                names.insert(name.clone());
                items.insert(name);
            }
            names.extend(declared.members);
        }
        Self {
            modules,
            names,
            items,
        }
    }

    /// 是否为空（无模块且无声明名）：空上下文不产生任何豁免
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty() && self.names.is_empty()
    }

    /// 项目上下文指纹（排序后哈希，跨进程确定）：并入转译缓存指纹，
    /// 项目声明集合变化时相关缓存自动失效
    pub fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut modules: Vec<&String> = self.modules.iter().collect();
        modules.sort_unstable();
        let mut names: Vec<&String> = self.names.iter().collect();
        names.sort_unstable();
        let mut items: Vec<&String> = self.items.iter().collect();
        items.sort_unstable();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        ("i18n-rust-project", modules, names, items).hash(&mut hasher);
        hasher.finish()
    }

    /// 裸使用处豁免：项目声明名（项 + 结构体字段，含其他文件声明）
    fn naked_exempt(&self, text: &str) -> bool {
        self.names.contains(text)
    }

    /// 路径链根判定：crate/super/self/项目模块名/项目项名。
    /// 根段后跟 `::` 时整条路径视为项目路径（链内全部段豁免）
    fn is_path_root(&self, text: &str) -> bool {
        matches!(text, "crate" | "super" | "self")
            || self.modules.contains(text)
            || self.items.contains(text)
    }
}

/// 类型体（结构体/枚举）扫描状态：收集命名字段与枚举变体
#[derive(Debug)]
struct TypeBodyScan {
    /// 是否为枚举体（枚举顶层成员是变体，收集为项）
    is_enum: bool,
    /// 花括号深度（1 = 类型体顶层）
    depth: u32,
    /// 处于成员开始位置（`{`/`,` 之后）：下一个标识符是字段名/变体名
    at_start: bool,
    /// 已见的名字（等 `:` 确认后收集为字段）
    pending_member: Option<String>,
}

/// 收集用户在声明位定义的标识符名（第一遍扫描）
///
/// 状态机与替换主循环一致：声明关键字后紧跟的标识符计入集合；
/// `mut` 在声明态内透明传递；空白/注释不打断声明态；符号终结声明位。
/// 另收集「值绑定」：函数参数名（`fn 名(参数: 类型)` 中的参数开始位
/// 且后跟 `:`）与闭包参数名（`|甲, 乙|`，可无类型标注直接收集）——
/// 绑定名是用户命名，不是库 API 引用。
/// 结构体/枚举体扫描（[`TypeBodyScan`]）：命名字段（`字段: 类型`）
/// 收集为值绑定，枚举顶层变体名收集为项（变体可经 `::` 访问）。
/// 集合内的名字在文件内的使用处豁免别名替换（保守近似：不做作用域
/// 分析，同名遮蔽场景同样豁免，与声明位保护的设计意图一致）。
/// 库特征实现块内的方法名（见 [`collect_library_impl_fns`]）从保护集合剔除。
pub fn collect_declared_names(source: &str, alias_map: &HashMap<String, String>) -> DeclaredNames {
    let mut result = DeclaredNames::default();
    let mut prev_decl: Option<&'static str> = None;
    // —— 类型体（struct/enum）扫描状态 ——
    // pending_type：已见 `struct`/`enum` 关键字，值为（是否枚举，是否已见名字）；
    // 名字后遇 `{` 进入体扫描，遇 `;` 放弃（unit/tuple 结构体无可收集成员）
    let mut pending_type: Option<(bool, bool)> = None;
    let mut type_body: Option<TypeBodyScan> = None;
    // 类型体内的括号/方括号深度：>0 时内部标识符不参与成员收集
    //（`pub(crate) 字段` 可见性、`[T; N]` 数组类型、`#[属性]` 属性）
    let mut body_paren = 0u32;
    let mut body_angle = 0u32;
    let mut body_bracket = 0u32;
    // —— 函数参数绑定跟踪 ——
    // fn_head：0=不在 fn 头部；1=已见 fn 等待参数括号；2=在参数括号内。
    // 括号内处于参数开始位置（`(` 或 `,` 之后）且后跟 `:` 的标识符是
    // 用户声明的参数名（`mut 名: 类型` 中 mut 透明传递）
    let mut fn_head = 0u32;
    // fn 名后的 `<...>` 泛型深度：`fn 名<T>(...)` 中的 `(` 不是参数括号
    let mut fn_angle = 0u32;
    let mut paren_depth = 0u32;
    let mut at_param_start = false;
    let mut pending_param: Option<String> = None;
    // —— 闭包参数绑定跟踪（`|甲, 乙| 体`，无类型标注直接收集）——
    let mut in_closure = false;
    // 闭包参数内的括号深度：解构/类型（`|(甲, 乙): (T, T)|`）内的名字不收集
    let mut closure_parens = 0u32;
    let mut closure_start = false;
    // 上一个有效 token 是否可作表达式结尾（区分位或 `a | b` 与闭包 `|甲|`）
    let mut prev_value_end = false;
    // 上一个 `|`（Or）token 的结束偏移：rustc_lexer 不合并 `||`，它是连续
    // 两个 `|` token，紧邻的第二个不得判定为闭包起始（`a || b` 是逻辑或）
    let mut last_or_end = usize::MAX;
    // token 序列预收集：本函数内三遍扫描（声明名/库特征方法/模式绑定）
    // 共享同一份 token 序列，避免重复词法化
    let tokens: Vec<rustc_lexer::Token> = tokenize(source).collect();
    let mut offset = 0;
    for token in &tokens {
        let text = &source[offset..offset + token.len];
        match token.kind {
            TokenKind::Ident => {
                // —— 类型头推进（struct/enum 后第一个标识符是类型名）——
                // 名字已由下方声明关键字逻辑收集为项；此处仅推进状态，
                // `pub` 修饰不消耗「等名字」态
                if text == "struct" {
                    pending_type = Some((false, false));
                } else if text == "enum" {
                    pending_type = Some((true, false));
                } else if let Some((_, seen)) = &mut pending_type
                    && !*seen
                    && text != "pub"
                {
                    *seen = true;
                }
                // —— 类型体成员收集 ——
                // 成员开始位置的标识符：枚举顶层是变体名（收集为项，变体可经
                // `::` 访问）；struct 字段与变体携带的结构体字段先入候选，
                // 等 `:` 确认后收集（`pub(crate)` 可见性、`#[属性]`、数组类型
                // `[T; N]` 与泛型 `<...>` 内部的标识符不参与）
                if let Some(body) = &mut type_body
                    && body_paren == 0
                    && body_angle == 0
                    && body_bracket == 0
                    && body.at_start
                    && text != "pub"
                {
                    if body.is_enum && body.depth == 1 {
                        result.items.insert(text.to_string());
                    } else {
                        body.pending_member = Some(text.to_string());
                    }
                    body.at_start = false;
                }
                if let Some(keyword) = prev_decl {
                    if VAR_DECL_KEYWORDS.contains(&keyword) {
                        result.variables.insert(text.to_string());
                    } else {
                        result.items.insert(text.to_string());
                    }
                }
                prev_decl = next_decl_keyword(prev_decl, text);
                // 函数参数名：参数开始位置的标识符（`mut` 后继续等待名字）
                if fn_head == 2 && paren_depth == 1 && at_param_start && text != "mut" {
                    pending_param = Some(text.to_string());
                    at_param_start = false;
                }
                if fn_head == 0 && text == "fn" {
                    fn_head = 1;
                    fn_angle = 0;
                }
                // 闭包参数名：`|` 或 `,` 之后的名字（`mut 名` 同样透明）
                if in_closure && closure_parens == 0 && closure_start && text != "mut" {
                    result.variables.insert(text.to_string());
                    closure_start = false;
                }
                // `move |甲|`：move 是闭包前缀，其后 `|` 仍是闭包起始
                prev_value_end = text != "move";
            }
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. } => {}
            TokenKind::OpenParen => {
                prev_decl = None;
                if type_body.is_some() {
                    body_paren += 1;
                }
                if fn_head == 1 && fn_angle == 0 {
                    fn_head = 2;
                    paren_depth = 1;
                    at_param_start = true;
                } else if fn_head == 2 {
                    paren_depth += 1;
                    at_param_start = false;
                }
                if in_closure {
                    closure_parens += 1;
                    closure_start = false;
                }
                pending_param = None;
                prev_value_end = false;
            }
            TokenKind::CloseParen => {
                prev_decl = None;
                if type_body.is_some() {
                    body_paren = body_paren.saturating_sub(1);
                }
                if fn_head == 2 {
                    paren_depth = paren_depth.saturating_sub(1);
                    if paren_depth == 0 {
                        fn_head = 0;
                    }
                    at_param_start = false;
                }
                if in_closure {
                    closure_parens = closure_parens.saturating_sub(1);
                    closure_start = false;
                }
                pending_param = None;
                prev_value_end = true;
            }
            TokenKind::Comma => {
                prev_decl = None;
                if fn_head == 2 && paren_depth == 1 {
                    at_param_start = true;
                }
                if in_closure && closure_parens == 0 {
                    closure_start = true;
                }
                // 类型体成员分隔：重置成员开始位（括号/尖括号/方括号内的
                // 逗号是类型参数分隔，不是成员分隔）
                if body_paren == 0
                    && body_angle == 0
                    && body_bracket == 0
                    && let Some(body) = &mut type_body
                {
                    body.at_start = true;
                    body.pending_member = None;
                }
                // 名字后遇 `,`（无类型标注模式）：丢弃待确认候选
                pending_param = None;
                prev_value_end = false;
            }
            TokenKind::Colon => {
                prev_decl = None;
                // `名:` → 确认函数参数绑定（其他位置不会有待确认候选）
                if let Some(name) = pending_param.take() {
                    result.variables.insert(name);
                }
                // `字段:` → 确认结构体字段名（收作成员名；`::` 的第二冒号
                // 无候选，无副作用）
                if let Some(body) = &mut type_body
                    && let Some(name) = body.pending_member.take()
                {
                    result.members.insert(name);
                }
                prev_value_end = false;
            }
            TokenKind::Or => {
                prev_decl = None;
                if in_closure {
                    if closure_parens == 0 {
                        in_closure = false;
                    }
                } else if !prev_value_end && last_or_end != offset {
                    // 前一个有效 token 不是表达式结尾，且不与上一个 `|`
                    // 相邻（`a || b` 是连续两个 `|` token）→ 闭包起始
                    //（位或 `a | b` 的 `|` 前是表达式，不进入闭包态）
                    in_closure = true;
                    closure_parens = 0;
                    closure_start = true;
                }
                last_or_end = offset + token.len;
                pending_param = None;
                prev_value_end = false;
            }
            TokenKind::Lt => {
                prev_decl = None;
                if fn_head == 1 {
                    fn_angle += 1;
                }
                if type_body.is_some() {
                    body_angle += 1;
                }
                pending_param = None;
                prev_value_end = false;
            }
            TokenKind::Gt => {
                prev_decl = None;
                if fn_head == 1 {
                    fn_angle = fn_angle.saturating_sub(1);
                }
                if type_body.is_some() {
                    body_angle = body_angle.saturating_sub(1);
                }
                pending_param = None;
                prev_value_end = false;
            }
            TokenKind::OpenBrace => {
                // `struct/enum 名 {` → 进入类型体成员扫描；体内嵌套花括号
                //（变体携带的结构体体、常量表达式块）加深一层
                if let Some((is_enum, seen)) = pending_type.take() {
                    if seen {
                        type_body = Some(TypeBodyScan {
                            is_enum,
                            depth: 1,
                            at_start: true,
                            pending_member: None,
                        });
                        body_paren = 0;
                        body_angle = 0;
                        body_bracket = 0;
                    }
                } else if type_body.is_some()
                    && body_paren == 0
                    && body_angle == 0
                    && body_bracket == 0
                    && let Some(body) = &mut type_body
                {
                    body.depth += 1;
                    body.at_start = true;
                    body.pending_member = None;
                }
                prev_decl = None;
                pending_param = None;
                prev_value_end = false;
            }
            TokenKind::CloseBrace => {
                // 类型体闭合：depth 归零退出扫描（括号/尖括号/方括号内的
                // 花括号不是类型体边界）
                if body_paren == 0 && body_angle == 0 && body_bracket == 0 {
                    let exited = if let Some(body) = &mut type_body {
                        body.depth = body.depth.saturating_sub(1);
                        if body.depth == 0 {
                            true
                        } else {
                            body.at_start = true;
                            body.pending_member = None;
                            false
                        }
                    } else {
                        false
                    };
                    if exited {
                        type_body = None;
                    }
                }
                prev_decl = None;
                pending_param = None;
                prev_value_end = true;
            }
            TokenKind::OpenBracket => {
                // 方括号内的标识符不参与成员收集（属性、数组类型 `[T; N]`）
                if type_body.is_some() {
                    body_bracket += 1;
                }
                prev_decl = None;
                pending_param = None;
                prev_value_end = false;
            }
            TokenKind::CloseBracket => {
                if type_body.is_some() {
                    body_bracket = body_bracket.saturating_sub(1);
                }
                prev_decl = None;
                pending_param = None;
                prev_value_end = true;
            }
            _ => {
                prev_decl = None;
                pending_param = None;
                // `struct/enum 名;`（单元结构体）：放弃类型体等待
                if matches!(token.kind, TokenKind::Semi) {
                    pending_type = None;
                }
                prev_value_end = matches!(
                    token.kind,
                    TokenKind::Literal { .. }
                        | TokenKind::CloseBracket
                        | TokenKind::CloseBrace
                        | TokenKind::Question
                );
            }
        }
        offset += token.len;
    }
    // 库特征实现块内的方法名从保护集合剔除：它们是库 API 规定的名称，
    // 应照常参与别名替换（如 `实现 抄写器 对于 类型 { 函数 渲染（...） }`）
    result.library_fns = collect_library_impl_fns(&tokens, source, &result.items, alias_map);
    for name in &result.library_fns {
        result.items.remove(name);
        result.variables.remove(name);
    }
    // 模式绑定收集（match 臂 / let 解构 / for 模式）：独立扫描
    collect_pattern_bindings(&tokens, source, &mut result);
    result
}

/// 推进声明关键字状态：`mut` 透明传递，其余按是否声明关键字更新
fn next_decl_keyword(prev: Option<&'static str>, text: &str) -> Option<&'static str> {
    if text == "mut" {
        return prev;
    }
    DECL_KEYWORDS.iter().find(|&&k| k == text).copied()
}

/// `impl` 块扫描状态（第二遍扫描用）
#[derive(Debug)]
enum ImplScan {
    /// 不在 impl 上下文中
    None,
    /// 已见 `impl`，等待特征名/类型名（`<...>` 泛型参数内的名字忽略）
    Header,
    /// 已见 impl 目标名，等待块开始 `{`
    Named(String),
    /// 在 impl 块内；`lib` 表示这是「库特征实现块」
    Body { lib: bool },
}

/// 识别「库特征实现块」内的方法名（第二遍扫描）
///
/// 实现外部（库）特征时，`impl` 块内的方法名是库 API 规定的方法名
/// （用户从映射表抄写而来），必须照常参与别名替换；而实现用户自定义
/// 特征（`trait 可显示`）时方法名是用户取的名字，保持声明位保护。
/// 判定「库特征」的两个条件同时满足：特征名不在用户项集合中（排除
/// 同文件自定义特征），且在别名映射表中（无映射词时替换循环本就无事
/// 可做，保持保护无害，也避免跨文件自定义特征被误判）。
fn collect_library_impl_fns(
    tokens: &[rustc_lexer::Token],
    source: &str,
    items: &HashSet<String>,
    alias_map: &HashMap<String, String>,
) -> HashSet<String> {
    let mut library_fns = HashSet::new();
    let mut state = ImplScan::None;
    // Header 中 `<...>` 泛型参数深度：`impl<T> 特征 for 类型` 里 `T` 不是特征名
    let mut angle_depth = 0u32;
    // Body 内大括号深度：归零时退出 impl 块
    let mut brace_depth = 0u32;
    // 上一个有意义 token 是否为 `fn`（空白/注释不打断，与主循环一致）
    let mut prev_is_fn = false;
    let mut offset = 0;
    for token in tokens {
        let text = &source[offset..offset + token.len];
        match token.kind {
            TokenKind::Ident => {
                match &state {
                    ImplScan::None => {
                        if text == "impl" {
                            state = ImplScan::Header;
                            angle_depth = 0;
                        }
                    }
                    ImplScan::Header => {
                        if angle_depth == 0 {
                            state = ImplScan::Named(text.to_string());
                        }
                    }
                    // `impl 特征 for 类型`：特征名之后的 for/类型名均忽略，等待 `{`
                    ImplScan::Named(_) => {}
                    ImplScan::Body { lib } => {
                        if *lib && prev_is_fn {
                            library_fns.insert(text.to_string());
                        }
                    }
                }
                prev_is_fn = text == "fn";
            }
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. } => {}
            TokenKind::OpenBrace => {
                if let ImplScan::Named(name) = &state {
                    let lib = !items.contains(name) && alias_map.contains_key(name);
                    state = ImplScan::Body { lib };
                    brace_depth = 0;
                } else if matches!(state, ImplScan::Body { .. }) {
                    brace_depth += 1;
                } else if matches!(state, ImplScan::Header) {
                    // `impl {`：非法写法，放弃跟踪
                    state = ImplScan::None;
                }
                prev_is_fn = false;
            }
            TokenKind::CloseBrace => {
                if matches!(state, ImplScan::Body { .. }) {
                    if brace_depth == 0 {
                        state = ImplScan::None;
                    } else {
                        brace_depth -= 1;
                    }
                }
                prev_is_fn = false;
            }
            TokenKind::Lt => {
                if matches!(state, ImplScan::Header) {
                    angle_depth += 1;
                }
                prev_is_fn = false;
            }
            TokenKind::Gt => {
                if matches!(state, ImplScan::Header) {
                    angle_depth = angle_depth.saturating_sub(1);
                }
                prev_is_fn = false;
            }
            _ => prev_is_fn = false,
        }
        offset += token.len;
    }
    library_fns
}

/// for 模式挂起中出现的「非模式词」：声明/语句关键字
///
/// 真模式里不可能出现这些词，出现即说明挂起跨越了语句边界
/// （残留/剥离代码），须丢弃挂起，避免把后续代码里的名字误收为绑定。
const STMT_BOUNDARY_KEYWORDS: &[&str] = &[
    "fn", "struct", "enum", "impl", "trait", "mod", "type", "const", "static", "let", "use", "pub",
    "return", "if", "else", "while", "loop", "match", "break", "continue", "unsafe", "async",
    "await", "move", "where",
];

/// 模式绑定收集（第三遍扫描）：match 臂 / let 解构 / for 模式
///
/// 模式中出现的标识符是「值绑定」（用户命名），须与函数/闭包参数一样
/// 全文件豁免替换——否则 `成功(连接) =>` 的 `连接`（撞 `连接`=join
/// 映射键）会被误替换为 `join`（实测确认的缺陷）。识别规则：
/// - match 臂：`=>`（`Eq`+`Gt` 相邻两 token）向左回溯到臂起点
///   （深度 0 的 `,`/`{`/`;`），区间内标识符按模式语法过滤后收集；
///   守卫表达式中的名字同为值使用，一并收集无害（方法调用位与
///   `::` 路径位不查值绑定集合，不会过度豁免库 API）；
/// - let 解构：`let` 后解构括号（`let (甲, 乙) = …`、
///   `let 点 { x, y } = …`）内的名字；`=` 结束（类型标注在括号外，
///   不受影响），if-let/while-let 同形覆盖；
/// - for 模式：`for` 到 `in` 之间的名字（`for &长度 in …`、
///   `for (键, 值) in …`）；挂起期间遇声明/语句关键字即丢弃。
///   `impl … for 类型 {` 的 for 由 impl 头部状态排除。
///
/// 过滤见 [`is_pattern_binding`]：`::` 路径段（`错误::连接` 的
/// `连接` 是库变体）、后跟 `(`/`{`/`[` 的构造路径（`有值(…)`、
/// `点 { … }`）、后跟 `:` 的字段名（`x: 甲` 的 `x`）、`mut`/`ref`
/// 修饰符都不是绑定。
fn collect_pattern_bindings(
    tokens: &[rustc_lexer::Token],
    source: &str,
    result: &mut DeclaredNames,
) {
    // —— let 解构态 / for 模式态 / impl 头部区间 ——
    let mut let_ok = false;
    let mut let_depth = 0u32;
    let mut for_ok = false;
    let mut for_depth = 0u32;
    let mut for_pending: Vec<String> = Vec::new();
    // `impl … for 类型 {` 的头部区间：其间出现的 `for` 是 impl 语法
    let mut in_impl_header = false;

    let mut offset = 0usize;
    for i in 0..tokens.len() {
        let kind = tokens[i].kind;
        let text = &source[offset..offset + tokens[i].len];
        match kind {
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. } => {}
            TokenKind::Ident => {
                if text == "impl" {
                    in_impl_header = true;
                }
                // —— for 模式态 ——
                if for_ok {
                    if text == "in" && for_depth == 0 {
                        // 模式结束：提交挂起绑定
                        result.variables.extend(for_pending.drain(..));
                        for_ok = false;
                    } else if for_depth == 0
                        && (text == "for" || STMT_BOUNDARY_KEYWORDS.contains(&text))
                    {
                        // 遇嵌套 for 重启挂起；遇声明/语句关键字丢弃挂起
                        for_pending.clear();
                        for_ok = text == "for";
                        for_depth = 0;
                    } else if is_pattern_binding(tokens, i, text) {
                        for_pending.push(text.to_string());
                    }
                } else if text == "for" && !in_impl_header {
                    for_ok = true;
                    for_depth = 0;
                    for_pending.clear();
                }
                // —— let 解构态（`=` 之前括号内的名字是绑定）——
                if let_ok && let_depth > 0 && is_pattern_binding(tokens, i, text) {
                    result.variables.insert(text.to_string());
                }
                if text == "let" {
                    let_ok = true;
                    let_depth = 0;
                }
            }
            TokenKind::OpenParen | TokenKind::OpenBracket => {
                if let_ok {
                    let_depth += 1;
                }
                if for_ok {
                    for_depth += 1;
                }
            }
            TokenKind::OpenBrace => {
                // impl 头部结束（进入块体）
                in_impl_header = false;
                if let_ok {
                    let_depth += 1;
                }
                if for_ok {
                    for_depth += 1;
                }
            }
            TokenKind::CloseParen | TokenKind::CloseBracket | TokenKind::CloseBrace => {
                let_depth = let_depth.saturating_sub(1);
                for_depth = for_depth.saturating_sub(1);
            }
            TokenKind::Semi => {
                in_impl_header = false;
                let_ok = false;
                let_depth = 0;
                for_pending.clear();
                for_ok = false;
                for_depth = 0;
            }
            TokenKind::Eq => {
                // `let 模式 =`：模式结束（`=` 之后是表达式，名字照常替换）；
                // for 模式里 `=` 不合法（防御）
                if let_ok && let_depth == 0 {
                    let_ok = false;
                }
                if for_ok && for_depth == 0 {
                    for_pending.clear();
                    for_ok = false;
                }
            }
            TokenKind::Lt | TokenKind::Colon if for_ok && for_depth == 0 => {
                // for 模式里 `<`/`:` 不合法（泛型/类型标注等非循环场景，防御）
                for_pending.clear();
                for_ok = false;
            }
            TokenKind::Gt => {
                // `=>`（`Eq`+`Gt` 相邻两 token）：match 臂箭头 → 回溯收集
                //（索引相邻即两 token 间无其他 token，`=>` 无空白）
                if let Some(p) = prev_sig_index(tokens, i)
                    && tokens[p].kind == TokenKind::Eq
                    && p == i - 1
                {
                    collect_match_arm_bindings(tokens, offset, i, source, result);
                }
            }
            _ => {}
        }
        offset += tokens[i].len;
    }
}

/// `=>` 回溯：收集 match 臂模式中的绑定名
///
/// 从箭头（索引 `arrow`，起点偏移 `arrow_offset`）向左回溯到臂起点
/// （深度 0 的 `,`/`{`/`;`），区间内标识符经 [`is_pattern_binding`]
/// 过滤后收为值绑定。`{}`/`()`/`[]` 深度配对处理结构体模式
/// （`点 { x, y }`）与嵌套模式。
///
/// 越界防护（weix-1 #27）：带块臂体（`=> { … }`）的**下一臂**回溯时
/// 会先遇到上一臂的块闭合 `}`；若不识别，深度失衡后 `,`/`;` 不再
/// 终止，回溯将穿透上一臂语句，把类型标注（`让 x: T = …` 的 `T`）
/// 等裸标识符误收为值绑定 → 其全文件豁免替换（`无符号机器整数`
/// = usize 实测不被翻译，产物 E0425）。块体臂的逗号可省略，其闭合
/// `}` 即当前臂模式的左边界：识别后立即截止
/// （见 [`close_brace_ends_arm_body`]）。
fn collect_match_arm_bindings(
    tokens: &[rustc_lexer::Token],
    arrow_offset: usize,
    arrow: usize,
    source: &str,
    result: &mut DeclaredNames,
) {
    // arrow 前一个有意义 token 是 `=`（调用方已确认 `=>` 相邻）
    let Some(eq) = prev_sig_index(tokens, arrow) else {
        return;
    };
    let mut depth = 0u32;
    // 反向累计各 token 的起点偏移（tokenize 连续覆盖全源）
    let mut start = arrow_offset.saturating_sub(tokens[eq].len);
    for j in (0..eq).rev() {
        start = start.saturating_sub(tokens[j].len);
        let kind = tokens[j].kind;
        match kind {
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. } => {}
            TokenKind::CloseParen | TokenKind::CloseBracket => depth += 1,
            TokenKind::CloseBrace => {
                // 上一臂块体闭合即当前臂模式左边界：就此截止，
                // 不得穿入上一臂体（见函数文档「越界防护」）
                if depth == 0 && close_brace_ends_arm_body(tokens, j) {
                    break;
                }
                depth += 1;
            }
            TokenKind::OpenParen | TokenKind::OpenBracket => {
                if depth == 0 {
                    break; // 模式区间外的括号（异常输入）：保守停止
                }
                depth -= 1;
            }
            TokenKind::OpenBrace => {
                if depth == 0 {
                    break; // match 体（或宏体）的起点
                }
                depth -= 1;
            }
            TokenKind::Comma | TokenKind::Semi => {
                if depth == 0 {
                    break; // 上一臂的分隔或语句边界
                }
            }
            TokenKind::Ident => {
                let text = &source[start..start + tokens[j].len];
                if is_pattern_binding(tokens, j, text) {
                    result.variables.insert(text.to_string());
                }
            }
            _ => {}
        }
    }
}

/// `}`（索引 `close`）是否为「臂块体闭合」（`=> { … }` 的收尾）
///
/// 判定：向左找配对的 `{`，其前一个有意义 token 是 `=>` 的 `>`
/// （`Eq`+`Gt` 相邻）则为臂体闭合。结构体模式的 `}`（配对 `{` 前
/// 是路径名，如 `点 { x }`、`点<T> { x }`）返回 false，照常参与
/// 深度配对。
fn close_brace_ends_arm_body(tokens: &[rustc_lexer::Token], close: usize) -> bool {
    let mut depth = 0u32;
    let mut k = close;
    while k > 0 {
        k -= 1;
        match tokens[k].kind {
            TokenKind::CloseBrace => depth += 1,
            TokenKind::OpenBrace => {
                if depth == 0 {
                    // 配对 `{`：前一个有意义 token 是 `=>` 的 `>`？
                    return prev_sig_index(tokens, k).is_some_and(|p| {
                        tokens[p].kind == TokenKind::Gt
                            && prev_sig_index(tokens, p)
                                .is_some_and(|pp| tokens[pp].kind == TokenKind::Eq && pp == p - 1)
                    });
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    false
}

/// 模式内标识符的「值绑定」判定
///
/// `mut`/`ref` 修饰符不是名字；`::` 路径段（变体/常量路径）、后跟
/// `(`/`{`/`[` 的构造路径（`有值(…)`、`点 { … }`）、后跟 `:` 的
/// 字段名（`x: 甲` 的 `x`，兼作 `::` 前段判定）都不是绑定。
fn is_pattern_binding(tokens: &[rustc_lexer::Token], i: usize, text: &str) -> bool {
    if text == "mut" || text == "ref" {
        return false;
    }
    if let Some(n) = next_sig_index(tokens, i) {
        match tokens[n].kind {
            TokenKind::Colon
            | TokenKind::OpenParen
            | TokenKind::OpenBrace
            | TokenKind::OpenBracket => return false,
            _ => {}
        }
    }
    // 前是 `::` 后段（前两个有意义 token 都是 Colon）→ 路径段，不是绑定
    if let Some(p) = prev_sig_index(tokens, i)
        && tokens[p].kind == TokenKind::Colon
        && let Some(pp) = prev_sig_index(tokens, p)
        && tokens[pp].kind == TokenKind::Colon
    {
        return false;
    }
    true
}

/// i 之前（不含 i）第一个有意义 token 的索引（跳过空白/注释）
fn prev_sig_index(tokens: &[rustc_lexer::Token], i: usize) -> Option<usize> {
    tokens[..i].iter().rposition(|t| !is_trivia_token(t.kind))
}

/// i 之后（不含 i）第一个有意义 token 的索引（跳过空白/注释）
fn next_sig_index(tokens: &[rustc_lexer::Token], i: usize) -> Option<usize> {
    tokens[i + 1..]
        .iter()
        .position(|t| !is_trivia_token(t.kind))
        .map(|p| p + i + 1)
}

/// 空白/注释（trivia）：不参与相邻性判断
fn is_trivia_token(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
    )
}

/// 将源码中的中文别名替换为英文标识符（仅替换标识符 token）
///
/// 输入应为已完成关键字转译的代码（声明关键字已是英文形式）。
/// 声明位的标识符（如 `fn 绝对值()`、`让 字符串 = 1`）保留原样，
/// 且这些用户声明名在文件内的使用处也不替换；`::` 限定后的路径段
/// 仅对「变量名」不豁免（如 `字符串::新建` 里的 `新建` 是库 API，
/// 照常替换），用户声明的项可经 `::` 访问（`接口错误::错误请求`），
/// 仍豁免。库特征实现块内的方法名是库 API 名称，照常替换（见模块文档）。
///
/// # 参数
/// - `source`: 待替换的源代码字符串
/// - `alias_map`: 中文别名 → 英文原名 的映射表
///
/// # 返回
/// 替换后的源代码字符串
pub fn replace_aliases(source: &str, alias_map: &HashMap<String, String>) -> String {
    replace_aliases_with_map(source, alias_map).output
}

/// 同 [`replace_aliases`]，同时产出编辑表（本阶段输入坐标）
pub fn replace_aliases_with_map(
    source: &str,
    alias_map: &HashMap<String, String>,
) -> ReplaceResult {
    replace_aliases_with_context(source, alias_map, None)
}

/// 同 [`replace_aliases_with_map`]，附项目级声明上下文（跨文件声明豁免）
///
/// `project` 为 None 时行为与 [`replace_aliases_with_map`] 完全一致。
/// 提供上下文时在逐文件豁免之上额外豁免：
/// - 项目声明名（含其他文件声明的项/结构体字段）的裸使用处；
/// - 项目模块名的裸使用处；
/// - 以 `crate`/`super`/`self`/项目模块名/项目项名为根的 `::` 路径链：
///   链内**与项目声明名同名**的段豁免（`crate::平台Linux::Linux内存源::新建`
///   ——跨文件引用成员与声明侧一致，避免 E0599）；链内非项目名
///   （`源码映射项::新建` 的 `新建` 未定义于项目）照常替换。
///   `::` 后非项目根的段（`盒子::新建`）同样照常替换：`新建` 是库 API。
pub fn replace_aliases_with_context(
    source: &str,
    alias_map: &HashMap<String, String>,
    project: Option<&ProjectContext>,
) -> ReplaceResult {
    // 映射表为空时直接返回，避免不必要的词法分析开销
    if alias_map.is_empty() {
        return ReplaceResult {
            output: source.to_string(),
            edits: Vec::new(),
        };
    }
    // 第一遍：收集用户声明名与库特征方法名（使用处豁免见 DeclaredNames）
    let declared = collect_declared_names(source, alias_map);
    // 单个 Colon 跟踪：rustc_lexer 把 `::` 拆成两个 Colon token
    let mut last_was_colon = false;
    // token 序列化收集：`.方法()` 判定需向后看一位（下一个有意义 token
    // 是否为 `(`），字段访问 `实例.字段` 与方法调用 `实例.方法()` 的
    // 豁免集合不同
    let tokens: Vec<rustc_lexer::Token> = tokenize(source).collect();
    let mut output = String::new();
    let mut edits = Vec::new();
    let mut current_offset = 0;
    // 上一个有意义 token 若是声明关键字，记录其文本（`mut` 透明传递）；
    // 空白/注释不重置该状态
    let mut prev_decl: Option<&'static str> = None;
    // 上一个有意义 token 是否为 `::`（两个连续 Colon token 的第二个）
    let mut prev_is_path_sep = false;
    // 上一个有意义 token 是否为 `.`（成员访问/方法链）
    let mut prev_is_dot = false;
    // —— 项目路径链跟踪（仅 project 为 Some 时生效）——
    // pending_project_root：上一标识符是项目根候选（crate/模块名/项名），
    // 等待 `::` 确认；in_project_path：根段已确认，链内段按项目声明名豁免
    let mut pending_project_root = false;
    let mut in_project_path = false;

    for (index, token) in tokens.iter().enumerate() {
        let len = token.len;
        let text = &source[current_offset..current_offset + len];
        match token.kind {
            TokenKind::Ident => {
                // `.方法()` 位：`.` 后的标识符且下一个有意义 token 是 `(`
                //（字段名不是方法名，豁免集合见下方 usage_exempt）
                let is_method_pos = prev_is_dot && next_significant_is_open_paren(&tokens, index);
                // 项目路径链：根段后跟 `::` 时链内段按项目声明名豁免。
                // 根段确认依赖上一标识符留存的候选标记与当前 `::` 状态
                let in_project_chain = project.is_some()
                    && (in_project_path || (pending_project_root && prev_is_path_sep));
                if in_project_chain {
                    in_project_path = true;
                }
                // 声明位保护：紧随声明关键字的名字是用户自己的定义；
                // 例外——库特征实现块内 `fn` 后的方法名是库 API 名称
                //（见 collect_library_impl_fns），照常参与替换
                let decl_protected = prev_decl.is_some()
                    && !(prev_decl == Some("fn") && declared.library_fns.contains(text));
                // 各位置豁免集合：
                // - 链内：仅项目声明名（项/模块）——`平台Linux::新建` 豁免，
                //   未定义于项目的 `源码映射项::新建` 照常替换（与教程
                //   `fn new` 定义侧一致）；
                // - `::` 后非链（根非项目根）：无项目上下文时保留旧行为
                //   （本文件项名豁免，`接口错误::错误请求`）；有上下文时
                //   一律按库 API 替换（`盒子::新建` 的同名项目项不得误伤）；
                // - `.方法()` 位：仅项目方法名（项集合）——`编辑表.长度()`
                //   的 `长度` 是 `Vec::len` 调用，字段名豁免不适用于方法位；
                // - 其他：本文件声明名 + 项目裸使用处豁免（含结构体字段）
                let usage_exempt = if in_project_chain {
                    declared.items.contains(text)
                        || project
                            .is_some_and(|p| p.items.contains(text) || p.modules.contains(text))
                } else if prev_is_path_sep {
                    project.is_none() && declared.items.contains(text)
                } else if is_method_pos {
                    declared.items.contains(text) || project.is_some_and(|p| p.items.contains(text))
                } else {
                    declared.items.contains(text)
                        || declared.variables.contains(text)
                        || declared.members.contains(text)
                        // 项目声明名（项+字段，含其他文件声明）与项目根词
                        //（模块名；crate/super/self 为英文，替换表不会命中，
                        // 在此仅作为路径链起点）
                        || project.is_some_and(|p| {
                            p.naked_exempt(text) || p.is_path_root(text)
                        })
                };
                if decl_protected || usage_exempt {
                    // 声明位标识符或用户声明名的使用处：保留原样
                    output.push_str(text);
                } else if let Some(english) = alias_map.get(text) {
                    edits.push(SourceMapEntry::new(current_offset, len, text, english));
                    output.push_str(english);
                } else {
                    output.push_str(text);
                }
                // 根段候选判定（基于母语原文；`包`等已在词法阶段转为英文）
                pending_project_root = project.is_some_and(|p| p.is_path_root(text));
                // `让 mut 名称`：声明位内的 mut 透明传递声明状态；
                // `&mut 类型` 等非声明位的 mut 不传递，库类型引用仍被替换
                prev_decl = next_decl_keyword(prev_decl, text);
                prev_is_path_sep = false;
                prev_is_dot = false;
            }
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. } => {
                output.push_str(text);
                // 空白与注释不打断声明状态、路径限定状态与成员访问状态
            }
            _ => {
                output.push_str(text);
                // 符号终结声明位；连续两个 Colon 构成 `::` 路径限定
                prev_decl = None;
                let is_colon = matches!(token.kind, TokenKind::Colon);
                // 非 Colon 符号终结项目路径链（`::` 的冒号保持链状态）
                if !is_colon {
                    in_project_path = false;
                    pending_project_root = false;
                }
                prev_is_path_sep = if is_colon {
                    prev_is_path_sep || last_was_colon
                } else {
                    false
                };
                last_was_colon = is_colon;
                prev_is_dot = matches!(token.kind, TokenKind::Dot);
            }
        }
        current_offset += len;
    }
    ReplaceResult { output, edits }
}

/// 下一个有意义 token（跳过空白/注释）是否为 `(`
///
/// 判定标识符处于「方法调用位」（`实例.方法()`）而非「字段访问位」
///（`实例.字段`）——两者豁免集合不同（字段名不是方法名）。
/// 行选择上限：rustc_lexer 的 `Token` 仅含 kind/len，`tokens` 与源码
/// 对齐，索引索引即偏移序位。
fn next_significant_is_open_paren(tokens: &[rustc_lexer::Token], index: usize) -> bool {
    tokens
        .get(index + 1..)
        .unwrap_or_default()
        .iter()
        .find(|t| {
            !matches!(
                t.kind,
                TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
            )
        })
        .is_some_and(|t| matches!(t.kind, TokenKind::OpenParen))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alias_map() -> HashMap<String, String> {
        HashMap::from([
            ("绝对值".to_string(), "abs".to_string()),
            ("字符串".to_string(), "String".to_string()),
        ])
    }

    #[test]
    fn test_method_call_and_type_position_replaced() {
        let out = replace_aliases("let s: 字符串 = x.绝对值();", &alias_map());
        assert_eq!(out, "let s: String = x.abs();");
    }

    #[test]
    fn test_declaration_sites_preserved() {
        let out = replace_aliases("fn 绝对值() {}\nlet 字符串 = 1;", &alias_map());
        assert_eq!(out, "fn 绝对值() {}\nlet 字符串 = 1;");
    }

    #[test]
    fn test_declared_name_usage_sites_preserved() {
        // 用户声明名在使用处同样豁免：声明位保护不能只保护声明点
        let out = replace_aliases("let 字符串 = 1;\nlet y = 字符串 + 2;", &alias_map());
        assert_eq!(out, "let 字符串 = 1;\nlet y = 字符串 + 2;");
    }

    #[test]
    fn test_undeclared_alias_usage_still_replaced() {
        // 无用户声明撞名时，使用处照常替换
        let out = replace_aliases("let y = 字符串::from(x);", &alias_map());
        assert_eq!(out, "let y = String::from(x);");
    }

    #[test]
    fn test_qualified_path_segment_not_exempt() {
        // `::` 限定后的路径段是库 API 访问，不受用户声明名豁免；
        // 裸使用处（第一行声明与第三行变量引用）仍豁免
        let map = HashMap::from([
            ("字符串".to_string(), "String".to_string()),
            ("新建".to_string(), "new".to_string()),
        ]);
        let out = replace_aliases(
            "let 新建 = 5;\nlet s = 字符串::新建();\nlet y = 新建;",
            &map,
        );
        assert_eq!(out, "let 新建 = 5;\nlet s = String::new();\nlet y = 新建;");
    }

    #[test]
    fn test_struct_and_const_declaration_preserved() {
        let out = replace_aliases("struct 字符串;\nconst 绝对值: i32 = 1;", &alias_map());
        assert_eq!(out, "struct 字符串;\nconst 绝对值: i32 = 1;");
    }

    #[test]
    fn test_let_mut_declaration_preserved() {
        // mut 透明传递声明状态，变量名仍受保护
        let out = replace_aliases("let mut 绝对值 = 1;", &alias_map());
        assert_eq!(out, "let mut 绝对值 = 1;");
    }

    #[test]
    fn test_ref_mut_type_still_replaced() {
        // `&mut 类型` 是非声明位，库类型引用仍应替换
        let out = replace_aliases("fn f(x: &mut 字符串) {}", &alias_map());
        assert_eq!(out, "fn f(x: &mut String) {}");
    }

    #[test]
    fn test_fn_param_binding_preserved() {
        // 函数参数名是值绑定（用户命名）：声明与使用处均保留，
        // 与格式化串中的 `{路径}` 保持一致（避免 E0425）
        let map = HashMap::from([("路径".to_string(), "Path".to_string())]);
        let src = "fn 完整网址(路径: &str) -> String { format!(\"{路径}\") }";
        let out = replace_aliases(src, &map);
        assert_eq!(out, src);
    }

    #[test]
    fn test_fn_param_type_position_still_replaced() {
        // 参数「类型」位置的映射词照常替换，只保护名字
        let map = HashMap::from([
            ("路径".to_string(), "Path".to_string()),
            ("字符串".to_string(), "String".to_string()),
        ]);
        let out = replace_aliases("fn f(路径: &字符串) {}", &map);
        assert_eq!(out, "fn f(路径: &String) {}");
    }

    #[test]
    fn test_closure_param_binding_preserved() {
        let map = HashMap::from([("值".to_string(), "values".to_string())]);
        let out = replace_aliases("let f = |值| 值 + 1;", &map);
        assert_eq!(out, "let f = |值| 值 + 1;");
    }

    #[test]
    fn test_move_closure_param_binding_preserved() {
        let map = HashMap::from([("值".to_string(), "values".to_string())]);
        let out = replace_aliases("let f = move |值| 值 + 1;", &map);
        assert_eq!(out, "let f = move |值| 值 + 1;");
    }

    #[test]
    fn test_bitwise_or_not_treated_as_closure() {
        // `a | b` 的 `|` 前是表达式结尾，不是闭包起始；`值` 照常替换
        let map = HashMap::from([("值".to_string(), "values".to_string())]);
        let out = replace_aliases("let r = a | 值;", &map);
        assert_eq!(out, "let r = a | values;");
    }

    #[test]
    fn test_logical_or_not_treated_as_closure() {
        // `a || 值` 是两个连续的 `|` token：第二个不得判定为闭包起始
        let map = HashMap::from([("值".to_string(), "values".to_string())]);
        let out = replace_aliases("let r = a || 值;", &map);
        assert_eq!(out, "let r = a || values;");
    }

    #[test]
    fn test_empty_closure_params_not_treated_as_binding() {
        // `|| 值` 是零参数闭包：`值` 不是绑定名，照常替换
        let map = HashMap::from([("值".to_string(), "values".to_string())]);
        let out = replace_aliases("let f = || 值;", &map);
        assert_eq!(out, "let f = || values;");
    }

    #[test]
    fn test_comment_between_decl_keyword_and_name() {
        let out = replace_aliases("fn /* 说明 */ 绝对值() {}", &alias_map());
        assert_eq!(out, "fn /* 说明 */ 绝对值() {}");
    }

    #[test]
    fn test_impl_target_still_replaced() {
        // impl 后的特征/类型多为库 API，不在保护名单内
        let out = replace_aliases("impl 字符串 for 客户端 {}", &alias_map());
        assert_eq!(out, "impl String for 客户端 {}");
    }

    #[test]
    fn test_user_item_qualified_path_exempt() {
        // 用户声明的项可经 `::` 访问：调用处仍豁免，与定义侧一致（避免 E0599）
        let map = HashMap::from([
            ("错误请求".to_string(), "bad_request".to_string()),
            ("未找到".to_string(), "not_found".to_string()),
        ]);
        let src = "struct 接口错误;\nimpl 接口错误 {\n    fn 错误请求() {}\n    fn 未找到() {}\n}\nlet e = 接口错误::错误请求();";
        let out = replace_aliases(src, &map);
        assert_eq!(out, src);
    }

    #[test]
    fn test_library_trait_impl_method_replaced() {
        // 库特征实现块内的方法名是库 API 名称，照常替换（避免 E0407/E0046）
        let map = HashMap::from([
            ("抄写器".to_string(), "Scribe".to_string()),
            ("渲染".to_string(), "render".to_string()),
            ("响应".to_string(), "Response".to_string()),
        ]);
        let out = replace_aliases(
            "impl 抄写器 for 接口错误 {\n    fn 渲染(self, r: &mut 响应) { r.渲染(); }\n}",
            &map,
        );
        assert_eq!(
            out,
            "impl Scribe for 接口错误 {\n    fn render(self, r: &mut Response) { r.render(); }\n}"
        );
    }

    #[test]
    fn test_user_trait_impl_method_preserved() {
        // 用户自定义特征（非映射词）的实现：方法名是用户取的，保持保护；
        // 特征声明与实现均保留中文
        let map = HashMap::from([("描述".to_string(), "describe".to_string())]);
        let src = "trait 可显示 {\n    fn 描述(&self);\n}\nimpl 可显示 for 狗 {\n    fn 描述(&self) {}\n}\n狗.描述();";
        let out = replace_aliases(src, &map);
        assert_eq!(out, src);
    }

    #[test]
    fn test_string_literal_and_comment_untouched() {
        let out = replace_aliases("println!(\"绝对值\"); // 绝对值", &alias_map());
        assert_eq!(out, "println!(\"绝对值\"); // 绝对值");
    }

    #[test]
    fn test_struct_field_names_preserved() {
        // 结构体字段名是用户命名（撞 `值`=values 映射键）：定义/构造/访问处
        // 全豁免；字段不可经 `::` 访问故收作值绑定
        let map = HashMap::from([("值".to_string(), "values".to_string())]);
        let src = "struct 容器 { 值: i32 }\nlet c = 容器 { 值: 1 };\nlet x = c.值;";
        let out = replace_aliases(src, &map);
        assert_eq!(out, src);
    }

    #[test]
    fn test_enum_variant_names_preserved() {
        // 枚举变体名是用户命名（撞 `保留`=retain、`丢弃`=drop 映射键）：
        // 定义与 `::` 限定使用处均豁免（变体可经 `::` 访问，收作项）
        let map = HashMap::from([
            ("保留".to_string(), "retain".to_string()),
            ("丢弃".to_string(), "drop".to_string()),
        ]);
        let src = "enum 判定结果 { 保留, 丢弃 }\nlet r = 判定结果::保留;";
        let out = replace_aliases(src, &map);
        assert_eq!(out, src);
    }

    #[test]
    fn test_enum_variant_struct_body_fields_preserved() {
        // 变体携带的结构体字段同受豁免；反例——无声明时映射词照常替换
        let map = HashMap::from([("内容".to_string(), "contents".to_string())]);
        let src = "enum 消息 { 退出, 写入 { 内容: String } }\nlet m = 消息::写入 { 内容: String::new() };";
        let out = replace_aliases(src, &map);
        assert_eq!(out, src);
        let out2 = replace_aliases("let x = 内容 + 1;", &map);
        assert_eq!(out2, "let x = contents + 1;");
    }

    #[test]
    fn test_pub_struct_field_preserved() {
        // pub 与 pub(crate) 可见性修饰的字段名收集（可见性修饰不消耗
        // 成员开始位，括号内标识符不参与收集）
        let map = HashMap::from([
            ("值".to_string(), "values".to_string()),
            ("计数".to_string(), "count".to_string()),
        ]);
        let src = "struct 容器 { pub 值: i32, pub(crate) 计数: u32 }";
        let out = replace_aliases(src, &map);
        assert_eq!(out, src);
    }

    #[test]
    fn test_generic_and_attribute_members_not_miscollected() {
        // 泛型参数 `乙` 不得被误收为字段（无声明则照常替换）
        let map = HashMap::from([
            ("甲".to_string(), "first".to_string()),
            ("乙".to_string(), "second".to_string()),
        ]);
        let src = "struct 容器 { 字段: 映射<甲, 乙> }";
        let out = replace_aliases(src, &map);
        assert_eq!(out, "struct 容器 { 字段: 映射<first, second> }");
    }

    /// 项目上下文夹具：模块 `平台Linux`，声明名 `新建`（weix-1 场景）
    fn project_ctx() -> ProjectContext {
        ProjectContext {
            modules: HashSet::from(["平台Linux".to_string()]),
            names: HashSet::from(["新建".to_string()]),
            items: HashSet::from(["新建".to_string()]),
        }
    }

    #[test]
    fn test_project_context_cross_file_call_preserved() {
        // 跨文件调用（#8）：`新建` 在本文件的转译单元无声明，由项目上下文
        // 豁免（否则被替换出 `new` → E0599，与声明侧 `fn 新建` 不一致）
        let map = HashMap::from([
            ("新建".to_string(), "new".to_string()),
            ("平台Linux".to_string(), "platform_linux".to_string()),
        ]);
        let ctx = project_ctx();
        let src = "let e = crate::平台Linux::Linux内存源::新建();";
        let out = replace_aliases_with_context(src, &map, Some(&ctx));
        assert_eq!(out.output, src);
        // 无项目上下文（旧行为）时模块名与成员都被替换
        let out2 = replace_aliases_with_context(src, &map, None);
        assert_eq!(
            out2.output,
            "let e = crate::platform_linux::Linux内存源::new();"
        );
    }

    #[test]
    fn test_project_context_module_root_chain() {
        // 模块名为根的路径链（无 crate 前缀）链内全豁免；
        // 模块名自身的裸使用处同样豁免（模块词不落入别名替换）
        let map = HashMap::from([
            ("新建".to_string(), "new".to_string()),
            ("平台Linux".to_string(), "platform_linux".to_string()),
        ]);
        let ctx = project_ctx();
        let src = "let a = 平台Linux::Linux内存源::新建();\nlet b = 平台Linux;";
        let out = replace_aliases_with_context(src, &map, Some(&ctx));
        assert_eq!(out.output, src);
    }

    #[test]
    fn test_project_context_library_path_segment_still_replaced() {
        // 非同模块的库路径段不受项目上下文影响：`盒子::新建` 的 `新建`
        // 是库 API（项目内同名 `fn 新建` 不得误伤）
        let map = HashMap::from([
            ("新建".to_string(), "new".to_string()),
            ("盒子".to_string(), "Box".to_string()),
        ]);
        let ctx = project_ctx();
        let out = replace_aliases_with_context("let b = 盒子::新建();", &map, Some(&ctx));
        assert_eq!(out.output, "let b = Box::new();");
    }

    #[test]
    fn test_project_context_naked_usage_exempt() {
        // 项目声明名（其他文件声明）的裸使用处豁免；
        // `::` 链外的普通表达式不受影响
        let map = HashMap::from([
            ("新建".to_string(), "new".to_string()),
            ("计算".to_string(), "calculate".to_string()),
        ]);
        let ctx = project_ctx();
        let out = replace_aliases_with_context("let x = 新建 + 计算;", &map, Some(&ctx));
        assert_eq!(out.output, "let x = 新建 + calculate;");
    }

    #[test]
    fn test_project_context_chain_ends_at_symbol() {
        // 路径链在非 Colon 符号处终结：链后的普通标识符照常替换
        let map = HashMap::from([
            ("新建".to_string(), "new".to_string()),
            ("计算".to_string(), "calculate".to_string()),
        ]);
        let ctx = project_ctx();
        let out = replace_aliases_with_context(
            "let e = crate::平台Linux::新建() + 计算;",
            &map,
            Some(&ctx),
        );
        assert_eq!(out.output, "let e = crate::平台Linux::新建() + calculate;");
    }

    /// 链内未定义于项目的成员照常替换（教程彩蛋块回归）：
    /// `源码映射项` 是项目声明的结构体，`新建` 不是项目声明的方法名
    ///（项目定义的是 `new`）——调用位必须替换出 `new`，与声明位一致；
    /// 同时验证方法调用位不查字段名（`编辑表.长度()` → `len`），
    /// 字段访问位仍豁免（`实例.长度` 保持）
    #[test]
    fn test_project_context_undeclared_member_and_method_pos() {
        let map = HashMap::from([
            ("新建".to_string(), "new".to_string()),
            ("源码映射项".to_string(), "SourceMapEntry".to_string()),
            ("推入".to_string(), "push".to_string()),
            ("长度".to_string(), "len".to_string()),
        ]);
        // 项目声明：结构体 `源码映射项`（项）+ 字段 `长度`（成员名，非项）；
        // `new` 为方法名（项）；不含 `新建`
        let ctx = ProjectContext {
            modules: HashSet::from(["替换模块路径".to_string()]),
            names: HashSet::from([
                "源码映射项".to_string(),
                "长度".to_string(),
                "new".to_string(),
            ]),
            items: HashSet::from(["源码映射项".to_string(), "new".to_string()]),
        };
        let src = "编辑表.推入(源码映射项::新建(当前偏移, 令牌长));\nlet n = 编辑表.长度();\nlet m = 实例.长度;";
        let out = replace_aliases_with_context(src, &map, Some(&ctx));
        assert_eq!(
            out.output,
            "编辑表.push(源码映射项::new(当前偏移, 令牌长));\nlet n = 编辑表.len();\nlet m = 实例.长度;"
        );
    }

    #[test]
    fn test_project_context_fingerprint_deterministic() {
        // 集合迭代顺序不影响指纹（排序后哈希）：跨进程缓存稳定
        let a = ProjectContext {
            modules: HashSet::from(["甲".to_string(), "乙".to_string()]),
            names: HashSet::from(["丙".to_string(), "丁".to_string()]),
            items: HashSet::from(["丙".to_string()]),
        };
        let b = ProjectContext {
            modules: HashSet::from(["乙".to_string(), "甲".to_string()]),
            names: HashSet::from(["丁".to_string(), "丙".to_string()]),
            items: HashSet::from(["丙".to_string()]),
        };
        assert_eq!(a.fingerprint(), b.fingerprint());
        let c = ProjectContext {
            modules: HashSet::from(["甲".to_string()]),
            names: HashSet::from(["丙".to_string()]),
            items: HashSet::new(),
        };
        assert_ne!(a.fingerprint(), c.fingerprint());
    }

    #[test]
    fn test_project_context_from_sources_collects_items_and_members() {
        // 项目扫描：项名与结构体字段都并入声明名，字段不入项名；
        // 源文件为母语原文（内部先跑词法转译再收集声明）
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("lang-packs/zh");
        let manager = crate::mapping_manager::MappingManager::load_from_dir(&dir)
            .expect("加载 zh 语言包失败");
        let ctx = ProjectContext::from_sources(
            HashSet::from(["甲".to_string()]),
            ["公开 函数 新建() {}\n结构体 配置 { pub 超时: u32 }"],
            &manager,
        );
        assert!(ctx.names.contains("新建"));
        assert!(ctx.names.contains("超时"));
        assert!(ctx.items.contains("新建"));
        assert!(!ctx.items.contains("超时"));
        assert!(ctx.modules.contains("甲"));
        assert!(!ctx.is_empty());
        assert!(ProjectContext::default().is_empty());
    }

    #[test]
    fn test_empty_map_returns_original() {
        assert_eq!(replace_aliases("任意内容", &HashMap::new()), "任意内容");
    }

    #[test]
    fn test_match_arm_binding_preserved() {
        // match 臂模式中的绑定名是值绑定：`连接`（撞 join 映射键）
        // 声明与使用处均豁免
        let map = HashMap::from([("连接".to_string(), "join".to_string())]);
        let src = "match 甲 { Some(连接) => 连接 + 1, None => 0 }";
        let out = replace_aliases(src, &map);
        assert_eq!(out, src);
    }

    #[test]
    fn test_match_arm_path_segment_still_replaced() {
        // 对照：`::` 路径段与后跟 `(` 的构造路径名不是绑定，照常替换
        let map = HashMap::from([
            ("连接".to_string(), "join".to_string()),
            ("数据行".to_string(), "Row".to_string()),
        ]);
        let out = replace_aliases("match 甲 { 错误::连接 => 0, 数据行(乙) => 1 }", &map);
        assert_eq!(out, "match 甲 { 错误::join => 0, Row(乙) => 1 }");
    }

    #[test]
    fn test_match_arm_struct_pattern_bindings() {
        // 结构体模式：路径名（后跟 `{`）照常替换；字段绑定（`x: 值`
        // 的 `值`）与缩写绑定（`y`）收为值绑定，使用处豁免
        let map = HashMap::from([
            ("点".to_string(), "Point".to_string()),
            ("值".to_string(), "values".to_string()),
        ]);
        let src = "match 甲 { 点 { x: 值, y: 乙 } => 值 + 乙 }";
        let out = replace_aliases(src, &map);
        assert_eq!(out, "match 甲 { Point { x: 值, y: 乙 } => 值 + 乙 }");
    }

    #[test]
    fn test_match_arm_field_name_still_replaced() {
        // 字段名（后跟 `:`）不是绑定：库字段名照常替换
        let map = HashMap::from([("值".to_string(), "values".to_string())]);
        let out = replace_aliases("match 甲 { 点 { 值: 乙 } => 乙 }", &map);
        assert_eq!(out, "match 甲 { 点 { values: 乙 } => 乙 }");
    }

    #[test]
    fn test_match_arm_with_guard_binding_preserved() {
        // 守卫表达式中的名字同为值使用，一并收集
        let map = HashMap::from([("连接".to_string(), "join".to_string())]);
        let src = "match 甲 { Some(连接) if 连接 > 0 => 连接, _ => 0 }";
        let out = replace_aliases(src, &map);
        assert_eq!(out, src);
    }

    #[test]
    fn test_match_arm_block_body_does_not_leak_to_next_arm() {
        // 缺陷回归（weix-1 #27）：带块臂体（`=> { … }`）的下一臂回溯
        // 曾越过块闭合 `}`，`;` 深度失衡后不再终止 → 穿透上一臂语句，
        // 把类型标注等裸标识符误收为值绑定（全文件豁免替换）：
        // `无符号机器整数`（stdlib 别名 = usize）因此不被翻译（E0425）
        let map = HashMap::from([("无符号机器整数".to_string(), "usize".to_string())]);
        let src = "match 乙 { Ok(报告值) => { let 库总数: 无符号机器整数 = 报告值; 库总数 } Err(_) => 0 }";
        let out = replace_aliases(src, &map);
        assert_eq!(
            out,
            "match 乙 { Ok(报告值) => { let 库总数: usize = 报告值; 库总数 } Err(_) => 0 }"
        );
    }

    #[test]
    fn test_match_arm_block_body_bindings_still_preserved() {
        // 修复不误伤：带块臂体的模式绑定仍被收集（体块 `}` 即模式左界），
        // 绑定名与使用处照常豁免
        let map = HashMap::from([("连接".to_string(), "join".to_string())]);
        let src = "match 甲 { Ok(连接) => { let n = 连接; n } Err(_) => 0 }";
        let out = replace_aliases(src, &map);
        assert_eq!(out, src);
    }

    #[test]
    fn test_let_destructure_binding_preserved() {
        // let 解构括号内的名字是绑定：声明与使用处豁免
        let map = HashMap::from([("连接".to_string(), "join".to_string())]);
        let src = "let (连接, 乙) = 对; let s = 连接;";
        let out = replace_aliases(src, &map);
        assert_eq!(out, src);
    }

    #[test]
    fn test_if_let_binding_preserved() {
        // if-let 与 let 链同形覆盖：`有值(连接)` 内为绑定
        let map = HashMap::from([("连接".to_string(), "join".to_string())]);
        let src = "if let Some(连接) = 甲 { let y = 连接; }";
        let out = replace_aliases(src, &map);
        assert_eq!(out, src);
    }

    #[test]
    fn test_for_pattern_binding_preserved() {
        // for 模式（解构/引用形式）内的名字是绑定；使用处豁免
        let map = HashMap::from([
            ("值".to_string(), "values".to_string()),
            ("长度".to_string(), "len".to_string()),
        ]);
        let src = "for (键, 值) in 映射 { let z = 值; }\nfor &长度 in 长度们 {}";
        let out = replace_aliases(src, &map);
        assert_eq!(out, src);
    }

    #[test]
    fn test_for_binding_only_until_in() {
        // `for` 与 `in` 之间的名字是绑定；`in` 之后的迭代源照常替换
        let map = HashMap::from([
            ("项".to_string(), "item".to_string()),
            ("迭代".to_string(), "iter".to_string()),
        ]);
        let out = replace_aliases("for 项 in 容器.迭代() { let z = 项; }", &map);
        assert_eq!(out, "for 项 in 容器.iter() { let z = 项; }");
    }

    #[test]
    fn test_impl_for_target_not_treated_as_binding() {
        // `impl … for 类型 {` 的 for 是 impl 语法：目标类型名照常替换，
        // 不被当作 for 模式绑定收集
        let map = HashMap::from([("渲染器".to_string(), "Renderer".to_string())]);
        let out = replace_aliases("impl 显示 for 渲染器 {}", &map);
        assert_eq!(out, "impl 显示 for Renderer {}");
    }

    #[test]
    fn test_pattern_keyword_without_binding_still_replaced() {
        // 对照：无模式绑定时，同词在值使用位照常替换
        let map = HashMap::from([("连接".to_string(), "join".to_string())]);
        let out = replace_aliases("let y = 连接;", &map);
        assert_eq!(out, "let y = join;");
    }
}
