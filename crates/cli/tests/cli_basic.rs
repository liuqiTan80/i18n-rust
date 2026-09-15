//! rzc 子命令冒烟测试：进程级最小行为验证
//!
//! 只覆盖不依赖网络与外部工具链的面：`--version`、`lang list`（内置
//! 语言包编译期嵌入）、`transpile`（纯内存转译，不调 rustc/cargo）。
//! 编译诊断链路的端到端测试由 LSP e2e（crates/lsp/tests/e2e.rs）与
//! 教程验证（tools/verify-tutorials.py）覆盖。

use assert_cmd::Command;

fn rzc() -> Command {
    Command::cargo_bin("rzc").expect("应能定位 rzc 二进制")
}

/// `--version` 输出 `rzc <版本>`，且与 Cargo.toml 声明一致
#[test]
fn test_version_outputs_name_and_version() {
    rzc()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains("rzc"))
        .stdout(predicates::str::contains(env!("CARGO_PKG_VERSION")));
}

/// `lang list` 无需 RZ_LANG_DIR 与网络：内置 11 语言包编译期嵌入
#[test]
fn test_lang_list_lists_builtin_packs() {
    rzc()
        .args(["lang", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("11"))
        .stdout(predicates::str::contains("zh"));
}

/// `transpile` 纯内存转译：母语关键字映射为标准 Rust，不产生文件
#[test]
fn test_transpile_outputs_standard_rust() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let file = dir.path().join("main.zh");
    std::fs::write(&file, "函数 主函数() {\n    打印行!(\"你好\");\n}\n")
        .expect("写入测试源码失败");

    rzc()
        .arg("transpile")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicates::str::contains("fn main()"))
        .stdout(predicates::str::contains("println!"));
}

/// `rzc cheat --lang zh`：输出母语↔Rust 速查表（关键字节含 函数→fn）
#[test]
fn test_cheat_zh_outputs_mapping_table() {
    rzc()
        .args(["cheat", "zh"])
        .assert()
        .success()
        .stdout(predicates::str::contains("zh ↔ Rust"))
        .stdout(predicates::str::contains("函数"))
        .stdout(predicates::str::contains("fn"))
        .stdout(predicates::str::contains("Keywords"));
}

/// `rzc cheat --markdown`：输出可嵌入文档的 Markdown 表格
#[test]
fn test_cheat_markdown_table() {
    rzc()
        .args(["cheat", "zh", "--markdown"])
        .assert()
        .success()
        .stdout(predicates::str::contains("| zh | Rust |"))
        .stdout(predicates::str::contains("| 函数 | `fn` |"));
}

/// `rzc cheat ja`：非中文语言包同样可输出速查表（全球用户路径）
#[test]
fn test_cheat_ja_outputs_mapping_table() {
    rzc()
        .args(["cheat", "ja"])
        .assert()
        .success()
        .stdout(predicates::str::contains("ja ↔ Rust"))
        .stdout(predicates::str::contains("Keywords"));
}

/// `rzc init --lang` 缺省跟随系统 locale：de_DE 环境下缺省 de（全球用户第一个项目是母语）
#[test]
fn test_init_default_lang_follows_locale() {
    rzc()
        .args(["init", "--help"])
        .env("LC_ALL", "de_DE.UTF-8")
        .env("LANG", "de_DE.UTF-8")
        .assert()
        .success()
        .stdout(predicates::str::contains("[default: de]"));
}

/// 回归（#7）：`--no-lint` 静默教学 lint 提示
///
/// 项目开发（非教学）场景中，未标注类型等初学者提示每次转译刷屏；
/// 默认输出（stderr 含 `[lint]` 标签）与关闭后输出（stderr 空）对比验证。
/// HOME 隔离到临时目录：缓存（~/.rz）不跨测试/机器残留，内容各自
/// 不同强制缓存未命中（命中会跳过管线而不产生任何告警，断言失真）。
#[test]
fn test_no_lint_silences_teaching_hints() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let official = dir.path().join("official.zh");
    let silenced = dir.path().join("silenced.zh");
    // 两个文件内容不同（不同变量名）→ 两次都是缓存未命中，管线真实执行
    std::fs::write(
        &official,
        "函数 主函数() {\n    让 数量甲 = 5;\n    打印行!(\"{}\", 数量甲);\n}\n",
    )
    .expect("写入测试源码失败");
    std::fs::write(
        &silenced,
        "函数 主函数() {\n    让 数量乙 = 6;\n    打印行!(\"{}\", 数量乙);\n}\n",
    )
    .expect("写入测试源码失败");

    // 默认：教学 lint 告警输出到 stderr（含 [lint] 标签）
    rzc()
        .env("HOME", dir.path())
        .env_remove("RZ_LOG")
        .arg("transpile")
        .arg(&official)
        .assert()
        .success()
        .stderr(predicates::str::contains("[lint]"));

    // --no-lint：转译输出不变，教学 lint 告警静默（stderr 无输出）
    rzc()
        .env("HOME", dir.path())
        .env_remove("RZ_LOG")
        .args(["--no-lint", "transpile"])
        .arg(&silenced)
        .assert()
        .success()
        .stdout(predicates::str::contains("let 数量乙 = 6;"))
        .stderr(predicates::str::is_empty());
}
