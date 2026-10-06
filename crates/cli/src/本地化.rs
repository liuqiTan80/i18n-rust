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
//!
//! 再生成方式见 tools/zh-selfhost/regen.sh；请勿手改同目录下的 ui.rs。
//! 注：外部 ABI 走透传——`std::path::Path`/`std::collections::HashMap`/`toml::from_str`/
//! `std::fs::read_to_string`/`std::env::var`/`i18n_rust_engine::语言::内置语言代码`/
//! `crate::语言包根目录`（crate 根 `main.zh` 已翻转）、`crate::语言包工具::全局语言目录`/
//! `crate::内置语言::获取内置数据/内置语言代码`、`crate::语言包工具::单元测试::{环境锁, 环境恢复::接管}`
//! 为已翻转的中文 ABI 或稳定外部名；本模块自身类型、方法、函数、局部与控制流全中文化，
//! 所有数据串（ui.toml 键、环境变量名、语言标签、断言文案）逐字保真。

use std::collections::HashMap;
use std::path::Path;

/// 界面消息表：语言代码 + 消息模板
pub struct 界面 {
    /// 消息模板（键 → 含 `{}` 占位符的模板）
    消息表: HashMap<String, String>,
}

impl 界面 {
    /// 从 TOML 内容构造消息表（["界面消息"] 节）
    fn 从文本构造(内容: &str) -> Self {
        let mut 消息表 = HashMap::new();
        if let Ok(值) = toml::from_str::<toml::Value>(内容)
            && let Some(表) = 值.get("界面消息").and_then(|节| 节.as_table())
        {
            for (键, 项) in 表 {
                if let Some(文本串) = 项.as_str() {
                    消息表.insert(键.clone(), 文本串.to_string());
                }
            }
        }
        界面 { 消息表 }
    }

    /// 加载指定语言的界面消息（项目内 > 全局 > 内置）
    pub fn 按语言加载(语言代码: &str) -> Self {
        // 1. 当前目录项目语言包根 <lang>/ui.toml（项目内自定义覆盖；
        //    主仓库为 crates/engine/lang-packs/，用户项目为 lang-packs/）
        let 本地 = crate::语言包根目录(Path::new("."))
            .join(语言代码)
            .join("ui.toml");
        if 本地.is_file()
            && let Ok(内容) = std::fs::read_to_string(&本地)
        {
            return Self::从文本构造(&内容);
        }
        // 2. 全局用户语言包目录
        let 全局 = crate::语言包工具::全局语言目录()
            .join(语言代码)
            .join("ui.toml");
        if 全局.is_file()
            && let Ok(内容) = std::fs::read_to_string(&全局)
        {
            return Self::从文本构造(&内容);
        }
        // 3. 内置语言包（未知语言代码自动回退中文）
        let 内置 = crate::内置语言::获取内置数据(语言代码);
        Self::从文本构造(内置.界面文本)
    }

    /// 加载 --lang-pack 显式指定目录的界面消息
    ///
    /// 目录含 ui.toml 时直接使用；否则按目录名回退常规加载链。
    pub fn 按显式目录加载(路径: &Path) -> Self {
        let 语言代码 = 路径
            .file_name()
            .and_then(|名| 名.to_str())
            .unwrap_or("zh")
            .to_string();
        let 界面文件 = 路径.join("ui.toml");
        if 界面文件.is_file()
            && let Ok(内容) = std::fs::read_to_string(&界面文件)
        {
            return Self::从文本构造(&内容);
        }
        Self::按语言加载(&语言代码)
    }

    /// 全局界面消息：用于 clap 帮助等无文件上下文的场景
    ///
    /// 每次调用重新解析（模板仅 60 余键，开销可忽略），
    /// 避免全局缓存导致环境变量变化后语言不切换。
    pub fn 全局() -> Self {
        Self::按语言加载(&检测界面语言())
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
        替换占位(&self.取文(键), 参数)
    }
}

/// 按出现顺序把 `{}` 替换为参数；参数多于占位符时忽略多余参数，
/// 参数不足时保留未替换的 `{}`（模板兜底，避免 panic）。
fn 替换占位(模板: &str, 参数: &[&str]) -> String {
    let mut 结果串 = String::with_capacity(模板.len());
    let mut 剩余 = 模板;
    for 参数项 in 参数 {
        match 剩余.find("{}") {
            Some(序号) => {
                结果串.push_str(&剩余[..序号]);
                结果串.push_str(参数项);
                剩余 = &剩余[序号 + 2..];
            }
            None => {
                // 占位符已耗尽：剩余模板原样保留
                结果串.push_str(剩余);
                return 结果串;
            }
        }
    }
    结果串.push_str(剩余);
    结果串
}

/// 界面语言选择：RZ_LANG 环境变量 > 系统语言 > zh
pub fn 检测界面语言() -> String {
    if let Ok(语言) = std::env::var("RZ_LANG") {
        let 语言 = 语言.trim().to_lowercase();
        if !语言.is_empty() {
            return 语言;
        }
    }
    检测系统语言()
}

/// 检测系统语言（用于 --lang 缺省值与界面语言回退）
///
/// 读取 LC_ALL / LC_MESSAGES / LANG 环境变量，取首段语言标签
/// （如 `zh_CN.UTF-8` → `zh`）匹配已支持语言；无法识别时默认中文。
/// 语言集合来自引擎 lang-packs 目录（单一事实源），不在此处维护匹配表。
pub fn 检测系统语言() -> String {
    for 变量名 in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(值) = std::env::var(变量名) {
            let 小写 = 值.to_lowercase();
            let 标签 = 小写.split(['_', '-', '.']).next().unwrap_or("").trim();
            // 普通话标签 cmn 归入中文（其余标签按内置语言代码集合精确匹配）
            let 代码 = if 标签 == "cmn" { "zh" } else { 标签 };
            if i18n_rust_engine::语言::内置语言代码().contains(&代码) {
                return 代码.to_string();
            }
        }
    }
    "zh".to_string()
}

#[cfg(test)]
mod 单元测试 {
    use super::*;
    use std::path::Path;

    #[test]
    fn 测试替换占位按序() {
        assert_eq!(替换占位("已导出到 {}", &["a.rs"]), "已导出到 a.rs");
        assert_eq!(
            替换占位("项目根: {}，错误: {}", &["/p", "boom"]),
            "项目根: /p，错误: boom"
        );
    }

    #[test]
    fn 测试替换占位多余参数忽略() {
        assert_eq!(替换占位("只有 {}", &["a", "b"]), "只有 a");
    }

    #[test]
    fn 测试替换占位缺参保留() {
        assert_eq!(替换占位("{} 和 {}", &["a"]), "a 和 {}");
        assert_eq!(替换占位("无占位符", &[]), "无占位符");
    }

    #[test]
    fn 测试按语言加载内置中文() {
        let 界面 = 界面::按语言加载("zh");
        assert_eq!(界面.取文("cli_about"), "多语言 Rust 教学方言编译器");
        // 缺失键回退键名
        assert_eq!(界面.取文("不存在的键"), "不存在的键");
    }

    #[test]
    fn 测试按语言加载全部内置含界面() {
        for 代码 in crate::内置语言::内置语言代码() {
            let 界面 = 界面::按语言加载(代码);
            assert!(!界面.消息表.is_empty(), "{代码} 的 ui 消息为空");
            assert_eq!(界面.取文("cli_about"), 界面.取文("cli_about"));
        }
    }

    #[test]
    fn 测试按语言加载未知回退中文() {
        let 界面 = 界面::按语言加载("xx");
        assert_eq!(界面.取文("cli_about"), "多语言 Rust 教学方言编译器");
    }

    #[test]
    fn 测试按显式目录加载回退() {
        let 界面 = 界面::按显式目录加载(Path::new("/不存在的目录/de"));
        // 单语分支：de 包已移除，命中的他语目录回退内置中文表
        assert_eq!(界面.取文("cli_about"), "多语言 Rust 教学方言编译器");
    }

    #[test]
    fn 测试检测系统语言标签() {
        // 修改全局环境变量（LANG），需持环境变量锁避免污染并发测试；
        // 同时临时接管清除 LC_ALL / LC_MESSAGES——其检测优先级高于 LANG，
        // CI 的 macos runner 预设 en 区域设置，不清除会覆盖本测试设置的 LANG
        let _锁 = crate::语言包工具::单元测试::环境锁();
        let _环境 = crate::语言包工具::单元测试::环境恢复::接管(
            &["LC_ALL", "LC_MESSAGES", "LANG"],
        );
        for (区域, 期望) in [
            ("zh_CN.UTF-8", "zh"),
            // 普通话标签 cmn 归入中文
            ("cmn_CN.UTF-8", "zh"),
            // 单语分支：本分支仅内置 zh，其余语言标签不在内置集合，一律回退 zh
            ("en_US.UTF-8", "zh"),
            ("de_DE.UTF-8", "zh"),
            ("ja_JP.UTF-8", "zh"),
            ("ru_RU.UTF-8", "zh"),
            ("fr_FR.UTF-8", "zh"),
        ] {
            unsafe {
                std::env::set_var("LANG", 区域);
            }
            assert_eq!(检测系统语言(), 期望, "locale: {区域}");
        }
        unsafe {
            std::env::remove_var("LANG");
        }
        assert_eq!(检测系统语言(), "zh");
    }

    #[test]
    fn 测试检测界面语言环境优先() {
        // 修改全局环境变量（RZ_LANG），需持环境变量锁避免污染并发测试
        let _锁 = crate::语言包工具::单元测试::环境锁();
        unsafe {
            std::env::set_var("RZ_LANG", "ru");
        }
        assert_eq!(检测界面语言(), "ru");
        unsafe {
            std::env::remove_var("RZ_LANG");
        }
        assert_eq!(检测界面语言(), 检测系统语言());
    }
}
