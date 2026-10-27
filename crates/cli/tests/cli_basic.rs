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
///
/// 失败输出的行首前缀同样按界面语言生成（本文件未定 RZ_LANG 时解析为 zh →
/// 「错误：」），因此错误类断言匹配母语前缀而非旧的硬编码 "Error:"。
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

/// `rzc init` 缺省跟随系统 locale：de_DE 环境下生成 `src/main.de`（全球用户第一个项目是母语）
#[test]
fn test_init_default_lang_follows_locale() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    rzc()
        .env("HOME", home.path())
        .env("LC_ALL", "de_DE.UTF-8")
        .env("LANG", "de_DE.UTF-8")
        .current_dir(work.path())
        .args(["init", "demo"])
        .assert()
        .success()
        .stdout(predicates::str::contains("demo"));
    assert!(
        work.path().join("demo/src/main.de").is_file(),
        "系统语言为 de 时缺省应生成 src/main.de"
    );
}

/// 回归：`--lang` 指向没有语言包的代码（如 `it`）必须立即报错
///
/// 旧行为是“创建成功”但只生成 `src/main.it`，等到首次 `rzc run` 才报未知扩展名。
#[test]
fn test_init_rejects_unknown_lang() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    rzc()
        .env("HOME", home.path())
        .current_dir(work.path())
        .args(["init", "demo", "--lang", "it"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("'it'"))
        // 报错同时给出可用语言代码，用户不用去翻文档
        .stderr(predicates::str::contains("ja"));
    assert!(
        !work.path().join("demo").exists(),
        "拒绝时应不留下半成品项目目录"
    );
}

/// 系统 locale 没有对应语言包且非交互（管道/CI）：提示已回退中文，不再静默降级
#[test]
fn test_init_unknown_locale_reports_fallback() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    rzc()
        .env("HOME", home.path())
        .env("LC_ALL", "it_IT.UTF-8")
        .env_remove("LANG")
        .env_remove("LC_MESSAGES")
        .env_remove("RZ_LANG")
        .current_dir(work.path())
        .write_stdin("")
        .args(["init", "demo"])
        .assert()
        .success()
        .stdout(predicates::str::contains("未识别系统语言 'it'"));
    assert!(
        work.path().join("demo/src/main.zh").is_file(),
        "回退后应生成 src/main.zh"
    );
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
    assert!(
        work.path().join("demo/.gitignore").is_file(),
        "应生成 .gitignore（托管生成物不入库）"
    );
    let gitignore =
        std::fs::read_to_string(work.path().join("demo/.gitignore")).unwrap_or_default();
    assert!(
        gitignore.contains(".rz/gen/"),
        ".gitignore 应忽略托管生成目录，实际内容：{gitignore}"
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

/// `rzc run <非默认入口>`：多文件项目下 cargo 只会运行 src/main，
/// 非默认入口（examples/ 等）必须明确报错，而不是静默运行另一个程序
#[test]
fn test_run_non_entry_file_fails_with_hint() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    let root = work.path().join("demo");
    std::fs::create_dir_all(root.join("src")).expect("创建 src 失败");
    std::fs::create_dir_all(root.join("examples")).expect("创建 examples 失败");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"demo\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(root.join("src/main.zh"), GOOD_PROGRAM).unwrap();
    std::fs::write(root.join("examples/示例.zh"), GOOD_PROGRAM).unwrap();

    let output = rzc()
        .env("HOME", home.path())
        .env_remove("RZ_LOG")
        .current_dir(&root)
        .args(["run", "examples/示例.zh"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr).to_string();
    assert!(stderr.starts_with("错误："), "{stderr}");
    assert!(
        stderr.contains("不是项目默认入口"),
        "应提示非默认入口：{stderr}"
    );
    assert!(
        stderr.contains("cargo run --example"),
        "应给出 cargo 运行方式：{stderr}"
    );
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

    // 托管布局：非入口产物在 `.rz/gen/` 镜像目录下，嵌套路径注解仍指「父词干/子词干.rs」
    let domain_rs = std::fs::read_to_string(work.path().join(".rz/gen/src/领域.rs"))
        .expect(".rz/gen/src/领域.rs 应已转译");
    assert!(
        domain_rs.contains("#[path") && domain_rs.contains("领域/工具.rs"),
        "非入口文件应按层级注解嵌套子模块：{domain_rs}"
    );
    assert!(
        domain_rs.contains("pub mod 工具"),
        "嵌套子模块应保持 pub（防 E0603）：{domain_rs}"
    );
    assert!(
        work.path().join(".rz/gen/src/领域/工具.rs").is_file(),
        "最深层产物也应镜像源码布局落在托管目录"
    );
    assert!(
        !work.path().join("src/领域.rs").exists(),
        "旧布局产物应已从 src/ 清走（src/ 只留母语源文件与入口产物）"
    );
    let main_rs = std::fs::read_to_string(work.path().join("src/main.rs")).expect("main.rs 应存在");
    assert!(
        main_rs.contains("#[path") && main_rs.contains("../.rz/gen/src/领域.rs"),
        "入口应跨目录注解托管产物：{main_rs}"
    );
}

/// 托管目录自愈：源文件被删除/改名后，失去源的产物被清走并告知数量
///
/// 旧布局不敢做这种清理（src/ 混着手写 `.rs`）；托管后 `.rz/gen/` 只存机器
/// 产物，残留会让 rustc 按默认模块查找命中旧文件（被删掉的代码仍在参与
/// 编译），因此删除必须与转译同处一条链路。
#[test]
fn test_check_prunes_products_without_source() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(work.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        work.path().join("Cargo.toml"),
        "[package]\nname=\"prune\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        work.path().join("src/main.zh"),
        "模块 数学;\n\n函数 主函数() {\n    打印行!(\"{}\", 数学::加一(1));\n}\n",
    )
    .unwrap();
    std::fs::write(
        work.path().join("src/数学.zh"),
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
    let 产物 = work.path().join(".rz/gen/src/数学.rs");
    assert!(产物.is_file(), "首轮应在托管目录生成模块产物");

    // 删除源文件并去掉 `模块` 声明（项目仍可编译）
    std::fs::remove_file(work.path().join("src/数学.zh")).unwrap();
    std::fs::write(
        work.path().join("src/main.zh"),
        "函数 主函数() {\n    打印行!(\"ok\");\n}\n",
    )
    .unwrap();
    rzc()
        .env("HOME", home.path())
        .env_remove("RZ_LOG")
        .current_dir(work.path())
        .args(["check", "src/main.zh"])
        .assert()
        .success()
        .stdout(predicates::str::contains("已清理"));
    assert!(!产物.exists(), "失去源文件的产物应已被清走");
    assert!(
        !work.path().join(".rz/gen/src/数学.rs.bak").exists(),
        "托管目录不应留下备份文件"
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

/// `rzc eject` 覆盖已有同名 `.rs`：与 run/check 同口径先备份为 `.rs.bak`，
/// 避免渐进迁移中手写产物被静默覆盖
#[test]
fn test_eject_backs_up_existing_output() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let zh = dir.path().join("main.zh");
    std::fs::write(&zh, GOOD_PROGRAM).unwrap();
    let handwritten = "fn main() { println!(\"手写\"); }\n";
    std::fs::write(dir.path().join("main.rs"), handwritten).unwrap();

    rzc().args(["eject"]).arg(&zh).assert().success();

    let bak = dir.path().join("main.rs.bak");
    assert!(bak.is_file(), "旧产物应备份为 main.rs.bak");
    assert_eq!(
        std::fs::read_to_string(&bak).unwrap(),
        handwritten,
        "备份内容应为被覆盖前的手写产物"
    );
    let rs = std::fs::read_to_string(dir.path().join("main.rs")).unwrap();
    assert!(rs.contains("fn main()"), "新产物应已写入：{rs}");
}

/// `rzc eject <目录>`：递归导出全部方言文件（P-02 回归：目录直接交给
/// `fs::read_to_string` 在 Windows 报「拒绝访问 (os error 5)」）
///
/// 同时覆盖声明与引用一致性（P-01/P-05/P-07/P-08 同源）：模块名（版本/
/// 连接设置）与结构体字段（主机/状态）撞 lang-pack 别名时，`mod` 声明、
/// use 路径段与字段访问必须同名保留（有项目上下文时整体豁免）。
#[test]
fn test_eject_directory_transpiles_all_files_consistently() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let src = dir.path().join("src");
    std::fs::create_dir_all(src.join("领域")).unwrap();
    std::fs::write(
        src.join("main.zh"),
        "模块 版本;\n模块 连接设置;\n模块 领域;\n\
         使用 包::版本::程序版本;\n使用 包::连接设置::数据库连接配置;\n\
         使用 包::领域::工具::问候;\n\n\
         函数 主函数() {\n\
         让 当前配置 = 数据库连接配置 { 主机: 字符串::从(\"本地\"), 状态: 字符串::从(\"正常\") };\n\
         打印行!(\"{} {} {}\", 程序版本(), 当前配置.主机, 当前配置.状态);\n\
         问候();\n}",
    )
    .unwrap();
    std::fs::write(
        src.join("版本.zh"),
        "公开 函数 程序版本() -> 字符串 {\n    字符串::从(\"1.0.0\")\n}",
    )
    .unwrap();
    std::fs::write(
        src.join("连接设置.zh"),
        "公开 结构体 数据库连接配置 {\n    公开 主机: 字符串,\n    公开 状态: 字符串,\n}",
    )
    .unwrap();
    std::fs::write(src.join("领域.zh"), "公开 模块 工具;").unwrap();
    std::fs::write(
        src.join("领域/工具.zh"),
        "公开 函数 问候() {\n    打印行!(\"你好\");\n}",
    )
    .unwrap();

    rzc().args(["eject"]).arg(&src).assert().success();

    let main_rs = std::fs::read_to_string(src.join("main.rs")).expect("main.rs 应已导出");
    // 声明与引用同名（P-05/P-07）：不得出现 crate::version / crate::Permissions
    assert!(main_rs.contains("mod 版本;"), "{main_rs}");
    assert!(main_rs.contains("use crate::版本::程序版本;"), "{main_rs}");
    assert!(
        main_rs.contains("use crate::连接设置::数据库连接配置;"),
        "{main_rs}"
    );
    assert!(!main_rs.contains("crate::version"), "{main_rs}");
    // 字段定义与访问同名（P-01/P-08）
    assert!(main_rs.contains("当前配置.主机"), "{main_rs}");
    assert!(main_rs.contains("当前配置.状态"), "{main_rs}");
    assert!(!main_rs.contains(".host"), "{main_rs}");
    // crate 根的非 ASCII 模块注解
    assert!(main_rs.contains("#[path = \"版本.rs\"]"), "{main_rs}");

    let field_rs = std::fs::read_to_string(src.join("连接设置.rs")).expect("连接设置.rs 应已导出");
    assert!(field_rs.contains("pub 主机: String"), "{field_rs}");
    assert!(field_rs.contains("pub 状态: String"), "{field_rs}");

    let version_rs = std::fs::read_to_string(src.join("版本.rs")).expect("版本.rs 应已导出");
    assert!(version_rs.contains("pub fn 程序版本()"), "{version_rs}");

    // 多层目录：子模块注解相对当前目录带自身词干前缀（rustc 默认查找规则）
    let domain_rs = std::fs::read_to_string(src.join("领域.rs")).expect("领域.rs 应已导出");
    assert!(
        domain_rs.contains("#[path = \"领域/工具.rs\"]") && domain_rs.contains("pub mod 工具;"),
        "{domain_rs}"
    );
    assert!(src.join("领域/工具.rs").is_file(), "嵌套模块应已导出");
}

/// 混合语言目录（`.zh` + `.ja`）：按扩展名分组、每组用自身语言包转译，
/// 否则日文关键字（関数/主関数）会被中文包原样保留在产物里
#[test]
fn test_eject_mixed_language_directory_uses_each_pack() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    std::fs::write(dir.path().join("main.zh"), "函数 主函数() {\n}\n").unwrap();
    std::fs::write(dir.path().join("tool.ja"), "関数 主関数() {\n}\n").unwrap();

    rzc().args(["eject"]).arg(dir.path()).assert().success();

    let main_rs = std::fs::read_to_string(dir.path().join("main.rs")).expect("main.rs 应已导出");
    let tool_rs = std::fs::read_to_string(dir.path().join("tool.rs")).expect("tool.rs 应已导出");
    assert!(main_rs.contains("fn main()"), "{main_rs}");
    assert!(tool_rs.contains("fn main()"), "{tool_rs}");
    assert!(
        !tool_rs.contains("関数"),
        "ja 文件必须用 ja 语言包转译：{tool_rs}"
    );
}

/// `rzc eject <空目录>`：无方言源文件时干净报错，不 panic
#[test]
fn test_eject_empty_directory_fails_cleanly() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    rzc()
        .args(["eject"])
        .arg(dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::starts_with("错误："));
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

/// 输入文件不存在：以非零退出码与母语错误行失败，而非 panic 堆栈
#[test]
fn test_missing_source_file_fails_cleanly() {
    let home = isolated_home();
    let missing = std::env::temp_dir().join(format!("rzc-missing-{}.zh", std::process::id()));
    let output = rzc()
        .env("HOME", home.path())
        .arg("transpile")
        .arg(&missing)
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr).to_string();
    assert!(stderr.starts_with("错误："), "{stderr}");
    assert!(
        stderr.contains("不存在"),
        "应给出母语「不存在」提示：{stderr}"
    );
    assert!(
        !stderr.to_lowercase().contains("os error"),
        "不应暴露 OS 错误：{stderr}"
    );
}

/// `rzc eject <不存在的路径>`：给出「不存在」提示，而不是无关的
/// 「无法识别源文件扩展名」（与 `run`/`check`/`transpile` 的缺失校验同口径）
#[test]
fn test_eject_missing_path_fails_with_hint() {
    let home = isolated_home();
    let missing = std::env::temp_dir().join(format!("rzc-eject-missing-{}.zh", std::process::id()));
    let output = rzc()
        .env("HOME", home.path())
        .args(["eject"])
        .arg(&missing)
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr).to_string();
    assert!(stderr.starts_with("错误："), "{stderr}");
    assert!(stderr.contains("不存在"), "应提示路径不存在：{stderr}");
    assert!(
        !stderr.contains("无法识别源文件扩展名"),
        "不应报无关的扩展名错误：{stderr}"
    );
}

/// `rzc eject <a.zh> <b.zh>`：一次传多个目标批量导出（复用单目标逻辑）
///
/// 逐个手动 eject 容易遗漏，批量模式必须：成功项逐个提示、末尾输出汇总、
/// 退出码为 0。
#[test]
fn test_eject_batch_multiple_targets() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let main = dir.path().join("main.zh");
    let tool = dir.path().join("工具.zh");
    std::fs::write(&main, GOOD_PROGRAM).unwrap();
    std::fs::write(&tool, "公开 函数 问候() {\n}\n").unwrap();

    let output = rzc()
        .args(["eject"])
        .arg(&main)
        .arg(&tool)
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout).to_string();
    assert!(
        stdout.contains("main.rs"),
        "应提示 main.rs 已导出：{stdout}"
    );
    assert!(
        stdout.contains("工具.rs"),
        "应提示 工具.rs 已导出：{stdout}"
    );
    assert_eq!(
        stdout.matches("已导出到").count(),
        2,
        "每个目标都应单独提示：{stdout}"
    );
    assert!(stdout.contains("共导出 2 个目标"), "应有成功汇总：{stdout}");
    assert!(dir.path().join("main.rs").is_file(), "main.rs 应已导出");
    assert!(dir.path().join("工具.rs").is_file(), "工具.rs 应已导出");
}

/// `rzc eject` 批量部分失败：成功项照常导出，失败项单独汇总，退出码非零
///
/// 不能因一个目标失败就中断其余目标（那反而更容易漏导出），
/// 也不能以 0 退出（否则 CI 会放过遗漏）。
#[test]
fn test_eject_batch_partial_failure_is_nonzero() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let present = dir.path().join("main.zh");
    std::fs::write(&present, GOOD_PROGRAM).unwrap();
    let missing = dir.path().join("缺失.zh");

    let output = rzc()
        .args(["eject"])
        .arg(&present)
        .arg(&missing)
        .assert()
        .failure();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr).to_string();
    assert!(stdout.contains("main.rs"), "成功项应照常导出：{stdout}");
    assert!(stderr.contains("导出失败"), "应列出失败项：{stderr}");
    assert!(stderr.contains("缺失.zh"), "失败项应带目标路径：{stderr}");
    assert!(
        stderr.contains("成功 1 个，失败 1 个"),
        "应有成功/失败汇总：{stderr}"
    );
    assert!(
        dir.path().join("main.rs").is_file(),
        "部分失败不应影响成功项的产物"
    );
}

/// `rzc eject` 重复目标：参数校验直接报错（避免同一文件被写两次）
#[test]
fn test_eject_duplicate_target_rejected() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let main = dir.path().join("main.zh");
    std::fs::write(&main, GOOD_PROGRAM).unwrap();

    let output = rzc()
        .args(["eject"])
        .arg(&main)
        .arg(&main)
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr).to_string();
    assert!(
        stderr.contains("重复"),
        "应在参数校验阶段报重复目标：{stderr}"
    );
}

/// `rzc eject` 导出前冲突扫描：字段名/声明名命中词条、会 panic 的词条被当文本用
///
/// 三类静默改语义的位置必须在导出前报出来（按 `.zh` 行列），且**只提示不阻断**：
/// 产物照常写出、退出码为 0；`--no-lint` 一并关闭（与教学 lint 同口径）。
#[test]
fn test_eject_reports_conflict_hints() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let zh = dir.path().join("main.zh");
    // 原始字符串：源码本身含 「"」 与换行，避免层层转义
    std::fs::write(
        &zh,
        r#"结构体 甲 {
    类型: 字符串,
}

函数 主函数() {
    让 自身 = 1;
    打印行!("{}", 解包(有值(3)));
}
"#,
    )
    .unwrap();

    let output = rzc().args(["eject"]).arg(&zh).assert().success();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr).to_string();
    assert!(stderr.contains("[eject_hint]"), "应有冲突提示：{stderr}");
    assert!(
        stderr.contains("第 2 行第 5 列") && stderr.contains("类型"),
        "字段名命中词条应报行列：{stderr}"
    );
    assert!(
        stderr.contains("第 6 行第 7 列") && stderr.contains("自身"),
        "声明名命中词条应报行列：{stderr}"
    );
    assert!(
        stderr.contains("解包") && stderr.contains("panic"),
        "panic 被当文本用应报出来：{stderr}"
    );
    assert!(
        stderr.matches("[eject_hint]").count() == 3,
        "三类位置各报一次：{stderr}"
    );
    assert!(dir.path().join("main.rs").is_file(), "提示不得阻断导出");

    // --no-lint：教学提示一并关闭，导出行为不变
    let output = rzc()
        .args(["--no-lint", "eject"])
        .arg(&zh)
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr).to_string();
    assert!(
        !stderr.contains("[eject_hint]"),
        "--no-lint 应关闭冲突提示：{stderr}"
    );
}

/// `run`/`check`/`transpile` 传目录：给出可操作提示（P-02 同类面统一收口），
/// 不得暴露 OS 原始错误（Windows「拒绝访问 (os error 5)」）
#[test]
fn test_directory_input_fails_with_actionable_hint() {
    let home = isolated_home();
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    for sub in ["run", "check", "transpile"] {
        let output = rzc()
            .env("HOME", home.path())
            .arg(sub)
            .arg(dir.path())
            .assert()
            .failure();
        let stderr = String::from_utf8_lossy(&output.get_output().stderr).to_string();
        assert!(
            stderr.starts_with("错误："),
            "{sub} 应以 错误：开头：{stderr}"
        );
        assert!(
            stderr.contains("rzc eject"),
            "{sub} 应把目录导出指向 eject：{stderr}"
        );
        assert!(
            !stderr.to_lowercase().contains("os error"),
            "{sub} 不应暴露 OS 错误：{stderr}"
        );
    }
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

/// `rzc mapping check <未知目标>`：非目录非内置代码 → 非零退出 + 母语错误行
#[test]
fn test_mapping_check_unknown_target_fails() {
    let home = isolated_home();
    rzc()
        .env("HOME", home.path())
        .args(["mapping", "check", "no-such-lang-xyz"])
        .assert()
        .failure()
        .stderr(predicates::str::starts_with("错误："));
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
        .stderr(predicates::str::starts_with("错误："));

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
        .stderr(predicates::str::starts_with("错误："));
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

/// `relocalize` 端到端：zh 工程 → 反译重生为 ja 方言工程（可再转译回 fn main）
///
/// 验证镜像写出、清单复制、AGENTS.md 生成，以及入口 `main` 取核心关键字（`主関数`）
/// 而非第三方 crate 别名——保证产物 `rzc transpile` 回标准 Rust 时得到 `fn main`。
#[test]
fn test_relocalize_zh_project_to_ja_dialect() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).expect("创建 src 失败");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"rlz\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n\n[workspace]\n",
    )
    .expect("写入 Cargo.toml 失败");
    std::fs::write(
        src_dir.join("main.zh"),
        "函数 主函数() {\n    打印行!(\"你好\");\n}\n",
    )
    .expect("写入 main.zh 失败");

    rzc()
        .args(["relocalize", "--to", "ja"])
        .arg(dir.path())
        .assert()
        .success();

    let out_main = dir.path().join("relocalized.ja/src/main.ja");
    assert!(out_main.is_file(), "应镜像写出 {}", out_main.display());
    let ja = std::fs::read_to_string(&out_main).expect("读取 main.ja 失败");
    assert!(ja.contains("関数"), "产物应含 ja `関数`: {ja}");
    assert!(ja.contains("主関数"), "入口应为 `主関数`: {ja}");
    assert!(
        dir.path().join("relocalized.ja/Cargo.toml").is_file(),
        "应复制 Cargo.toml"
    );
    assert!(
        dir.path().join("relocalized.ja/AGENTS.md").is_file(),
        "应生成 AGENTS.md"
    );

    // 反译产物再正向转译应回到 `fn main`（可编译性探针）
    rzc()
        .arg("transpile")
        .arg(&out_main)
        .assert()
        .success()
        .stdout(predicates::str::contains("fn main"));
}

/// `relocalize` 缺 `--to` 时报错退出（必填参数）
#[test]
fn test_relocalize_requires_to() {
    rzc()
        .args(["relocalize", "/tmp"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("--to"));
}
