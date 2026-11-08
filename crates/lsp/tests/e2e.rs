//! LSP 端到端测试：真实 rust-analyzer + 母语诊断翻译全链路
//!
//! 启动真实 `i18n-rust-lsp` 二进制（Cargo 提供的二进制路径环境变量），通过 JSON-RPC 协议完成
//! 初始化 → 初始化完成通知 → 打开文档通知（含错误的 `.zh` 文件）→ 等待
//! 诊断发布通知，断言诊断消息被翻译为母语教学诊断。
//!
//! 覆盖矩阵：
//! - 4 种代表性语言（中文 / 日文 / 俄文西里尔 / 阿拉伯文从右到左）
//!   验证 E0425（未定义名称）诊断的消息翻译与教学提示；
//! - 全角标点教学诊断注入（代码位置的全角标点在 IDE 内联提示）；
//! - 教学检查诊断注入（未标注类型/魔法数字 + 忽略标记）；
//! - 教学告警通道唯一性（诊断按文件发布，服务进程标准错误不再无归属刷屏）；
//! - 多模块项目无假红语料（模块聚合 / `#[path]` 注解 / 包含资源，
//!   rust-analyzer 升级时必跑，防诊断格式漂移）。
//!
//! 前置条件：rust-analyzer 可执行文件可用（查找顺序与 `analyzer.rs` 相同：
//! RUST_ANALYZER_PATH 环境变量 → ~/.rz/toolchain/bin/rust-analyzer → PATH）。
//! 找不到时的颜色由 [`取分析器路径`] 按环境判定：本地开发环境打印
//! 原因并跳过；持续集成环境（`CI` 等环境变量）直接判红——被跳过的用例在报告里也显示为
//! 通过，不拦住就会出现「端到端全链路一条没跑、任务全绿」。持续集成中先安装工具链
//! 再显式运行：`cargo test -p i18n-rust-lsp --test e2e -- --ignored`

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// 各语言 E0425 翻译的特征词：取自 `errors.toml` [E0425]「消息模板」
/// （新版 rust-analyzer 直接以 `code: "E0425"` 上报未解析名诊断，LSP 命中
/// 错误码表渲染，而非短语替换路径）；关键词必须为该模板独有子串
const 各语言关键词: &[(&str, &str)] = &[
    ("zh", "找不到"),
    ("ja", "見つかりません"),
    ("ru", "не найдено"),
    ("ar", "غير موجود"),
];

/// 按语言码取 E0425 特征词（单一事实来源，测试函数不得再硬编码）
fn 取语言关键词<'甲>(语言码: &str) -> &'甲 str {
    各语言关键词
        .iter()
        .find(|(码, _)| *码 == 语言码)
        .unwrap_or_else(|| panic!("LANG_KEYWORDS 未配置语言：{语言码}"))
        .1
}

/// 各语言方言源码：关键字取自该语言包（日文/俄文/阿拉伯文与中文不同），
/// 标识符统一用中文（转译保留用户标识符，rust-analyzer 报错引用原名，跨语言可断言）
fn 取方言源码(语言码: &str) -> String {
    match 语言码 {
        "zh" => "函数 主函数() {\n    让 数量 = 5;\n    让 结果 = 数量 + 不存在的变量;\n}\n",
        "ja" => "関数 主関数() {\n    宣言 数量 = 5;\n    宣言 结果 = 数量 + 不存在的变量;\n}\n",
        "ru" => {
            "функция главная() {\n    пусть 数量 = 5;\n    пусть 结果 = 数量 + 不存在的变量;\n}\n"
        }
        "ar" => "دالة رئيسي() {\n    دع 数量 = 5;\n    دع 结果 = 数量 + 不存在的变量;\n}\n",
        其他 => panic!("未知语言：{其他}"),
    }
    .to_string()
}

/// rust-analyzer 查找顺序与 `analyzer.rs` 保持一致
fn 查找分析器() -> Option<PathBuf> {
    if let Ok(路径) = std::env::var("RUST_ANALYZER_PATH")
        && !路径.is_empty()
    {
        return Some(PathBuf::from(路径));
    }
    if let Some(分析器) = i18n_rust_engine::工具链::查找工具链程序("rust-analyzer") {
        return Some(分析器);
    }
    let 程序名 = if cfg!(windows) {
        "rust-analyzer.exe"
    } else {
        "rust-analyzer"
    };
    std::env::var_os("PATH").and_then(|路径变量| {
        std::env::split_paths(&路径变量)
            .map(|目录| 目录.join(程序名))
            .find(|候选| 候选.exists())
    })
}

/// JSON-RPC 分帧编码
fn 编码消息(消息: &Value) -> Vec<u8> {
    let 正文 = serde_json::to_vec(消息).expect("消息序列化失败");
    let mut 输出 = format!("Content-Length: {}\r\n\r\n", 正文.len()).into_bytes();
    输出.extend_from_slice(&正文);
    输出
}

/// 从 LSP 服务进程标准输出持续读取并解析分帧消息（线程内阻塞读）
fn 派生读取线程(标准输出文本: ChildStdout) -> mpsc::Receiver<Value> {
    let (发送端, 接收端) = mpsc::channel();
    std::thread::spawn(move || {
        let mut 读取器 = BufReader::new(标准输出文本);
        loop {
            let mut 内容长度: Option<usize> = None;
            // 逐行读分帧头，直到空行
            loop {
                let mut line = String::new();
                if 读取器.read_line(&mut line).unwrap_or(0) == 0 {
                    return; // 文件结束：子进程退出
                }
                let line = line.trim();
                if line.is_empty() {
                    break;
                }
                if let Some(余下) = line.strip_prefix("Content-Length:") {
                    内容长度 = 余下.trim().parse::<usize>().ok();
                }
            }
            let Some(长度) = 内容长度 else { continue };
            let mut 正文 = vec![0u8; 长度];
            if 读取器.read_exact(&mut 正文).is_err() {
                return;
            }
            match serde_json::from_slice::<Value>(&正文) {
                Ok(消息) => {
                    if 发送端.send(消息).is_err() {
                        return;
                    }
                }
                Err(_) => continue, // 容忍异常帧
            }
        }
    });
    接收端
}

/// 等待收到指定编号的响应（跳过通知与其他响应），超时返回无
fn 等待响应(接收端: &mpsc::Receiver<Value>, 编号: u64, 超时: Duration) -> Option<Value> {
    let 截止时间 = Instant::now() + 超时;
    loop {
        let 剩余 = 截止时间.saturating_duration_since(Instant::now());
        if 剩余.is_zero() {
            return None;
        }
        let 消息 = 接收端.recv_timeout(剩余).ok()?;
        if 消息.get("id").and_then(Value::as_u64) == Some(编号) {
            return Some(消息);
        }
    }
}

/// 收到的诊断发布通知批次统计：地址 →（批次数, 样例消息）
type 批次统计 = BTreeMap<String, (usize, String)>;

/// 记录一条诊断发布通知批次（不论地址是否为目标）
fn 记录批次(统计: &mut 批次统计, 消息: &Value) {
    if 消息.get("method").and_then(Value::as_str) != Some("textDocument/publishDiagnostics") {
        return;
    }
    let 批次地址 = 消息
        .pointer("/params/uri")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let 样例: String = 消息
        .pointer("/params/diagnostics")
        .and_then(Value::as_array)
        .and_then(|数组| 数组.first())
        .and_then(|诊断项| 诊断项.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .take(160)
        .collect();
    let 条目 = 统计.entry(批次地址).or_insert((0, String::new()));
    条目.0 += 1;
    if 条目.1.is_empty() {
        条目.1 = 样例;
    }
}

/// 转储批次统计（超时诊断：显示收到的全部地址与样例，最多 30 个）
fn 转储批次统计(统计: &批次统计, 目标地址: &str) {
    eprintln!(
        "已收到 {} 个不同 URI 的 publishDiagnostics 批次（目标 URI：{目标地址}）：",
        统计.len()
    );
    for (地址, (次数, 样例)) in 统计.iter().take(30) {
        eprintln!("  - {地址} × {次数}：{样例}");
    }
}

/// 等待目标地址的诊断发布通知：rust-analyzer 分批发诊断
/// （先发格式错误，后发名称解析错误等），因此持续收集直到
/// 诊断文本包含关键词（或超时）。空诊断通知被跳过。
///
/// 超时时转储收到的全部批次地址统计：目标地址收不到诊断时，
/// 可区分「链路从未发布」与「发布到了另一种地址形式」（Windows
/// 盘符大小写/分隔符/编码差异会导致字符串比较不相等）。
fn 等待含关键词诊断(
    接收端: &mpsc::Receiver<Value>,
    地址: &str,
    关键词: &str,
    超时: Duration,
) -> Option<Value> {
    let 截止时间 = Instant::now() + 超时;
    let mut 已见文本: Vec<String> = Vec::new();
    let mut 统计 = BTreeMap::new();
    loop {
        let 剩余 = 截止时间.saturating_duration_since(Instant::now());
        if 剩余.is_zero() {
            eprintln!("⚠️ 等待「{关键词}」超时，已收到诊断文本：");
            for 文本值 in 已见文本.iter().take(20) {
                eprintln!("  {文本值}");
            }
            转储批次统计(&统计, 地址);
            return None;
        }
        let 消息 = match 接收端.recv_timeout(剩余) {
            Ok(消息) => 消息,
            Err(_) => {
                // 通道断开或单次等待超时：若已过总时限则转储已收到的
                // 诊断文本并返回（否则继续等，保持 120s 总时限语义）
                if Instant::now() >= 截止时间 {
                    eprintln!("⚠️ 等待「{关键词}」超时，已收到诊断文本：");
                    for 文本值 in 已见文本.iter().take(20) {
                        eprintln!("  {文本值}");
                    }
                    转储批次统计(&统计, 地址);
                    return None;
                }
                continue;
            }
        };
        记录批次(&mut 统计, &消息);
        if 消息.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            && 消息.pointer("/params/uri").and_then(Value::as_str) == Some(地址)
        {
            let 诊断 = 消息
                .pointer("/params/diagnostics")
                .and_then(Value::as_array);
            let Some(诊断) = 诊断 else {
                continue;
            };
            if 诊断.is_empty() {
                continue;
            }
            let 文本集 = 诊断
                .iter()
                .filter_map(|诊断项| 诊断项.get("message").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            已见文本.push(文本集.clone());
            if 文本集.contains(关键词) {
                return Some(消息);
            }
        }
    }
}

/// 发送通知
fn 发送通知(标准输入流: &mut BufWriter<ChildStdin>, 方法: &str, 参数: Value) {
    let 消息 = json!({ "jsonrpc": "2.0", "method": 方法, "params": 参数 });
    标准输入流
        .write_all(&编码消息(&消息))
        .expect("写入通知失败");
    标准输入流.flush().expect("刷新通知失败");
}

/// 发送请求并等待响应
fn 发送请求(
    标准输入流: &mut BufWriter<ChildStdin>,
    接收端: &mpsc::Receiver<Value>,
    编号: u64,
    方法: &str,
    参数: Value,
    超时: Duration,
) -> Value {
    let 消息 = json!({ "jsonrpc": "2.0", "id": 编号, "method": 方法, "params": 参数 });
    标准输入流
        .write_all(&编码消息(&消息))
        .expect("写入请求失败");
    标准输入流.flush().expect("刷新请求失败");
    等待响应(接收端, 编号, 超时).unwrap_or_else(|| {
        panic!("等待 {方法} 响应超时（{超时:?}）");
    })
}

/// 发送请求并在等待响应期间收集 `window/showMessage` 通知
///
/// 返回（收到的最后一条通知, 响应）。用于验证“启动失败先告知原因”的顺序：
/// 通知必须在错误响应前到达（客户端收到错误响应即处置连接）。
fn 请求并收集通知(
    标准输入流: &mut BufWriter<ChildStdin>,
    接收端: &mpsc::Receiver<Value>,
    编号: u64,
    方法: &str,
    参数: Value,
    超时: Duration,
) -> (Option<Value>, Option<Value>) {
    let 消息 = json!({ "jsonrpc": "2.0", "id": 编号, "method": 方法, "params": 参数 });
    标准输入流
        .write_all(&编码消息(&消息))
        .expect("写入请求失败");
    标准输入流.flush().expect("刷新请求失败");
    let 截止时间 = Instant::now() + 超时;
    let mut 通知项 = None;
    loop {
        let 剩余 = 截止时间.saturating_duration_since(Instant::now());
        if 剩余.is_zero() {
            return (通知项, None);
        }
        let Ok(消息) = 接收端.recv_timeout(剩余) else {
            return (通知项, None);
        };
        if 消息.get("method").and_then(Value::as_str) == Some("window/showMessage") {
            通知项 = Some(消息);
            continue;
        }
        if 消息.get("id").and_then(Value::as_u64) == Some(编号) && 消息.get("method").is_none()
        {
            return (通知项, Some(消息));
        }
    }
}

/// 发送初始化请求，验证响应成功
fn 初始化(
    标准输入流: &mut BufWriter<ChildStdin>,
    接收端: &mpsc::Receiver<Value>,
    根地址: &str,
) -> Value {
    let 响应 = 发送请求(
        标准输入流,
        接收端,
        1,
        "initialize",
        json!({
            "processId": null,
            "rootUri": 根地址,
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": {
                        "relatedInformation": true
                    }
                }
            },
            "workspaceFolders": [
                { "uri": 根地址, "name": "e2e-project" }
            ]
        }),
        Duration::from_secs(60),
    );
    assert!(
        响应.get("result").is_some(),
        "initialize 响应应含 result：{响应}"
    );
    响应
}

/// 启动 LSP 服务进程：返回（服务进程, 标准输入, 消息接收器）
fn 启动服务(
    分析器路径: &Path,
    语言包: &Path,
) -> (Child, BufWriter<ChildStdin>, mpsc::Receiver<Value>) {
    let 程序 = env!("CARGO_BIN_EXE_i18n-rust-lsp");
    let mut 服务进程 = Command::new(程序)
        .args(["--language-pack"])
        .arg(语言包)
        .env("RUST_ANALYZER_PATH", 分析器路径)
        .env("RZ_LOG", "error")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("启动 i18n-rust-lsp 失败");
    let 标准输入流 = BufWriter::new(服务进程.stdin.take().expect("取 stdin 失败"));
    let 接收端 = 派生读取线程(服务进程.stdout.take().expect("取 stdout 失败"));
    (服务进程, 标准输入流, 接收端)
}

/// 同 [`启动服务`]，但抓取子进程标准错误，且放开引擎日志级别
///
/// 常规 `启动服务` 把标准错误丢弃并设 `RZ_LOG=error`——引擎的警告级
/// 教学告警因此从未被任何用例看过。本变体专为「服务进程标准错误该不该出现
/// 某类输出」提供可断言的通道：
/// - `RZ_LOG=warn`：与默认级别一致，显式写出以免日后默认变化把断言变成空转；
/// - 标准错误逐行送进通道（rust-analyzer 子进程继承同一标准错误，见
///   `analyzer.rs`，故通道里也有 rust-analyzer 的输出——断言只针对引擎教学告警的
///   行特征，不受影响）。
fn 启动服务并捕获错误输出(
    分析器路径: &Path,
    语言包: &Path,
) -> (
    Child,
    BufWriter<ChildStdin>,
    mpsc::Receiver<Value>,
    mpsc::Receiver<String>,
) {
    let 程序 = env!("CARGO_BIN_EXE_i18n-rust-lsp");
    let mut 服务进程 = Command::new(程序)
        .args(["--language-pack"])
        .arg(语言包)
        .env("RUST_ANALYZER_PATH", 分析器路径)
        .env("RZ_LOG", "warn")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("启动 i18n-rust-lsp 失败");
    let 标准输入流 = BufWriter::new(服务进程.stdin.take().expect("取 stdin 失败"));
    let 接收端 = 派生读取线程(服务进程.stdout.take().expect("取 stdout 失败"));
    let (错误发送端, 错误接收端) = mpsc::channel();
    let 标准错误文本 = 服务进程.stderr.take().expect("取 stderr 失败");
    std::thread::spawn(move || {
        let mut 读取器 = BufReader::new(标准错误文本);
        let mut line = String::new();
        loop {
            line.clear();
            match 读取器.read_line(&mut line) {
                Ok(0) | Err(_) => return,
                Ok(_) => {
                    if 错误发送端.send(line.trim_end().to_string()).is_err() {
                        return;
                    }
                }
            }
        }
    });
    (服务进程, 标准输入流, 接收端, 错误接收端)
}

/// 有界抽取标准错误通道：子进程退出后收尾一小段，避免 rust-analyzer 仍
/// 持有继承的标准错误写端时无界阻塞（读端拿到的告警都产生在锚点之前）
fn 抽取错误输出(错误接收端: &mpsc::Receiver<String>) -> Vec<String> {
    let mut 行集 = Vec::new();
    loop {
        match 错误接收端.recv_timeout(Duration::from_millis(300)) {
            Ok(line) => 行集.push(line),
            Err(mpsc::RecvTimeoutError::Timeout) => return 行集,
            Err(mpsc::RecvTimeoutError::Disconnected) => return 行集,
        }
    }
}

/// 创建临时项目：`Cargo.toml` + 含错误的 `main.zh`
fn 创建测试项目(目录: &Path, 语言码: &str) -> (String, String) {
    std::fs::create_dir_all(目录.join("src")).expect("创建 src 目录失败");
    std::fs::write(
        目录.join("Cargo.toml"),
        "[package]\nname = \"e2e-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("写入 Cargo.toml 失败");
    // 注：不用打印行!——它的格式参数是宏展开期错误，
    // 会让 rust-analyzer 跳过后续语义分析（名称解析），E0425 便不会到达
    let 源码 = 取方言源码(语言码);
    let 地址 = format!("file://{}", 目录.join("src/main.zh").display());
    std::fs::write(目录.join("src/main.zh"), &源码).expect("写入 main.zh 失败");
    (地址, 源码)
}

/// 优雅关闭：关闭请求 → 退出通知 → 等待退出
fn 关闭服务(
    服务进程: &mut Child,
    标准输入流: &mut BufWriter<ChildStdin>,
    接收端: &mpsc::Receiver<Value>,
) {
    let 响应 = 发送请求(
        标准输入流,
        接收端,
        2,
        "shutdown",
        json!(null),
        Duration::from_secs(15),
    );
    assert!(响应.get("result").is_some(), "shutdown 响应异常：{响应}");
    发送通知(标准输入流, "exit", json!(null));
    let _ = 服务进程.wait();
}

/// 是否在持续集成里跑：GitHub 持续集成会设 `CI=true` 等环境变量
fn 运行于持续集成() -> bool {
    ["CI", "GITHUB_ACTIONS"].iter().any(|键| {
        std::env::var(键)
            .ok()
            .is_some_and(|值| !值.is_empty() && 值 != "false")
    })
}

/// 取 rust-analyzer；返回无表示「本地无工具链，调用方应直接返回」
///
/// 旧写法是在各用例里「打印跳过原因并返回」，把「没跑」报成绿色
/// 的通过；测试框架没有「跳过」状态，报告上它与真实通过长得一模一样。
/// CI 里这是致命的：工具链安装环节只要把二进制放到查找链之外（或今后
/// 被改成统一跳过），整个端到端任务会在零验证的情况下全绿。因此
/// CI 环境下缺工具链一律判红，本地仍沿用「打印原因并跳过」的约定。
fn 取分析器路径() -> Option<PathBuf> {
    let 已找到 = 查找分析器();
    if 已找到.is_some() {
        return 已找到;
    }
    assert!(
        !运行于持续集成(),
        "CI 中未找到 rust-analyzer：端到端用例不得静默跳过（安装步骤见 \
         .github/workflows/ci.yml 的 lsp-e2e job：`rzc install toolchain --ra-only --force`）"
    );
    eprintln!("跳过 LSP 端到端测试：未找到 rust-analyzer（可设置 RUST_ANALYZER_PATH）");
    None
}

/// 只需要「能不能跑」而不需要路径的用例开头走这里（判定口径与
/// [`取分析器路径`] 完全一致，避开同一文件里再长一份跳过约定）
fn 分析器可用() -> bool {
    取分析器路径().is_some()
}

/// 单个语言的全链路诊断翻译验证
///
/// `关键词` 为该语言 E0425 翻译模板中的特征词（`errors.toml` [消息翻译] 节），
/// `语言码` 为语言包目录名。变量名断言跨语言通用（模板 {名称} 嵌入同一
/// 中文变量名，转译保留用户标识符）。
fn 运行端到端语言(语言码: &str, 关键词: &str) {
    let Some(分析器路径) = 取分析器路径() else {
        return; // 本地无工具链：上面已打印原因
    };

    // 语言包：测试运行时当前目录为 `crates/lsp`，从包清单目录定位引擎语言包
    let 清单目录 = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let 语言包 = 清单目录.join(format!("../engine/lang-packs/{语言码}"));
    assert!(
        语言包.join("keywords.toml").exists(),
        "语言包目录不存在：{}",
        语言包.display()
    );

    // 临时项目
    let 临时目录 = tempfile::tempdir().expect("创建临时项目失败");
    let (地址, 源码) = 创建测试项目(临时目录.path(), 语言码);

    // 启动 LSP 服务器（显式传入 rust-analyzer 路径，避免 PATH 干扰）
    let (mut 服务进程, mut 标准输入流, 接收端) = 启动服务(&分析器路径, &语言包);

    // 1. 初始化
    let 根地址 = format!("file://{}", 临时目录.path().display());
    初始化(&mut 标准输入流, &接收端, &根地址);

    // 2. 初始化完成通知（触发 rust-analyzer 侧初始化）
    发送通知(&mut 标准输入流, "initialized", json!({}));

    // 3. 打开文档通知：含错误的方言文件
    发送通知(
        &mut 标准输入流,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": 地址,
                "languageId": "rust-zh",
                "version": 1,
                "text": 源码
            }
        }),
    );

    // 4. 等待诊断：rust-analyzer 分批发（空诊断先行），持续收集直到
    //    出现 E0425 的母语翻译；首次分析需加载系统根目录，时限 120 秒
    let 诊断批次 = 等待含关键词诊断(&接收端, &地址, 关键词, Duration::from_secs(120))
        .expect("120s 内未收到含目标关键词的诊断（rust-analyzer 可能未就绪）");
    let 诊断 = 诊断批次
        .pointer("/params/diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(!诊断.is_empty(), "应至少有一条诊断：{诊断批次}");

    // 5. 断言：E0425（未定义名称）的消息被翻译为母语教学诊断
    let 已翻译 = 诊断
        .iter()
        .filter_map(|诊断项| 诊断项.get("message").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        已翻译.contains(关键词),
        "诊断应被翻译为{语言码}（含「{关键词}」），实际：\n{已翻译}"
    );
    assert!(
        已翻译.contains("不存在的变量"),
        "诊断应包含被引用的变量名：\n{已翻译}"
    );

    // 6. 优雅关闭
    关闭服务(&mut 服务进程, &mut 标准输入流, &接收端);
    eprintln!("✅ LSP 端到端测试通过（{语言码}）：E0425 诊断已翻译为母语");
}

#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端_中文诊断已翻译() {
    运行端到端语言("zh", 取语言关键词("zh"));
}

#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端_日文诊断已翻译() {
    运行端到端语言("ja", 取语言关键词("ja"));
}

#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端_俄文诊断已翻译() {
    运行端到端语言("ru", 取语言关键词("ru"));
}

#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端_阿拉伯文诊断已翻译() {
    运行端到端语言("ar", 取语言关键词("ar"));
}

/// 启动失败必须自解释
///
/// rust-analyzer 不可用时（未安装/不可执行/初始化超时），代理过去直接退出，
/// 编辑器只显示 `Server initialization failed.` 与
/// `Pending response rejected since connection got disposed`，用户无从得知该装什么。现在必须先推
/// `window/showMessage`（带具体原因）再回初始化错误响应。
#[test]
fn 端到端_启动失败报告原因() {
    let 清单目录 = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let 语言包 = 清单目录.join("../engine/lang-packs/zh");
    // 用目录充当 rust-analyzer 路径：存在但不可执行，稳定复现“启动失败”
    let 假分析器 = tempfile::tempdir().expect("创建临时目录失败");
    let (mut 服务进程, mut 标准输入流, 接收端) = 启动服务(假分析器.path(), &语言包);

    let (通知项, 响应) = 请求并收集通知(
        &mut 标准输入流,
        &接收端,
        1,
        "initialize",
        json!({
            "processId": std::process::id(),
            "rootUri": null,
            "capabilities": {}
        }),
        Duration::from_secs(30),
    );

    let 通知项 = 通知项.expect("启动失败应先收到 window/showMessage 通知");
    let 文本值 = 通知项
        .pointer("/params/message")
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(
        文本值.contains("rust-analyzer"),
        "通知应说明失败原因（含 rust-analyzer）：{文本值}"
    );
    assert_eq!(
        通知项.pointer("/params/type").and_then(Value::as_u64),
        Some(1),
        "通知应为 Error 级别（type=1）"
    );

    let 响应 = 响应.expect("应收到 initialize 错误响应");
    let 错误消息 = 响应
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(响应.get("error").is_some(), "initialize 应回错误：{响应}");
    assert!(
        错误消息.contains("rust-analyzer"),
        "错误消息应含原因：{错误消息}"
    );

    // 不能挂在半初始化状态：进程应自行以非零码退出
    assert_eq!(
        服务进程.wait().expect("等待进程退出失败").code(),
        Some(1),
        "启动失败应以退出码 1 结束"
    );
}

/// 全角标点教学诊断注入：代码位置的全角标点以提示级诊断在 IDE 内联提示
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端_全角诊断已注入() {
    let Some(分析器路径) = 取分析器路径() else {
        return; // 本地无工具链：上面已打印原因
    };

    let 清单目录 = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let 语言包 = 清单目录.join("../engine/lang-packs/zh");

    let 临时目录 = tempfile::tempdir().expect("创建临时项目失败");
    std::fs::create_dir_all(临时目录.path().join("src")).expect("创建 src 目录失败");
    std::fs::write(
        临时目录.path().join("Cargo.toml"),
        "[package]\nname = \"e2e-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("写入 Cargo.toml 失败");
    // 代码位置含全角分号与全角感叹号（中文输入法常见错误）：
    // 转译后保留在虚拟文本中，代理注入全角教学诊断
    let 源码 = "函数 主函数() {\n    让 数量 = 5；\n    打印行！（\"你好\"）\n}\n";
    let 地址 = format!("file://{}", 临时目录.path().join("src/main.zh").display());
    std::fs::write(临时目录.path().join("src/main.zh"), 源码).expect("写入 main.zh 失败");

    let (mut 服务进程, mut 标准输入流, 接收端) = 启动服务(&分析器路径, &语言包);
    let 根地址 = format!("file://{}", 临时目录.path().display());
    初始化(&mut 标准输入流, &接收端, &根地址);
    发送通知(&mut 标准输入流, "initialized", json!({}));
    发送通知(
        &mut 标准输入流,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": 地址,
                "languageId": "rust-zh",
                "version": 1,
                "text": 源码
            }
        }),
    );

    // 等待代理注入的全角标点诊断（消息模板含「检测到全角标点」；
    // 该片段与 rust-analyzer/rustc 诊断的翻译文本（「检查是否混入了全角标点」）
    // 不撞车，避免等待到未注入教学诊断的批次）
    let 诊断批次 =
        等待含关键词诊断(&接收端, &地址, "检测到全角标点", Duration::from_secs(120))
            .expect("120s 内未收到全角标点教学诊断（注入链路异常）");
    let 诊断 = 诊断批次
        .pointer("/params/diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let 全角诊断: Vec<&Value> = 诊断
        .iter()
        .filter(|诊断项| 诊断项.get("code").and_then(Value::as_str) == Some("fullwidth"))
        .collect();
    assert!(
        !全角诊断.is_empty(),
        "应有 fullwidth 教学诊断：\n{诊断批次}"
    );
    assert_eq!(全角诊断[0]["severity"], 3, "教学诊断应为 Hint 级");
    let 消息文本 = 全角诊断[0]["message"].as_str().unwrap_or("");
    assert!(
        消息文本.contains("第 2 行"),
        "诊断应定位到全角分号所在行（第 2 行）：{消息文本}"
    );

    关闭服务(&mut 服务进程, &mut 标准输入流, &接收端);
    eprintln!("✅ LSP 端到端测试通过：全角标点教学诊断已注入");
}

/// 教学检查诊断注入：未标注类型/魔法数字以提示级诊断在 IDE 内联提示，
/// 「教学忽略」标记行不产生诊断
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端_教学检查诊断已注入() {
    let Some(分析器路径) = 取分析器路径() else {
        return; // 本地无工具链：上面已打印原因
    };

    let 清单目录 = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let 语言包 = 清单目录.join("../engine/lang-packs/zh");

    let 临时目录 = tempfile::tempdir().expect("创建临时项目失败");
    std::fs::create_dir_all(临时目录.path().join("src")).expect("创建 src 目录失败");
    std::fs::write(
        临时目录.path().join("Cargo.toml"),
        "[package]\nname = \"e2e-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("写入 Cargo.toml 失败");
    // 第 2 行类型无法自行推导（`Vec::new()` 未写类型参数，触发
    // `lint-untyped-let` 检查）；可自动推导的字面量不再提示。第 3 行带忽略标记
    let 源码 = "函数 主函数() {\n    让 数量 = Vec::new();\n    让 已忽略 = 1;  // 教学忽略\n}\n";
    let 地址 = format!("file://{}", 临时目录.path().join("src/main.zh").display());
    std::fs::write(临时目录.path().join("src/main.zh"), 源码).expect("写入 main.zh 失败");

    let (mut 服务进程, mut 标准输入流, 接收端) = 启动服务(&分析器路径, &语言包);
    let 根地址 = format!("file://{}", 临时目录.path().display());
    初始化(&mut 标准输入流, &接收端, &根地址);
    发送通知(&mut 标准输入流, "initialized", json!({}));
    发送通知(
        &mut 标准输入流,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": 地址,
                "languageId": "rust-zh",
                "version": 1,
                "text": 源码
            }
        }),
    );

    // 等待代理注入的教学检查诊断（消息模板含「未标注类型」）
    let 诊断批次 = 等待含关键词诊断(&接收端, &地址, "未标注类型", Duration::from_secs(120))
        .expect("120s 内未收到教学 lint 诊断（注入链路异常）");
    let 诊断 = 诊断批次
        .pointer("/params/diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let 教学诊断: Vec<&Value> = 诊断
        .iter()
        .filter(|诊断项| {
            诊断项
                .get("code")
                .and_then(Value::as_str)
                .is_some_and(|码| 码.starts_with("lint-"))
        })
        .collect();
    assert!(!教学诊断.is_empty(), "应有教学 lint 诊断：\n{诊断批次}");
    assert_eq!(教学诊断[0]["severity"], 3, "教学诊断应为 Hint 级");
    let 行号 = 教学诊断[0]["range"]["start"]["line"].as_u64().unwrap();
    assert_eq!(行号, 1, "lint 应定位到第 2 行（0 起行号 1）");
    // 忽略标记行（第 3 行，0 起行号 2）不应出现教学检查诊断
    assert!(
        !教学诊断
            .iter()
            .any(|诊断项| 诊断项["range"]["start"]["line"].as_u64() == Some(2)),
        "「教学忽略」标记行不应有 lint 诊断：{诊断:?}"
    );

    关闭服务(&mut 服务进程, &mut 标准输入流, &接收端);
    eprintln!("✅ LSP 端到端测试通过：教学 lint 诊断已注入（含忽略标记）");
}

/// 教学告警只走诊断通道：服务进程标准错误不重复刷无归属告警
///
/// 全角标点/教学检查在 IDE 里由 `注入教学诊断` 算成诊断、按文件
/// 归属发布；而转译管线的非静默入口会把同一批告警再用引擎日志格式往
/// 服务进程标准错误打一遍，且不含文件名——`运行镜像检查` 逐项目文件循环，
/// 等于每次打开/保存都把整个项目的教学告警无归属刷屏（与问题
/// 面板完全重复）。本用例锁定两面：
/// ① 客户端仍能收到全角教学诊断（正向，防有人把注入链路删掉来「消除刷屏」）；
/// ② 服务进程标准错误不出现引擎格式的教学告警行。
///
/// 需要 `RZ_LOG=warn`（本用例自己的子进程环境，见
/// [`启动服务并捕获错误输出`]）：否则②在任何实现下都恒成立，用例会空转。
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端_教学告警不泄漏到服务错误输出() {
    let Some(分析器路径) = 取分析器路径() else {
        return; // 本地无工具链：上面已打印原因
    };

    let 清单目录 = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let 语言包 = 清单目录.join("../engine/lang-packs/zh");

    let 临时目录 = tempfile::tempdir().expect("创建临时项目失败");
    std::fs::create_dir_all(临时目录.path().join("src")).expect("创建 src 目录失败");
    std::fs::write(
        临时目录.path().join("Cargo.toml"),
        "[package]\nname = \"e2e-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("写入 Cargo.toml 失败");
    // 打开文件：第 4 行魔法数字（教学检查）+ 第 5 行行尾全角分号（全角）
    // + 未定义变量（供锚点的 E0425 转译诊断）
    let 主源码 =
        "模块 辅助;\n\n函数 主函数() {\n    让 数量 = 5;\n    让 结果 = 数量 + 不存在的变量；\n}\n";
    std::fs::write(临时目录.path().join("src/main.zh"), 主源码).expect("写入 main.zh 失败");
    // 未打开的兄弟文件：也带一条全角告警——非静默入口会连它一起刷进标准错误
    std::fs::write(
        临时目录.path().join("src/辅助.zh"),
        "公开 函数 加一(数: i32) -> i32 {\n    数 + 1；\n}\n",
    )
    .expect("写入 辅助.zh 失败");
    let 地址 = format!("file://{}", 临时目录.path().join("src/main.zh").display());

    let (mut 服务进程, mut 标准输入流, 接收端, 错误接收端) =
        启动服务并捕获错误输出(&分析器路径, &语言包);
    let 根地址 = format!("file://{}", 临时目录.path().display());
    初始化(&mut 标准输入流, &接收端, &根地址);
    发送通知(&mut 标准输入流, "initialized", json!({}));
    发送通知(
        &mut 标准输入流,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": 地址,
                "languageId": "rust-zh",
                "version": 1,
                "text": 主源码
            }
        }),
    );

    // 锚点：镜像检查发布的 E0425 母语翻译——它出现即表示 `运行镜像检查`
    // 已逐项目文件跑完转译（非静默入口的刷屏就发生在该循环里）；出现后再收
    // 15s 诊断，确保教学诊断注入也已走完
    let (已找到, 诊断集) = 收集诊断直至稳定(
        &接收端,
        &地址,
        "找不到",
        Duration::from_secs(120),
        Duration::from_secs(15),
    );
    assert!(
        已找到,
        "锚点（E0425 母语翻译）未出现：镜像检查未跑完，本用例未能验证转译循环"
    );
    // ① 注入链路仍在工作：教学诊断按文件归属发到客户端
    let 全角诊断: Vec<&Value> = 诊断集
        .iter()
        .filter(|诊断项| 诊断项.get("code").and_then(Value::as_str) == Some("fullwidth"))
        .collect();
    assert!(
        !全角诊断.is_empty(),
        "客户端应收到 fullwidth 教学诊断（注入链路不得删除）：\n{诊断集:?}"
    );

    关闭服务(&mut 服务进程, &mut 标准输入流, &接收端);

    // ② 服务进程标准错误不出现引擎格式的教学告警行（`[时间] [级别] [模块] …`）：
    //    按模块标签匹配，不依赖级别文案（级别文字随语言包本地化）
    let 教学标签 = ["[fullwidth]", "[lint]", "[unicode_confusion]"];
    let 错误输出行 = 抽取错误输出(&错误接收端);
    let 泄漏行: Vec<&String> = 错误输出行
        .iter()
        .filter(|line| 教学标签.iter().any(|标签| line.contains(标签)))
        .collect();
    assert!(
        泄漏行.is_empty(),
        "教学告警不得无归属刷进 server stderr（已由诊断按文件发布）：\n{}",
        泄漏行
            .iter()
            .map(|line| line.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    eprintln!("✅ LSP 端到端测试通过：教学告警只走诊断通道，stderr 不再刷屏");
}

/// 等待目标地址的「空诊断」批次（修复后/关闭文档通知后诊断被清空）
///
/// rust-analyzer 与代理可能先发布若干非空批次（旧版本残留），仅当收到
/// 该地址的诊断发布通知且诊断数组为空时才返回真。
fn 等待空诊断(接收端: &mpsc::Receiver<Value>, 地址: &str, 超时: Duration) -> bool {
    let 截止时间 = Instant::now() + 超时;
    loop {
        let 剩余 = 截止时间.saturating_duration_since(Instant::now());
        if 剩余.is_zero() {
            return false;
        }
        let Ok(消息) = 接收端.recv_timeout(剩余) else {
            continue;
        };
        if 消息.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            && 消息.pointer("/params/uri").and_then(Value::as_str) == Some(地址)
            && 消息
                .pointer("/params/diagnostics")
                .and_then(Value::as_array)
                .is_some_and(|诊断集| 诊断集.is_empty())
        {
            return true;
        }
    }
}

/// 启动中文会话并打开一份源码（初始化 + 初始化完成通知 + 打开文档通知）
fn 打开中文会话(
    源码: &str,
    项目文件: &[(&str, &str)],
) -> (
    tempfile::TempDir,
    Child,
    BufWriter<ChildStdin>,
    mpsc::Receiver<Value>,
    String,
) {
    // 工具链缺失时 `取分析器路径` 已在 CI 判红，本地缺工具链的情况由各
    // 用例开头的同款调用提前返回，因此这里拿到无只可能是新用例忘了加判定。
    let 分析器路径 =
        取分析器路径().expect("用例开头未做 rust_analyzer_for_run 判定就调用了 open_zh_session");
    let 清单目录 = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let 语言包 = 清单目录.join("../engine/lang-packs/zh");
    let 临时目录 = tempfile::tempdir().expect("创建临时项目失败");
    std::fs::create_dir_all(临时目录.path().join("src")).expect("创建 src 目录失败");
    std::fs::write(
        临时目录.path().join("Cargo.toml"),
        "[package]\nname = \"e2e-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("写入 Cargo.toml 失败");
    for (相对路径, 内容) in 项目文件 {
        let 文件路径 = 临时目录.path().join(相对路径);
        if let Some(父目录) = 文件路径.parent() {
            std::fs::create_dir_all(父目录).expect("创建项目子目录失败");
        }
        std::fs::write(文件路径, 内容).expect("写入项目文件失败");
    }
    let 地址 = format!("file://{}", 临时目录.path().join("src/main.zh").display());
    let (服务进程, mut 标准输入流, 接收端) = 启动服务(&分析器路径, &语言包);
    let 根地址 = format!("file://{}", 临时目录.path().display());
    初始化(&mut 标准输入流, &接收端, &根地址);
    发送通知(&mut 标准输入流, "initialized", json!({}));
    发送通知(
        &mut 标准输入流,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": 地址,
                "languageId": "rust-zh",
                "version": 1,
                "text": 源码
            }
        }),
    );
    (临时目录, 服务进程, 标准输入流, 接收端, 地址)
}

/// 文档生命周期：打开文档通知后镜像检查报错 → 变更文档通知修复清空 → 保存文档通知权威
/// 复核仍为空 → 再改坏后保存重新报错 → 关闭文档通知清空
///
/// 覆盖 `处理文档变更` 的全量替换/缓存更新、`处理文档保存` 的镜像
/// `cargo check` 触发与 `处理文档关闭` 的关闭同步（这些路径纯靠单测构造
/// `代理服务器` 成本过高，走真实协议）。
///
/// 注意：Cargo 项目内 rust-analyzer 原生 E 系列编译错误按设计被抑制（以代理镜像
/// `cargo check` 的 rustc 口径为准），故未保存的实时编辑只下发 rust-analyzer 原生
/// 诊断（教学检查等），编译错误须在保存文档通知（或打开文档通知）后由镜像
/// 检查发布——本测试按该语义锁定，而非假定编辑即重发编译错误。
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端_修改保存关闭生命周期() {
    if !分析器可用() {
        return; // 本地无工具链：已打印原因
    }
    let 坏源码 = 取方言源码("zh");
    let 好源码 = "函数 主函数() {\n    打印行!(\"好\");\n}\n";
    let (_临时目录, mut 服务进程, mut 标准输入流, 接收端, 地址) =
        打开中文会话(&坏源码, &[("src/main.zh", &坏源码)]);

    // 1. 打开即有 E0425 母语诊断（打开文档通知触发镜像 `cargo check`）
    等待含关键词诊断(&接收端, &地址, "找不到", Duration::from_secs(120))
        .expect("打开坏文件后应收到镜像检查的 E0425 翻译诊断");

    // 2. 变更文档通知改为正确代码：rust-analyzer 实时诊断清空
    发送通知(
        &mut 标准输入流,
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": 地址, "version": 2 },
            "contentChanges": [{ "text": 好源码 }]
        }),
    );
    assert!(
        等待空诊断(&接收端, &地址, Duration::from_secs(90)),
        "修复后诊断应清空（didChange 缓存更新）"
    );

    // 3. 保存文档通知正确代码：镜像检查复核后仍为空诊断
    发送通知(
        &mut 标准输入流,
        "textDocument/didSave",
        json!({ "textDocument": { "uri": 地址 }, "text": 好源码 }),
    );
    assert!(
        等待空诊断(&接收端, &地址, Duration::from_secs(120)),
        "保存正确代码后镜像检查应发布空诊断"
    );

    // 4. 再改回错误代码并保存：镜像检查重新发布编译错误（未保存的纯编辑
    //    不会重发 E 系列诊断，见测试文档注释）
    发送通知(
        &mut 标准输入流,
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": 地址, "version": 3 },
            "contentChanges": [{ "text": 坏源码 }]
        }),
    );
    发送通知(
        &mut 标准输入流,
        "textDocument/didSave",
        json!({ "textDocument": { "uri": 地址 }, "text": 坏源码 }),
    );
    等待含关键词诊断(&接收端, &地址, "找不到", Duration::from_secs(120))
        .expect("保存坏代码后应再次收到镜像检查的 E0425 诊断");

    // 5. 关闭文档通知：关闭处理器同步执行（条目移除/兄弟模块同步）；关闭后代理
    //    不再为原地址发空批次（缓存条目已删，rust-analyzer 的清空落在虚拟地址上；
    //    LSP 客户端关闭文档时自行清诊断）。紧随其后的关闭请求能正常应答，
    //    即证明关闭处理器未恐慌、主循环仍存活
    发送通知(
        &mut 标准输入流,
        "textDocument/didClose",
        json!({ "textDocument": { "uri": 地址 } }),
    );

    关闭服务(&mut 服务进程, &mut 标准输入流, &接收端);
    eprintln!("✅ LSP 端到端测试通过：didChange/didSave/didClose 生命周期诊断同步");
}

/// 保存文档通知触发代理自行运行 `cargo check`（草稿 + 真实项目镜像），服务器保持可用
///
/// 覆盖 `处理文档保存` → `触发编译检查` → `运行编译检查一次` 与
/// `运行镜像检查` 链路（纯单测无法触达的子进程编排路径）；保存后服务器仍能
/// 响应请求，证明异步检查线程未拖垮主循环。
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端_保存触发构建检查() {
    if !分析器可用() {
        return; // 本地无工具链：已打印原因
    }
    let 源码 = 取方言源码("zh");
    let (_临时目录, mut 服务进程, mut 标准输入流, 接收端, 地址) =
        打开中文会话(&源码, &[("src/main.zh", &源码)]);
    等待含关键词诊断(&接收端, &地址, "找不到", Duration::from_secs(120)).expect("应先收到 RA 诊断");

    发送通知(
        &mut 标准输入流,
        "textDocument/didSave",
        json!({ "textDocument": { "uri": 地址 }, "text": 源码 }),
    );

    // 保存后服务器必须仍能正常响应（`cargo check` 在后台线程跑，不阻塞主循环）；
    // 格式化请求走 `运行格式化` 子进程并反向翻译回母语
    let 响应 = 发送请求(
        &mut 标准输入流,
        &接收端,
        20,
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": 地址 },
            "options": { "tabSize": 4, "insertSpaces": true }
        }),
        Duration::from_secs(120),
    );
    let 编辑集 = 响应
        .get("result")
        .and_then(Value::as_array)
        .expect("formatting 应返回编辑数组（cargo check 期间主循环仍可用）");
    assert!(!编辑集.is_empty(), "坏代码也应能 rustfmt 出格式化结果");
    let 新文本 = 编辑集[0]["newText"].as_str().expect("编辑应含 newText");
    assert!(
        新文本.contains("函数"),
        "格式化结果应反向翻译回母语：{新文本}"
    );

    关闭服务(&mut 服务进程, &mut 标准输入流, &接收端);
    eprintln!("✅ LSP 端到端测试通过：didSave 后台 cargo check 不阻塞主循环");
}

/// 请求转发：悬停请求经 `转发请求` 代理到 rust-analyzer 并回到客户端
///
/// 覆盖转发链路的位置换算/地址替换/响应编号还原（85 行的 `转发请求`
/// 主体此前只有端到端打开文档通知路径，无请求往返）。
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端_悬停转发往返() {
    if !分析器可用() {
        return; // 本地无工具链：已打印原因
    }
    // 干净文件：避免错误诊断干扰，悬停落到主函数名上
    let 源码 = "函数 主函数() {\n    打印行!(\"好\");\n}\n";
    let (_临时目录, mut 服务进程, mut 标准输入流, 接收端, 地址) =
        打开中文会话(源码, &[("src/main.zh", 源码)]);
    assert!(
        等待空诊断(&接收端, &地址, Duration::from_secs(120)),
        "干净文件分析后应无诊断"
    );

    // rust-analyzer 在分析迭代途中可能回 -32801「内容已修改」（合法 LSP 响应，
    // 客户端会重试）：重试 3 次；最终仍为该错误也证明转发往返链路畅通
    let mut 响应 = Value::Null;
    for 尝试 in 0..3u64 {
        响应 = 发送请求(
            &mut 标准输入流,
            &接收端,
            21 + 尝试,
            "textDocument/hover",
            json!({
                "textDocument": { "uri": 地址 },
                // 第 1 行「函数 主函数() {」的「主函数」内（UTF-16 BMP，字=偏移）
                "position": { "line": 0, "character": 4 }
            }),
            Duration::from_secs(60),
        );
        if 响应.get("result").is_some() {
            break;
        }
        let 内容已修改 = 响应
            .pointer("/error/code")
            .and_then(Value::as_i64)
            .is_some_and(|码| 码 == -32801);
        if !内容已修改 {
            panic!("hover 转发返回非预期错误：{响应}");
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    assert!(
        响应.get("result").is_some()
            || 响应.pointer("/error/code").and_then(Value::as_i64) == Some(-32801),
        "hover 应返回结果或合法的 content-modified 重试信号：{响应}"
    );

    关闭服务(&mut 服务进程, &mut 标准输入流, &接收端);
    eprintln!("✅ LSP 端到端测试通过：hover 请求转发往返正常");
}

/// 语言矩阵完整性：各语言关键词与语言包目录一一对应，防止漏配语言
#[test]
fn 测试_各语言关键词覆盖预期语言() {
    let 预期 = ["zh", "ja", "ru", "ar"];
    for 码 in 预期 {
        assert!(
            各语言关键词.iter().any(|(候选, _)| *候选 == 码),
            "缺少 {码} 的 e2e 覆盖"
        );
    }
}

/// 收集目标地址的诊断直至稳定（返回锚点是否出现 + 窗口内全部诊断）
///
/// 从调用起持续收集；锚点（关键词）出现在某批诊断后，再继续收集
/// 稳定窗口时长——覆盖 rust-analyzer 的后续批次与代理镜像 `cargo check`
/// 的权威诊断发布。总超时为全程硬上限（防锚点反复触发无限延长）。
fn 收集诊断直至稳定(
    接收端: &mpsc::Receiver<Value>,
    地址: &str,
    关键词: &str,
    总超时: Duration,
    稳定时长: Duration,
) -> (bool, Vec<Value>) {
    let 硬截止 = Instant::now() + 总超时;
    let mut 已找到 = false;
    let mut 稳定截止: Option<Instant> = None;
    let mut 全部: Vec<Value> = Vec::new();
    let mut 统计 = BTreeMap::new();
    loop {
        let 当下 = Instant::now();
        let 上限 = 稳定截止.unwrap_or(硬截止).min(硬截止);
        if 当下 >= 上限 {
            if !已找到 {
                eprintln!("⚠️ 锚点「{关键词}」未在超时内出现：");
                转储批次统计(&统计, 地址);
            }
            return (已找到, 全部);
        }
        let 等待时长 = (上限 - 当下).min(Duration::from_millis(500));
        let 消息 = match 接收端.recv_timeout(等待时长) {
            Ok(消息项) => 消息项,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if !已找到 {
                    eprintln!("⚠️ 通道断开且锚点「{关键词}」未出现：");
                    转储批次统计(&统计, 地址);
                }
                return (已找到, 全部);
            }
        };
        记录批次(&mut 统计, &消息);
        if 消息.get("method").and_then(Value::as_str) != Some("textDocument/publishDiagnostics")
            || 消息.pointer("/params/uri").and_then(Value::as_str) != Some(地址)
        {
            continue;
        }
        let Some(诊断集) = 消息
            .pointer("/params/diagnostics")
            .and_then(Value::as_array)
        else {
            continue;
        };
        if 诊断集.is_empty() {
            continue;
        }
        let 批次文本 = 诊断集
            .iter()
            .filter_map(|诊断项| 诊断项.get("message").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        if 批次文本.contains(关键词) {
            已找到 = true;
            稳定截止 = Some(Instant::now() + 稳定时长);
        }
        全部.extend(诊断集.iter().cloned());
    }
}

/// 多模块项目语料·无假红回归（rust-analyzer 升级必跑）
///
/// 测试样例同时覆盖三个历史误报根因，并以真错误（E0425）为锚点：
/// 1. 入口含文件式 `模块 工具;` 声明 → 历史上报 E0583/E0754
///    （剥离文件模块声明 + 镜像 `#[path]` 注解修复）；
/// 2. 跨文件引用未打开的兄弟模块 `工具::加一` → 历史上报 E0432/E0433
///    （同目录模块聚合修复）；
/// 3. `包含字符串!("数据.txt")` 资源引用 → 历史上报无法读取
///    （包含资源复制修复）。
///
/// 断言：锚点出现后追加稳定窗口内收集到的全部诊断（含镜像 `cargo check`
/// 的权威批次）不含以上误报；真锚点不被过滤链误杀。
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端_语料项目无误报红() {
    let Some(分析器路径) = 取分析器路径() else {
        return; // 本地无工具链：上面已打印原因
    };

    let 清单目录 = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let 语言包 = 清单目录.join("../engine/lang-packs/zh");

    let 临时目录 = tempfile::tempdir().expect("创建临时项目失败");
    std::fs::create_dir_all(临时目录.path().join("src")).expect("创建 src 目录失败");
    std::fs::write(
        临时目录.path().join("Cargo.toml"),
        "[package]\nname = \"e2e-corpus\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("写入 Cargo.toml 失败");
    let 主源码 = "模块 工具;\n\n函数 主函数() {\n    让 结果 = 工具::加一(1) + 不存在的变量;\n    让 页面 = 包含字符串!(\"数据.txt\");\n    打印行!(\"{} {}\", 结果, 页面);\n}\n";
    std::fs::write(临时目录.path().join("src/main.zh"), 主源码).expect("写入 main.zh 失败");
    // 未打开的兄弟模块（同目录模块场景）
    std::fs::write(
        临时目录.path().join("src/工具.zh"),
        "公开 函数 加一(数: i32) -> i32 {\n    数 + 1\n}\n",
    )
    .expect("写入 工具.zh 失败");
    // 包含资源（资源复制场景）
    std::fs::write(临时目录.path().join("src/数据.txt"), "占位内容\n").expect("写入 数据.txt 失败");
    let 地址 = format!("file://{}", 临时目录.path().join("src/main.zh").display());

    let (mut 服务进程, mut 标准输入流, 接收端) = 启动服务(&分析器路径, &语言包);
    let 根地址 = format!("file://{}", 临时目录.path().display());
    初始化(&mut 标准输入流, &接收端, &根地址);
    发送通知(&mut 标准输入流, "initialized", json!({}));
    发送通知(
        &mut 标准输入流,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": 地址,
                "languageId": "rust-zh",
                "version": 1,
                "text": 主源码
            }
        }),
    );

    // 锚点：E0425「找不到」的母语翻译；命中后再收集 15s 稳定窗口
    let (已找到, 诊断集) = 收集诊断直至稳定(
        &接收端,
        &地址,
        "找不到",
        Duration::from_secs(120),
        Duration::from_secs(15),
    );
    assert!(
        已找到,
        "锚点（E0425 母语翻译）未出现：rust-analyzer 未就绪或翻译链路异常"
    );

    // 误报断言一：诊断错误码（与消息文案无关，rust-analyzer 改文案也能抓住）
    let 错误码集: Vec<String> = 诊断集
        .iter()
        .filter_map(|诊断项| {
            诊断项
                .get("code")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    for 误报码 in ["E0432", "E0433", "E0583", "E0754"] {
        assert!(
            !错误码集.contains(&误报码.to_string()),
            "出现误报 {误报码}（模块聚合/注解/引用链路失效）：\n{诊断集:#?}"
        );
    }

    // 误报断言二：消息文本特征（无错误码的「无法读取」等 + rust-analyzer 特有文案）
    let 文本集 = 诊断集
        .iter()
        .filter_map(|诊断项| 诊断项.get("message").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    for 误报 in [
        "couldn't read",
        "unresolved import",
        "cannot find module",
        "not included in the module tree",
    ] {
        assert!(
            !文本集.contains(误报),
            "出现误报特征「{误报}」（RA 消息格式漂移或修复失效）：\n{文本集}"
        );
    }

    // 反向断言：真错误不被过滤链误杀
    assert!(
        文本集.contains("找不到"),
        "真错误 E0425 的翻译不应被过滤：\n{文本集}"
    );

    关闭服务(&mut 服务进程, &mut 标准输入流, &接收端);
    eprintln!("✅ LSP 端到端测试通过：多模块 + include 项目无假红");
}

/// 多层模块项目零假红：`src/main.zh` → `src/领域.zh` → `src/领域/工具.zh`
///
/// 回归此前三类级联假红：非入口方言文件的 `模块 工具;` 未补层级 `#[path]`
/// 导致 rustc E0754/E0583、子模块私有 E0603。打开文档通知入口后：
/// - 兄弟同步递归登记嵌套文件，虚拟项目以「顶层聚合 + 父文件末尾
///   `#[path]` 子声明」表达层级；
/// - 镜像 `cargo check` 按真实多层布局转译注解。
///
/// 三个方言地址的最终诊断批次均须为空（rust-analyzer 链与镜像链）。
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端_嵌套模块无错误诊断() {
    if !分析器可用() {
        return; // 本地无工具链：已打印原因
    }
    let 主源码 = "模块 领域;\n\n函数 主函数() {\n    打印行!(\"{}\", 领域::工具::加一(1));\n}\n";
    let 领域源码 = "公开 模块 工具;\n";
    let 工具源码 = "公开 函数 加一(数: i32) -> i32 {\n    数 + 1\n}\n";
    // 临时目录必须活到测试结束（会话工作目录），故绑定保活
    let (_临时目录, mut 服务进程, mut 标准输入流, 接收端, 主地址) = 打开中文会话(
        主源码,
        &[
            ("src/main.zh", 主源码),
            ("src/领域.zh", 领域源码),
            ("src/领域/工具.zh", 工具源码),
        ],
    );

    // 镜像 `cargo check` 成功时不产生 JSON 消息、代理也不会给未打开文件发空
    // 批次；因此以「入口收到空批次（rust-analyzer 链就绪）+ 之后固定稳定窗口（覆盖
    // 镜像 `cargo check`）」为收集策略，窗口内汇总所有地址的全部诊断。
    // 等待入口空批次（rust-analyzer 链就绪），期间收集所有地址的诊断批次——
    // 同步完成前的首批可能「先红后清」，也不能放过
    let mut 已收集: Vec<(String, Value)> = Vec::new();
    let 就绪截止 = Instant::now() + Duration::from_secs(150);
    let mut 入口已清空 = false;
    while Instant::now() < 就绪截止 {
        let Ok(消息) = 接收端.recv_timeout(就绪截止.saturating_duration_since(Instant::now()))
        else {
            break;
        };
        if 消息.get("method").and_then(Value::as_str) != Some("textDocument/publishDiagnostics") {
            continue;
        }
        let Some(诊断集) = 消息
            .pointer("/params/diagnostics")
            .and_then(Value::as_array)
        else {
            continue;
        };
        let 地址 = 消息
            .pointer("/params/uri")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if 地址 == 主地址 && 诊断集.is_empty() {
            入口已清空 = true;
            break;
        }
        for 诊断项 in 诊断集 {
            已收集.push((地址.clone(), 诊断项.clone()));
        }
    }
    assert!(
        入口已清空,
        "入口在 150s 内未收到空诊断批次（RA 未就绪或入口持续有诊断）"
    );
    // 空批次后再留稳定窗口覆盖镜像 `cargo check`（成功时镜像不发任何批次）
    let 稳定截止 = Instant::now() + Duration::from_secs(45);
    while let Ok(消息) = 接收端.recv_timeout(稳定截止.saturating_duration_since(Instant::now()))
    {
        if 消息.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            && let Some(诊断集) = 消息
                .pointer("/params/diagnostics")
                .and_then(Value::as_array)
        {
            let 地址 = 消息
                .pointer("/params/uri")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            for 诊断项 in 诊断集 {
                已收集.push((地址.clone(), 诊断项.clone()));
            }
        }
    }

    // 错误级诊断（严重级别为 1）一律不应出现：零错误项目下全为假红
    let 错误项: Vec<_> = 已收集
        .iter()
        .filter(|(_, 诊断项)| 诊断项.get("severity").and_then(Value::as_u64) == Some(1))
        .collect();
    assert!(
        错误项.is_empty(),
        "多层模块项目出现错误级诊断（假红）：{错误项:#?}"
    );
    // 模块解析类错误码黑名单（rust-analyzer 改文案也能抓住）
    for (地址, 诊断项) in &已收集 {
        if let Some(错误码) = 诊断项.get("code").and_then(Value::as_str) {
            assert!(
                !["E0583", "E0754", "E0603", "E0432", "E0433"].contains(&错误码),
                "多层模块出现模块解析误报 {错误码}（{地址}）：{诊断项}"
            );
        }
    }
    // 无错误码的解析类消息黑名单
    let 文本集 = 已收集
        .iter()
        .filter_map(|(_, 诊断项)| 诊断项.get("message").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    for 误报 in [
        "unresolved import",
        "cannot find module",
        "not included in the module tree",
        "未解析",
        "找不到模块",
    ] {
        assert!(
            !文本集.contains(误报),
            "多层模块出现误报特征「{误报}」：\n{文本集}"
        );
    }

    关闭服务(&mut 服务进程, &mut 标准输入流, &接收端);
    eprintln!("✅ LSP 端到端测试通过：多层模块项目零假红");
}
