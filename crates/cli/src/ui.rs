// 界面消息本地化模块
//
// 每个语言包的 ui.toml 提供 ["界面消息"] 节，
// 内含 CLI（clap 帮助 + 运行时消息）与 LSP 帮助的全部用户可见文本。
// 占位符 `{}` 在运行时按出现顺序替换为具体参数。
//
// 语言包目录加载优先级：
// 1. 当前目录 lang-packs/<语言>/（项目内自定义覆盖）
// 2. 全局 ~/.rz/lang-packs/<语言>/
// 3. 内置语言包
//
// 语言选择优先级：
// RZ_LANG 环境变量 > 系统语言（LANG/LC_ALL） > zh

use std::collections::HashMap;
use std::path::Path;

/// 界面消息表：语言代码 + 消息模板
pub struct Ui {
    /// 消息模板（键 → 含 `{}` 占位符的模板）
    messages: HashMap<String, String>,
}

impl Ui {
    /// 从 TOML 内容构造消息表（["界面消息"] 节）
    fn from_str(content: &str) -> Self {
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
        Ui { messages }
    }

    /// 加载指定语言的界面消息（项目内 > 全局 > 内置）
    pub fn for_lang(lang_code: &str) -> Self {
        // 1. 当前目录 lang-packs/<lang>/ui.toml（项目内自定义覆盖）
        let local = Path::new("lang-packs").join(lang_code).join("ui.toml");
        if local.is_file()
            && let Ok(content) = std::fs::read_to_string(&local)
        {
            return Self::from_str(&content);
        }
        // 2. 全局用户语言包目录
        let global = crate::lang_manager::global_lang_dir()
            .join(lang_code)
            .join("ui.toml");
        if global.is_file()
            && let Ok(content) = std::fs::read_to_string(&global)
        {
            return Self::from_str(&content);
        }
        // 3. 内置语言包（未知语言代码自动回退中文）
        let builtin = crate::builtin_lang::get_builtin_data(lang_code);
        Self::from_str(builtin.ui_toml)
    }

    /// 加载 --lang-pack 显式指定目录的界面消息
    ///
    /// 目录含 ui.toml 时直接使用；否则按目录名回退常规加载链。
    pub fn for_explicit_dir(path: &Path) -> Self {
        let lang_code = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("zh")
            .to_string();
        let ui_file = path.join("ui.toml");
        if ui_file.is_file()
            && let Ok(content) = std::fs::read_to_string(&ui_file)
        {
            return Self::from_str(&content);
        }
        Self::for_lang(&lang_code)
    }

    /// 全局界面消息：用于 clap 帮助等无文件上下文的场景
    ///
    /// 每次调用重新解析（模板仅 60 余键，开销可忽略），
    /// 避免全局缓存导致环境变量变化后语言不切换。
    pub fn global() -> Self {
        Self::for_lang(&detect_ui_lang())
    }

    /// 取消息模板（缺失时回退键名本身，便于定位遗漏）
    pub fn t(&self, key: &str) -> String {
        self.messages
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    /// 取消息模板并替换 `{}` 占位符（按出现顺序）
    pub fn f(&self, key: &str, args: &[&str]) -> String {
        substitute(&self.t(key), args)
    }
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

/// 界面语言选择：RZ_LANG 环境变量 > 系统语言 > zh
pub fn detect_ui_lang() -> String {
    if let Ok(lang) = std::env::var("RZ_LANG") {
        let lang = lang.trim().to_lowercase();
        if !lang.is_empty() {
            return lang;
        }
    }
    detect_system_language()
}

/// 检测系统语言（用于 --lang 缺省值与界面语言回退）
///
/// 读取 LC_ALL / LC_MESSAGES / LANG 环境变量，取首段语言标签
/// （如 `zh_CN.UTF-8` → `zh`）匹配已支持语言；无法识别时默认中文。
pub fn detect_system_language() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = std::env::var(var) {
            let lower = val.to_lowercase();
            let tag = lower
                .split(['_', '-', '.'])
                .next()
                .unwrap_or("")
                .trim();
            let code = match tag {
                "zh" | "cmn" => "zh",
                "en" => "en",
                "de" => "de",
                "ja" => "ja",
                "ru" => "ru",
                "es" => "es",
                "fr" => "fr",
                "pt" => "pt",
                "ko" => "ko",
                "ar" => "ar",
                "hi" => "hi",
                _ => continue,
            };
            return code.to_string();
        }
    }
    "zh".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_for_lang_builtin_zh() {
        let ui = Ui::for_lang("zh");
        assert_eq!(ui.t("cli_about"), "多语言 Rust 教学方言编译器");
        // 缺失键回退键名
        assert_eq!(ui.t("不存在的键"), "不存在的键");
    }

    #[test]
    fn test_for_lang_all_builtin_have_ui() {
        for code in crate::builtin_lang::builtin_lang_codes() {
            let ui = Ui::for_lang(code);
            assert!(!ui.messages.is_empty(), "{code} 的 ui 消息为空");
            assert_eq!(ui.t("cli_about"), ui.t("cli_about"));
        }
    }

    #[test]
    fn test_for_lang_unknown_falls_back_zh() {
        let ui = Ui::for_lang("xx");
        assert_eq!(ui.t("cli_about"), "多语言 Rust 教学方言编译器");
    }

    #[test]
    fn test_for_explicit_dir_falls_back() {
        let ui = Ui::for_explicit_dir(Path::new("/不存在的目录/de"));
        assert_eq!(ui.t("cli_about"), "Ein mehrsprachiger Rust-Lehrdialekt-Compiler");
    }

    #[test]
    fn test_detect_system_language_tags() {
        for (locale, expected) in [
            ("zh_CN.UTF-8", "zh"),
            ("en_US.UTF-8", "en"),
            ("de_DE.UTF-8", "de"),
            ("ja_JP.UTF-8", "ja"),
            ("ru_RU.UTF-8", "ru"),
            ("es_ES.UTF-8", "es"),
            ("fr_FR.UTF-8", "fr"),
            ("pt_BR.UTF-8", "pt"),
            ("ko_KR.UTF-8", "ko"),
            ("ar_SA.UTF-8", "ar"),
            ("hi_IN.UTF-8", "hi"),
        ] {
            unsafe {
                std::env::set_var("LANG", locale);
            }
            assert_eq!(detect_system_language(), expected, "locale: {locale}");
        }
        unsafe {
            std::env::remove_var("LANG");
        }
        assert_eq!(detect_system_language(), "zh");
    }

    #[test]
    fn test_detect_ui_lang_env_priority() {
        unsafe {
            std::env::set_var("RZ_LANG", "ru");
        }
        assert_eq!(detect_ui_lang(), "ru");
        unsafe {
            std::env::remove_var("RZ_LANG");
        }
        assert_eq!(detect_ui_lang(), detect_system_language());
    }
}
