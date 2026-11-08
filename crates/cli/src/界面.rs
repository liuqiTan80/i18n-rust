//! 界面消息本地化模块
//!
//! 每个语言包的界面消息文件提供「界面消息」节，
//! 内含命令行帮助（含运行时消息）与语言服务的全部用户可见文本。
//! 占位符 `{}` 在运行时按出现顺序替换为具体参数。
//!
//! 语言包目录加载优先级：
//! 1. 当前目录 lang-packs/<语言>/（项目内自定义覆盖）
//! 2. 全局 ~/.rz/lang-packs/<语言>/
//! 3. 内置语言包
//!
//! 语言选择优先级：
//! RZ_LANG 环境变量 > 系统语言（LANG/LC_ALL） > 默认中文

use std::path::Path;

use i18n_rust_engine::界面消息::消息表;
use i18n_rust_engine::语言::内置消息表;

/// 命令是否成功退出（统一取进程状态，供各模块复用，避免各处重复字段访问写法）
pub(crate) fn 命令成功(命令输出: &std::process::Output) -> bool {
    命令输出.status.success()
}

/// 界面消息表
///
/// 磁盘覆盖命中时持有解析结果；否则只记语言代码、查询时按引用直取引擎
/// 内置静态表，避免每次 [`界面消息表::全局表`] 都重新解析一遍界面消息文件
/// （调用点上百处）。语言选择仍是「每次调用重算」：环境变量变化后
/// [`界面消息表::全局表`] 立即切换。
pub struct 界面消息表 {
    /// 落盘命中的自定义表（项目内 / 全局 / 显式目录）
    自定义表: Option<消息表>,
    /// 内置回退语言代码（`自定义表` 为 `无` 时生效，未知代码由引擎路由回中文）
    语言代码: String,
}

impl 界面消息表 {
    /// 用已读入的界面消息内容构造自定义表
    fn 从内容建表(内容: String, 语言码: &str) -> Self {
        Self {
            自定义表: Some(消息表::成表(&内容)),
            语言代码: 语言码.to_string(),
        }
    }

    /// 内置表路径：只记语言代码，查询时按引用直取引擎静态表
    fn 内置表(语言码: &str) -> Self {
        Self {
            自定义表: None,
            语言代码: 语言码.to_string(),
        }
    }

    /// 加载指定语言的界面消息（项目内 > 全局 > 内置）
    pub fn 按语言建表(语言码: &str) -> Self {
        // 1. 当前目录项目语言包根 <语言>/界面消息文件（项目内自定义覆盖；
        //    主仓库为 crates/engine/lang-packs/，用户项目为 lang-packs/）
        let 本地路径 = crate::语言包根目录(Path::new("."))
            .join(语言码)
            .join("ui.toml");
        if 本地路径.is_file()
            && let Ok(内容) = std::fs::read_to_string(&本地路径)
        {
            return Self::从内容建表(内容, 语言码);
        }
        // 2. 全局用户语言包目录
        let 全局路径 = crate::语言管理::全局语言目录().join(语言码).join("ui.toml");
        if 全局路径.is_file()
            && let Ok(内容) = std::fs::read_to_string(&全局路径)
        {
            return Self::从内容建表(内容, 语言码);
        }
        // 3. 内置语言包：只记语言代码，查询时按引用直取引擎静态表
        Self::内置表(语言码)
    }

    /// 加载 --lang-pack 显式指定目录的界面消息
    ///
    /// 目录含界面消息文件时直接使用；否则按目录名回退常规加载链。
    pub fn 按显式目录建表(目录: &Path) -> Self {
        let 语言码 = 目录
            .file_name()
            .and_then(|本名| 本名.to_str())
            .unwrap_or("zh");
        let 界面文件 = 目录.join("ui.toml");
        if 界面文件.is_file()
            && let Ok(内容) = std::fs::read_to_string(&界面文件)
        {
            return Self::从内容建表(内容, 语言码);
        }
        Self::按语言建表(语言码)
    }

    /// 全局界面消息：用于命令行帮助等无文件上下文的场景
    ///
    /// 每次调用重算语言与磁盘覆盖，避免全局缓存导致环境变量变化后语言不切换；
    /// 内置表按引用直取，不重复解析界面消息文件。
    pub fn 全局表() -> Self {
        Self::按语言建表(&探测界面语言())
    }

    /// 取消息模板：自定义表 → 内置中文表 → 键名本身
    pub fn 取消息(&self, 键名: &str) -> String {
        if let Some(自定义内容) = &self.自定义表 {
            自定义内容.取消息(键名)
        } else {
            i18n_rust_engine::界面消息::取模板(内置消息表(&self.语言代码), 键名)
        }
    }

    /// 取消息模板并替换 `{}` 占位符（按出现顺序）
    pub fn 取带参消息(&self, 键名: &str, 参数切片: &[&str]) -> String {
        i18n_rust_engine::界面消息::代入参数(&self.取消息(键名), 参数切片)
    }
}

/// 界面语言选择：RZ_LANG 环境变量 > 系统语言 > 默认中文
pub fn 探测界面语言() -> String {
    if let Ok(语言值) = std::env::var("RZ_LANG") {
        let 规范值 = 语言值.trim().to_lowercase();
        if !规范值.is_empty() {
            return 规范值;
        }
    }
    探测系统语言()
}

/// 检测系统语言（用于 --lang 缺省值与界面语言回退）
///
/// 读取 LC_ALL / LC_MESSAGES / LANG 环境变量，取首段语言标签
/// （如中文区域的区域串 → 语言代码）匹配已支持语言；无法识别时默认中文。
/// 语言集合来自引擎语言包目录（单一事实源），不在此处维护匹配表。
pub fn 探测系统语言() -> String {
    探测系统语言标签().unwrap_or_else(|| "zh".to_string())
}

/// 检测系统语言标签：命中已支持语言时返回语言代码，无法识别时返回 `无`
///
/// 与 [`探测系统语言`] 的唯一区别是不吞掉「未识别」这一事实：
/// 初始化命令需要据此决定是提示回退、还是列出菜单让用户自选语言。
pub fn 探测系统语言标签() -> Option<String> {
    for 区域变量 in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(区域值) = std::env::var(区域变量) {
            let 小写区域 = 区域值.to_lowercase();
            let 标签 = 小写区域.split(['_', '-', '.']).next().unwrap_or("").trim();
            // 普通话标签归入中文（其余标签按内置语言代码集合精确匹配）
            let 语言码 = if 标签 == "cmn" { "zh" } else { 标签 };
            if i18n_rust_engine::语言::内置语言代码().contains(&语言码) {
                return Some(语言码.to_string());
            }
        }
    }
    None
}

/// 读取系统区域设置的语言标签原文（LC_ALL > LC_MESSAGES > LANG 的首段）
///
/// 只做语法拆分（例如某区域串拆出语言代码），不判断是否为已支持语言；
/// 供「未识别系统语言」提示回显用户实际的区域设置。
pub fn 系统区域标签() -> Option<String> {
    for 区域变量 in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(区域值) = std::env::var(区域变量) {
            let 小写区域 = 区域值.to_lowercase();
            let 标签 = 小写区域.split(['_', '-', '.']).next().unwrap_or("").trim();
            if !标签.is_empty() {
                return Some(标签.to_string());
            }
        }
    }
    None
}

/// 语言代码 → 该语言的自称（交互式菜单里每种语言以自身文字呈现）
///
/// 未收录的代码返回 `无`，调用方直接回退显示代码本身；
/// 新增语言包时覆盖性测试会失败提醒补表。
pub fn 母语语言名(代码: &str) -> Option<&'static str> {
    if 代码 == "ar" {
        return Some("العربية");
    }
    if 代码 == "de" {
        return Some("Deutsch");
    }
    if 代码 == "en" {
        return Some("English");
    }
    if 代码 == "es" {
        return Some("Español");
    }
    if 代码 == "fr" {
        return Some("Français");
    }
    if 代码 == "hi" {
        return Some("हिन्दी");
    }
    if 代码 == "ja" {
        return Some("日本語");
    }
    if 代码 == "ko" {
        return Some("한국어");
    }
    if 代码 == "pt" {
        return Some("Português");
    }
    if 代码 == "ru" {
        return Some("Русский");
    }
    if 代码 == "zh" {
        return Some("中文");
    }
    None
}

/// 解析交互式语言菜单的选择
///
/// 接受序号（如 3）、语言代码（大小写不敏感）或空行（取默认值）；
/// 其余输入返回 `无`，由调用方提示重新输入。
pub fn 解析语言选择(输入: &str, 代码表: &[String], 缺省值: &str) -> Option<String> {
    let 裁剪值 = 输入.trim();
    if 裁剪值.is_empty() {
        return Some(缺省值.to_string());
    }
    if let Ok(序号) = 裁剪值.parse::<usize>()
        && (1..=代码表.len()).contains(&序号)
    {
        return Some(代码表[序号 - 1].clone());
    }
    let 小写输入 = 裁剪值.to_lowercase();
    代码表
        .iter()
        .find(|本代码| 本代码.as_str() == 小写输入)
        .cloned()
}

#[cfg(test)]
mod 母语测试 {
    use super::*;

    /// 占位符替换走向引擎统一实现（顺序填充、多余参数忽略、缺参保留）
    #[test]
    fn 取带参消息替换占位符() {
        let 界面表 = 界面消息表::按语言建表("zh");
        assert_eq!(
            界面表.取带参消息("err_line_col", &["3", "7"]),
            "第 3 行第 7 列"
        );
        assert_eq!(
            界面表.取带参消息("cli_about", &[]),
            界面表.取消息("cli_about")
        );
    }

    #[test]
    fn 中文内置表可用() {
        let 界面表 = 界面消息表::按语言建表("zh");
        assert_eq!(界面表.取消息("cli_about"), "多语言 Rust 教学方言编译器");
        // 缺失键回退键名
        assert_eq!(界面表.取消息("不存在的键"), "不存在的键");
    }

    #[test]
    fn 全部内置语言都有界面消息() {
        for 语言码 in crate::内置语言::内置语言清单() {
            let 界面表 = 界面消息表::按语言建表(语言码);
            // 键名原样返回说明该语言缺这条消息（已回退过中文仍没有）
            assert_ne!(
                界面表.取消息("cli_about"),
                "cli_about",
                "{语言码} 的界面消息缺失 cli_about"
            );
        }
    }

    #[test]
    fn 未知语言回退中文() {
        let 界面表 = 界面消息表::按语言建表("xx");
        assert_eq!(界面表.取消息("cli_about"), "多语言 Rust 教学方言编译器");
    }

    #[test]
    fn 显式目录不可用则回退() {
        let 界面表 = 界面消息表::按显式目录建表(Path::new("/不存在的目录/de"));
        assert_eq!(
            界面表.取消息("cli_about"),
            "Ein mehrsprachiger Rust-Lehrdialekt-Compiler"
        );
    }

    #[test]
    fn 系统语言标签识别() {
        // 修改全局环境变量（LANG），需持环境变量锁避免污染并发测试；
        // 同时临时接管清除 LC_ALL / LC_MESSAGES——其检测优先级高于 LANG，
        // 持续集成环境的区域设置会覆盖本测试设置的 LANG
        let _锁 = crate::语言管理::tests::环境锁();
        let _环境 =
            crate::语言管理::tests::环境还原::接管(&["LC_ALL", "LC_MESSAGES", "LANG"]);
        for (区域值, 预期值) in [
            ("zh_CN.UTF-8", "zh"),
            // 英文语言包已恢复（恒等映射），英文区域设置直接命中英文
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
            // 普通话标签归入中文
            ("cmn_CN.UTF-8", "zh"),
        ] {
            unsafe {
                std::env::set_var("LANG", 区域值);
            }
            assert_eq!(探测系统语言(), 预期值, "区域设置：{区域值}");
        }
        unsafe {
            std::env::remove_var("LANG");
        }
        assert_eq!(探测系统语言(), "zh");
    }

    /// 新增内置语言包时菜单自称表必须同步（否则菜单里显示裸代码）
    #[test]
    fn 母语语言名覆盖内置清单() {
        for 语言码 in crate::内置语言::内置语言清单() {
            assert!(
                母语语言名(语言码).is_some(),
                "语言包 {语言码} 缺少菜单自称，请补母语语言名"
            );
        }
        assert_eq!(母语语言名("xx"), None);
    }

    /// 菜单输入：序号 / 语言代码（大小写不敏感）/ 空行取默认 / 其余拒绝
    #[test]
    fn 解析语言选择各分支() {
        let 代码表: Vec<String> = ["de", "ja", "zh"]
            .iter()
            .map(|本代码| 本代码.to_string())
            .collect();
        assert_eq!(解析语言选择("2\n", &代码表, "zh").as_deref(), Some("ja"));
        assert_eq!(解析语言选择(" 3 ", &代码表, "zh").as_deref(), Some("zh"));
        assert_eq!(解析语言选择("JA", &代码表, "zh").as_deref(), Some("ja"));
        assert_eq!(解析语言选择("", &代码表, "zh").as_deref(), Some("zh"));
        // 越界序号 / 未支持代码 / 乱输入都交给调用方重试
        assert_eq!(解析语言选择("0", &代码表, "zh"), None);
        assert_eq!(解析语言选择("4", &代码表, "zh"), None);
        assert_eq!(解析语言选择("it", &代码表, "zh"), None);
    }

    /// 未识别区域设置时保留原文标签，供回退提示回显
    #[test]
    fn 系统区域标签保留原文() {
        let _锁 = crate::语言管理::tests::环境锁();
        let _环境 =
            crate::语言管理::tests::环境还原::接管(&["LC_ALL", "LC_MESSAGES", "LANG"]);
        unsafe {
            std::env::set_var("LANG", "it_IT.UTF-8");
        }
        assert_eq!(探测系统语言标签(), None);
        assert_eq!(系统区域标签().as_deref(), Some("it"));
    }

    #[test]
    fn 界面语言环境变量优先() {
        // 修改全局环境变量（RZ_LANG），需持环境变量锁避免污染并发测试
        let _锁 = crate::语言管理::tests::环境锁();
        unsafe {
            std::env::set_var("RZ_LANG", "ru");
        }
        assert_eq!(探测界面语言(), "ru");
        unsafe {
            std::env::remove_var("RZ_LANG");
        }
        assert_eq!(探测界面语言(), 探测系统语言());
    }
}
