// 模块路径替换模块
// 将源代码中 `use` 语句的中文模块路径段替换为英文路径段。
// 例如将 `使用 标准集合::哈希映射` 替换为 `使用 std::collections::HashMap`。

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
/// 按路径长度降序进行替换，避免短匹配意外干扰长匹配
/// （例如 `集合` 不应干扰 `标准集合` 的替换）
pub fn replace_module_paths(source: &str, path_map: &HashMap<String, String>) -> String {
    if path_map.is_empty() {
        return source.to_string();
    }
    let mut result = source.to_string();
    // 按长度降序排列，确保长路径优先替换
    let mut entries: Vec<(&String, &String)> = path_map.iter().collect();
    entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    for (chinese, english) in entries {
        result = result.replace(chinese.as_str(), english.as_str());
    }
    result
}
