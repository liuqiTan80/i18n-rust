//! 界面消息本地化模块（LSP 独立实现，不依赖 cli crate）
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

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

/// 全局界面（main 启动时初始化；未初始化时回退内置 zh，便于测试）
static 全局界面槽: OnceLock<界面> = OnceLock::new();
/// 未初始化时的 zh 回退实例（独立于全局界面槽，避免占用全局槽位）
static 回退中文界面: OnceLock<界面> = OnceLock::new();

/// 初始化全局界面消息（用 --language-pack 目录加载）
pub fn 初始化全局(语言包路径: &Path) {
    let 界面 = 界面::创建(语言包路径);
    let _ = 全局界面槽.set(界面);
}

/// 获取全局界面消息（未初始化时回退内置 zh，不占用全局槽位）
pub fn 全局() -> &'static 界面 {
    全局界面槽.get().unwrap_or_else(|| {
        回退中文界面.get_or_init(|| 界面::从文本构造(取内置界面("zh").unwrap_or_default()))
    })
}

/// 界面消息表
pub struct 界面 {
    /// 消息模板（键 → 含 `{}` 占位符的模板）
    消息表: HashMap<String, String>,
    /// 大白话解释表（["解释"] 节，键 = 符号路径/方法名，如 "Option::unwrap"）
    解释表: HashMap<String, String>,
}

// 内置 ui.toml 由引擎 crate 编译期嵌入（避免发布打包时 include_str! 路径失效），
// 经 [`取内置界面`] 按语言代码获取，保证任意语言包目录名都能得到对应提示语

impl 界面 {
    /// 从 TOML 内容构造消息表（["界面消息"] + ["解释"] 节）
    fn 从文本构造(内容: &str) -> Self {
        let mut 消息表 = HashMap::new();
        let mut 解释表 = HashMap::new();
        if let Ok(值) = toml::from_str::<toml::Value>(内容) {
            if let Some(表) = 值.get("界面消息").and_then(|节| 节.as_table()) {
                for (键, 项) in 表 {
                    if let Some(文本串) = 项.as_str() {
                        消息表.insert(键.clone(), 文本串.to_string());
                    }
                }
            }
            if let Some(表) = 值.get("解释").and_then(|节| 节.as_table()) {
                for (键, 项) in 表 {
                    if let Some(文本串) = 项.as_str() {
                        解释表.insert(键.clone(), 文本串.to_string());
                    }
                }
            }
        }
        界面 {
            消息表, 解释表
        }
    }

    /// 按加载优先级获取界面消息
    pub fn 创建(语言包路径: &Path) -> Self {
        // 1. --language-pack 显式目录内的 ui.toml
        if let Ok(内容) = std::fs::read_to_string(语言包路径.join("ui.toml")) {
            let 界面 = Self::从文本构造(&内容);
            if !界面.消息表.is_empty() {
                return 界面;
            }
        }
        // 2. --language-pack 目录名匹配内置语言包
        if let Some(名) = 语言包路径.file_name().and_then(|段| 段.to_str())
            && let Some(内置) = 取内置界面(名)
        {
            return Self::从文本构造(内置);
        }
        // 3. RZ_LANG 环境变量
        if let Ok(语言) = std::env::var("RZ_LANG") {
            let 语言 = 语言.trim().to_lowercase();
            if !语言.is_empty()
                && let Some(内置) = 取内置界面(&语言)
            {
                return Self::从文本构造(内置);
            }
        }
        // 4. 系统语言
        for 变量 in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(值) = std::env::var(变量) {
                let 小写 = 值.to_lowercase();
                let 标签 = 小写.split(['_', '-', '.']).next().unwrap_or("").trim();
                if let Some(内置) = 取内置界面(标签) {
                    return Self::从文本构造(内置);
                }
            }
        }
        // 5. 中文（默认）
        Self::从文本构造(取内置界面("zh").unwrap_or_default())
    }

    /// 取消息模板（缺失时回退键名本身，便于定位遗漏）
    pub fn 取文(&self, 键: &str) -> String {
        self.消息表
            .get(键)
            .cloned()
            .unwrap_or_else(|| 键.to_string())
    }

    /// 取消息模板并替换 `{}` 占位符（按出现顺序）
    pub fn 取文带参(&self, 键: &str, 参数: &[&str]) -> String {
        按序替换占位符(&self.取文(键), 参数)
    }

    /// 取大白话解释（缺失返回 None，调用方保持原样回退）
    pub fn 取解释(&self, 键: &str) -> Option<&str> {
        self.解释表.get(键).map(String::as_str)
    }
}

/// 语言代码 → 内置 ui.toml（数据由引擎 crate 编译期嵌入）
fn 取内置界面(语言代码: &str) -> Option<&'static str> {
    i18n_rust_engine::语言::内置文件(语言代码, "ui.toml")
}

/// 按出现顺序把 `{}` 替换为参数；参数多于占位符时忽略多余参数，
/// 参数不足时保留未替换的 `{}`（模板兜底，避免 panic）。
fn 按序替换占位符(模板: &str, 参数: &[&str]) -> String {
    let mut 结果串 = String::with_capacity(模板.len());
    let mut 剩余 = 模板;
    for 参数项 in 参数 {
        match 剩余.find("{}") {
            Some(位置) => {
                结果串.push_str(&剩余[..位置]);
                结果串.push_str(参数项);
                剩余 = &剩余[位置 + 2..];
            }
            None => {
                结果串.push_str(剩余);
                return 结果串;
            }
        }
    }
    结果串.push_str(剩余);
    结果串
}

#[cfg(test)]
mod 单元测试 {
    use super::*;

    #[test]
    fn 测试内置全部语言可用() {
        for 代码 in ["zh", "de", "ja", "ru", "es", "fr", "pt", "ko", "ar", "hi"] {
            assert!(取内置界面(代码).is_some(), "{代码} 应内置 ui.toml");
        }
    }

    #[test]
    fn 测试替换占位有序与兜底() {
        assert_eq!(按序替换占位符("未知参数: {}", &["x"]), "未知参数: x");
        assert_eq!(按序替换占位符("{} 和 {}", &["a"]), "a 和 {}");
        assert_eq!(按序替换占位符("无", &[]), "无");
    }

    #[test]
    fn 测试从文本构造解析解释() {
        let 界面 = 界面::从文本构造(
            "[\"界面消息\"]\n\"a\" = \"b\"\n[\"解释\"]\n\"Option::unwrap\" = \"直接取出值，出错就崩溃\"\n\"tokio::spawn\" = \"后台任务\"\n",
        );
        assert_eq!(
            界面.取解释("Option::unwrap"),
            Some("直接取出值，出错就崩溃")
        );
        assert_eq!(界面.取解释("tokio::spawn"), Some("后台任务"));
        assert_eq!(界面.取解释("不存在的键"), None);
        // 只有 ["解释"] 没有 ["界面消息"] 时也能工作
        let 仅解释 = 界面::从文本构造("[\"解释\"]\n\"Vec::push\" = \"末尾追加\"\n");
        assert_eq!(仅解释.取解释("Vec::push"), Some("末尾追加"));
        assert_eq!(仅解释.取文("lsp_about"), "lsp_about");
    }

    #[test]
    fn 测试按目录名加载() {
        let 界面 = 界面::创建(Path::new("/任意路径/de"));
        assert_eq!(界面.取文("lsp_about"), "i18n-rust LSP-Proxy-Server");
    }

    #[test]
    fn 测试默认回退中文() {
        // 加载优先级中环境变量（RZ_LANG / LC_ALL / LC_MESSAGES / LANG）先于
        // 默认中文：CI 的 macos runner 预设 en 区域设置，会先命中 en 语言包。
        // 此处临时接管清除全部语言来源，验证“无任何来源时回退 zh”。
        // 本 crate 仅此测试触碰这些变量，无需跨测试锁。
        let 键 = ["RZ_LANG", "LC_ALL", "LC_MESSAGES", "LANG"];
        let 已存: Vec<(&str, Option<String>)> = 键
            .iter()
            .map(|键名项| (*键名项, std::env::var(键名项).ok()))
            .collect();
        for 键名项 in 键 {
            unsafe {
                std::env::remove_var(键名项);
            }
        }
        let 关于 = 界面::创建(Path::new("/不存在的目录")).取文("lsp_about");
        // 先恢复环境变量再断言：断言失败 panic 也不污染并行测试
        for (键名项, 值项) in 已存 {
            match 值项 {
                Some(值) => unsafe {
                    std::env::set_var(键名项, 值);
                },
                None => unsafe {
                    std::env::remove_var(键名项);
                },
            }
        }
        assert_eq!(关于, "i18n-rust LSP 代理服务器");
    }
}
