//! 内置语言包 - 将默认语言包嵌入到可执行文件中
//!
//! 语言包 TOML 数据由引擎 crate 在编译期嵌入（include_str!），
//! 本模块通过 [`i18n_rust_engine::语言::内置文件`] 获取，
//! 使得 rzc 可执行文件无需附带语言包目录即可独立运行。
//! 通过 [`获取内置数据`] 按语言代码获取对应的内置语言包，
//! 未知语言代码自动回退到中文。

/// 单个语言的完整内置数据
pub struct 内置语言数据 {
    /// 关键字映射 TOML
    pub 关键字文本: &'static str,
    /// 模块路径映射 TOML
    pub 模块路径文本: &'static str,
    /// 标准库映射 TOML（模块路径 + 标识符别名）
    pub 标准库文本: &'static str,
    /// 错误消息翻译 TOML
    pub 错误文本: &'static str,
    /// 语言包信息 TOML（名称 / 扩展名 / 版本）
    pub 语言信息文本: &'static str,
    /// 界面消息 TOML（CLI / LSP 用户可见提示语）
    pub 界面文本: &'static str,
    /// 第三方库映射文件列表（文件名, 内容）
    pub 三方库数据: &'static [(&'static str, &'static str)],
}

/// 定义内置语言包的宏
///
/// 参数：
/// - `$名字`：生成的 `static` 变量名
/// - `$语言目录`：语言包目录名（如 `"zh"`、`"de"`）
///
/// 第三方库映射清单不再手工维护：从引擎编译期扫描结果
/// （[`i18n_rust_engine::语言::内置语言文件`]，build.rs 生成）动态过滤
/// `crates/` 前缀条目，与语言包目录（单一数据源）天然一致，
/// 新增/删除 crates 文件后无需修改本文件。
macro_rules! 定义内置语言 {
    ($名字:ident, $语言目录:literal) => {
        static $名字: std::sync::LazyLock<内置语言数据> = std::sync::LazyLock::new(|| {
            // 第三方库映射数组需 'static 引用，用 Box::leak 提升（仅初始化一次）
            let 三方库数据: &'static [(&'static str, &'static str)] = Box::leak(Box::new(
                i18n_rust_engine::语言::内置语言文件($语言目录)
                    .into_iter()
                    .filter_map(|(文件路径, 内容)| {
                        文件路径.strip_prefix("crates/").map(|名| (名, 内容))
                    })
                    .collect::<Vec<(&'static str, &'static str)>>(),
            ));
            内置语言数据 {
                关键字文本: 内置文件或崩溃($语言目录, "keywords.toml"),
                模块路径文本: 内置文件或崩溃($语言目录, "module_paths.toml"),
                标准库文本: 内置文件或崩溃($语言目录, "stdlib.toml"),
                错误文本: 内置文件或崩溃($语言目录, "errors.toml"),
                语言信息文本: 内置文件或崩溃($语言目录, "lang_info.toml"),
                界面文本: 内置文件或崩溃($语言目录, "ui.toml"),
                三方库数据,
            }
        });
    };
}

/// 从引擎内置语言包取文件内容（缺失时 panic，内置数据必须完整）
fn 内置文件或崩溃(语言: &str, 目标文件: &str) -> &'static str {
    i18n_rust_engine::语言::内置文件(语言, 目标文件).expect("内置语言包文件缺失：引擎未嵌入该文件")
}

// 中文内置语言包（完整翻译映射；第三方库映射清单自动纳入）
//
// 本分支为中文专属：其余语言包已从 crates/engine/lang-packs/ 移除，
// 不再定义对应 static（其 LazyLock 会因引擎未嵌入而 panic）。未知语言代码
// 一律经 获取内置数据 回退中文。
定义内置语言!(中文数据, "zh");

/// 根据语言代码获取内置语言包数据
///
/// 已知语言代码（`"zh"`）返回对应语言包；
/// **未知语言代码自动回退到中文**，保证任何语言设置下都有可用数据。
///
/// 内部数据由构建脚本（build.rs）从 `crates/engine/lang-packs/` 扫描生成，
/// 新增语言包后需重新构建；删除语言包（如英文，Rust 本就以英文书写，
/// 恒等映射无教学价值）时同步更新本文件与各引用点。
///
/// # 使用示例
///
/// 根据用户设置（如命令行参数、文件扩展名或环境变量）获取语言数据：
///
/// ```
/// // 用户设置的语言代码（实际来源可为 --语言包 参数或 .zh/.de 文件扩展名）
/// let 语言码 = std::env::var("RZ_LANG").unwrap_or_else(|_| "zh".to_string());
/// let 数据 = 获取内置数据(&语言码); // 未知代码自动回退中文
///
/// // 直接使用嵌入的 TOML 内容
/// println!("关键字映射: {}", 数据.关键字文本);
/// ```
///
/// 本分支为中文专属，[`获取内置数据`] 恒回退中文包；若需重新支持多语言，
/// 用 [`定义内置语言!`] 添加对应 static，并在 [`获取内置数据`] 恢复按代码分支的 `匹配`。
pub fn 获取内置数据(语言代码: &str) -> &内置语言数据 {
    // 本分支仅内置中文：任意语言代码（含他语/未知）一律回退中文包
    let _ = 语言代码;
    &中文数据
}

/// 判断语言代码是否有对应的内置语言包
///
/// 用于区分"已内置的语言"与"需通过 `rzc lang install` 远程安装的语言"，
/// 避免未知语言被静默回退到中文时用户无感知。
/// 单一事实源：引擎 lang-packs 目录（build.rs 自动生成）。
pub fn 拥有内置语言(语言代码: &str) -> bool {
    i18n_rust_engine::语言::拥有所属语言(语言代码)
}

/// 所有内置语言包的代码列表
///
/// 供 `rzc lang list` 展示与 `rzc lang remove` 的内置保护使用。
/// 其他语言通过 `rzc lang install` 从远程仓库安装。
/// 单一事实源：引擎 lang-packs 目录（build.rs 自动生成），
/// 避免代码清单与语言包目录、扩展名清单并行维护而漂移。
pub fn 内置语言代码() -> Vec<&'static str> {
    i18n_rust_engine::语言::内置语言代码()
}

#[cfg(test)]
mod 单元测试 {
    use super::*;

    /// 全部内置语言均能获取到数据，且 TOML 内容非空
    #[test]
    fn 测试获取内置数据已知语言() {
        for 语言码 in 内置语言代码() {
            let 数据 = 获取内置数据(语言码);
            assert!(!数据.关键字文本.is_empty(), "{语言码} keywords 为空");
            assert!(!数据.模块路径文本.is_empty(), "{语言码} module_paths 为空");
            assert!(!数据.标准库文本.is_empty(), "{语言码} stdlib 为空");
            assert!(!数据.错误文本.is_empty(), "{语言码} errors 为空");
            assert!(!数据.语言信息文本.is_empty(), "{语言码} lang_info 为空");
            assert!(!数据.界面文本.is_empty(), "{语言码} ui 为空");
            assert!(
                数据.界面文本.contains("\"界面消息\""),
                "{语言码} ui.toml 应包含 [界面消息] 节"
            );
        }
    }

    /// 未知语言代码回退到中文
    #[test]
    fn 测试未知语言回退中文() {
        let 数据 = 获取内置数据("xx");
        let 中文 = 获取内置数据("zh");
        // 回退数据与中文包为同一静态实例（指针相等）
        assert!(std::ptr::eq(数据, 中文), "未知语言应回退到中文包");
    }

    /// crates 清单与引擎编译期扫描结果逐项一致（单一数据源，杜绝手工清单漂移）；
    /// stdlib.toml 两节齐全（模块路径 + 标识符）
    #[test]
    fn 测试各语言三方库清单() {
        for 语言码 in 内置语言代码() {
            let 扫描结果: Vec<&str> = i18n_rust_engine::语言::内置语言文件(语言码)
                .into_iter()
                .filter_map(|(文件路径, _)| 文件路径.strip_prefix("crates/"))
                .collect();
            let 清单: Vec<&str> = 获取内置数据(语言码)
                .三方库数据
                .iter()
                .map(|(名, _)| *名)
                .collect();
            assert_eq!(清单, 扫描结果, "{语言码} crates 清单应与引擎扫描一致");
            assert!(!清单.is_empty(), "{语言码} 应含第三方库映射");
        }
        // zh 已内置 tauri 映射（桌面应用常用），防止清单再次遗漏
        assert!(
            获取内置数据("zh")
                .三方库数据
                .iter()
                .any(|(名, _)| *名 == "tauri.toml"),
            "zh 内置清单应包含 tauri.toml"
        );
        // stdlib.toml 中模块路径与标识符两节均存在
        let 数据 = 获取内置数据("zh");
        assert!(数据.标准库文本.contains("[\"模块路径\"]"));
        assert!(数据.标准库文本.contains("[\"标识符\"]"));
    }

    /// 各语言包元数据互相独立（名称不同）
    #[test]
    fn 测试语言信息互异() {
        let 代码列表 = 内置语言代码();
        let 信息: Vec<&str> = 代码列表
            .iter()
            .map(|语言码| 获取内置数据(语言码).语言信息文本)
            .collect();
        for (序号, 甲) in 信息.iter().enumerate() {
            for (次序号, 乙) in 信息[序号 + 1..].iter().enumerate() {
                assert_ne!(
                    甲,
                    乙,
                    "{} 与 {} 的 lang_info 相同",
                    代码列表[序号],
                    代码列表[序号 + 1 + 次序号]
                );
            }
        }
    }

    /// 内置代码列表与 拥有内置语言 一致
    #[test]
    fn 测试内置代码一致() {
        let 代码列表 = 内置语言代码();
        for 语言码 in 代码列表 {
            assert!(拥有内置语言(语言码), "{语言码} 应在内置列表中");
        }
        assert!(!拥有内置语言("xx"));
    }
}
