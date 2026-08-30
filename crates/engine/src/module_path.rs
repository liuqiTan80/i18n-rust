// 模块路径替换模块
// 1. 将源代码中 `use` 语句的中文模块路径段替换为英文路径段
//    （如 `使用 标准集合::哈希映射` → `使用 std::collections::HashMap`）；
// 2. 为已知模块路径段添加 `crate::` 前缀（LSP 虚拟项目跨文件引用专用，
//    原实现在 lsp crate，迁入引擎统一维护转译规则）。
// 两个阶段均以 token 级替换并产出编辑表，供全管线编辑地图组合。

use crate::cache::SourceMapEntry;
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
    pub edits: Vec<SourceMapEntry>,
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
    let tokens: Vec<_> = tokenize(source).collect();
    let mut output = String::new();
    let mut edits = Vec::new();
    let mut current_offset = 0;
    // use 语句内的路径段才启用模块路径映射；
    // 表达式中的 `类型::关联函数`（如 `引用计数::新建`）应走别名映射，
    // 不能被模块路径映射误替换成 `rc::新建`
    let mut in_use_stmt = false;

    for token in tokens.iter() {
        let len = token.len;
        let text = &source[current_offset..current_offset + len];
        match token.kind {
            TokenKind::Ident => {
                // use 语句内所有路径段（含末段/单段）都查映射：
                // use 中不可能出现用户变量，替换安全；
                // 末段若不在路径映射（如 `服务器` 属于标识符别名），
                // 保持原样交给别名替换处理
                if in_use_stmt {
                    if let Some(english) = path_map.get(text) {
                        edits.push(SourceMapEntry::new(current_offset, len, text, english));
                        output.push_str(english);
                    } else {
                        output.push_str(text);
                    }
                } else {
                    output.push_str(text);
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
                output.push_str(text);
            }
            _ => output.push_str(text),
        }
        current_offset += len;
    }
    ReplaceResult { output, edits }
}

/// 为已知模块路径段添加 `crate::` 前缀
///
/// Rust 2018+ 中，子模块内的裸路径 `模块::项` 无法解析到 crate 根的模块，
/// 必须写成 `crate::模块::项` 或先 use。LSP 虚拟项目把每个方言文件聚合为
/// 同一 crate 的兄弟模块，因此为引用其他模块的路径段自动补全前缀，
/// 使 rust-analyzer 能够解析跨文件引用（references/rename）。
///
/// 已带前缀的路径（`crate::辅助`、`其他::辅助`）不会被重复处理。
pub fn qualify_module_paths_with_map(
    content: &str,
    module_names: &HashSet<String>,
) -> ReplaceResult {
    if module_names.is_empty() || content.is_empty() {
        return ReplaceResult {
            output: content.to_string(),
            edits: Vec::new(),
        };
    }
    let tokens: Vec<_> = tokenize(content).collect();
    let mut output = String::with_capacity(content.len() + module_names.len() * 8);
    let mut edits = Vec::new();
    let mut offset = 0usize;

    for i in 0..tokens.len() {
        let token = &tokens[i];
        let text = &content[offset..][..token.len];
        offset += token.len;

        if is_ws(token.kind) {
            output.push_str(text);
            continue;
        }

        // 模块路径段：标识符属于已知模块名、后跟 `::`、且不在既有路径段之后
        // （`crate::辅助`、`a::辅助` 中的 `辅助` 已处于路径内，跳过）
        let needs_prefix = is_ident(token.kind) && {
            let raw_name = text.strip_prefix("r#").unwrap_or(text);
            module_names.contains(raw_name)
                && is_path_separator_after(&tokens, i)
                && !is_path_separator_before(&tokens, i)
        };
        if needs_prefix {
            let prefixed = format!("crate::{}", text);
            edits.push(SourceMapEntry::new(
                offset - token.len,
                token.len,
                text,
                &prefixed,
            ));
            output.push_str(&prefixed);
        } else {
            output.push_str(text);
        }
    }
    ReplaceResult { output, edits }
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
fn is_path_separator_after(tokens: &[rustc_lexer::Token], current: usize) -> bool {
    let mut colon_count = 0;
    for token in &tokens[(current + 1)..] {
        if is_ws(token.kind) {
            continue;
        }
        if matches!(token.kind, TokenKind::Colon) {
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
fn is_path_separator_before(tokens: &[rustc_lexer::Token], current: usize) -> bool {
    let mut colon_count = 0;
    for token in tokens[..current].iter().rev() {
        if is_ws(token.kind) {
            continue;
        }
        if matches!(token.kind, TokenKind::Colon) {
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
        assert_eq!(e.replacement, "std::collections");
        assert_eq!(
            &"使用 标准集合::哈希映射;"[e.source_offset..e.source_offset + e.length],
            "标准集合"
        );
    }

    // ===== qualify（crate:: 前缀）测试（自 lsp crate 迁移） =====

    #[test]
    fn test_qualify_adds_prefix() {
        let set = HashSet::from(["辅助".to_string(), "主".to_string()]);
        assert_eq!(
            qualify_module_paths_with_map("fn main() {\n    辅助::辅助函数();\n}", &set).output,
            "fn main() {\n    crate::辅助::辅助函数();\n}"
        );
    }

    #[test]
    fn test_qualify_no_double_prefix() {
        let set = HashSet::from(["辅助".to_string(), "主".to_string()]);
        assert_eq!(
            qualify_module_paths_with_map("crate::辅助::辅助函数()", &set).output,
            "crate::辅助::辅助函数()"
        );
        // 其他路径段内的模块名（a::辅助）不重复处理
        assert_eq!(
            qualify_module_paths_with_map("a::辅助::辅助函数()", &set).output,
            "a::辅助::辅助函数()"
        );
    }

    #[test]
    fn test_qualify_non_module_untouched() {
        let set = HashSet::from(["辅助".to_string(), "主".to_string()]);
        assert_eq!(
            qualify_module_paths_with_map("x::方法()", &set).output,
            "x::方法()"
        );
    }

    #[test]
    fn test_qualify_empty_set_keeps_original() {
        assert_eq!(
            qualify_module_paths_with_map("辅助::辅助函数()", &HashSet::new()).output,
            "辅助::辅助函数()"
        );
    }

    #[test]
    fn test_qualify_records_edits() {
        let set = HashSet::from(["辅助".to_string()]);
        let result = qualify_module_paths_with_map("fn f() { 辅助::x() }", &set);
        assert_eq!(result.output, "fn f() { crate::辅助::x() }");
        assert_eq!(result.edits.len(), 1);
        let e = &result.edits[0];
        assert_eq!(e.original, "辅助");
        assert_eq!(e.replacement, "crate::辅助");
    }
}
