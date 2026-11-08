//! rzc 子命令冒烟测试：进程级最小行为验证
//!
//! 只覆盖不依赖网络与外部工具链的面：`--version`、`lang list`（内置
//! 语言包编译期嵌入）、`rzc transpile`（纯内存转译，不调 rustc/cargo）。
//! 编译诊断链路的端到端测试由 LSP 端到端测试（crates/lsp/tests/e2e.rs）与
//! 教程验证（tools/verify-tutorials.py）覆盖。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 测试沙箱根目录（进程内唯一，仅创建一次）
///
/// 测试必须与开发者机器隔离：`~/.rz/lang-packs` 中的用户安装语言包会改变
/// `lang list` 的计数（断言「11」失真），`~/.rz` 缓存会影响转译告警输出。
/// 统一把主目录（HOME）、用户目录（USERPROFILE）与语言包目录变量指向空沙箱，使结果仅取决于
/// 编译期嵌入的内置语言包与源码本身。
///
/// 失败输出的行首前缀同样按界面语言生成（本文件未定 RZ_LANG 时解析为 zh →
/// 「错误：」），因此错误类断言匹配母语前缀而非旧的硬编码 "Error:"。
fn 沙箱根() -> PathBuf {
    static 根: OnceLock<PathBuf> = OnceLock::new();
    根.get_or_init(|| {
        let 目录 = std::env::temp_dir().join(format!("rzc-test-sandbox-{}", std::process::id()));
        std::fs::create_dir_all(&目录).expect("创建测试沙箱目录失败");
        目录
    })
    .clone()
}

/// 指向**私有** HOME 与私有语言包目录的命令构造器
///
/// `rzc()` 把语言包目录变量预设为进程内**共享**的沙箱子目录，而
/// `global_lang_dir()` 优先读该变量、其次才看 `HOME`；因此只覆盖 `HOME` 的测试
/// 仍会写进共享目录，与 `lang list` 的内置包计数断言相撞（并行时偶发 12 ≠ 11）。
/// 凡读写全局语言包目录的测试都用本函数，两个位置一起私有化。
fn 私有主目录命令(主目录: &Path) -> assert_cmd::Command {
    let mut 命令值 = 命令构造器();
    命令值
        .env("HOME", 主目录)
        .env("RZ_LANG_DIR", 主目录.join("lang-packs"));
    命令值
}

fn 命令构造器() -> assert_cmd::Command {
    let mut 命令值 = assert_cmd::Command::cargo_bin("rzc").expect("应能定位 rzc 二进制");
    let 沙箱 = 沙箱根();
    命令值
        .env("HOME", &沙箱)
        .env("USERPROFILE", &沙箱)
        .env("RZ_LANG_DIR", 沙箱.join("lang-packs"))
        .env_remove("RZ_LOG");
    命令值
}

/// `--version` 输出 `rzc <版本>`，且与 Cargo.toml 声明一致
#[test]
fn 测试_版本输出含名称与版本() {
    命令构造器()
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
fn 测试_语言列表列出内置语言包() {
    let 主目录 = 隔离主目录();
    私有主目录命令(主目录.path())
        .args(["lang", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("11"))
        .stdout(predicates::str::contains("zh"));
}

/// `rzc transpile` 纯内存转译：母语关键字映射为标准 Rust，不产生文件
#[test]
fn 测试_转译输出标准语法() {
    let 目录 = tempfile::tempdir().expect("创建临时目录失败");
    let file = 目录.path().join("main.zh");
    std::fs::write(&file, "函数 主函数() {\n    打印行!(\"你好\");\n}\n")
        .expect("写入测试源码失败");

    命令构造器()
        .arg("transpile")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicates::str::contains("fn main()"))
        .stdout(predicates::str::contains("println!"));
}

/// `rzc cheat --lang zh`：输出母语↔Rust 速查表（关键字节含「函数」对应的函数关键字）
#[test]
fn 测试_速查中文输出映射表() {
    命令构造器()
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
fn 测试_速查输出标记表格() {
    命令构造器()
        .args(["cheat", "zh", "--markdown"])
        .assert()
        .success()
        .stdout(predicates::str::contains("| zh | Rust |"))
        .stdout(predicates::str::contains("| 函数 | `fn` |"));
}

/// `rzc cheat ja`：非中文语言包同样可输出速查表（全球用户路径）
#[test]
fn 测试_速查日文输出映射表() {
    命令构造器()
        .args(["cheat", "ja"])
        .assert()
        .success()
        .stdout(predicates::str::contains("ja ↔ Rust"))
        .stdout(predicates::str::contains("Keywords"));
}

/// `rzc init` 缺省跟随系统区域语言：德语区域（`de`）环境下生成 `src/main.de`（全球用户第一个项目是母语）
#[test]
fn 测试_初始化默认语言跟随系统区域() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    命令构造器()
        .env("HOME", 主目录.path())
        .env("LC_ALL", "de_DE.UTF-8")
        .env("LANG", "de_DE.UTF-8")
        .current_dir(工作目录.path())
        .args(["init", "demo"])
        .assert()
        .success()
        .stdout(predicates::str::contains("demo"));
    assert!(
        工作目录.path().join("demo/src/main.de").is_file(),
        "系统语言为 de 时缺省应生成 src/main.de"
    );
}

/// 回归：`--lang` 指向没有语言包的代码（如意大利语代码）必须立即报错
///
/// 旧行为是“创建成功”但只生成 `src/main.it`，等到首次 `rzc run` 才报未知扩展名。
#[test]
fn 测试_初始化拒绝未知语言() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    命令构造器()
        .env("HOME", 主目录.path())
        .current_dir(工作目录.path())
        .args(["init", "demo", "--lang", "it"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("'it'"))
        // 报错同时给出可用语言代码，用户不用去翻文档
        .stderr(predicates::str::contains("ja"));
    assert!(
        !工作目录.path().join("demo").exists(),
        "拒绝时应不留下半成品项目目录"
    );
}

/// 系统区域语言没有对应语言包且非交互（管道/CI）：提示已回退中文，不再静默降级
#[test]
fn 测试_初始化未知区域报告回退() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    命令构造器()
        .env("HOME", 主目录.path())
        .env("LC_ALL", "it_IT.UTF-8")
        .env_remove("LANG")
        .env_remove("LC_MESSAGES")
        .env_remove("RZ_LANG")
        .current_dir(工作目录.path())
        .write_stdin("")
        .args(["init", "demo"])
        .assert()
        .success()
        .stdout(predicates::str::contains("未识别系统语言 'it'"));
    assert!(
        工作目录.path().join("demo/src/main.zh").is_file(),
        "回退后应生成 src/main.zh"
    );
}

/// 回归（#7）：`--no-lint` 静默教学风格检查提示
///
/// 项目开发（非教学）场景中，未标注类型等初学者提示每次转译刷屏；
/// 默认输出（标准错误含 `[lint]` 标签）与关闭后输出（标准错误为空）对比验证。
/// HOME 隔离到临时目录：缓存（~/.rz）不跨测试/机器残留。两个文件内容各自
/// 不同（不同变量名）只是让两次都是新鲜语境；教学告警本身已与缓存命中与否
/// 无关（回归见教学告警随转译缓存的用例）。
#[test]
fn 测试_关闭检查静默教学提示() {
    let 目录 = tempfile::tempdir().expect("创建临时目录失败");
    let 官方源码 = 目录.path().join("official.zh");
    let 静默源码 = 目录.path().join("silenced.zh");
    // 两个文件内容不同（不同变量名）→ 两次都是缓存未命中，管线真实执行
    std::fs::write(
        &官方源码,
        "函数 主函数() {\n    让 数量甲 = 5;\n    打印行!(\"{}\", 数量甲);\n}\n",
    )
    .expect("写入测试源码失败");
    std::fs::write(
        &静默源码,
        "函数 主函数() {\n    让 数量乙 = 6;\n    打印行!(\"{}\", 数量乙);\n}\n",
    )
    .expect("写入测试源码失败");

    // 默认：教学风格检查告警输出到标准错误（含 `[lint]` 标签）
    命令构造器()
        .env("HOME", 目录.path())
        .env_remove("RZ_LOG")
        .arg("transpile")
        .arg(&官方源码)
        .assert()
        .success()
        .stderr(predicates::str::contains("[lint]"));

    // `--no-lint`：转译输出不变，教学风格检查告警静默（标准错误无输出）
    命令构造器()
        .env("HOME", 目录.path())
        .env_remove("RZ_LOG")
        .args(["--no-lint", "transpile"])
        .arg(&静默源码)
        .assert()
        .success()
        .stdout(predicates::str::contains("let 数量乙 = 6;"))
        .stderr(predicates::str::is_empty());
}

/// 回归（缺陷 B）：教学告警不得随转译缓存命中而消失，且每条带文件归属
///
/// 旧实现把告警挂在「缓存未命中才执行」的转译闭包里：HOME 隔离下同一份源码连
/// 跑两次 `rzc check`，第一次出「`[lint]` …：魔法数字「5」…」」，第二次一条告警
/// 都不出。学习者会把它读成「提示消失＝问题已修」。同一次运行里入口文件的告警
/// 是裸行列、兄弟文件却带 `工具.zh：` 前缀，两种格式混着出，也没法把告警对回文件。
#[test]
fn 测试_教学告警在转译缓存命中后仍存在() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(工作目录.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        工作目录.path().join("Cargo.toml"),
        "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    // 入口与兄弟模块各含一个魔法数字：两条告警分属不同文件，都要有归属
    std::fs::write(
        工作目录.path().join("src/main.zh"),
        "模块 工具;\n\n函数 主函数() {\n    让 数量 = 5;\n    打印行!(\"{}\", 数量);\n    工具::算一算();\n}\n",
    )
    .unwrap();
    std::fs::write(
        工作目录.path().join("src/工具.zh"),
        "公开 函数 算一算() {\n    让 倍数 = 42;\n    打印行!(\"{}\", 倍数);\n}\n",
    )
    .unwrap();

    // 同一份源码跑两遍 `rzc check`：第二遍转译缓存必然命中（同 HOME、同语境指纹）
    let 首跑输出 = 检查错误输出(&["check", "src/main.zh"], &主目录, &工作目录);
    let 复跑输出 = 检查错误输出(&["check", "src/main.zh"], &主目录, &工作目录);

    for (轮次, 错误输出) in [
        ("首跑（缓存未命中）", &首跑输出),
        ("复跑（缓存命中）", &复跑输出),
    ] {
        assert!(
            错误输出.contains("main.zh：第 4 行第 12 列：魔法数字「5」"),
            "{轮次}应有入口文件的教学告警，且带文件名归属：\n{错误输出}"
        );
        assert!(
            错误输出.contains("工具.zh：第 2 行第 12 列：魔法数字「42」"),
            "{轮次}应有兄弟文件的教学告警：\n{错误输出}"
        );
    }

    // 缓存命中与否不得改变告警集合（剔掉时间戳与级别前缀后逐字一致）
    assert_eq!(
        教学告警正文(&首跑输出),
        教学告警正文(&复跑输出),
        "两次运行的教学告警应逐字一致——缓存只复用转译产物，源码里的问题还在原地"
    );

    // 对照（防误伤）：`--no-lint` 仍静默全部教学风格检查，缓存命中路径也一样静默
    let 静默后 = 检查错误输出(&["--no-lint", "check", "src/main.zh"], &主目录, &工作目录);
    assert!(
        !静默后.contains("魔法数字"),
        "--no-lint 应静默全部教学告警（缓存命中路径同样静默）：\n{静默后}"
    );
}

/// 取出日志行里 `[lint]` 之后的告警正文（剔掉时间戳与级别，只比告警本身）
fn 教学告警正文(输出文本: &str) -> Vec<&str> {
    输出文本
        .lines()
        .filter_map(|日志行| {
            日志行
                .rfind("[lint] ")
                .map(|位置| &日志行[位置 + "[lint] ".len()..])
        })
        .collect()
}

/// 按给定参数跑一次 `rzc check` 类命令，返回标准错误
///
/// HOME 隔离到临时目录（~/.rz 转译缓存不跟真实用户目录相碰）、清除日志环境变量
/// （界面语言与日志级别不跟机器环境相碰），多个断言分支共用同一份命令构造。
fn 检查错误输出(
    参数: &[&str],
    主目录: &tempfile::TempDir,
    工作目录: &tempfile::TempDir,
) -> String {
    let 错误输出 = 命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(工作目录.path())
        .args(参数)
        .assert()
        .success()
        .get_output()
        .stderr
        .clone();
    String::from_utf8(错误输出).expect("stderr 应为 UTF-8")
}

// ========== 进程级子命令链路（真实 rustc / 文件系统，隔离 HOME） ==========

/// 新建隔离 HOME 的临时目录（~/.rz 缓存不跨测试污染）
fn 隔离主目录() -> tempfile::TempDir {
    tempfile::tempdir().expect("创建 HOME 临时目录失败")
}

/// 好程序：编译运行必过、无警告
const 好程序: &str = "函数 主函数() {\n    打印行!(\"输出：{}\", 42);\n}\n";

/// 中文方言库入口样例（`src/lib.zh` 用）：只有公开函数，没有主函数
const 好库: &str = "公开 函数 答案() -> 整数 {\n    6 * 7\n}\n";

/// 兄弟库入口样例（`src/lib.zh` 用，可执行入口与库入口配方）：库里的函数供主程序 `use 包名::`
const 翻倍库: &str = "公开 函数 翻倍(x: 整数) -> 整数 {\n    x * 2\n}\n";

/// 引用**本项目自己的库** 的主程序（`rzc check src/lib.<ext>` 之后才可解析）
const 主程序引用自有库: &str =
    "使用 calc::翻倍;\n\n函数 主函数() {\n    打印行!(\"{}\", 翻倍(21));\n}\n";

/// 坏程序：E0384（不可变变量重复赋值），CLI 须翻译成母语教学诊断
const 坏程序: &str =
    "函数 主函数() {\n    让 数量 = 10;\n    数量 = 数量 + 1;\n    打印行!(\"{}\", 数量);\n}\n";

/// `rzc init` 生成完整项目骨架
#[test]
fn 测试_初始化创建项目骨架() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    命令构造器()
        .env("HOME", 主目录.path())
        .current_dir(工作目录.path())
        .args(["init", "demo", "--lang", "zh"])
        .assert()
        .success()
        .stdout(predicates::str::contains("demo"));
    assert!(
        工作目录.path().join("demo/Cargo.toml").is_file(),
        "应生成 Cargo.toml"
    );
    assert!(
        工作目录.path().join("demo/src/main.zh").is_file(),
        "应生成 src/main.zh 入口样板"
    );
    assert!(
        工作目录.path().join("demo/.gitignore").is_file(),
        "应生成 .gitignore（托管生成物不入库）"
    );
    let 忽略配置 =
        std::fs::read_to_string(工作目录.path().join("demo/.gitignore")).unwrap_or_default();
    assert!(
        忽略配置.contains(".rz/gen/"),
        ".gitignore 应忽略托管生成目录，实际内容：{忽略配置}"
    );
}

/// `rzc run`：无依赖教学项目走 rustc 直调路径，程序标准输出原样透传
#[test]
fn 测试_运行执行转译后程序() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    // 初始化骨架后覆盖为可断言输出的程序
    命令构造器()
        .env("HOME", 主目录.path())
        .current_dir(工作目录.path())
        .args(["init", "run-demo", "--lang", "zh"])
        .assert()
        .success();
    let 主程序中文源码 = 工作目录.path().join("run-demo/src/main.zh");
    std::fs::write(&主程序中文源码, 好程序).expect("写入 main.zh 失败");

    命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(工作目录.path().join("run-demo"))
        .args(["run", "src/main.zh"])
        .assert()
        .success()
        .stdout(predicates::str::contains("输出：42"));
}

/// `rzc run <非默认入口>`：多文件项目下 cargo 只会运行 src/main，
/// 非默认入口（示例目录等）必须明确报错，而不是静默运行另一个程序
#[test]
fn 测试_运行非入口文件失败并给提示() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    let 根目录 = 工作目录.path().join("demo");
    std::fs::create_dir_all(根目录.join("src")).expect("创建 src 失败");
    std::fs::create_dir_all(根目录.join("examples")).expect("创建 examples 失败");
    std::fs::write(
        根目录.join("Cargo.toml"),
        "[package]\nname=\"demo\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(根目录.join("src/main.zh"), 好程序).unwrap();
    std::fs::write(根目录.join("examples/示例.zh"), 好程序).unwrap();

    let 输出结果 = 命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(&根目录)
        .args(["run", "examples/示例.zh"])
        .assert()
        .failure();
    let 错误输出 = String::from_utf8_lossy(&输出结果.get_output().stderr).to_string();
    assert!(错误输出.starts_with("错误："), "{错误输出}");
    assert!(
        错误输出.contains("不是项目默认入口"),
        "应提示非默认入口：{错误输出}"
    );
    assert!(
        错误输出.contains("cargo run --example"),
        "应给出 cargo 运行方式：{错误输出}"
    );
}

/// `rzc check` 好程序：退出码 0
#[test]
fn 测试_检查好程序成功() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(工作目录.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        工作目录.path().join("Cargo.toml"),
        "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(工作目录.path().join("src/main.zh"), 好程序).unwrap();
    命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(工作目录.path())
        .args(["check", "src/main.zh"])
        .assert()
        .success();
}

/// 项目里还没有 Cargo 能读取的构建目标时，`rzc add` 必须补母语指引
///
/// `rzc init` 只生成方言入口（`src/main.en`），Cargo 认不到构建目标，于是新建项目
/// 后**第一条 `rzc add` 必然失败**，原文是构建器内部术语
/// （`no targets specified in the manifest`）且不含任何可执行动作。cargo 先解析
/// 清单、后访问注册表，故本用例离线即复现失败路径（实测无
/// `Updating crates.io index`）。断言同时覆盖 zh/en 两种界面语言，并确认 cargo
/// 原文未被吞掉。
#[test]
fn 测试_添加依赖无构建目标时输出母语提示() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(工作目录.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        工作目录.path().join("Cargo.toml"),
        "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(工作目录.path().join("src/main.en"), "fn main() {}\n").unwrap();

    let 运行 = |语言: &str| {
        let 输出 = 命令构造器()
            .env("HOME", 主目录.path())
            .env("RZ_LANG", 语言)
            .env_remove("RZ_LOG")
            .current_dir(工作目录.path())
            .args(["add", "any-crate"])
            .assert()
            .failure()
            .get_output()
            .stderr
            .clone();
        String::from_utf8_lossy(&输出).to_string()
    };

    let 中文输出 = 运行("zh");
    assert!(
        中文输出.contains("还没有 Cargo 能读取的构建入口"),
        "应有母语指引：{中文输出}"
    );
    assert!(
        中文输出.contains("rzc check src/main.en"),
        "指引须含可执行动作：{中文输出}"
    );
    assert!(
        中文输出.contains("no targets"),
        "cargo 原文须原样可见：{中文输出}"
    );

    let 英文输出 = 运行("en");
    assert!(
        英文输出.contains("no build target Cargo can read"),
        "应有英文指引：{英文输出}"
    );
    assert!(
        英文输出.contains("Run `rzc check src/main.en` first"),
        "{英文输出}"
    );
    assert!(
        英文输出.contains("no targets"),
        "cargo 原文须原样可见：{英文输出}"
    );
}

/// 提示里的入口名取 `main.<ext>`，不受目录读取顺序影响
///
/// 项目里除入口外还有别的方言文件时，随便挑一个会让读者去检查一个**模块文件**
/// ——那不是入口，生成不了 `src/main.rs`，墙照旧撞第二次。兄弟文件特意取 `api.zh`：
/// 它字典序在 `main.zh` 之前，所以「排序后取第一个」的实现会在这里露馅，
/// 只有显式优先主入口才通过。
#[test]
fn 测试_添加提示优先主入口() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(工作目录.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        工作目录.path().join("Cargo.toml"),
        "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(工作目录.path().join("src/main.zh"), 好程序).unwrap();
    std::fs::write(工作目录.path().join("src/api.zh"), "fn helper() {}\n").unwrap();
    std::fs::write(工作目录.path().join("src/工具.zh"), "fn helper2() {}\n").unwrap();

    for _ in 0..3 {
        let 输出 = 命令构造器()
            .env("HOME", 主目录.path())
            .env("RZ_LANG", "zh")
            .env_remove("RZ_LOG")
            .current_dir(工作目录.path())
            .args(["add", "any-crate"])
            .assert()
            .failure()
            .get_output()
            .stderr
            .clone();
        let 错误输出 = String::from_utf8_lossy(&输出).to_string();
        assert!(
            错误输出.contains("rzc check src/main.zh"),
            "应点名入口文件：{错误输出}"
        );
        assert!(
            !错误输出.contains("rzc check src/api.zh"),
            "不该把排序靠前的模块文件当入口：{错误输出}"
        );
    }
}

/// 清单自身有问题时**不得**套用「无构建目标」指引
///
/// 命中判定要求两个条件同时成立（项目确实无构建目标 + cargo 确实报在无构建目标
/// 上）。少了后半个条件，包名为空这类真清单错误也会被指向 `rzc check`，
/// 把读者引向无关动作；本用例锁定不误报。
#[test]
fn 测试_清单错误不误报缺少构建目标() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(工作目录.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        工作目录.path().join("Cargo.toml"),
        "[package]\nname=\"\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(工作目录.path().join("src/main.en"), "fn main() {}\n").unwrap();

    let 输出 = 命令构造器()
        .env("HOME", 主目录.path())
        .env("RZ_LANG", "zh")
        .env_remove("RZ_LOG")
        .current_dir(工作目录.path())
        .args(["add", "any-crate"])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let 错误输出 = String::from_utf8_lossy(&输出).to_string();
    assert!(
        !错误输出.contains("还没有 Cargo 能读取的构建入口"),
        "真 manifest 错误不该给无 target 指引：{错误输出}"
    );
    assert!(
        错误输出.contains("package name cannot be empty"),
        "cargo 原文可见：{错误输出}"
    );
}

/// 方言库入口 `src/lib.<ext>` 落 Cargo 自动发现的库根，造库零配置
///
/// 此前库入口名不在入口名集合里：纯库目录里 `rzc check src/lib.zh` 被清单预检拦下
/// （「Cargo.toml 没有声明编译目标」），唯一的可用配方是在 Cargo.toml 手写
/// `[lib] path = ".rz/gen/src/lib.rs"`——把「造一个方言库」变成一段要背的配置。
/// 现在产物直写 `src/lib.rs`，Cargo 认得它，`rzc check` 与 `cargo build` 都过。
#[test]
fn 测试_检查库入口写出源库产物() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(工作目录.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        工作目录.path().join("Cargo.toml"),
        "[package]\nname=\"greetlib\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(工作目录.path().join("src/lib.zh"), 好库).unwrap();
    // 升级前库文件不是入口，产物写在托管目录；那份残留现在没人引用了
    std::fs::create_dir_all(工作目录.path().join(".rz/gen/src")).unwrap();
    std::fs::write(工作目录.path().join(".rz/gen/src/lib.rs"), "陈旧产物").unwrap();

    命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(工作目录.path())
        .args(["check", "src/lib.zh"])
        .assert()
        .success();

    let 库产物路径 = 工作目录.path().join("src/lib.rs");
    let 产物内容 = std::fs::read_to_string(&库产物路径).expect("应生成 src/lib.rs（库 crate 根）");
    // 母语函数名不翻译（只有关键字与类型有映射），断言产物形态即可
    assert!(
        产物内容.contains("pub fn") && 产物内容.contains("-> i32"),
        "{产物内容}"
    );
    // 托管目录不该再留一份同源的旧产物：留着会被 rustc 默认模块查找命中
    assert!(
        !工作目录.path().join(".rz/gen/src/lib.rs").exists(),
        "入口产物落 crate 根后托管副本应清掉"
    );
}

/// `rzc run` 库入口给母语指引，而不是让人去找一个不存在的可执行入口
///
/// 库没有可执行程序，cargo 此时报 `a bin target must be available for cargo run`。
/// `run_not_default_entry()` 那条（指向 `cargo run --bin <名称>`）在这里是误导：
/// 项目里根本没有可执行入口。故库入口单列一条消息，给出的动作只有「校验」
/// 与「补主程序」。界面语言跟随文件扩展名（`.zh` → zh，`.en` → en）。
#[test]
fn 测试_运行库入口说明无可执行程序() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(工作目录.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        工作目录.path().join("Cargo.toml"),
        "[package]\nname=\"greetlib\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(工作目录.path().join("src/lib.zh"), 好库).unwrap();
    std::fs::write(
        工作目录.path().join("src/lib.en"),
        "pub fn answer() -> i32 { 6 * 7 }\n",
    )
    .unwrap();

    let 运行 = |file: &str| {
        let 输出 = 命令构造器()
            .env("HOME", 主目录.path())
            .env_remove("RZ_LOG")
            .current_dir(工作目录.path())
            .args(["run", file])
            .assert()
            .failure()
            .get_output()
            .stderr
            .clone();
        String::from_utf8_lossy(&输出).to_string()
    };

    let 中文输出 = 运行("src/lib.zh");
    assert!(
        中文输出.contains("库里没有可执行的主程序"),
        "应有母语说明：{中文输出}"
    );
    assert!(
        中文输出.contains("rzc check src/lib.zh"),
        "须给出可执行动作：{中文输出}"
    );
    assert!(
        !中文输出.contains("不是项目默认入口"),
        "不该套用非默认入口那条消息：{中文输出}"
    );

    let 英文输出 = 运行("src/lib.en");
    assert!(
        英文输出.contains("library has no executable main"),
        "应有英文说明：{英文输出}"
    );
    assert!(
        英文输出.contains("rzc check src/lib.en"),
        "须给出可执行动作：{英文输出}"
    );
}

/// 兄弟库文件不得被顺带写成库根：`rzc check src/main.zh` 不造库目标
///
/// 项目里同时有 `src/main.zh` 与 `src/lib.zh` 时，若项目级转译把两者都按入口规则
/// 落库根，就等于替用户凭空加出一个编译目标——实测非 ASCII 包名立刻撞上
/// rustc 硬错 `crate name ... passed to --extern is not a valid ASCII identifier`
/// （可执行入口链接库必须 `--extern`），原先不参与编译的库还会把陈年错误一起报出来。
/// 入口产物只由**本次请求的文件**写，这条边界靠本用例钉住。
#[test]
fn 测试_检查主程序不物化兄弟库() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(工作目录.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        工作目录.path().join("Cargo.toml"),
        "[package]\nname=\"双入口\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(工作目录.path().join("src/main.zh"), 好程序).unwrap();
    std::fs::write(工作目录.path().join("src/lib.zh"), 好库).unwrap();

    命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(工作目录.path())
        .args(["check", "src/main.zh"])
        .assert()
        .success();

    assert!(工作目录.path().join("src/main.rs").exists());
    assert!(
        !工作目录.path().join("src/lib.rs").exists(),
        "未请求的兄弟库文件不该被写成库 crate 根"
    );
}

/// 老配方项目里，未请求的兄弟库文件仍要被项目级转译刷新
///
/// `[lib] path` 指着托管目录的项目（纳管库入口之前的唯一造库配方）靠的是
/// `rzc check src/main.zh` 顺带把 `src/lib.zh` 转译到那份声明路径。若因为
/// 「库文件现在也是入口」就把它加进兄弟跳过名单，cargo 会编译一份不再更新的产物
/// ——代码改了却不报错，比报错更糟。故库入口只在**被请求**时落库根。
#[test]
fn 测试_兄弟库刷新声明的生成产物() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(工作目录.path().join(".rz/gen/src")).unwrap();
    std::fs::create_dir_all(工作目录.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        工作目录.path().join("Cargo.toml"),
        "[package]\nname=\"mixlib\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[lib]\npath=\".rz/gen/src/lib.rs\"\n",
    )
    .unwrap();
    std::fs::write(工作目录.path().join("src/main.zh"), 好程序).unwrap();
    std::fs::write(工作目录.path().join("src/lib.zh"), 好库).unwrap();
    let 生成库产物 = 工作目录.path().join(".rz/gen/src/lib.rs");
    std::fs::write(&生成库产物, "陈旧产物").unwrap();

    命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(工作目录.path())
        .args(["check", "src/main.zh"])
        .assert()
        .success();

    let 刷新后产物 = std::fs::read_to_string(&生成库产物).expect("声明的库产物应被刷新");
    assert!(
        刷新后产物.contains("pub fn"),
        "陈旧产物应被覆盖：{刷新后产物}"
    );
    // 请求的是主程序，库产物不得被当成入口挪走
    assert!(!工作目录.path().join("src/lib.rs").exists());
}

/// 可执行入口与库入口并存的项目里，检查主程序**不得**清掉兄弟库已生成的库根产物
///
/// 库配方是「先 `rzc check src/lib.zh` 生成 src/lib.rs，再检查主程序」。项目级
/// 转译把兄弟 `src/lib.zh` 也走一遍时，会按旧布局规则清理「源文件旁边的同名 .rs」——
/// 而 `src/lib.rs` 恰恰是 Cargo 的库目标产物，不是旧布局遗留。清掉之后第二次检查
/// 立刻 E0432，等于这条配方自己把自己拆了（本轮实机复现，先前四个用例都没覆盖到
/// 「两次检查的先后顺序」这一维度）。反向验证：恢复无条件清理，第二步失败。
#[test]
fn 测试_检查主程序保留兄弟库的库产物() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    let 根目录 = 工作目录.path();
    std::fs::create_dir_all(根目录.join("src")).expect("创建 src 失败");
    std::fs::write(
        根目录.join("Cargo.toml"),
        "[package]\nname=\"calc\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(根目录.join("src/lib.zh"), 翻倍库).unwrap();
    std::fs::write(根目录.join("src/main.zh"), 主程序引用自有库).unwrap();

    let 执行检查 = |参数: &str| {
        命令构造器()
            .env("HOME", 主目录.path())
            .env_remove("RZ_LOG")
            .current_dir(根目录)
            .args(["check", 参数])
            .assert()
            .success();
    };

    执行检查("src/lib.zh");
    assert!(
        根目录.join("src/lib.rs").is_file(),
        "库入口应写出 src/lib.rs"
    );
    执行检查("src/main.zh");
    assert!(
        根目录.join("src/lib.rs").is_file(),
        "check 主程序不得清掉兄弟库的 crate 根产物"
    );
}

/// 主程序引用**自己的库** 时不得建议 `rzc add 包名`
///
/// rustc 分不出「自己的库还没生成产物」和「没声明的第三方库」，两种都报 E0432 并
/// 建议 `cargo add`；而 Cargo 明确拒绝包依赖自己（实测报错原文说明不能把包添加为
/// 它自身的依赖）。此前 rzc 把这条建议原样译成母语、又追加一条同款
/// 提示，读者照着做一定白跑一趟。现在两处都换成能走通的动作：先检查库入口。
#[test]
fn 测试_自有库提示指向库入口而非添加依赖() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    let 根目录 = 工作目录.path();
    std::fs::create_dir_all(根目录.join("src")).expect("创建 src 失败");
    std::fs::write(
        根目录.join("Cargo.toml"),
        "[package]\nname=\"calc\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(根目录.join("src/lib.zh"), 翻倍库).unwrap();
    std::fs::write(根目录.join("src/main.zh"), 主程序引用自有库).unwrap();

    let 输出 = 命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(根目录)
        .args(["check", "src/main.zh"])
        .assert()
        .failure()
        .get_output()
        .clone();
    // 诊断正文走标准输出、加依赖提示走标准错误，断言要覆盖两条流
    let 输出文本 = format!(
        "{}{}",
        String::from_utf8_lossy(&输出.stdout),
        String::from_utf8_lossy(&输出.stderr)
    );

    assert!(
        输出文本.contains("是本项目的包名"),
        "应点明是自己的包名：{输出文本}"
    );
    assert!(
        输出文本.contains("rzc check src/lib.zh"),
        "须给出能走通的动作：{输出文本}"
    );
    assert!(
        !输出文本.contains("疑似未声明的第三方库"),
        "不该把自有 crate 说成第三方库：{输出文本}"
    );
    assert!(
        !输出文本.contains("若要使用名为 calc 的第三方库"),
        "rustc 的 `cargo add` 建议应被剔除：{输出文本}"
    );
}

/// 真正没声明的第三方库照旧给 `rzc add` 提示（别把正确提示一起吞了）
///
/// 本地包名判定只在名字命中 `[package] name` / `[lib] name` 时接管提示；这里包名
/// 故意取一个与所导入真库名不同的本地包名，旧行为必须原样保留。
#[test]
fn 测试_未知库仍触发第三方提示() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    let 根目录 = 工作目录.path();
    std::fs::create_dir_all(根目录.join("src")).expect("创建 src 失败");
    std::fs::write(
        根目录.join("Cargo.toml"),
        "[package]\nname=\"notserde\"\nversion=\"0.1.0\"\nedition=\"2021\"\n\n[dependencies]\n",
    )
    .unwrap();
    std::fs::write(
        根目录.join("src/main.zh"),
        "使用 serde::序列化;\n\n函数 主函数() {\n    打印行!(\"ok\");\n}\n",
    )
    .unwrap();

    let 输出 = 命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(根目录)
        .args(["check", "src/main.zh"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let 输出文本 = format!(
        "{}{}",
        String::from_utf8_lossy(&输出.stdout),
        String::from_utf8_lossy(&输出.stderr)
    );
    assert!(
        输出文本.contains("rzc add serde"),
        "第三方库仍应给 rzc add 提示：{输出文本}"
    );
    assert!(!输出文本.contains("是本项目的包名"), "{输出文本}");
}

/// 可执行入口与库入口并存项目的非 ASCII 包名硬错给母语译文（不再喷英文、不再刷待译清单）
///
/// 实机测清的触发条件：**两种目标共存 + 非 ASCII 包名**——主程序完全不引用库也撞
/// （Cargo 要用 `--extern 计算器=…` 把库交给主程序），而只有库或只有主程序都能编。
/// 这条无错误码的 rustc 消息此前 11 语言全都没有条目，终端只剩英文原句 +
/// 「暂无译文，已记入待译清单」。本用例钉住译文到位（诊断走标准输出）。
#[test]
fn 测试_非英文字符包名双目标打印本地化外部链接错误() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    let 根目录 = 工作目录.path();
    std::fs::create_dir_all(根目录.join("src")).expect("创建 src 失败");
    std::fs::write(
        根目录.join("Cargo.toml"),
        "[package]\nname=\"计算器\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(根目录.join("src/lib.zh"), 好库).unwrap();
    std::fs::write(根目录.join("src/main.zh"), 好程序).unwrap();

    let 运行检查 = |参数: &str| {
        命令构造器()
            .env("HOME", 主目录.path())
            .env_remove("RZ_LOG")
            .current_dir(根目录)
            .args(["check", 参数])
            .output()
            .expect("运行 rzc 失败")
    };

    // 只有库时中文包名能编（先确认这一步不是已经挂了）
    let 仅库输出 = 运行检查("src/lib.zh");
    assert!(
        仅库输出.status.success(),
        "纯库项目应能编译：{}",
        String::from_utf8_lossy(&仅库输出.stderr)
    );

    let 主程序输出 = 运行检查("src/main.zh");
    assert!(
        !主程序输出.status.success(),
        "bin+lib + 非 ASCII 包名应失败"
    );
    let 输出文本 = format!(
        "{}{}",
        String::from_utf8_lossy(&主程序输出.stdout),
        String::from_utf8_lossy(&主程序输出.stderr)
    );
    assert!(
        输出文本.contains("不是合法的 ASCII 标识符") && 输出文本.contains("计算器"),
        "应给母语译文并保留包名：{输出文本}"
    );
    assert!(
        !输出文本.contains("is not a valid ASCII identifier"),
        "不该再看到英文原句：{输出文本}"
    );
    assert!(
        !输出文本.contains("暂无译文"),
        "该消息已不再待译：{输出文本}"
    );
    // zh 模板写的是“Cargo.toml 里的包名”（不带反引号）：加了反引号会被类型
    // 映射改写成 `线程名`（标准库词表里「线程名」对应「名称」），变成误导指引
    assert!(
        !输出文本.contains("线程名"),
        "译文被类型映射误改写：{输出文本}"
    );
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
fn 测试_英文恒等语言不报告未译条目() {
    let 运行 = |主目录: &Path, 扩展名: &str, 源码: &str| {
        let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
        let 根目录 = 工作目录.path();
        std::fs::create_dir_all(根目录.join("src")).expect("创建 src 失败");
        std::fs::write(
            根目录.join("Cargo.toml"),
            "[package]\nname=\"gapcheck\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        std::fs::write(根目录.join(format!("src/main.{扩展名}")), 源码).unwrap();
        let 输出 = 命令构造器()
            .env("HOME", 主目录)
            .env_remove("RZ_LOG")
            .current_dir(根目录)
            .args(["check", &format!("src/main.{扩展名}")])
            .output()
            .expect("运行 rzc 失败");
        let 输出文本 = format!(
            "{}{}",
            String::from_utf8_lossy(&输出.stdout),
            String::from_utf8_lossy(&输出.stderr)
        );
        let 待译文件路径 = 主目录
            .join(".rz")
            .join("gaps")
            .join(format!("{扩展名}.tsv"));
        (输出文本, 待译文件路径)
    };

    // 两个 HOME 由测试作用域持有（写进闭包会随闭包返回被删，清单永远验不到）
    let 中文主目录 = 隔离主目录();
    let 英文主目录 = 隔离主目录();

    // zh：该条诊断的残段与汇总句仍不在消息表里，因此必须提示 + 落盘——
    // 它是维护者发现漏译的唯一途径
    let (中文输出文本, 中文待译文件) = 运行(
        中文主目录.path(),
        "zh",
        "函数 主函数() {\n    让 未用变量 = 1;\n    打印行!(\"ok\");\n}\n",
    );
    assert!(
        中文输出文本.contains("未使用的变量"),
        "zh 侧诊断不应被吞：{中文输出文本}"
    );
    assert!(
        中文输出文本.contains("暂无译文"),
        "zh 命中不了消息表时应提示并邀请 `rzc gaps`：{中文输出文本}"
    );
    assert!(
        中文待译文件.exists(),
        "zh 侧漏译应落盘到 {}",
        中文待译文件.display()
    );

    // en：同一句话在恒等语言下不是损失，不得提示也不得落盘
    let (英文输出文本, 英文待译文件) = 运行(
        英文主目录.path(),
        "en",
        "fn main() {\n    let unused_var = 1;\n    println!(\"ok\");\n}\n",
    );
    assert!(
        英文输出文本.contains("unused variable"),
        "en 侧诊断必须照旧可见（绝不吞诊断）：{英文输出文本}"
    );
    assert!(
        !英文输出文本.contains("have no translation") && !英文输出文本.contains("暂无译文"),
        "en 是恒等语言，原句已是最终形态，不该被告知「有消息未译」：{英文输出文本}"
    );
    assert!(
        !英文待译文件.exists(),
        "en 侧不应落盘待译清单（它永远补不完）：{}",
        英文待译文件.display()
    );
}

/// 待译清单：一行一条记录（跨行消息以转义字面量存放，可无损读回）
///
/// rustc JSON 的一类消息本身带换行（如 E0277 的「下列类型实现了该特征」清单）。
/// 旧实现把它原样落盘：一条记录炸成十几行裸续行，`rzc gaps` 报的条数不是
/// 待译句子数，且续行会被按当前界面语言误归类（历史文件里占了一半行数）。
#[test]
fn 测试_待译清单每条记录一行() {
    let 主目录 = 隔离主目录();
    let 待译目录 = 主目录.path().join(".rz").join("gaps");
    std::fs::create_dir_all(&待译目录).expect("创建 gaps 目录失败");
    let file = 待译目录.join("zt.tsv");
    // 新格式：多行消息以 `\n` 字面量存放，一条一行
    std::fs::write(
        &file,
        "zt\thelp\tE0277\tfirst line\\n  second line\\nand 56 others\nzt\tmain\t\tplain message\n",
    )
    .expect("写入清单失败");

    let 输出 = 命令构造器()
        .env("HOME", 主目录.path())
        .args(["gaps", "--lang", "zt"])
        .output()
        .expect("运行 rzc gaps 失败");
    let 输出文本 = String::from_utf8_lossy(&输出.stdout).to_string();
    let 行列表: Vec<&str> = 输出文本.lines().collect();
    assert_eq!(
        行列表.len(),
        2,
        "两条待译应输出两行，不得被消息内容拆行：{行列表:?}"
    );
    assert!(
        行列表
            .iter()
            .all(|行内容| 行内容.contains("first line\\n") || 行内容.contains("plain message")),
        "导出件应保留转义形式供工具消费：{行列表:?}"
    );

    // `--export` 产物可被引擎读回，且消息内容逐字不变
    let 导出路径 = 主目录.path().join("exported.tsv");
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["gaps", "--lang", "zt", "--export"])
        .arg(&导出路径)
        .assert()
        .success();
    let 读回记录 = i18n_rust_engine::诊断::解析清单行(
        &std::fs::read_to_string(&导出路径).expect("读取导出件失败"),
    );
    assert_eq!(读回记录.len(), 2, "导出件应被读回两条记录");
    assert!(读回记录.iter().any(|记录| 记录.3 == "plain message"));
    assert!(
        读回记录
            .iter()
            .any(|记录| 记录.3 == "first line\n  second line\nand 56 others"),
        "跨行消息导出后必须能逐字还原：{:?}",
        读回记录
    );
}

/// 待译清单：历史未转义文件（裸续行）读回不拆条，实跑一轮后完成格式迁移
///
/// 旧行为会把这些裸行当成新记录（按当前界面语言归类），且因为读不回原记录
/// 而每轮重贴同一句（本机实测 `and 56 others` 在 en/ja/ru 各积 64/42/42 次）。
#[test]
fn 测试_待译清单下次运行迁移历史多行文件() {
    let 主目录 = 隔离主目录();
    let 待译目录 = 主目录.path().join(".rz").join("gaps");
    std::fs::create_dir_all(&待译目录).expect("创建 gaps 目录失败");
    let file = 待译目录.join("zh.tsv");
    // 旧格式：一条主记录 + 一条被拆成三行的跨行记录（裸续行无列结构）
    // 续行只能拼到它前面的记录上——真实历史文件里裸行总是跟在主记录后
    std::fs::write(
        &file,
        "zh\tmain\tE9999\tanchor record\nzh\tmain\t\tlegacy multiline start\n\tlegacy tail with tab\nand its last line\n",
    )
    .expect("写入历史清单失败");

    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    let 根目录 = 工作目录.path();
    std::fs::create_dir_all(根目录.join("src")).expect("创建 src 失败");
    std::fs::write(
        根目录.join("Cargo.toml"),
        "[package]\nname=\"gapmig\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    // 两个不同变量名 → 两次都是缓存未命中，真实产生新的待译记录
    let 源码列表 = [
        "函数 主函数() {\n    让 未用变量甲 = 1;\n}\n",
        "函数 主函数() {\n    让 未用变量乙 = 2;\n}\n",
    ];
    for 源码 in 源码列表 {
        std::fs::write(根目录.join("src/main.zh"), 源码).unwrap();
        命令构造器()
            .env("HOME", 主目录.path())
            .current_dir(根目录)
            .args(["check", "src/main.zh"])
            .output()
            .expect("运行 rzc check 失败");
    }

    let 原始内容 = std::fs::read_to_string(&file).expect("清单应仍存在");
    let 记录集 = i18n_rust_engine::诊断::解析清单行(&原始内容);
    assert!(
        记录集.iter().any(|记录| 记录.0 == "zh"),
        "实跑产生的记录应落在 zh 清单里：{:?}",
        记录集
    );
    assert_eq!(
        原始内容.lines().count(),
        记录集.len(),
        "迁移后应一行一条记录（裸续行拼回上一条，不再虚增条数）"
    );
    assert!(
        !原始内容.contains("legacy multiline start\n\tlegacy tail"),
        "历史裸行应被拼回单行转义格式：{原始内容}"
    );
    assert!(
        记录集
            .iter()
            .any(|记录| 记录.3
                == "legacy multiline start\n\tlegacy tail with tab\nand its last line"),
        "拼回的历史记录内容不得丢失：{:?}",
        记录集
    );
}

/// `rzc check` 多层模块项目：递归收集 + 非入口文件按层级补 `#[path]`
///
/// 布局：src/main.zh → src/领域.zh → src/领域/工具.zh。此前非入口方言文件
/// 内的文件式模块声明没有层级路径注解，rustc 报 E0754/E0583/E0603。
#[test]
fn 测试_检查嵌套模块项目成功() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(工作目录.path().join("src/领域")).expect("创建嵌套目录失败");
    std::fs::write(
        工作目录.path().join("Cargo.toml"),
        "[package]\nname=\"nested\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        工作目录.path().join("src/main.zh"),
        "模块 领域;\n\n函数 主函数() {\n    打印行!(\"{}\", 领域::工具::加一(1));\n}\n",
    )
    .unwrap();
    std::fs::write(工作目录.path().join("src/领域.zh"), "公开 模块 工具;\n").unwrap();
    std::fs::write(
        工作目录.path().join("src/领域/工具.zh"),
        "公开 函数 加一(数: i32) -> i32 {\n    数 + 1\n}\n",
    )
    .unwrap();

    命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(工作目录.path())
        .args(["check", "src/main.zh"])
        .assert()
        .success();

    // 托管布局：非入口产物在 `.rz/gen/` 镜像目录下，嵌套路径注解仍指「父词干/子词干.rs」
    let 领域产物 = std::fs::read_to_string(工作目录.path().join(".rz/gen/src/领域.rs"))
        .expect(".rz/gen/src/领域.rs 应已转译");
    assert!(
        领域产物.contains("#[path") && 领域产物.contains("领域/工具.rs"),
        "非入口文件应按层级注解嵌套子模块：{领域产物}"
    );
    assert!(
        领域产物.contains("pub mod 工具"),
        "嵌套子模块应保持 pub（防 E0603）：{领域产物}"
    );
    assert!(
        工作目录.path().join(".rz/gen/src/领域/工具.rs").is_file(),
        "最深层产物也应镜像源码布局落在托管目录"
    );
    assert!(
        !工作目录.path().join("src/领域.rs").exists(),
        "旧布局产物应已从 src/ 清走（src/ 只留母语源文件与入口产物）"
    );
    let 主程序产物 =
        std::fs::read_to_string(工作目录.path().join("src/main.rs")).expect("main.rs 应存在");
    assert!(
        主程序产物.contains("#[path") && 主程序产物.contains("../.rz/gen/src/领域.rs"),
        "入口应跨目录注解托管产物：{主程序产物}"
    );
}

/// 托管目录自愈：源文件被删除/改名后，失去源的产物被清走并告知数量
///
/// 旧布局不敢做这种清理（源码目录混着手写 `.rs`）；托管后 `.rz/gen/` 只存机器
/// 产物，残留会让 rustc 按默认模块查找命中旧文件（被删掉的代码仍在参与
/// 编译），因此删除必须与转译同处一条链路。
#[test]
fn 测试_检查清理无源文件产物() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(工作目录.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        工作目录.path().join("Cargo.toml"),
        "[package]\nname=\"prune\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        工作目录.path().join("src/main.zh"),
        "模块 数学;\n\n函数 主函数() {\n    打印行!(\"{}\", 数学::加一(1));\n}\n",
    )
    .unwrap();
    std::fs::write(
        工作目录.path().join("src/数学.zh"),
        "公开 函数 加一(数: i32) -> i32 {\n    数 + 1\n}\n",
    )
    .unwrap();
    命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(工作目录.path())
        .args(["check", "src/main.zh"])
        .assert()
        .success();
    let 产物 = 工作目录.path().join(".rz/gen/src/数学.rs");
    assert!(产物.is_file(), "首轮应在托管目录生成模块产物");

    // 删除源文件并去掉 `模块` 声明（项目仍可编译）
    std::fs::remove_file(工作目录.path().join("src/数学.zh")).unwrap();
    std::fs::write(
        工作目录.path().join("src/main.zh"),
        "函数 主函数() {\n    打印行!(\"ok\");\n}\n",
    )
    .unwrap();
    命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(工作目录.path())
        .args(["check", "src/main.zh"])
        .assert()
        .success()
        .stdout(predicates::str::contains("已清理"));
    assert!(!产物.exists(), "失去源文件的产物应已被清走");
    assert!(
        !工作目录.path().join(".rz/gen/src/数学.rs.bak").exists(),
        "托管目录不应留下备份文件"
    );
}

/// `rzc check` 坏程序：退出码非 0，诊断为母语（E0425 等错误码 + 中文消息 +
/// 💡 教学提示），不泄漏 rustc 英文原文
#[test]
fn 测试_检查坏程序输出翻译诊断() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(工作目录.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        工作目录.path().join("Cargo.toml"),
        "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(工作目录.path().join("src/main.zh"), 坏程序).unwrap();
    let 输出结果 = 命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(工作目录.path())
        .args(["check", "src/main.zh"])
        .assert()
        .failure();
    let 合并输出 = format!(
        "{}{}",
        String::from_utf8_lossy(&输出结果.get_output().stdout),
        String::from_utf8_lossy(&输出结果.get_output().stderr)
    );
    assert!(
        合并输出.contains("E0384"),
        "应保留 rustc 错误码：{合并输出}"
    );
    assert!(
        合并输出.contains("不可变变量"),
        "错误消息应翻译为母语：{合并输出}"
    );
    assert!(合并输出.contains('💡'), "应附教学提示：{合并输出}");
}

/// `rzc eject`：导出标准 Rust 文件（零锁定毕业路径）
#[test]
fn 测试_导出写出标准语法文件() {
    let 主目录 = 隔离主目录();
    let 目录 = tempfile::tempdir().expect("创建临时目录失败");
    let 中文源码 = 目录.path().join("main.zh");
    std::fs::write(&中文源码, 好程序).unwrap();
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["eject"])
        .arg(&中文源码)
        .assert()
        .success()
        .stdout(predicates::str::contains("main.rs"));
    let 产物文本 = std::fs::read_to_string(目录.path().join("main.rs")).expect("main.rs 应已导出");
    assert!(
        产物文本.contains("fn main()"),
        "导出应为标准 Rust：{产物文本}"
    );
    assert!(
        产物文本.contains("println!"),
        "宏应还原为标准 Rust：{产物文本}"
    );
}

/// `rzc eject` 覆盖已有同名 `.rs`：与运行/检查同口径先备份为 `.rs.bak`，
/// 避免渐进迁移中手写产物被静默覆盖
#[test]
fn 测试_导出备份已有产物() {
    let 目录 = tempfile::tempdir().expect("创建临时目录失败");
    let 中文源码 = 目录.path().join("main.zh");
    std::fs::write(&中文源码, 好程序).unwrap();
    let 手写产物 = "fn main() { println!(\"手写\"); }\n";
    std::fs::write(目录.path().join("main.rs"), 手写产物).unwrap();

    命令构造器()
        .args(["eject"])
        .arg(&中文源码)
        .assert()
        .success();

    let 备份路径 = 目录.path().join("main.rs.bak");
    assert!(备份路径.is_file(), "旧产物应备份为 main.rs.bak");
    assert_eq!(
        std::fs::read_to_string(&备份路径).unwrap(),
        手写产物,
        "备份内容应为被覆盖前的手写产物"
    );
    let 产物文本 = std::fs::read_to_string(目录.path().join("main.rs")).unwrap();
    assert!(产物文本.contains("fn main()"), "新产物应已写入：{产物文本}");
}

/// `rzc eject <目录>`：递归导出全部方言文件（P-02 回归：目录直接交给
/// `fs::read_to_string` 在 Windows 报「拒绝访问（系统错误码 5）」）
///
/// 同时覆盖声明与引用一致性（P-01/P-05/P-07/P-08 同源）：模块名（版本/
/// 连接设置）与结构体字段（主机/状态）撞语言包别名时，模块声明、
/// 使用路径段与字段访问必须同名保留（有项目上下文时整体豁免）。
#[test]
fn 测试_导出目录一致转译全部文件() {
    let 目录 = tempfile::tempdir().expect("创建临时目录失败");
    let 源码 = 目录.path().join("src");
    std::fs::create_dir_all(源码.join("领域")).unwrap();
    std::fs::write(
        源码.join("main.zh"),
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
        源码.join("版本.zh"),
        "公开 函数 程序版本() -> 字符串 {\n    字符串::从(\"1.0.0\")\n}",
    )
    .unwrap();
    std::fs::write(
        源码.join("连接设置.zh"),
        "公开 结构体 数据库连接配置 {\n    公开 主机: 字符串,\n    公开 状态: 字符串,\n}",
    )
    .unwrap();
    std::fs::write(源码.join("领域.zh"), "公开 模块 工具;").unwrap();
    std::fs::write(
        源码.join("领域/工具.zh"),
        "公开 函数 问候() {\n    打印行!(\"你好\");\n}",
    )
    .unwrap();

    命令构造器().args(["eject"]).arg(&源码).assert().success();

    let 主程序产物 = std::fs::read_to_string(源码.join("main.rs")).expect("main.rs 应已导出");
    // 声明与引用同名（P-05/P-07）：不得出现 `crate::version` / `crate::Permissions`
    assert!(主程序产物.contains("mod 版本;"), "{主程序产物}");
    assert!(
        主程序产物.contains("use crate::版本::程序版本;"),
        "{主程序产物}"
    );
    assert!(
        主程序产物.contains("use crate::连接设置::数据库连接配置;"),
        "{主程序产物}"
    );
    assert!(!主程序产物.contains("crate::version"), "{主程序产物}");
    // 字段定义与访问同名（P-01/P-08）
    assert!(主程序产物.contains("当前配置.主机"), "{主程序产物}");
    assert!(主程序产物.contains("当前配置.状态"), "{主程序产物}");
    assert!(!主程序产物.contains(".host"), "{主程序产物}");
    // 库根的非 ASCII 模块注解
    assert!(主程序产物.contains("#[path = \"版本.rs\"]"), "{主程序产物}");

    let 字段产物 = std::fs::read_to_string(源码.join("连接设置.rs")).expect("连接设置.rs 应已导出");
    assert!(字段产物.contains("pub 主机: String"), "{字段产物}");
    assert!(字段产物.contains("pub 状态: String"), "{字段产物}");

    let 版本产物 = std::fs::read_to_string(源码.join("版本.rs")).expect("版本.rs 应已导出");
    assert!(版本产物.contains("pub fn 程序版本()"), "{版本产物}");

    // 多层目录：子模块注解相对当前目录带自身词干前缀（rustc 默认查找规则）
    let 领域产物 = std::fs::read_to_string(源码.join("领域.rs")).expect("领域.rs 应已导出");
    assert!(
        领域产物.contains("#[path = \"领域/工具.rs\"]") && 领域产物.contains("pub mod 工具;"),
        "{领域产物}"
    );
    assert!(源码.join("领域/工具.rs").is_file(), "嵌套模块应已导出");
}

/// 混合语言目录（`.zh` + `.ja`）：按扩展名分组、每组用自身语言包转译，
/// 否则日文关键字（関数/主関数）会被中文包原样保留在产物里
#[test]
fn 测试_导出混合语言目录按各自语言包() {
    let 目录 = tempfile::tempdir().expect("创建临时目录失败");
    std::fs::write(目录.path().join("main.zh"), "函数 主函数() {\n}\n").unwrap();
    std::fs::write(目录.path().join("tool.ja"), "関数 主関数() {\n}\n").unwrap();

    命令构造器()
        .args(["eject"])
        .arg(目录.path())
        .assert()
        .success();

    let 主程序产物 =
        std::fs::read_to_string(目录.path().join("main.rs")).expect("main.rs 应已导出");
    let 工具产物 = std::fs::read_to_string(目录.path().join("tool.rs")).expect("tool.rs 应已导出");
    assert!(主程序产物.contains("fn main()"), "{主程序产物}");
    assert!(工具产物.contains("fn main()"), "{工具产物}");
    assert!(
        !工具产物.contains("関数"),
        "ja 文件必须用 ja 语言包转译：{工具产物}"
    );
}

/// `rzc eject <空目录>`：无方言源文件时干净报错，不恐慌
#[test]
fn 测试_导出空目录干净失败() {
    let 目录 = tempfile::tempdir().expect("创建临时目录失败");
    命令构造器()
        .args(["eject"])
        .arg(目录.path())
        .assert()
        .failure()
        .stderr(predicates::str::starts_with("错误："));
}

/// `rzc doctor`：组件体检始终退出 0 并输出工具链/语言包分区
#[test]
fn 测试_体检输出健康报告() {
    let 主目录 = 隔离主目录();
    命令构造器()
        .env("HOME", 主目录.path())
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
/// 「提前看得见」（判定与 rzc 运行/检查的阻断同源）
#[test]
fn 测试_体检报告项目缺少构建入口() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(工作目录.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        工作目录.path().join("Cargo.toml"),
        "[package]\nname=\"doccheck\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(工作目录.path().join("src/main.zh"), 好程序).unwrap();

    let 输出 = 命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(工作目录.path())
        .arg("doctor")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let 输出文本 = String::from_utf8_lossy(&输出).to_string();
    assert!(
        输出文本.contains("包管理器读不到任何构建目标"),
        "{输出文本}"
    );
    assert!(
        输出文本.contains("rzc check src/main.zh"),
        "须给出能走通的命令：{输出文本}"
    );
    assert!(
        输出文本.contains("no targets specified in the manifest"),
        "应写明直接 cargo build 会撞的那句原话：{输出文本}"
    );

    // 生成入口产物后自检转绿：不得留下长期误报。
    // 走 `rzc check`（`rzc transpile` 只打印英文、不落产物）；不断言其退出码——
    // 入口产物在调用 cargo 之前就已写好，沙箱里没装工具链时 cargo 会失败，
    // 但自检只看「Cargo 读不读得到构建目标」，只看产物
    命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(工作目录.path())
        .args(["check", "src/main.zh"])
        .output()
        .expect("运行 rzc check 失败");
    let 输出文本 = String::from_utf8_lossy(
        &命令构造器()
            .env("HOME", 主目录.path())
            .env_remove("RZ_LOG")
            .current_dir(工作目录.path())
            .arg("doctor")
            .assert()
            .success()
            .get_output()
            .stdout,
    )
    .to_string();
    assert!(
        输出文本.contains("包管理器可读入口: 正常"),
        "产物齐备后应转绿：{输出文本}"
    );
}

/// `rzc mapping check`：对内置 zh 语言包跑冲突检测，基线干净时退出 0
#[test]
fn 测试_映射检查干净语言包() {
    let 主目录 = 隔离主目录();
    let 语言包目录 = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../engine/lang-packs/zh");
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["mapping", "check"])
        .arg(&语言包目录)
        .assert()
        .success();
}

/// 输入文件不存在：以非零退出码与母语错误行失败，而非恐慌堆栈
#[test]
fn 测试_缺失源文件干净失败() {
    let 主目录 = 隔离主目录();
    let 缺失路径 = std::env::temp_dir().join(format!("rzc-missing-{}.zh", std::process::id()));
    let 输出结果 = 命令构造器()
        .env("HOME", 主目录.path())
        .arg("transpile")
        .arg(&缺失路径)
        .assert()
        .failure();
    let 错误输出 = String::from_utf8_lossy(&输出结果.get_output().stderr).to_string();
    assert!(错误输出.starts_with("错误："), "{错误输出}");
    assert!(
        错误输出.contains("不存在"),
        "应给出母语「不存在」提示：{错误输出}"
    );
    assert!(
        !错误输出.to_lowercase().contains("os error"),
        "不应暴露 OS 错误：{错误输出}"
    );
}

/// `rzc eject <不存在的路径>`：给出「不存在」提示，而不是无关的
/// 「无法识别源文件扩展名」（与运行/检查/转译子命令的缺失校验同口径）
#[test]
fn 测试_导出缺失路径失败并给提示() {
    let 主目录 = 隔离主目录();
    let 缺失路径 =
        std::env::temp_dir().join(format!("rzc-eject-missing-{}.zh", std::process::id()));
    let 输出结果 = 命令构造器()
        .env("HOME", 主目录.path())
        .args(["eject"])
        .arg(&缺失路径)
        .assert()
        .failure();
    let 错误输出 = String::from_utf8_lossy(&输出结果.get_output().stderr).to_string();
    assert!(错误输出.starts_with("错误："), "{错误输出}");
    assert!(错误输出.contains("不存在"), "应提示路径不存在：{错误输出}");
    assert!(
        !错误输出.contains("无法识别源文件扩展名"),
        "不应报无关的扩展名错误：{错误输出}"
    );
}

/// `rzc eject <a.zh> <b.zh>`：一次传多个目标批量导出（复用单目标逻辑）
///
/// 逐个手动导出容易遗漏，批量模式必须：成功项逐个提示、末尾输出汇总、
/// 退出码为 0。
#[test]
fn 测试_导出批量多目标() {
    let 目录 = tempfile::tempdir().expect("创建临时目录失败");
    let 主程序路径 = 目录.path().join("main.zh");
    let 工具路径 = 目录.path().join("工具.zh");
    std::fs::write(&主程序路径, 好程序).unwrap();
    std::fs::write(&工具路径, "公开 函数 问候() {\n}\n").unwrap();

    let 输出结果 = 命令构造器()
        .args(["eject"])
        .arg(&主程序路径)
        .arg(&工具路径)
        .assert()
        .success();
    let 标准输出文本 = String::from_utf8_lossy(&输出结果.get_output().stdout).to_string();
    assert!(
        标准输出文本.contains("main.rs"),
        "应提示 main.rs 已导出：{标准输出文本}"
    );
    assert!(
        标准输出文本.contains("工具.rs"),
        "应提示 工具.rs 已导出：{标准输出文本}"
    );
    assert_eq!(
        标准输出文本.matches("已导出到").count(),
        2,
        "每个目标都应单独提示：{标准输出文本}"
    );
    assert!(
        标准输出文本.contains("共导出 2 个目标"),
        "应有成功汇总：{标准输出文本}"
    );
    assert!(目录.path().join("main.rs").is_file(), "main.rs 应已导出");
    assert!(目录.path().join("工具.rs").is_file(), "工具.rs 应已导出");
}

/// `rzc eject` 批量部分失败：成功项照常导出，失败项单独汇总，退出码非零
///
/// 不能因一个目标失败就中断其余目标（那反而更容易漏导出），
/// 也不能以 0 退出（否则 CI 会放过遗漏）。
#[test]
fn 测试_导出批量部分失败退出码非零() {
    let 目录 = tempfile::tempdir().expect("创建临时目录失败");
    let 存在路径 = 目录.path().join("main.zh");
    std::fs::write(&存在路径, 好程序).unwrap();
    let 缺失路径 = 目录.path().join("缺失.zh");

    let 输出结果 = 命令构造器()
        .args(["eject"])
        .arg(&存在路径)
        .arg(&缺失路径)
        .assert()
        .failure();
    let 标准输出文本 = String::from_utf8_lossy(&输出结果.get_output().stdout).to_string();
    let 错误输出 = String::from_utf8_lossy(&输出结果.get_output().stderr).to_string();
    assert!(
        标准输出文本.contains("main.rs"),
        "成功项应照常导出：{标准输出文本}"
    );
    assert!(错误输出.contains("导出失败"), "应列出失败项：{错误输出}");
    assert!(
        错误输出.contains("缺失.zh"),
        "失败项应带目标路径：{错误输出}"
    );
    assert!(
        错误输出.contains("成功 1 个，失败 1 个"),
        "应有成功/失败汇总：{错误输出}"
    );
    assert!(
        目录.path().join("main.rs").is_file(),
        "部分失败不应影响成功项的产物"
    );
}

/// `rzc eject` 重复目标：参数校验直接报错（避免同一文件被写两次）
#[test]
fn 测试_导出重复目标被拒绝() {
    let 目录 = tempfile::tempdir().expect("创建临时目录失败");
    let 主程序路径 = 目录.path().join("main.zh");
    std::fs::write(&主程序路径, 好程序).unwrap();

    let 输出结果 = 命令构造器()
        .args(["eject"])
        .arg(&主程序路径)
        .arg(&主程序路径)
        .assert()
        .failure();
    let 错误输出 = String::from_utf8_lossy(&输出结果.get_output().stderr).to_string();
    assert!(
        错误输出.contains("重复"),
        "应在参数校验阶段报重复目标：{错误输出}"
    );
}

/// `rzc eject` 导出前冲突扫描：字段名/声明名命中词条、会恐慌的词条被当文本用
///
/// 三类静默改语义的位置必须在导出前报出来（按 `.zh` 行列），且**只提示不阻断**：
/// 产物照常写出、退出码为 0；`--no-lint` 一并关闭（与教学风格检查同口径）。
#[test]
fn 测试_导出报告冲突提示() {
    let 目录 = tempfile::tempdir().expect("创建临时目录失败");
    let 中文源码 = 目录.path().join("main.zh");
    // 原始字符串：源码本身含 「"」 与换行，避免层层转义
    std::fs::write(
        &中文源码,
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

    let 输出结果 = 命令构造器()
        .args(["eject"])
        .arg(&中文源码)
        .assert()
        .success();
    let 错误输出 = String::from_utf8_lossy(&输出结果.get_output().stderr).to_string();
    assert!(
        错误输出.contains("[eject_hint]"),
        "应有冲突提示：{错误输出}"
    );
    assert!(
        错误输出.contains("第 2 行第 5 列") && 错误输出.contains("类型"),
        "字段名命中词条应报行列：{错误输出}"
    );
    assert!(
        错误输出.contains("第 6 行第 7 列") && 错误输出.contains("自身"),
        "声明名命中词条应报行列：{错误输出}"
    );
    assert!(
        错误输出.contains("解包") && 错误输出.contains("panic"),
        "panic 被当文本用应报出来：{错误输出}"
    );
    assert!(
        错误输出.matches("[eject_hint]").count() == 3,
        "三类位置各报一次：{错误输出}"
    );
    assert!(目录.path().join("main.rs").is_file(), "提示不得阻断导出");

    // `--no-lint`：教学提示一并关闭，导出行为不变
    let 输出结果 = 命令构造器()
        .args(["--no-lint", "eject"])
        .arg(&中文源码)
        .assert()
        .success();
    let 错误输出 = String::from_utf8_lossy(&输出结果.get_output().stderr).to_string();
    assert!(
        !错误输出.contains("[eject_hint]"),
        "--no-lint 应关闭冲突提示：{错误输出}"
    );
}

/// 运行/检查/转译子命令传目录：给出可操作提示（P-02 同类面统一收口），
/// 不得暴露操作系统原始错误（Windows 报「拒绝访问」并带系统错误码）
#[test]
fn 测试_目录输入失败给可操作提示() {
    let 主目录 = 隔离主目录();
    let 目录 = tempfile::tempdir().expect("创建临时目录失败");
    for 子命令 in ["run", "check", "transpile"] {
        let 输出结果 = 命令构造器()
            .env("HOME", 主目录.path())
            .arg(子命令)
            .arg(目录.path())
            .assert()
            .failure();
        let 错误输出 = String::from_utf8_lossy(&输出结果.get_output().stderr).to_string();
        assert!(
            错误输出.starts_with("错误："),
            "{子命令} 应以 错误：开头：{错误输出}"
        );
        assert!(
            错误输出.contains("rzc eject"),
            "{子命令} 应把目录导出指向 eject：{错误输出}"
        );
        assert!(
            !错误输出.to_lowercase().contains("os error"),
            "{子命令} 不应暴露 OS 错误：{错误输出}"
        );
    }
}

/// `rzc mapping check`（无参数）：全内置语言校验 + 跨语言条目对比 +
/// 跨语言完整性门禁，基线干净时退出 0
#[test]
fn 测试_映射检查全部内置与跨语言() {
    let 主目录 = 隔离主目录();
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["mapping", "check"])
        .assert()
        .success();
}

/// `rzc mapping check <未知目标>`：非目录非内置代码 → 非零退出 + 母语错误行
#[test]
fn 测试_映射检查未知目标失败() {
    let 主目录 = 隔离主目录();
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["mapping", "check", "no-such-lang-xyz"])
        .assert()
        .failure()
        .stderr(predicates::str::starts_with("错误："));
}

/// `rzc mapping check <坏语言包目录>`：语言包清单 TOML 重复键 → 校验失败，
/// 报告渲染出问题文件名（问题报告的本地化链路）
#[test]
fn 测试_映射检查损坏外部语言包失败() {
    let 主目录 = 隔离主目录();
    let 语言包 = tempfile::tempdir().expect("创建语言包目录失败");
    std::fs::write(
        语言包.path().join("keywords.toml"),
        "[\"声明\"]\n\"函数\" = \"fn\"\n",
    )
    .unwrap();
    let 映射目录 = 语言包.path().join("crates");
    std::fs::create_dir_all(&映射目录).unwrap();
    std::fs::write(
        映射目录.join("broken.toml"),
        "[\"标识符\"]\n\"键\" = \"a\"\n\"键\" = \"b\"\n",
    )
    .unwrap();
    let 输出结果 = 命令构造器()
        .env("HOME", 主目录.path())
        .args(["mapping", "check"])
        .arg(语言包.path())
        .assert()
        .failure();
    let 合并输出 = format!(
        "{}{}",
        String::from_utf8_lossy(&输出结果.get_output().stdout),
        String::from_utf8_lossy(&输出结果.get_output().stderr)
    );
    assert!(
        合并输出.contains("broken.toml"),
        "报告应点名问题文件：{合并输出}"
    );
}

/// 本地语言包生命周期：从目录安装 → 列表可见 → 删除 → 再删报错；
/// 非法语言代码（路径穿越）被拒绝。全程离线
#[test]
fn 测试_本地语言包安装列出删除生命周期() {
    let 主目录 = 隔离主目录();
    let 暂存目录 = tempfile::tempdir().expect("创建暂存目录失败");
    let 语言包 = 暂存目录.path().join("zz");
    std::fs::create_dir_all(&语言包).unwrap();
    std::fs::write(
        语言包.join("keywords.toml"),
        "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n",
    )
    .unwrap();

    // 非法目录（无 keywords.toml）安装失败
    let 非法包 = 暂存目录.path().join("yy");
    std::fs::create_dir_all(&非法包).unwrap();
    私有主目录命令(主目录.path())
        .args(["lang", "install"])
        .arg(&非法包)
        .assert()
        .failure()
        .stderr(predicates::str::starts_with("错误："));

    // 合法目录安装成功
    私有主目录命令(主目录.path())
        .args(["lang", "install"])
        .arg(&语言包)
        .assert()
        .success();
    // 列表中可见用户安装标记
    私有主目录命令(主目录.path())
        .args(["lang", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("zz"));
    // 重复安装（无 `--force`）失败
    私有主目录命令(主目录.path())
        .args(["lang", "install"])
        .arg(&语言包)
        .assert()
        .failure();
    // `--force` 重装成功
    私有主目录命令(主目录.path())
        .args(["lang", "install", "--force"])
        .arg(&语言包)
        .assert()
        .success();
    // 删除成功
    私有主目录命令(主目录.path())
        .args(["lang", "remove", "zz"])
        .assert()
        .success();
    // 再删：已不存在 → 失败
    私有主目录命令(主目录.path())
        .args(["lang", "remove", "zz"])
        .assert()
        .failure();
    // 路径穿越代码被拒绝
    私有主目录命令(主目录.path())
        .args(["lang", "remove", "../evil"])
        .assert()
        .failure();
}

/// `rzc mapping scaffold`（规则提供方）：纯本地生成待办骨架，无需网络；
/// 未知源语言报错
#[test]
fn 测试_映射脚手架规则提供方本地生成() {
    let 主目录 = 隔离主目录();
    let 输出 = tempfile::tempdir().expect("创建输出目录失败");
    命令构造器()
        .env("HOME", 主目录.path())
        .args([
            "mapping",
            "scaffold",
            "zh",
            "xx",
            "--provider",
            "rule",
            "--output",
        ])
        .arg(输出.path())
        .assert()
        .success();
    // 输出目录应至少生成一个带待办注释的语言包清单 TOML
    let mut 找到待办 = false;
    for 条目 in 递归列出文件(输出.path()) {
        if 条目.extension().and_then(|扩展| 扩展.to_str()) == Some("toml")
            && std::fs::read_to_string(&条目)
                .map(|文件内容| 文件内容.contains("TODO(xx)"))
                .unwrap_or(false)
        {
            找到待办 = true;
            break;
        }
    }
    assert!(
        找到待办,
        "应在 {} 下生成带 TODO(xx) 的骨架",
        输出.path().display()
    );
    // 未知源语言：失败
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["mapping", "scaffold", "no-such-lang", "xx", "--output"])
        .arg(输出.path())
        .assert()
        .failure()
        .stderr(predicates::str::starts_with("错误："));
}

/// AI 模式的失败判定必须发生在写文件之前：拼错的 `--provider` 与缺失的密钥
/// 都不该留下一堆待翻译文件；`--provider openai` 需被识别为 AI 链路，而不是
/// 静默按规则跑完（可选值与变量名在任何界面语言下都是英文字面）
#[test]
fn 测试_映射脚手架智能校验先于写入() {
    let 主目录 = 隔离主目录();

    // 未知服务商：失败 + 列出可选值 + 不产生任何文件
    let 输出 = tempfile::tempdir().expect("创建输出目录失败");
    命令构造器()
        .env("HOME", 主目录.path())
        .args([
            "mapping",
            "scaffold",
            "zh",
            "xx",
            "--provider",
            "zzz",
            "--output",
        ])
        .arg(输出.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "deepseek / openai / compatible / rule",
        ));
    assert!(
        递归列出文件(输出.path()).is_empty(),
        "未知服务商不应写入任何文件"
    );

    // openai 别名走 AI 链路：无密钥时失败并点名 RZ_AI_API_KEY，同样不落文件
    let 输出 = tempfile::tempdir().expect("创建输出目录失败");
    let mut 命令 = 命令构造器();
    命令
        .env("HOME", 主目录.path())
        .args([
            "mapping",
            "scaffold",
            "zh",
            "xx",
            "--provider",
            "openai",
            "--output",
        ])
        .arg(输出.path());
    for 密钥 in ["DEEPSEEK_API_KEY", "RZ_AI_API_KEY", "OPENAI_API_KEY"] {
        命令.env_remove(密钥);
    }
    命令
        .assert()
        .failure()
        .stderr(predicates::str::contains("RZ_AI_API_KEY"));
    assert!(
        递归列出文件(输出.path()).is_empty(),
        "缺少密钥不应写入任何文件"
    );
}

/// 递归列出目录内文件（测试小工具）
fn 递归列出文件(根目录: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut 输出 = Vec::new();
    if let Ok(条目集) = std::fs::read_dir(根目录) {
        for 条目 in 条目集.flatten() {
            let 路径值 = 条目.path();
            if 路径值.is_dir() {
                输出.extend(递归列出文件(&路径值));
            } else {
                输出.push(路径值);
            }
        }
    }
    输出
}

/// `rzc mapping coverage zh`：对后端真实源码跑语料覆盖矩阵，
/// 需在仓库目录内执行（向上找工作区根）
#[test]
fn 测试_映射覆盖率跑真实后端语料() {
    let 主目录 = 隔离主目录();
    // 向上找根时命中第一个含 Cargo.toml 的目录，故必须直接以工作区根为当前目录
    //（从 crates/cli 起找会停在库目录自身，语料目录为空）
    let 工作区根 = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|路径| 路径.parent())
        .expect("应能定位工作区根")
        .to_path_buf();
    命令构造器()
        .env("HOME", 主目录.path())
        .current_dir(&工作区根)
        .args(["mapping", "coverage", "--lang", "zh"])
        .assert()
        .success()
        .stdout(predicates::str::contains("语料"))
        .stdout(predicates::str::contains("覆盖通过"));
}

/// `rzc relocalize` 子命令端到端：zh 工程 → 反译重生为 ja 方言工程（可再转译回 `fn main`）
///
/// 验证镜像写出、清单复制、AGENTS.md 生成，以及入口名取核心关键字（`主関数`）
/// 而非第三方库别名——保证产物 `rzc transpile` 回标准 Rust 时得到 `fn main`。
#[test]
fn 测试_本地化中文工程到日文方言() {
    let 目录 = tempfile::tempdir().expect("创建临时目录失败");
    let 源码目录 = 目录.path().join("src");
    std::fs::create_dir_all(&源码目录).expect("创建 src 失败");
    std::fs::write(
        目录.path().join("Cargo.toml"),
        "[package]\nname = \"rlz\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n\n[workspace]\n",
    )
    .expect("写入 Cargo.toml 失败");
    std::fs::write(
        源码目录.join("main.zh"),
        "函数 主函数() {\n    打印行!(\"你好\");\n}\n",
    )
    .expect("写入 main.zh 失败");

    命令构造器()
        .args(["relocalize", "--to", "ja"])
        .arg(目录.path())
        .assert()
        .success();

    let 输出主程序 = 目录.path().join("relocalized.ja/src/main.ja");
    assert!(输出主程序.is_file(), "应镜像写出 {}", 输出主程序.display());
    let 日文输出 = std::fs::read_to_string(&输出主程序).expect("读取 main.ja 失败");
    assert!(日文输出.contains("関数"), "产物应含 ja `関数`: {日文输出}");
    assert!(日文输出.contains("主関数"), "入口应为 `主関数`: {日文输出}");
    assert!(
        目录.path().join("relocalized.ja/Cargo.toml").is_file(),
        "应复制 Cargo.toml"
    );
    assert!(
        目录.path().join("relocalized.ja/AGENTS.md").is_file(),
        "应生成 AGENTS.md"
    );

    // 反译产物再正向转译应回到 `fn main`（可编译性探针）
    命令构造器()
        .arg("transpile")
        .arg(&输出主程序)
        .assert()
        .success()
        .stdout(predicates::str::contains("fn main"));
}

/// `rzc relocalize` 缺 `--to` 时报错退出（必填参数）
#[test]
fn 测试_本地化缺目标语言报错() {
    命令构造器()
        .args(["relocalize", "/tmp"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("--to"));
}
