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
/// 统一把 HOME / USERPROFILE 指向空沙箱，语言包目录随 `$HOME/.rz/lang-packs`
/// 默认解析而一同隔离；结果仅取决于编译期嵌入的内置语言包与源码本身。
///
/// 切勿在此统一设置进程共享的 `RZ_LANG_DIR`：安装类测试（如
/// `语言本地装卸删生命周期`）会覆盖 HOME 却继承该共享目录，与并行
/// 读取它的 `lang list` 测试竞态，导致「11」计数间歇性失真为「12」。
/// 让语言包目录跟随各自的 HOME 解析，安装测试自然隔离进自己的临时主目录。
fn 沙箱根() -> &'static PathBuf {
    static 根: OnceLock<PathBuf> = OnceLock::new();
    根.get_or_init(|| {
        let 临时目录 =
            std::env::temp_dir().join(format!("rzc-test-sandbox-{}", std::process::id()));
        std::fs::create_dir_all(&临时目录).expect("创建测试沙箱目录失败");
        临时目录
    })
}

fn 命令构造器() -> Command {
    let mut 命令 = Command::cargo_bin("rzc").expect("应能定位 rzc 二进制");
    let 沙箱 = 沙箱根();
    命令
        .env("HOME", 沙箱)
        .env("USERPROFILE", 沙箱)
        .env_remove("RZ_LOG");
    命令
}

/// `--version` 输出 `rzc <版本>`，且与 Cargo.toml 声明一致
#[test]
fn 版本命令报告名称与版本() {
    命令构造器()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains("rzc"))
        .stdout(predicates::str::contains(env!("CARGO_PKG_VERSION")));
}

/// `lang list` 无需网络：本分支内置中文单包编译期嵌入；沙箱内无用户安装包，
/// 计数稳定为 1（不随开发者机器上的 `~/.rz/lang-packs` 变化）
#[test]
fn 语言列表列出内置包() {
    命令构造器()
        .args(["lang", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("共 1 个"))
        .stdout(predicates::str::contains("zh"));
}

/// `transpile` 纯内存转译：母语关键字映射为标准 Rust，不产生文件
#[test]
fn 转译输出标准产物() {
    let 临时目录 = tempfile::tempdir().expect("创建临时目录失败");
    let 源文件 = 临时目录.path().join("main.zh");
    std::fs::write(&源文件, "函数 主函数() {\n    打印行!(\"你好\");\n}\n")
        .expect("写入测试源码失败");

    命令构造器()
        .arg("transpile")
        .arg(&源文件)
        .assert()
        .success()
        .stdout(predicates::str::contains("fn main()"))
        .stdout(predicates::str::contains("println!"));
}

/// `rzc cheat --lang zh`：输出母语↔Rust 速查表（关键字节含 函数→fn）
#[test]
fn 速查中文输出映射表() {
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
fn 速查输出表格格式() {
    命令构造器()
        .args(["cheat", "zh", "--markdown"])
        .assert()
        .success()
        .stdout(predicates::str::contains("| zh | Rust |"))
        .stdout(predicates::str::contains("| 函数 | `fn` |"));
}

/// `rzc init --lang` 缺省取系统 locale 命中的内置语言：本分支仅内置 zh，
/// 即便 de_DE 环境也回退缺省 zh
#[test]
fn 初始化默认语言回退中文() {
    命令构造器()
        .args(["init", "--help"])
        .env("LC_ALL", "de_DE.UTF-8")
        .env("LANG", "de_DE.UTF-8")
        .assert()
        .success()
        .stdout(predicates::str::contains("[default: zh]"));
}

/// 回归（#7）：`--no-lint` 静默教学 lint 提示
///
/// 项目开发（非教学）场景中，未标注类型等初学者提示每次转译刷屏；
/// 默认输出（stderr 含 `[lint]` 标签）与关闭后输出（stderr 空）对比验证。
/// HOME 隔离到临时目录：缓存（~/.rz）不跨测试/机器残留，内容各自
/// 不同强制缓存未命中（命中会跳过管线而不产生任何告警，断言失真）。
#[test]
fn 关闭检查静默教学提示() {
    let 临时目录 = tempfile::tempdir().expect("创建临时目录失败");
    let 官方文件 = 临时目录.path().join("official.zh");
    let 静默文件 = 临时目录.path().join("silenced.zh");
    // 两个文件内容不同（不同变量名）→ 两次都是缓存未命中，管线真实执行
    std::fs::write(
        &官方文件,
        "函数 主函数() {\n    让 数量甲 = 5;\n    打印行!(\"{}\", 数量甲);\n}\n",
    )
    .expect("写入测试源码失败");
    std::fs::write(
        &静默文件,
        "函数 主函数() {\n    让 数量乙 = 6;\n    打印行!(\"{}\", 数量乙);\n}\n",
    )
    .expect("写入测试源码失败");

    // 默认：教学 lint 告警输出到 stderr（含 [lint] 标签）
    命令构造器()
        .env("HOME", 临时目录.path())
        .env_remove("RZ_LOG")
        .arg("transpile")
        .arg(&官方文件)
        .assert()
        .success()
        .stderr(predicates::str::contains("[lint]"));

    // --no-lint：转译输出不变，教学 lint 告警静默（stderr 无输出）
    命令构造器()
        .env("HOME", 临时目录.path())
        .env_remove("RZ_LOG")
        .args(["--no-lint", "transpile"])
        .arg(&静默文件)
        .assert()
        .success()
        .stdout(predicates::str::contains("let 数量乙 = 6;"))
        .stderr(predicates::str::is_empty());
}

// ========== 进程级子命令链路（真实 rustc / 文件系统，隔离 HOME） ==========

/// 新建隔离 HOME 的临时目录（~/.rz 缓存不跨测试污染）
fn 隔离主目录() -> tempfile::TempDir {
    tempfile::tempdir().expect("创建 HOME 临时目录失败")
}

/// 好程序：编译运行必过、无警告
const 好程序: &str = "函数 主函数() {\n    打印行!(\"输出：{}\", 42);\n}\n";

/// 坏程序：E0384（不可变变量重复赋值），CLI 须翻译成母语教学诊断
const 坏程序: &str =
    "函数 主函数() {\n    让 数量 = 10;\n    数量 = 数量 + 1;\n    打印行!(\"{}\", 数量);\n}\n";

/// `rzc init` 生成完整项目骨架
#[test]
fn 初始化创建工程骨架() {
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
        工作目录.path().join("demo/AGENTS.md").is_file(),
        "应生成 AGENTS.md（随项目分发的 AI 中文方言编码规则）"
    );
    // 内容非空且含指引正文：若漏加 agents_md_template 这个 ui 键，取文 会回退成键名字面量
    let 代理规则 = std::fs::read_to_string(工作目录.path().join("demo/AGENTS.md"))
        .expect("读取 AGENTS.md 失败");
    assert!(
        代理规则.contains("中文") && 代理规则.contains("rzc run"),
        "AGENTS.md 应含方言编码指引正文，实得开头：{}",
        代理规则
    );
}

/// `rzc run`：无依赖教学项目走 rustc 直调路径，程序 stdout 原样透传
#[test]
fn 运行执行转译后程序() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    // init 骨架后覆盖为可断言输出的程序
    命令构造器()
        .env("HOME", 主目录.path())
        .current_dir(工作目录.path())
        .args(["init", "run-demo", "--lang", "zh"])
        .assert()
        .success();
    let 方言入口 = 工作目录.path().join("run-demo/src/main.zh");
    std::fs::write(&方言入口, 好程序).expect("写入 main.zh 失败");

    命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(工作目录.path().join("run-demo"))
        .args(["run", "src/main.zh"])
        .assert()
        .success()
        .stdout(predicates::str::contains("输出：42"));
}

/// `rzc check` 好程序：退出码 0
#[test]
fn 检查好程序通过() {
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

/// `rzc check` 多层模块项目：递归收集 + 非入口文件按层级补 `#[path]`
///
/// 布局：src/main.zh → src/领域.zh → src/领域/工具.zh。此前非入口方言文件
/// 内的文件式 `mod` 声明没有层级路径注解，rustc 报 E0754/E0583/E0603。
#[test]
fn 检查嵌套模块工程通过() {
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

    // 产物原位写回且嵌套路径注解指向「父词干/子词干.rs」
    let 领域产物 =
        std::fs::read_to_string(工作目录.path().join("src/领域.rs")).expect("领域.rs 应已转译");
    assert!(
        领域产物.contains("#[path") && 领域产物.contains("领域/工具.rs"),
        "非入口文件应按层级注解嵌套子模块：{领域产物}"
    );
    assert!(
        领域产物.contains("pub mod 工具"),
        "嵌套子模块应保持 pub（防 E0603）：{领域产物}"
    );
    let 入口产物 =
        std::fs::read_to_string(工作目录.path().join("src/main.rs")).expect("main.rs 应存在");
    assert!(
        入口产物.contains("#[path") && 入口产物.contains("领域.rs"),
        "入口应注解顶层模块路径：{入口产物}"
    );
}

/// `rzc check` 坏程序：退出码非 0，诊断为母语（E0425 等错误码 + 中文消息 +
/// 💡 教学提示），不泄漏 rustc 英文原文
#[test]
fn 检查坏程序产出译文诊断() {
    let 主目录 = 隔离主目录();
    let 工作目录 = tempfile::tempdir().expect("创建工作目录失败");
    std::fs::create_dir_all(工作目录.path().join("src")).expect("创建 src 失败");
    std::fs::write(
        工作目录.path().join("Cargo.toml"),
        "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .unwrap();
    std::fs::write(工作目录.path().join("src/main.zh"), 坏程序).unwrap();
    let 执行输出 = 命令构造器()
        .env("HOME", 主目录.path())
        .env_remove("RZ_LOG")
        .current_dir(工作目录.path())
        .args(["check", "src/main.zh"])
        .assert()
        .failure();
    let 合并输出 = format!(
        "{}{}",
        String::from_utf8_lossy(&执行输出.get_output().stdout),
        String::from_utf8_lossy(&执行输出.get_output().stderr)
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
fn 反编译写出标准产物文件() {
    let 主目录 = 隔离主目录();
    let 临时目录 = tempfile::tempdir().expect("创建临时目录失败");
    let 方言文件 = 临时目录.path().join("main.zh");
    std::fs::write(&方言文件, 好程序).unwrap();
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["eject"])
        .arg(&方言文件)
        .assert()
        .success()
        .stdout(predicates::str::contains("main.rs"));
    let 产物文件 =
        std::fs::read_to_string(临时目录.path().join("main.rs")).expect("main.rs 应已导出");
    assert!(
        产物文件.contains("fn main()"),
        "导出应为标准 Rust：{产物文件}"
    );
    assert!(
        产物文件.contains("println!"),
        "宏应还原为标准 Rust：{产物文件}"
    );
}

/// `rzc doctor`：组件体检始终退出 0 并输出工具链/语言包分区
#[test]
fn 诊断命令打印健康报告() {
    let 主目录 = 隔离主目录();
    命令构造器()
        .env("HOME", 主目录.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicates::str::contains("rzc doctor"))
        .stdout(predicates::str::contains("语言包"));
}

/// `rzc mapping check`：对内置 zh 语言包跑冲突检测，基线干净时退出 0
#[test]
fn 映射检查干净语言包() {
    let 主目录 = 隔离主目录();
    let 语言包路径 = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../engine/lang-packs/zh");
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["mapping", "check"])
        .arg(&语言包路径)
        .assert()
        .success();
}

/// 输入文件不存在：以非零退出码与 Error 行失败，而非 panic 堆栈
#[test]
fn 缺失源文件干净失败() {
    let 主目录 = 隔离主目录();
    let 缺失文件 = std::env::temp_dir().join(format!("rzc-missing-{}.zh", std::process::id()));
    命令构造器()
        .env("HOME", 主目录.path())
        .arg("transpile")
        .arg(&缺失文件)
        .assert()
        .failure()
        .stderr(predicates::str::starts_with("Error:"));
}

/// `rzc mapping check`（无参数）：全内置语言校验 + 跨语言条目对比 +
/// 跨语言完整性门禁，基线干净时退出 0
#[test]
fn 映射检查全部内置跨语言() {
    let 主目录 = 隔离主目录();
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["mapping", "check"])
        .assert()
        .success();
}

/// `rzc mapping check <未知目标>`：非目录非内置代码 → 非零退出 + Error
#[test]
fn 映射检查未知目标失败() {
    let 主目录 = 隔离主目录();
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["mapping", "check", "no-such-lang-xyz"])
        .assert()
        .failure()
        .stderr(predicates::str::starts_with("Error:"));
}

/// `rzc mapping check <坏语言包目录>`：crates TOML 重复键 → 校验失败，
/// 报告渲染出问题文件名（render_issue 本地化链路）
#[test]
fn 映射检查损坏外包失败() {
    let 主目录 = 隔离主目录();
    let 语言包目录 = tempfile::tempdir().expect("创建语言包目录失败");
    std::fs::write(
        语言包目录.path().join("keywords.toml"),
        "[\"声明\"]\n\"函数\" = \"fn\"\n",
    )
    .unwrap();
    let 三方目录 = 语言包目录.path().join("crates");
    std::fs::create_dir_all(&三方目录).unwrap();
    std::fs::write(
        三方目录.join("broken.toml"),
        "[\"标识符\"]\n\"键\" = \"a\"\n\"键\" = \"b\"\n",
    )
    .unwrap();
    let 执行输出 = 命令构造器()
        .env("HOME", 主目录.path())
        .args(["mapping", "check"])
        .arg(语言包目录.path())
        .assert()
        .failure();
    let 合并输出 = format!(
        "{}{}",
        String::from_utf8_lossy(&执行输出.get_output().stdout),
        String::from_utf8_lossy(&执行输出.get_output().stderr)
    );
    assert!(
        合并输出.contains("broken.toml"),
        "报告应点名问题文件：{合并输出}"
    );
}

/// 本地语言包生命周期：从目录安装 → list 可见 → 删除 → 再删报错；
/// 非法语言代码（路径穿越）被拒绝。全程离线
#[test]
fn 语言本地装卸删生命周期() {
    let 主目录 = 隔离主目录();
    let 暂存目录 = tempfile::tempdir().expect("创建暂存目录失败");
    let 语言包目录 = 暂存目录.path().join("zz");
    std::fs::create_dir_all(&语言包目录).unwrap();
    std::fs::write(
        语言包目录.join("keywords.toml"),
        "[\"声明\"]\n\"函数\" = \"fn\"\n\"让\" = \"let\"\n",
    )
    .unwrap();

    // 非法目录（无 keywords.toml）安装失败
    let 非法目录 = 暂存目录.path().join("yy");
    std::fs::create_dir_all(&非法目录).unwrap();
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["lang", "install"])
        .arg(&非法目录)
        .assert()
        .failure()
        .stderr(predicates::str::starts_with("Error:"));

    // 合法目录安装成功
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["lang", "install"])
        .arg(&语言包目录)
        .assert()
        .success();
    // list 中可见 zz（用户安装标记）
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["lang", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("zz"));
    // 重复安装（无 --force）失败
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["lang", "install"])
        .arg(&语言包目录)
        .assert()
        .failure();
    // --force 重装成功
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["lang", "install", "--force"])
        .arg(&语言包目录)
        .assert()
        .success();
    // 删除成功
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["lang", "remove", "zz"])
        .assert()
        .success();
    // 再删：已不存在 → 失败
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["lang", "remove", "zz"])
        .assert()
        .failure();
    // 路径穿越代码被拒绝
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["lang", "remove", "../evil"])
        .assert()
        .failure();
}

/// `rzc mapping scaffold`（rule 提供方）：纯本地生成 TODO 骨架，无需网络；
/// 未知源语言报错
#[test]
fn 映射脚手架规则本地() {
    let 主目录 = 隔离主目录();
    let 输出变量 = tempfile::tempdir().expect("创建输出目录失败");
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
        .arg(输出变量.path())
        .assert()
        .success();
    // 输出目录应至少生成一个带 TODO 注释的 crates TOML
    let mut 发现待办 = false;
    for 迭代项 in 递归列目录(输出变量.path()) {
        if 迭代项.extension().and_then(|扩展名| 扩展名.to_str()) == Some("toml")
            && std::fs::read_to_string(&迭代项)
                .map(|文件内容项| 文件内容项.contains("TODO(xx)"))
                .unwrap_or(false)
        {
            发现待办 = true;
            break;
        }
    }
    assert!(
        发现待办,
        "应在 {} 下生成带 TODO(xx) 的骨架",
        输出变量.path().display()
    );
    // 未知源语言：失败
    命令构造器()
        .env("HOME", 主目录.path())
        .args(["mapping", "scaffold", "no-such-lang", "xx", "--output"])
        .arg(输出变量.path())
        .assert()
        .failure()
        .stderr(predicates::str::starts_with("Error:"));
}

/// 递归列出目录内文件（测试小工具）
fn 递归列目录(扫描根: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut 输出变量 = Vec::new();
    if let Ok(目录项集) = std::fs::read_dir(扫描根) {
        for 迭代项 in 目录项集.flatten() {
            let 路径项 = 迭代项.path();
            if 路径项.is_dir() {
                输出变量.extend(递归列目录(&路径项));
            } else {
                输出变量.push(路径项);
            }
        }
    }
    输出变量
}

/// `rzc mapping coverage zh`：对后端真实源码跑语料覆盖矩阵，
/// 需在仓库目录内执行（向上找工作区根）
#[test]
fn 映射覆盖率真实语料() {
    let 主目录 = 隔离主目录();
    // 向上找根时命中第一个含 Cargo.toml 的目录，故必须直接以工作区根为 cwd
    //（从 crates/cli 起找会停在 crate 自身，语料目录为空）
    let 工作区根 = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|路径项| 路径项.parent())
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
