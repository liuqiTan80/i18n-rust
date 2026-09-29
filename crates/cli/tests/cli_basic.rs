//! rzc 子命令冒烟测试：进程级最小行为验证
//!
//! 只覆盖不依赖网络与外部工具链的面：`--version`、`lang list`（内置
//! 语言包编译期嵌入）、`transpile`（纯内存转译，不调 rustc/cargo）。
//! 编译诊断链路的端到端测试由 LSP e2e（crates/lsp/tests/e2e.rs）与
//! 教程验证（tools/verify-tutorials.py）覆盖。

use assert_cmd::Command;
use std::path::PathBuf;
use std::sync::OnceLock;

/// 测试沙箱根目录（进程内唯一，仅创建一次）
///
/// 测试必须与开发者机器隔离：`~/.rz/lang-packs` 中的用户安装语言包会改变
/// `lang list` 的计数（断言「11」失真），`~/.rz` 缓存会影响转译告警输出。
/// 统一把 HOME / USERPROFILE / RZ_LANG_DIR 指向空沙箱，使结果仅取决于
/// 编译期嵌入的内置语言包与源码本身。
fn 沙箱根() -> &'static PathBuf {
    static 根: OnceLock<PathBuf> = OnceLock::new();
    根.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("rzc-test-sandbox-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("创建测试沙箱目录失败");
        dir
    })
}

fn rzc() -> Command {
    let mut cmd = Command::cargo_bin("rzc").expect("应能定位 rzc 二进制");
    let 沙箱 = 沙箱根();
    cmd.env("HOME", 沙箱)
        .env("USERPROFILE", 沙箱)
        .env("RZ_LANG_DIR", 沙箱.join("lang-packs"))
        .env_remove("RZ_LOG");
    cmd
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

/// `lang list` 无需网络：内置 11 语言包编译期嵌入；沙箱内无用户安装包，
/// 计数稳定为 11（不随开发者机器上的 `~/.rz/lang-packs` 变化）
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

// ========== 进程级子命令链路（真实 rustc / 文件系统，隔离 HOME） ==========

/// 新建隔离 HOME 的临时目录（~/.rz 缓存不跨测试污染）
fn isolated_home() -> tempfile::TempDir {
    tempfile::tempdir().expect("创建 HOME 临时目录失败")
}

/// 好程序：编译运行必过、无警告
const GOOD_PROGRAM: &str = "函数 主函数() {\n    打印行!(\"输出：{}\", 42);\n}\n";

/// 坏程序：E0384（不可变变量重复赋值），CLI 须翻译成母语教学诊断
const BAD_PROGRAM: &str =
    "函数 主函数() {\n    让 数量 = 10;\n    数量 = 数量 + 1;\n    打印行!(\"{}\", 数量);\n}\n";

/// `rzc init` 生成完整项目骨架
#[test]
fn test_init_creates_project_skeleton() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    rzc()
        .env("HOME", home.path())
        .current_dir(work.path())
        .args(["init", "demo", "--lang", "zh"])
        .assert()
        .success()
        .stdout(predicates::str::contains("demo"));
    assert!(
        work.path().join("demo/Cargo.toml").is_file(),
        "应生成 Cargo.toml"
    );
    assert!(
        work.path().join("demo/src/main.zh").is_file(),
        "应生成 src/main.zh 入口样板"
    );
}

/// `rzc run`：无依赖教学项目走 rustc 直调路径，程序 stdout 原样透传
#[test]
fn test_run_executes_transpiled_program() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    // init 骨架后覆盖为可断言输出的程序
    rzc()
        .env("HOME", home.path())
        .current_dir(work.path())
        .args(["init", "run-demo", "--lang", "zh"])
        .assert()
        .success();
    let main_zh = work.path().join("run-demo/src/main.zh");
    std::fs::write(&main_zh, GOOD_PROGRAM).expect("写入 main.zh 失败");

    rzc()
        .env("HOME", home.path())
        .env_remove("RZ_LOG")
        .current_dir(work.path().join("run-demo"))
        .args(["run", "src/main.zh"])
        .assert()
        .success()
        .stdout(predicates::str::contains("输出：42"));
}

/// `rzc check` 好程序：退出码 0
#[test]
fn test_check_good_program_succeeds() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(work.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        work.path().join("Cargo.toml"),
        "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(work.path().join("src/main.zh"), GOOD_PROGRAM).unwrap();
    rzc()
        .env("HOME", home.path())
        .env_remove("RZ_LOG")
        .current_dir(work.path())
        .args(["check", "src/main.zh"])
        .assert()
        .success();
}

/// `rzc check` 多层模块项目：递归收集 + 非入口文件按层级补 `#[path]`
///
/// 布局：src/main.zh → src/领域.zh → src/领域/工具.zh。此前非入口方言文件
/// 内的文件式 `mod` 声明没有层级路径注解，rustc 报 E0754/E0583/E0603。
#[test]
fn test_check_nested_module_project_succeeds() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(work.path().join("src/领域")).expect("创建嵌套目录失败");
    std::fs::write(
        work.path().join("Cargo.toml"),
        "[package]\nname=\"nested\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        work.path().join("src/main.zh"),
        "模块 领域;\n\n函数 主函数() {\n    打印行!(\"{}\", 领域::工具::加一(1));\n}\n",
    )
    .unwrap();
    std::fs::write(work.path().join("src/领域.zh"), "公开 模块 工具;\n").unwrap();
    std::fs::write(
        work.path().join("src/领域/工具.zh"),
        "公开 函数 加一(数: i32) -> i32 {\n    数 + 1\n}\n",
    )
    .unwrap();

    rzc()
        .env("HOME", home.path())
        .env_remove("RZ_LOG")
        .current_dir(work.path())
        .args(["check", "src/main.zh"])
        .assert()
        .success();

    // 产物原位写回且嵌套路径注解指向「父词干/子词干.rs」
    let domain_rs =
        std::fs::read_to_string(work.path().join("src/领域.rs")).expect("领域.rs 应已转译");
    assert!(
        domain_rs.contains("#[path") && domain_rs.contains("领域/工具.rs"),
        "非入口文件应按层级注解嵌套子模块：{domain_rs}"
    );
    assert!(
        domain_rs.contains("pub mod 工具"),
        "嵌套子模块应保持 pub（防 E0603）：{domain_rs}"
    );
    let main_rs = std::fs::read_to_string(work.path().join("src/main.rs")).expect("main.rs 应存在");
    assert!(
        main_rs.contains("#[path") && main_rs.contains("领域.rs"),
        "入口应注解顶层模块路径：{main_rs}"
    );
}

/// `rzc check` 坏程序：退出码非 0，诊断为母语（E0425 等错误码 + 中文消息 +
/// 💡 教学提示），不泄漏 rustc 英文原文
#[test]
fn test_check_bad_program_emits_translated_diagnostic() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(work.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        work.path().join("Cargo.toml"),
        "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(work.path().join("src/main.zh"), BAD_PROGRAM).unwrap();
    let output = rzc()
        .env("HOME", home.path())
        .env_remove("RZ_LOG")
        .current_dir(work.path())
        .args(["check", "src/main.zh"])
        .assert()
        .failure();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.get_output().stdout),
        String::from_utf8_lossy(&output.get_output().stderr)
    );
    assert!(
        combined.contains("E0384"),
        "应保留 rustc 错误码：{combined}"
    );
    assert!(
        combined.contains("不可变变量"),
        "错误消息应翻译为母语：{combined}"
    );
    assert!(combined.contains('💡'), "应附教学提示：{combined}");
}

/// `rzc eject`：导出标准 Rust 文件（零锁定毕业路径）
#[test]
fn test_eject_writes_standard_rust_file() {
    let home = isolated_home();
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let zh = dir.path().join("main.zh");
    std::fs::write(&zh, GOOD_PROGRAM).unwrap();
    rzc()
        .env("HOME", home.path())
        .args(["eject"])
        .arg(&zh)
        .assert()
        .success()
        .stdout(predicates::str::contains("main.rs"));
    let rs = std::fs::read_to_string(dir.path().join("main.rs")).expect("main.rs 应已导出");
    assert!(rs.contains("fn main()"), "导出应为标准 Rust：{rs}");
    assert!(rs.contains("println!"), "宏应还原为标准 Rust：{rs}");
}

/// `rzc doctor`：组件体检始终退出 0 并输出工具链/语言包分区
#[test]
fn test_doctor_prints_health_report() {
    let home = isolated_home();
    rzc()
        .env("HOME", home.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicates::str::contains("rzc doctor"))
        .stdout(predicates::str::contains("语言包"));
}

/// `rzc mapping check`：对内置 zh 语言包跑冲突检测，基线干净时退出 0
#[test]
fn test_mapping_check_clean_lang_pack() {
    let home = isolated_home();
    let lang_pack = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../engine/lang-packs/zh");
    rzc()
        .env("HOME", home.path())
        .args(["mapping", "check"])
        .arg(&lang_pack)
        .assert()
        .success();
}

/// 输入文件不存在：以非零退出码与 Error 行失败，而非 panic 堆栈
#[test]
fn test_missing_source_file_fails_cleanly() {
    let home = isolated_home();
    let missing = std::env::temp_dir().join(format!("rzc-missing-{}.zh", std::process::id()));
    rzc()
        .env("HOME", home.path())
        .arg("transpile")
        .arg(&missing)
        .assert()
        .failure()
        .stderr(predicates::str::starts_with("Error:"));
}

/// `rzc mapping check`（无参数）：全内置语言校验 + 跨语言条目对比 +
/// 跨语言完整性门禁，基线干净时退出 0
#[test]
fn test_mapping_check_all_builtin_and_cross_lang() {
    let home = isolated_home();
    rzc()
        .env("HOME", home.path())
        .args(["mapping", "check"])
        .assert()
        .success();
}

/// `rzc mapping check <未知目标>`：非目录非内置代码 → 非零退出 + Error
#[test]
fn test_mapping_check_unknown_target_fails() {
    let home = isolated_home();
    rzc()
        .env("HOME", home.path())
        .args(["mapping", "check", "no-such-lang-xyz"])
        .assert()
        .failure()
        .stderr(predicates::str::starts_with("Error:"));
}

/// `rzc mapping check <坏语言包目录>`：crates TOML 重复键 → 校验失败，
/// 报告渲染出问题文件名（render_issue 本地化链路）
#[test]
fn test_mapping_check_broken_external_pack_fails() {
    let home = isolated_home();
    let pack = tempfile::tempdir().expect("创建语言包目录失败");
    std::fs::write(
        pack.path().join("keywords.toml"),
        "[\"声明\"]\n\"函数\" = \"fn\"\n",
    )
    .unwrap();
    let crates_dir = pack.path().join("crates");
    std::fs::create_dir_all(&crates_dir).unwrap();
    std::fs::write(
        crates_dir.join("broken.toml"),
        "[\"标识符\"]\n\"键\" = \"a\"\n\"键\" = \"b\"\n",
    )
    .unwrap();
    let output = rzc()
        .env("HOME", home.path())
        .args(["mapping", "check"])
        .arg(pack.path())
        .assert()
        .failure();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.get_output().stdout),
        String::from_utf8_lossy(&output.get_output().stderr)
    );
    assert!(
        combined.contains("broken.toml"),
        "报告应点名问题文件：{combined}"
    );
}

/// 本地语言包生命周期：从目录安装 → list 可见 → 删除 → 再删报错；
/// 非法语言代码（路径穿越）被拒绝。全程离线
#[test]
fn test_lang_local_install_list_remove_lifecycle() {
    let home = isolated_home();
    let staging = tempfile::tempdir().expect("创建暂存目录失败");
    let pack = staging.path().join("zz");
    std::fs::create_dir_all(&pack).unwrap();
    std::fs::write(
        pack.join("keywords.toml"),
        "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n",
    )
    .unwrap();

    // 非法目录（无 keywords.toml）安装失败
    let bad = staging.path().join("yy");
    std::fs::create_dir_all(&bad).unwrap();
    rzc()
        .env("HOME", home.path())
        .args(["lang", "install"])
        .arg(&bad)
        .assert()
        .failure()
        .stderr(predicates::str::starts_with("Error:"));

    // 合法目录安装成功
    rzc()
        .env("HOME", home.path())
        .args(["lang", "install"])
        .arg(&pack)
        .assert()
        .success();
    // list 中可见 zz（用户安装标记）
    rzc()
        .env("HOME", home.path())
        .args(["lang", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("zz"));
    // 重复安装（无 --force）失败
    rzc()
        .env("HOME", home.path())
        .args(["lang", "install"])
        .arg(&pack)
        .assert()
        .failure();
    // --force 重装成功
    rzc()
        .env("HOME", home.path())
        .args(["lang", "install", "--force"])
        .arg(&pack)
        .assert()
        .success();
    // 删除成功
    rzc()
        .env("HOME", home.path())
        .args(["lang", "remove", "zz"])
        .assert()
        .success();
    // 再删：已不存在 → 失败
    rzc()
        .env("HOME", home.path())
        .args(["lang", "remove", "zz"])
        .assert()
        .failure();
    // 路径穿越代码被拒绝
    rzc()
        .env("HOME", home.path())
        .args(["lang", "remove", "../evil"])
        .assert()
        .failure();
}

/// `rzc mapping scaffold`（rule 提供方）：纯本地生成 TODO 骨架，无需网络；
/// 未知源语言报错
#[test]
fn test_mapping_scaffold_rule_local() {
    let home = isolated_home();
    let out = tempfile::tempdir().expect("创建输出目录失败");
    rzc()
        .env("HOME", home.path())
        .args([
            "mapping",
            "scaffold",
            "zh",
            "xx",
            "--provider",
            "rule",
            "--output",
        ])
        .arg(out.path())
        .assert()
        .success();
    // 输出目录应至少生成一个带 TODO 注释的 crates TOML
    let mut found_todo = false;
    for entry in walkdir(out.path()) {
        if entry.extension().and_then(|e| e.to_str()) == Some("toml")
            && std::fs::read_to_string(&entry)
                .map(|c| c.contains("TODO(xx)"))
                .unwrap_or(false)
        {
            found_todo = true;
            break;
        }
    }
    assert!(
        found_todo,
        "应在 {} 下生成带 TODO(xx) 的骨架",
        out.path().display()
    );
    // 未知源语言：失败
    rzc()
        .env("HOME", home.path())
        .args(["mapping", "scaffold", "no-such-lang", "xx", "--output"])
        .arg(out.path())
        .assert()
        .failure()
        .stderr(predicates::str::starts_with("Error:"));
}

/// 递归列出目录内文件（测试小工具）
fn walkdir(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                out.extend(walkdir(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// `rzc mapping coverage zh`：对后端真实源码跑语料覆盖矩阵，
/// 需在仓库目录内执行（向上找工作区根）
#[test]
fn test_mapping_coverage_on_real_backend_corpus() {
    let home = isolated_home();
    // 向上找根时命中第一个含 Cargo.toml 的目录，故必须直接以工作区根为 cwd
    //（从 crates/cli 起找会停在 crate 自身，语料目录为空）
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("应能定位工作区根")
        .to_path_buf();
    rzc()
        .env("HOME", home.path())
        .current_dir(&workspace_root)
        .args(["mapping", "coverage", "--lang", "zh"])
        .assert()
        .success()
        .stdout(predicates::str::contains("语料"))
        .stdout(predicates::str::contains("覆盖通过"));
}
