//! LSP 端到端测试：真实 rust-analyzer + 中文诊断翻译全链路
//!
//! 启动真实 i18n-rust-lsp 二进制（CARGO_BIN_EXE），通过 JSON-RPC 协议完成
//! initialize → initialized → didOpen（含错误的 .zh 文件）→ 等待
//! publishDiagnostics，断言诊断消息被翻译为母语教学诊断。
//!
//! 前置条件：rust-analyzer 可执行文件可用（查找顺序与 analyzer.rs 相同：
//! RUST_ANALYZER_PATH 环境变量 → ~/.rz/toolchain/bin/rust-analyzer → PATH）。
//! 找不到时测试自动跳过（打印原因，不失败），方便无 rust-analyzer 的
//! 开发环境；CI 中先安装工具链再显式运行：
//! `cargo test -p i18n-rust-lsp --test e2e -- --ignored`

use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

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

/// 等待目标 uri 的 publishDiagnostics：rust-analyzer 分批发诊断
/// （先发 format 错误，后发名称解析错误等），因此持续收集直到
/// 诊断文本包含 `keyword`（或超时）。空诊断通知被跳过。
fn wait_diagnostics_containing(
    rx: &mpsc::Receiver<Value>,
    uri: &str,
    keyword: &str,
    timeout: Duration,
) -> Option<Value> {
    let deadline = Instant::now() + timeout;
    loop {
        let remain = deadline.saturating_duration_since(Instant::now());
        if remain.is_zero() {
            return None;
        }
        let msg = rx.recv_timeout(remain).ok()?;
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

/// 创建临时项目：Cargo.toml + 含错误的 main.zh
fn create_test_project(dir: &Path) -> (String, String) {
    std::fs::create_dir_all(dir.join("src")).expect("创建 src 目录失败");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"e2e-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("写入 Cargo.toml 失败");
    // 注：不用打印行!——println 的 format 参数是宏展开期错误，
    // 会让 rust-analyzer 跳过后续语义分析（名称解析），E0425 便不会到达
    let source = "函数 主函数() {\n    让 数量 = 5;\n    让 结果 = 数量 + 不存在的变量;\n}\n";
    let uri = format!("file://{}", dir.join("src/main.zh").display());
    std::fs::write(dir.join("src/main.zh"), source).expect("写入 main.zh 失败");
    (uri, source.to_string())
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

#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn e2e_chinese_diagnostics_translated() {
    let Some(ra_path) = find_rust_analyzer() else {
        eprintln!("跳过 LSP 端到端测试：未找到 rust-analyzer（可设置 RUST_ANALYZER_PATH）");
        return;
    };

    // 语言包：测试运行时 cwd 为 crates/lsp，从 manifest 目录定位引擎语言包
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("缺少 manifest 目录"));
    let lang_pack = manifest_dir.join("../engine/lang-packs/zh");
    assert!(
        lang_pack.join("keywords.toml").exists(),
        "语言包目录不存在：{}",
        lang_pack.display()
    );

    // 临时项目
    let temp = tempfile::tempdir().expect("创建临时项目失败");
    let (uri, source) = create_test_project(temp.path());

    // 启动 LSP 服务器（显式传入 rust-analyzer 路径，避免 PATH 干扰）
    let bin = env!("CARGO_BIN_EXE_i18n-rust-lsp");
    let mut child = Command::new(bin)
        .args(["--language-pack"])
        .arg(&lang_pack)
        .env("RUST_ANALYZER_PATH", &ra_path)
        .env("RZ_LOG", "error")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("启动 i18n-rust-lsp 失败");
    let mut stdin = BufWriter::new(child.stdin.take().expect("取 stdin 失败"));
    let rx = spawn_reader(child.stdout.take().expect("取 stdout 失败"));

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
    //    出现 E0425 的中文翻译；首次分析需加载 sysroot，时限 120s
    let diag = wait_diagnostics_containing(&rx, &uri, "找不到", Duration::from_secs(120))
        .expect("120s 内未收到含「找不到」的诊断（rust-analyzer 可能未就绪）");
    let diagnostics = diag
        .pointer("/params/diagnostics")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(!diagnostics.is_empty(), "应至少有一条诊断：{diag}");

    // 5. 断言：E0425（未定义名称）的消息被翻译为中文教学诊断
    let translated = diagnostics
        .iter()
        .filter_map(|d| d.get("message").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        translated.contains("找不到"),
        "诊断应被翻译为中文（含「找不到」），实际：\n{translated}"
    );
    assert!(
        translated.contains("不存在的变量"),
        "诊断应包含被引用的变量名：\n{translated}"
    );

    // 6. 优雅关闭
    shutdown(&mut child, &mut stdin, &rx);
    eprintln!("✅ LSP 端到端测试通过：E0425 诊断已翻译为中文");
}
