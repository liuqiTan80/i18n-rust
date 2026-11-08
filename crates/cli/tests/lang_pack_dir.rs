//! 语言包目录加载端到端：`--lang-pack` 显式目录（库目录下全部映射文件）
//!
//! 以仓库内 zh 语言包为对象（含 `rzc mapping auto` 生成的第三方库映射，
//! 如 tauri 2.11.5），验证文件系统加载链把 `crates/*.toml` 词条接入转译：
//! 母语词条必须替换为对应 Rust 名称（应用事件→事件类型、网址→网址类型），
//! 证明新生成的映射产物可直接用于真实项目。

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;

fn 命令构造器() -> Command {
    Command::cargo_bin("rzc").expect("应能定位 rzc 二进制")
}

/// 仓库内 zh 语言包目录（单一数据源：crates/engine/lang-packs/）
fn 中文语言包目录() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/engine/lang-packs/zh")
}

/// tauri 映射端到端：母语词条经 `--lang-pack` 目录加载后正确替换，不残留
#[test]
fn 测试_桌面框架映射词条从语言包目录转译() {
    let 目录 = tempfile::tempdir().expect("创建临时目录失败");
    let file = 目录.path().join("main.zh");
    std::fs::write(
        &file,
        "函数 主函数() {\n    让 事件示例 = 应用事件::默认();\n    让 地址 = 网址::解析(\"https://tauri.app\");\n}\n",
    )
    .expect("写入测试源码失败");

    命令构造器()
        .arg("transpile")
        .arg(&file)
        .arg("--lang-pack")
        .arg(中文语言包目录())
        .assert()
        .success()
        .stdout(predicates::str::contains("fn main()"))
        .stdout(predicates::str::contains("Event::"))
        .stdout(predicates::str::contains("Url::"))
        .stdout(predicates::str::contains("应用事件").not())
        .stdout(predicates::str::contains("网址").not());
}
