// 别名替换模块
// 将源代码中的中文标识符别名（如第三方库的中文名称）替换为英文标识符。
// 仅替换标识符类型的 token，不触碰字符串字面量和注释等内容。
// 声明位保护：紧跟在声明关键字（fn/struct/let 等）后的标识符是用户自己的定义，
// 不是库 API 引用，不参与别名替换；且用户声明的名字在整个文件内的
// 裸使用处都豁免（两遍扫描：先收集声明名，再逐 token 替换），
// 避免 `let 新建 = 5` 声明位受保护而后续使用处被误替换成 new。
// 三条精细规则：
// 1. `::` 限定后的路径段仅对「变量名」不豁免——变量不可能经 `::` 访问
//    （`字符串::新建` 里的 `新建` 是库 API，照常替换），而用户声明的项
//    （fn/struct/enum/trait/type/mod）可以经 `::` 访问
//    （`接口错误::错误请求`），仍受豁免，否则定义与调用两侧不一致
//    （定义保留中文、调用被替换，报 E0599）；
// 2. 库特征实现块（`impl <映射词特征> for 类型 {}`）内的方法名是库 API
//    规定的名称（用户从映射表抄写而来），不是用户自定义名字，不受声明位
//    保护——`实现 抄写器 对于 类型 { 函数 渲染（...） }` 中的 `渲染`
//    必须转译为 `render`，否则无法匹配 `Scribe::render`（E0407/E0046）；
// 3. 函数参数与闭包参数是「值绑定」（用户命名）：`fn 完整网址(路径: &str)`
//    的 `路径` 在声明与使用处都豁免替换（参数「类型」位置的字照常替换），
//    与格式化串中的 `{路径}` 保持一致，避免 E0425（找不到名称 `路径`）。

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
    /// 项声明名（fn/struct/enum/trait/type/mod）：`::` 后的同名标识符仍豁免
    pub items: HashSet<String>,
    /// 变量声明名（let/const/static）：`::` 后的同名标识符照常替换
    pub variables: HashSet<String>,
    /// 库特征实现块内的方法名：声明位保护对这些名字失效（库 API 名称）
    pub library_fns: HashSet<String>,
}

/// 收集用户在声明位定义的标识符名（第一遍扫描）
///
/// 状态机与替换主循环一致：声明关键字后紧跟的标识符计入集合；
/// `mut` 在声明态内透明传递；空白/注释不打断声明态；符号终结声明位。
/// 另收集「值绑定」：函数参数名（`fn 名(参数: 类型)` 中的参数开始位
/// 且后跟 `:`）与闭包参数名（`|甲, 乙|`，可无类型标注直接收集）——
/// 绑定名是用户命名，不是库 API 引用。
/// 集合内的名字在文件内的使用处豁免别名替换（保守近似：不做作用域
/// 分析，同名遮蔽场景同样豁免，与声明位保护的设计意图一致）。
/// 库特征实现块内的方法名（见 [`collect_library_impl_fns`]）从保护集合剔除。
pub fn collect_declared_names(source: &str, alias_map: &HashMap<String, String>) -> DeclaredNames {
    let mut result = DeclaredNames::default();
    let mut prev_decl: Option<&'static str> = None;
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
    let mut offset = 0;
    for token in tokenize(source) {
        let text = &source[offset..offset + token.len];
        match token.kind {
            TokenKind::Ident => {
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
                pending_param = None;
                prev_value_end = false;
            }
            TokenKind::Gt => {
                prev_decl = None;
                if fn_head == 1 {
                    fn_angle = fn_angle.saturating_sub(1);
                }
                pending_param = None;
                prev_value_end = false;
            }
            _ => {
                prev_decl = None;
                pending_param = None;
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
    result.library_fns = collect_library_impl_fns(source, &result.items, alias_map);
    for name in &result.library_fns {
        result.items.remove(name);
        result.variables.remove(name);
    }
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
    for token in tokenize(source) {
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
    let token_stream = tokenize(source);
    let mut output = String::new();
    let mut edits = Vec::new();
    let mut current_offset = 0;
    // 上一个有意义 token 若是声明关键字，记录其文本（`mut` 透明传递）；
    // 空白/注释不重置该状态
    let mut prev_decl: Option<&'static str> = None;
    // 上一个有意义 token 是否为 `::`（两个连续 Colon token 的第二个）
    let mut prev_is_path_sep = false;

    for token in token_stream {
        let len = token.len;
        let text = &source[current_offset..current_offset + len];
        match token.kind {
            TokenKind::Ident => {
                // 声明位保护：紧随声明关键字的名字是用户自己的定义；
                // 例外——库特征实现块内 `fn` 后的方法名是库 API 名称
                //（见 collect_library_impl_fns），照常参与替换
                let decl_protected = prev_decl.is_some()
                    && !(prev_decl == Some("fn") && declared.library_fns.contains(text));
                // 用户声明名在使用处豁免；`::` 后仅「项名」豁免：
                // 变量不可能经 `::` 访问（`字符串::新建` 是库 API），
                // 而项可以（`接口错误::错误请求` 是用户关联函数）
                let usage_exempt = if prev_is_path_sep {
                    declared.items.contains(text)
                } else {
                    declared.items.contains(text) || declared.variables.contains(text)
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
                // `让 mut 名称`：声明位内的 mut 透明传递声明状态；
                // `&mut 类型` 等非声明位的 mut 不传递，库类型引用仍被替换
                prev_decl = next_decl_keyword(prev_decl, text);
                prev_is_path_sep = false;
            }
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. } => {
                output.push_str(text);
                // 空白与注释不打断声明状态与路径限定状态
            }
            _ => {
                output.push_str(text);
                // 符号终结声明位；连续两个 Colon 构成 `::` 路径限定
                prev_decl = None;
                prev_is_path_sep = if matches!(token.kind, TokenKind::Colon) {
                    prev_is_path_sep || last_was_colon
                } else {
                    false
                };
                last_was_colon = matches!(token.kind, TokenKind::Colon);
            }
        }
        current_offset += len;
    }
    ReplaceResult { output, edits }
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
    fn test_empty_map_returns_original() {
        assert_eq!(replace_aliases("任意内容", &HashMap::new()), "任意内容");
    }
}
