//! 代理服务器模块
//!
//! LSP 代理服务器的核心逻辑：
//! 1. 通过 lsp-server 与编辑器客户端建立 LSP 连接
//! 2. 接收方言文件（.zh/.en/.de 等）变更，翻译后通知 rust-analyzer
//! 3. 转发 rust-analyzer 的响应/通知，并还原位置信息
//! 4. 翻译诊断消息为对应语言

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use lsp_server::{Connection, Message, Notification, Request, Response};
use serde_json::{Value, json};

use i18n_rust_engine::mapping_source;

use crate::analyzer::AnalyzerConnection;
use crate::response_map::ResponseMapper;
use crate::translation_cache::{TranslationCache, TranslationEntry};

/// 默认支持的方言文件扩展名（与内置语言包 lang_info.toml 的扩展名一致）
/// 可通过命令行 `--extensions` 参数覆盖
const DEFAULT_EXTENSIONS: &[&str] = &[
    ".zh", ".en", ".de", ".ja", ".ru", ".es", ".fr", ".pt", ".ko", ".ar", ".hi",
];

/// LSP 代理服务器
pub struct ProxyServer {
    /// 与编辑器客户端的 LSP 连接
    connection: Connection,
    /// 翻译缓存
    cache: Arc<TranslationCache>,
    /// rust-analyzer 子进程
    analyzer: AnalyzerConnection,
    /// 响应映射器
    mapper: Arc<ResponseMapper>,
    /// 自增请求 ID
    request_counter: Arc<std::sync::atomic::AtomicI64>,
    /// 待映射的 rust-analyzer 请求 ID → 原始客户端请求信息
    pending_requests: Arc<std::sync::Mutex<HashMap<i64, PendingRequestInfo>>>,
    /// 支持的方言文件扩展名列表（如 `.zh`、`.en`）
    supported_extensions: Vec<String>,
    /// 上次重载时的模块集合版本号（初值 -1 保证首个文档打开时重载一次）
    last_module_version: std::sync::atomic::AtomicI64,
}

/// 记录一个转发给 rust-analyzer 的请求的原始信息
#[derive(Debug, Clone)]
struct PendingRequestInfo {
    original_id: lsp_server::RequestId,
    method: String,
    original_uri: String,
    /// 转发时刻（用于超时清理，避免响应永不到达时条目无限累积）
    created_at: std::time::Instant,
}

/// 转发请求的等待超时：超过后向客户端应答错误并丢弃条目
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

impl ProxyServer {
    /// 创建并初始化代理服务器
    ///
    /// `extensions`: 支持的方言文件扩展名列表（如 `.zh`），
    /// 传空列表时使用默认值（`.zh` / `.en` / `.de`）。
    pub fn new(
        lang_pack_path: &Path,
        extensions: &[String],
    ) -> anyhow::Result<(Self, lsp_server::IoThreads)> {
        // 1. 加载语言包
        let (keyword_map, macro_map, alias_map) = load_language_pack(lang_pack_path)?;
        log::info!(
            "{}",
            crate::ui::global().f(
                "lsp_log_loaded_mappings",
                &[&keyword_map.len().to_string(), &macro_map.len().to_string()]
            )
        );

        // 2. 创建翻译缓存（临时目录按用户隔离，避免多用户共享 /tmp 路径）
        let temp_dir = virtual_temp_dir()?;
        let cache = TranslationCache::new(keyword_map, macro_map, alias_map, temp_dir);

        // 3. 启动 rust-analyzer
        let analyzer = AnalyzerConnection::start()?;

        // 4. 创建响应映射器
        let mapper = Arc::new(ResponseMapper::new(cache.clone()));

        // 5. 建立 LSP 连接（stdio）
        let (connection, io_threads) = Connection::stdio();

        let server = Self {
            connection,
            cache,
            analyzer,
            mapper,
            request_counter: Arc::new(std::sync::atomic::AtomicI64::new(1000)),
            pending_requests: Arc::new(std::sync::Mutex::new(HashMap::new())),
            supported_extensions: if extensions.is_empty() {
                DEFAULT_EXTENSIONS.iter().map(|s| s.to_string()).collect()
            } else {
                extensions.to_vec()
            },
            last_module_version: std::sync::atomic::AtomicI64::new(-1),
        };

        Ok((server, io_threads))
    }

    /// 运行服务器主循环
    pub fn run(self, io_threads: lsp_server::IoThreads) -> anyhow::Result<()> {
        // 1. 等待 initialize 请求并握手
        let (init_id, init_params) = self.handshake()?;
        log::info!("{}", crate::ui::global().t("lsp_log_client_init"));

        // 2. 初始化 rust-analyzer
        self.initialize_analyzer(&init_params)?;

        // 3. 回复客户端 initialize
        self.reply_initialize(init_id)?;

        // 4. 启动 rust-analyzer 消息转发线程
        self.start_forwarding_thread()?;

        // 5. 进入主循环
        self.main_loop()?;

        // 6. 主循环结束后（收到 shutdown 或客户端断开）清理资源。
        // 消息转发线程持有客户端发送端的克隆，并阻塞在 rust-analyzer
        // 的消息通道上；必须先停止 rust-analyzer 子进程让转发线程退出，
        // 再释放服务器（含客户端连接发送端），否则 IO 写入线程
        // 因通道永不关闭而无法结束，进程将无法退出。
        let mut server = self;
        server.analyzer.stop();
        drop(server);

        // 7. 等待 IO 线程结束
        io_threads.join()?;

        Ok(())
    }

    /// 握手：接收 initialize 请求并回复
    fn handshake(&self) -> anyhow::Result<(lsp_server::RequestId, Value)> {
        let msg = self.connection.receiver.recv().map_err(|e| {
            anyhow::anyhow!(
                "{}",
                crate::ui::global().f("lsp_err_recv_initialize", &[&e.to_string()])
            )
        })?;

        match msg {
            Message::Request(req) => {
                if req.method == "initialize" {
                    let id = req.id.clone();
                    let params = req.params.clone();
                    Ok((id, params))
                } else {
                    anyhow::bail!(
                        "{}",
                        crate::ui::global().f("lsp_err_expect_initialize", &[&req.method])
                    )
                }
            }
            _ => anyhow::bail!("{}", crate::ui::global().t("lsp_err_expect_request")),
        }
    }

    /// 向 rust-analyzer 发送 initialize 并等待响应
    fn initialize_analyzer(&self, params: &Value) -> anyhow::Result<()> {
        // 不透传客户端的 rootUri/workspaceFolders：
        // 客户端工作区（如整个 zrRust 项目）与母语文件无关，
        // 透传会导致 rust-analyzer 全量分析并发布海量诊断，
        // 阻塞代理到客户端的消息通道。
        // 改用虚拟项目目录作为 rust-analyzer 的工作区根，
        // 使其只分析自动生成的虚拟 .rs 文件。
        let _ = params;
        let virtual_project = self.cache.virtual_project_uri();

        let init_request = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": virtual_project,
                "capabilities": {
                    "textDocument": {
                        "completion": {
                            "completionItem": {
                                "snippetSupport": false,
                                "labelDetailsSupport": true
                            }
                        },
                        "publishDiagnostics": {
                            "relatedInformation": true
                        },
                        "hover": {
                            "contentFormat": ["markdown", "plaintext"]
                        },
                        "definition": {},
                        "references": {},
                        "documentHighlight": {},
                        "documentSymbol": {
                            "hierarchicalDocumentSymbolSupport": true
                        },
                        "rename": {
                            "prepareSupport": true,
                            "prepareSupportDefaultBehavior": 1
                        },
                        "codeAction": {
                            "codeActionLiteralSupport": {
                                "codeActionKind": {
                                    "valueSet": ["quickfix", "refactor", "source"]
                                }
                            },
                            "resolveSupport": { "properties": ["edit"] }
                        },
                        "signatureHelp": {
                            "signatureInformation": {
                                "parameterInformation": { "labelOffsetSupport": true }
                            }
                        }
                    },
                    "workspace": {
                        "workspaceEdit": { "documentChanges": true }
                    }
                },
                "workspaceFolders": [{
                    "uri": virtual_project,
                    "name": "i18n-virtual"
                }],
                // 虚拟项目无依赖/无构建脚本/无过程宏：关闭相应后台任务，
                // 减少启动与保存时的 cargo 开销；诊断（checkOnSave）保留
                "initializationOptions": {
                    "cargo": { "buildScripts": { "enable": false } },
                    "procMacro": { "enable": false }
                }
            }
        });

        self.analyzer.send(&init_request)?;

        // 等待 rust-analyzer 的 initialize 响应（超时则直接报错，
        // 避免在半初始化状态下继续服务导致行为不可预测）
        let mut 初始化完成 = false;
        for _ in 0..200 {
            if let Some(response) = self.analyzer.try_recv()
                && response.get("id").and_then(|v| v.as_i64()) == Some(0)
            {
                log::info!("{}", crate::ui::global().t("lsp_log_ra_init_done"));
                初始化完成 = true;
                break;
            }
            thread::sleep(std::time::Duration::from_millis(50));
        }
        if !初始化完成 {
            anyhow::bail!("{}", crate::ui::global().t("lsp_err_ra_init_timeout"));
        }

        // 注意：initialized 通知不在本处发送，
        // 而是等客户端发来 initialized 时再转发（见 handle_client_notification），
        // 避免重复发送导致 rust-analyzer 报 unhandled notification。

        Ok(())
    }

    /// 回复客户端 initialize 响应
    fn reply_initialize(&self, id: lsp_server::RequestId) -> anyhow::Result<()> {
        let capabilities = json!({
            "capabilities": {
                "textDocumentSync": {
                    "openClose": true,
                    "change": 2,
                    "save": { "includeText": true }
                },
                "completionProvider": {
                    "triggerCharacters": [".", ":"]
                },
                "hoverProvider": true,
                "definitionProvider": true,
                "referencesProvider": true,
                "documentSymbolProvider": true,
                "codeActionProvider": true,
                "renameProvider": true,
                "documentHighlightProvider": true,
                "documentFormattingProvider": true,
                "signatureHelpProvider": {
                    "triggerCharacters": ["(", ","]
                }
            },
            "serverInfo": {
                "name": "i18n-rust-lsp",
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        let response = Response {
            id,
            result: Some(capabilities),
            error: None,
        };

        self.connection
            .sender
            .send(Message::Response(response))
            .map_err(|e| {
                anyhow::anyhow!(
                    "{}",
                    crate::ui::global().f("lsp_err_send_initialize", &[&e.to_string()])
                )
            })?;
        Ok(())
    }

    /// 启动 rust-analyzer → 客户端的消息转发线程
    fn start_forwarding_thread(&self) -> anyhow::Result<()> {
        let receiver = self.analyzer.message_channel();
        let mapper = self.mapper.clone();
        let sender = self.connection.sender.clone();
        let pending = self.pending_requests.clone();
        // rust-analyzer 主动请求（如 workspace/diagnostic/refresh）
        // 的响应需要回发给 rust-analyzer 本身，而非客户端。
        let ra_sender = self.analyzer.sender_clone();

        thread::spawn(move || {
            // 用 recv_timeout 代替阻塞 recv，使无消息时也能周期性
            // 清理超时请求（rust-analyzer 挂起/丢弃请求时向客户端应答错误）
            let mut 上次清理 = std::time::Instant::now();
            loop {
                match receiver.recv_timeout(std::time::Duration::from_secs(1)) {
                    Ok(msg) => {
                        handle_analyzer_message(&msg, &mapper, &sender, &pending, &ra_sender)
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
                if 上次清理.elapsed() >= std::time::Duration::from_secs(10) {
                    cleanup_expired_requests(&pending, &sender);
                    上次清理 = std::time::Instant::now();
                }
            }
            log::info!("{}", crate::ui::global().t("lsp_log_ra_thread_exit"));
        });

        Ok(())
    }

    /// 主消息循环
    fn main_loop(&self) -> anyhow::Result<()> {
        loop {
            let msg = match self.connection.receiver.recv() {
                Ok(msg) => msg,
                Err(_) => {
                    log::info!("{}", crate::ui::global().t("lsp_log_client_disconnected"));
                    break;
                }
            };

            match msg {
                Message::Request(req) => {
                    if req.method == "shutdown" {
                        log::info!("{}", crate::ui::global().t("lsp_log_shutdown"));
                        let response = Response {
                            id: req.id,
                            result: Some(Value::Null),
                            error: None,
                        };
                        let _ = self.connection.sender.send(Message::Response(response));
                        break;
                    }
                    // 单条请求处理失败（如 rust-analyzer 写入失败）只记录错误，
                    // 不退出服务器，避免一次偶发故障杀死整个会话
                    if let Err(e) = self.handle_client_request(req) {
                        log::error!(
                            "{}",
                            crate::ui::global().f("lsp_err_handle_request", &[&e.to_string()])
                        );
                    }
                }
                Message::Notification(notif) => {
                    if let Err(e) = self.handle_client_notification(notif) {
                        log::error!(
                            "{}",
                            crate::ui::global().f("lsp_err_handle_notification", &[&e.to_string()])
                        );
                    }
                }
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    /// 处理客户端请求
    fn handle_client_request(&self, req: Request) -> anyhow::Result<()> {
        log::debug!(
            "{}",
            crate::ui::global().f(
                "lsp_log_client_request",
                &[&req.method, &format!("{:?}", req.id)]
            )
        );

        match req.method.as_str() {
            "initialize" => Ok(()), // 已在握手中处理
            // 格式化采用全文件替换策略，由代理自行处理（不转发 rust-analyzer）
            "textDocument/formatting" => self.handle_formatting(req),
            "textDocument/completion"
            | "textDocument/hover"
            | "textDocument/definition"
            | "textDocument/references"
            | "textDocument/documentSymbol"
            | "textDocument/codeAction"
            | "textDocument/rename"
            | "textDocument/documentHighlight"
            | "textDocument/signatureHelp" => self.forward_request(req),
            _ => self.forward_request(req),
        }
    }

    /// 处理客户端通知
    fn handle_client_notification(&self, notif: Notification) -> anyhow::Result<()> {
        log::debug!(
            "{}",
            crate::ui::global().f("lsp_log_client_notification", &[&notif.method])
        );

        match notif.method.as_str() {
            "textDocument/didOpen" => self.handle_did_open(&notif.params),
            "textDocument/didChange" => self.handle_did_change(&notif.params),
            "textDocument/didClose" => self.handle_did_close(&notif.params),
            "textDocument/didSave" => self.handle_did_save(&notif.params),
            "initialized" => {
                // 转发给 rust-analyzer，并将虚拟项目目录加入其工作区，
                // 使其加载自动生成的 Cargo.toml，获得跨文件语义分析能力
                let msg = json!({
                    "jsonrpc": "2.0",
                    "method": "initialized",
                    "params": {}
                });
                self.analyzer.send(&msg)?;

                let workspace_notification = json!({
                    "jsonrpc": "2.0",
                    "method": "workspace/didChangeWorkspaceFolders",
                    "params": {
                        "event": {
                            "added": [{
                                "uri": self.cache.virtual_project_uri(),
                                "name": "i18n-virtual"
                            }],
                            "removed": []
                        }
                    }
                });
                self.analyzer.send(&workspace_notification)
            }
            _ => {
                let msg = json!({
                    "jsonrpc": "2.0",
                    "method": notif.method,
                    "params": notif.params
                });
                self.analyzer.send(&msg)
            }
        }
    }

    /// 判断 URI 是否为受支持的方言文件
    fn is_supported_file(&self, uri: &str) -> bool {
        is_supported_file(uri, &self.supported_extensions)
    }

    /// 模块集合发生变化时重载虚拟项目工作区（否则跳过，避免频繁全量重扫）
    fn reload_if_modules_changed(&self) -> anyhow::Result<()> {
        let new_version = self.cache.module_version() as i64;
        let prev = self
            .last_module_version
            .swap(new_version, std::sync::atomic::Ordering::SeqCst);
        if prev == new_version {
            return Ok(());
        }
        // 模块集合变化：main.rs 的聚合已更新，显式重载让 rust-analyzer
        // 重新扫描并识别新模块（文件系统监听可能失败）
        self.reload_virtual_project()
    }

    /// 处理文档打开
    fn handle_did_open(&self, params: &Value) -> anyhow::Result<()> {
        let doc = &params["textDocument"];
        let uri = doc["uri"].as_str().unwrap_or("");
        let content = doc["text"].as_str().unwrap_or("");
        let version = doc["version"].as_i64().unwrap_or(1) as i32;

        if !self.is_supported_file(uri) {
            return Ok(());
        }

        let (entry, other_changes) = self.cache.update_document(uri, content, version)?;

        // 模块集合变化时重载虚拟项目工作区，确保 rust-analyzer
        // 重新扫描并识别 main.rs 中的新模块聚合。
        self.reload_if_modules_changed()?;

        // 模块集合变化可能导致其他已打开文件被重写
        // （其虚拟内容新增/移除了 crate:: 前缀），重新通知 rust-analyzer。
        // 这些文档已在 rust-analyzer 中打开，必须用 didChange 全量同步
        // （对已打开文档重复发送 didOpen 违反 LSP 协议）
        for change_entry in &other_changes {
            let ra_msg = json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didChange",
                "params": {
                    "textDocument": {
                        "uri": change_entry.virtual_uri,
                        "version": change_entry.version
                    },
                    "contentChanges": [{ "text": change_entry.en_content }]
                }
            });
            self.analyzer.send(&ra_msg)?;
        }

        let ra_msg = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": entry.virtual_uri,
                    "languageId": "rust",
                    "version": version,
                    "text": entry.en_content
                }
            }
        });
        self.analyzer.send(&ra_msg)?;
        log::info!(
            "{}",
            crate::ui::global().f("lsp_log_doc_opened", &[uri, &version.to_string()])
        );
        Ok(())
    }

    /// 处理文档变更
    ///
    /// 代理声明增量同步（change=2）以减少客户端→代理的传输量：
    /// 无 range 的变更项为全量文本（兼容旧客户端），
    /// 带 range 的按 LSP 位置（UTF-16）逐项应用到缓存中的母语文本。
    /// 应用后仍全量重译，并以全量替换通知 rust-analyzer。
    fn handle_did_change(&self, params: &Value) -> anyhow::Result<()> {
        let doc = &params["textDocument"];
        let uri = doc["uri"].as_str().unwrap_or("");
        let version = doc["version"].as_i64().unwrap_or(1) as i32;

        if !self.is_supported_file(uri) {
            return Ok(());
        }

        let Some(changes_list) = params["contentChanges"].as_array() else {
            return Ok(());
        };

        // 在缓存旧文本上按序应用变更，得到新全文
        let mut content = self
            .cache
            .query_original(uri)
            .map(|e| e.zh_content.clone())
            .unwrap_or_default();
        for change in changes_list {
            if let Some(text) = change["text"].as_str() {
                if change.get("range").is_none() {
                    // 全量替换
                    content = text.to_string();
                } else {
                    apply_incremental_change(&mut content, &change["range"], text);
                }
            }
        }

        let (entry, _) = self.cache.update_document(uri, &content, version)?;
        let ra_msg = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {
                    "uri": entry.virtual_uri,
                    "version": version
                },
                "contentChanges": [{ "text": entry.en_content }]
            }
        });
        self.analyzer.send(&ra_msg)?;
        Ok(())
    }

    /// 处理文档关闭
    fn handle_did_close(&self, params: &Value) -> anyhow::Result<()> {
        let uri = params["textDocument"]["uri"].as_str().unwrap_or("");
        if !self.is_supported_file(uri) {
            return Ok(());
        }

        if let Some(entry) = self.cache.query_original(uri) {
            let ra_msg = json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didClose",
                "params": {
                    "textDocument": { "uri": entry.virtual_uri }
                }
            });
            self.analyzer.send(&ra_msg)?;
        }

        // 关闭文档会缩小模块集合，其余条目的虚拟内容可能被重写
        let other_changes = self.cache.close_document(uri)?;

        // 模块集合变化时重载虚拟项目工作区（main.rs 的模块聚合已变化）
        self.reload_if_modules_changed()?;

        // 模块集合缩小：其余条目仍在 rust-analyzer 中打开，
        // 用 didChange 全量同步（重复 didOpen 违反 LSP 协议）
        for change_entry in &other_changes {
            let ra_msg = json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didChange",
                "params": {
                    "textDocument": {
                        "uri": change_entry.virtual_uri,
                        "version": change_entry.version
                    },
                    "contentChanges": [{ "text": change_entry.en_content }]
                }
            });
            self.analyzer.send(&ra_msg)?;
        }
        Ok(())
    }

    /// 处理文档保存
    fn handle_did_save(&self, params: &Value) -> anyhow::Result<()> {
        let uri = params["textDocument"]["uri"].as_str().unwrap_or("");
        if !self.is_supported_file(uri) {
            return Ok(());
        }

        if let Some(entry) = self.cache.query_original(uri) {
            let ra_msg = json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didSave",
                "params": {
                    "textDocument": { "uri": entry.virtual_uri },
                    "text": entry.en_content
                }
            });
            self.analyzer.send(&ra_msg)?;
        }
        Ok(())
    }

    /// 通知 rust-analyzer 重新加载虚拟项目工作区
    ///
    /// 虚拟项目的 main.rs 聚合了新打开的 .zh 文件的模块，
    /// 但文件系统监听可能失败（notify error），
    /// 因此打开文档后显式触发 removed+added 重载，
    /// 让 rust-analyzer 重新扫描并识别新模块。
    fn reload_virtual_project(&self) -> anyhow::Result<()> {
        let uri = self.cache.virtual_project_uri();
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "workspace/didChangeWorkspaceFolders",
            "params": {
                "event": {
                    "removed": [{ "uri": uri, "name": "i18n-virtual" }],
                    "added": [{ "uri": uri, "name": "i18n-virtual" }]
                }
            }
        });
        self.analyzer.send(&notification)
    }

    /// 处理代码格式化请求（textDocument/formatting）
    ///
    /// 采用全文件替换策略，不转发 rust-analyzer：
    /// 取缓存中的英文译文 → rustfmt 格式化 → 反向翻译回母语 →
    /// 返回覆盖整个文档的 TextEdit。
    /// 文档未打开或 rustfmt 失败时返回空数组，不崩溃。
    fn handle_formatting(&self, req: Request) -> anyhow::Result<()> {
        let uri = req.params["textDocument"]["uri"].as_str().unwrap_or("");

        let edits = match self.cache.query_original(uri) {
            Some(entry) => {
                let tab_size = req.params["options"]["tabSize"].as_u64().unwrap_or(4);
                match run_rustfmt(&entry.en_content, tab_size) {
                    Some(formatted_en) => {
                        // 将格式化后的英文代码反向翻译为母语代码
                        let formatted_native = self.cache.reverse_transpile(&formatted_en);
                        vec![json!({
                            "range": {
                                "start": { "line": 0, "character": 0 },
                                "end": text_end_position(&entry.zh_content)
                            },
                            "newText": formatted_native
                        })]
                    }
                    None => Vec::new(),
                }
            }
            None => Vec::new(),
        };

        let response = Response {
            id: req.id,
            result: Some(Value::Array(edits)),
            error: None,
        };
        self.connection
            .sender
            .send(Message::Response(response))
            .map_err(|e| {
                anyhow::anyhow!(
                    "{}",
                    crate::ui::global().f("lsp_err_send_format", &[&e.to_string()])
                )
            })?;
        Ok(())
    }

    /// 转发请求到 rust-analyzer
    ///
    /// 除了将 URI 替换为虚拟文件 URI，还将请求中的位置
    /// （position/range）从母语坐标转换为英文坐标，
    /// 并将 rename 的 newName 翻译为英文。
    fn forward_request(&self, req: Request) -> anyhow::Result<()> {
        let ra_id = self
            .request_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let original_uri = req.params["textDocument"]["uri"]
            .as_str()
            .unwrap_or("")
            .to_string();

        {
            let mut pending = self
                .pending_requests
                .lock()
                .map_err(|_| anyhow::anyhow!("{}", crate::ui::global().t("lsp_err_lock")))?;
            pending.insert(
                ra_id,
                PendingRequestInfo {
                    original_id: req.id,
                    method: req.method.clone(),
                    original_uri: original_uri.clone(),
                    created_at: std::time::Instant::now(),
                },
            );
        }

        // 替换 URI 为虚拟 URI，并转换请求中的位置
        let mut params = req.params.clone();
        let entry = if self.is_supported_file(&original_uri) {
            self.cache.query_original(&original_uri)
        } else {
            None
        };

        if let Some(entry) = &entry {
            // 1. 替换 textDocument.uri
            if let Some(uri_field) = params
                .get_mut("textDocument")
                .and_then(|td| td.get_mut("uri"))
            {
                *uri_field = Value::String(entry.virtual_uri.clone());
            }

            // 2. 按方法转换位置参数（母语坐标 → 英文坐标）
            match req.method.as_str() {
                "textDocument/completion"
                | "textDocument/hover"
                | "textDocument/definition"
                | "textDocument/references"
                | "textDocument/rename"
                | "textDocument/documentHighlight"
                | "textDocument/signatureHelp" => {
                    if let Some(position) = params.get_mut("position") {
                        *position = position_to_en(&self.cache, entry, position);
                    }
                }
                "textDocument/codeAction" => {
                    if let Some(range) = params.get_mut("range") {
                        *range = range_to_en(&self.cache, entry, range);
                    }
                    // context.diagnostics 来自我们发布的中文诊断，同样需要转换
                    if let Some(diags_list) = params
                        .get_mut("context")
                        .and_then(|c| c.get_mut("diagnostics"))
                        .and_then(|d| d.as_array_mut())
                    {
                        for diag in diags_list.iter_mut() {
                            if let Some(range) = diag.get_mut("range") {
                                *range = range_to_en(&self.cache, entry, range);
                            }
                        }
                    }
                }
                _ => {}
            }

            // 3. rename 的 newName：中文 → 英文（不在关键字映射中则保持原样）
            if req.method == "textDocument/rename"
                && let Some(zh_name) = params.get("newName").and_then(|v| v.as_str())
            {
                let en_name = self
                    .cache
                    .keyword_map()
                    .get(zh_name)
                    .or_else(|| self.cache.alias_map().get(zh_name))
                    .cloned()
                    .unwrap_or_else(|| zh_name.to_string());
                params["newName"] = Value::String(en_name);
            }
        }

        let ra_msg = json!({
            "jsonrpc": "2.0",
            "id": ra_id,
            "method": req.method,
            "params": params
        });
        self.analyzer.send(&ra_msg)
    }
}

/// 判断 URI 是否以任一受支持的方言扩展名结尾
fn is_supported_file(uri: &str, extensions: &[String]) -> bool {
    extensions.iter().any(|ext| uri.ends_with(ext))
}

/// 计算虚拟项目临时目录（按用户 + 进程实例隔离）并拒绝符号链接
///
/// 固定共享的 /tmp 路径在多用户机器上可被预创建为符号链接，
/// 后续写文件会跟随链接覆写任意位置；按用户名隔离并校验规避此风险。
/// 同一用户的多个编辑器实例各起一个 LSP 代理进程，再叠加 PID 后缀
/// 避免互相同目录覆写 Cargo.toml / src 内容；启动时清理已死进程
/// 的残留目录（见 [`cleanup_stale_virtual_dirs`]）防止无限累积。
fn virtual_temp_dir() -> anyhow::Result<PathBuf> {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "default".to_string());
    let safe_user: String = user
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    cleanup_stale_virtual_dirs(&safe_user);
    let dir = std::env::temp_dir().join(format!(
        "i18n_lsp_virtual_{}_{}",
        safe_user,
        std::process::id()
    ));
    if dir
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        anyhow::bail!(
            "{}",
            crate::ui::global().f("lsp_err_temp_symlink", &[&dir.display().to_string()])
        );
    }
    Ok(dir)
}

/// 清理同用户的残留虚拟目录：仅删除名称中带 PID 后缀且进程已死的目录
///
/// 尽力而为：任何失败（目录列举失败、无法解析 PID、删除失败）都静默跳过，
/// 不影响本实例启动。存活检查对 Unix（kill 0 信号）与 Windows
/// （OpenProcess 语义的 tasklist 查询不可移植，退回 mtime 启发式）分别处理。
fn cleanup_stale_virtual_dirs(safe_user: &str) {
    let prefix = format!("i18n_lsp_virtual_{}_", safe_user);
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        // 仅处理本用户且带 PID 后缀的目录；无后缀的旧版目录不动（避免误删）
        let Some(pid_str) = name_str.strip_prefix(&prefix) else {
            continue;
        };
        if pid_str == std::process::id().to_string() {
            continue; // 当前进程自己的目录
        }
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        if process_alive(pid) {
            continue;
        }
        // 进程已死：目录是残留，尽力删除（失败静默）
        let path = entry.path();
        if path
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(true)
        {
            continue; // 符号链接不跟随删除
        }
        let _ = std::fs::remove_dir_all(&path);
    }
}

/// 判断 PID 是否存活（仅用于残留目录清理，误判代价低）
///
/// Unix：kill 0 信号探活；返回 -1（进程不存在或无权限）时保守视为存活，
/// 宁可残留目录下轮再清，也不误删活进程的文件。
fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        // 不实际发信号（sig=0），仅做存在性检查
        unsafe { kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        // 非 Unix 平台无廉价探活手段：保守视为存活，不删除
        let _ = pid;
        true
    }
}

/// 清理超时的待映射请求：向客户端应答错误，避免永久等待与条目泄漏
fn cleanup_expired_requests(
    pending: &Arc<std::sync::Mutex<HashMap<i64, PendingRequestInfo>>>,
    sender: &crossbeam_channel::Sender<Message>,
) {
    let now = std::time::Instant::now();
    let mut expired: Vec<lsp_server::RequestId> = Vec::new();
    {
        let mut map = match pending.lock() {
            Ok(m) => m,
            Err(_) => return,
        };
        map.retain(|_id, info| {
            if now.duration_since(info.created_at) > REQUEST_TIMEOUT {
                expired.push(info.original_id.clone());
                false
            } else {
                true
            }
        });
    }
    for id in expired {
        log::warn!("{}", crate::ui::global().t("lsp_warn_request_timeout"));
        let response = Response {
            id,
            result: None,
            error: Some(lsp_server::ResponseError {
                code: -32603,
                message: "rust-analyzer did not respond in time".to_string(),
                data: None,
            }),
        };
        let _ = sender.send(Message::Response(response));
    }
}

/// 调用系统 rustfmt 格式化英文代码
///
/// 通过 stdin 传入源码、stdout 取回格式化结果。
/// 新版 rustfmt（1.9+）不传文件参数时从 stdin 读取，需配合 `--emit stdout`；
/// rustfmt 不存在、源码含语法错误或输出非 UTF-8 时返回 None。
fn run_rustfmt(source: &str, tab_size: u64) -> Option<String> {
    let mut child = std::process::Command::new("rustfmt")
        .arg("--emit")
        .arg("stdout")
        .arg("--edition")
        .arg("2021")
        .arg("--config")
        .arg(format!("tab_spaces={}", tab_size.max(1)))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(source.as_bytes()).ok()?;
        // stdin 在此处 drop，关闭管道使 rustfmt 结束输出
    }
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// 计算母语文本末尾的 LSP 位置（最后一行、最后一列的 UTF-16 长度）
///
/// 用于生成覆盖整个文档的格式化 TextEdit 的 range 终点。
fn text_end_position(content: &str) -> Value {
    let line_count = content.matches('\n').count() as u64 + 1;
    let last_line = content.rsplit('\n').next().unwrap_or("");
    let col_count = last_line.chars().map(|c| c.len_utf16() as u64).sum::<u64>();
    json!({ "line": line_count - 1, "character": col_count })
}

/// 将增量变更（range + text）应用到母语文本
///
/// LSP 位置按 UTF-16 code unit 计数；越界位置钳制到行尾/文末，
/// 防御客户端发来异常 range 时 panic。
fn apply_incremental_change(content: &mut String, range: &Value, text: &str) {
    let start = lsp_position_to_offset(content, &range["start"]);
    let end = lsp_position_to_offset(content, &range["end"]).max(start);
    content.replace_range(start..end, text);
}

/// LSP 位置（line/character，UTF-16）→ 文本字节偏移
///
/// 行号越界钳制到最后一行；列号越界钳制到行尾（含换行符前）。
fn lsp_position_to_offset(content: &str, position: &Value) -> usize {
    let line = position["line"].as_u64().unwrap_or(0) as u32;
    let character = position["character"].as_u64().unwrap_or(0) as u32;
    let mut line_start = 0usize;
    for (当前行, (idx, _)) in content.match_indices('\n').enumerate() {
        if 当前行 as u32 == line {
            break;
        }
        line_start = idx + 1;
    }
    // 行内按 UTF-16 单元前进，列号用尽或到达行尾（不含换行符）即停
    let line_text = &content[line_start..];
    let mut utf16 = 0u32;
    for (i, c) in line_text.char_indices() {
        if c == '\n' || utf16 >= character {
            return line_start + i;
        }
        utf16 += c.len_utf16() as u32;
    }
    line_start + line_text.len()
}

/// 将 LSP 位置（position）从母语坐标转换为英文坐标
///
/// 当前翻译逐行替换关键字、行数保持不变（行映射为 1:1），
/// 因此仅列号需要按列偏移映射转换。
fn position_to_en(cache: &TranslationCache, entry: &TranslationEntry, position: &Value) -> Value {
    let line = position["line"].as_u64().unwrap_or(0) as u32;
    let col = position["character"].as_u64().unwrap_or(0) as u32;
    let en_col = cache.zh_col_to_en_col(&entry.virtual_uri, line, col);
    json!({ "line": line, "character": en_col })
}

/// 将 LSP 范围（range）从母语坐标转换为英文坐标
fn range_to_en(cache: &TranslationCache, entry: &TranslationEntry, range: &Value) -> Value {
    let start = position_to_en(cache, entry, &range["start"]);
    let end = position_to_en(cache, entry, &range["end"]);
    json!({ "start": start, "end": end })
}

/// 处理来自 rust-analyzer 的消息并转发给客户端
fn handle_analyzer_message(
    msg: &Value,
    mapper: &Arc<ResponseMapper>,
    sender: &crossbeam_channel::Sender<Message>,
    pending: &Arc<std::sync::Mutex<HashMap<i64, PendingRequestInfo>>>,
    reply_sender: &crate::analyzer::Sender,
) {
    if let Some(id) = msg.get("id").and_then(|v| v.as_i64()) {
        // 是响应
        let original_info = {
            let mut pending_map = match pending.lock() {
                Ok(m) => m,
                Err(_) => return,
            };
            pending_map.remove(&id)
        };

        if let Some(info) = original_info {
            // rust-analyzer 返回错误时（如文件不存在），原样透传给客户端
            if msg.get("error").is_some() {
                let error_value = msg["error"].clone();
                let response = Response {
                    id: info.original_id,
                    result: None,
                    error: Some(lsp_server::ResponseError {
                        code: error_value["code"].as_i64().unwrap_or(-32603) as i32,
                        message: error_value["message"]
                            .as_str()
                            .map(String::from)
                            .unwrap_or_else(|| crate::ui::global().t("lsp_internal_error")),
                        data: error_value.get("data").cloned(),
                    }),
                };
                let _ = sender.send(Message::Response(response));
                return;
            }

            let result = msg.get("result").cloned().unwrap_or(Value::Null);
            let mapped_result = match info.method.as_str() {
                "textDocument/completion" => {
                    mapper.map_completion_response(&result, &info.original_uri)
                }
                "textDocument/hover" => mapper.map_hover_response(&result, &info.original_uri),
                "textDocument/definition" => mapper.map_definition_response(&result),
                "textDocument/references" => mapper.map_references_response(&result),
                "textDocument/documentSymbol" => {
                    mapper.map_document_symbol_response(&result, &info.original_uri)
                }
                "textDocument/codeAction" => {
                    mapper.map_code_action_response(&result, &info.original_uri)
                }
                "codeAction/resolve" => mapper.map_code_action_resolve_response(&result),
                "textDocument/rename" => mapper.map_rename_response(&result),
                "textDocument/documentHighlight" => {
                    mapper.map_document_highlight_response(&result, &info.original_uri)
                }
                _ => result,
            };

            let response = Response {
                id: info.original_id,
                result: Some(mapped_result),
                error: None,
            };
            let _ = sender.send(Message::Response(response));
        } else if msg.get("method").and_then(|v| v.as_str()).is_some() {
            // 待映射表中没有对应记录：这是 rust-analyzer 主动发来的请求
            // （如 workspace/configuration、workspace/diagnostic/refresh）。
            // 必须把响应回发给 rust-analyzer 本身，否则它会一直等待。
            let method = msg["method"].as_str().unwrap_or("");
            let result = if method == "workspace/configuration" {
                // 规范要求返回与请求项等长的数组（每项用 null 表示默认配置），
                // 直接返回 null 会被 rust-analyzer 视为协议错误
                let count = msg["params"]["items"]
                    .as_array()
                    .map(|a| a.len())
                    .unwrap_or(0);
                Value::Array(vec![Value::Null; count])
            } else {
                Value::Null
            };
            let response = json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result
            });
            let _ = reply_sender.send(&response);
        }
    } else if let Some(method) = msg.get("method").and_then(|v| v.as_str()) {
        // 是通知
        match method {
            "textDocument/publishDiagnostics" => {
                if let Some(params) = msg.get("params") {
                    // 只转发虚拟母语文件的诊断：
                    // rust-analyzer 可能对其他项目文件发布诊断，
                    // 这些与母语代码无关，不应发给客户端。
                    let diag_uri = params["uri"].as_str().unwrap_or("");
                    if !mapper.is_virtual_uri(diag_uri) {
                        return;
                    }
                    let mapped = mapper.map_diagnostics(params);
                    let notification = Notification {
                        method: method.to_string(),
                        params: mapped,
                    };
                    let _ = sender.send(Message::Notification(notification));
                }
            }
            _ => {
                let notification = Notification {
                    method: method.to_string(),
                    params: msg.get("params").cloned().unwrap_or(Value::Null),
                };
                let _ = sender.send(Message::Notification(notification));
            }
        }
    }
}

/// 语言包三映射表：(关键字映射, 宏映射, 别名映射)
type LangPackMaps = (
    HashMap<String, String>,
    HashMap<String, String>,
    HashMap<String, String>,
);

/// 加载语言包：返回 (关键字映射, 宏映射, 别名映射)
///
/// 关键字与别名分离（与 CLI 统一管线对齐）：关键字在词法阶段无条件替换，
/// 标准库/第三方库标识符（别名）在词法转译后经声明位保护替换，
/// 避免用户声明与库别名撞名时被误替换（如 `让 新 = 5`）。
fn load_language_pack(lang_pack_path: &Path) -> anyhow::Result<LangPackMaps> {
    let mappings_path = lang_pack_path.join("映射表");
    if mappings_path.exists() {
        match mapping_source::load_keyword_mapping(lang_pack_path) {
            Ok(map) => return Ok((map, HashMap::new(), HashMap::new())),
            Err(e) => log::warn!(
                "{}",
                crate::ui::global().f("lsp_log_mappings_fallback", &[&e.to_string()])
            ),
        }
    }

    let keywords_path = lang_pack_path.join("keywords.toml");
    if keywords_path.exists() {
        // 复用 engine 统一加载器（与 CLI 完全同源）：关键字/别名分离、
        // stdlib 优先于第三方库、crates/*.toml 按文件名排序合并；
        // 模块路径映射 LSP 虚拟项目不使用，但随同一入口加载保持语义一致
        let manager =
            i18n_rust_engine::mapping_manager::MappingManager::load_from_dir(lang_pack_path)
                .map_err(|e| {
                    anyhow::anyhow!(
                        "{}",
                        crate::ui::global().f("lsp_err_load_keywords", &[&e.to_string()])
                    )
                })?;
        return Ok((
            manager.keyword_map.clone(),
            manager.get_macro_map(),
            manager.alias_map.clone(),
        ));
    }

    log::warn!("{}", crate::ui::global().t("lsp_warn_builtin_fallback"));
    Ok((
        mapping_source::create_builtin_keyword_mapping(),
        HashMap::new(),
        HashMap::new(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// 默认扩展名列表覆盖全部 11 个内置语言包，未知扩展名不匹配
    #[test]
    fn test_is_supported_file_defaults() {
        let extensions: Vec<String> = DEFAULT_EXTENSIONS.iter().map(|s| s.to_string()).collect();
        assert!(is_supported_file(
            "file:///project/src/main.zh",
            &extensions
        ));
        assert!(is_supported_file(
            "file:///project/src/main.en",
            &extensions
        ));
        assert!(is_supported_file(
            "file:///project/src/main.de",
            &extensions
        ));
        assert!(is_supported_file(
            "file:///project/src/main.ru",
            &extensions
        ));
        assert!(is_supported_file(
            "file:///project/src/main.ja",
            &extensions
        ));
        assert!(!is_supported_file(
            "file:///project/src/main.rs",
            &extensions
        ));
        assert!(!is_supported_file(
            "file:///project/src/main.xyz",
            &extensions
        ));
    }

    /// 自定义扩展名列表生效
    #[test]
    fn test_is_supported_file_custom() {
        let extensions = vec![".fr".to_string()];
        assert!(is_supported_file(
            "file:///project/src/main.fr",
            &extensions
        ));
        assert!(!is_supported_file(
            "file:///project/src/main.zh",
            &extensions
        ));
    }

    fn create_test_cache() -> (Arc<TranslationCache>, tempfile::TempDir) {
        let map = HashMap::from([
            ("函数".into(), "fn".into()),
            ("让".into(), "let".into()),
            ("可变".into(), "mut".into()),
        ]);
        let temp = tempfile::tempdir().unwrap();
        let cache = TranslationCache::new(
            map,
            HashMap::new(),
            HashMap::new(),
            temp.path().to_path_buf(),
        );
        (cache, temp)
    }

    #[test]
    fn test_position_to_en() {
        let (cache, _temp) = create_test_cache();
        let (entry, _) = cache
            .update_document("file:///test/main.zh", "让 x = 1;", 1)
            .unwrap();
        assert_eq!(entry.en_content, "let x = 1;");

        // 中文列 0（"让" 起点）→ 英文列 0
        let position = json!({ "line": 0, "character": 0 });
        let en = position_to_en(&cache, &entry, &position);
        assert_eq!(en["line"], 0);
        assert_eq!(en["character"], 0);

        // 中文列 3（"x" 末尾）→ 英文列 5（"让" 为 1 个 UTF-16 单元，"let" 占 3 列）
        let position = json!({ "line": 0, "character": 3 });
        let en = position_to_en(&cache, &entry, &position);
        assert_eq!(en["character"], 5);

        // 中文列 2（"x" 起点）→ 英文列 4
        let position = json!({ "line": 0, "character": 2 });
        let en = position_to_en(&cache, &entry, &position);
        assert_eq!(en["character"], 4);
    }

    #[test]
    fn test_range_to_en() {
        let (cache, _temp) = create_test_cache();
        let (entry, _) = cache
            .update_document("file:///test/main.zh", "让 x = 1;", 1)
            .unwrap();

        let range = json!({
            "start": { "line": 0, "character": 2 },
            "end": { "line": 0, "character": 3 }
        });
        let en = range_to_en(&cache, &entry, &range);
        assert_eq!(en["start"]["character"], 4);
        assert_eq!(en["end"]["character"], 5);
    }

    #[test]
    fn test_rename_full_chain() {
        // 模拟完整闭环：中文请求 → 转换转发 → rust-analyzer 响应 → 映射回母语
        let (cache, _temp) = create_test_cache();
        let mapper = ResponseMapper::new(cache.clone());
        let (entry, _) = cache
            .update_document("file:///test/main.zh", "函数 主() {}", 1)
            .unwrap();
        assert_eq!(entry.en_content, "fn 主() {}");

        // —— 请求方向（与 forward_request 相同的转换逻辑）——
        let request_params = json!({
            "textDocument": { "uri": "file:///test/main.zh" },
            "position": { "line": 0, "character": 0 },
            "newName": "函数"
        });

        let mut params = request_params.clone();
        // 1. URI 替换为虚拟 URI
        params["textDocument"]["uri"] = Value::String(entry.virtual_uri.clone());
        // 2. 位置转换为英文坐标
        params["position"] = position_to_en(&cache, &entry, &params["position"]);
        // 3. newName 中文 → 英文
        let en_name = cache.keyword_map().get("函数").cloned().unwrap();
        params["newName"] = Value::String(en_name);

        assert_eq!(
            params["textDocument"]["uri"].as_str().unwrap(),
            entry.virtual_uri
        );
        assert_eq!(params["position"]["character"], 0);
        assert_eq!(params["newName"].as_str().unwrap(), "fn");

        // —— 响应方向（rust-analyzer 返回虚拟文件编辑）——
        let response = json!({
            "changes": {
                entry.virtual_uri.clone(): [{
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 2 }
                    },
                    "newText": "fn"
                }]
            }
        });
        let mapped = mapper.map_rename_response(&response);
        let edit = &mapped["changes"]["file:///test/main.zh"][0];

        // 客户端收到的编辑：URI 还原、位置为母语坐标、newText 恢复中文
        assert_eq!(edit["range"]["start"]["character"], 0);
        assert_eq!(edit["range"]["end"]["character"], 2);
        assert_eq!(edit["newText"].as_str().unwrap(), "函数");
    }

    #[test]
    fn test_text_end_position() {
        // 空文档
        assert_eq!(text_end_position(""), json!({ "line": 0, "character": 0 }));
        // 单行 ASCII
        assert_eq!(
            text_end_position("ab"),
            json!({ "line": 0, "character": 2 })
        );
        // 中文按 UTF-16 计数（4 个汉字 = 4 个单元）
        assert_eq!(
            text_end_position("行\n中文测试"),
            json!({ "line": 1, "character": 4 })
        );
        // 末尾换行：最后一行是空行
        assert_eq!(
            text_end_position("行\n"),
            json!({ "line": 1, "character": 0 })
        );
    }

    #[test]
    fn test_lsp_position_to_offset() {
        let text = "让 可变 x = 5;\n让 y = 10;";
        // 首行中文列：「可变」起点（列 2）
        let pos = json!({ "line": 0, "character": 2 });
        assert_eq!(lsp_position_to_offset(text, &pos), "让 ".len());
        // 第二行起点
        let pos = json!({ "line": 1, "character": 0 });
        assert_eq!(lsp_position_to_offset(text, &pos), "让 可变 x = 5;\n".len());
        // 列号越界钳制到行尾（不含换行符）
        let pos = json!({ "line": 0, "character": 999 });
        assert_eq!(lsp_position_to_offset(text, &pos), "让 可变 x = 5;".len());
        // 行号越界钳制到最后一行
        let pos = json!({ "line": 99, "character": 2 });
        assert_eq!(
            lsp_position_to_offset(text, &pos),
            "让 可变 x = 5;\n让 ".len()
        );
    }

    #[test]
    fn test_apply_incremental_change() {
        // 中文行内插入
        let mut text = "让 x = 5;".to_string();
        let range = json!({
            "start": { "line": 0, "character": 2 },
            "end": { "line": 0, "character": 2 }
        });
        apply_incremental_change(&mut text, &range, "可变 ");
        assert_eq!(text, "让 可变 x = 5;");

        // 跨行删除替换
        let mut text = "行一\n行二\n行三".to_string();
        let range = json!({
            "start": { "line": 0, "character": 1 },
            "end": { "line": 1, "character": 1 }
        });
        apply_incremental_change(&mut text, &range, "新");
        // 删除「一\n行」并插入「新」
        assert_eq!(text, "行新二\n行三");

        // 异常 range（end < start）不 panic，退化为插入
        let mut text = "abc".to_string();
        let range = json!({
            "start": { "line": 0, "character": 3 },
            "end": { "line": 0, "character": 1 }
        });
        apply_incremental_change(&mut text, &range, "X");
        assert_eq!(text, "abcX");
    }
}
