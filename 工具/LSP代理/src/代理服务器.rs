//! 代理服务器模块
//!
//! LSP 代理服务器的核心逻辑：
//! 1. 通过 lsp-server 与编辑器客户端建立 LSP 连接
//! 2. 接收 .zh 文件变更，翻译后通知 rust-analyzer
//! 3. 转发 rust-analyzer 的响应/通知，并还原位置信息
//! 4. 翻译诊断消息为中文

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use lsp_server::{Connection, Message, Notification, Request, Response};
use serde_json::{json, Value};

use i18n_rust_engine::映射源;

use crate::翻译缓存::翻译缓存;
use crate::分析器连接::分析器连接;
use crate::响应映射::响应映射器;

/// LSP 代理服务器
pub struct 代理服务器 {
    /// 与编辑器客户端的 LSP 连接
    连接: Connection,
    /// 翻译缓存
    缓存: Arc<翻译缓存>,
    /// rust-analyzer 子进程
    分析器: 分析器连接,
    /// 响应映射器
    映射器: Arc<响应映射器>,
    /// 自增请求 ID
    请求计数器: Arc<std::sync::atomic::AtomicI64>,
    /// 待映射的 rust-analyzer 请求 ID → 原始客户端请求信息
    待映射请求: Arc<std::sync::Mutex<HashMap<i64, 待映射信息>>>,
}

/// 记录一个转发给 rust-analyzer 的请求的原始信息
#[derive(Debug, Clone)]
struct 待映射信息 {
    原始ID: lsp_server::RequestId,
    方法: String,
    原始URI: String,
}

impl 代理服务器 {
    /// 创建并初始化代理服务器
    pub fn 新建(语言包路径: &PathBuf) -> anyhow::Result<(Self, lsp_server::IoThreads)> {
        // 1. 加载语言包
        let (关键字映射, 宏名称集合) = 加载语言包(语言包路径)?;
        log::info!("已加载 {} 个关键字映射，{} 个宏名称", 关键字映射.len(), 宏名称集合.len());

        // 2. 创建翻译缓存
        let 临时目录 = std::env::temp_dir().join("i18n_lsp_virtual");
        let 缓存 = 翻译缓存::新建(关键字映射, 宏名称集合, 临时目录);

        // 3. 启动 rust-analyzer
        let 分析器 = 分析器连接::启动()?;

        // 4. 创建响应映射器
        let 映射器 = Arc::new(响应映射器::新建(缓存.clone()));

        // 5. 建立 LSP 连接（stdio）
        let (连接, io线程) = Connection::stdio();

        let 服务器 = Self {
            连接,
            缓存,
            分析器,
            映射器,
            请求计数器: Arc::new(std::sync::atomic::AtomicI64::new(1000)),
            待映射请求: Arc::new(std::sync::Mutex::new(HashMap::new())),
        };

        Ok((服务器, io线程))
    }

    /// 运行服务器主循环
    pub fn 运行(self, io线程: lsp_server::IoThreads) -> anyhow::Result<()> {
        // 1. 等待 initialize 请求并握手
        let (初始化ID, 初始化参数) = self.握手()?;
        log::info!("客户端初始化完成");

        // 2. 初始化 rust-analyzer
        self.初始化分析器(&初始化参数)?;

        // 3. 回复客户端 initialize
        self.回复初始化(初始化ID)?;

        // 4. 启动 rust-analyzer 消息转发线程
        self.启动转发线程()?;

        // 5. 进入主循环
        self.主循环()?;

        // 6. 等待 IO 线程结束
        io线程.join()?;

        Ok(())
    }

    /// 握手：接收 initialize 请求并回复
    fn 握手(&self) -> anyhow::Result<(lsp_server::RequestId, Value)> {
        let msg = self.连接.receiver.recv()
            .map_err(|e| anyhow::anyhow!("接收 initialize 请求失败: {}", e))?;

        match msg {
            Message::Request(req) => {
                if req.method == "initialize" {
                    let id = req.id.clone();
                    let params = req.params.clone();
                    Ok((id, params))
                } else {
                    anyhow::bail!("期望 initialize 请求，收到: {}", req.method)
                }
            }
            _ => anyhow::bail!("期望 Request，收到其他类型消息"),
        }
    }

    /// 向 rust-analyzer 发送 initialize 并等待响应
    fn 初始化分析器(&self, 参数: &Value) -> anyhow::Result<()> {
        // 透传客户端的 rootUri 和 workspaceFolders
        let root_uri = 参数.get("rootUri").cloned().unwrap_or(Value::Null);
        let workspace_folders = 参数.get("workspaceFolders").cloned().unwrap_or(Value::Null);

        let 初始化请求 = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "completion": {
                            "completionItem": { "snippetSupport": false }
                        },
                        "publishDiagnostics": {
                            "relatedInformation": true
                        }
                    }
                },
                "workspaceFolders": workspace_folders
            }
        });

        self.分析器.发送(&初始化请求)?;

        // 等待 rust-analyzer 的 initialize 响应
        for _ in 0..200 {
            if let Some(响应) = self.分析器.尝试接收() {
                if 响应.get("id").and_then(|v| v.as_i64()) == Some(0) {
                    log::info!("rust-analyzer 初始化完成");
                    break;
                }
            }
            thread::sleep(std::time::Duration::from_millis(50));
        }

        // 发送 initialized 通知
        let 通知 = json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });
        self.分析器.发送(&通知)?;

        Ok(())
    }

    /// 回复客户端 initialize 响应
    fn 回复初始化(&self, id: lsp_server::RequestId) -> anyhow::Result<()> {
        let 能力 = json!({
            "capabilities": {
                "textDocumentSync": {
                    "openClose": true,
                    "change": 1,
                    "save": { "includeText": true }
                },
                "completionProvider": {
                    "triggerCharacters": [".", ":"]
                },
                "hoverProvider": true,
                "definitionProvider": true,
                "referencesProvider": true,
                "documentSymbolProvider": true,
                "codeActionProvider": true
            },
            "serverInfo": {
                "name": "i18n-rust-lsp",
                "version": "0.1.0"
            }
        });

        let 响应 = Response {
            id,
            result: Some(能力),
            error: None,
        };

        self.连接.sender.send(Message::Response(响应))
            .map_err(|e| anyhow::anyhow!("发送 initialize 响应失败: {}", e))?;
        Ok(())
    }

    /// 启动 rust-analyzer → 客户端的消息转发线程
    fn 启动转发线程(&self) -> anyhow::Result<()> {
        let 接收端 = self.分析器.消息通道();
        let 映射器 = self.映射器.clone();
        let 发送端 = self.连接.sender.clone();
        let 待映射 = self.待映射请求.clone();

        thread::spawn(move || {
            while let Ok(消息) = 接收端.recv() {
                处理分析器消息(&消息, &映射器, &发送端, &待映射);
            }
            log::info!("rust-analyzer 转发线程已退出");
        });

        Ok(())
    }

    /// 主消息循环
    fn 主循环(&self) -> anyhow::Result<()> {
        loop {
            let msg = match self.连接.receiver.recv() {
                Ok(msg) => msg,
                Err(_) => {
                    log::info!("客户端连接已断开");
                    break;
                }
            };

            match msg {
                Message::Request(req) => {
                    if req.method == "shutdown" {
                        log::info!("收到 shutdown 请求");
                        let 响应 = Response {
                            id: req.id,
                            result: Some(Value::Null),
                            error: None,
                        };
                        let _ = self.连接.sender.send(Message::Response(响应));
                        break;
                    }
                    self.处理客户端请求(req)?;
                }
                Message::Notification(notif) => {
                    self.处理客户端通知(notif)?;
                }
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    /// 处理客户端请求
    fn 处理客户端请求(&self, req: Request) -> anyhow::Result<()> {
        log::debug!("客户端请求: {} id={:?}", req.method, req.id);

        match req.method.as_str() {
            "initialize" => Ok(()), // 已在握手中处理
            "textDocument/completion" |
            "textDocument/hover" |
            "textDocument/definition" |
            "textDocument/references" |
            "textDocument/documentSymbol" |
            "textDocument/codeAction" => {
                self.转发请求(req)
            }
            _ => self.转发请求(req),
        }
    }

    /// 处理客户端通知
    fn 处理客户端通知(&self, notif: Notification) -> anyhow::Result<()> {
        log::debug!("客户端通知: {}", notif.method);

        match notif.method.as_str() {
            "textDocument/didOpen" => self.处理文档打开(&notif.params),
            "textDocument/didChange" => self.处理文档变更(&notif.params),
            "textDocument/didClose" => self.处理文档关闭(&notif.params),
            "textDocument/didSave" => self.处理文档保存(&notif.params),
            _ => {
                let 消息 = json!({
                    "jsonrpc": "2.0",
                    "method": notif.method,
                    "params": notif.params
                });
                self.分析器.发送(&消息)
            }
        }
    }

    /// 处理文档打开
    fn 处理文档打开(&self, 参数: &Value) -> anyhow::Result<()> {
        let 文档 = &参数["textDocument"];
        let URI = 文档["uri"].as_str().unwrap_or("");
        let 内容 = 文档["text"].as_str().unwrap_or("");
        let 版本 = 文档["version"].as_i64().unwrap_or(1) as i32;

        if !URI.ends_with(".zh") { return Ok(()); }

        let 条目 = self.缓存.更新文档(URI, 内容, 版本)?;

        let ra消息 = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": 条目.虚拟URI,
                    "languageId": "rust",
                    "version": 版本,
                    "text": 条目.英文内容
                }
            }
        });
        self.分析器.发送(&ra消息)?;
        log::info!("文档已打开并翻译: {}", URI);
        Ok(())
    }

    /// 处理文档变更
    fn 处理文档变更(&self, 参数: &Value) -> anyhow::Result<()> {
        let 文档 = &参数["textDocument"];
        let URI = 文档["uri"].as_str().unwrap_or("");
        let 版本 = 文档["version"].as_i64().unwrap_or(1) as i32;

        if !URI.ends_with(".zh") { return Ok(()); }

        if let Some(变更列表) = 参数["contentChanges"].as_array() {
            if let Some(最后一个) = 变更列表.last() {
                if let Some(内容) = 最后一个["text"].as_str() {
                    let 条目 = self.缓存.更新文档(URI, 内容, 版本)?;
                    let ra消息 = json!({
                        "jsonrpc": "2.0",
                        "method": "textDocument/didChange",
                        "params": {
                            "textDocument": {
                                "uri": 条目.虚拟URI,
                                "version": 版本
                            },
                            "contentChanges": [{ "text": 条目.英文内容 }]
                        }
                    });
                    self.分析器.发送(&ra消息)?;
                }
            }
        }
        Ok(())
    }

    /// 处理文档关闭
    fn 处理文档关闭(&self, 参数: &Value) -> anyhow::Result<()> {
        let URI = 参数["textDocument"]["uri"].as_str().unwrap_or("");
        if !URI.ends_with(".zh") { return Ok(()); }

        if let Some(条目) = self.缓存.查询原始(URI) {
            let ra消息 = json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didClose",
                "params": {
                    "textDocument": { "uri": 条目.虚拟URI }
                }
            });
            self.分析器.发送(&ra消息)?;
        }
        self.缓存.关闭文档(URI)?;
        Ok(())
    }

    /// 处理文档保存
    fn 处理文档保存(&self, 参数: &Value) -> anyhow::Result<()> {
        let URI = 参数["textDocument"]["uri"].as_str().unwrap_or("");
        if !URI.ends_with(".zh") { return Ok(()); }

        if let Some(条目) = self.缓存.查询原始(URI) {
            let ra消息 = json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didSave",
                "params": {
                    "textDocument": { "uri": 条目.虚拟URI },
                    "text": 条目.英文内容
                }
            });
            self.分析器.发送(&ra消息)?;
        }
        Ok(())
    }

    /// 转发请求到 rust-analyzer
    fn 转发请求(&self, req: Request) -> anyhow::Result<()> {
        let ra_id = self.请求计数器.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let 原始URI = req.params["textDocument"]["uri"]
            .as_str().unwrap_or("").to_string();

        {
            let mut 映射 = self.待映射请求.lock()
                .map_err(|_| anyhow::anyhow!("待映射请求锁获取失败"))?;
            映射.insert(ra_id, 待映射信息 {
                原始ID: req.id,
                方法: req.method.clone(),
                原始URI,
            });
        }

        // 替换 URI 为虚拟 URI
        let mut 参数 = req.params.clone();
        if let Some(uri字段) = 参数.get_mut("textDocument").and_then(|td| td.get_mut("uri")) {
            if let Some(URI) = uri字段.as_str() {
                if URI.ends_with(".zh") {
                    if let Some(条目) = self.缓存.查询原始(URI) {
                        *uri字段 = Value::String(条目.虚拟URI);
                    }
                }
            }
        }

        let ra消息 = json!({
            "jsonrpc": "2.0",
            "id": ra_id,
            "method": req.method,
            "params": 参数
        });
        self.分析器.发送(&ra消息)
    }
}

/// 处理来自 rust-analyzer 的消息并转发给客户端
fn 处理分析器消息(
    消息: &Value,
    映射器: &Arc<响应映射器>,
    发送端: &crossbeam_channel::Sender<Message>,
    待映射: &Arc<std::sync::Mutex<HashMap<i64, 待映射信息>>>,
) {
    if let Some(id) = 消息.get("id").and_then(|v| v.as_i64()) {
        // 是响应
        let 原始信息 = {
            let mut 映射表 = match 待映射.lock() {
                Ok(m) => m,
                Err(_) => return,
            };
            映射表.remove(&id)
        };

        if let Some(info) = 原始信息 {
            let 结果 = 消息.get("result").cloned().unwrap_or(Value::Null);
            let 映射后结果 = match info.方法.as_str() {
                "textDocument/completion" => 映射器.映射补全响应(&结果),
                "textDocument/hover" => 映射器.映射悬停响应(&结果),
                "textDocument/definition" => 映射器.映射定义响应(&结果),
                "textDocument/references" => 映射器.映射引用响应(&结果),
                _ => 结果,
            };

            let 响应 = Response {
                id: info.原始ID,
                result: Some(映射后结果),
                error: None,
            };
            let _ = 发送端.send(Message::Response(响应));
        }
    } else if let Some(method) = 消息.get("method").and_then(|v| v.as_str()) {
        // 是通知
        match method {
            "textDocument/publishDiagnostics" => {
                if let Some(参数) = 消息.get("params") {
                    let 映射后 = 映射器.映射诊断(参数);
                    let 通知 = Notification {
                        method: method.to_string(),
                        params: 映射后,
                    };
                    let _ = 发送端.send(Message::Notification(通知));
                }
            }
            _ => {
                let 通知 = Notification {
                    method: method.to_string(),
                    params: 消息.get("params").cloned().unwrap_or(Value::Null),
                };
                let _ = 发送端.send(Message::Notification(通知));
            }
        }
    }
}

/// 加载语言包
fn 加载语言包(语言包路径: &PathBuf) -> anyhow::Result<(HashMap<String, String>, HashSet<String>)> {
    let 映射表路径 = 语言包路径.join("映射表");
    if 映射表路径.exists() {
        match 映射源::加载关键字映射(语言包路径) {
            Ok(映射) => return Ok((映射, HashSet::new())),
            Err(e) => log::warn!("从映射表加载失败: {}, 使用备用", e),
        }
    }

    let 关键字路径 = 语言包路径.join("关键字.toml");
    if 关键字路径.exists() {
        let 管理器 = i18n_rust_engine::映射管理::映射管理器::从文件加载(&关键字路径)
            .map_err(|e| anyhow::anyhow!("加载关键字失败: {}", e))?;
        let 宏集合 = 管理器.获取宏名称集合();
        return Ok((管理器.关键字映射.clone(), 宏集合));
    }

    log::warn!("未找到语言包文件，使用内置关键字映射");
    Ok((映射源::创建内置关键字映射(), HashSet::new()))
}
