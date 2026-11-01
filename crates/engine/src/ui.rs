//! 界面消息表：ui.toml 解析、占位符替换与缺键回退的唯一定义
//!
//! CLI（`rzc`）与 LSP 各有自己的**加载链**——磁盘目录位置、环境变量优先级
//! 不同，这部分留在各自 crate；但「TOML 怎么解析」「`{}` 怎么替换」「缺键
//! 回退到哪」必须只有一份实现，否则同一句提示会因入口不同而变语言。
//!
//! 引擎自身走编译期嵌入的静态表（[`crate::语言::builtin_message_table`]），
//! 零解析直取；[`UiTable`] 承载运行期从磁盘或显式目录读到的自定义表。

use std::collections::HashMap;

/// 一张界面消息表
///
/// 界面消息来自 ui.toml 的 `["界面消息"]` 节；`["解释"]` 节是 LSP 悬停提示
/// 用的大白话说明（键为符号路径，如 `Option::unwrap`），缺省时为空表。
pub struct UiTable {
    messages: HashMap<String, String>,
    explanations: HashMap<String, String>,
}

impl UiTable {
    /// 解析 ui.toml 内容；解析失败视为空表，由调用方继续沿加载链回退
    pub fn parse(content: &str) -> Self {
        let mut messages = HashMap::new();
        let mut explanations = HashMap::new();
        if let Ok(value) = toml::from_str::<toml::Value>(content) {
            collect_section(&value, "界面消息", &mut messages);
            collect_section(&value, "解释", &mut explanations);
        }
        Self {
            messages,
            explanations,
        }
    }

    /// 是否没有任何界面消息
    ///
    /// 显式目录里的 ui.toml 可能只写了 `["解释"]` 节，此时界面消息应继续
    /// 沿加载链回退，而不是拿一张空表把整个界面变成裸键名。
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// 取消息模板：本表 → 内置中文表 → 键名本身
    pub fn t(&self, key: &str) -> String {
        lookup(&self.messages, key)
    }

    /// 取消息模板并替换 `{}` 占位符（按出现顺序）
    pub fn f(&self, key: &str, args: &[&str]) -> String {
        substitute(&self.t(key), args)
    }

    /// 取大白话解释（缺失返回 `None`，调用方保持原样回退）
    pub fn explanation(&self, key: &str) -> Option<&str> {
        self.explanations.get(key).map(String::as_str)
    }
}

/// 从消息表取模板：本表 → 内置中文表 → 键名本身
///
/// 自定义语言包（用户手写 ui.toml）通常只覆盖自己关心的键，缺失键回退中文
/// 而不是显示裸键名；只有连中文表都没有该键时才回退键名，让漏译一眼可见。
pub fn lookup(messages: &HashMap<String, String>, key: &str) -> String {
    if let Some(text) = messages.get(key) {
        return text.clone();
    }
    crate::语言::builtin_message_table("zh")
        .get(key)
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

/// 按出现顺序把 `{}` 替换为参数；参数多于占位符时忽略多余参数，
/// 参数不足时保留未替换的 `{}`（模板兜底，避免 panic）。
pub fn substitute(template: &str, args: &[&str]) -> String {
    let mut result = String::with_capacity(template.len());
    let mut remaining = template;
    for arg in args {
        match remaining.find("{}") {
            Some(pos) => {
                result.push_str(&remaining[..pos]);
                result.push_str(arg);
                remaining = &remaining[pos + 2..];
            }
            None => {
                // 占位符已耗尽：剩余模板原样保留
                result.push_str(remaining);
                return result;
            }
        }
    }
    result.push_str(remaining);
    result
}

/// 把 TOML 文档的某一节按「键 → 字符串值」收集进目标表（非字符串值忽略）
fn collect_section(value: &toml::Value, name: &str, out: &mut HashMap<String, String>) {
    if let Some(table) = value.get(name).and_then(|v| v.as_table()) {
        for (key, val) in table {
            if let Some(text) = val.as_str() {
                out.insert(key.clone(), text.to_string());
            }
        }
    }
}

/// 解析 ui.toml 的 `["界面消息"]` 节
///
/// 供 build.rs 生成的静态表初始化调用（生成代码在 `语言` 模块内展开），
/// 与其他入口共用同一份解析语义。使用自有 String 存储而非 `&'static str`：
/// 每语言表仅解析一次，`Box::leak` 会永久泄漏全部消息文本，无必要。
pub(crate) fn parse_messages(content: &str) -> HashMap<String, String> {
    let mut messages = HashMap::new();
    if let Ok(value) = toml::from_str::<toml::Value>(content) {
        collect_section(&value, "界面消息", &mut messages);
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_both_sections() {
        let table = UiTable::parse(
            "[\"界面消息\"]\n\"a\" = \"b\"\n[\"解释\"]\n\"Option::unwrap\" = \"直接取出值，出错就崩溃\"\n",
        );
        assert_eq!(table.t("a"), "b");
        assert_eq!(
            table.explanation("Option::unwrap"),
            Some("直接取出值，出错就崩溃")
        );
        assert_eq!(table.explanation("不存在的键"), None);
    }

    #[test]
    fn test_parse_explanations_only_is_empty() {
        // 只有 ["解释"] 节：界面消息为空，调用方应继续沿加载链回退
        let table = UiTable::parse("[\"解释\"]\n\"Vec::push\" = \"末尾追加\"\n");
        assert!(table.is_empty());
        assert_eq!(table.explanation("Vec::push"), Some("末尾追加"));
        // 界面消息缺键回退中文内置表，再缺失才回退键名
        assert_eq!(table.t("err_line_col"), "第 {} 行第 {} 列");
        assert_eq!(table.t("no_such_key"), "no_such_key");
    }

    #[test]
    fn test_parse_invalid_toml_is_empty() {
        let table = UiTable::parse("这不是 TOML [[[");
        assert!(table.is_empty());
        assert_eq!(table.t("err_line_col"), "第 {} 行第 {} 列");
    }

    #[test]
    fn test_custom_table_missing_key_falls_back_to_zh() {
        // 自定义语言包只写了一个键：其余键回退中文内置表而非裸键名
        let table = UiTable::parse("[\"界面消息\"]\n\"cli_about\" = \"Custom\"\n");
        assert_eq!(table.t("cli_about"), "Custom");
        assert_eq!(table.t("err_line_col"), "第 {} 行第 {} 列");
    }

    #[test]
    fn test_substitute_ordered() {
        assert_eq!(substitute("已导出到 {}", &["a.rs"]), "已导出到 a.rs");
        assert_eq!(
            substitute("项目根: {}，错误: {}", &["/p", "boom"]),
            "项目根: /p，错误: boom"
        );
    }

    #[test]
    fn test_substitute_extra_args_ignored() {
        assert_eq!(substitute("只有 {}", &["a", "b"]), "只有 a");
    }

    #[test]
    fn test_substitute_missing_args_kept() {
        assert_eq!(substitute("{} 和 {}", &["a"]), "a 和 {}");
        assert_eq!(substitute("无占位符", &[]), "无占位符");
    }

    #[test]
    fn test_f_substitutes_after_lookup() {
        let table = UiTable::parse("[\"界面消息\"]\n\"k\" = \"{} 与 {}\"\n");
        assert_eq!(table.f("k", &["甲", "乙"]), "甲 与 乙");
        // 缺键走中文表后再替换：占位符数量随回退模板而定
        assert_eq!(table.f("err_line_col", &["3", "7"]), "第 3 行第 7 列");
    }
}
