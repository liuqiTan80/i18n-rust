// 内置语言包 - 将默认语言包嵌入到可执行文件中
//
// 使用 `include_str!` 宏在编译时将语言包 TOML 文件嵌入二进制，
// 使得 rzc 可执行文件无需附带语言包目录即可独立运行。
// 通过 [`get_builtin_data`] 按语言代码获取对应的内置语言包。

/// 单个语言的完整内置数据
pub struct BuiltinLangData {
    /// 关键字映射 TOML
    pub keywords_toml: &'static str,
    /// 模块路径映射 TOML
    pub module_paths_toml: &'static str,
    /// 错误消息翻译 TOML
    pub errors_toml: &'static str,
    /// 语言包信息 TOML（名称 / 扩展名 / 版本）
    pub lang_info_toml: &'static str,
    /// 第三方库映射文件列表（文件名, 内容）
    pub crates_data: &'static [(&'static str, &'static str)],
}

/// 中文内置语言包
static ZH_DATA: BuiltinLangData = BuiltinLangData {
    keywords_toml: include_str!("../../../lang-packs/zh/keywords.toml"),
    module_paths_toml: include_str!("../../../lang-packs/zh/module_paths.toml"),
    errors_toml: include_str!("../../../lang-packs/zh/errors.toml"),
    lang_info_toml: include_str!("../../../lang-packs/zh/lang_info.toml"),
    crates_data: &[
        (
            "序列化.toml",
            include_str!("../../../lang-packs/zh/crates/序列化.toml"),
        ),
        (
            "异步.toml",
            include_str!("../../../lang-packs/zh/crates/异步.toml"),
        ),
        (
            "命令行.toml",
            include_str!("../../../lang-packs/zh/crates/命令行.toml"),
        ),
        (
            "数据库.toml",
            include_str!("../../../lang-packs/zh/crates/数据库.toml"),
        ),
        (
            "工具.toml",
            include_str!("../../../lang-packs/zh/crates/工具.toml"),
        ),
        (
            "日志.toml",
            include_str!("../../../lang-packs/zh/crates/日志.toml"),
        ),
        (
            "网络.toml",
            include_str!("../../../lang-packs/zh/crates/网络.toml"),
        ),
        (
            "错误处理.toml",
            include_str!("../../../lang-packs/zh/crates/错误处理.toml"),
        ),
        (
            "Web框架.toml",
            include_str!("../../../lang-packs/zh/crates/Web框架.toml"),
        ),
    ],
};

/// 英文内置语言包（恒等映射，用于验证多语言架构）
static EN_DATA: BuiltinLangData = BuiltinLangData {
    keywords_toml: include_str!("../../../lang-packs/en/keywords.toml"),
    module_paths_toml: include_str!("../../../lang-packs/en/module_paths.toml"),
    errors_toml: include_str!("../../../lang-packs/en/errors.toml"),
    lang_info_toml: include_str!("../../../lang-packs/en/lang_info.toml"),
    crates_data: &[],
};

/// 根据语言代码获取内置语言包数据
///
/// 新增语言时：在 [`get_builtin_data`] 中增加分支，并在本文件中添加对应 static 数据。
pub fn get_builtin_data(lang_code: &str) -> Option<&'static BuiltinLangData> {
    match lang_code {
        "zh" => Some(&ZH_DATA),
        "en" => Some(&EN_DATA),
        _ => None,
    }
}

/// 所有内置语言包的代码列表
///
/// 供 `rzc lang list` 展示与 `rzc lang remove` 的内置保护使用。
/// 其他语言通过 `rzc lang install` 从远程仓库安装。
pub fn builtin_lang_codes() -> Vec<&'static str> {
    vec!["zh", "en"]
}
