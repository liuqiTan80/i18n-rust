//! 界面消息本地化模块
//!
//! 每个语言包的 ui.toml 提供 ["界面消息"] 节，
//! 内含 CLI（clap 帮助 + 运行时消息）与 LSP 帮助的全部用户可见文本。
//! 占位符 `{}` 在运行时按出现顺序替换为具体参数。
//!
//! 语言包目录加载优先级：
//! 1. 当前目录 lang-packs/<语言>/（项目内自定义覆盖）
//! 2. 全局 ~/.rz/lang-packs/<语言>/
//! 3. 内置语言包
//!
//! 语言选择优先级：
//! RZ_LANG 环境变量 > 系统语言（LANG/LC_ALL） > zh

use std::path::Path;

use i18n_rust_engine::界面消息::消息表;
use i18n_rust_engine::语言::builtin_message_table;

/// 界面消息表
///
/// 磁盘覆盖命中时持有解析结果；否则只记语言代码、查询时按引用直取引擎
/// 内置静态表，避免每次 [`Ui::global`] 都重新解析一遍 ui.toml（调用点上百处）。
/// 语言选择仍是「每次调用重算」：环境变量变化后 `global()` 立即切换。
pub struct Ui {
    /// 落盘命中的自定义表（项目内 / 全局 / 显式目录）
    custom: Option<消息表>,
    /// 内置回退语言代码（`custom` 为 `None` 时生效，未知代码由引擎路由回中文）
    lang: String,
}

impl Ui {
    /// 用已读入的 ui.toml 内容构造自定义表
    fn from_content(content: String, lang_code: &str) -> Self {
        Self {
            custom: Some(消息表::成表(&content)),
            lang: lang_code.to_string(),
        }
    }

    /// 内置表路径：只记语言代码，查询时按引用直取引擎静态表
    fn builtin(lang_code: &str) -> Self {
        Self {
            custom: None,
            lang: lang_code.to_string(),
        }
    }

    /// 加载指定语言的界面消息（项目内 > 全局 > 内置）
    pub fn for_lang(lang_code: &str) -> Self {
        // 1. 当前目录项目语言包根 <lang>/ui.toml（项目内自定义覆盖；
        //    主仓库为 crates/engine/lang-packs/，用户项目为 lang-packs/）
        let local = crate::lang_pack_root_of(Path::new("."))
            .join(lang_code)
            .join("ui.toml");
        if local.is_file()
            && let Ok(content) = std::fs::read_to_string(&local)
        {
            return Self::from_content(content, lang_code);
        }
        // 2. 全局用户语言包目录
        let global = crate::lang_manager::global_lang_dir()
            .join(lang_code)
            .join("ui.toml");
        if global.is_file()
            && let Ok(content) = std::fs::read_to_string(&global)
        {
            return Self::from_content(content, lang_code);
        }
        // 3. 内置语言包：只记语言代码，查询时按引用直取引擎静态表
        Self::builtin(lang_code)
    }

    /// 加载 --lang-pack 显式指定目录的界面消息
    ///
    /// 目录含 ui.toml 时直接使用；否则按目录名回退常规加载链。
    pub fn for_explicit_dir(path: &Path) -> Self {
        let lang_code = path.file_name().and_then(|s| s.to_str()).unwrap_or("zh");
        let ui_file = path.join("ui.toml");
        if ui_file.is_file()
            && let Ok(content) = std::fs::read_to_string(&ui_file)
        {
            return Self::from_content(content, lang_code);
        }
        Self::for_lang(lang_code)
    }

    /// 全局界面消息：用于 clap 帮助等无文件上下文的场景
    ///
    /// 每次调用重算语言与磁盘覆盖，避免全局缓存导致环境变量变化后语言不切换；
    /// 内置表按引用直取，不重复解析 TOML。
    pub fn global() -> Self {
        Self::for_lang(&detect_ui_lang())
    }

    /// 取消息模板：自定义表 → 内置中文表 → 键名本身
    pub fn t(&self, key: &str) -> String {
        match &self.custom {
            Some(table) => table.t(key),
            None => {
                i18n_rust_engine::界面消息::取模板(builtin_message_table(&self.lang), key)
            }
        }
    }

    /// 取消息模板并替换 `{}` 占位符（按出现顺序）
    pub fn f(&self, key: &str, args: &[&str]) -> String {
        i18n_rust_engine::界面消息::代入参数(&self.t(key), args)
    }
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
/// 语言集合来自引擎 lang-packs 目录（单一事实源），不在此处维护匹配表。
pub fn detect_system_language() -> String {
    detect_system_language_tag().unwrap_or_else(|| "zh".to_string())
}

/// 检测系统语言标签：命中已支持语言时返回语言代码，无法识别时返回 `None`
///
/// 与 [`detect_system_language`] 的唯一区别是不吞掉「未识别」这一事实：
/// `rzc init` 需要据此决定是提示回退、还是列出菜单让用户自选语言。
pub fn detect_system_language_tag() -> Option<String> {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = std::env::var(var) {
            let lower = val.to_lowercase();
            let tag = lower.split(['_', '-', '.']).next().unwrap_or("").trim();
            // 普通话标签 cmn 归入中文（其余标签按内置语言代码集合精确匹配）
            let code = if tag == "cmn" { "zh" } else { tag };
            if i18n_rust_engine::语言::builtin_language_codes().contains(&code) {
                return Some(code.to_string());
            }
        }
    }
    None
}

/// 读取系统 locale 的语言标签原文（LC_ALL > LC_MESSAGES > LANG 的首段）
///
/// 只做语法拆分（`it_IT.UTF-8` → `it`），不判断是否为已支持语言；
/// 供「未识别系统语言」提示回显用户实际的 locale。
pub fn system_locale_tag() -> Option<String> {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = std::env::var(var) {
            let lower = val.to_lowercase();
            let tag = lower.split(['_', '-', '.']).next().unwrap_or("").trim();
            if !tag.is_empty() {
                return Some(tag.to_string());
            }
        }
    }
    None
}

/// 语言代码 → 该语言的自称（交互式菜单里每种语言以自身文字呈现）
///
/// 未收录的代码返回 `None`，调用方直接回退显示代码本身；
/// 新增语言包时 `test_native_language_name_covers_builtin` 会失败提醒补表。
pub fn native_language_name(code: &str) -> Option<&'static str> {
    Some(match code {
        "ar" => "العربية",
        "de" => "Deutsch",
        "en" => "English",
        "es" => "Español",
        "fr" => "Français",
        "hi" => "हिन्दी",
        "ja" => "日本語",
        "ko" => "한국어",
        "pt" => "Português",
        "ru" => "Русский",
        "zh" => "中文",
        _ => return None,
    })
}

/// 解析交互式语言菜单的选择
///
/// 接受序号（`3`）、语言代码（`ja` / `JA`）或空行（取默认值）；
/// 其余输入返回 `None`，由调用方提示重新输入。
pub fn parse_lang_choice(input: &str, codes: &[String], default: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Some(default.to_string());
    }
    if let Ok(index) = trimmed.parse::<usize>()
        && (1..=codes.len()).contains(&index)
    {
        return Some(codes[index - 1].clone());
    }
    let lowered = trimmed.to_lowercase();
    codes.iter().find(|code| code.as_str() == lowered).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 占位符替换走向引擎统一实现（顺序填充、多余参数忽略、缺参保留）
    #[test]
    fn test_f_substitutes_placeholders() {
        let ui = Ui::for_lang("zh");
        assert_eq!(ui.f("err_line_col", &["3", "7"]), "第 3 行第 7 列");
        assert_eq!(ui.f("cli_about", &[]), ui.t("cli_about"));
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
            // 键名原样返回说明该语言缺这条消息（已回退过中文仍没有）
            assert_ne!(
                ui.t("cli_about"),
                "cli_about",
                "{code} 的 ui 消息缺失 cli_about"
            );
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
        assert_eq!(
            ui.t("cli_about"),
            "Ein mehrsprachiger Rust-Lehrdialekt-Compiler"
        );
    }

    #[test]
    fn test_detect_system_language_tags() {
        // 修改全局环境变量（LANG），需持环境变量锁避免污染并发测试；
        // 同时临时接管清除 LC_ALL / LC_MESSAGES——其检测优先级高于 LANG，
        // CI 的 macos runner 预设 en 区域设置，不清除会覆盖本测试设置的 LANG
        let _lock = crate::lang_manager::tests::env_lock();
        let _env = crate::lang_manager::tests::EnvRestore::take(&["LC_ALL", "LC_MESSAGES", "LANG"]);
        for (locale, expected) in [
            ("zh_CN.UTF-8", "zh"),
            // en 语言包已恢复（恒等映射），en 区域设置直接命中 en
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
            // 普通话标签 cmn 归入中文
            ("cmn_CN.UTF-8", "zh"),
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

    /// 新增内置语言包时菜单自称表必须同步（否则菜单里显示裸代码）
    #[test]
    fn test_native_language_name_covers_builtin() {
        for code in crate::builtin_lang::builtin_lang_codes() {
            assert!(
                native_language_name(code).is_some(),
                "语言包 {code} 缺少菜单自称，请补 native_language_name"
            );
        }
        assert_eq!(native_language_name("xx"), None);
    }

    /// 菜单输入：序号 / 语言代码（大小写不敏感）/ 空行取默认 / 其余拒绝
    #[test]
    fn test_parse_lang_choice() {
        let codes: Vec<String> = ["de", "ja", "zh"].iter().map(|c| c.to_string()).collect();
        assert_eq!(
            parse_lang_choice("2\n", &codes, "zh").as_deref(),
            Some("ja")
        );
        assert_eq!(
            parse_lang_choice(" 3 ", &codes, "zh").as_deref(),
            Some("zh")
        );
        assert_eq!(parse_lang_choice("JA", &codes, "zh").as_deref(), Some("ja"));
        assert_eq!(parse_lang_choice("", &codes, "zh").as_deref(), Some("zh"));
        // 越界序号 / 未支持代码 / 乱输入都交给调用方重试
        assert_eq!(parse_lang_choice("0", &codes, "zh"), None);
        assert_eq!(parse_lang_choice("4", &codes, "zh"), None);
        assert_eq!(parse_lang_choice("it", &codes, "zh"), None);
    }

    /// 未识别 locale 时保留原文标签，供回退提示回显
    #[test]
    fn test_system_locale_tag_keeps_raw() {
        let _lock = crate::lang_manager::tests::env_lock();
        let _env = crate::lang_manager::tests::EnvRestore::take(&["LC_ALL", "LC_MESSAGES", "LANG"]);
        unsafe {
            std::env::set_var("LANG", "it_IT.UTF-8");
        }
        assert_eq!(detect_system_language_tag(), None);
        assert_eq!(system_locale_tag().as_deref(), Some("it"));
    }

    #[test]
    fn test_detect_ui_lang_env_priority() {
        // 修改全局环境变量（RZ_LANG），需持环境变量锁避免污染并发测试
        let _lock = crate::lang_manager::tests::env_lock();
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
