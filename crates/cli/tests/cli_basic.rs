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

/// `lang list` 无需 RZ_LANG_DIR 与网络：内置 10 语言包编译期嵌入
#[test]
fn test_lang_list_lists_builtin_packs() {
    rzc()
        .args(["lang", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("10"))
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
