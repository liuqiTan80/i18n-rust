// 模块路径替换模块
// 将源代码中 `use` 语句的中文模块路径段替换为英文路径段。
// 例如将 `使用 标准集合::哈希映射` 替换为 `使用 std::collections::HashMap`。

use rustc_lexer::{TokenKind, tokenize};
use std::collections::HashMap;

/// 将源代码中的中文模块路径段替换为英文
///
/// # 参数
/// - `source`: 待替换的源代码字符串
/// - `path_map`: 中文路径段 → 英文路径段 的映射表
///
/// # 返回
/// 替换后的源代码字符串
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
    if path_map.is_empty() {
        return source.to_string();
    }
    let token_stream = tokenize(source);
    let tokens: Vec<_> = token_stream.collect();
    let mut output = String::new();
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
    output
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
        // `读取全部字符串` 是完整 token，其中的 `字符串` 不应被替换
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
}
