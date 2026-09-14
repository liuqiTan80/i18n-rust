//! 语言包目录加载端到端：`--lang-pack` 显式目录（crates/ 下全部映射文件）
//!
//! 以仓库内 zh 语言包为对象（含 `rzc mapping auto` 生成的第三方库映射，
//! 如 tauri 2.11.5），验证文件系统加载链把 `crates/*.toml` 词条接入转译：
//! 母语词条必须替换为对应 Rust 名称（应用事件→Event、网址→Url），
//! 证明新生成的映射产物可直接用于真实项目。

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;

fn rzc() -> Command {
    Command::cargo_bin("rzc").expect("应能定位 rzc 二进制")
}

/// 仓库内 zh 语言包目录（单一数据源：crates/engine/lang-packs/）
fn zh_pack_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/engine/lang-packs/zh")
}

/// tauri 映射端到端：母语词条经 --lang-pack 目录加载后正确替换，不残留
#[test]
fn test_tauri_mapping_entries_transpile_from_pack_dir() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let file = dir.path().join("main.zh");
    std::fs::write(
        &file,
        "函数 主函数() {\n    让 事件示例 = 应用事件::默认();\n    让 地址 = 网址::解析(\"https://tauri.app\");\n}\n",
    )
    .expect("写入测试源码失败");

    rzc()
        .arg("transpile")
        .arg(&file)
        .arg("--lang-pack")
        .arg(zh_pack_dir())
        .assert()
        .success()
        .stdout(predicates::str::contains("fn main()"))
        .stdout(predicates::str::contains("Event::"))
        .stdout(predicates::str::contains("Url::"))
        .stdout(predicates::str::contains("应用事件").not())
        .stdout(predicates::str::contains("网址").not());
}
