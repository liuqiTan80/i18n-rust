//! LSP 端到端测试：真实 rust-analyzer + 母语诊断翻译全链路
//!
//! 启动真实 i18n-rust-lsp 二进制（CARGO_BIN_EXE），通过 JSON-RPC 协议完成
//! initialize → initialized → didOpen（含错误的 .zh 文件）→ 等待
//! publishDiagnostics，断言诊断消息被翻译为母语教学诊断。
//!
//! 覆盖矩阵：
//! - 4 种代表性语言（zh 中文 / ja 日文 / ru 俄文西里尔 / ar 阿拉伯文 RTL）
//!   验证 E0425（未定义名称）诊断的消息翻译与教学提示；
//! - 全角标点教学诊断注入（代码位置的全角标点在 IDE 内联提示）；
//! - 教学 lint 诊断注入（未标注类型/魔法数字 + 忽略标记）；
//! - 多模块项目无假红语料（模块聚合 / `#[path]` 注解 / include 资源，
//!   rust-analyzer 升级时必跑，防诊断格式漂移）。
//!
//! 前置条件：rust-analyzer 可执行文件可用（查找顺序与 analyzer.rs 相同：
//! RUST_ANALYZER_PATH 环境变量 → ~/.rz/toolchain/bin/rust-analyzer → PATH）。
//! 找不到时测试自动跳过（打印原因，不失败），方便无 rust-analyzer 的
//! 开发环境；CI 中先安装工具链再显式运行：
//! `cargo test -p i18n-rust-lsp --test e2e -- --ignored`

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// 各语言 E0425 翻译的特征词：取自 errors.toml [E0425]「消息模板」
/// （新版 rust-analyzer 直接以 `code: "E0425"` 上报未解析名诊断，LSP 命中
/// 错误码表渲染，而非短语替换路径）；关键词必须为该模板独有子串
const 语言特征词表: &[(&str, &str)] = &[
    ("zh", "找不到"),
    ("ja", "見つかりません"),
    ("ru", "не найдено"),
    ("ar", "غير موجود"),
];

/// 按语言码取 E0425 特征词（单一事实来源，测试函数不得再硬编码）
fn 取特征词(语言码: &str) -> &'static str {
    语言特征词表
        .iter()
        .find(|(状态码, _)| *状态码 == 语言码)
        .unwrap_or_else(|| panic!("LANG_KEYWORDS 未配置语言：{语言码}"))
        .1
}

/// 各语言方言源码：关键字取自该语言包（ja/ru/ar 与 zh 不同），
/// 标识符统一用中文（转译保留用户标识符，RA 报错引用原名，跨语言可断言）
fn 取方言源(语言码: &str) -> String {
    match 语言码 {
        "zh" => "函数 主函数() {\n    让 数量 = 5;\n    让 结果 = 数量 + 不存在的变量;\n}\n",
        "ja" => "関数 主関数() {\n    宣言 数量 = 5;\n    宣言 结果 = 数量 + 不存在的变量;\n}\n",
        "ru" => {
            "функция главная() {\n    пусть 数量 = 5;\n    пусть 结果 = 数量 + 不存在的变量;\n}\n"
        }
        "ar" => "دالة رئيسي() {\n    دع 数量 = 5;\n    دع 结果 = 数量 + 不存在的变量;\n}\n",
        其他语言 => panic!("未知语言：{其他语言}"),
    }
    .to_string()
}

/// rust-analyzer 查找顺序与 analyzer.rs 保持一致
fn 查找分析器程序() -> Option<PathBuf> {
    if let Ok(环境路径) = std::env::var("RUST_ANALYZER_PATH")
        && !环境路径.is_empty()
    {
        return Some(PathBuf::from(环境路径));
    }
    if let Some(路径值) = i18n_rust_engine::工具链::查找工具链程序("rust-analyzer") {
        return Some(路径值);
    }
    let 程序名 = if cfg!(windows) {
        "rust-analyzer.exe"
    } else {
        "rust-analyzer"
    };
    std::env::var_os("PATH").and_then(|环境路径| {
        std::env::split_paths(&环境路径)
            .map(|目录路径| 目录路径.join(程序名))
            .find(|路径值| 路径值.exists())
    })
}

/// JSON-RPC 分帧编码
fn 编码分帧消息(消息: &Value) -> Vec<u8> {
    let 报文体 = serde_json::to_vec(消息).expect("消息序列化失败");
    let mut 帧字节 = format!("Content-Length: {}\r\n\r\n", 报文体.len()).into_bytes();
    帧字节.extend_from_slice(&报文体);
    帧字节
}

/// 从 LSP 子进程 stdout 持续读取并解析分帧消息（线程内阻塞读）
fn 启动读取线程(标准输出: ChildStdout) -> mpsc::Receiver<Value> {
    let (发送端, 接收端) = mpsc::channel();
    std::thread::spawn(move || {
        let mut 读取器 = BufReader::new(标准输出);
        loop {
            let mut 内容长度: Option<usize> = None;
            // 逐行读 header，直到空行
            loop {
                let mut 行文本 = String::new();
                if 读取器.read_line(&mut 行文本).unwrap_or(0) == 0 {
                    return; // EOF：子进程退出
                }
                let 行文本 = 行文本.trim();
                if 行文本.is_empty() {
                    break;
                }
                if let Some(前缀余串) = 行文本.strip_prefix("Content-Length:") {
                    内容长度 = 前缀余串.trim().parse::<usize>().ok();
                }
            }
            let Some(长度) = 内容长度 else { continue };
            let mut 报文体 = vec![0u8; 长度];
            if 读取器.read_exact(&mut 报文体).is_err() {
                return;
            }
            match serde_json::from_slice::<Value>(&报文体) {
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

/// 等待收到指定 id 的响应（跳过通知与其他响应），超时返回 None
fn 等待响应(
    接收端: &mpsc::Receiver<Value>,
    请求编号: u64,
    超时时长: Duration,
) -> Option<Value> {
    let 截止时刻 = Instant::now() + 超时时长;
    loop {
        let 剩余时长 = 截止时刻.saturating_duration_since(Instant::now());
        if 剩余时长.is_zero() {
            return None;
        }
        let 消息 = 接收端.recv_timeout(剩余时长).ok()?;
        if 消息.get("id").and_then(Value::as_u64) == Some(请求编号) {
            return Some(消息);
        }
    }
}

/// 收到的 publishDiagnostics 批次统计：URI →（批次数, 样例消息）
type 批次统计表 = BTreeMap<String, (usize, String)>;

/// 记录一条 publishDiagnostics 批次（不论 URI 是否为目标）
fn 记录批次(统计: &mut 批次统计表, 消息: &Value) {
    if 消息.get("method").and_then(Value::as_str) != Some("textDocument/publishDiagnostics") {
        return;
    }
    let 批次定位 = 消息
        .pointer("/params/uri")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let 样例文本: String = 消息
        .pointer("/params/diagnostics")
        .and_then(Value::as_array)
        .and_then(|数组项| 数组项.first())
        .and_then(|诊断项| 诊断项.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .take(160)
        .collect();
    let 条目 = 统计.entry(批次定位).or_insert((0, String::new()));
    条目.0 += 1;
    if 条目.1.is_empty() {
        条目.1 = 样例文本;
    }
}

/// 转储批次统计（超时诊断：显示收到的全部 URI 与样例，最多 30 个）
fn 转储批次统计(统计: &批次统计表, 目标定位: &str) {
    eprintln!(
        "已收到 {} 个不同 URI 的 publishDiagnostics 批次（目标 URI：{目标定位}）：",
        统计.len()
    );
    for (资源定位, (计数, 样例文本)) in 统计.iter().take(30) {
        eprintln!("  - {资源定位} × {计数}：{样例文本}");
    }
}

/// 等待目标 uri 的 publishDiagnostics：rust-analyzer 分批发诊断
/// （先发 format 错误，后发名称解析错误等），因此持续收集直到
/// 诊断文本包含 `keyword`（或超时）。空诊断通知被跳过。
///
/// 超时时转储收到的全部批次 URI 统计：目标 uri 收不到诊断时，
/// 可区分「链路从未发布」与「发布到了另一种 URI 形式」（Windows
/// 盘符大小写/分隔符/编码差异会导致字符串比较不相等）。
fn 等待含关键词诊断(
    接收端: &mpsc::Receiver<Value>,
    资源定位: &str,
    关键词: &str,
    超时时长: Duration,
) -> Option<Value> {
    let 截止时刻 = Instant::now() + 超时时长;
    let mut 已见文本集: Vec<String> = Vec::new();
    let mut 统计 = 批次统计表::new();
    loop {
        let 剩余时长 = 截止时刻.saturating_duration_since(Instant::now());
        if 剩余时长.is_zero() {
            eprintln!("⚠️ 等待「{关键词}」超时，已收到诊断文本：");
            for 文本项 in 已见文本集.iter().take(20) {
                eprintln!("  {文本项}");
            }
            转储批次统计(&统计, 资源定位);
            return None;
        }
        let 消息 = match 接收端.recv_timeout(剩余时长) {
            Ok(消息) => 消息,
            Err(_) => {
                // 通道断开或单次等待超时：若已过总时限则转储已收到的
                // 诊断文本并返回（否则继续等，保持 120s 总时限语义）
                if Instant::now() >= 截止时刻 {
                    eprintln!("⚠️ 等待「{关键词}」超时，已收到诊断文本：");
                    for 文本项 in 已见文本集.iter().take(20) {
                        eprintln!("  {文本项}");
                    }
                    转储批次统计(&统计, 资源定位);
                    return None;
                }
                continue;
            }
        };
        记录批次(&mut 统计, &消息);
        if 消息.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            && 消息.pointer("/params/uri").and_then(Value::as_str) == Some(资源定位)
        {
            let 诊断数组 = 消息
                .pointer("/params/diagnostics")
                .and_then(Value::as_array);
            let Some(诊断数组) = 诊断数组 else {
                continue;
            };
            if 诊断数组.is_empty() {
                continue;
            }
            let 文本合集 = 诊断数组
                .iter()
                .filter_map(|诊断项| 诊断项.get("message").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            已见文本集.push(文本合集.clone());
            if 文本合集.contains(关键词) {
                return Some(消息);
            }
        }
    }
}

/// 发送通知
fn 发送通知(标准输入: &mut BufWriter<ChildStdin>, 方法名: &str, 参数字段: Value) {
    let 消息 = json!({ "jsonrpc": "2.0", "method": 方法名, "params": 参数字段 });
    标准输入
        .write_all(&编码分帧消息(&消息))
        .expect("写入通知失败");
    标准输入.flush().expect("刷新通知失败");
}

/// 发送请求并等待响应
fn 发送请求(
    标准输入: &mut BufWriter<ChildStdin>,
    接收端: &mpsc::Receiver<Value>,
    请求编号: u64,
    方法名: &str,
    参数字段: Value,
    超时时长: Duration,
) -> Value {
    let 消息 = json!({ "jsonrpc": "2.0", "id": 请求编号, "method": 方法名, "params": 参数字段 });
    标准输入
        .write_all(&编码分帧消息(&消息))
        .expect("写入请求失败");
    标准输入.flush().expect("刷新请求失败");
    等待响应(接收端, 请求编号, 超时时长).unwrap_or_else(|| {
        panic!("等待 {方法名} 响应超时（{超时时长:?}）");
    })
}

/// 发送 initialize 请求，验证响应成功
fn 发送初始化请求(
    标准输入: &mut BufWriter<ChildStdin>,
    接收端: &mpsc::Receiver<Value>,
    根定位: &str,
) -> Value {
    let 响应值 = 发送请求(
        标准输入,
        接收端,
        1,
        "initialize",
        json!({
            "processId": null,
            "rootUri": 根定位,
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": {
                        "relatedInformation": true
                    }
                }
            },
            "workspaceFolders": [
                { "uri": 根定位, "name": "e2e-project" }
            ]
        }),
        Duration::from_secs(60),
    );
    assert!(
        响应值.get("result").is_some(),
        "initialize 响应应含 result：{响应值}"
    );
    响应值
}

/// 启动 LSP 子进程：返回（子进程, stdin, 消息接收器）
fn 启动服务子进程(
    分析器路径: &Path,
    语言包路径: &Path,
) -> (Child, BufWriter<ChildStdin>, mpsc::Receiver<Value>) {
    let 二进制名 = env!("CARGO_BIN_EXE_i18n-rust-lsp");
    let mut 子进程 = Command::new(二进制名)
        .args(["--language-pack"])
        .arg(语言包路径)
        .env("RUST_ANALYZER_PATH", 分析器路径)
        .env("RZ_LOG", "error")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("启动 i18n-rust-lsp 失败");
    let 标准输入 = BufWriter::new(子进程.stdin.take().expect("取 stdin 失败"));
    let 接收端 = 启动读取线程(子进程.stdout.take().expect("取 stdout 失败"));
    (子进程, 标准输入, 接收端)
}

/// 创建临时项目：Cargo.toml + 含错误的 main.zh
fn 搭建测试工程(目录路径: &Path, 语言码: &str) -> (String, String) {
    std::fs::create_dir_all(目录路径.join("src")).expect("创建 src 目录失败");
    std::fs::write(
        目录路径.join("Cargo.toml"),
        "[package]\nname = \"e2e-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("写入 Cargo.toml 失败");
    // 注：不用打印行!——println 的 format 参数是宏展开期错误，
    // 会让 rust-analyzer 跳过后续语义分析（名称解析），E0425 便不会到达
    let 源文本 = 取方言源(语言码);
    let 资源定位 = format!("file://{}", 目录路径.join("src/main.zh").display());
    std::fs::write(目录路径.join("src/main.zh"), &源文本).expect("写入 main.zh 失败");
    (资源定位, 源文本)
}

/// 优雅关闭：shutdown → exit → 等待退出
fn 发送关闭请求(
    子进程: &mut Child,
    标准输入: &mut BufWriter<ChildStdin>,
    接收端: &mpsc::Receiver<Value>,
) {
    let 响应值 = 发送请求(
        标准输入,
        接收端,
        2,
        "shutdown",
        json!(null),
        Duration::from_secs(15),
    );
    assert!(
        响应值.get("result").is_some(),
        "shutdown 响应异常：{响应值}"
    );
    发送通知(标准输入, "exit", json!(null));
    let _ = 子进程.wait();
}

/// 单个语言的全链路诊断翻译验证
///
/// `keyword` 为该语言 E0425 翻译模板中的特征词（errors.toml [消息翻译] 节），
/// `lang_code` 为语言包目录名。变量名断言跨语言通用（模板 {名称} 嵌入同一
/// 中文变量名，转译保留用户标识符）。
fn 运行单语言端到端(语言码: &str, 关键词: &str) {
    let Some(分析器路径) = 查找分析器程序() else {
        eprintln!("跳过 LSP 端到端测试：未找到 rust-analyzer（可设置 RUST_ANALYZER_PATH）");
        return;
    };

    // 语言包：测试运行时 cwd 为 crates/lsp，从 manifest 目录定位引擎语言包
    let 清单目录 = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let 语言包路径 = 清单目录.join(format!("../engine/lang-packs/{语言码}"));
    assert!(
        语言包路径.join("keywords.toml").exists(),
        "语言包目录不存在：{}",
        语言包路径.display()
    );

    // 临时项目
    let 临时工程 = tempfile::tempdir().expect("创建临时项目失败");
    let (资源定位, 源文本) = 搭建测试工程(临时工程.path(), 语言码);

    // 启动 LSP 服务器（显式传入 rust-analyzer 路径，避免 PATH 干扰）
    let (mut 子进程, mut 标准输入, 接收端) = 启动服务子进程(&分析器路径, &语言包路径);

    // 1. initialize
    let 根定位 = format!("file://{}", 临时工程.path().display());
    发送初始化请求(&mut 标准输入, &接收端, &根定位);

    // 2. initialized 通知（触发 rust-analyzer 侧初始化）
    发送通知(&mut 标准输入, "initialized", json!({}));

    // 3. didOpen 含错误的方言文件
    发送通知(
        &mut 标准输入,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": 资源定位,
                "languageId": "rust-zh",
                "version": 1,
                "text": 源文本
            }
        }),
    );

    // 4. 等待诊断：rust-analyzer 分批发（空诊断先行），持续收集直到
    //    出现 E0425 的母语翻译；首次分析需加载 sysroot，时限 120s
    let 诊断消息 = 等待含关键词诊断(&接收端, &资源定位, 关键词, Duration::from_secs(120))
        .expect("120s 内未收到含目标关键词的诊断（rust-analyzer 可能未就绪）");
    let 诊断数组 = 诊断消息
        .pointer("/params/diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(!诊断数组.is_empty(), "应至少有一条诊断：{诊断消息}");

    // 5. 断言：E0425（未定义名称）的消息被翻译为母语教学诊断
    let 译文文本 = 诊断数组
        .iter()
        .filter_map(|诊断项| 诊断项.get("message").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        译文文本.contains(关键词),
        "诊断应被翻译为{语言码}（含「{关键词}」），实际：\n{译文文本}"
    );
    assert!(
        译文文本.contains("不存在的变量"),
        "诊断应包含被引用的变量名：\n{译文文本}"
    );

    // 6. 优雅关闭
    发送关闭请求(&mut 子进程, &mut 标准输入, &接收端);
    eprintln!("✅ LSP 端到端测试通过（{语言码}）：E0425 诊断已翻译为母语");
}

#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端中文诊断已翻译() {
    运行单语言端到端("zh", 取特征词("zh"));
}

#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端日文诊断已翻译() {
    运行单语言端到端("ja", 取特征词("ja"));
}

#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端俄文诊断已翻译() {
    运行单语言端到端("ru", 取特征词("ru"));
}

#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端阿拉伯文诊断已翻译() {
    运行单语言端到端("ar", 取特征词("ar"));
}

/// 全角标点教学诊断注入：代码位置的全角标点以 Hint 级诊断在 IDE 内联提示
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端全角标点诊断注入() {
    let Some(分析器路径) = 查找分析器程序() else {
        eprintln!("跳过 LSP 端到端测试：未找到 rust-analyzer（可设置 RUST_ANALYZER_PATH）");
        return;
    };

    let 清单目录 = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let 语言包路径 = 清单目录.join("../engine/lang-packs/zh");

    let 临时工程 = tempfile::tempdir().expect("创建临时项目失败");
    std::fs::create_dir_all(临时工程.path().join("src")).expect("创建 src 目录失败");
    std::fs::write(
        临时工程.path().join("Cargo.toml"),
        "[package]\nname = \"e2e-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("写入 Cargo.toml 失败");
    // 代码位置含全角分号与全角感叹号（中文输入法常见错误）：
    // 转译后保留在虚拟文本中，代理注入 fullwidth 教学诊断
    let 源文本 = "函数 主函数() {\n    让 数量 = 5；\n    打印行！（\"你好\"）\n}\n";
    let 资源定位 = format!("file://{}", 临时工程.path().join("src/main.zh").display());
    std::fs::write(临时工程.path().join("src/main.zh"), 源文本).expect("写入 main.zh 失败");

    let (mut 子进程, mut 标准输入, 接收端) = 启动服务子进程(&分析器路径, &语言包路径);
    let 根定位 = format!("file://{}", 临时工程.path().display());
    发送初始化请求(&mut 标准输入, &接收端, &根定位);
    发送通知(&mut 标准输入, "initialized", json!({}));
    发送通知(
        &mut 标准输入,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": 资源定位,
                "languageId": "rust-zh",
                "version": 1,
                "text": 源文本
            }
        }),
    );

    // 等待代理注入的全角标点诊断（消息模板含「检测到全角标点」；
    // 该片段与 RA/rustc 诊断的翻译文本（「检查是否混入了全角标点」）
    // 不撞车，避免等待到未注入教学诊断的批次）
    let 诊断消息 = 等待含关键词诊断(
        &接收端,
        &资源定位,
        "检测到全角标点",
        Duration::from_secs(120),
    )
    .expect("120s 内未收到全角标点教学诊断（注入链路异常）");
    let 诊断数组 = 诊断消息
        .pointer("/params/diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let 全角诊断: Vec<&Value> = 诊断数组
        .iter()
        .filter(|诊断项| 诊断项.get("code").and_then(Value::as_str) == Some("fullwidth"))
        .collect();
    assert!(
        !全角诊断.is_empty(),
        "应有 fullwidth 教学诊断：\n{诊断消息}"
    );
    assert_eq!(全角诊断[0]["severity"], 3, "教学诊断应为 Hint 级");
    let 消息文本 = 全角诊断[0]["message"].as_str().unwrap_or("");
    assert!(
        消息文本.contains("第 2 行"),
        "诊断应定位到全角分号所在行（第 2 行）：{消息文本}"
    );

    发送关闭请求(&mut 子进程, &mut 标准输入, &接收端);
    eprintln!("✅ LSP 端到端测试通过：全角标点教学诊断已注入");
}

/// 教学 lint 诊断注入：未标注类型/魔法数字以 Hint 级诊断在 IDE 内联提示，
/// 「教学忽略」标记行不产生诊断
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端教学检查诊断注入() {
    let Some(分析器路径) = 查找分析器程序() else {
        eprintln!("跳过 LSP 端到端测试：未找到 rust-analyzer（可设置 RUST_ANALYZER_PATH）");
        return;
    };

    let 清单目录 = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let 语言包路径 = 清单目录.join("../engine/lang-packs/zh");

    let 临时工程 = tempfile::tempdir().expect("创建临时项目失败");
    std::fs::create_dir_all(临时工程.path().join("src")).expect("创建 src 目录失败");
    std::fs::write(
        临时工程.path().join("Cargo.toml"),
        "[package]\nname = \"e2e-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("写入 Cargo.toml 失败");
    // 第 2 行类型无法自行推导（Vec::new() 无 turbofish，触发
    // lint-untyped-let）；可自动推导的字面量不再提示。第 3 行带忽略标记
    let 源文本 = "函数 主函数() {\n    让 数量 = Vec::new();\n    让 已忽略 = 1;  // 教学忽略\n}\n";
    let 资源定位 = format!("file://{}", 临时工程.path().join("src/main.zh").display());
    std::fs::write(临时工程.path().join("src/main.zh"), 源文本).expect("写入 main.zh 失败");

    let (mut 子进程, mut 标准输入, 接收端) = 启动服务子进程(&分析器路径, &语言包路径);
    let 根定位 = format!("file://{}", 临时工程.path().display());
    发送初始化请求(&mut 标准输入, &接收端, &根定位);
    发送通知(&mut 标准输入, "initialized", json!({}));
    发送通知(
        &mut 标准输入,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": 资源定位,
                "languageId": "rust-zh",
                "version": 1,
                "text": 源文本
            }
        }),
    );

    // 等待代理注入的教学 lint 诊断（消息模板含「未标注类型」）
    let 诊断消息 =
        等待含关键词诊断(&接收端, &资源定位, "未标注类型", Duration::from_secs(120))
            .expect("120s 内未收到教学 lint 诊断（注入链路异常）");
    let 诊断数组 = 诊断消息
        .pointer("/params/diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let 检查诊断: Vec<&Value> = 诊断数组
        .iter()
        .filter(|诊断项| {
            诊断项
                .get("code")
                .and_then(Value::as_str)
                .is_some_and(|特征码| 特征码.starts_with("lint-"))
        })
        .collect();
    assert!(!检查诊断.is_empty(), "应有教学 lint 诊断：\n{诊断消息}");
    assert_eq!(检查诊断[0]["severity"], 3, "教学诊断应为 Hint 级");
    let 行文本 = 检查诊断[0]["range"]["start"]["line"].as_u64().unwrap();
    assert_eq!(行文本, 1, "lint 应定位到第 2 行（0 起行号 1）");
    // 忽略标记行（第 3 行，0 起行号 2）不应出现 lint 诊断
    assert!(
        !检查诊断
            .iter()
            .any(|诊断项| 诊断项["range"]["start"]["line"].as_u64() == Some(2)),
        "「教学忽略」标记行不应有 lint 诊断：{诊断数组:?}"
    );

    发送关闭请求(&mut 子进程, &mut 标准输入, &接收端);
    eprintln!("✅ LSP 端到端测试通过：教学 lint 诊断已注入（含忽略标记）");
}

/// 等待目标 uri 的「空诊断」批次（修复后/didClose 后诊断被清空）
///
/// rust-analyzer 与代理可能先发布若干非空批次（旧版本残留），仅当收到
/// 该 uri 的 publishDiagnostics 且 diagnostics 为空数组时才返回 true。
fn 等待空诊断(
    接收端: &mpsc::Receiver<Value>, 资源定位: &str, 超时时长: Duration
) -> bool {
    let 截止时刻 = Instant::now() + 超时时长;
    loop {
        let 剩余时长 = 截止时刻.saturating_duration_since(Instant::now());
        if 剩余时长.is_zero() {
            return false;
        }
        let Ok(消息) = 接收端.recv_timeout(剩余时长) else {
            continue;
        };
        if 消息.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            && 消息.pointer("/params/uri").and_then(Value::as_str) == Some(资源定位)
            && 消息
                .pointer("/params/diagnostics")
                .and_then(Value::as_array)
                .is_some_and(|诊断列表| 诊断列表.is_empty())
        {
            return true;
        }
    }
}

/// 启动 zh 会话并打开一份源码（initialize + initialized + didOpen）
fn 打开中文会话(
    源文本: &str,
    工程文件表: &[(&str, &str)],
) -> (
    tempfile::TempDir,
    Child,
    BufWriter<ChildStdin>,
    mpsc::Receiver<Value>,
    String,
) {
    let 分析器路径 = 查找分析器程序().expect("需要 rust-analyzer（可设置 RUST_ANALYZER_PATH）");
    let 清单目录 = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let 语言包路径 = 清单目录.join("../engine/lang-packs/zh");
    let 临时工程 = tempfile::tempdir().expect("创建临时项目失败");
    std::fs::create_dir_all(临时工程.path().join("src")).expect("创建 src 目录失败");
    std::fs::write(
        临时工程.path().join("Cargo.toml"),
        "[package]\nname = \"e2e-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("写入 Cargo.toml 失败");
    for (相对路径, 文件内容) in 工程文件表 {
        let 路径值 = 临时工程.path().join(相对路径);
        if let Some(父目录) = 路径值.parent() {
            std::fs::create_dir_all(父目录).expect("创建项目子目录失败");
        }
        std::fs::write(路径值, 文件内容).expect("写入项目文件失败");
    }
    let 资源定位 = format!("file://{}", 临时工程.path().join("src/main.zh").display());
    let (子进程, mut 标准输入, 接收端) = 启动服务子进程(&分析器路径, &语言包路径);
    let 根定位 = format!("file://{}", 临时工程.path().display());
    发送初始化请求(&mut 标准输入, &接收端, &根定位);
    发送通知(&mut 标准输入, "initialized", json!({}));
    发送通知(
        &mut 标准输入,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": 资源定位,
                "languageId": "rust-zh",
                "version": 1,
                "text": 源文本
            }
        }),
    );
    (临时工程, 子进程, 标准输入, 接收端, 资源定位)
}

/// 文档生命周期：didOpen 镜像检查报错 → didChange 修复清空 → didSave 权威
/// 复核仍为空 → 再改坏后保存重新报错 → didClose 清空
///
/// 覆盖 handle_did_change 的全量替换/缓存更新、handle_did_save 的镜像
/// cargo check 触发与 handle_did_close 的关闭同步（这些路径纯靠单测构造
/// 代理服务器 成本过高，走真实协议）。
///
/// 注意：cargo 项目内 RA 原生 E 系列编译错误按设计被抑制（以代理镜像
/// `cargo check` 的 rustc 口径为准），故未保存的实时编辑只下发 RA 原生
/// 诊断（教学 lint 等），编译错误须在 didSave（或 didOpen）后由镜像
/// 检查发布——本测试按该语义锁定，而非假定编辑即重发编译错误。
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端改存关生命周期() {
    let 错误样本 = 取方言源("zh");
    let 正确代码 = "函数 主函数() {\n    打印行!(\"好\");\n}\n";
    let (_保活工程, mut 子进程, mut 标准输入, 接收端, 资源定位) =
        打开中文会话(&错误样本, &[("src/main.zh", &错误样本)]);

    // 1. 打开即有 E0425 母语诊断（didOpen 触发镜像 cargo check）
    等待含关键词诊断(&接收端, &资源定位, "找不到", Duration::from_secs(120))
        .expect("打开坏文件后应收到镜像检查的 E0425 翻译诊断");

    // 2. didChange 改为正确代码：RA 实时诊断清空
    发送通知(
        &mut 标准输入,
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": 资源定位, "version": 2 },
            "contentChanges": [{ "text": 正确代码 }]
        }),
    );
    assert!(
        等待空诊断(&接收端, &资源定位, Duration::from_secs(90)),
        "修复后诊断应清空（didChange 缓存更新）"
    );

    // 3. didSave 正确代码：镜像检查复核后仍为空诊断
    发送通知(
        &mut 标准输入,
        "textDocument/didSave",
        json!({ "textDocument": { "uri": 资源定位 }, "text": 正确代码 }),
    );
    assert!(
        等待空诊断(&接收端, &资源定位, Duration::from_secs(120)),
        "保存正确代码后镜像检查应发布空诊断"
    );

    // 4. 再改回错误代码并保存：镜像检查重新发布编译错误（未保存的纯编辑
    //    不会重发 E 系列诊断，见测试文档注释）
    发送通知(
        &mut 标准输入,
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": 资源定位, "version": 3 },
            "contentChanges": [{ "text": 错误样本 }]
        }),
    );
    发送通知(
        &mut 标准输入,
        "textDocument/didSave",
        json!({ "textDocument": { "uri": 资源定位 }, "text": 错误样本 }),
    );
    等待含关键词诊断(&接收端, &资源定位, "找不到", Duration::from_secs(120))
        .expect("保存坏代码后应再次收到镜像检查的 E0425 诊断");

    // 5. didClose：关闭处理器同步执行（条目移除/兄弟模块同步）；关闭后代理
    //    不再为原 URI 发空批次（缓存条目已删，RA 的清空落在虚拟 URI 上；
    //    LSP 客户端关闭文档时自行清诊断）。紧随其后的 shutdown 能正常应答，
    //    即证明关闭处理器未 panic、主循环仍存活
    发送通知(
        &mut 标准输入,
        "textDocument/didClose",
        json!({ "textDocument": { "uri": 资源定位 } }),
    );

    发送关闭请求(&mut 子进程, &mut 标准输入, &接收端);
    eprintln!("✅ LSP 端到端测试通过：didChange/didSave/didClose 生命周期诊断同步");
}

/// didSave 触发代理自跑 cargo check（草稿 + 真实项目镜像），服务器保持可用
///
/// 覆盖 handle_did_save → trigger_cargo_check → run_cargo_check_once 与
/// 镜像校验 链路（纯单测无法触达的子进程编排路径）；保存后服务器仍能
/// 响应请求，证明异步 check 线程未拖垮主循环。
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端保存触发构建检查() {
    let 源文本 = 取方言源("zh");
    let (_保活工程, mut 子进程, mut 标准输入, 接收端, 资源定位) =
        打开中文会话(&源文本, &[("src/main.zh", &源文本)]);
    等待含关键词诊断(&接收端, &资源定位, "找不到", Duration::from_secs(120))
        .expect("应先收到 RA 诊断");

    发送通知(
        &mut 标准输入,
        "textDocument/didSave",
        json!({ "textDocument": { "uri": 资源定位 }, "text": 源文本 }),
    );

    // 保存后服务器必须仍能正常响应（cargo check 在后台线程跑，不阻塞主循环）；
    // formatting 走 run_rustfmt 子进程并反向翻译回母语
    let 响应值 = 发送请求(
        &mut 标准输入,
        &接收端,
        20,
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": 资源定位 },
            "options": { "tabSize": 4, "insertSpaces": true }
        }),
        Duration::from_secs(120),
    );
    let 编辑数组 = 响应值
        .get("result")
        .and_then(Value::as_array)
        .expect("formatting 应返回编辑数组（cargo check 期间主循环仍可用）");
    assert!(!编辑数组.is_empty(), "坏代码也应能 rustfmt 出格式化结果");
    let 新文本 = 编辑数组[0]["newText"].as_str().expect("编辑应含 newText");
    assert!(
        新文本.contains("函数"),
        "格式化结果应反向翻译回母语：{新文本}"
    );

    发送关闭请求(&mut 子进程, &mut 标准输入, &接收端);
    eprintln!("✅ LSP 端到端测试通过：didSave 后台 cargo check 不阻塞主循环");
}

/// 请求转发：hover 经 forward_request 代理到 rust-analyzer 并回到客户端
///
/// 覆盖转发链路的位置换算/URI 替换/响应 id 还原（85 行的 forward_request
/// 主体此前只有 e2e didOpen 通知路径，无请求往返）。
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端悬浮转发往返() {
    // 干净文件：避免错误诊断干扰，hover 落到主函数名上
    let 源文本 = "函数 主函数() {\n    打印行!(\"好\");\n}\n";
    let (_保活工程, mut 子进程, mut 标准输入, 接收端, 资源定位) =
        打开中文会话(源文本, &[("src/main.zh", 源文本)]);
    assert!(
        等待空诊断(&接收端, &资源定位, Duration::from_secs(120)),
        "干净文件分析后应无诊断"
    );

    // RA 在分析迭代途中可能回 -32801 content modified（合法 LSP 响应，
    // 客户端会重试）：重试 3 次；最终仍为该错误也证明转发往返链路畅通
    let mut 响应值 = Value::Null;
    for 尝试次数 in 0..3u64 {
        响应值 = 发送请求(
            &mut 标准输入,
            &接收端,
            21 + 尝试次数,
            "textDocument/hover",
            json!({
                "textDocument": { "uri": 资源定位 },
                // 第 1 行「函数 主函数() {」的「主函数」内（UTF-16 BMP，字=偏移）
                "position": { "line": 0, "character": 4 }
            }),
            Duration::from_secs(60),
        );
        if 响应值.get("result").is_some() {
            break;
        }
        let 内容已变 = 响应值
            .pointer("/error/code")
            .and_then(Value::as_i64)
            .is_some_and(|状态码| 状态码 == -32801);
        if !内容已变 {
            panic!("hover 转发返回非预期错误：{响应值}");
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    assert!(
        响应值.get("result").is_some()
            || 响应值.pointer("/error/code").and_then(Value::as_i64) == Some(-32801),
        "hover 应返回结果或合法的 content-modified 重试信号：{响应值}"
    );

    发送关闭请求(&mut 子进程, &mut 标准输入, &接收端);
    eprintln!("✅ LSP 端到端测试通过：hover 请求转发往返正常");
}

/// 语言矩阵完整性：LANG_KEYWORDS 与语言包目录一一对应，防止漏配语言
#[test]
fn 语言特征词覆盖预期语言() {
    let 预期语言表 = ["zh", "ja", "ru", "ar"];
    for 状态码 in 预期语言表 {
        assert!(
            语言特征词表.iter().any(|(特征码, _)| *特征码 == 状态码),
            "缺少 {状态码} 的 e2e 覆盖"
        );
    }
}

/// 收集目标 uri 的诊断直至稳定（返回锚点是否出现 + 窗口内全部诊断）
///
/// 从调用起持续收集；锚点（keyword）出现在某批诊断后，再继续收集
/// `settle` 时长——覆盖 rust-analyzer 的后续批次与代理镜像 cargo check
/// 的权威诊断发布。`total_timeout` 为全程硬上限（防锚点反复触发无限延长）。
fn 收集诊断至稳定(
    接收端: &mpsc::Receiver<Value>,
    资源定位: &str,
    关键词: &str,
    总超时: Duration,
    稳定时长: Duration,
) -> (bool, Vec<Value>) {
    let 硬截止 = Instant::now() + 总超时;
    let mut 已找到 = false;
    let mut 稳定截止: Option<Instant> = None;
    let mut 全部诊断: Vec<Value> = Vec::new();
    let mut 统计 = 批次统计表::new();
    loop {
        let 当前时刻 = Instant::now();
        let 时限 = 稳定截止.unwrap_or(硬截止).min(硬截止);
        if 当前时刻 >= 时限 {
            if !已找到 {
                eprintln!("⚠️ 锚点「{关键词}」未在超时内出现：");
                转储批次统计(&统计, 资源定位);
            }
            return (已找到, 全部诊断);
        }
        let 等待时长 = (时限 - 当前时刻).min(Duration::from_millis(500));
        let 消息 = match 接收端.recv_timeout(等待时长) {
            Ok(消息项) => 消息项,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if !已找到 {
                    eprintln!("⚠️ 通道断开且锚点「{关键词}」未出现：");
                    转储批次统计(&统计, 资源定位);
                }
                return (已找到, 全部诊断);
            }
        };
        记录批次(&mut 统计, &消息);
        if 消息.get("method").and_then(Value::as_str) != Some("textDocument/publishDiagnostics")
            || 消息.pointer("/params/uri").and_then(Value::as_str) != Some(资源定位)
        {
            continue;
        }
        let Some(诊断数组) = 消息
            .pointer("/params/diagnostics")
            .and_then(Value::as_array)
        else {
            continue;
        };
        if 诊断数组.is_empty() {
            continue;
        }
        let 批次文本 = 诊断数组
            .iter()
            .filter_map(|诊断项| 诊断项.get("message").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        if 批次文本.contains(关键词) {
            已找到 = true;
            稳定截止 = Some(Instant::now() + 稳定时长);
        }
        全部诊断.extend(诊断数组.iter().cloned());
    }
}

/// 多模块项目语料·无假红回归（rust-analyzer 升级必跑）
///
/// fixture 同时覆盖三个历史误报根因，并以真错误（E0425）为锚点：
/// 1. 入口含文件式 `模块 工具;` 声明 → 历史上报 E0583/E0754
///    （剥离文件模块声明 + 镜像 `#[path]` 注解修复）；
/// 2. 跨文件引用未打开的兄弟模块 `工具::加一` → 历史上报 E0432/E0433
///    （C2a 同目录模块聚合修复）；
/// 3. `包含字符串!("数据.txt")` 资源引用 → 历史上报 couldn't read
///    （C2b include 资源复制修复）。
///
/// 断言：锚点出现后追加稳定窗口内收集到的全部诊断（含镜像 cargo check
/// 的权威批次）不含以上误报；真锚点不被过滤链误杀。
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端语料工程无假红() {
    let Some(分析器路径) = 查找分析器程序() else {
        eprintln!("跳过 LSP 端到端测试：未找到 rust-analyzer（可设置 RUST_ANALYZER_PATH）");
        return;
    };

    let 清单目录 = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let 语言包路径 = 清单目录.join("../engine/lang-packs/zh");

    let 临时工程 = tempfile::tempdir().expect("创建临时项目失败");
    std::fs::create_dir_all(临时工程.path().join("src")).expect("创建 src 目录失败");
    std::fs::write(
        临时工程.path().join("Cargo.toml"),
        "[package]\nname = \"e2e-corpus\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("写入 Cargo.toml 失败");
    let 主源文本 = "模块 工具;\n\n函数 主函数() {\n    让 结果 = 工具::加一(1) + 不存在的变量;\n    让 页面 = 包含字符串!(\"数据.txt\");\n    打印行!(\"{} {}\", 结果, 页面);\n}\n";
    std::fs::write(临时工程.path().join("src/main.zh"), 主源文本).expect("写入 main.zh 失败");
    // 未打开的兄弟模块（C2a 场景）
    std::fs::write(
        临时工程.path().join("src/工具.zh"),
        "公开 函数 加一(数: i32) -> i32 {\n    数 + 1\n}\n",
    )
    .expect("写入 工具.zh 失败");
    // include 资源（C2b 场景）
    std::fs::write(临时工程.path().join("src/数据.txt"), "占位内容\n").expect("写入 数据.txt 失败");
    let 资源定位 = format!("file://{}", 临时工程.path().join("src/main.zh").display());

    let (mut 子进程, mut 标准输入, 接收端) = 启动服务子进程(&分析器路径, &语言包路径);
    let 根定位 = format!("file://{}", 临时工程.path().display());
    发送初始化请求(&mut 标准输入, &接收端, &根定位);
    发送通知(&mut 标准输入, "initialized", json!({}));
    发送通知(
        &mut 标准输入,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": 资源定位,
                "languageId": "rust-zh",
                "version": 1,
                "text": 主源文本
            }
        }),
    );

    // 锚点：E0425「找不到」的母语翻译；命中后再收集 15s 稳定窗口
    let (已找到, 诊断列表) = 收集诊断至稳定(
        &接收端,
        &资源定位,
        "找不到",
        Duration::from_secs(120),
        Duration::from_secs(15),
    );
    assert!(
        已找到,
        "锚点（E0425 母语翻译）未出现：rust-analyzer 未就绪或翻译链路异常"
    );

    // 误报断言一：诊断 code（与消息文案无关，RA 改文案也能抓住）
    let 状态码集: Vec<String> = 诊断列表
        .iter()
        .filter_map(|诊断项| {
            诊断项
                .get("code")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    for 错误样本 in ["E0432", "E0433", "E0583", "E0754"] {
        assert!(
            !状态码集.contains(&错误样本.to_string()),
            "出现误报 {错误样本}（模块聚合/注解/引用链路失效）：\n{诊断列表:#?}"
        );
    }

    // 误报断言二：消息文本特征（无 code 的 couldn't read 等 + RA 特有文案）
    let 文本合集 = 诊断列表
        .iter()
        .filter_map(|诊断项| 诊断项.get("message").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    for 错误样本 in [
        "couldn't read",
        "unresolved import",
        "cannot find module",
        "not included in the module tree",
    ] {
        assert!(
            !文本合集.contains(错误样本),
            "出现误报特征「{错误样本}」（RA 消息格式漂移或修复失效）：\n{文本合集}"
        );
    }

    // 反向断言：真错误不被过滤链误杀
    assert!(
        文本合集.contains("找不到"),
        "真错误 E0425 的翻译不应被过滤：\n{文本合集}"
    );

    发送关闭请求(&mut 子进程, &mut 标准输入, &接收端);
    eprintln!("✅ LSP 端到端测试通过：多模块 + include 项目无假红");
}

/// 多层模块项目零假红：`src/main.zh` → `src/领域.zh` → `src/领域/工具.zh`
///
/// 回归此前三类级联假红：非入口方言文件的 `mod 工具;` 未补层级 `#[path]`
/// 导致 rustc E0754/E0583、子模块私有 E0603。didOpen 入口后：
/// - 兄弟同步递归登记嵌套文件，虚拟项目以「顶层聚合 + 父文件末尾
///   `#[path]` 子声明」表达层级；
/// - 镜像 cargo check 按真实多层布局转译注解。
///
/// 三个方言 URI 的最终诊断批次均须为空（RA 链与镜像链）。
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn 端到端嵌套模块无假诊断() {
    let 主源文本 = "模块 领域;\n\n函数 主函数() {\n    打印行!(\"{}\", 领域::工具::加一(1));\n}\n";
    let 领域源文本 = "公开 模块 工具;\n";
    let 工具源文本 = "公开 函数 加一(数: i32) -> i32 {\n    数 + 1\n}\n";
    // temp 必须活到测试结束（会话工作目录），故绑定保活
    let (_保活工程, mut 子进程, mut 标准输入, 接收端, 主入口定位) = 打开中文会话(
        主源文本,
        &[
            ("src/main.zh", 主源文本),
            ("src/领域.zh", 领域源文本),
            ("src/领域/工具.zh", 工具源文本),
        ],
    );

    // 镜像 cargo check 成功时不产生 JSON 消息、代理也不会给未打开文件发空
    // 批次；因此以「入口收到空批次（RA 链就绪）+ 之后固定稳定窗口（覆盖
    // 镜像 cargo check）」为收集策略，窗口内汇总所有 URI 的全部诊断。
    // 等待入口空批次（RA 链就绪），期间收集所有 URI 的诊断批次——
    // sync 完成前的首批可能「先红后清」，也不能放过
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
        let Some(诊断列表) = 消息
            .pointer("/params/diagnostics")
            .and_then(Value::as_array)
        else {
            continue;
        };
        let 定位串 = 消息
            .pointer("/params/uri")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if 定位串 == 主入口定位 && 诊断列表.is_empty() {
            入口已清空 = true;
            break;
        }
        for 诊断项 in 诊断列表 {
            已收集.push((定位串.clone(), 诊断项.clone()));
        }
    }
    assert!(
        入口已清空,
        "入口在 150s 内未收到空诊断批次（RA 未就绪或入口持续有诊断）"
    );
    // 空批次后再留稳定窗口覆盖镜像 cargo check（成功时镜像不发任何批次）
    let 稳定截止 = Instant::now() + Duration::from_secs(45);
    while let Ok(消息) = 接收端.recv_timeout(稳定截止.saturating_duration_since(Instant::now()))
    {
        if 消息.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            && let Some(诊断列表) = 消息
                .pointer("/params/diagnostics")
                .and_then(Value::as_array)
        {
            let 定位串 = 消息
                .pointer("/params/uri")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            for 诊断项 in 诊断列表 {
                已收集.push((定位串.clone(), 诊断项.clone()));
            }
        }
    }

    // 错误级诊断（severity=1）一律不应出现：零错误项目下全为假红
    let 错误诊断: Vec<_> = 已收集
        .iter()
        .filter(|(_, 诊断项)| 诊断项.get("severity").and_then(Value::as_u64) == Some(1))
        .collect();
    assert!(
        错误诊断.is_empty(),
        "多层模块项目出现错误级诊断（假红）：{错误诊断:#?}"
    );
    // 模块解析类 code 黑名单（RA 改文案也能抓住）
    for (定位串, 诊断项) in &已收集 {
        if let Some(状态码) = 诊断项.get("code").and_then(Value::as_str) {
            assert!(
                !["E0583", "E0754", "E0603", "E0432", "E0433"].contains(&状态码),
                "多层模块出现模块解析误报 {状态码}（{定位串}）：{诊断项}"
            );
        }
    }
    // 无 code 的解析类消息黑名单
    let 文本合集 = 已收集
        .iter()
        .filter_map(|(_, 诊断项)| 诊断项.get("message").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    for 错误样本 in [
        "unresolved import",
        "cannot find module",
        "not included in the module tree",
        "未解析",
        "找不到模块",
    ] {
        assert!(
            !文本合集.contains(错误样本),
            "多层模块出现误报特征「{错误样本}」：\n{文本合集}"
        );
    }

    发送关闭请求(&mut 子进程, &mut 标准输入, &接收端);
    eprintln!("✅ LSP 端到端测试通过：多层模块项目零假红");
}
