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
const LANG_KEYWORDS: &[(&str, &str)] = &[
    ("zh", "找不到"),
    ("ja", "見つかりません"),
    ("ru", "не найдено"),
    ("ar", "غير موجود"),
];

/// 按语言码取 E0425 特征词（单一事实来源，测试函数不得再硬编码）
fn lang_keyword(lang_code: &str) -> &'static str {
    LANG_KEYWORDS
        .iter()
        .find(|(code, _)| *code == lang_code)
        .unwrap_or_else(|| panic!("LANG_KEYWORDS 未配置语言：{lang_code}"))
        .1
}

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
    run_e2e_lang("zh", lang_keyword("zh"));
}

#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn e2e_ja_diagnostics_translated() {
    run_e2e_lang("ja", lang_keyword("ja"));
}

#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn e2e_ru_diagnostics_translated() {
    run_e2e_lang("ru", lang_keyword("ru"));
}

#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn e2e_ar_diagnostics_translated() {
    run_e2e_lang("ar", lang_keyword("ar"));
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
    // 第 2 行类型无法自行推导（Vec::new() 无 turbofish，触发
    // lint-untyped-let）；可自动推导的字面量不再提示。第 3 行带忽略标记
    let source = "函数 主函数() {\n    让 数量 = Vec::new();\n    让 已忽略 = 1;  // 教学忽略\n}\n";
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

/// 等待目标 uri 的「空诊断」批次（修复后/didClose 后诊断被清空）
///
/// rust-analyzer 与代理可能先发布若干非空批次（旧版本残留），仅当收到
/// 该 uri 的 publishDiagnostics 且 diagnostics 为空数组时才返回 true。
fn wait_empty_diagnostics(rx: &mpsc::Receiver<Value>, uri: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let remain = deadline.saturating_duration_since(Instant::now());
        if remain.is_zero() {
            return false;
        }
        let Ok(msg) = rx.recv_timeout(remain) else {
            continue;
        };
        if msg.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            && msg.pointer("/params/uri").and_then(Value::as_str) == Some(uri)
            && msg
                .pointer("/params/diagnostics")
                .and_then(Value::as_array)
                .is_some_and(|diags| diags.is_empty())
        {
            return true;
        }
    }
}

/// 启动 zh 会话并打开一份源码（initialize + initialized + didOpen）
fn open_zh_session(
    source: &str,
    project_files: &[(&str, &str)],
) -> (
    tempfile::TempDir,
    Child,
    BufWriter<ChildStdin>,
    mpsc::Receiver<Value>,
    String,
) {
    let ra_path = find_rust_analyzer().expect("需要 rust-analyzer（可设置 RUST_ANALYZER_PATH）");
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
    for (rel, contents) in project_files {
        let p = temp.path().join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("创建项目子目录失败");
        }
        std::fs::write(p, contents).expect("写入项目文件失败");
    }
    let uri = format!("file://{}", temp.path().join("src/main.zh").display());
    let (child, mut stdin, rx) = spawn_lsp(&ra_path, &lang_pack);
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
    (temp, child, stdin, rx, uri)
}

/// 文档生命周期：didOpen 镜像检查报错 → didChange 修复清空 → didSave 权威
/// 复核仍为空 → 再改坏后保存重新报错 → didClose 清空
///
/// 覆盖 handle_did_change 的全量替换/缓存更新、handle_did_save 的镜像
/// cargo check 触发与 handle_did_close 的关闭同步（这些路径纯靠单测构造
/// ProxyServer 成本过高，走真实协议）。
///
/// 注意：cargo 项目内 RA 原生 E 系列编译错误按设计被抑制（以代理镜像
/// `cargo check` 的 rustc 口径为准），故未保存的实时编辑只下发 RA 原生
/// 诊断（教学 lint 等），编译错误须在 didSave（或 didOpen）后由镜像
/// 检查发布——本测试按该语义锁定，而非假定编辑即重发编译错误。
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn e2e_did_change_save_close_lifecycle() {
    let bad = lang_source("zh");
    let good = "函数 主函数() {\n    打印行!(\"好\");\n}\n";
    let (_temp, mut child, mut stdin, rx, uri) = open_zh_session(&bad, &[("src/main.zh", &bad)]);

    // 1. 打开即有 E0425 母语诊断（didOpen 触发镜像 cargo check）
    wait_diagnostics_containing(&rx, &uri, "找不到", Duration::from_secs(120))
        .expect("打开坏文件后应收到镜像检查的 E0425 翻译诊断");

    // 2. didChange 改为正确代码：RA 实时诊断清空
    send_notification(
        &mut stdin,
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [{ "text": good }]
        }),
    );
    assert!(
        wait_empty_diagnostics(&rx, &uri, Duration::from_secs(90)),
        "修复后诊断应清空（didChange 缓存更新）"
    );

    // 3. didSave 正确代码：镜像检查复核后仍为空诊断
    send_notification(
        &mut stdin,
        "textDocument/didSave",
        json!({ "textDocument": { "uri": uri }, "text": good }),
    );
    assert!(
        wait_empty_diagnostics(&rx, &uri, Duration::from_secs(120)),
        "保存正确代码后镜像检查应发布空诊断"
    );

    // 4. 再改回错误代码并保存：镜像检查重新发布编译错误（未保存的纯编辑
    //    不会重发 E 系列诊断，见测试文档注释）
    send_notification(
        &mut stdin,
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": 3 },
            "contentChanges": [{ "text": bad }]
        }),
    );
    send_notification(
        &mut stdin,
        "textDocument/didSave",
        json!({ "textDocument": { "uri": uri }, "text": bad }),
    );
    wait_diagnostics_containing(&rx, &uri, "找不到", Duration::from_secs(120))
        .expect("保存坏代码后应再次收到镜像检查的 E0425 诊断");

    // 5. didClose：关闭处理器同步执行（条目移除/兄弟模块同步）；关闭后代理
    //    不再为原 URI 发空批次（缓存条目已删，RA 的清空落在虚拟 URI 上；
    //    LSP 客户端关闭文档时自行清诊断）。紧随其后的 shutdown 能正常应答，
    //    即证明关闭处理器未 panic、主循环仍存活
    send_notification(
        &mut stdin,
        "textDocument/didClose",
        json!({ "textDocument": { "uri": uri } }),
    );

    shutdown(&mut child, &mut stdin, &rx);
    eprintln!("✅ LSP 端到端测试通过：didChange/didSave/didClose 生命周期诊断同步");
}

/// didSave 触发代理自跑 cargo check（草稿 + 真实项目镜像），服务器保持可用
///
/// 覆盖 handle_did_save → trigger_cargo_check → run_cargo_check_once 与
/// mirror_check 链路（纯单测无法触达的子进程编排路径）；保存后服务器仍能
/// 响应请求，证明异步 check 线程未拖垮主循环。
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn e2e_did_save_triggers_cargo_check() {
    let source = lang_source("zh");
    let (_temp, mut child, mut stdin, rx, uri) =
        open_zh_session(&source, &[("src/main.zh", &source)]);
    wait_diagnostics_containing(&rx, &uri, "找不到", Duration::from_secs(120))
        .expect("应先收到 RA 诊断");

    send_notification(
        &mut stdin,
        "textDocument/didSave",
        json!({ "textDocument": { "uri": uri }, "text": source }),
    );

    // 保存后服务器必须仍能正常响应（cargo check 在后台线程跑，不阻塞主循环）；
    // formatting 走 run_rustfmt 子进程并反向翻译回母语
    let resp = send_request(
        &mut stdin,
        &rx,
        20,
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": uri },
            "options": { "tabSize": 4, "insertSpaces": true }
        }),
        Duration::from_secs(120),
    );
    let edits = resp
        .get("result")
        .and_then(Value::as_array)
        .expect("formatting 应返回编辑数组（cargo check 期间主循环仍可用）");
    assert!(!edits.is_empty(), "坏代码也应能 rustfmt 出格式化结果");
    let new_text = edits[0]["newText"].as_str().expect("编辑应含 newText");
    assert!(
        new_text.contains("函数"),
        "格式化结果应反向翻译回母语：{new_text}"
    );

    shutdown(&mut child, &mut stdin, &rx);
    eprintln!("✅ LSP 端到端测试通过：didSave 后台 cargo check 不阻塞主循环");
}

/// 请求转发：hover 经 forward_request 代理到 rust-analyzer 并回到客户端
///
/// 覆盖转发链路的位置换算/URI 替换/响应 id 还原（85 行的 forward_request
/// 主体此前只有 e2e didOpen 通知路径，无请求往返）。
#[test]
#[ignore = "需要 rust-analyzer 可执行文件（CI 安装工具链后显式运行）"]
fn e2e_hover_forwarded_roundtrip() {
    // 干净文件：避免错误诊断干扰，hover 落到主函数名上
    let source = "函数 主函数() {\n    打印行!(\"好\");\n}\n";
    let (_temp, mut child, mut stdin, rx, uri) =
        open_zh_session(source, &[("src/main.zh", source)]);
    assert!(
        wait_empty_diagnostics(&rx, &uri, Duration::from_secs(120)),
        "干净文件分析后应无诊断"
    );

    // RA 在分析迭代途中可能回 -32801 content modified（合法 LSP 响应，
    // 客户端会重试）：重试 3 次；最终仍为该错误也证明转发往返链路畅通
    let mut resp = Value::Null;
    for attempt in 0..3u64 {
        resp = send_request(
            &mut stdin,
            &rx,
            21 + attempt,
            "textDocument/hover",
            json!({
                "textDocument": { "uri": uri },
                // 第 1 行「函数 主函数() {」的「主函数」内（UTF-16 BMP，字=偏移）
                "position": { "line": 0, "character": 4 }
            }),
            Duration::from_secs(60),
        );
        if resp.get("result").is_some() {
            break;
        }
        let modified = resp
            .pointer("/error/code")
            .and_then(Value::as_i64)
            .is_some_and(|code| code == -32801);
        if !modified {
            panic!("hover 转发返回非预期错误：{resp}");
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    assert!(
        resp.get("result").is_some()
            || resp.pointer("/error/code").and_then(Value::as_i64) == Some(-32801),
        "hover 应返回结果或合法的 content-modified 重试信号：{resp}"
    );

    shutdown(&mut child, &mut stdin, &rx);
    eprintln!("✅ LSP 端到端测试通过：hover 请求转发往返正常");
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
fn e2e_nested_module_no_false_diagnostics() {
    let main_src = "模块 领域;\n\n函数 主函数() {\n    打印行!(\"{}\", 领域::工具::加一(1));\n}\n";
    let domain_src = "公开 模块 工具;\n";
    let tool_src = "公开 函数 加一(数: i32) -> i32 {\n    数 + 1\n}\n";
    // temp 必须活到测试结束（会话工作目录），故绑定保活
    let (_temp, mut child, mut stdin, rx, main_uri) = open_zh_session(
        main_src,
        &[
            ("src/main.zh", main_src),
            ("src/领域.zh", domain_src),
            ("src/领域/工具.zh", tool_src),
        ],
    );

    // 镜像 cargo check 成功时不产生 JSON 消息、代理也不会给未打开文件发空
    // 批次；因此以「入口收到空批次（RA 链就绪）+ 之后固定稳定窗口（覆盖
    // 镜像 cargo check）」为收集策略，窗口内汇总所有 URI 的全部诊断。
    // 等待入口空批次（RA 链就绪），期间收集所有 URI 的诊断批次——
    // sync 完成前的首批可能「先红后清」，也不能放过
    let mut collected: Vec<(String, Value)> = Vec::new();
    let ready_deadline = Instant::now() + Duration::from_secs(150);
    let mut entry_cleared = false;
    while Instant::now() < ready_deadline {
        let Ok(msg) = rx.recv_timeout(ready_deadline.saturating_duration_since(Instant::now()))
        else {
            break;
        };
        if msg.get("method").and_then(Value::as_str) != Some("textDocument/publishDiagnostics") {
            continue;
        }
        let Some(diags) = msg.pointer("/params/diagnostics").and_then(Value::as_array) else {
            continue;
        };
        let u = msg
            .pointer("/params/uri")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if u == main_uri && diags.is_empty() {
            entry_cleared = true;
            break;
        }
        for d in diags {
            collected.push((u.clone(), d.clone()));
        }
    }
    assert!(
        entry_cleared,
        "入口在 150s 内未收到空诊断批次（RA 未就绪或入口持续有诊断）"
    );
    // 空批次后再留稳定窗口覆盖镜像 cargo check（成功时镜像不发任何批次）
    let settle_deadline = Instant::now() + Duration::from_secs(45);
    while let Ok(msg) = rx.recv_timeout(settle_deadline.saturating_duration_since(Instant::now())) {
        if msg.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            && let Some(diags) = msg.pointer("/params/diagnostics").and_then(Value::as_array)
        {
            let u = msg
                .pointer("/params/uri")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            for d in diags {
                collected.push((u.clone(), d.clone()));
            }
        }
    }

    // 错误级诊断（severity=1）一律不应出现：零错误项目下全为假红
    let errors: Vec<_> = collected
        .iter()
        .filter(|(_, d)| d.get("severity").and_then(Value::as_u64) == Some(1))
        .collect();
    assert!(
        errors.is_empty(),
        "多层模块项目出现错误级诊断（假红）：{errors:#?}"
    );
    // 模块解析类 code 黑名单（RA 改文案也能抓住）
    for (u, d) in &collected {
        if let Some(code) = d.get("code").and_then(Value::as_str) {
            assert!(
                !["E0583", "E0754", "E0603", "E0432", "E0433"].contains(&code),
                "多层模块出现模块解析误报 {code}（{u}）：{d}"
            );
        }
    }
    // 无 code 的解析类消息黑名单
    let texts = collected
        .iter()
        .filter_map(|(_, d)| d.get("message").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    for bad in [
        "unresolved import",
        "cannot find module",
        "not included in the module tree",
        "未解析",
        "找不到模块",
    ] {
        assert!(
            !texts.contains(bad),
            "多层模块出现误报特征「{bad}」：\n{texts}"
        );
    }

    shutdown(&mut child, &mut stdin, &rx);
    eprintln!("✅ LSP 端到端测试通过：多层模块项目零假红");
}
