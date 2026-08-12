// 模块：模块路径替换
// 功能：将源代码中 `使用` 语句的中文路径段替换为英文路径段

use std::collections::HashMap;

/// 将源代码中的中文模块路径段替换为英文
pub fn 替换模块路径(源码: &str, 路径映射: &HashMap<String, String>) -> String {
    if 路径映射.is_empty() {
        return 源码.to_string();
    }
    let mut 结果 = 源码.to_string();
    // 按长度降序替换，避免短匹配干扰
    let mut 条目: Vec<(&String, &String)> = 路径映射.iter().collect();
    条目.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    for (中文, 英文) in 条目 {
        结果 = 结果.replace(中文.as_str(), 英文.as_str());
    }
    结果
}