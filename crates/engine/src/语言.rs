//! 全局语言与界面消息模块
//!
//! 引擎内所有用户可见消息（错误、诊断、日志、分类名等）经此模块
//! 按全局语言代码输出，彻底消除硬编码中文。
//!
//! - 语言由 CLI / LSP 启动时通过 [`set_language`] 指定，默认 `zh`；
//! - 消息模板来自各语言包 `ui.toml` 的 `["界面消息"]` 节，编译期嵌入，
//!   运行期惰性解析（每语言一次），占位符 `{}` 按出现顺序替换；
//! - 缺失键依次回退：当前语言表 → 中文表 → 键名本身。

use std::collections::HashMap;
use std::sync::Mutex;

/// 全局语言代码（默认 zh，由 CLI/LSP 启动时设置；可重复设置以便测试恢复）
static CURRENT_LANG: Mutex<String> = Mutex::new(String::new());

/// 获取语言锁；投毒时恢复而非 panic（锁内无关键不变量，
/// 仅存语言代码字符串，panic 传播会拖垮整个诊断输出链路）
fn lang_lock() -> std::sync::MutexGuard<'static, String> {
    CURRENT_LANG
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
/// 测试级互斥锁：串行化所有修改全局语言或断言语言相关文本的测试，
/// 防止 test_set_language_de 等并行执行时把 CURRENT_LANG 污染为其他语言
pub(crate) static LANG_TEST_LOCK: Mutex<()> = Mutex::new(());

/// 设置全局语言代码
pub fn set_language(code: &str) {
    *lang_lock() = code.to_string();
}

/// RAII 语言作用域守卫：构造时设置语言，drop 时恢复进入前的值
///
/// 用于测试与临时切换场景，消除手工 `set_language` + 末尾恢复
/// 的遗忘风险（忘记恢复会在并行测试间污染全局语言，是历史
/// flaky 的根因类别）。注意：守卫只保证恢复，不保证串行化；
/// 测试中仍应配合 [`LANG_TEST_LOCK`] 持锁使用。
pub struct LanguageGuard {
    previous: String,
}

impl LanguageGuard {
    /// 切换到指定语言，记录进入前的语言供 drop 时恢复
    pub fn enter(code: &str) -> Self {
        let mut lang = lang_lock();
        let previous = lang.clone();
        *lang = code.to_string();
        Self { previous }
    }
}

impl Drop for LanguageGuard {
    fn drop(&mut self) {
        // mem::take 避免在 drop 中克隆；恢复后 previous 置空不再使用
        *lang_lock() = std::mem::take(&mut self.previous);
    }
}

/// 便捷入口：`let _g = 语言::with_language("ru");` 作用域内生效，离开自动恢复
pub fn with_language(code: &str) -> LanguageGuard {
    LanguageGuard::enter(code)
}

/// 当前全局语言代码（未设置时为 zh）
pub fn current_language() -> String {
    let lang = lang_lock();
    if lang.is_empty() {
        "zh".to_string()
    } else {
        lang.clone()
    }
}

/// 解析 ui.toml 内容为消息表（键 → 模板）
///
/// 使用自有 String 存储而非 `&'static str`：每语言表仅解析一次，
/// `Box::leak` 会永久泄漏全部消息文本，无必要。
fn parse_ui(content: &str) -> HashMap<String, String> {
    let mut messages = HashMap::new();
    if let Ok(value) = toml::from_str::<toml::Value>(content)
        && let Some(table) = value.get("界面消息").and_then(|v| v.as_table())
    {
        for (key, val) in table {
            if let Some(text) = val.as_str() {
                messages.insert(key.clone(), text.to_string());
            }
        }
    }
    messages
}

// build.rs 扫描 lang-packs/ 自动生成的内嵌清单（勿手工编辑）：
// - BUILTIN_FILES：(语言代码, 相对路径, 内容) 全量清单
// - UI_TABLE_*：每语言 ui.toml 消息表静态实例 + ui_table_for 路由
// 新增语言包文件无需修改任何 Rust 代码，重新编译即自动纳入。
include!(concat!(env!("OUT_DIR"), "/builtin_generated.rs"));

/// 按语言代码取内置语言包文件的编译期内容（供 CLI / LSP 嵌入回退数据）
///
/// `file` 为语言包目录内的相对路径，如 `"keywords.toml"`、
/// `"crates/序列化.toml"`；未知语言或文件返回 `None`。
/// 清单由 build.rs 扫描 lang-packs/ 自动生成，覆盖全部语言与文件。
pub fn builtin_file(lang: &str, file: &str) -> Option<&'static str> {
    BUILTIN_FILES
        .iter()
        .find(|(l, f, _)| *l == lang && *f == file)
        .map(|(_, _, content)| *content)
}

/// 按语言代码取消息表（未知语言回退中文表）；路由由 build.rs 生成
fn table_for(code: &str) -> &'static HashMap<String, String> {
    ui_table_for(code)
}

/// 取指定语言的消息模板；缺失时回退中文表，再缺失回退键名本身
fn t_in(code: &str, key: &str) -> String {
    if code != "zh"
        && let Some(text) = table_for(code).get(key)
    {
        return text.to_string();
    }
    ui_table_for("zh")
        .get(key)
        .map(|s| s.to_string())
        .unwrap_or_else(|| key.to_string())
}

/// 取当前语言的消息模板；缺失时回退中文表，再缺失回退键名本身
pub fn t(key: &str) -> String {
    t_in(&current_language(), key)
}

/// 按指定语言取消息模板并替换 `{}` 占位符（纯函数，测试与内部使用）
fn f_in(code: &str, key: &str, args: &[&str]) -> String {
    substitute(&t_in(code, key), args)
}

/// 取消息模板并替换 `{}` 占位符（按出现顺序）
pub fn f(key: &str, args: &[&str]) -> String {
    f_in(&current_language(), key, args)
}

/// 按出现顺序把 `{}` 替换为参数；参数多于占位符时忽略多余参数，
/// 参数不足时保留未替换的 `{}`（模板兜底，避免 panic）。
fn substitute(template: &str, args: &[&str]) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_zh() {
        let _guard = LANG_TEST_LOCK.lock().unwrap();
        set_language("zh");
        assert_eq!(current_language(), "zh");
        assert_eq!(t_in("zh", "err_line_col"), "第 {} 行第 {} 列");
        assert_eq!(t_in("zh", "cli_about"), "多语言 Rust 教学方言编译器");
    }

    #[test]
    fn test_set_language_de() {
        let _guard = LANG_TEST_LOCK.lock().unwrap();
        // 作用域守卫：离开时自动恢复进入前的语言，无需手工还原
        let _lang = with_language("de");
        assert_eq!(current_language(), "de");
        // 纯函数按语言取模板，不受全局状态干扰
        assert_eq!(t_in("de", "err_line_col"), "Zeile {}, Spalte {}");
        assert_eq!(f_in("de", "err_line_col", &["1", "2"]), "Zeile 1, Spalte 2");
    }

    #[test]
    fn test_language_guard_restores_previous() {
        let _guard = LANG_TEST_LOCK.lock().unwrap();
        set_language("zh");
        {
            let _lang = with_language("ru");
            assert_eq!(current_language(), "ru");
            // 嵌套守卫逐层恢复
            let _inner = with_language("de");
            assert_eq!(current_language(), "de");
        }
        assert_eq!(current_language(), "zh");
    }

    #[test]
    fn test_fallback_chain() {
        let _guard = LANG_TEST_LOCK.lock().unwrap();
        let _lang = with_language("de");
        // 德语表缺 unicode_name_*（仅 zh 提供），回退中文表
        assert_eq!(t_in("de", "unicode_name_200B"), "零宽空格");
        // 完全缺失的键回退键名
        assert_eq!(t_in("de", "no_such_key"), "no_such_key");
    }

    #[test]
    fn test_all_langs_have_common_keys() {
        for code in [
            "zh", "en", "de", "ja", "ru", "es", "fr", "pt", "ko", "ar", "hi",
        ] {
            let table = table_for(code);
            for key in ["err_line_col", "diag_kind_error", "mapping_cat_keywords"] {
                assert!(table.contains_key(key), "{code} 缺少 {key}");
            }
        }
    }

    #[test]
    fn test_substitute_ordered() {
        assert_eq!(substitute("已导出到 {}", &["a.rs"]), "已导出到 a.rs");
        assert_eq!(
            substitute("项目根: {}，错误: {}", &["/p", "boom"]),
            "项目根: /p，错误: boom"
        );
        assert_eq!(substitute("只有 {}", &["a", "b"]), "只有 a");
        assert_eq!(substitute("{} 和 {}", &["a"]), "a 和 {}");
    }
}
