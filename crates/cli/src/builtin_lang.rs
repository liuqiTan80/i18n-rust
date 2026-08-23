// 内置语言包 - 将默认语言包嵌入到可执行文件中
//
// 语言包 TOML 数据由引擎 crate 在编译期嵌入（include_str!），
// 本模块通过 [`i18n_rust_engine::语言::builtin_file`] 获取，
// 使得 rzc 可执行文件无需附带语言包目录即可独立运行。
// 通过 [`get_builtin_data`] 按语言代码获取对应的内置语言包，
// 未知语言代码自动回退到中文。

/// 单个语言的完整内置数据
pub struct BuiltinLangData {
    /// 关键字映射 TOML
    pub keywords_toml: &'static str,
    /// 模块路径映射 TOML
    pub module_paths_toml: &'static str,
    /// 标准库映射 TOML（模块路径 + 标识符别名）
    pub stdlib_toml: &'static str,
    /// 错误消息翻译 TOML
    pub errors_toml: &'static str,
    /// 语言包信息 TOML（名称 / 扩展名 / 版本）
    pub lang_info_toml: &'static str,
    /// 界面消息 TOML（CLI / LSP 用户可见提示语）
    pub ui_toml: &'static str,
    /// 第三方库映射文件列表（文件名, 内容）
    pub crates_data: &'static [(&'static str, &'static str)],
}

/// 定义内置语言包的宏
///
/// 参数：
/// - `$name`：生成的 `static` 变量名
/// - `$lang_dir`：语言包目录名（如 `"zh"`、`"de"`）
/// - `$($crate_file),*`：第三方库映射文件名列表，
///   文件名随语言包本地化（中文为 `序列化.toml`、俄语为 `Сериализация.toml`、
///   日语为 `直列化.toml` 等，salvo.toml 为 crate 专有名词保持不变），
///   需与对应语言包 `crates/` 目录下的文件一致
macro_rules! define_builtin_lang {
    ($name:ident, $lang_dir:literal, [$($crate_file:literal),* $(,)?]) => {
        static $name: std::sync::LazyLock<BuiltinLangData> = std::sync::LazyLock::new(|| {
            // 第三方库映射数组需 'static 引用，用 Box::leak 提升（仅初始化一次）
            let crates_data: &'static [(&'static str, &'static str)] = Box::leak(Box::new([
                $((
                    $crate_file,
                    builtin_file_or_panic($lang_dir, concat!("crates/", $crate_file)),
                ),)*
            ]));
            BuiltinLangData {
                keywords_toml: builtin_file_or_panic($lang_dir, "keywords.toml"),
                module_paths_toml: builtin_file_or_panic($lang_dir, "module_paths.toml"),
                stdlib_toml: builtin_file_or_panic($lang_dir, "stdlib.toml"),
                errors_toml: builtin_file_or_panic($lang_dir, "errors.toml"),
                lang_info_toml: builtin_file_or_panic($lang_dir, "lang_info.toml"),
                ui_toml: builtin_file_or_panic($lang_dir, "ui.toml"),
                crates_data,
            }
        });
    };
}

/// 从引擎内置语言包取文件内容（缺失时 panic，内置数据必须完整）
fn builtin_file_or_panic(lang: &str, file: &str) -> &'static str {
    i18n_rust_engine::语言::builtin_file(lang, file).expect("内置语言包文件缺失：引擎未嵌入该文件")
}

// 中文内置语言包（完整翻译映射 + 10 个第三方库映射）
define_builtin_lang!(
    ZH_DATA,
    "zh",
    [
        "序列化.toml",
        "异步.toml",
        "命令行.toml",
        "数据库.toml",
        "工具.toml",
        "日志.toml",
        "网络.toml",
        "错误处理.toml",
        "Web框架.toml",
        "salvo.toml",
    ]
);

// 德语内置语言包（德语错误教学提示 + 10 个第三方库映射）
define_builtin_lang!(
    DE_DATA,
    "de",
    [
        "Serialisierung.toml",
        "Asynchron.toml",
        "Kommandozeile.toml",
        "Datenbank.toml",
        "Werkzeuge.toml",
        "Protokollierung.toml",
        "Netzwerk.toml",
        "Fehlerbehandlung.toml",
        "Web_Framework.toml",
        "salvo.toml",
    ]
);

// 日语内置语言包
define_builtin_lang!(
    JA_DATA,
    "ja",
    [
        "直列化.toml",
        "非同期.toml",
        "コマンドライン.toml",
        "データベース.toml",
        "ユーティリティ.toml",
        "ロギング.toml",
        "ネットワーク.toml",
        "エラー処理.toml",
        "Webフレームワーク.toml",
        "salvo.toml",
    ]
);

// 俄语内置语言包
define_builtin_lang!(
    RU_DATA,
    "ru",
    [
        "Сериализация.toml",
        "Асинхронность.toml",
        "Командная_строка.toml",
        "База_данных.toml",
        "Утилиты.toml",
        "Логирование.toml",
        "Сеть.toml",
        "Обработка_ошибок.toml",
        "Веб_фреймворк.toml",
        "salvo.toml",
    ]
);

// 西班牙语内置语言包
define_builtin_lang!(
    ES_DATA,
    "es",
    [
        "Serialización.toml",
        "Asíncrono.toml",
        "Línea_de_comandos.toml",
        "Base_de_datos.toml",
        "Utilidades.toml",
        "Registro.toml",
        "Red.toml",
        "Manejo_de_errores.toml",
        "Marco_Web.toml",
        "salvo.toml",
    ]
);

// 法语内置语言包
define_builtin_lang!(
    FR_DATA,
    "fr",
    [
        "Sérialisation.toml",
        "Asynchrone.toml",
        "Ligne_de_commande.toml",
        "Base_de_données.toml",
        "Utilitaires.toml",
        "Journalisation.toml",
        "Réseau.toml",
        "Gestion_des_erreurs.toml",
        "Framework_Web.toml",
        "salvo.toml",
    ]
);

// 葡萄牙语内置语言包
define_builtin_lang!(
    PT_DATA,
    "pt",
    [
        "Serialização.toml",
        "Assíncrono.toml",
        "Linha_de_comando.toml",
        "Banco_de_dados.toml",
        "Utilitários.toml",
        "Registro.toml",
        "Rede.toml",
        "Tratamento_de_erros.toml",
        "Framework_Web.toml",
        "salvo.toml",
    ]
);

// 韩语内置语言包
define_builtin_lang!(
    KO_DATA,
    "ko",
    [
        "직렬화.toml",
        "비동기.toml",
        "명령줄.toml",
        "데이터베이스.toml",
        "유틸리티.toml",
        "로깅.toml",
        "네트워크.toml",
        "오류_처리.toml",
        "웹_프레임워크.toml",
        "salvo.toml",
    ]
);

// 阿拉伯语内置语言包
define_builtin_lang!(
    AR_DATA,
    "ar",
    [
        "تسلسل.toml",
        "غير_متزامن.toml",
        "سطر_الأوامر.toml",
        "قاعدة_البيانات.toml",
        "أدوات.toml",
        "تتبع.toml",
        "شبكة.toml",
        "معالجة_الأخطاء.toml",
        "إطار_الويب.toml",
        "salvo.toml",
    ]
);

// 印地语内置语言包
define_builtin_lang!(
    HI_DATA,
    "hi",
    [
        "क्रमबद्धन.toml",
        "अतुल्यकालिक.toml",
        "आदेश_पंक्ति.toml",
        "डेटाबेस.toml",
        "उपयोगिता.toml",
        "अनुरेखण.toml",
        "नेटवर्क.toml",
        "त्रुटि_प्रबंधन.toml",
        "वेब_फ्रेमवर्क.toml",
        "salvo.toml",
    ]
);

/// 根据语言代码获取内置语言包数据
///
/// 已知语言代码（`"zh"` / `"de"`）返回对应语言包；
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
/// let lang_code = std::env::var("RZ_LANG").unwrap_or_else(|_| "zh".to_string());
/// let data = get_builtin_data(&lang_code); // 未知代码自动回退中文
///
/// // 直接使用嵌入的 TOML 内容
/// println!("关键字映射: {}", data.keywords_toml);
/// ```
///
/// 新增语言时：在 [`get_builtin_data`] 与 [`has_builtin_lang`] 中增加分支，
/// 用 [`define_builtin_lang!`] 添加对应 static 数据，并更新 [`builtin_lang_codes`]。
pub fn get_builtin_data(lang_code: &str) -> &BuiltinLangData {
    match lang_code {
        "zh" => &ZH_DATA,
        "de" => &DE_DATA,
        "ja" => &JA_DATA,
        "ru" => &RU_DATA,
        "es" => &ES_DATA,
        "fr" => &FR_DATA,
        "pt" => &PT_DATA,
        "ko" => &KO_DATA,
        "ar" => &AR_DATA,
        "hi" => &HI_DATA,
        // 未知语言代码回退到中文（教学语言默认值）
        _ => &ZH_DATA,
    }
}

/// 判断语言代码是否有对应的内置语言包
///
/// 用于区分"已内置的语言"与"需通过 `rzc lang install` 远程安装的语言"，
/// 避免未知语言被静默回退到中文时用户无感知。
pub fn has_builtin_lang(lang_code: &str) -> bool {
    matches!(
        lang_code,
        "zh" | "de" | "ja" | "ru" | "es" | "fr" | "pt" | "ko" | "ar" | "hi"
    )
}

/// 所有内置语言包的代码列表
///
/// 供 `rzc lang list` 展示与 `rzc lang remove` 的内置保护使用。
/// 其他语言通过 `rzc lang install` 从远程仓库安装。
pub fn builtin_lang_codes() -> Vec<&'static str> {
    vec!["zh", "de", "ja", "ru", "es", "fr", "pt", "ko", "ar", "hi"]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 全部内置语言均能获取到数据，且 TOML 内容非空
    #[test]
    fn test_get_builtin_data_known_langs() {
        for code in [
            "zh", "en", "de", "ja", "ru", "es", "fr", "pt", "ko", "ar", "hi",
        ] {
            let data = get_builtin_data(code);
            assert!(!data.keywords_toml.is_empty(), "{code} keywords 为空");
            assert!(
                !data.module_paths_toml.is_empty(),
                "{code} module_paths 为空"
            );
            assert!(!data.stdlib_toml.is_empty(), "{code} stdlib 为空");
            assert!(!data.errors_toml.is_empty(), "{code} errors 为空");
            assert!(!data.lang_info_toml.is_empty(), "{code} lang_info 为空");
            assert!(!data.ui_toml.is_empty(), "{code} ui 为空");
            assert!(
                data.ui_toml.contains("\"界面消息\""),
                "{code} ui.toml 应包含 [界面消息] 节"
            );
        }
    }

    /// 未知语言代码回退到中文
    #[test]
    fn test_get_builtin_data_unknown_falls_back_to_zh() {
        let data = get_builtin_data("xx");
        let zh = get_builtin_data("zh");
        // 回退数据与中文包为同一静态实例（指针相等）
        assert!(std::ptr::eq(data, zh), "未知语言应回退到中文包");
    }

    /// 全部内置语言均含 10 个第三方库映射；
    /// stdlib.toml 两节齐全（模块路径 + 标识符）
    #[test]
    fn test_crates_data_per_lang() {
        for code in ["zh", "de", "ja", "ru", "es", "fr", "pt", "ko", "ar", "hi"] {
            assert_eq!(
                get_builtin_data(code).crates_data.len(),
                10,
                "{code} 应含 10 个第三方库映射"
            );
        }
        // stdlib.toml 中模块路径与标识符两节均存在
        for data in [get_builtin_data("zh"), get_builtin_data("de")] {
            assert!(data.stdlib_toml.contains("[\"模块路径\"]"));
            assert!(data.stdlib_toml.contains("[\"标识符\"]"));
        }
    }

    /// 各语言包元数据互相独立（名称不同）
    #[test]
    fn test_lang_info_distinct() {
        let codes = builtin_lang_codes();
        let infos: Vec<&str> = codes
            .iter()
            .map(|code| get_builtin_data(code).lang_info_toml)
            .collect();
        for (i, a) in infos.iter().enumerate() {
            for (j, b) in infos[i + 1..].iter().enumerate() {
                assert_ne!(
                    a,
                    b,
                    "{} 与 {} 的 lang_info 相同",
                    codes[i],
                    codes[i + 1 + j]
                );
            }
        }
    }

    /// 内置代码列表与 has_builtin_lang 一致
    #[test]
    fn test_builtin_codes_consistent() {
        let codes = builtin_lang_codes();
        for code in codes {
            assert!(has_builtin_lang(code), "{code} 应在内置列表中");
        }
        assert!(!has_builtin_lang("xx"));
    }
}
