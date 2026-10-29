//! rzc 子命令冒烟测试：进程级最小行为验证
//!
//! 只覆盖不依赖网络与外部工具链的面：`--version`、`lang list`（内置
//! 语言包编译期嵌入）、`transpile`（纯内存转译，不调 rustc/cargo）。
//! 编译诊断链路的端到端测试由 LSP e2e（crates/lsp/tests/e2e.rs）与
//! 教程验证（tools/verify-tutorials.py）覆盖。

use assert_cmd::Command;
use std::path::{Path, PathBuf};
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

/// 指向**私有** HOME 与私有语言包目录的命令构造器
///
/// `rzc()` 把 `RZ_LANG_DIR` 预设为进程内**共享**的沙箱子目录，而
/// `global_lang_dir()` 优先读该变量、其次才看 `HOME`；因此只覆盖 `HOME` 的测试
/// 仍会写进共享目录，与 `lang list` 的内置包计数断言相撞（并行时偶发 12 ≠ 11）。
/// 凡读写全局语言包目录的测试都用本函数，两个位置一起私有化。
fn rzc_in_private_home(home: &Path) -> Command {
    let mut cmd = rzc();
    cmd.env("HOME", home)
        .env("RZ_LANG_DIR", home.join("lang-packs"));
    cmd
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

/// `lang list` 无需网络：内置 11 语言包编译期嵌入；私有 HOME 与私有语言包目录
/// 双保险，计数既不随开发者机器上的 `~/.rz/lang-packs` 变化，也不随同批次
/// 安装/卸载语言包的测试变化
#[test]
fn test_lang_list_lists_builtin_packs() {
    let home = isolated_home();
    rzc_in_private_home(home.path())
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

/// 中文方言库入口样例（`src/lib.zh` 用）：只有公开函数，没有主函数
const GOOD_LIB: &str = "公开 函数 答案() -> 整数 {\n    6 * 7\n}\n";

/// 兄弟库入口样例（`src/lib.zh` 用，bin+lib 配方）：库里的函数供主程序 `use 包名::`
const LIB_DOUBLE: &str = "公开 函数 翻倍(x: 整数) -> 整数 {\n    x * 2\n}\n";

/// 引用**本项目自己的库 crate** 的主程序（`rzc check src/lib.<ext>` 之后才可解析）
const MAIN_USES_OWN_LIB: &str =
    "使用 calc::翻倍;\n\n函数 主函数() {\n    打印行!(\"{}\", 翻倍(21));\n}\n";

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

/// 项目里还没有 Cargo 能读取的构建 target 时，`rzc add` 必须补母语指引
///
/// `rzc init` 只生成方言入口（`src/main.en`），Cargo 认不到 target，于是新建项目
/// 后**第一条 `rzc add` 必然失败**，原文是构建器内部术语
/// （`no targets specified in the manifest`）且不含任何可执行动作。cargo 先解析
/// manifest、后访问 registry，故本用例离线即复现失败路径（实测无
/// "Updating crates.io index"）。断言同时覆盖 zh/en 两种界面语言，并确认 cargo
/// 原文未被吞掉。
#[test]
fn test_add_without_cargo_target_prints_native_hint() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(work.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        work.path().join("Cargo.toml"),
        "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(work.path().join("src/main.en"), "fn main() {}\n").unwrap();

    let run = |lang: &str| {
        let out = rzc()
            .env("HOME", home.path())
            .env("RZ_LANG", lang)
            .env_remove("RZ_LOG")
            .current_dir(work.path())
            .args(["add", "any-crate"])
            .assert()
            .failure()
            .get_output()
            .stderr
            .clone();
        String::from_utf8_lossy(&out).to_string()
    };

    let zh = run("zh");
    assert!(
        zh.contains("还没有 Cargo 能读取的构建入口"),
        "应有母语指引：{zh}"
    );
    assert!(
        zh.contains("rzc check src/main.en"),
        "指引须含可执行动作：{zh}"
    );
    assert!(zh.contains("no targets"), "cargo 原文须原样可见：{zh}");

    let en = run("en");
    assert!(
        en.contains("no build target Cargo can read"),
        "应有英文指引：{en}"
    );
    assert!(en.contains("Run `rzc check src/main.en` first"), "{en}");
    assert!(en.contains("no targets"), "cargo 原文须原样可见：{en}");
}

/// 提示里的入口名取 `main.<ext>`，不受 `read_dir` 顺序影响
///
/// 项目里除入口外还有别的方言文件时，随便挑一个会让读者去 check 一个**模块文件**
/// ——那不是入口，生成不了 `src/main.rs`，墙照旧撞第二次。兄弟文件特意取 `api.zh`：
/// 它字典序在 `main.zh` 之前，所以「排序后取第一个」的实现会在这里露馅，
/// 只有显式优先 `main` 才通过。
#[test]
fn test_add_hint_prefers_main_entry() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(work.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        work.path().join("Cargo.toml"),
        "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(work.path().join("src/main.zh"), GOOD_PROGRAM).unwrap();
    std::fs::write(work.path().join("src/api.zh"), "fn helper() {}\n").unwrap();
    std::fs::write(work.path().join("src/工具.zh"), "fn helper2() {}\n").unwrap();

    for _ in 0..3 {
        let out = rzc()
            .env("HOME", home.path())
            .env("RZ_LANG", "zh")
            .env_remove("RZ_LOG")
            .current_dir(work.path())
            .args(["add", "any-crate"])
            .assert()
            .failure()
            .get_output()
            .stderr
            .clone();
        let stderr = String::from_utf8_lossy(&out).to_string();
        assert!(
            stderr.contains("rzc check src/main.zh"),
            "应点名入口文件：{stderr}"
        );
        assert!(
            !stderr.contains("rzc check src/api.zh"),
            "不该把排序靠前的模块文件当入口：{stderr}"
        );
    }
}

/// manifest 自身有问题时**不得**套用「无 target」指引
///
/// 命中判定要求两个条件同时成立（项目确实无 target + cargo 确实报在无 target
/// 上）。少了后半个条件，包名为空这类真 manifest 错误也会被指向 `rzc check`，
/// 把读者引向无关动作；本用例锁定不误报。
#[test]
fn test_add_manifest_error_does_not_claim_missing_target() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(work.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        work.path().join("Cargo.toml"),
        "[package]\nname=\"\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(work.path().join("src/main.en"), "fn main() {}\n").unwrap();

    let out = rzc()
        .env("HOME", home.path())
        .env("RZ_LANG", "zh")
        .env_remove("RZ_LOG")
        .current_dir(work.path())
        .args(["add", "any-crate"])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&out).to_string();
    assert!(
        !stderr.contains("还没有 Cargo 能读取的构建入口"),
        "真 manifest 错误不该给无 target 指引：{stderr}"
    );
    assert!(
        stderr.contains("package name cannot be empty"),
        "cargo 原文可见：{stderr}"
    );
}

/// 方言库入口 `src/lib.<ext>` 落 Cargo 自动发现的库 crate 根，造库零配置
///
/// 此前 `lib` 不在入口名集合里：纯库目录里 `rzc check src/lib.zh` 被清单预检拦下
/// （「Cargo.toml 没有声明编译目标」），唯一的可用配方是在 Cargo.toml 手写
/// `[lib] path = ".rz/gen/src/lib.rs"`——把「造一个方言库」变成一段要背的配置。
/// 现在产物直写 `src/lib.rs`，Cargo 认得它，`rzc check` 与 `cargo build` 都过。
#[test]
fn test_check_lib_entry_writes_src_lib_rs() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(work.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        work.path().join("Cargo.toml"),
        "[package]\nname=\"greetlib\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(work.path().join("src/lib.zh"), GOOD_LIB).unwrap();
    // 升级前 lib 不是入口，产物写在托管目录；那份残留现在没人引用了
    std::fs::create_dir_all(work.path().join(".rz/gen/src")).unwrap();
    std::fs::write(work.path().join(".rz/gen/src/lib.rs"), "陈旧产物").unwrap();

    rzc()
        .env("HOME", home.path())
        .env_remove("RZ_LOG")
        .current_dir(work.path())
        .args(["check", "src/lib.zh"])
        .assert()
        .success();

    let lib_rs = work.path().join("src/lib.rs");
    let product = std::fs::read_to_string(&lib_rs).expect("应生成 src/lib.rs（库 crate 根）");
    // 母语函数名不翻译（只有关键字与类型有映射），断言产物形态即可
    assert!(
        product.contains("pub fn") && product.contains("-> i32"),
        "{product}"
    );
    // 托管目录不该再留一份同源的旧产物：留着会被 rustc 默认模块查找命中
    assert!(
        !work.path().join(".rz/gen/src/lib.rs").exists(),
        "入口产物落 crate 根后托管副本应清掉"
    );
}

/// `rzc run` 库入口给母语指引，而不是让人去找一个不存在的 bin
///
/// 库 crate 没有可执行程序，cargo 此时报 `a bin target must be available for
/// cargo run`。`run_not_default_entry` 那条（指向 `cargo run --bin <名称>`）在这
/// 里是误导：项目里根本没有 bin。故库入口单列一条消息，给出的动作只有「校验」
/// 与「补主程序」。界面语言跟随文件扩展名（`.zh` → zh，`.en` → en）。
#[test]
fn test_run_lib_entry_explains_no_executable() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(work.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        work.path().join("Cargo.toml"),
        "[package]\nname=\"greetlib\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(work.path().join("src/lib.zh"), GOOD_LIB).unwrap();
    std::fs::write(
        work.path().join("src/lib.en"),
        "pub fn answer() -> i32 { 6 * 7 }\n",
    )
    .unwrap();

    let run = |file: &str| {
        let out = rzc()
            .env("HOME", home.path())
            .env_remove("RZ_LOG")
            .current_dir(work.path())
            .args(["run", file])
            .assert()
            .failure()
            .get_output()
            .stderr
            .clone();
        String::from_utf8_lossy(&out).to_string()
    };

    let zh = run("src/lib.zh");
    assert!(zh.contains("库里没有可执行的主程序"), "应有母语说明：{zh}");
    assert!(
        zh.contains("rzc check src/lib.zh"),
        "须给出可执行动作：{zh}"
    );
    assert!(
        !zh.contains("不是项目默认入口"),
        "不该套用非默认入口那条消息：{zh}"
    );

    let en = run("src/lib.en");
    assert!(
        en.contains("library has no executable main"),
        "应有英文说明：{en}"
    );
    assert!(
        en.contains("rzc check src/lib.en"),
        "须给出可执行动作：{en}"
    );
}

/// 兄弟库文件不得被顺带写成 crate 根：`rzc check src/main.zh` 不造 lib 目标
///
/// 项目里同时有 `src/main.zh` 与 `src/lib.zh` 时，若项目级转译把两者都按入口规则
/// 落 crate 根，就等于替用户凭空加出一个编译目标——实测非 ASCII 包名立刻撞上
/// rustc 硬错 `crate name ... passed to --extern is not a valid ASCII identifier`
/// （bin 链接 lib 必须 --extern），原先不参与编译的 lib 还会把陈年错误一起报出来。
/// 入口产物只由**本次请求的文件**写，这条边界靠本用例钉住。
#[test]
fn test_check_main_does_not_materialize_sibling_lib() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(work.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        work.path().join("Cargo.toml"),
        "[package]\nname=\"双入口\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(work.path().join("src/main.zh"), GOOD_PROGRAM).unwrap();
    std::fs::write(work.path().join("src/lib.zh"), GOOD_LIB).unwrap();

    rzc()
        .env("HOME", home.path())
        .env_remove("RZ_LOG")
        .current_dir(work.path())
        .args(["check", "src/main.zh"])
        .assert()
        .success();

    assert!(work.path().join("src/main.rs").exists());
    assert!(
        !work.path().join("src/lib.rs").exists(),
        "未请求的兄弟库文件不该被写成库 crate 根"
    );
}

/// 老配方项目里，未请求的兄弟库文件仍要被项目级转译刷新
///
/// `[lib] path` 指着托管目录的项目（纳管 lib 之前的唯一造库配方）靠的是
/// `rzc check src/main.zh` 顺带把 `src/lib.zh` 转译到那份声明路径。若因为
/// 「lib 现在也是入口」就把它加进兄弟跳过名单，cargo 会编译一份不再更新的产物
/// ——代码改了却不报错，比报错更糟。故 lib 只在**被请求**时落 crate 根。
#[test]
fn test_sibling_lib_refreshes_declared_gen_product() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(work.path().join(".rz/gen/src")).unwrap();
    std::fs::create_dir_all(work.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        work.path().join("Cargo.toml"),
        "[package]\nname=\"mixlib\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[lib]\npath=\".rz/gen/src/lib.rs\"\n",
    )
    .unwrap();
    std::fs::write(work.path().join("src/main.zh"), GOOD_PROGRAM).unwrap();
    std::fs::write(work.path().join("src/lib.zh"), GOOD_LIB).unwrap();
    let gen_lib = work.path().join(".rz/gen/src/lib.rs");
    std::fs::write(&gen_lib, "陈旧产物").unwrap();

    rzc()
        .env("HOME", home.path())
        .env_remove("RZ_LOG")
        .current_dir(work.path())
        .args(["check", "src/main.zh"])
        .assert()
        .success();

    let refreshed = std::fs::read_to_string(&gen_lib).expect("声明的库产物应被刷新");
    assert!(
        refreshed.contains("pub fn"),
        "陈旧产物应被覆盖：{refreshed}"
    );
    // 请求的是主程序，库产物不得被当成入口挪走
    assert!(!work.path().join("src/lib.rs").exists());
}

/// bin+lib 项目里，check 主程序**不得**清掉兄弟库已生成的 crate 根产物
///
/// 库配方是「先 `rzc check src/lib.zh` 生成 src/lib.rs，再 check 主程序」。项目级
/// 转译把兄弟 `src/lib.zh` 也走一遍时，会按旧布局规则清理「源文件旁边的同名 .rs」——
/// 而 `src/lib.rs` 恰恰是 Cargo 的库目标产物，不是旧布局遗留。清掉之后第二次 check
/// 立刻 E0432，等于这条配方自己把自己拆了（本轮实机复现，先前四个用例都没覆盖到
/// 「两次 check 的先后顺序」这一维度）。反向验证：恢复无条件清理，第二步失败。
#[test]
fn test_check_main_keeps_sibling_lib_crate_product() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    let root = work.path();
    std::fs::create_dir_all(root.join("src")).expect("创建 src 失败");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"calc\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(root.join("src/lib.zh"), LIB_DOUBLE).unwrap();
    std::fs::write(root.join("src/main.zh"), MAIN_USES_OWN_LIB).unwrap();

    let check = |arg: &str| {
        rzc()
            .env("HOME", home.path())
            .env_remove("RZ_LOG")
            .current_dir(root)
            .args(["check", arg])
            .assert()
            .success();
    };

    check("src/lib.zh");
    assert!(root.join("src/lib.rs").is_file(), "库入口应写出 src/lib.rs");
    check("src/main.zh");
    assert!(
        root.join("src/lib.rs").is_file(),
        "check 主程序不得清掉兄弟库的 crate 根产物"
    );
}

/// 主程序引用**自己的库 crate** 时不得建议 `rzc add 包名`
///
/// rustc 分不出「自己的库还没生成产物」和「没声明的第三方库」，两种都报 E0432 并
/// 建议 `cargo add`；而 Cargo 明确拒绝包依赖自己（实测 `error: cannot add `calc`
/// as a dependency to itself`）。此前 rzc 把这条建议原样译成母语、又追加一条同款
/// 提示，读者照着做一定白跑一趟。现在两处都换成能走通的动作：先 check 库入口。
#[test]
fn test_own_crate_hint_points_at_lib_entry_not_rzc_add() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    let root = work.path();
    std::fs::create_dir_all(root.join("src")).expect("创建 src 失败");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"calc\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(root.join("src/lib.zh"), LIB_DOUBLE).unwrap();
    std::fs::write(root.join("src/main.zh"), MAIN_USES_OWN_LIB).unwrap();

    let out = rzc()
        .env("HOME", home.path())
        .env_remove("RZ_LOG")
        .current_dir(root)
        .args(["check", "src/main.zh"])
        .assert()
        .failure()
        .get_output()
        .clone();
    // 诊断正文走 stdout、加依赖提示走 stderr，断言要覆盖两条流
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        text.contains("是本项目的包名"),
        "应点明是自己的包名：{text}"
    );
    assert!(
        text.contains("rzc check src/lib.zh"),
        "须给出能走通的动作：{text}"
    );
    assert!(
        !text.contains("疑似未声明的第三方库"),
        "不该把自有 crate 说成第三方库：{text}"
    );
    assert!(
        !text.contains("若要使用名为 calc 的第三方库"),
        "rustc 的 `cargo add` 建议应被剔除：{text}"
    );
}

/// 真正没声明的第三方库照旧给 `rzc add` 提示（别把正确提示一起吞了）
///
/// 本地包名判定只在名字命中 `[package] name` / `[lib] name` 时接管提示；这里包名
/// 故意取 `notserde`，导入的是注册表上的真库名，旧行为必须原样保留。
#[test]
fn test_third_party_hint_still_fires_for_unknown_crate() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    let root = work.path();
    std::fs::create_dir_all(root.join("src")).expect("创建 src 失败");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"notserde\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.zh"),
        "使用 serde::序列化;\n\n函数 主函数() {\n    打印行!(\"ok\");\n}\n",
    )
    .unwrap();

    let out = rzc()
        .env("HOME", home.path())
        .env_remove("RZ_LOG")
        .current_dir(root)
        .args(["check", "src/main.zh"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("rzc add serde"),
        "第三方库仍应给 rzc add 提示：{text}"
    );
    assert!(!text.contains("是本项目的包名"), "{text}");
}

/// bin+lib 项目的非 ASCII 包名硬错给母语译文（不再喷英文、不再刷待译清单）
///
/// 实机测清的触发条件：**两种目标共存 + 非 ASCII 包名**——主程序完全不引用库也撞
/// （Cargo 要用 `--extern 计算器=…` 把库交给主程序），而只有库或只有主程序都能编。
/// 这条无错误码的 rustc 消息此前 11 语言全都没有条目，终端只剩英文原句 +
/// 「暂无译文，已记入待译清单」。本用例钉住译文到位（诊断走 stdout）。
#[test]
fn test_non_ascii_package_name_bin_lib_prints_localized_extern_error() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    let root = work.path();
    std::fs::create_dir_all(root.join("src")).expect("创建 src 失败");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"计算器\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(root.join("src/lib.zh"), GOOD_LIB).unwrap();
    std::fs::write(root.join("src/main.zh"), GOOD_PROGRAM).unwrap();

    let run_check = |arg: &str| {
        rzc()
            .env("HOME", home.path())
            .env_remove("RZ_LOG")
            .current_dir(root)
            .args(["check", arg])
            .output()
            .expect("运行 rzc 失败")
    };

    // 只有库时中文包名能编（先确认这一步不是已经挂了）
    let lib_only = run_check("src/lib.zh");
    assert!(
        lib_only.status.success(),
        "纯库项目应能编译：{}",
        String::from_utf8_lossy(&lib_only.stderr)
    );

    let main_out = run_check("src/main.zh");
    assert!(!main_out.status.success(), "bin+lib + 非 ASCII 包名应失败");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&main_out.stdout),
        String::from_utf8_lossy(&main_out.stderr)
    );
    assert!(
        text.contains("不是合法的 ASCII 标识符") && text.contains("计算器"),
        "应给母语译文并保留包名：{text}"
    );
    assert!(
        !text.contains("is not a valid ASCII identifier"),
        "不该再看到英文原句：{text}"
    );
    assert!(!text.contains("暂无译文"), "该消息已不再待译：{text}");
    // zh 模板写的是“Cargo.toml 里的包名”（不带反引号）：加了反引号会被类型
    // 映射改写成 `线程名`（stdlib 表里 线程名 = name），变成误导指引
    assert!(!text.contains("线程名"), "译文被类型映射误改写：{text}");
}

/// 恒等语言（en）不得谎报「有消息未译」，而母语侧的待译反馈必须原样保留
///
/// 起因：en 包不携带「消息翻译」节，所以每条 cargo/rustc 消息在 en 下都「未命中」。
/// 旧实现据此在英语输出末尾加一行 `Note: N message(s) have no translation, shown
/// as-is`（它在 en 下恒为真），并往 `~/.rz/gaps/en.tsv` 堆英文原句（实测积到
/// 1000+ 行且永远补不完——英语本就不需要译文）。
///
/// 同一个「未使用变量」程序跑两种语言作对照：zh 侧仍提示并落盘（那是真漏译的
/// 唯一反馈渠道，不能删），en 侧诊断照旧显示但不提示、不落盘。
#[test]
fn test_english_identity_lang_does_not_report_untranslated_gaps() {
    let run = |home: &Path, ext: &str, src: &str| {
        let work = tempfile::tempdir().expect("创建工作目录失败");
        let root = work.path();
        std::fs::create_dir_all(root.join("src")).expect("创建 src 失败");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"gapcheck\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        std::fs::write(root.join(format!("src/main.{ext}")), src).unwrap();
        let out = rzc()
            .env("HOME", home)
            .env_remove("RZ_LOG")
            .current_dir(root)
            .args(["check", &format!("src/main.{ext}")])
            .output()
            .expect("运行 rzc 失败");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let gap_file = home.join(".rz").join("gaps").join(format!("{ext}.tsv"));
        (text, gap_file)
    };

    // 两个 HOME 由测试作用域持有（写进闭包会随闭包返回被删，清单永远验不到）
    let zh_home = isolated_home();
    let en_home = isolated_home();

    // zh：该条诊断的残段与汇总句仍不在消息表里，因此必须提示 + 落盘——
    // 它是维护者发现漏译的唯一途径
    let (zh_text, zh_gaps) = run(
        zh_home.path(),
        "zh",
        "函数 主函数() {\n    让 未用变量 = 1;\n    打印行!(\"ok\");\n}\n",
    );
    assert!(
        zh_text.contains("未使用的变量"),
        "zh 侧诊断不应被吞：{zh_text}"
    );
    assert!(
        zh_text.contains("暂无译文"),
        "zh 命中不了消息表时应提示并邀请 `rzc gaps`：{zh_text}"
    );
    assert!(zh_gaps.exists(), "zh 侧漏译应落盘到 {}", zh_gaps.display());

    // en：同一句话在恒等语言下不是损失，不得提示也不得落盘
    let (en_text, en_gaps) = run(
        en_home.path(),
        "en",
        "fn main() {\n    let unused_var = 1;\n    println!(\"ok\");\n}\n",
    );
    assert!(
        en_text.contains("unused variable"),
        "en 侧诊断必须照旧可见（绝不吞诊断）：{en_text}"
    );
    assert!(
        !en_text.contains("have no translation") && !en_text.contains("暂无译文"),
        "en 是恒等语言，原句已是最终形态，不该被告知「有消息未译」：{en_text}"
    );
    assert!(
        !en_gaps.exists(),
        "en 侧不应落盘待译清单（它永远补不完）：{}",
        en_gaps.display()
    );
}

/// 待译清单：一行一条记录（跨行消息以转义字面量存放，可无损读回）
///
/// rustc JSON 的一类消息本身带换行（如 E0277 的「下列类型实现了该特征」清单）。
/// 旧实现把它原样落盘：一条记录炸成十几行裸续行，`rzc gaps` 报的条数不是
/// 待译句子数，且续行会被按当前界面语言误归类（历史文件里占了一半行数）。
#[test]
fn test_gaps_listing_keeps_one_record_per_line() {
    let home = isolated_home();
    let gaps_dir = home.path().join(".rz").join("gaps");
    std::fs::create_dir_all(&gaps_dir).expect("创建 gaps 目录失败");
    let file = gaps_dir.join("zt.tsv");
    // 新格式：多行消息以 `\n` 字面量存放，一条一行
    std::fs::write(
        &file,
        "zt\thelp\tE0277\tfirst line\\n  second line\\nand 56 others\nzt\tmain\t\tplain message\n",
    )
    .expect("写入清单失败");

    let out = rzc()
        .env("HOME", home.path())
        .args(["gaps", "--lang", "zt"])
        .output()
        .expect("运行 rzc gaps 失败");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "两条待译应输出两行，不得被消息内容拆行：{lines:?}"
    );
    assert!(
        lines
            .iter()
            .all(|l| l.contains("first line\\n") || l.contains("plain message")),
        "导出件应保留转义形式供工具消费：{lines:?}"
    );

    // --export 产物可被引擎读回，且消息内容逐字不变
    let export = home.path().join("exported.tsv");
    rzc()
        .env("HOME", home.path())
        .args(["gaps", "--lang", "zt", "--export"])
        .arg(&export)
        .assert()
        .success();
    let read = i18n_rust_engine::diagnostic::parse_gap_lines(
        &std::fs::read_to_string(&export).expect("读取导出件失败"),
    );
    assert_eq!(read.len(), 2, "导出件应被读回两条记录");
    assert!(read.iter().any(|g| g.3 == "plain message"));
    assert!(
        read.iter()
            .any(|g| g.3 == "first line\n  second line\nand 56 others"),
        "跨行消息导出后必须能逐字还原：{:?}",
        read
    );
}

/// 待译清单：历史未转义文件（裸续行）读回不拆条，实跑一轮后完成格式迁移
///
/// 旧行为会把这些裸行当成新记录（按当前界面语言归类），且因为读不回原记录
/// 而每轮重贴同一句（本机实测 `and 56 others` 在 en/ja/ru 各积 64/42/42 次）。
#[test]
fn test_gaps_migrates_legacy_multiline_file_on_next_run() {
    let home = isolated_home();
    let gaps_dir = home.path().join(".rz").join("gaps");
    std::fs::create_dir_all(&gaps_dir).expect("创建 gaps 目录失败");
    let file = gaps_dir.join("zh.tsv");
    // 旧格式：一条主记录 + 一条被拆成三行的跨行记录（裸续行无列结构）
    // 续行只能拼到它前面的记录上——真实历史文件里裸行总是跟在主记录后
    std::fs::write(
        &file,
        "zh\tmain\tE9999\tanchor record\nzh\tmain\t\tlegacy multiline start\n\tlegacy tail with tab\nand its last line\n",
    )
    .expect("写入历史清单失败");

    let work = tempfile::tempdir().expect("创建工作目录失败");
    let root = work.path();
    std::fs::create_dir_all(root.join("src")).expect("创建 src 失败");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"gapmig\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    // 两个不同变量名 → 两次都是缓存未命中，真实产生新的待译记录
    let sources = [
        "函数 主函数() {\n    让 未用变量甲 = 1;\n}\n",
        "函数 主函数() {\n    让 未用变量乙 = 2;\n}\n",
    ];
    for src in sources {
        std::fs::write(root.join("src/main.zh"), src).unwrap();
        rzc()
            .env("HOME", home.path())
            .current_dir(root)
            .args(["check", "src/main.zh"])
            .output()
            .expect("运行 rzc check 失败");
    }

    let raw = std::fs::read_to_string(&file).expect("清单应仍存在");
    let records = i18n_rust_engine::diagnostic::parse_gap_lines(&raw);
    assert!(
        records.iter().any(|g| g.0 == "zh"),
        "实跑产生的记录应落在 zh 清单里：{:?}",
        records
    );
    assert_eq!(
        raw.lines().count(),
        records.len(),
        "迁移后应一行一条记录（裸续行拼回上一条，不再虚增条数）"
    );
    assert!(
        !raw.contains("legacy multiline start\n\tlegacy tail"),
        "历史裸行应被拼回单行转义格式：{raw}"
    );
    assert!(
        records
            .iter()
            .any(|g| g.3 == "legacy multiline start\n\tlegacy tail with tab\nand its last line"),
        "拼回的历史记录内容不得丢失：{:?}",
        records
    );
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
        .stdout(predicates::str::contains("语言包"))
        .stdout(predicates::str::contains("项目:"));
}

/// `rzc doctor` 在项目里跑：入口产物还没生成时点名「Cargo 读不到任何构建目标」，
/// 并给出能走通的那条 `rzc check`——把直接 `cargo build` 撞
/// `no targets specified in the manifest` 这条墙从「读得懂 rustc 原文」降级为
/// 「提前看得见」（判定与 rzc run/check 的阻断同源）
#[test]
fn test_doctor_reports_missing_cargo_entry_in_project() {
    let home = isolated_home();
    let work = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(work.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        work.path().join("Cargo.toml"),
        "[package]\nname=\"doccheck\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(work.path().join("src/main.zh"), GOOD_PROGRAM).unwrap();

    let out = rzc()
        .env("HOME", home.path())
        .env_remove("RZ_LOG")
        .current_dir(work.path())
        .arg("doctor")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&out).to_string();
    assert!(text.contains("Cargo 读不到任何构建目标"), "{text}");
    assert!(
        text.contains("rzc check src/main.zh"),
        "须给出能走通的命令：{text}"
    );
    assert!(
        text.contains("no targets specified in the manifest"),
        "应写明直接 cargo build 会撞的那句原话：{text}"
    );

    // 生成入口产物后自检转绿：不得留下长期误报。
    // 走 `rzc check`（`rzc transpile` 只打印英文、不落产物）；不断言其退出码——
    // 入口产物在调用 cargo 之前就已写好，沙箱里没装工具链时 cargo 会失败，
    // 但自检只看「Cargo 读不读得到 target」，只看产物
    rzc()
        .env("HOME", home.path())
        .env_remove("RZ_LOG")
        .current_dir(work.path())
        .args(["check", "src/main.zh"])
        .output()
        .expect("运行 rzc check 失败");
    let text = String::from_utf8_lossy(
        &rzc()
            .env("HOME", home.path())
            .env_remove("RZ_LOG")
            .current_dir(work.path())
            .arg("doctor")
            .assert()
            .success()
            .get_output()
            .stdout,
    )
    .to_string();
    assert!(
        text.contains("Cargo 可读入口: 正常"),
        "产物齐备后应转绿：{text}"
    );
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
    rzc_in_private_home(home.path())
        .args(["lang", "install"])
        .arg(&bad)
        .assert()
        .failure()
        .stderr(predicates::str::starts_with("错误："));

    // 合法目录安装成功
    rzc_in_private_home(home.path())
        .args(["lang", "install"])
        .arg(&pack)
        .assert()
        .success();
    // list 中可见 zz（用户安装标记）
    rzc_in_private_home(home.path())
        .args(["lang", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("zz"));
    // 重复安装（无 --force）失败
    rzc_in_private_home(home.path())
        .args(["lang", "install"])
        .arg(&pack)
        .assert()
        .failure();
    // --force 重装成功
    rzc_in_private_home(home.path())
        .args(["lang", "install", "--force"])
        .arg(&pack)
        .assert()
        .success();
    // 删除成功
    rzc_in_private_home(home.path())
        .args(["lang", "remove", "zz"])
        .assert()
        .success();
    // 再删：已不存在 → 失败
    rzc_in_private_home(home.path())
        .args(["lang", "remove", "zz"])
        .assert()
        .failure();
    // 路径穿越代码被拒绝
    rzc_in_private_home(home.path())
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

/// AI 模式的失败判定必须发生在写文件之前：拼错的 `--provider` 与缺失的密钥
/// 都不该留下一堆待翻译文件；`--provider openai` 需被识别为 AI 链路，而不是
/// 静默按 rule 跑完（可选值与变量名在任何界面语言下都是英文字面）
#[test]
fn test_mapping_scaffold_ai_validation_precedes_writes() {
    let home = isolated_home();

    // 未知服务商：失败 + 列出可选值 + 不产生任何文件
    let out = tempfile::tempdir().expect("创建输出目录失败");
    rzc()
        .env("HOME", home.path())
        .args([
            "mapping",
            "scaffold",
            "zh",
            "xx",
            "--provider",
            "zzz",
            "--output",
        ])
        .arg(out.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "deepseek / openai / compatible / rule",
        ));
    assert!(walkdir(out.path()).is_empty(), "未知服务商不应写入任何文件");

    // openai 别名走 AI 链路：无密钥时失败并点名 RZ_AI_API_KEY，同样不落文件
    let out = tempfile::tempdir().expect("创建输出目录失败");
    let mut cmd = rzc();
    cmd.env("HOME", home.path())
        .args([
            "mapping",
            "scaffold",
            "zh",
            "xx",
            "--provider",
            "openai",
            "--output",
        ])
        .arg(out.path());
    for 密钥 in ["DEEPSEEK_API_KEY", "RZ_AI_API_KEY", "OPENAI_API_KEY"] {
        cmd.env_remove(密钥);
    }
    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("RZ_AI_API_KEY"));
    assert!(walkdir(out.path()).is_empty(), "缺少密钥不应写入任何文件");
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
