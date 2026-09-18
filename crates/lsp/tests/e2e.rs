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

/// 各语言 E0425 翻译的特征词：取自 errors.toml [消息翻译] 节（LSP 走短语替换路径，
/// 非 E0425 模板）：ru 的 cannot find value 翻译为「невозможно найти значение」、
/// ar 为「لا يمكن العثور على القيمة」
const LANG_KEYWORDS: &[(&str, &str)] = &[
    ("zh", "找不到"),
    ("ja", "見つかりません"),
    ("ru", "найти значение"),
    ("ar", "العثور"),
];

/// 各语言方言源码：关键字取自该语言包（ja/ru/ar 与 zh 不同），
/// 标识符统一用中文（转译保留用户标识符，RA 报错引用原名，跨语言可断言）
fn lang_source(lang_code: &str) -> String {
    match lang_code {
        "zh" => "函数 主函数() {\n    让 数量 = 5;\n    让 结果 = 数量 + 不存在的变量;\n}\n",
        "ja" => "関数 主関数() {\n    宣言 数量 = 5;\n    宣言 结果 = 数量 + 不存在的变量;\n}\n",
        "ru" => {
            "функция главная() {\n    пусть 数量 = 5;\n    пусть 结果 = 数量 + 不存在的变量;\n}\n"
        }
        "ar" => "دالة رئيسي() {\n    دع 数量 = 5;\n    دع 结果 = 数量 + 不存在的变量;\n}\n",
        other => panic!("未知语言：{other}"),
    }
    .to_string()
}

/// rust-analyzer 查找顺序与 analyzer.rs 保持一致
fn find_rust_analyzer() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("RUST_ANALYZER_PATH")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    if let Some(p) = i18n_rust_engine::toolchain::find_toolchain_bin("rust-analyzer") {
        return Some(p);
    }
    let exe = if cfg!(windows) {
        "rust-analyzer.exe"
    } else {
        "rust-analyzer"
    };
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(exe))
            .find(|p| p.exists())
    })
}

/// JSON-RPC 分帧编码
fn encode_message(msg: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(msg).expect("消息序列化失败");
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(&body);
    out
}

/// 从 LSP 子进程 stdout 持续读取并解析分帧消息（线程内阻塞读）
fn spawn_reader(stdout: ChildStdout) -> mpsc::Receiver<Value> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut content_length: Option<usize> = None;
            // 逐行读 header，直到空行
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    return; // EOF：子进程退出
                }
                let line = line.trim();
                if line.is_empty() {
                    break;
                }
                if let Some(rest) = line.strip_prefix("Content-Length:") {
                    content_length = rest.trim().parse::<usize>().ok();
                }
            }
            let Some(len) = content_length else { continue };
            let mut body = vec![0u8; len];
            if reader.read_exact(&mut body).is_err() {
                return;
            }
            match serde_json::from_slice::<Value>(&body) {
                Ok(msg) => {
                    if tx.send(msg).is_err() {
                        return;
                    }
                }
                Err(_) => continue, // 容忍异常帧
            }
        }
    });
    rx
}

/// 等待收到指定 id 的响应（跳过通知与其他响应），超时返回 None
fn wait_response(rx: &mpsc::Receiver<Value>, id: u64, timeout: Duration) -> Option<Value> {
    let deadline = Instant::now() + timeout;
    loop {
        let remain = deadline.saturating_duration_since(Instant::now());
        if remain.is_zero() {
            return None;
        }
        let msg = rx.recv_timeout(remain).ok()?;
        if msg.get("id").and_then(Value::as_u64) == Some(id) {
            return Some(msg);
        }
    }
}

/// 收到的 publishDiagnostics 批次统计：URI →（批次数, 样例消息）
type BatchStats = BTreeMap<String, (usize, String)>;

/// 记录一条 publishDiagnostics 批次（不论 URI 是否为目标）
fn record_batch(stats: &mut BatchStats, msg: &Value) {
    if msg.get("method").and_then(Value::as_str) != Some("textDocument/publishDiagnostics") {
        return;
    }
    let batch_uri = msg
        .pointer("/params/uri")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let sample: String = msg
        .pointer("/params/diagnostics")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|d| d.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .take(160)
        .collect();
    let entry = stats.entry(batch_uri).or_insert((0, String::new()));
    entry.0 += 1;
    if entry.1.is_empty() {
        entry.1 = sample;
    }
}

/// 转储批次统计（超时诊断：显示收到的全部 URI 与样例，最多 30 个）
fn dump_batch_stats(stats: &BatchStats, target_uri: &str) {
    eprintln!(
        "已收到 {} 个不同 URI 的 publishDiagnostics 批次（目标 URI：{target_uri}）：",
        stats.len()
    );
    for (uri, (count, sample)) in stats.iter().take(30) {
        eprintln!("  - {uri} × {count}：{sample}");
    }
}

/// 等待目标 uri 的 publishDiagnostics：rust-analyzer 分批发诊断
/// （先发 format 错误，后发名称解析错误等），因此持续收集直到
/// 诊断文本包含 `keyword`（或超时）。空诊断通知被跳过。
///
/// 超时时转储收到的全部批次 URI 统计：目标 uri 收不到诊断时，
/// 可区分「链路从未发布」与「发布到了另一种 URI 形式」（Windows
/// 盘符大小写/分隔符/编码差异会导致字符串比较不相等）。
fn wait_diagnostics_containing(
    rx: &mpsc::Receiver<Value>,
    uri: &str,
    keyword: &str,
    timeout: Duration,
) -> Option<Value> {
    let deadline = Instant::now() + timeout;
    let mut seen_texts: Vec<String> = Vec::new();
    let mut stats = BatchStats::new();
    loop {
        let remain = deadline.saturating_duration_since(Instant::now());
        if remain.is_zero() {
            eprintln!("⚠️ 等待「{keyword}」超时，已收到诊断文本：");
            for t in seen_texts.iter().take(20) {
                eprintln!("  {t}");
            }
            dump_batch_stats(&stats, uri);
            return None;
        }
        let msg = match rx.recv_timeout(remain) {
            Ok(msg) => msg,
            Err(_) => {
                // 通道断开或单次等待超时：若已过总时限则转储已收到的
                // 诊断文本并返回（否则继续等，保持 120s 总时限语义）
                if Instant::now() >= deadline {
                    eprintln!("⚠️ 等待「{keyword}」超时，已收到诊断文本：");
                    for t in seen_texts.iter().take(20) {
                        eprintln!("  {t}");
                    }
                    dump_batch_stats(&stats, uri);
                    return None;
                }
                continue;
            }
        };
        record_batch(&mut stats, &msg);
        if msg.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            && msg.pointer("/params/uri").and_then(Value::as_str) == Some(uri)
        {
            let diagnostics = msg.pointer("/params/diagnostics").and_then(Value::as_array);
            let Some(diagnostics) = diagnostics else {
                continue;
            };
            if diagnostics.is_empty() {
                continue;
            }
            let texts = diagnostics
                .iter()
                .filter_map(|d| d.get("message").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            seen_texts.push(texts.clone());
            if texts.contains(keyword) {
                return Some(msg);
            }
        }
    }
}

/// 发送通知
fn send_notification(stdin: &mut BufWriter<ChildStdin>, method: &str, params: Value) {
    let msg = json!({ "jsonrpc": "2.0", "method": method, "params": params });
    stdin
        .write_all(&encode_message(&msg))
        .expect("写入通知失败");
    stdin.flush().expect("刷新通知失败");
}

/// 发送请求并等待响应
fn send_request(
    stdin: &mut BufWriter<ChildStdin>,
    rx: &mpsc::Receiver<Value>,
    id: u64,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    stdin
        .write_all(&encode_message(&msg))
        .expect("写入请求失败");
    stdin.flush().expect("刷新请求失败");
    wait_response(rx, id, timeout).unwrap_or_else(|| {
        panic!("等待 {method} 响应超时（{timeout:?}）");
    })
}

/// 发送 initialize 请求，验证响应成功
fn initialize(
    stdin: &mut BufWriter<ChildStdin>,
    rx: &mpsc::Receiver<Value>,
    root_uri: &str,
) -> Value {
    let resp = send_request(
        stdin,
        rx,
        1,
        "initialize",
        json!({
            "processId": null,
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": {
                        "relatedInformation": true
                    }
                }
            },
            "workspaceFolders": [
                { "uri": root_uri, "name": "e2e-project" }
            ]
        }),
        Duration::from_secs(60),
    );
    assert!(
        resp.get("result").is_some(),
        "initialize 响应应含 result：{resp}"
    );
    resp
}

/// 启动 LSP 子进程：返回（子进程, stdin, 消息接收器）
fn spawn_lsp(
    ra_path: &Path,
    lang_pack: &Path,
) -> (Child, BufWriter<ChildStdin>, mpsc::Receiver<Value>) {
    let bin = env!("CARGO_BIN_EXE_i18n-rust-lsp");
    let mut child = Command::new(bin)
        .args(["--language-pack"])
        .arg(lang_pack)
        .env("RUST_ANALYZER_PATH", ra_path)
        .env("RZ_LOG", "error")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("启动 i18n-rust-lsp 失败");
    let stdin = BufWriter::new(child.stdin.take().expect("取 stdin 失败"));
    let rx = spawn_reader(child.stdout.take().expect("取 stdout 失败"));
    (child, stdin, rx)
}

/// 创建临时项目：Cargo.toml + 含错误的 main.zh
fn create_test_project(dir: &Path, lang_code: &str) -> (String, String) {
    std::fs::create_dir_all(dir.join("src")).expect("创建 src 目录失败");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"e2e-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("写入 Cargo.toml 失败");
    // 注：不用打印行!——println 的 format 参数是宏展开期错误，
    // 会让 rust-analyzer 跳过后续语义分析（名称解析），E0425 便不会到达
    let source = lang_source(lang_code);
    let uri = format!("file://{}", dir.join("src/main.zh").display());
    std::fs::write(dir.join("src/main.zh"), &source).expect("写入 main.zh 失败");
    (uri, source)
}

/// 优雅关闭：shutdown → exit → 等待退出
fn shutdown(child: &mut Child, stdin: &mut BufWriter<ChildStdin>, rx: &mpsc::Receiver<Value>) {
    let resp = send_request(
        stdin,
        rx,
        2,
        "shutdown",
        json!(null),
        Duration::from_secs(15),
    );
    assert!(resp.get("result").is_some(), "shutdown 响应异常：{resp}");
    send_notification(stdin, "exit", json!(null));
    let _ = child.wait();
}

/// 单个语言的全链路诊断翻译验证
///
/// `keyword` 为该语言 E0425 翻译模板中的特征词（errors.toml [消息翻译] 节），
/// `lang_code` 为语言包目录名。变量名断言跨语言通用（模板 {名称} 嵌入同一
/// 中文变量名，转译保留用户标识符）。
fn run_e2e_lang(lang_code: &str, keyword: &str) {
    let Some(ra_path) = find_rust_analyzer() else {
        eprintln!("跳过 LSP 端到端测试：未找到 rust-analyzer（可设置 RUST_ANALYZER_PATH）");
        return;
    };

    // 语言包：测试运行时 cwd 为 crates/lsp，从 manifest 目录定位引擎语言包
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let lang_pack = manifest_dir.join(format!("../engine/lang-packs/{lang_code}"));
    assert!(
        lang_pack.join("keywords.toml").exists(),
        "语言包目录不存在：{}",
        lang_pack.display()
    );

    // 临时项目
    let temp = tempfile::tempdir().expect("创建临时项目失败");
    let (uri, source) = create_test_project(temp.path(), lang_code);

    // 启动 LSP 服务器（显式传入 rust-analyzer 路径，避免 PATH 干扰）
    let (mut child, mut stdin, rx) = spawn_lsp(&ra_path, &lang_pack);

    // 1. initialize
    let root_uri = format!("file://{}", temp.path().display());
    initialize(&mut stdin, &rx, &root_uri);

    // 2. initialized 通知（触发 rust-analyzer 侧初始化）
    send_notification(&mut stdin, "initialized", json!({}));

    // 3. didOpen 含错误的方言文件
    send_notification(
        &mut stdin,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "rust-zh",
                "version": 1,
                "text": source
            }
        }),
    );

    // 4. 等待诊断：rust-analyzer 分批发（空诊断先行），持续收集直到
    //    出现 E0425 的母语翻译；首次分析需加载 sysroot，时限 120s
    let diag = wait_diagnostics_containing(&rx, &uri, keyword, Duration::from_secs(120))
        .expect("120s 内未收到含目标关键词的诊断（rust-analyzer 可能未就绪）");
    let diagnostics = diag
        .pointer("/params/diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(!diagnostics.is_empty(), "应至少有一条诊断：{diag}");

    // 5. 断言：E0425（未定义名称）的消息被翻译为母语教学诊断
    let translated = diagnostics
        .iter()
        .filter_map(|d| d.get("message").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        translated.contains(keyword),
        "诊断应被翻译为{lang_code}（含「{keyword}」），实际：\n{translated}"
    );
    assert!(
        translated.contains("不存在的变量"),
        "诊断应包含被引用的变量名：\n{translated}"
    );

    // 6. 优雅关闭
    shutdown(&mut child, &mut stdin, &rx);
    eprintln!("✅ LSP 端到端测试通过（{lang_code}）：E0425 诊断已翻译为母语");
}

#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn e2e_zh_diagnostics_translated() {
    run_e2e_lang("zh", "找不到");
}

#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn e2e_ja_diagnostics_translated() {
    run_e2e_lang("ja", "見つかりません");
}

#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn e2e_ru_diagnostics_translated() {
    run_e2e_lang("ru", "найти значение");
}

#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn e2e_ar_diagnostics_translated() {
    run_e2e_lang("ar", "العثور");
}

/// 全角标点教学诊断注入：代码位置的全角标点以 Hint 级诊断在 IDE 内联提示
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn e2e_fullwidth_diagnostic_injected() {
    let Some(ra_path) = find_rust_analyzer() else {
        eprintln!("跳过 LSP 端到端测试：未找到 rust-analyzer（可设置 RUST_ANALYZER_PATH）");
        return;
    };

    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let lang_pack = manifest_dir.join("../engine/lang-packs/zh");

    let temp = tempfile::tempdir().expect("创建临时项目失败");
    std::fs::create_dir_all(temp.path().join("src")).expect("创建 src 目录失败");
    std::fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"e2e-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("写入 Cargo.toml 失败");
    // 代码位置含全角分号与全角感叹号（中文输入法常见错误）：
    // 转译后保留在虚拟文本中，代理注入 fullwidth 教学诊断
    let source = "函数 主函数() {\n    让 数量 = 5；\n    打印行！（\"你好\"）\n}\n";
    let uri = format!("file://{}", temp.path().join("src/main.zh").display());
    std::fs::write(temp.path().join("src/main.zh"), source).expect("写入 main.zh 失败");

    let (mut child, mut stdin, rx) = spawn_lsp(&ra_path, &lang_pack);
    let root_uri = format!("file://{}", temp.path().display());
    initialize(&mut stdin, &rx, &root_uri);
    send_notification(&mut stdin, "initialized", json!({}));
    send_notification(
        &mut stdin,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "rust-zh",
                "version": 1,
                "text": source
            }
        }),
    );

    // 等待代理注入的全角标点诊断（消息模板含「检测到全角标点」；
    // 该片段与 RA/rustc 诊断的翻译文本（「检查是否混入了全角标点」）
    // 不撞车，避免等待到未注入教学诊断的批次）
    let diag = wait_diagnostics_containing(&rx, &uri, "检测到全角标点", Duration::from_secs(120))
        .expect("120s 内未收到全角标点教学诊断（注入链路异常）");
    let diagnostics = diag
        .pointer("/params/diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let fullwidth: Vec<&Value> = diagnostics
        .iter()
        .filter(|d| d.get("code").and_then(Value::as_str) == Some("fullwidth"))
        .collect();
    assert!(!fullwidth.is_empty(), "应有 fullwidth 教学诊断：\n{diag}");
    assert_eq!(fullwidth[0]["severity"], 3, "教学诊断应为 Hint 级");
    let message = fullwidth[0]["message"].as_str().unwrap_or("");
    assert!(
        message.contains("第 2 行"),
        "诊断应定位到全角分号所在行（第 2 行）：{message}"
    );

    shutdown(&mut child, &mut stdin, &rx);
    eprintln!("✅ LSP 端到端测试通过：全角标点教学诊断已注入");
}

/// 教学 lint 诊断注入：未标注类型/魔法数字以 Hint 级诊断在 IDE 内联提示，
/// 「教学忽略」标记行不产生诊断
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn e2e_teaching_lint_diagnostic_injected() {
    let Some(ra_path) = find_rust_analyzer() else {
        eprintln!("跳过 LSP 端到端测试：未找到 rust-analyzer（可设置 RUST_ANALYZER_PATH）");
        return;
    };

    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let lang_pack = manifest_dir.join("../engine/lang-packs/zh");

    let temp = tempfile::tempdir().expect("创建临时项目失败");
    std::fs::create_dir_all(temp.path().join("src")).expect("创建 src 目录失败");
    std::fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"e2e-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("写入 Cargo.toml 失败");
    // 第 2 行未标注类型（触发 lint-untyped-let），第 3 行带忽略标记（不触发）
    let source = "函数 主函数() {\n    让 数量 = 5;\n    让 已忽略 = 1;  // 教学忽略\n}\n";
    let uri = format!("file://{}", temp.path().join("src/main.zh").display());
    std::fs::write(temp.path().join("src/main.zh"), source).expect("写入 main.zh 失败");

    let (mut child, mut stdin, rx) = spawn_lsp(&ra_path, &lang_pack);
    let root_uri = format!("file://{}", temp.path().display());
    initialize(&mut stdin, &rx, &root_uri);
    send_notification(&mut stdin, "initialized", json!({}));
    send_notification(
        &mut stdin,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "rust-zh",
                "version": 1,
                "text": source
            }
        }),
    );

    // 等待代理注入的教学 lint 诊断（消息模板含「未标注类型」）
    let diag = wait_diagnostics_containing(&rx, &uri, "未标注类型", Duration::from_secs(120))
        .expect("120s 内未收到教学 lint 诊断（注入链路异常）");
    let diagnostics = diag
        .pointer("/params/diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let lint_diags: Vec<&Value> = diagnostics
        .iter()
        .filter(|d| {
            d.get("code")
                .and_then(Value::as_str)
                .is_some_and(|c| c.starts_with("lint-"))
        })
        .collect();
    assert!(!lint_diags.is_empty(), "应有教学 lint 诊断：\n{diag}");
    assert_eq!(lint_diags[0]["severity"], 3, "教学诊断应为 Hint 级");
    let line = lint_diags[0]["range"]["start"]["line"].as_u64().unwrap();
    assert_eq!(line, 1, "lint 应定位到第 2 行（0 起行号 1）");
    // 忽略标记行（第 3 行，0 起行号 2）不应出现 lint 诊断
    assert!(
        !lint_diags
            .iter()
            .any(|d| d["range"]["start"]["line"].as_u64() == Some(2)),
        "「教学忽略」标记行不应有 lint 诊断：{diagnostics:?}"
    );

    shutdown(&mut child, &mut stdin, &rx);
    eprintln!("✅ LSP 端到端测试通过：教学 lint 诊断已注入（含忽略标记）");
}

/// 语言矩阵完整性：LANG_KEYWORDS 与语言包目录一一对应，防止漏配语言
#[test]
fn test_lang_keywords_cover_expected_languages() {
    let expected = ["zh", "ja", "ru", "ar"];
    for code in expected {
        assert!(
            LANG_KEYWORDS.iter().any(|(c, _)| *c == code),
            "缺少 {code} 的 e2e 覆盖"
        );
    }
}

/// 收集目标 uri 的诊断直至稳定（返回锚点是否出现 + 窗口内全部诊断）
///
/// 从调用起持续收集；锚点（keyword）出现在某批诊断后，再继续收集
/// `settle` 时长——覆盖 rust-analyzer 的后续批次与代理镜像 cargo check
/// 的权威诊断发布。`total_timeout` 为全程硬上限（防锚点反复触发无限延长）。
fn collect_diagnostics_until_settle(
    rx: &mpsc::Receiver<Value>,
    uri: &str,
    keyword: &str,
    total_timeout: Duration,
    settle: Duration,
) -> (bool, Vec<Value>) {
    let hard_deadline = Instant::now() + total_timeout;
    let mut found = false;
    let mut settle_deadline: Option<Instant> = None;
    let mut all: Vec<Value> = Vec::new();
    let mut stats = BatchStats::new();
    loop {
        let now = Instant::now();
        let limit = settle_deadline.unwrap_or(hard_deadline).min(hard_deadline);
        if now >= limit {
            if !found {
                eprintln!("⚠️ 锚点「{keyword}」未在超时内出现：");
                dump_batch_stats(&stats, uri);
            }
            return (found, all);
        }
        let wait = (limit - now).min(Duration::from_millis(500));
        let msg = match rx.recv_timeout(wait) {
            Ok(m) => m,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if !found {
                    eprintln!("⚠️ 通道断开且锚点「{keyword}」未出现：");
                    dump_batch_stats(&stats, uri);
                }
                return (found, all);
            }
        };
        record_batch(&mut stats, &msg);
        if msg.get("method").and_then(Value::as_str) != Some("textDocument/publishDiagnostics")
            || msg.pointer("/params/uri").and_then(Value::as_str) != Some(uri)
        {
            continue;
        }
        let Some(diagnostics) = msg.pointer("/params/diagnostics").and_then(Value::as_array) else {
            continue;
        };
        if diagnostics.is_empty() {
            continue;
        }
        let batch = diagnostics
            .iter()
            .filter_map(|d| d.get("message").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        if batch.contains(keyword) {
            found = true;
            settle_deadline = Some(Instant::now() + settle);
        }
        all.extend(diagnostics.iter().cloned());
    }
}

/// 多模块项目语料·无假红回归（rust-analyzer 升级必跑）
///
/// fixture 同时覆盖三个历史误报根因，并以真错误（E0425）为锚点：
/// 1. 入口含文件式 `模块 工具;` 声明 → 历史上报 E0583/E0754
///    （strip_file_module_decls + 镜像 `#[path]` 注解修复）；
/// 2. 跨文件引用未打开的兄弟模块 `工具::加一` → 历史上报 E0432/E0433
///    （C2a 同目录模块聚合修复）；
/// 3. `包含字符串!("数据.txt")` 资源引用 → 历史上报 couldn't read
///    （C2b include 资源复制修复）。
///
/// 断言：锚点出现后追加稳定窗口内收集到的全部诊断（含镜像 cargo check
/// 的权威批次）不含以上误报；真锚点不被过滤链误杀。
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn e2e_no_false_red_in_corpus_project() {
    let Some(ra_path) = find_rust_analyzer() else {
        eprintln!("跳过 LSP 端到端测试：未找到 rust-analyzer（可设置 RUST_ANALYZER_PATH）");
        return;
    };

    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let lang_pack = manifest_dir.join("../engine/lang-packs/zh");

    let temp = tempfile::tempdir().expect("创建临时项目失败");
    std::fs::create_dir_all(temp.path().join("src")).expect("创建 src 目录失败");
    std::fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"e2e-corpus\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("写入 Cargo.toml 失败");
    let main_src = "模块 工具;\n\n函数 主函数() {\n    让 结果 = 工具::加一(1) + 不存在的变量;\n    让 页面 = 包含字符串!(\"数据.txt\");\n    打印行!(\"{} {}\", 结果, 页面);\n}\n";
    std::fs::write(temp.path().join("src/main.zh"), main_src).expect("写入 main.zh 失败");
    // 未打开的兄弟模块（C2a 场景）
    std::fs::write(
        temp.path().join("src/工具.zh"),
        "公开 函数 加一(数: i32) -> i32 {\n    数 + 1\n}\n",
    )
    .expect("写入 工具.zh 失败");
    // include 资源（C2b 场景）
    std::fs::write(temp.path().join("src/数据.txt"), "占位内容\n").expect("写入 数据.txt 失败");
    let uri = format!("file://{}", temp.path().join("src/main.zh").display());

    let (mut child, mut stdin, rx) = spawn_lsp(&ra_path, &lang_pack);
    let root_uri = format!("file://{}", temp.path().display());
    initialize(&mut stdin, &rx, &root_uri);
    send_notification(&mut stdin, "initialized", json!({}));
    send_notification(
        &mut stdin,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "rust-zh",
                "version": 1,
                "text": main_src
            }
        }),
    );

    // 锚点：E0425「找不到」的母语翻译；命中后再收集 15s 稳定窗口
    let (found, diags) = collect_diagnostics_until_settle(
        &rx,
        &uri,
        "找不到",
        Duration::from_secs(120),
        Duration::from_secs(15),
    );
    assert!(
        found,
        "锚点（E0425 母语翻译）未出现：rust-analyzer 未就绪或翻译链路异常"
    );

    // 误报断言一：诊断 code（与消息文案无关，RA 改文案也能抓住）
    let codes: Vec<String> = diags
        .iter()
        .filter_map(|d| d.get("code").and_then(Value::as_str).map(str::to_string))
        .collect();
    for bad in ["E0432", "E0433", "E0583", "E0754"] {
        assert!(
            !codes.contains(&bad.to_string()),
            "出现误报 {bad}（模块聚合/注解/引用链路失效）：\n{diags:#?}"
        );
    }

    // 误报断言二：消息文本特征（无 code 的 couldn't read 等 + RA 特有文案）
    let texts = diags
        .iter()
        .filter_map(|d| d.get("message").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    for bad in [
        "couldn't read",
        "unresolved import",
        "cannot find module",
        "not included in the module tree",
    ] {
        assert!(
            !texts.contains(bad),
            "出现误报特征「{bad}」（RA 消息格式漂移或修复失效）：\n{texts}"
        );
    }

    // 反向断言：真错误不被过滤链误杀
    assert!(
        texts.contains("找不到"),
        "真错误 E0425 的翻译不应被过滤：\n{texts}"
    );

    shutdown(&mut child, &mut stdin, &rx);
    eprintln!("✅ LSP 端到端测试通过：多模块 + include 项目无假红");
}
