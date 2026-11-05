//! 界面消息本地化模块（LSP 自有加载链，解析与回退语义复用引擎 `界面消息` 模块）
//!
//! 每个语言包的 `ui.toml` 提供 `["界面消息"]` 节，内含 LSP 帮助与错误提示；
//! 可选提供 `["解释"]` 节，内含方法/函数的大白话使用提示（键为符号路径）。
//! 占位符 `{}` 在运行时按出现顺序替换为具体参数。
//!
//! 加载优先级：
//! 1. 显式语言包目录内的 `ui.toml`（用户自定义覆盖）
//! 2. 按语言包目录名匹配内置语言包（如 `lang-packs/de` → 德语提示语）
//! 3. `RZ_LANG` 环境变量
//! 4. 系统语言（`LC_ALL` / `LC_MESSAGES` / `LANG`）
//! 5. 中文（默认）

use std::path::Path;
use std::sync::OnceLock;

use i18n_rust_engine::界面消息::消息表;

/// 全局界面（入口启动时设定；未设定时回退内置中文，便于测试）
static 全局界面槽: OnceLock<界面> = OnceLock::new();
/// 未设定时的中文回退实例（独立于全局界面槽，避免占用全局槽位）
static 回退界面槽: OnceLock<界面> = OnceLock::new();

/// 初始化全局界面消息（用显式语言包目录加载）
///
/// 同时把命中的语言代码同步给引擎全局语言（[`引擎库::语言::设定语言`]）：
/// 引擎侧的用户可见文本按**全局语言**取模板，与 LSP 自己的 `lsp_*` 消息是两条
/// 链路。不同步会让界面消息与引擎教学文本各说各话。
pub fn 设定全局(语言包路径: &Path) {
    let (界面实例, 语言代码) = 界面::载入带语言(语言包路径);
    i18n_rust_engine::语言::设定语言(&语言代码);
    let _ = 全局界面槽.set(界面实例);
}

/// 获取全局界面消息（未设定时回退内置中文，不占用全局槽位）
pub fn 全局界面<'甲>() -> &'甲 界面 {
    全局界面槽.get().unwrap_or_else(|| {
        回退界面槽.get_or_init(|| 界面::自内容构造(内置界面("zh").unwrap_or_default()))
    })
}

/// 界面消息表
///
/// 解析、`{}` 替换与缺键回退统一由引擎 [`消息表`] 实现；本模块只保留
/// LSP 自己的加载链（显式目录 → 目录名 → 环境变量 → 中文）。
pub struct 界面 {
    界面表: 消息表,
}

// 内置 ui.toml 由引擎包编译期嵌入（避免发布打包时路径失效），
// 经 [`内置界面`] 按语言代码获取，保证任意语言包目录名都能得到对应提示语

impl 界面 {
    /// 从 TOML 内容构造消息表（`["界面消息"]` + `["解释"]` 节）
    fn 自内容构造(内容: &str) -> Self {
        Self {
            界面表: 消息表::成表(内容),
        }
    }

    /// 按加载优先级获取界面消息
    ///
    /// 只要消息表、不要语言代码时用这个；两者需同源的场景（启动初始化）改用
    /// [`界面::载入带语言`]，别在这里再判一遍语言。
    pub fn 载入(语言包路径: &Path) -> Self {
        Self::载入带语言(语言包路径).0
    }

    /// 按加载优先级获取界面消息，并返回实际命中的语言代码
    ///
    /// 语言代码与消息表由同一条链产出：两条链路各判一次语言，就会出现「界面消息
    /// 是德语、引擎教学文本是中文」的混合输出。第 1 档的自定义目录里的 ui.toml
    /// 没有语言身份，按目录名登记：未知代码在引擎侧逐级回退内置中文表。
    pub fn 载入带语言(语言包路径: &Path) -> (Self, String) {
        let 目录语言 = 语言包路径
            .file_name()
            .and_then(|串| 串.to_str())
            .unwrap_or("zh")
            .to_string();
        // 1. 显式语言包目录内的 ui.toml
        if let Ok(内容) = std::fs::read_to_string(语言包路径.join("ui.toml")) {
            let 界面实例 = Self::自内容构造(&内容);
            if !界面实例.界面表.没有消息() {
                return (界面实例, 目录语言);
            }
        }
        // 2. 目录名匹配内置语言包
        if let Some(内置文本) = 内置界面(&目录语言) {
            return (Self::自内容构造(内置文本), 目录语言);
        }
        // 3. RZ_LANG 环境变量
        let 环境语言 = std::env::var("RZ_LANG")
            .map(|值| 值.trim().to_lowercase())
            .unwrap_or_default();
        if !环境语言.is_empty() {
            if let Some(内置文本) = 内置界面(&环境语言) {
                return (Self::自内容构造(内置文本), 环境语言);
            }
        }
        // 4. 系统语言
        for 区域变量 in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(值) = std::env::var(区域变量) {
                let 小写 = 值.to_lowercase();
                let 语言标签 = 小写.split(['_', '-', '.']).next().unwrap_or("").trim();
                if let Some(内置文本) = 内置界面(语言标签) {
                    return (Self::自内容构造(内置文本), 语言标签.to_string());
                }
            }
        }
        // 5. 中文（默认）
        (
            Self::自内容构造(内置界面("zh").unwrap_or_default()),
            "zh".to_string(),
        )
    }

    /// 取消息模板：本表 → 内置中文表 → 键名本身
    pub fn 取消息(&self, 键名: &str) -> String {
        self.界面表.取消息(键名)
    }

    /// 取消息模板并替换 `{}` 占位符（按出现顺序）
    pub fn 取带参消息(&self, 键名: &str, 消息参数: &[&str]) -> String {
        self.界面表.取带参消息(键名, 消息参数)
    }

    /// 取大白话解释（缺失返回 `无`，调用方保持原样回退）
    pub fn 取解释(&self, 键名: &str) -> Option<&str> {
        self.界面表.解释(键名)
    }
}

/// 语言代码 → 内置 ui.toml（数据由引擎包编译期嵌入）
fn 内置界面(语言代码: &str) -> Option<&'static str> {
    i18n_rust_engine::语言::内置文件(语言代码, "ui.toml")
}

#[cfg(test)]
mod 母语测试 {
    use super::*;

    #[test]
    fn 内置全部语言可用() {
        for 语言代码 in ["zh", "de", "ja", "ru", "es", "fr", "pt", "ko", "ar", "hi"] {
            assert!(内置界面(语言代码).is_some(), "{语言代码} 应内置 ui.toml");
        }
    }

    #[test]
    fn 取带参消息委托引擎() {
        // 占位符替换走引擎统一实现：顺序填充、缺参保留 `{}`
        let 界面实例 = 界面::自内容构造("[\"界面消息\"]\n\"k\" = \"{} 和 {}\"\n");
        assert_eq!(界面实例.取带参消息("k", &["a"]), "a 和 {}");
        assert_eq!(界面实例.取带参消息("k", &["a", "b", "c"]), "a 和 b");
    }

    #[test]
    fn 解析解释节() {
        let 界面实例 = 界面::自内容构造(
            "[\"界面消息\"]\n\"a\" = \"b\"\n[\"解释\"]\n\"Option::unwrap\" = \"直接取出值，出错就崩溃\"\n\"tokio::spawn\" = \"后台任务\"\n",
        );
        assert_eq!(
            界面实例.取解释("Option::unwrap"),
            Some("直接取出值，出错就崩溃")
        );
        assert_eq!(界面实例.取解释("tokio::spawn"), Some("后台任务"));
        assert_eq!(界面实例.取解释("不存在的键"), None);
        // 只有 ["解释"] 没有 ["界面消息"] 时也能工作
        let 仅解释 = 界面::自内容构造("[\"解释\"]\n\"Vec::push\" = \"末尾追加\"\n");
        assert_eq!(仅解释.取解释("Vec::push"), Some("末尾追加"));
        // 没有 ["界面消息"] 节：界面消息回退内置中文表（不再直接吐裸键名）
        assert_eq!(仅解释.取消息("lsp_about"), "i18n-rust LSP 代理服务器");
    }

    #[test]
    fn 按目录名加载() {
        let 界面实例 = 界面::载入(Path::new("/任意路径/de"));
        assert_eq!(界面实例.取消息("lsp_about"), "i18n-rust LSP-Proxy-Server");
    }

    #[test]
    fn 默认回退中文() {
        // 加载优先级中环境变量（RZ_LANG / LC_ALL / LC_MESSAGES / LANG）先于
        // 默认中文：CI 的区域设置可能先命中其它语言包。此处临时接管清除全部
        // 语言来源，验证「无任何来源时回退中文」。本包仅此测试触碰这些变量。
        let 键集 = ["RZ_LANG", "LC_ALL", "LC_MESSAGES", "LANG"];
        let 保存值: Vec<(&str, Option<String>)> = 键集
            .iter()
            .map(|键| (*键, std::env::var(键).ok()))
            .collect();
        for 键 in 键集 {
            unsafe {
                std::env::remove_var(键);
            }
        }
        let 关于 = 界面::载入(Path::new("/不存在的目录")).取消息("lsp_about");
        // 先恢复环境变量再断言：断言失败触发恐慌也不污染并行测试
        for (键, 旧值) in 保存值 {
            match 旧值 {
                Some(值) => unsafe {
                    std::env::set_var(键, 值);
                },
                None => unsafe {
                    std::env::remove_var(键);
                },
            }
        }
        assert_eq!(关于, "i18n-rust LSP 代理服务器");
    }
}
