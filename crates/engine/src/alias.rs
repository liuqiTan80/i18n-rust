//! 别名替换模块
//! 将源代码中的中文标识符别名（如第三方库的中文名称）替换为英文标识符。
//! 仅替换标识符类型的 token，不触碰字符串字面量和注释等内容。
//! 声明位保护：紧跟在声明关键字（fn/struct/let 等）后的标识符是用户自己的定义，
//! 不是库 API 引用，不参与别名替换；且用户声明的名字在整个文件内的
//! 裸使用处都豁免（两遍扫描：先收集声明名，再逐 token 替换），
//! 避免 `let 新建 = 5` 声明位受保护而后续使用处被误替换成 new。
//! 三条精细规则：
//! 1. `::` 限定后的路径段按「链根」分级处理：根是项目符号
//!    （crate/self/super/项目模块名/项目项名）时链内**与项目声明名
//!    同名**的段豁免——`平台Linux::新建` 的定义侧与调用侧一致
//!    （避免 E0599）；链内非项目名与根非项目符号的段都照常替换
//!    （`源码映射项::新建` 的 `新建` 未定义于项目、`盒子::新建`
//!    的 `新建` 是库 API）；无项目上下文时退化为「本文件项名豁免」
//!    （`接口错误::错误请求`）；
//! 2. 库特征实现块（`impl <映射词特征> for 类型 {}`）内的方法名是库 API
//!    规定的名称（用户从映射表抄写而来），不是用户自定义名字，不受声明位
//!    保护——`实现 抄写器 对于 类型 { 函数 渲染（...） }` 中的 `渲染`
//!    必须转译为 `render`，否则无法匹配 `Scribe::render`（E0407/E0046）；
//! 3. 函数参数与闭包参数是「值绑定」（用户命名）：`fn 完整网址(路径: &str)`
//!    的 `路径` 在声明与使用处都豁免替换（参数「类型」位置的字照常替换），
//!    与格式化串中的 `{路径}` 保持一致，避免 E0425（找不到名称 `路径`）；
//! 4. 结构体字段名与枚举变体名同样是用户命名，纳入声明收集：
//!    字段 `struct 容器 { 值: i32 }` 的 `值`（撞 `值`=values 映射键）
//!    在定义/构造/访问处（`实例.值`）全豁免；枚举变体
//!    `enum 判定结果 { 保留, 丢弃 }` 的 `保留`/`丢弃`（撞
//!    retain/drop 映射键）同理；变体可经 `::` 访问
//!    （`判定结果::保留`）故收集为项，字段收集为值绑定。
//!    方法调用位（`实例.方法()`）不查字段名：`编辑表.长度()` 的
//!    `长度` 是 `Vec::len` 调用，只查「项目方法名集合」（项）；
//! 5. 模式绑定（match 臂、let 解构、for 模式）同样是值绑定：
//!    `匹配 有值(入参) { 有值(连接) => … }` 的 `连接`（撞
//!    `连接`=join 映射键）全文件豁免（见 collect_pattern_bindings）。

use rustc_lexer::TokenKind;
use std::collections::{HashMap, HashSet};

use crate::cache::SourceMapEntry;
use crate::name_cache::NameCache;

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

/// 用户声明名与库特征方法名的收集结果
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
        let 声明们: Vec<DeclaredNames> = sources
            .into_iter()
            .map(|source| collect_source_declarations(source, manager))
            .collect();
        Self::from_declarations(modules, 声明们.iter())
    }

    /// 同 [`Self::from_sources`]，但单文件声明收集结果由调用方提供
    ///
    /// 汇总规则唯一来源（[`Self::from_sources`] 亦委托本函数）：调用方可按
    /// 文件粒度缓存 [`collect_source_declarations`] 的结果（如 LSP 按内容哈希
    /// 缓存），避免同一内容被重复跑词法转译，同时无需复刻合并规则。
    pub fn from_declarations<'a>(
        modules: HashSet<String>,
        declarations: impl IntoIterator<Item = &'a DeclaredNames>,
    ) -> Self {
        let mut names = HashSet::new();
        let mut items = HashSet::new();
        for declared in declarations {
            for name in &declared.items {
                names.insert(name.clone());
                items.insert(name.clone());
            }
            names.extend(declared.members.iter().cloned());
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

/// 收集单个方言源文件的声明名（词法转译 + 声明扫描）
///
/// 与 [`ProjectContext::from_sources`] 内部的单文件收集步骤完全一致
/// （后者亦委托本函数，规则唯一来源），供需要按文件粒度缓存结果的调用方
/// 使用——例如 LSP 每按键都要汇总全部打开文件的声明名，若每次重算
/// 等于 O(项目规模) 次词法转译；按内容哈希缓存本函数的返回值后，
/// 每次按键只有内容真正变化的那个文件需要重算。
pub fn collect_source_declarations(
    source: &str,
    manager: &crate::mapping_manager::MappingManager,
) -> DeclaredNames {
    let lex = crate::lexer::transpile_with_map(
        source,
        manager.get_keyword_map(),
        manager.get_macro_map(),
        manager.get_derive_map(),
        manager.get_use_defer_words(),
        manager.get_method_defer_words(),
        manager.get_alias_map(),
    );
    collect_declared_names(&lex.output, manager.get_alias_map())
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
    let tokens = crate::collect_tokens(source);
    collect_declared_names_in(source, alias_map, &tokens)
}

/// [`collect_declared_names`] 的实现：调用方已持有 token 序列时直接复用，
/// 避免同一份源码被重复词法化
///
/// 管线内部也直接走本入口：延迟项目上下文的入口要对 `lex.output` 分词才能
/// 收集声明，而 use 路径阶段恰好也在这份文本上跑，两者共用同一份 token 流。
///
/// 声明收集的三个方面合并为**一次** token 遍历：
/// - 声明位（`fn`/`struct`/`let`/`const`/`static`… 后的名字）、类型体成员、
///   函数与闭包参数；
/// - 库特征实现块的方法名：`impl 特征 for 类型 { fn 方法 }` 中，特征名既不是
///   本文件声明的项、又在别名映射表里时，块内方法名是库 API（用户从映射表
///   抄写而来），必须照常参与替换；实现用户自定义特征时方法名是用户取的名字，
///   保持声明位保护；
/// - 模式绑定：match 臂（`=>` 向左回溯到臂起点）、let 解构括号内、`for` 到
///   `in` 之间的名字是用户命名的值绑定，与参数一样全文件豁免替换
///   （`成功(连接) =>` 的 `连接` 撞 `连接`=join 键，误替换即缺陷）。
///
/// 三组状态互不读对方的中途结果（库特征判定用的 `items` 只在整份源码扫完后
/// 才需要），所以能并在一起：每个 token 只取一次文本切片、只分派一次。此前
/// 三者各走一遍 token 流，98 KB / 2805 行样例声明收集 1684 → 1513 µs
/// （−10.2%，同进程交替 5/5 轮）；1193 份语料与旧实现逐字段零差异。
///
/// 但仍保留原来的**两阶段顺序**：库特征方法名要拿扫完后的完整 `items` 判定
/// 并统一剔除（impl 块可以出现在目标类型声明之前），模式绑定在那之后再并入。
pub(crate) fn collect_declared_names_in(
    source: &str,
    alias_map: &HashMap<String, String>,
    tokens: &[rustc_lexer::Token],
) -> DeclaredNames {
    let mut result = DeclaredNames::default();
    // —— 一：声明位 / 类型体 / 函数与闭包参数 ——
    let mut prev_decl: Option<&'static str> = None;
    let mut pending_type: Option<(bool, bool)> = None;
    let mut type_body: Option<TypeBodyScan> = None;
    let mut body_paren = 0u32;
    let mut body_angle = 0u32;
    let mut body_bracket = 0u32;
    let mut fn_head = 0u32;
    let mut fn_angle = 0u32;
    let mut paren_depth = 0u32;
    let mut at_param_start = false;
    let mut pending_param: Option<String> = None;
    let mut in_closure = false;
    let mut closure_parens = 0u32;
    let mut closure_start = false;
    let mut prev_value_end = false;
    let mut last_or_end = usize::MAX;
    // —— 二：impl 块（库特征判定延后到扫完之后）——
    // impl 块可以出现在目标类型声明之前，故不能边扫边按 `items` 判定；
    // 先把每块的（目标名, 块内 fn 名）记下，扫完后统一判定。名字用源码切片，不分配。
    let mut impl_header = false;
    let mut impl_target: Option<&str> = None;
    let mut impl_block: Option<usize> = None;
    let mut impl_angle = 0u32;
    let mut impl_brace = 0u32;
    let mut prev_is_fn = false;
    let mut impl_blocks: Vec<(&str, Vec<&str>)> = Vec::new();
    // —— 三：模式绑定 ——
    let mut let_ok = false;
    let mut let_depth = 0u32;
    let mut for_ok = false;
    let mut for_depth = 0u32;
    let mut for_pending: Vec<&str> = Vec::new();
    let mut in_impl_header = false;
    // 模式绑定单独暂存：库特征方法名的剔除只作用于声明位收集结果，
    // 旧实现是「先剔除、再把模式绑定并入 variables」，此处保持同一顺序
    let mut bindings: HashSet<String> = HashSet::new();
    let mut offset = 0usize;
    for i in 0..tokens.len() {
        let kind = tokens[i].kind;
        let text = &source[offset..offset + tokens[i].len];
        match kind {
            TokenKind::Ident => {
                // —— 一：类型头推进（struct/enum 后第一个标识符是类型名）——
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
                // —— 一：类型体成员收集 ——
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
                // —— 一：声明位 ——
                if let Some(keyword) = prev_decl {
                    if VAR_DECL_KEYWORDS.contains(&keyword) {
                        result.variables.insert(text.to_string());
                    } else {
                        result.items.insert(text.to_string());
                    }
                }
                prev_decl = next_decl_keyword(prev_decl, text);
                // —— 一：函数参数名 ——
                if fn_head == 2 && paren_depth == 1 && at_param_start && text != "mut" {
                    pending_param = Some(text.to_string());
                    at_param_start = false;
                }
                if fn_head == 0 && text == "fn" {
                    fn_head = 1;
                    fn_angle = 0;
                }
                // —— 一：闭包参数名 ——
                if in_closure && closure_parens == 0 && closure_start && text != "mut" {
                    result.variables.insert(text.to_string());
                    closure_start = false;
                }
                prev_value_end = text != "move";

                // —— 二：impl 块 ——
                if let Some(block) = impl_block {
                    // 块体内的 fn 名：紧跟在 `fn` 之后的标识符
                    if prev_is_fn {
                        impl_blocks[block].1.push(text);
                    }
                } else if impl_header {
                    // `<...>` 泛型参数里的名字不是 impl 目标
                    if impl_angle == 0 {
                        impl_header = false;
                        impl_target = Some(text);
                    }
                } else if impl_target.is_none() && text == "impl" {
                    impl_header = true;
                    impl_angle = 0;
                }
                prev_is_fn = text == "fn";

                // —— 三：模式绑定 ——
                if text == "impl" {
                    in_impl_header = true;
                }
                if for_ok {
                    if text == "in" && for_depth == 0 {
                        // 模式结束：提交挂起绑定
                        bindings.extend(for_pending.drain(..).map(str::to_string));
                        for_ok = false;
                    } else if for_depth == 0
                        && (text == "for" || STMT_BOUNDARY_KEYWORDS.contains(&text))
                    {
                        // 遇嵌套 for 重启挂起；遇声明/语句关键字丢弃挂起
                        for_pending.clear();
                        for_ok = text == "for";
                        for_depth = 0;
                    } else if is_pattern_binding(tokens, i, text) {
                        for_pending.push(text);
                    }
                } else if text == "for" && !in_impl_header {
                    for_ok = true;
                    for_depth = 0;
                    for_pending.clear();
                }
                if let_ok && let_depth > 0 && is_pattern_binding(tokens, i, text) {
                    bindings.insert(text.to_string());
                }
                if text == "let" {
                    let_ok = true;
                    let_depth = 0;
                }
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
                prev_is_fn = false;
                if let_ok {
                    let_depth += 1;
                }
                if for_ok {
                    for_depth += 1;
                }
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
                prev_is_fn = false;
                let_depth = let_depth.saturating_sub(1);
                for_depth = for_depth.saturating_sub(1);
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
                pending_param = None;
                prev_value_end = false;
                prev_is_fn = false;
            }
            TokenKind::Colon => {
                prev_decl = None;
                // `名:` → 确认函数参数绑定（其他位置不会有待确认候选）
                if let Some(name) = pending_param.take() {
                    result.variables.insert(name);
                }
                // `字段:` → 确认结构体字段名
                if let Some(body) = &mut type_body
                    && let Some(name) = body.pending_member.take()
                {
                    result.members.insert(name);
                }
                prev_value_end = false;
                prev_is_fn = false;
                // `let 名: 类型` 的 `:` 是类型标注分隔——其后是类型，不再是
                // 模式绑定；`::` 是路径分隔（两个相邻 Colon），不结束 let 态
                if let_ok
                    && let_depth == 0
                    && !prev_sig_index(tokens, i)
                        .is_some_and(|p| tokens[p].kind == TokenKind::Colon)
                    && !next_sig_index(tokens, i)
                        .is_some_and(|n| tokens[n].kind == TokenKind::Colon)
                {
                    let_ok = false;
                    let_depth = 0;
                }
                // for 模式里 `:` 不合法（类型标注等非循环场景，防御）
                if for_ok && for_depth == 0 {
                    for_pending.clear();
                    for_ok = false;
                }
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
                    in_closure = true;
                    closure_parens = 0;
                    closure_start = true;
                }
                last_or_end = offset + tokens[i].len;
                pending_param = None;
                prev_value_end = false;
                prev_is_fn = false;
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
                if impl_header {
                    impl_angle += 1;
                }
                prev_is_fn = false;
                // for 模式里 `<` 不合法（泛型等非循环场景，防御）
                if for_ok && for_depth == 0 {
                    for_pending.clear();
                    for_ok = false;
                }
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
                if impl_header {
                    impl_angle = impl_angle.saturating_sub(1);
                }
                prev_is_fn = false;
                // `=>`（Eq+Gt 相邻两 token）：match 臂箭头 → 回溯收集
                if let Some(p) = prev_sig_index(tokens, i)
                    && tokens[p].kind == TokenKind::Eq
                    && p == i - 1
                {
                    collect_match_arm_bindings(tokens, offset, i, source, &mut bindings);
                }
            }
            TokenKind::OpenBrace => {
                // `struct/enum 名 {` → 进入类型体成员扫描
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
                // `impl 目标 {` → 进入块体；体内嵌套花括号加深一层
                if let Some(target) = impl_target.take() {
                    impl_blocks.push((target, Vec::new()));
                    impl_block = Some(impl_blocks.len() - 1);
                    impl_brace = 0;
                } else if impl_block.is_some() {
                    impl_brace += 1;
                } else {
                    // `impl {`：非法写法，放弃跟踪
                    impl_header = false;
                }
                prev_is_fn = false;
                // impl 头部结束（进入块体）
                in_impl_header = false;
                if let_ok {
                    let_depth += 1;
                }
                if for_ok {
                    for_depth += 1;
                }
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
                // impl 块体闭合：嵌套深度归零时退出追踪
                if impl_block.is_some() {
                    if impl_brace == 0 {
                        impl_block = None;
                    } else {
                        impl_brace -= 1;
                    }
                }
                prev_is_fn = false;
                let_depth = let_depth.saturating_sub(1);
                for_depth = for_depth.saturating_sub(1);
            }
            TokenKind::OpenBracket => {
                // 方括号内的标识符不参与成员收集（属性、数组类型 `[T; N]`）
                if type_body.is_some() {
                    body_bracket += 1;
                }
                prev_decl = None;
                pending_param = None;
                prev_value_end = false;
                prev_is_fn = false;
                if let_ok {
                    let_depth += 1;
                }
                if for_ok {
                    for_depth += 1;
                }
            }
            TokenKind::CloseBracket => {
                if type_body.is_some() {
                    body_bracket = body_bracket.saturating_sub(1);
                }
                prev_decl = None;
                pending_param = None;
                prev_value_end = true;
                prev_is_fn = false;
                let_depth = let_depth.saturating_sub(1);
                for_depth = for_depth.saturating_sub(1);
            }
            _ => {
                prev_decl = None;
                pending_param = None;
                if matches!(kind, TokenKind::Semi) {
                    // `struct/enum 名;`（单元结构体）：放弃类型体等待
                    pending_type = None;
                    // 语句边界：清空全部模式状态
                    in_impl_header = false;
                    let_ok = false;
                    let_depth = 0;
                    for_pending.clear();
                    for_ok = false;
                    for_depth = 0;
                } else if matches!(kind, TokenKind::Eq) {
                    // `let 模式 =`：模式结束（`=` 之后是表达式）
                    if let_ok && let_depth == 0 {
                        let_ok = false;
                    }
                    // for 模式里 `=` 不合法（防御）
                    if for_ok && for_depth == 0 {
                        for_pending.clear();
                        for_ok = false;
                    }
                }
                prev_value_end = matches!(
                    kind,
                    TokenKind::Literal { .. }
                        | TokenKind::CloseBracket
                        | TokenKind::CloseBrace
                        | TokenKind::Question
                );
                prev_is_fn = false;
            }
        }
        offset += tokens[i].len;
    }
    // 库特征实现块：目标名既不是用户项、又在别名映射表中 → 块内方法名是库 API
    //（判定用扫完后的完整 items；先全部判定再统一剔除，与旧实现两阶段一致）
    let mut library_methods: Vec<&str> = Vec::new();
    for (target, methods) in &impl_blocks {
        if result.items.contains(*target) || !alias_map.contains_key(*target) {
            continue;
        }
        library_methods.extend(methods.iter().copied());
    }
    for method in library_methods {
        result.library_fns.insert(method.to_string());
        result.items.remove(method);
        result.variables.remove(method);
    }
    // 模式绑定最后并入：旧实现顺序为「先剔除库方法名、再并入模式绑定」
    result.variables.extend(bindings);
    result
}

/// 推进声明关键字状态：`mut` 透明传递，其余按是否声明关键字更新
fn next_decl_keyword(prev: Option<&'static str>, text: &str) -> Option<&'static str> {
    if text == "mut" {
        return prev;
    }
    DECL_KEYWORDS.iter().find(|&&k| k == text).copied()
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
    bindings: &mut HashSet<String>,
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
                    bindings.insert(text.to_string());
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
    replace_aliases_with_context_and_module_paths(source, alias_map, &HashMap::new(), project)
}

/// 别名阶段的名字级事实：一次解出主循环要问的那几张表的答案
///
/// 主循环对每个标识符都要查别名表、判定项目路径根（`modules`/`items` 两次
/// 哈希查找）、推进声明关键字状态（`DECL_KEYWORDS` 线性扫）。这三件事只取决于
/// 名字本身，因此按名字直映缓存（见 [`crate::name_cache`]）：同名重复出现时
/// 只付一次探测。模块路径表**不在**缓存内——它只在路径根位（后跟 `::`）才
/// 需要，而路径根位是少数，无条件预查反而会为每个标识符多搭一次随机访存。
#[derive(Clone, Copy)]
struct AliasFacts<'a> {
    /// 别名表命中值
    alias: Option<&'a str>,
    /// 项目路径根（crate/super/self/项目模块名/项目项名）；无项目上下文时恒 false
    path_root: bool,
    /// 声明位关键字（`mut` 除外：它的透明传递由调用处按上一个状态处理）
    decl_keyword: Option<&'static str>,
}

/// 同 [`replace_aliases_with_context`]，附模块路径表（use 之外的模块前缀转译）
///
/// 管线的「模块路径替换」阶段只处理 use 语句；表达式与属性位置的
/// `模块词::项`（`异步运行时::睡眠`、`#[异步运行时::主函数]`）在本函数内
/// 回退模块路径表完成（office-zb #10/#18：此前「模块词」保留、「项」被
/// 别名替换，产物 `异步运行时::main` 报 E0433）。回退规则：
/// - 仅**路径根位置**（后跟 `::`、且前不是 `::` 的段）触发；
/// - 别名表命中优先——标识符语义覆盖模块语义（`路径::新建` 的 `路径`
///   是 `Path` 而非 `path` 模块）；
/// - 路径根位上的字段/变量名豁免不适用（见 `usage_exempt` 的根位分支）；
/// - 替换值做 crate 名连字符规范化（`-` → `_`）。
pub fn replace_aliases_with_context_and_module_paths(
    source: &str,
    alias_map: &HashMap<String, String>,
    module_path_map: &HashMap<String, String>,
    project: Option<&ProjectContext>,
) -> ReplaceResult {
    replace_aliases_with_context_and_module_paths_using_declarations(
        source,
        alias_map,
        module_path_map,
        project,
        None,
    )
}

/// 同 [`replace_aliases_with_context_and_module_paths`]，但可复用调用方
/// 已收集好的声明名
///
/// 延迟项目上下文的管线入口（[`crate::transpile_pipeline_with_deferred_project`]）
/// 必需在别名阶段之前先拿到本文件声明才能建项目上下文，而它拿到的声明是
/// 从**词法产物**收集的；本阶段的输入则是「词法 → use 路径 → `crate::` 前缀」
/// 之后的文本。两处的声明集合相同（后续两个阶段只改 use 段路径与路径根位
/// 前缀，既不动声明关键字也不动声明位名字），因此可省掉本阶段内部那趟声明
/// 状态机（分词仍要为替换主循环付，省不掉）——359 行样例上该状态机 47.3 µs，
/// 占管线约 9%，现场收集整趟（含分词）则占 ~21%。
///
/// 等价性由 `tests/alias_declared_reuse.rs` 的差分测试锁定：两条入口对同一
/// 语料的 `output` 与 `edits` 逐字段一致。`declared` 为 `None` 时行为与原入口相同。
pub fn replace_aliases_with_context_and_module_paths_using_declarations(
    source: &str,
    alias_map: &HashMap<String, String>,
    module_path_map: &HashMap<String, String>,
    project: Option<&ProjectContext>,
    declared: Option<&DeclaredNames>,
) -> ReplaceResult {
    replace_aliases_reusing(source, alias_map, module_path_map, project, declared, None)
}

/// [`replace_aliases_with_context_and_module_paths_using_declarations`] 的实现，
/// 另可复用调用方已分好的 token 流
///
/// `tokens` 必须与 `source` 同一份文本（同一序列）。管线里本阶段的输入是
/// 「词法 → use 路径」之后的文本，那份 token 流可由「词法产物的 token + use 路径
/// 阶段各编辑的新长度」派生（见 `crate::rebase_tokens`），省掉整份重新分词——
/// 359 行样例上该分词 46~72 µs，占管线一成上下。
pub(crate) fn replace_aliases_reusing(
    source: &str,
    alias_map: &HashMap<String, String>,
    module_path_map: &HashMap<String, String>,
    project: Option<&ProjectContext>,
    declared: Option<&DeclaredNames>,
    tokens: Option<&[rustc_lexer::Token]>,
) -> ReplaceResult {
    // 映射表为空时直接返回，避免不必要的词法分析开销
    if alias_map.is_empty() && module_path_map.is_empty() {
        return ReplaceResult {
            output: source.to_string(),
            edits: Vec::new(),
        };
    }
    // token 序列只解析一次：候选位预扫、声明名收集与替换主循环共享（`.方法()`
    // 判定需向后看一位——下一个有意义 token 是否为 `(`，字段访问 `实例.字段`
    // 与方法调用 `实例.方法()` 的豁免集合不同）。调用方已分好词时直接借用
    let owned_tokens;
    let tokens: &[rustc_lexer::Token] = match tokens {
        Some(tokens) => tokens,
        None => {
            owned_tokens = crate::collect_tokens(source);
            &owned_tokens
        }
    };
    // 候选位预扫：整份源码没有任何替换候选（别名表命中 / 路径根位命中模块路径表）
    // 时直接原样返回。真实语料 1192 份里 774 份属于这一类，此前照旧付「声明名
    // 收集 + 逐位置豁免判定」两趟（该阶段 6.1 ms 里的 4.2 ms）
    if !has_replacement_candidate(tokens, source, alias_map, module_path_map) {
        return ReplaceResult {
            output: source.to_string(),
            edits: Vec::new(),
        };
    }
    // 第一遍：收集用户声明名与库特征方法名（使用处豁免见 DeclaredNames）。
    // 调用方已收集好时直接借用，未收集时才现场走状态机
    let owned_declared;
    let declared: &DeclaredNames = match declared {
        Some(declared) => declared,
        None => {
            owned_declared = collect_declared_names_in(source, alias_map, tokens);
            &owned_declared
        }
    };
    // 名字级事实缓存（见 [`AliasFacts`]）：按名字直映
    let mut name_cache: NameCache<'_, AliasFacts<'_>> = NameCache::new();
    // 单个 Colon 跟踪：rustc_lexer 把 `::` 拆成两个 Colon token
    let mut last_was_colon = false;
    // 输出只在替换点刷出：未命中的 token 不逐个 `push_str`，替换时才把
    // 「上次刷出位置 → 本 token 起点」的原始片段整段拷过去。2000 行样例
    // 有 1.5 万个 token、替换点只有几十个，逐 token 复制是纯开销
    let mut output = String::with_capacity(source.len());
    let mut copied = 0usize;
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
                // 替换候选先行判定：别名表命中，或处于路径根位且模块路径表命中。
                // 两者皆无时是否豁免不影响输出（豁免只决定要不要替换），直接原样
                // 输出：大文件里绝大多数标识符走这条快路，省掉 5 次豁免哈希查找
                let facts = name_cache.get_or(text, || AliasFacts {
                    alias: alias_map.get(text).map(String::as_str),
                    path_root: project.is_some_and(|p| p.is_path_root(text)),
                    decl_keyword: DECL_KEYWORDS.iter().find(|&&k| k == text).copied(),
                });
                let alias_hit = facts.alias;
                let is_path_head_candidate =
                    !prev_is_path_sep && next_significant_is_path_sep(tokens, index);
                // 模块路径表只在路径根位查一次（命中判定与取值共用同一次查找）
                let module_hit = if is_path_head_candidate {
                    module_path_map.get(text).map(String::as_str)
                } else {
                    None
                };
                // 项目路径链状态必须无条件推进（链内段的豁免判定依赖它，与当前
                // 标识符是否有替换候选无关）；根段确认依赖上一标识符留存的候选
                // 标记与当前 `::` 状态
                let in_project_chain = project.is_some()
                    && (in_project_path || (pending_project_root && prev_is_path_sep));
                if in_project_chain {
                    in_project_path = true;
                }
                if alias_hit.is_some() || module_hit.is_some() {
                    // `.方法()` 位：`.` 后的标识符且下一个有意义 token 是 `(`
                    //（字段名不是方法名，豁免集合见下方 usage_exempt）
                    let is_method_pos =
                        prev_is_dot && next_significant_is_open_paren(tokens, index);
                    // 路径根位置（首段）：后跟 `::` 且前不是 `::` 的标识符——
                    // `模块词::项` 的首段。字段/变量不可经 `::` 访问，根位豁免
                    // 不适用（office-zb #27）；模块词回退替换亦仅限根位
                    let is_path_head = is_path_head_candidate;
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
                        declared.items.contains(text)
                            || project.is_some_and(|p| p.items.contains(text))
                    } else if is_path_head {
                        // 路径根位（后跟 `::`）：字段/变量/裸名豁免不适用——它们
                        // 不可经 `::` 访问。缺陷回归（office-zb #27 字段场景）：
                        // 字段名与词表词撞名（如「天数」=Days）时，`天数::新建`
                        // 曾因字段豁免保留「天数」、只替换段位 → `天数::new`
                        // 报 E0433；字面量/字段访问位等非根位仍按字段豁免。
                        // 仅本文件项名与项目项名/模块名（路径链根）在根位豁免
                        declared.items.contains(text) || facts.path_root
                    } else {
                        declared.items.contains(text)
                            || declared.variables.contains(text)
                            || declared.members.contains(text)
                            // 项目声明名（项+字段，含其他文件声明）与项目根词
                            //（模块名；crate/super/self 为英文，替换表不会命中，
                            // 在此仅作为路径链起点）
                            || project.is_some_and(|p| p.naked_exempt(text))
                            || facts.path_root
                    };
                    // 声明位标识符与用户声明名的使用处保留原样（随片段拷出）
                    if !decl_protected && !usage_exempt {
                        if let Some(english) = alias_hit {
                            edits.push(SourceMapEntry::new(current_offset, len, text, english));
                            output.push_str(&source[copied..current_offset]);
                            output.push_str(english);
                            copied = current_offset + len;
                        } else if let Some(english) = module_hit {
                            // 路径根位回退模块路径表：`异步运行时::睡眠` → `tokio::sleep`、
                            // `#[异步运行时::主函数]` → `#[tokio::main]`（office-zb #10/#18）。
                            // use 语句内已在管线模块路径阶段替换，此处覆盖表达式/属性位置
                            let normalized = crate::module_path::normalize_crate_hyphen(english);
                            let replacement = normalized.as_deref().unwrap_or(english);
                            edits.push(SourceMapEntry::new(current_offset, len, text, replacement));
                            output.push_str(&source[copied..current_offset]);
                            output.push_str(replacement);
                            copied = current_offset + len;
                        }
                    }
                }
                // 根段候选判定（基于母语原文；`包`等已在词法阶段转为英文）
                pending_project_root = facts.path_root;
                // `让 mut 名称`：声明位内的 mut 透明传递声明状态；
                // `&mut 类型` 等非声明位的 mut 不传递，库类型引用仍被替换
                prev_decl = if text == "mut" {
                    prev_decl
                } else {
                    facts.decl_keyword
                };
                prev_is_path_sep = false;
                prev_is_dot = false;
            }
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. } => {
                // 空白与注释不打断声明状态、路径限定状态与成员访问状态；
                // 原文随片段拷出，无需单独输出
            }
            _ => {
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
    output.push_str(&source[copied..]);
    ReplaceResult { output, edits }
}

/// 整份源码是否存在任何替换候选
///
/// 主循环只在两处替换：别名表命中，或路径根位（后跟 `::`、前不是 `::`）命中
/// 模块路径表。声明名收集与逐位置豁免判定都只服务于这些候选，因此候选数为零
/// 的文件整条阶段可以省掉——扫描本身只做一次标识符查表 + 路径根位前瞻，比
/// 「收集声明 + 判定豁免」两趟便宜得多。
///
/// 状态机与主循环逐字对齐（`::` 由两个 Colon token 构成、空白/注释不打断），
/// 否则预扫可能漏判候选，把该替换的文件原样返回。
fn has_replacement_candidate(
    tokens: &[rustc_lexer::Token],
    source: &str,
    alias_map: &HashMap<String, String>,
    module_path_map: &HashMap<String, String>,
) -> bool {
    let mut offset = 0usize;
    let mut last_was_colon = false;
    let mut prev_is_path_sep = false;
    for (index, token) in tokens.iter().enumerate() {
        let text = &source[offset..offset + token.len];
        match token.kind {
            TokenKind::Ident => {
                if alias_map.contains_key(text) {
                    return true;
                }
                if !prev_is_path_sep
                    && !module_path_map.is_empty()
                    && module_path_map.contains_key(text)
                    && next_significant_is_path_sep(tokens, index)
                {
                    return true;
                }
                prev_is_path_sep = false;
            }
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. } => {}
            _ => {
                let is_colon = matches!(token.kind, TokenKind::Colon);
                prev_is_path_sep = if is_colon {
                    prev_is_path_sep || last_was_colon
                } else {
                    false
                };
                last_was_colon = is_colon;
            }
        }
        offset += token.len;
    }
    false
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

/// 下一个有意义 token（跳过空白/注释）是否为 `::`
///
/// 判定标识符处于「路径根位置」（`模块词::项` 的首段）。rustc_lexer 将
/// `::` 拆分为两个连续 Colon token；空白/注释不打断（与主循环
/// `last_was_colon` 状态对空白的处理一致）。
fn next_significant_is_path_sep(tokens: &[rustc_lexer::Token], index: usize) -> bool {
    let mut colon_count = 0;
    for t in tokens.get(index + 1..).unwrap_or_default() {
        if matches!(
            t.kind,
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
        ) {
            continue;
        }
        if matches!(t.kind, TokenKind::Colon) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn alias_map() -> HashMap<String, String> {
        HashMap::from([
            ("绝对值".to_string(), "abs".to_string()),
            ("字符串".to_string(), "String".to_string()),
        ])
    }

    /// 零候选文件：整份源码既无别名表命中、也无路径根位命中模块路径表时，
    /// 预扫直接原样返回（不产出编辑）。早期实现照旧付「声明名收集 + 逐位置
    /// 豁免判定」两趟，等价性由本测试与下方候选位测试共同锁定。
    #[test]
    fn no_candidate_file_returns_unchanged() {
        let map = alias_map();
        let src = "fn 主函数() {\n    let x = 1;\n    let 总和 = x + 2;\n}\n";
        let r = replace_aliases_with_context_and_module_paths(src, &map, &HashMap::new(), None);
        assert_eq!(r.output, src);
        assert!(r.edits.is_empty(), "无候选文件不应产出编辑：{:?}", r.edits);
    }

    /// 预扫的状态机必须与主循环一致：`::` 由两个 Colon token 组成，中间允许
    /// 夹空白与注释。预扫若漏认路径根位，整份文件会被误判为零候选而原样返回。
    #[test]
    fn candidate_scan_handles_path_sep_with_trivia() {
        let map = HashMap::from([("打印".to_string(), "print".to_string())]);
        let modules = HashMap::from([("异步运行时".to_string(), "tokio".to_string())]);
        for src in [
            "异步运行时::睡眠(1);",
            "异步运行时 :: 睡眠(1);",
            "异步运行时: /* c */ : 睡眠(1);",
        ] {
            let r = replace_aliases_with_context_and_module_paths(src, &map, &modules, None);
            assert!(
                !r.edits.is_empty(),
                "路径根位候选被预扫漏掉，整份文件原样返回：{src}"
            );
            assert!(
                r.output.contains("tokio"),
                "路径根位未回退模块路径表：{}",
                r.output
            );
        }
    }

    /// 有候选但候选全被豁免（声明位）：输出必须与零候选路径一致——原样、无编辑。
    /// 豁免只决定「要不要替换」，不创造替换；预扫据此只看候选存在性。
    #[test]
    fn exempt_candidate_yields_no_edits() {
        let map = alias_map();
        let src = "struct 字符串;\n";
        let r = replace_aliases_with_context_and_module_paths(src, &map, &HashMap::new(), None);
        assert_eq!(r.output, src);
        assert!(r.edits.is_empty(), "声明位不应被替换：{:?}", r.edits);
    }

    /// 有候选文件不受预扫影响：所有可替换点仍照常替换（含路径根段与类型位）。
    #[test]
    fn candidate_present_still_replaces_all_sites() {
        let map = HashMap::from([
            ("字符串".to_string(), "String".to_string()),
            ("新建".to_string(), "new".to_string()),
        ]);
        let src = "fn 主函数() {\n    let b = 字符串::新建();\n    let c: 字符串 = b;\n}\n";
        let r = replace_aliases_with_context_and_module_paths(src, &map, &HashMap::new(), None);
        assert!(
            r.output.contains("String::new()"),
            "路径根与段位未全部替换：{}",
            r.output
        );
        assert!(
            r.output.contains("let c: String = b;"),
            "类型位未替换：{}",
            r.output
        );
        assert_eq!(r.edits.len(), 3, "可替换点应全部替换：{:?}", r.edits);
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

    #[test]
    fn test_let_type_annotation_with_bracket_not_binding() {
        // weix-1 #15：let 类型标注含 `[`（切片/数组）时，`[` 曾把 let 深度
        // 抬回非零、`=` 无法结束 let 态，类型内别名被误收为值绑定而全文件
        // 豁免替换（E0425）。修复后顶层的 `:` 立即结束 let 态，类型内别名照常替换。
        let map = HashMap::from([("单元数据".to_string(), "Data".to_string())]);
        let src = "let 行列表: 向量<&[单元数据]> = 区域.全部行(); let z = 单元数据;";
        let out = replace_aliases(src, &map);
        assert_eq!(
            out,
            "let 行列表: 向量<&[Data]> = 区域.全部行(); let z = Data;"
        );
    }

    #[test]
    fn test_let_struct_pattern_field_colon_still_binding() {
        // 修复不误伤：结构体模式 `点 { x: 甲 }` 的 `:` 位于括号深度内（字段名
        // 分隔），不能结束 let 态——字段值 `甲` 仍是绑定，须全文件豁免。
        let map = HashMap::from([("数据".to_string(), "data".to_string())]);
        let src = "let 点 { x: 数据, y: 乙 } = 甲; let z = 数据;";
        let out = replace_aliases(src, &map);
        assert_eq!(out, src);
    }

    /// 项目上下文的字段名（不参与项集合）在路径根位不得豁免（office-zb
    /// #27 字段场景）：字段名「天数」撞词表词（Days）时，`天数::新建` 须
    /// 替换出 `Days::new`——此前因字段并入项目 names 而全豁免，产物
    /// `天数::new` 报 E0433
    #[test]
    fn test_project_field_not_exempt_in_path_root_position() {
        let map = HashMap::from([
            ("天数".to_string(), "Days".to_string()),
            ("新建".to_string(), "new".to_string()),
        ]);
        let ctx = ProjectContext {
            modules: HashSet::new(),
            names: HashSet::from(["天数".to_string()]),
            items: HashSet::new(),
        };
        let out = replace_aliases_with_context("let d = 天数::新建(1);", &map, Some(&ctx));
        assert_eq!(out.output, "let d = Days::new(1);");
        // 裸使用处的字段名照旧豁免（值语义与字面量场景）
        let out2 = replace_aliases_with_context("let x = 天数 + 1;", &map, Some(&ctx));
        assert_eq!(out2.output, "let x = 天数 + 1;");
    }

    /// 单文件字段场景（无项目上下文）：同文件声明的字段名撞词表词时，
    /// 路径根位同样不豁免；字面量与字段访问位仍豁免
    #[test]
    fn test_local_field_not_exempt_in_path_root_position() {
        let map = HashMap::from([
            ("天数".to_string(), "Days".to_string()),
            ("新建".to_string(), "new".to_string()),
        ]);
        let src = "struct 账本 { 天数: i32 }\nlet d = 天数::新建(1);\nlet x = 账本 { 天数: 1 };";
        let out = replace_aliases(src, &map);
        assert_eq!(
            out,
            "struct 账本 { 天数: i32 }\nlet d = Days::new(1);\nlet x = 账本 { 天数: 1 };"
        );
    }

    /// 模块路径回退：use 之外的限定路径前缀（office-zb #10）——
    /// 「异步运行时」在模块路径表（tokio），表达式位置根段回退替换；
    /// 段位「睡眠」由别名表替换
    #[test]
    fn test_module_path_prefix_replaced_in_expression() {
        let alias = HashMap::from([("睡眠".to_string(), "sleep".to_string())]);
        let modules = HashMap::from([("异步运行时".to_string(), "tokio".to_string())]);
        let out = replace_aliases_with_context_and_module_paths(
            "let r = 异步运行时::睡眠(1);",
            &alias,
            &modules,
            None,
        );
        assert_eq!(out.output, "let r = tokio::sleep(1);");
    }

    /// 属性宏路径位置的模块词同样回退转译（office-zb #18）
    #[test]
    fn test_module_path_prefix_in_attribute() {
        let alias = HashMap::from([("主函数".to_string(), "main".to_string())]);
        let modules = HashMap::from([("异步运行时".to_string(), "tokio".to_string())]);
        let out = replace_aliases_with_context_and_module_paths(
            "#[异步运行时::主函数]\nfn f() {}",
            &alias,
            &modules,
            None,
        );
        assert_eq!(out.output, "#[tokio::main]\nfn f() {}");
    }

    /// 模块路径回退的 crate 名连字符规范化（`tauri-plugin-autostart` → `_`）
    #[test]
    fn test_module_path_prefix_hyphen_normalized() {
        let alias = HashMap::from([("初始化".to_string(), "init".to_string())]);
        let modules =
            HashMap::from([("自启插件".to_string(), "tauri-plugin-autostart".to_string())]);
        let out = replace_aliases_with_context_and_module_paths(
            "自启插件::初始化();",
            &alias,
            &modules,
            None,
        );
        assert_eq!(out.output, "tauri_plugin_autostart::init();");
    }

    /// 别名表命中优先于模块路径回退（「路径」为 `Path` 而非 `path` 模块）
    #[test]
    fn test_module_path_fallback_alias_priority() {
        let alias = HashMap::from([
            ("路径".to_string(), "Path".to_string()),
            ("新建".to_string(), "new".to_string()),
        ]);
        let modules = HashMap::from([("路径".to_string(), "path".to_string())]);
        let out = replace_aliases_with_context_and_module_paths(
            "let p = 路径::新建();",
            &alias,
            &modules,
            None,
        );
        assert_eq!(out.output, "let p = Path::new();");
    }

    /// 回退仅限路径根位：裸使用与 `::` 后段（中段）不查模块路径表
    #[test]
    fn test_module_path_fallback_only_at_path_head() {
        let alias = HashMap::new();
        let modules = HashMap::from([("时间".to_string(), "time".to_string())]);
        let out =
            replace_aliases_with_context_and_module_paths("let t = 时间;", &alias, &modules, None);
        assert_eq!(out.output, "let t = 时间;");
        let out2 = replace_aliases_with_context_and_module_paths(
            "let x = 我的类型::时间();",
            &alias,
            &modules,
            None,
        );
        assert_eq!(out2.output, "let x = 我的类型::时间();");
    }

    /// 本文件声明的类型名与模块词撞名：路径根位按本地项豁免（本地语义优先）
    #[test]
    fn test_local_item_shadows_module_path_fallback() {
        let alias = HashMap::from([("新建".to_string(), "new".to_string())]);
        let modules = HashMap::from([("时间".to_string(), "time".to_string())]);
        let out = replace_aliases_with_context_and_module_paths(
            "struct 时间 {}\nlet t = 时间::新建();",
            &alias,
            &modules,
            None,
        );
        assert_eq!(out.output, "struct 时间 {}\nlet t = 时间::new();");
    }

    /// 名字级事实缓存按名字直映：槽位冲突时两个名字各得其事实
    ///
    /// 槽内保存名字全文并逐字节比较；若误按槽位取用，同槽的未映射名字会拿到
    /// 前一个已映射名字的译文。
    #[test]
    fn test_alias_facts_cache_collision_keeps_both_names() {
        let alias = HashMap::from([("甲".to_string(), "one".to_string())]);
        let want = crate::name_cache::name_cache_slot("甲");
        let collide = (0..5000)
            .map(|i| format!("甲{i}"))
            .find(|name| crate::name_cache::name_cache_slot(name) == want)
            .expect("应能找到同槽位的另一个名字");
        let source = format!("let y = 甲 + {collide};\n");
        let out = replace_aliases(&source, &alias);
        assert_eq!(out, format!("let y = one + {collide};\n"));
    }

    /// 项目上下文的名字级缓存：路径根事实在重复名字上一致生效
    ///
    /// 同一个名字既作路径链根又出现在裸使用位，两处的豁免依据不同
    ///（根位看 `path_root`，裸位看项目声明名），缓存不能把两件事混成一件。
    #[test]
    fn test_project_path_root_fact_reused_across_positions() {
        let alias = HashMap::from([
            ("平台Linux".to_string(), "platform".to_string()),
            ("新建".to_string(), "new".to_string()),
        ]);
        let mut items = HashSet::new();
        items.insert("平台Linux".to_string());
        let project = ProjectContext {
            modules: HashSet::new(),
            names: items.clone(),
            items,
        };
        let out = replace_aliases_with_context(
            "let a = 平台Linux::新建();\nlet b = 平台Linux();",
            &alias,
            Some(&project),
        );
        // 根位按项目路径根豁免、裸位按项目声明名豁免，两处都不替换；
        // 链内段「新建」未定义于项目，照常按库 API 替换
        assert_eq!(
            out.output,
            "let a = 平台Linux::new();\nlet b = 平台Linux();"
        );
    }
}
