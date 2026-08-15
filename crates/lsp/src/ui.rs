// 界面消息本地化模块（LSP 独立实现，不依赖 cli crate）
//
// 每个语言包的 ui.toml 提供 ["界面消息"] 节，内含 LSP 帮助与错误提示。
// 占位符 `{}` 在运行时按出现顺序替换为具体参数。
//
// 加载优先级：
// 1. --language-pack 显式目录内的 ui.toml（用户自定义覆盖）
// 2. 按 --language-pack 目录名匹配内置语言包（如 lang-packs/de → 德语提示语）
// 3. RZ_LANG 环境变量
// 4. 系统语言（LC_ALL / LC_MESSAGES / LANG）
// 5. 中文（默认）

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

/// 全局 UI（main 启动时 init；未 init 时回退内置 zh，便于测试）
static GLOBAL: OnceLock<Ui> = OnceLock::new();
/// 未 init 时的 zh 回退实例（独立于 GLOBAL，避免占用全局槽位）
static FALLBACK_ZH: OnceLock<Ui> = OnceLock::new();

/// 初始化全局界面消息（用 --language-pack 目录加载）
pub fn init(lang_pack_path: &Path) {
    let ui = Ui::load(lang_pack_path);
    let _ = GLOBAL.set(ui);
}

/// 获取全局界面消息（未 init 时回退内置 zh，不占用全局槽位）
pub fn global() -> &'static Ui {
    GLOBAL
        .get()
        .unwrap_or_else(|| FALLBACK_ZH.get_or_init(|| Ui::from_toml(BUILTIN_ZH)))
}

/// 界面消息表
pub struct Ui {
    /// 消息模板（键 → 含 `{}` 占位符的模板）
    messages: HashMap<String, String>,
}

// 内置全部 11 个语言的 ui.toml，保证任意语言包目录名都能得到对应提示语
const BUILTIN_ZH: &str = include_str!("../../../lang-packs/zh/ui.toml");
const BUILTIN_EN: &str = include_str!("../../../lang-packs/en/ui.toml");
const BUILTIN_DE: &str = include_str!("../../../lang-packs/de/ui.toml");
const BUILTIN_JA: &str = include_str!("../../../lang-packs/ja/ui.toml");
const BUILTIN_RU: &str = include_str!("../../../lang-packs/ru/ui.toml");
const BUILTIN_ES: &str = include_str!("../../../lang-packs/es/ui.toml");
const BUILTIN_FR: &str = include_str!("../../../lang-packs/fr/ui.toml");
const BUILTIN_PT: &str = include_str!("../../../lang-packs/pt/ui.toml");
const BUILTIN_KO: &str = include_str!("../../../lang-packs/ko/ui.toml");
const BUILTIN_AR: &str = include_str!("../../../lang-packs/ar/ui.toml");
const BUILTIN_HI: &str = include_str!("../../../lang-packs/hi/ui.toml");

impl Ui {
    /// 从 TOML 内容构造消息表（["界面消息"] 节）
    fn from_toml(content: &str) -> Self {
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

    /// 按加载优先级获取界面消息
    pub fn load(lang_pack_path: &Path) -> Self {
        // 1. --language-pack 显式目录内的 ui.toml
        if let Ok(content) = std::fs::read_to_string(lang_pack_path.join("ui.toml")) {
            let ui = Self::from_toml(&content);
            if !ui.messages.is_empty() {
                return ui;
            }
        }
        // 2. --language-pack 目录名匹配内置语言包
        if let Some(name) = lang_pack_path.file_name().and_then(|s| s.to_str()) {
            if let Some(builtin) = builtin_ui(name) {
                return Self::from_toml(builtin);
            }
        }
        // 3. RZ_LANG 环境变量
        if let Ok(lang) = std::env::var("RZ_LANG") {
            let lang = lang.trim().to_lowercase();
            if !lang.is_empty()
                && let Some(builtin) = builtin_ui(&lang)
            {
                return Self::from_toml(builtin);
            }
        }
        // 4. 系统语言
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(var) {
                let lower = val.to_lowercase();
                let tag = lower
                    .split(['_', '-', '.'])
                    .next()
                    .unwrap_or("")
                    .trim();
                if let Some(builtin) = builtin_ui(tag) {
                    return Self::from_toml(builtin);
                }
            }
        }
        // 5. 中文（默认）
        Self::from_toml(BUILTIN_ZH)
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

/// 语言代码 → 内置 ui.toml
fn builtin_ui(lang_code: &str) -> Option<&'static str> {
    match lang_code {
        "zh" => Some(BUILTIN_ZH),
        "en" => Some(BUILTIN_EN),
        "de" => Some(BUILTIN_DE),
        "ja" => Some(BUILTIN_JA),
        "ru" => Some(BUILTIN_RU),
        "es" => Some(BUILTIN_ES),
        "fr" => Some(BUILTIN_FR),
        "pt" => Some(BUILTIN_PT),
        "ko" => Some(BUILTIN_KO),
        "ar" => Some(BUILTIN_AR),
        "hi" => Some(BUILTIN_HI),
        _ => None,
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
    fn test_builtin_all_langs_available() {
        for code in ["zh", "en", "de", "ja", "ru", "es", "fr", "pt", "ko", "ar", "hi"] {
            assert!(
                builtin_ui(code).is_some(),
                "{code} 应内置 ui.toml"
            );
        }
    }

    #[test]
    fn test_substitute_ordered_and_fallback() {
        assert_eq!(substitute("未知参数: {}", &["x"]), "未知参数: x");
        assert_eq!(substitute("{} 和 {}", &["a"]), "a 和 {}");
        assert_eq!(substitute("无", &[]), "无");
    }

    #[test]
    fn test_load_by_dir_name() {
        let ui = Ui::load(Path::new("/任意路径/de"));
        assert_eq!(
            ui.t("lsp_about"),
            "i18n-rust LSP-Proxy-Server"
        );
    }

    #[test]
    fn test_load_default_falls_back_zh() {
        let ui = Ui::load(Path::new("/不存在的目录"));
        assert_eq!(ui.t("lsp_about"), "i18n-rust LSP 代理服务器");
    }
}
