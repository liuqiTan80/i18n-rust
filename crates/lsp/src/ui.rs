//! 界面消息本地化模块（LSP 自有加载链，解析与回退语义复用引擎 `ui` 模块）
//!
//! 每个语言包的 ui.toml 提供 ["界面消息"] 节，内含 LSP 帮助与错误提示；
//! 可选提供 ["解释"] 节，内含方法/函数的大白话使用提示（键为符号路径）。
//! 占位符 `{}` 在运行时按出现顺序替换为具体参数。
//!
//! 加载优先级：
//! 1. --language-pack 显式目录内的 ui.toml（用户自定义覆盖）
//! 2. 按 --language-pack 目录名匹配内置语言包（如 lang-packs/de → 德语提示语）
//! 3. RZ_LANG 环境变量
//! 4. 系统语言（LC_ALL / LC_MESSAGES / LANG）
//! 5. 中文（默认）

use std::path::Path;
use std::sync::OnceLock;

use i18n_rust_engine::界面消息::消息表;

/// 全局 UI（main 启动时 init；未 init 时回退内置 zh，便于测试）
static GLOBAL: OnceLock<Ui> = OnceLock::new();
/// 未 init 时的 zh 回退实例（独立于 GLOBAL，避免占用全局槽位）
static FALLBACK_ZH: OnceLock<Ui> = OnceLock::new();

/// 初始化全局界面消息（用 --language-pack 目录加载）
///
/// 同时把命中的语言代码同步给引擎全局语言（[`i18n_rust_engine::语言::设定语言`]）：
/// 引擎侧的用户可见文本（Unicode 混淆 / 全角标点 / 教学 lint / 诊断模板）按**全局
/// 语言**取模板，与 LSP 自己的 `lsp_*` 消息是两条链路。不同步的后果有两层：
/// - 德语用户在编辑器里看到的引擎教学文本恒为中文（CLI 启动时就同步，见
///   `crates/cli/src/main.rs`，两个入口口径不一致）；
/// - `语言::uses_cyrillic_script` 也读全局语言：恒为 zh 时俄文方言的西里尔标识符
///   被逐个判成「同形异义字符」，对合法代码刷假告警。
pub fn init(lang_pack_path: &Path) {
    let (ui, lang) = Ui::load_with_lang(lang_pack_path);
    i18n_rust_engine::语言::设定语言(&lang);
    let _ = GLOBAL.set(ui);
}

/// 获取全局界面消息（未 init 时回退内置 zh，不占用全局槽位）
pub fn global() -> &'static Ui {
    GLOBAL.get().unwrap_or_else(|| {
        FALLBACK_ZH.get_or_init(|| Ui::from_toml(builtin_ui("zh").unwrap_or_default()))
    })
}

/// 界面消息表
///
/// 解析、`{}` 替换与缺键回退统一由引擎 [`消息表`] 实现；本模块只保留
/// LSP 自己的加载链（显式目录 → 目录名 → 环境变量 → 中文）。
pub struct Ui {
    table: 消息表,
}

// 内置 ui.toml 由引擎 crate 编译期嵌入（避免发布打包时 include_str! 路径失效），
// 经 [`builtin_ui`] 按语言代码获取，保证任意语言包目录名都能得到对应提示语

impl Ui {
    /// 从 TOML 内容构造消息表（["界面消息"] + ["解释"] 节）
    fn from_toml(content: &str) -> Self {
        Self {
            table: 消息表::成表(content),
        }
    }

    /// 按加载优先级获取界面消息
    ///
    /// 只要消息表、不要语言代码时用这个；两者需同源的场景（启动初始化）改用
    /// [`Ui::load_with_lang`]，别在这里再判一遍语言。
    pub fn load(lang_pack_path: &Path) -> Self {
        Self::load_with_lang(lang_pack_path).0
    }

    /// 按加载优先级获取界面消息，并返回实际命中的语言代码
    ///
    /// 语言代码与消息表由同一条链产出：两条链路各判一次语言，就会出现「界面消息
    /// 是德语、引擎教学文本是中文」的混合输出（缺陷 C）。
    /// 第 1 档的自定义目录里的 ui.toml 没有语言身份，按目录名登记：未知代码在
    /// 引擎侧逐级回退内置中文表（`语言::t` 的回退链），与登记 zh 同形。
    pub fn load_with_lang(lang_pack_path: &Path) -> (Self, String) {
        let dir_lang = lang_pack_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("zh")
            .to_string();
        // 1. --language-pack 显式目录内的 ui.toml
        if let Ok(content) = std::fs::read_to_string(lang_pack_path.join("ui.toml")) {
            let ui = Self::from_toml(&content);
            if !ui.table.没有消息() {
                return (ui, dir_lang);
            }
        }
        // 2. --language-pack 目录名匹配内置语言包
        if let Some(builtin) = builtin_ui(&dir_lang) {
            return (Self::from_toml(builtin), dir_lang);
        }
        // 3. RZ_LANG 环境变量
        let env_lang = std::env::var("RZ_LANG")
            .map(|v| v.trim().to_lowercase())
            .unwrap_or_default();
        if !env_lang.is_empty()
            && let Some(builtin) = builtin_ui(&env_lang)
        {
            return (Self::from_toml(builtin), env_lang);
        }
        // 4. 系统语言
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(var) {
                let lower = val.to_lowercase();
                let tag = lower.split(['_', '-', '.']).next().unwrap_or("").trim();
                if let Some(builtin) = builtin_ui(tag) {
                    return (Self::from_toml(builtin), tag.to_string());
                }
            }
        }
        // 5. 中文（默认）
        (
            Self::from_toml(builtin_ui("zh").unwrap_or_default()),
            "zh".to_string(),
        )
    }

    /// 取消息模板：本表 → 内置中文表 → 键名本身
    pub fn t(&self, key: &str) -> String {
        self.table.取消息(key)
    }

    /// 取消息模板并替换 `{}` 占位符（按出现顺序）
    pub fn f(&self, key: &str, args: &[&str]) -> String {
        self.table.取带参消息(key, args)
    }

    /// 取大白话解释（缺失返回 None，调用方保持原样回退）
    pub fn explanation(&self, key: &str) -> Option<&str> {
        self.table.解释(key)
    }
}

/// 语言代码 → 内置 ui.toml（数据由引擎 crate 编译期嵌入）
fn builtin_ui(lang_code: &str) -> Option<&'static str> {
    i18n_rust_engine::语言::builtin_file(lang_code, "ui.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_all_langs_available() {
        for code in ["zh", "de", "ja", "ru", "es", "fr", "pt", "ko", "ar", "hi"] {
            assert!(builtin_ui(code).is_some(), "{code} 应内置 ui.toml");
        }
    }

    #[test]
    fn test_f_delegates_to_engine() {
        // 占位符替换走引擎统一实现：顺序填充、缺参保留 `{}`
        let ui = Ui::from_toml("[\"界面消息\"]\n\"k\" = \"{} 和 {}\"\n");
        assert_eq!(ui.f("k", &["a"]), "a 和 {}");
        assert_eq!(ui.f("k", &["a", "b", "c"]), "a 和 b");
    }

    #[test]
    fn test_from_toml_parses_explanations() {
        let ui = Ui::from_toml(
            "[\"界面消息\"]\n\"a\" = \"b\"\n[\"解释\"]\n\"Option::unwrap\" = \"直接取出值，出错就崩溃\"\n\"tokio::spawn\" = \"后台任务\"\n",
        );
        assert_eq!(
            ui.explanation("Option::unwrap"),
            Some("直接取出值，出错就崩溃")
        );
        assert_eq!(ui.explanation("tokio::spawn"), Some("后台任务"));
        assert_eq!(ui.explanation("不存在的键"), None);
        // 只有 ["解释"] 没有 ["界面消息"] 时也能工作
        let only_expl = Ui::from_toml("[\"解释\"]\n\"Vec::push\" = \"末尾追加\"\n");
        assert_eq!(only_expl.explanation("Vec::push"), Some("末尾追加"));
        // 没有 ["界面消息"] 节：界面消息回退内置中文表（不再直接吐裸键名）
        assert_eq!(only_expl.t("lsp_about"), "i18n-rust LSP 代理服务器");
    }

    #[test]
    fn test_load_by_dir_name() {
        let ui = Ui::load(Path::new("/任意路径/de"));
        assert_eq!(ui.t("lsp_about"), "i18n-rust LSP-Proxy-Server");
    }

    #[test]
    fn test_load_default_falls_back_zh() {
        // 加载优先级中环境变量（RZ_LANG / LC_ALL / LC_MESSAGES / LANG）先于
        // 默认中文：CI 的 macos runner 预设 en 区域设置，会先命中 en 语言包。
        // 此处临时接管清除全部语言来源，验证“无任何来源时回退 zh”。
        // 本 crate 仅此测试触碰这些变量，无需跨测试锁。
        let keys = ["RZ_LANG", "LC_ALL", "LC_MESSAGES", "LANG"];
        let saved: Vec<(&str, Option<String>)> =
            keys.iter().map(|k| (*k, std::env::var(k).ok())).collect();
        for k in keys {
            unsafe {
                std::env::remove_var(k);
            }
        }
        let about = Ui::load(Path::new("/不存在的目录")).t("lsp_about");
        // 先恢复环境变量再断言：断言失败 panic 也不污染并行测试
        for (k, v) in saved {
            match v {
                Some(val) => unsafe {
                    std::env::set_var(k, val);
                },
                None => unsafe {
                    std::env::remove_var(k);
                },
            }
        }
        assert_eq!(about, "i18n-rust LSP 代理服务器");
    }
}
