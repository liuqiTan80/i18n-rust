//! LSP 启动时是否把界面语言同步给引擎（缺陷 C 回归）
//!
//! LSP 自己的 `lsp_*` 消息走 `crates/lsp/src/ui.rs` 的加载链，解析与代入落在
//! 引擎侧的 `crates/engine/src/界面消息.rs`；引擎的用户可见
//! 文本（Unicode 混淆 / 全角标点 / 教学 lint / 诊断模板）走 `语言::` 的**全局
//! 语言**。改前 `init` 只加载前者、从不调 `语言::设定语言`（CLI 在启动时
//! 同步），于是德语/俄文用户的编辑器里「界面消息是母语、引擎教学文本恒为中文」。
//!
//! 本文件单独成一个测试二进制：`init` 写的是进程级全局（GLOBAL 与 语言::当前语言码），
//! 不能与依赖默认语言的用例同进程并行。

use std::path::PathBuf;

use i18n_rust_engine::unicode_confusion::check_unicode_confusion;
use i18n_rust_engine::语言;

/// 按语言码定位引擎内置语言包目录（测试运行时 cwd 为 crates/lsp）
fn lang_pack(lang_code: &str) -> PathBuf {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    manifest.join(format!("../engine/lang-packs/{lang_code}"))
}

/// `init` 必须同步引擎全局语言，并消除西里尔标识符的假告警
#[test]
fn init_syncs_engine_language_and_kills_native_script_false_positives() {
    // 俄文方言的合法源码：每个西里尔字母都命中同形异义表
    let source = "функция главная() {\n    пусть количество = 5;\n}\n";

    // 钉住「全局语言 = zh」这一改前状态：此时同形判定照常报（不是检测坏了，
    // 而是 LSP 根本没把用户语言告诉引擎）
    语言::设定语言("zh");
    assert_eq!(语言::当前语言(), "zh");
    assert!(
        !check_unicode_confusion(source).is_empty(),
        "zh 语境下西里尔标识符应被判为同形异义（这正是 LSP 改前的实际状态）"
    );

    i18n_rust_lsp::ui::init(&lang_pack("ru"));

    assert_eq!(
        语言::当前语言(),
        "ru",
        "init 应把命中的语言代码同步给引擎（改前从不调 设定语言）"
    );
    assert!(
        check_unicode_confusion(source).is_empty(),
        "ru 语境下合法的西里尔标识符不该报同形异义，实际：{:?}",
        check_unicode_confusion(source)
    );
    // 引擎模板也随语言切换：同一键在 ru 表里是俄文，不再恒为中文
    assert_eq!(语言::取消息("unicode_cat_homoglyph"), "символ-омоглиф");
    // LSP 自己的消息表同一条链，界面语言与引擎语言不得分叉
    assert_eq!(
        i18n_rust_lsp::ui::global().t("lsp_about"),
        "LSP-прокси-сервер i18n-rust"
    );
}

/// 语言代码与消息表由同一条链产出（`load_with_lang` 纯函数入口，不碰全局）
#[test]
fn load_with_lang_reports_the_language_it_actually_used() {
    let (ui, lang) = i18n_rust_lsp::ui::Ui::load_with_lang(&lang_pack("de"));
    assert_eq!(lang, "de", "显式语言包目录应按目录名登记语言");
    assert_eq!(ui.t("lsp_about"), "i18n-rust LSP-Proxy-Server");

    let (ui, lang) = i18n_rust_lsp::ui::Ui::load_with_lang(&lang_pack("zh"));
    assert_eq!(lang, "zh");
    assert_eq!(ui.t("lsp_about"), "i18n-rust LSP 代理服务器");

    // 目录里没有 ui.toml 时走目录名匹配内置包，语言代码同样要说清（改前这里
    // 只回消息表、语言代码无人知晓，两条链路就分叉了）
    let (ui, lang) = i18n_rust_lsp::ui::Ui::load_with_lang(std::path::Path::new("/任意路径/ja"));
    assert_eq!(lang, "ja");
    assert!(
        !ui.t("lsp_about").contains("LSP 代理服务器"),
        "ja 包的消息表不该回退成中文：{}",
        ui.t("lsp_about")
    );
}
