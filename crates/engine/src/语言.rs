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
use std::sync::{LazyLock, Mutex};

/// 全局语言代码（默认 zh，由 CLI/LSP 启动时设置；可重复设置以便测试恢复）
static CURRENT_LANG: Mutex<String> = Mutex::new(String::new());

#[cfg(test)]
/// 测试级互斥锁：串行化所有修改全局语言或断言语言相关文本的测试，
/// 防止 test_set_language_de 等并行执行时把 CURRENT_LANG 污染为其他语言
pub(crate) static LANG_TEST_LOCK: Mutex<()> = Mutex::new(());

/// 设置全局语言代码
pub fn set_language(code: &str) {
    *CURRENT_LANG.lock().unwrap() = code.to_string();
}

/// 当前全局语言代码（未设置时为 zh）
pub fn current_language() -> String {
    let lang = CURRENT_LANG.lock().unwrap();
    if lang.is_empty() {
        "zh".to_string()
    } else {
        lang.clone()
    }
}

/// 解析 ui.toml 内容为消息表（键 → 模板）
fn parse_ui(content: &'static str) -> HashMap<&'static str, &'static str> {
    let mut messages = HashMap::new();
    if let Ok(value) = toml::from_str::<toml::Value>(content)
        && let Some(table) = value.get("界面消息").and_then(|v| v.as_table())
    {
        for (key, val) in table {
            if let Some(text) = val.as_str() {
                messages.insert(
                    Box::leak(key.clone().into_boxed_str()) as &'static str,
                    Box::leak(text.to_string().into_boxed_str()) as &'static str,
                );
            }
        }
    }
    messages
}

macro_rules! ui_table {
    ($name:ident, $path:literal) => {
        static $name: LazyLock<HashMap<&'static str, &'static str>> =
            LazyLock::new(|| parse_ui(include_str!($path)));
    };
}

ui_table!(ZH, "../lang-packs/zh/ui.toml");
ui_table!(EN, "../lang-packs/en/ui.toml");
ui_table!(DE, "../lang-packs/de/ui.toml");
ui_table!(JA, "../lang-packs/ja/ui.toml");
ui_table!(RU, "../lang-packs/ru/ui.toml");
ui_table!(ES, "../lang-packs/es/ui.toml");
ui_table!(FR, "../lang-packs/fr/ui.toml");
ui_table!(PT, "../lang-packs/pt/ui.toml");
ui_table!(KO, "../lang-packs/ko/ui.toml");
ui_table!(AR, "../lang-packs/ar/ui.toml");
ui_table!(HI, "../lang-packs/hi/ui.toml");

/// 按语言代码取内置语言包文件的编译期内容（供 CLI / LSP 嵌入回退数据）
///
/// `file` 为语言包目录内的相对路径，如 `"keywords.toml"`、
/// `"crates/序列化.toml"`；未知语言或文件返回 `None`。
pub fn builtin_file(lang: &str, file: &str) -> Option<&'static str> {
    let content = match (lang, file) {
        ("zh", "keywords.toml") => include_str!("../lang-packs/zh/keywords.toml"),
        ("zh", "module_paths.toml") => include_str!("../lang-packs/zh/module_paths.toml"),
        ("zh", "stdlib.toml") => include_str!("../lang-packs/zh/stdlib.toml"),
        ("zh", "errors.toml") => include_str!("../lang-packs/zh/errors.toml"),
        ("zh", "lang_info.toml") => include_str!("../lang-packs/zh/lang_info.toml"),
        ("zh", "ui.toml") => include_str!("../lang-packs/zh/ui.toml"),
        ("zh", "crates/序列化.toml") => include_str!("../lang-packs/zh/crates/序列化.toml"),
        ("zh", "crates/异步.toml") => include_str!("../lang-packs/zh/crates/异步.toml"),
        ("zh", "crates/命令行.toml") => include_str!("../lang-packs/zh/crates/命令行.toml"),
        ("zh", "crates/数据库.toml") => include_str!("../lang-packs/zh/crates/数据库.toml"),
        ("zh", "crates/工具.toml") => include_str!("../lang-packs/zh/crates/工具.toml"),
        ("zh", "crates/日志.toml") => include_str!("../lang-packs/zh/crates/日志.toml"),
        ("zh", "crates/网络.toml") => include_str!("../lang-packs/zh/crates/网络.toml"),
        ("zh", "crates/错误处理.toml") => include_str!("../lang-packs/zh/crates/错误处理.toml"),
        ("zh", "crates/Web框架.toml") => include_str!("../lang-packs/zh/crates/Web框架.toml"),
        ("en", "keywords.toml") => include_str!("../lang-packs/en/keywords.toml"),
        ("en", "module_paths.toml") => include_str!("../lang-packs/en/module_paths.toml"),
        ("en", "stdlib.toml") => include_str!("../lang-packs/en/stdlib.toml"),
        ("en", "errors.toml") => include_str!("../lang-packs/en/errors.toml"),
        ("en", "lang_info.toml") => include_str!("../lang-packs/en/lang_info.toml"),
        ("en", "ui.toml") => include_str!("../lang-packs/en/ui.toml"),
        ("de", "keywords.toml") => include_str!("../lang-packs/de/keywords.toml"),
        ("de", "module_paths.toml") => include_str!("../lang-packs/de/module_paths.toml"),
        ("de", "stdlib.toml") => include_str!("../lang-packs/de/stdlib.toml"),
        ("de", "errors.toml") => include_str!("../lang-packs/de/errors.toml"),
        ("de", "lang_info.toml") => include_str!("../lang-packs/de/lang_info.toml"),
        ("de", "ui.toml") => include_str!("../lang-packs/de/ui.toml"),
        ("ja", "keywords.toml") => include_str!("../lang-packs/ja/keywords.toml"),
        ("ja", "module_paths.toml") => include_str!("../lang-packs/ja/module_paths.toml"),
        ("ja", "stdlib.toml") => include_str!("../lang-packs/ja/stdlib.toml"),
        ("ja", "errors.toml") => include_str!("../lang-packs/ja/errors.toml"),
        ("ja", "lang_info.toml") => include_str!("../lang-packs/ja/lang_info.toml"),
        ("ja", "ui.toml") => include_str!("../lang-packs/ja/ui.toml"),
        ("ru", "keywords.toml") => include_str!("../lang-packs/ru/keywords.toml"),
        ("ru", "module_paths.toml") => include_str!("../lang-packs/ru/module_paths.toml"),
        ("ru", "stdlib.toml") => include_str!("../lang-packs/ru/stdlib.toml"),
        ("ru", "errors.toml") => include_str!("../lang-packs/ru/errors.toml"),
        ("ru", "lang_info.toml") => include_str!("../lang-packs/ru/lang_info.toml"),
        ("ru", "ui.toml") => include_str!("../lang-packs/ru/ui.toml"),
        ("es", "keywords.toml") => include_str!("../lang-packs/es/keywords.toml"),
        ("es", "module_paths.toml") => include_str!("../lang-packs/es/module_paths.toml"),
        ("es", "stdlib.toml") => include_str!("../lang-packs/es/stdlib.toml"),
        ("es", "errors.toml") => include_str!("../lang-packs/es/errors.toml"),
        ("es", "lang_info.toml") => include_str!("../lang-packs/es/lang_info.toml"),
        ("es", "ui.toml") => include_str!("../lang-packs/es/ui.toml"),
        ("fr", "keywords.toml") => include_str!("../lang-packs/fr/keywords.toml"),
        ("fr", "module_paths.toml") => include_str!("../lang-packs/fr/module_paths.toml"),
        ("fr", "stdlib.toml") => include_str!("../lang-packs/fr/stdlib.toml"),
        ("fr", "errors.toml") => include_str!("../lang-packs/fr/errors.toml"),
        ("fr", "lang_info.toml") => include_str!("../lang-packs/fr/lang_info.toml"),
        ("fr", "ui.toml") => include_str!("../lang-packs/fr/ui.toml"),
        ("pt", "keywords.toml") => include_str!("../lang-packs/pt/keywords.toml"),
        ("pt", "module_paths.toml") => include_str!("../lang-packs/pt/module_paths.toml"),
        ("pt", "stdlib.toml") => include_str!("../lang-packs/pt/stdlib.toml"),
        ("pt", "errors.toml") => include_str!("../lang-packs/pt/errors.toml"),
        ("pt", "lang_info.toml") => include_str!("../lang-packs/pt/lang_info.toml"),
        ("pt", "ui.toml") => include_str!("../lang-packs/pt/ui.toml"),
        ("ko", "keywords.toml") => include_str!("../lang-packs/ko/keywords.toml"),
        ("ko", "module_paths.toml") => include_str!("../lang-packs/ko/module_paths.toml"),
        ("ko", "stdlib.toml") => include_str!("../lang-packs/ko/stdlib.toml"),
        ("ko", "errors.toml") => include_str!("../lang-packs/ko/errors.toml"),
        ("ko", "lang_info.toml") => include_str!("../lang-packs/ko/lang_info.toml"),
        ("ko", "ui.toml") => include_str!("../lang-packs/ko/ui.toml"),
        ("ar", "keywords.toml") => include_str!("../lang-packs/ar/keywords.toml"),
        ("ar", "module_paths.toml") => include_str!("../lang-packs/ar/module_paths.toml"),
        ("ar", "stdlib.toml") => include_str!("../lang-packs/ar/stdlib.toml"),
        ("ar", "errors.toml") => include_str!("../lang-packs/ar/errors.toml"),
        ("ar", "lang_info.toml") => include_str!("../lang-packs/ar/lang_info.toml"),
        ("ar", "ui.toml") => include_str!("../lang-packs/ar/ui.toml"),
        ("hi", "keywords.toml") => include_str!("../lang-packs/hi/keywords.toml"),
        ("hi", "module_paths.toml") => include_str!("../lang-packs/hi/module_paths.toml"),
        ("hi", "stdlib.toml") => include_str!("../lang-packs/hi/stdlib.toml"),
        ("hi", "errors.toml") => include_str!("../lang-packs/hi/errors.toml"),
        ("hi", "lang_info.toml") => include_str!("../lang-packs/hi/lang_info.toml"),
        ("hi", "ui.toml") => include_str!("../lang-packs/hi/ui.toml"),
        _ => return None,
    };
    Some(content)
}

/// 按语言代码取消息表（未知语言回退中文表）
fn table_for(code: &str) -> &'static HashMap<&'static str, &'static str> {
    match code {
        "en" => &EN,
        "de" => &DE,
        "ja" => &JA,
        "ru" => &RU,
        "es" => &ES,
        "fr" => &FR,
        "pt" => &PT,
        "ko" => &KO,
        "ar" => &AR,
        "hi" => &HI,
        _ => &ZH,
    }
}

/// 取指定语言的消息模板；缺失时回退中文表，再缺失回退键名本身
fn t_in(code: &str, key: &str) -> String {
    if code != "zh"
        && let Some(text) = table_for(code).get(key)
    {
        return text.to_string();
    }
    ZH.get(key)
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
        set_language("de");
        assert_eq!(current_language(), "de");
        // 纯函数按语言取模板，不受全局状态干扰
        assert_eq!(t_in("de", "err_line_col"), "Zeile {}, Spalte {}");
        assert_eq!(f_in("de", "err_line_col", &["1", "2"]), "Zeile 1, Spalte 2");
        set_language("zh");
    }

    #[test]
    fn test_fallback_chain() {
        let _guard = LANG_TEST_LOCK.lock().unwrap();
        // 德语表缺 unicode_name_*（仅 zh 提供），回退中文表
        assert_eq!(t_in("de", "unicode_name_200B"), "零宽空格");
        // 完全缺失的键回退键名
        assert_eq!(t_in("de", "no_such_key"), "no_such_key");
        set_language("zh");
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
