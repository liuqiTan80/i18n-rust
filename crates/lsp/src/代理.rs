//! 代理服务器模块
//!
//! LSP 代理服务器的核心逻辑：
//! 1. 通过 lsp-服务器实例 与编辑器客户端建立 LSP 连接
//! 2. 接收方言文件（.zh/.de 等）变更，翻译后通知 rust-analyzer
//! 3. 转发 rust-analyzer 的响应/通知，并还原位置信息
//! 4. 翻译诊断消息为对应语言

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use lsp_server::{Connection, Message, Notification, Request, Response};
use serde_json::{Value, json};

use i18n_rust_engine::映射源;

use crate::分析器::分析器连接;
use crate::响应映射::响应映射器;
use crate::翻译缓存::{翻译条目, 路径转定位, 转译缓存};

/// 默认支持的方言文件扩展名（与内置语言包 lang_info.toml 的扩展名一致，
/// 单一来源：引擎 lang-packs 目录，避免清单与语言包漂移）；
/// 可通过命令行 `--extensions` 参数覆盖
fn 默认扩展名() -> Vec<String> {
    i18n_rust_engine::语言::内置语言扩展()
        .into_iter()
        .map(|条目项| format!(".{条目项}"))
        .collect()
}

/// LSP 代理服务器
pub struct 代理服务器 {
    /// 与编辑器客户端的 LSP 连接
    客户端连接: Connection,
    /// 翻译缓存
    缓存: Arc<转译缓存>,
    /// rust-analyzer 子进程
    分析器实例: 分析器连接,
    /// 响应映射器
    映射器实例: Arc<响应映射器>,
    /// 自增请求 ID
    请求计数器: Arc<std::sync::atomic::AtomicI64>,
    /// 待映射的 rust-analyzer 请求 ID → 原始客户端请求信息
    待处理请求表: Arc<std::sync::Mutex<HashMap<i64, 待处理请求信息>>>,
    /// 支持的方言文件扩展名列表（如 `.zh`、`.de`）
    受支持扩展名: Vec<String>,
    /// 上次重载时的模块集合版本号（初值 -1 保证首个文档打开时重载一次）
    上次模块版本: std::sync::atomic::AtomicI64,
    /// rust-analyzer 声明的语义着色能力（initialize 响应透传给客户端，
    /// 保证 legend 与 token 类型索引一致，否则变量等语义着色错乱）
    语义令牌提供者配置: std::sync::Mutex<Option<Value>>,
    /// 最近一次转发的内置诊断（方言 uri → 诊断列表）：代理自跑 cargo check
    /// 后合并发布，避免 check 结果覆盖语法/类型等实时诊断
    内置诊断: Arc<std::sync::Mutex<HashMap<String, Vec<Value>>>>,
    /// cargo check 是否在运行（didSave 频繁时跳过进行中的 check，避免并发卡锁）
    检查进行中: Arc<std::sync::atomic::AtomicBool>,
    /// 运行期间是否收到新的保存请求（check 完成后补跑一次，
    /// 合并连续保存的中间状态，避免最终诊断停留在旧版本）
    检查待补跑: Arc<std::sync::atomic::AtomicBool>,
}

/// 记录一个转发给 rust-analyzer 的请求的原始信息
#[derive(Debug, Clone)]
struct 待处理请求信息 {
    原始请求号: lsp_server::RequestId,
    method: String,
    原始资源定位: String,
    /// 转发时刻（用于超时清理，避免响应永不到达时条目无限累积）
    创建时刻: std::time::Instant,
    /// codeAction 请求上下文诊断中提取的未声明 crate 名
    ///（响应时注入“添加依赖”快捷修复；非 codeAction 请求为空）
    未解析依赖: Vec<String>,
    /// codeAction 请求上下文中的教学诊断（全角标点/教学 lint，方言坐标）：
    /// 响应时注入对应快捷修复（一键替换半角/忽略此行）；非 codeAction 请求为空
    教学诊断: Vec<Value>,
}

/// 转发请求的等待超时：超过后向客户端应答错误并丢弃条目
const 请求超时: std::time::Duration = std::time::Duration::from_secs(60);

impl 代理服务器 {
    /// 创建并初始化代理服务器
    ///
    /// `extensions`: 支持的方言文件扩展名列表（如 `.zh`），
    /// 传空列表时使用默认值（`.zh` / `.de`）。
    pub fn 装配实例(
        语言包路径: &Path,
        扩展名列表: &[String],
    ) -> anyhow::Result<(Self, lsp_server::IoThreads)> {
        // 1. 加载语言包（统一映射管理器，含模块路径映射）
        let 管理器 = 加载语言包(语言包路径)?;
        // 初始化诊断翻译器（errors.toml 消息表 + 由映射管理器构建的 type_map，
        // 与 CLI 同源），供 map_diagnostics/镜像检查翻译 rust-analyzer 诊断（E0004 等）
        crate::响应映射::初始化诊断翻译器(语言包路径, &管理器);
        log::info!(
            "{}",
            crate::本地化::全局().取文带参(
                "lsp_log_loaded_mappings",
                &[
                    &管理器.关键词映射表.len().to_string(),
                    &管理器.取宏映射表().len().to_string()
                ]
            )
        );

        // 2. 创建翻译缓存（临时目录按用户隔离，避免多用户共享 /tmp 路径）
        let 临时根路径 = 虚拟临时目录()?;
        let 缓存 = 转译缓存::新建缓存(管理器, 临时根路径);

        // 3. 启动 rust-analyzer
        let 分析器实例 = 分析器连接::启动服务()?;

        // 4. 创建响应映射器
        let 映射器实例 = Arc::new(响应映射器::新建映射器(缓存.clone()));

        // 5. 建立 LSP 连接（stdio）
        let (客户端连接, 输入线程) = Connection::stdio();

        let 服务器实例 = Self {
            客户端连接,
            缓存,
            分析器实例,
            映射器实例,
            请求计数器: Arc::new(std::sync::atomic::AtomicI64::new(1000)),
            待处理请求表: Arc::new(std::sync::Mutex::new(HashMap::new())),
            受支持扩展名: if 扩展名列表.is_empty() {
                默认扩展名()
            } else {
                扩展名列表.to_vec()
            },
            上次模块版本: std::sync::atomic::AtomicI64::new(-1),
            语义令牌提供者配置: std::sync::Mutex::new(None),
            内置诊断: Arc::new(std::sync::Mutex::new(HashMap::new())),
            检查进行中: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            检查待补跑: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };

        Ok((服务器实例, 输入线程))
    }

    /// 运行服务器主循环
    pub fn 服务循环(self, 输入线程: lsp_server::IoThreads) -> anyhow::Result<()> {
        // 1. 等待 initialize 请求并握手
        let (初始化标识, 初始化参数) = self.握手()?;
        log::info!("{}", crate::本地化::全局().取文("lsp_log_client_init"));

        // 2. 初始化 rust-analyzer
        self.初始化分析器(&初始化参数)?;

        // 3. 回复客户端 initialize
        self.应答初始化(初始化标识)?;

        // 4. 启动 rust-analyzer 消息转发线程
        self.启动转发线程()?;

        // 5. 进入主循环
        self.主循环()?;

        // 6. 主循环结束后（收到 shutdown 或客户端断开）清理资源。
        // 消息转发线程持有客户端发送端的克隆，并阻塞在 rust-analyzer
        // 的消息通道上；必须先停止 rust-analyzer 子进程让转发线程退出，
        // 再释放服务器（含客户端连接发送端），否则 IO 写入线程
        // 因通道永不关闭而无法结束，进程将无法退出。
        let 服务器实例 = self;
        服务器实例.分析器实例.停止进程();
        drop(服务器实例);

        // 7. 等待 IO 线程结束
        输入线程.join()?;

        Ok(())
    }

    /// 握手：接收 initialize 请求并回复
    fn 握手(&self) -> anyhow::Result<(lsp_server::RequestId, Value)> {
        let 消息体 = self.客户端连接.receiver.recv().map_err(|条目项| {
            anyhow::anyhow!(
                "{}",
                crate::本地化::全局()
                    .取文带参("lsp_err_recv_initialize", &[&条目项.to_string()])
            )
        })?;

        match 消息体 {
            Message::Request(请求对象) => {
                if 请求对象.method == "initialize" {
                    let 请求编号 = 请求对象.id.clone();
                    let 请求参数体 = 请求对象.params.clone();
                    Ok((请求编号, 请求参数体))
                } else {
                    anyhow::bail!(
                        "{}",
                        crate::本地化::全局()
                            .取文带参("lsp_err_expect_initialize", &[&请求对象.method])
                    )
                }
            }
            _ => anyhow::bail!("{}", crate::本地化::全局().取文("lsp_err_expect_request")),
        }
    }

    /// 向 rust-analyzer 发送 initialize 并等待响应
    fn 初始化分析器(&self, 请求参数体: &Value) -> anyhow::Result<()> {
        // 不透传客户端的 rootUri/workspaceFolders：
        // 客户端工作区（如整个 zrRust 项目）与母语文件无关，
        // 透传会导致 rust-analyzer 全量分析并发布海量诊断，
        // 阻塞代理到客户端的消息通道。
        // 改用虚拟项目目录作为 rust-analyzer 的工作区根，
        // 使其只分析自动生成的虚拟 .rs 文件。
        let _ = 请求参数体;
        let 虚拟项目定位 = self.缓存.虚拟项目资源定位();

        let 初始化请求 = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": 虚拟项目定位,
                "capabilities": {
                    "textDocument": {
                        "completion": {
                            "completionItem": {
                                // 启用 snippet：rust-analyzer 的方法/关键字补全
                                // 依赖 snippet 才能附带括号（如 `长度()`）与占位符
                                "snippetSupport": true,
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
                    "uri": 虚拟项目定位,
                    "name": "i18n-virtual"
                }],
                // 虚拟项目无依赖/无构建脚本/无过程宏：关闭相应后台任务，
                // 减少启动与保存时的 cargo 开销；诊断（checkOnSave）保留
                "initializationOptions": {
                    "cargo": { "buildScripts": { "enable": false } },
                    "procMacro": { "enable": false },
                    // 注：不启用 rust-analyzer 的 checkOnSave——其诊断在虚拟项目
                    // 上不可达（cargo 常驻但无诊断发布）；改为代理在 didSave 时
                    // 自跑 cargo check 并发布（见 触发cargo检查）
                    // custom snippet：方法/关键字补全附带括号与占位符
                    //（如 `长度()`、`让 可变 ${1:x}`），配合 snippetSupport 使用
                    "completion": { "snippets": "custom" }
                }
            }
        });

        self.分析器实例.send(&初始化请求)?;

        // 等待 rust-analyzer 的 initialize 响应（超时则直接报错，
        // 避免在半初始化状态下继续服务导致行为不可预测）
        let mut 初始化完成 = false;
        for _ in 0..200 {
            if let Some(消息体) = self.分析器实例.try_recv() {
                if 消息体.get("id").and_then(|值项| 值项.as_i64()) == Some(0) {
                    // 保存 rust-analyzer 的语义着色能力声明：initialize 响应
                    // 透传给客户端，保证 legend 与 token 类型索引一致
                    // （否则变量/参数等语义 token 的类型索引错位，着色乱或不显示）
                    let 令牌提供者 =
                        消息体["result"]["capabilities"]["semanticTokensProvider"].clone();
                    if !令牌提供者.is_null()
                        && let Ok(mut 槽位) = self.语义令牌提供者配置.lock()
                    {
                        *槽位 = Some(令牌提供者);
                    }
                    log::info!("{}", crate::本地化::全局().取文("lsp_log_ra_init_done"));
                    初始化完成 = true;
                    break;
                }
                // 初始化期间 rust-analyzer 可能主动请求配置（workspace/configuration）：
                // 必须立即响应，否则它一直等待导致配置加载挂起。
                // 返回补全 snippet 配置并禁用其自跑 cargo check（见
                // [`分析器配置结果`] 的说明），其余键维持默认。
                if 消息体.get("method").and_then(|值项| 值项.as_str())
                    == Some("workspace/configuration")
                {
                    let 数量 = 消息体["params"]["items"]
                        .as_array()
                        .map(|数组项| 数组项.len())
                        .unwrap_or(0);
                    let 应答结果 = 分析器配置结果(数量);
                    let 响应体 = json!({
                        "jsonrpc": "2.0",
                        "id": 消息体["id"].clone(),
                        "result": 应答结果
                    });
                    if let Err(条目项) = self.分析器实例.send(&响应体) {
                        log::warn!("配置响应发送失败: {条目项}");
                    }
                }
            }
            thread::sleep(std::time::Duration::from_millis(50));
        }
        if !初始化完成 {
            anyhow::bail!("{}", crate::本地化::全局().取文("lsp_err_ra_init_timeout"));
        }

        // 注意：initialized 通知不在本处发送，
        // 而是等客户端发来 initialized 时再转发（见 处理客户端通知），
        // 避免重复发送导致 rust-analyzer 报 unhandled notification。

        Ok(())
    }

    /// 回复客户端 initialize 响应
    fn 应答初始化(&self, 请求编号: lsp_server::RequestId) -> anyhow::Result<()> {
        let mut 能力表 = json!({
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
        // 透传 rust-analyzer 的语义着色能力（含 legend），
        // 使客户端请求 semanticTokens 并正确渲染变量/参数等颜色
        if let Ok(槽位) = self.语义令牌提供者配置.lock()
            && let Some(令牌提供者) = 槽位.as_ref()
        {
            能力表["capabilities"]["semanticTokensProvider"] = 令牌提供者.clone();
        }

        let 响应体 = Response {
            id: 请求编号,
            result: Some(能力表),
            error: None,
        };

        self.客户端连接
            .sender
            .send(Message::Response(响应体))
            .map_err(|条目项| {
                anyhow::anyhow!(
                    "{}",
                    crate::本地化::全局()
                        .取文带参("lsp_err_send_initialize", &[&条目项.to_string()])
                )
            })?;
        Ok(())
    }

    /// 启动 rust-analyzer → 客户端的消息转发线程
    fn 启动转发线程(&self) -> anyhow::Result<()> {
        let 接收端 = self.分析器实例.取消息通道();
        let 映射器实例 = self.映射器实例.clone();
        let 发送端 = self.客户端连接.sender.clone();
        let 待处理项 = self.待处理请求表.clone();
        // rust-analyzer 主动请求（如 workspace/diagnostic/refresh）
        // 的响应需要回发给 rust-analyzer 本身，而非客户端。
        let 分析器发送端 = self.分析器实例.取发送器副本();
        // 内置诊断缓存：cargo check 结果合并发布时使用
        let 内置诊断 = self.内置诊断.clone();

        thread::spawn(move || {
            // 用 recv_timeout 代替阻塞 recv，使无消息时也能周期性
            // 清理超时请求（rust-analyzer 挂起/丢弃请求时向客户端应答错误）
            let mut 上次清理 = std::time::Instant::now();
            loop {
                match 接收端.recv_timeout(std::time::Duration::from_secs(1)) {
                    Ok(消息体) => 处理分析器消息(
                        &消息体,
                        &映射器实例,
                        &发送端,
                        &待处理项,
                        &分析器发送端,
                        &内置诊断,
                    ),
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
                if 上次清理.elapsed() >= std::time::Duration::from_secs(10) {
                    清理过期请求(&待处理项, &发送端);
                    上次清理 = std::time::Instant::now();
                }
            }
            log::info!("{}", crate::本地化::全局().取文("lsp_log_ra_thread_exit"));
        });

        Ok(())
    }

    /// 主消息循环
    ///
    /// 用 recv_timeout 轮询代替阻塞 recv：空闲时周期性检查
    /// rust-analyzer 崩溃标志，触发自动重启（进程崩溃后编辑器内
    /// 功能会静默失效，必须主动恢复而非空转等待）。
    fn 主循环(&self) -> anyhow::Result<()> {
        loop {
            let 消息体 = match self
                .客户端连接
                .receiver
                .recv_timeout(std::time::Duration::from_millis(100))
            {
                Ok(消息体) => 消息体,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    // rust-analyzer 异常退出：自动重启并重做完整握手
                    if self.分析器实例.检测崩溃()
                        && let Err(条目项) = self.重启分析器()
                    {
                        log::error!(
                            "{}",
                            crate::本地化::全局()
                                .取文带参("lsp_err_ra_restart", &[&条目项.to_string()])
                        );
                        // 重启失败（如二进制被卸载）：通知客户端并暂停自动重试，
                        // 避免每 100ms 刷一次错误日志与 showMessage
                        let 请求参数体 = json!({
                            "type": 1,
                            "message": crate::本地化::全局().取文带参("lsp_err_ra_restart", &[&条目项.to_string()])
                        });
                        let _ = self
                            .客户端连接
                            .sender
                            .send(Message::Notification(Notification {
                                method: "window/showMessage".to_string(),
                                params: 请求参数体,
                            }));
                        self.分析器实例.清除崩溃标志();
                    }
                    continue;
                }
                Err(_) => {
                    log::info!(
                        "{}",
                        crate::本地化::全局().取文("lsp_log_client_disconnected")
                    );
                    break;
                }
            };

            match 消息体 {
                Message::Request(请求对象) => {
                    if 请求对象.method == "shutdown" {
                        log::info!("{}", crate::本地化::全局().取文("lsp_log_shutdown"));
                        let 响应体 = Response {
                            id: 请求对象.id,
                            result: Some(Value::Null),
                            error: None,
                        };
                        let _ = self.客户端连接.sender.send(Message::Response(响应体));
                        break;
                    }
                    // 单条请求处理失败（如 rust-analyzer 写入失败）只记录错误，
                    // 不退出服务器，避免一次偶发故障杀死整个会话
                    if let Err(条目项) = self.处理客户端请求(请求对象) {
                        log::error!(
                            "{}",
                            crate::本地化::全局()
                                .取文带参("lsp_err_handle_request", &[&条目项.to_string()])
                        );
                    }
                }
                Message::Notification(通知对象) => {
                    if let Err(条目项) = self.处理客户端通知(通知对象) {
                        log::error!(
                            "{}",
                            crate::本地化::全局().取文带参(
                                "lsp_err_handle_notification",
                                &[&条目项.to_string()]
                            )
                        );
                    }
                }
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    /// rust-analyzer 崩溃后的自动恢复：重启子进程并重做完整握手，
    /// 使编辑器会话无感恢复（诊断/补全/悬停等重新可用）。
    fn 重启分析器(&self) -> anyhow::Result<()> {
        log::warn!("{}", crate::本地化::全局().取文("lsp_log_ra_crashed"));
        self.分析器实例.重启进程()?;
        // 重握手：initialize（含语义着色能力捕获）
        self.初始化分析器(&Value::Null)?;
        // initialized 通知：客户端不会重复发送，此处必须手动补发
        self.分析器实例
            .send(&json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }))?;
        // 工作区：initialize 请求已携带虚拟项目 workspaceFolders，
        // 新进程按磁盘现状直接加载（main.rs 已由模块集合变化写盘），
        // 无需再发工作区变更通知
        // 重新打开所有已打开文档的虚拟文件（其磁盘副本与缓冲区不同步，
        // 必须重发 didOpen 提供内存副本）；兄弟模块未打开，不发 didOpen
        // ——否则 rust-analyzer 以内存副本为准，后续磁盘更新通知
        //（didChangeWatchedFiles）不再生效
        for 条目 in self.缓存.全部条目() {
            if !条目.是否打开 {
                continue;
            }
            let 消息体 = json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": 条目.虚拟资源定位,
                        "languageId": "rust",
                        "version": 条目.文档版本,
                        "text": 条目.分析器源码
                    }
                }
            });
            self.分析器实例.send(&消息体)?;
        }
        // 旧进程遗留的待映射请求永远不会有响应：统一向客户端应答错误，
        // 避免客户端永久等待（否则编辑器内对应请求一直转圈）
        失败全部待处理(&self.待处理请求表, &self.客户端连接.sender);
        log::info!("{}", crate::本地化::全局().取文("lsp_log_ra_restarted"));
        Ok(())
    }

    /// 处理客户端请求
    fn 处理客户端请求(&self, 请求对象: Request) -> anyhow::Result<()> {
        log::debug!(
            "{}",
            crate::本地化::全局().取文带参(
                "lsp_log_client_request",
                &[&请求对象.method, &format!("{:?}", 请求对象.id)]
            )
        );

        match 请求对象.method.as_str() {
            "initialize" => Ok(()), // 已在握手中处理
            // 格式化采用全文件替换策略，由代理自行处理（不转发 rust-analyzer）
            "textDocument/formatting" => self.处理格式化(请求对象),
            "textDocument/completion"
            | "textDocument/hover"
            | "textDocument/definition"
            | "textDocument/references"
            | "textDocument/documentSymbol"
            | "textDocument/codeAction"
            | "textDocument/rename"
            | "textDocument/documentHighlight"
            | "textDocument/signatureHelp" => self.转发请求(请求对象),
            _ => self.转发请求(请求对象),
        }
    }

    /// 处理客户端通知
    fn 处理客户端通知(&self, 通知对象: Notification) -> anyhow::Result<()> {
        log::debug!(
            "{}",
            crate::本地化::全局().取文带参("lsp_log_client_notification", &[&通知对象.method])
        );

        match 通知对象.method.as_str() {
            "textDocument/didOpen" => self.处理文档打开(&通知对象.params),
            "textDocument/didChange" => self.处理文档变更(&通知对象.params),
            "textDocument/didClose" => self.处理文档关闭(&通知对象.params),
            "textDocument/didSave" => self.处理文档保存(&通知对象.params),
            "initialized" => {
                // 转发 initialized 给 rust-analyzer。
                // 虚拟项目工作区已在 initialize 请求中作为 workspaceFolders
                // 声明：rust-analyzer 收到 initialized 后统一加载，
                // 避免二次加载与工作区重载的并发竞态
                //（历史上 reload.rs SendError panic 即源于此）。
                let 消息体 = json!({
                    "jsonrpc": "2.0",
                    "method": "initialized",
                    "params": {}
                });
                self.分析器实例.send(&消息体)
            }
            _ => {
                let 消息体 = json!({
                    "jsonrpc": "2.0",
                    "method": 通知对象.method,
                    "params": 通知对象.params
                });
                self.分析器实例.send(&消息体)
            }
        }
    }

    /// 判断 URI 是否为受支持的方言文件
    fn 文件类型受支持(&self, 资源定位: &str) -> bool {
        文件类型受支持(资源定位, &self.受支持扩展名)
    }

    /// 模块集合变化时通知 rust-analyzer 重读聚合 main.rs
    ///
    /// 打开/关闭方言文件会重写虚拟项目的聚合 main.rs；rust-analyzer 的
    /// 文件系统监听对虚拟项目（/tmp 下的临时目录）不可靠，其 VFS 中的
    /// main.rs 内容会停留在旧版本，导致模块文件被判定为“not included
    /// anywhere in the module tree”（unlinked-file 误报）。此处显式以
    /// didChangeWatchedFiles（Changed）告知 main.rs 已更新，让
    /// rust-analyzer 重读该文件（实测比工作区 removed+added 重载更快，
    /// 且不触发 reload 竞态崩溃）。
    fn 模块变更则通知主文件已更新(&self) -> anyhow::Result<()> {
        let 新版本号 = self.缓存.模块版本() as i64;
        let 前值 = self
            .上次模块版本
            .swap(新版本号, std::sync::atomic::Ordering::SeqCst);
        if 前值 == 新版本号 {
            return Ok(());
        }
        let 主资源定位 = 路径转定位(&self.缓存.虚拟项目目录().join("src").join("main.rs"));
        let 通知消息 = json!({
            "jsonrpc": "2.0",
            "method": "workspace/didChangeWatchedFiles",
            "params": {
                "changes": [{ "uri": 主资源定位, "type": 2 }]
            }
        });
        self.分析器实例.send(&通知消息)
    }

    /// 其他虚拟条目的内容变化通知：按条目状态路由
    ///
    /// 打开中的文档在 rust-analyzer 中有对应的内存副本，必须用 didChange
    /// 全量同步（对已打开文档重复 didOpen 违反 LSP 协议）；兄弟模块
    /// 未在编辑器中打开，其虚拟文件由代理直接写盘，用
    /// didChangeWatchedFiles（Changed）让 rust-analyzer 从磁盘重读。
    fn 通知虚拟条目已变更(
        &self, 条目列表: &[Arc<翻译条目>]
    ) -> anyhow::Result<()> {
        for 条目 in 条目列表 {
            let 分析器消息 = if 条目.是否打开 {
                json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/didChange",
                    "params": {
                        "textDocument": {
                            "uri": 条目.虚拟资源定位,
                            "version": 条目.文档版本
                        },
                        "contentChanges": [{ "text": 条目.分析器源码 }]
                    }
                })
            } else {
                json!({
                    "jsonrpc": "2.0",
                    "method": "workspace/didChangeWatchedFiles",
                    "params": {
                        "changes": [{ "uri": 条目.虚拟资源定位, "type": 2 }]
                    }
                })
            };
            self.分析器实例.send(&分析器消息)?;
        }
        Ok(())
    }

    /// 处理文档打开
    fn 处理文档打开(&self, 请求参数体: &Value) -> anyhow::Result<()> {
        let 文档项 = &请求参数体["textDocument"];
        let 资源定位 = 文档项["uri"].as_str().unwrap_or("");
        let 源码内容 = 文档项["text"].as_str().unwrap_or("");
        let 文档版本 = 文档项["version"].as_i64().unwrap_or(1) as i32;

        if !self.文件类型受支持(资源定位) {
            return Ok(());
        }

        let (条目, 其他变更) = self.缓存.更新文档(资源定位, 源码内容, 文档版本)?;

        // 模块集合变化时通知 rust-analyzer 重读聚合 main.rs，
        // 确保其识别新模块（虚拟项目的文件系统监听不可靠）。
        self.模块变更则通知主文件已更新()?;

        // 模块集合变化可能导致其他文件被重写（其虚拟内容新增/移除
        // crate:: 前缀）：打开中的用 didChange 同步，兄弟模块用
        // didChangeWatchedFiles 通知磁盘重读
        self.通知虚拟条目已变更(&其他变更)?;

        let 分析器消息 = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": 条目.虚拟资源定位,
                    "languageId": "rust",
                    "version": 文档版本,
                    "text": 条目.分析器源码
                }
            }
        });
        self.分析器实例.send(&分析器消息)?;
        log::info!(
            "{}",
            crate::本地化::全局()
                .取文带参("lsp_log_doc_opened", &[资源定位, &文档版本.to_string()])
        );

        // 打开即跑权威检查（真实项目镜像）：无需保存即可见编译级诊断；
        // 镜像不可用时回退虚拟项目检查（draft_virtual=false 的兜底）
        self.触发cargo检查(资源定位, false)?;
        Ok(())
    }

    /// 处理文档变更
    ///
    /// 代理声明增量同步（change=2）以减少客户端→代理的传输量：
    /// 无 range 的变更项为全量文本（兼容旧客户端），
    /// 带 range 的按 LSP 位置（UTF-16）逐项应用到缓存中的母语文本。
    /// 应用后仍全量重译，并以全量替换通知 rust-analyzer。
    fn 处理文档变更(&self, 请求参数体: &Value) -> anyhow::Result<()> {
        let 文档项 = &请求参数体["textDocument"];
        let 资源定位 = 文档项["uri"].as_str().unwrap_or("");
        let 文档版本 = 文档项["version"].as_i64().unwrap_or(1) as i32;

        if !self.文件类型受支持(资源定位) {
            return Ok(());
        }

        let Some(变更列表) = 请求参数体["contentChanges"].as_array() else {
            return Ok(());
        };

        // 在缓存旧文本上按序应用变更，得到新全文
        let mut 源码内容 = self
            .缓存
            .查询原文(资源定位)
            .map(|条目项| 条目项.中文原文.clone())
            .unwrap_or_default();
        for 变更项 in 变更列表 {
            if let Some(文字) = 变更项["text"].as_str() {
                if 变更项.get("range").is_none() {
                    // 全量替换
                    源码内容 = 文字.to_string();
                } else {
                    应用增量变更(&mut 源码内容, &变更项["range"], 文字);
                }
            }
        }

        let (条目, 其他变更) = self.缓存.更新文档(资源定位, &源码内容, 文档版本)?;
        let 分析器消息 = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {
                    "uri": 条目.虚拟资源定位,
                    "version": 文档版本
                },
                "contentChanges": [{ "text": 条目.分析器源码 }]
            }
        });
        self.分析器实例.send(&分析器消息)?;
        // 兄弟模块集合可能增删（磁盘文件变化）：聚合 main.rs 需重读；
        // 其他条目内容被重写时按打开状态分别同步
        self.模块变更则通知主文件已更新()?;
        self.通知虚拟条目已变更(&其他变更)?;
        Ok(())
    }

    /// 处理文档关闭
    fn 处理文档关闭(&self, 请求参数体: &Value) -> anyhow::Result<()> {
        let 资源定位 = 请求参数体["textDocument"]["uri"].as_str().unwrap_or("");
        if !self.文件类型受支持(资源定位) {
            return Ok(());
        }

        if let Some(条目) = self.缓存.查询原文(资源定位) {
            let 分析器消息 = json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didClose",
                "params": {
                    "textDocument": { "uri": 条目.虚拟资源定位 }
                }
            });
            self.分析器实例.send(&分析器消息)?;
        }

        // 关闭文档会缩小模块集合，其余条目的虚拟内容可能被重写
        let 其他变更 = self.缓存.关闭文档(资源定位)?;

        // 模块集合变化时通知 rust-analyzer 重读聚合 main.rs（模块已移除）
        self.模块变更则通知主文件已更新()?;

        // 其余条目按状态同步：打开中的用 didChange 全量同步，
        // 兄弟模块（含刚降级的本条）用 didChangeWatchedFiles 磁盘重读
        self.通知虚拟条目已变更(&其他变更)?;
        Ok(())
    }

    /// 处理文档保存
    fn 处理文档保存(&self, 请求参数体: &Value) -> anyhow::Result<()> {
        let 资源定位 = 请求参数体["textDocument"]["uri"].as_str().unwrap_or("");
        if !self.文件类型受支持(资源定位) {
            return Ok(());
        }

        if let Some(条目) = self.缓存.查询原文(资源定位) {
            let 分析器消息 = json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didSave",
                "params": {
                    "textDocument": { "uri": 条目.虚拟资源定位 },
                    "text": 条目.分析器源码
                }
            });
            self.分析器实例.send(&分析器消息)?;
        }

        // 代理自跑 cargo check 并发布诊断：先虚拟项目草稿（快速反馈），
        // 再真实项目镜像（权威，与 rzc 构建口径一致）。rust-analyzer
        // 的 checkOnSave 在虚拟项目上诊断不可达（cargo 常驻但无发布），
        // 而所有权可视化依赖 E0382 等 check 诊断（移动黄/使用红/生命周期绿）
        self.触发cargo检查(资源定位, true)?;
        Ok(())
    }

    /// 异步执行 cargo check 并发布诊断
    ///
    /// 两条通路：
    /// - 虚拟项目草稿检查（`draft_virtual` 时先跑）：教学场景无第三方
    ///   依赖，秒级反馈；
    /// - 真实项目镜像检查（权威）：项目树复制 + 方言转译产物覆盖 +
    ///   `cargo check --offline`，复用真实 target 目录的依赖产物，
    ///   诊断口径与 rzc 构建完全一致；镜像不可用时回退虚拟检查。
    ///
    /// `saved_uri`：触发本次检查的方言文件（定位项目根）；
    /// `draft_virtual`：是否先跑虚拟项目草稿检查（didSave 为 true，
    /// didOpen 为 false——镜像已含全部信息，草稿无必要）。
    fn 触发cargo检查(
        &self, 保存资源定位: &str, 草稿虚拟标记: bool
    ) -> anyhow::Result<()> {
        let 缓存 = self.缓存.clone();
        let 映射器实例 = self.映射器实例.clone();
        let 发送端 = self.客户端连接.sender.clone();
        let 内置诊断 = self.内置诊断.clone();
        let 检查进行中 = self.检查进行中.clone();
        let 检查待补跑 = self.检查待补跑.clone();
        let 扩展名列表 = self.受支持扩展名.clone();
        let 已知词表 = 映射器实例.lint词表().clone();
        let 保存资源定位 = 保存资源定位.to_string();
        let 项目目录 = 缓存.虚拟项目目录();

        // 并发保护：上次 check 未结束（可能卡在锁等待）时，标记待重跑
        // 并立即返回——check 完成后补跑一次合并中间状态，避免连续保存
        // 时中间 check 被静默丢弃，最终诊断停留在旧版本
        if 检查进行中.swap(true, std::sync::atomic::Ordering::SeqCst) {
            检查待补跑.store(true, std::sync::atomic::Ordering::SeqCst);
            return Ok(());
        }

        std::thread::spawn(move || {
            // 循环执行：期间收到新保存请求（pending 置位）则补跑一次；
            // pending 由首轮消费，最多连跑两次，合并中间全部状态
            loop {
                检查待补跑.store(false, std::sync::atomic::Ordering::SeqCst);
                if 草稿虚拟标记 {
                    Self::运行cargo检查一次(
                        &缓存,
                        &映射器实例,
                        &发送端,
                        &内置诊断,
                        &项目目录,
                    );
                }
                // 镜像检查（权威）；不可用时回退虚拟检查
                //（didSave 已跑过草稿，无需重复）
                let 镜像校验通过 = crate::镜像校验::执行镜像检查(
                    &缓存,
                    &发送端,
                    &内置诊断,
                    &扩展名列表,
                    &保存资源定位,
                    &已知词表,
                );
                if !镜像校验通过 && !草稿虚拟标记 {
                    Self::运行cargo检查一次(
                        &缓存,
                        &映射器实例,
                        &发送端,
                        &内置诊断,
                        &项目目录,
                    );
                }
                if !检查待补跑.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
            }
            检查进行中.store(false, std::sync::atomic::Ordering::SeqCst);
        });
        Ok(())
    }

    /// 执行一次 cargo check 并发布诊断（供 [`触发cargo检查`] 的线程循环调用）
    ///
    /// 不负责 check_running/check_pending 标记（由调用方线程循环统一管理）。
    fn 运行cargo检查一次(
        缓存: &Arc<转译缓存>,
        映射器实例: &Arc<响应映射器>,
        发送端: &crossbeam_channel::Sender<Message>,
        内置诊断: &Arc<std::sync::Mutex<HashMap<String, Vec<Value>>>>,
        项目目录: &Path,
    ) {
        // 超时控制：cargo 在锁竞争等场景可能长时间不退出，
        // 轮询等待最多 30 秒后强杀，避免每次保存都挂起一个进程
        let 子进程 = match std::process::Command::new("cargo")
            // --offline：虚拟项目无第三方依赖，跳过 crates.io 索引访问
            // （无 Cargo.lock 时 cargo 默认联网解析依赖，网络不可达会卡死）
            .args(["check", "--offline", "--message-format=json"])
            .current_dir(项目目录)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(进程句柄) => 进程句柄,
            Err(条目项) => {
                log::warn!("cargo check spawn failed: {条目项}");
                return;
            }
        };
        let mut 子进程可选 = Some(子进程);
        let mut 进程输出 = None;
        let mut 已超时 = false;
        for 循环序号 in 0..300 {
            match 子进程可选.as_mut().map(|进程句柄| 进程句柄.try_wait()) {
                Some(Ok(Some(_))) => {
                    进程输出 = 子进程可选
                        .take()
                        .and_then(|进程句柄| 进程句柄.wait_with_output().ok());
                    break;
                }
                Some(Ok(None)) => {
                    if 循环序号 == 299 {
                        已超时 = true;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                _ => break,
            }
        }
        if 已超时 {
            log::warn!("cargo check timeout killed");
            if let Some(mut 进程句柄) = 子进程可选.take() {
                let _ = 进程句柄.kill();
                let _ = 进程句柄.wait();
            }
        }
        let Some(进程输出) = 进程输出 else {
            return;
        };

        // 解析 compiler-message 行 → 按虚拟 uri 聚合（值 = (方言 uri, 诊断列表)）
        let 标准输出文本 = String::from_utf8_lossy(&进程输出.stdout);
        let mut 按定位分组: HashMap<String, (String, Vec<Value>)> = HashMap::new();
        for 行号 in 标准输出文本.lines() {
            let Ok(值项) = serde_json::from_str::<Value>(行号) else {
                continue;
            };
            if 值项["reason"].as_str() != Some("compiler-message") {
                continue;
            }
            let 消息体 = &值项["message"];
            let Some(区间) = 消息体["spans"].as_array().and_then(|数组项| 数组项.first())
            else {
                continue;
            };
            let Some(文件名段) = 区间["file_name"].as_str() else {
                continue;
            };
            // rustc 可能输出相对路径（cwd 为虚拟项目目录），拼上项目目录
            let 文件路径 = if std::path::Path::new(文件名段).is_absolute() {
                std::path::PathBuf::from(文件名段)
            } else {
                项目目录.join(文件名段)
            };
            // 仅处理虚拟方言文件（聚合 main.rs、标准库等跳过）
            let Some(条目) = 缓存.按虚拟资源定位查询(&路径转定位(&文件路径))
            else {
                continue;
            };
            let 行号 = 区间["line_start"].as_u64().unwrap_or(1).saturating_sub(1);
            let 列号 = 区间["column_start"].as_u64().unwrap_or(1).saturating_sub(1);
            let 结束行 = 区间["line_end"]
                .as_u64()
                .unwrap_or(行号 + 1)
                .saturating_sub(1);
            let 结束列 = 区间["column_end"]
                .as_u64()
                .unwrap_or(列号 + 1)
                .saturating_sub(1);
            let 诊断码 = 消息体["code"]["code"].as_str().unwrap_or("").to_string();
            let 严重级别 = match 消息体["level"].as_str() {
                Some("error") => 1,
                Some("warning") => 2,
                _ => 3,
            };
            let 诊断项 = json!({
                "range": {
                    "start": { "line": 行号, "character": 列号 },
                    "end": { "line": 结束行, "character": 结束列 }
                },
                "severity": 严重级别,
                "code": 诊断码,
                "message": 消息体["message"].as_str().unwrap_or(""),
            });
            按定位分组
                .entry(条目.虚拟资源定位.clone())
                .or_insert_with(|| (条目.原始资源定位.clone(), Vec::new()))
                .1
                .push(诊断项);
        }

        // 合并内置诊断（语法/类型）后发布：同 code 且同起始行视为重复。
        // 注意：map_diagnostics 期望输入虚拟 uri（内部还原为方言 uri），
        // 传方言 uri 会导致位置映射查不到条目而丢失全部诊断。
        for (虚拟资源定位, (原始资源定位, mut 诊断列表)) in 按定位分组 {
            if let Ok(锁守卫) = 内置诊断.lock()
                && let Some(内置映射) = 锁守卫.get(&原始资源定位)
            {
                for 条目项 in 内置映射.clone() {
                    let 重复 = 诊断列表.iter().any(|诊断对象| {
                        诊断对象["code"] == 条目项["code"]
                            && 诊断对象["range"]["start"]["line"]
                                == 条目项["range"]["start"]["line"]
                    });
                    if !重复 {
                        诊断列表.push(条目项);
                    }
                }
            }
            let 请求参数体 = json!({ "uri": 虚拟资源定位, "diagnostics": 诊断列表 });
            let mut 映射结果 = 映射器实例.映射诊断(&请求参数体);
            // 教学诊断注入（与 RA 链一致）：由 entry 内容直接计算，
            // 不依赖 builtin 缓存时序（虚拟检查可能先于 RA 首批发布，
            // 缓存为空时教学提示不能丢失）
            if let Some(条目) = 缓存.查询原文(&原始资源定位) {
                let mut 最终诊断 = 映射结果["diagnostics"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                crate::响应映射::诊断文本::注入教学诊断(
                    &mut 最终诊断,
                    &条目,
                    映射器实例.lint词表(),
                    &映射器实例.歧义构造词集(),
                );
                映射结果["diagnostics"] = Value::Array(最终诊断);
            }
            let 通知消息 = Notification {
                method: "textDocument/publishDiagnostics".to_string(),
                params: 映射结果,
            };
            let _ = 发送端.send(Message::Notification(通知消息));
        }
    }

    /// 处理代码格式化请求（textDocument/formatting）
    ///
    /// 采用全文件替换策略，不转发 rust-analyzer：
    /// 取缓存中的英文译文 → rustfmt 格式化 → 反向翻译回母语 →
    /// 返回覆盖整个文档的 TextEdit。
    /// 文档未打开或 rustfmt 失败时返回空数组，不崩溃。
    fn 处理格式化(&self, 请求对象: Request) -> anyhow::Result<()> {
        let 资源定位 = 请求对象.params["textDocument"]["uri"]
            .as_str()
            .unwrap_or("");

        let 编辑列表 = match self.缓存.查询原文(资源定位) {
            Some(条目) => {
                let 制表位宽度 = 请求对象.params["options"]["tabSize"].as_u64().unwrap_or(4);
                match 运行rustfmt(&条目.英文源码, 制表位宽度) {
                    Some(格式化英文) => {
                        // 将格式化后的英文代码反向翻译为母语代码
                        // （按文档编辑地图精确删除代理添加的 crate:: 前缀）
                        let 格式化母语 = self.缓存.逆向转译(Some(资源定位), &格式化英文);
                        vec![json!({
                            "range": {
                                "start": { "line": 0, "character": 0 },
                                "end": 文本末尾位置(&条目.中文原文)
                            },
                            "newText": 格式化母语
                        })]
                    }
                    None => Vec::new(),
                }
            }
            None => Vec::new(),
        };

        let 响应体 = Response {
            id: 请求对象.id,
            result: Some(Value::Array(编辑列表)),
            error: None,
        };
        self.客户端连接
            .sender
            .send(Message::Response(响应体))
            .map_err(|条目项| {
                anyhow::anyhow!(
                    "{}",
                    crate::本地化::全局()
                        .取文带参("lsp_err_send_format", &[&条目项.to_string()])
                )
            })?;
        Ok(())
    }

    /// 转发请求到 rust-analyzer
    ///
    /// 除了将 URI 替换为虚拟文件 URI，还将请求中的位置
    /// （position/range）从母语坐标转换为英文坐标，
    /// 并将 rename 的 newName 翻译为英文。
    fn 转发请求(&self, 请求对象: Request) -> anyhow::Result<()> {
        let 分析器请求号 = self
            .请求计数器
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let 原始资源定位 = 请求对象.params["textDocument"]["uri"]
            .as_str()
            .unwrap_or("")
            .to_string();

        {
            let mut 待处理项 = self
                .待处理请求表
                .lock()
                .map_err(|_| anyhow::anyhow!("{}", crate::本地化::全局().取文("lsp_err_lock")))?;
            待处理项.insert(
                分析器请求号,
                待处理请求信息 {
                    原始请求号: 请求对象.id,
                    method: 请求对象.method.clone(),
                    原始资源定位: 原始资源定位.clone(),
                    创建时刻: std::time::Instant::now(),
                    未解析依赖: if 请求对象.method == "textDocument/codeAction" {
                        let 诊断列表 = 请求对象.params["context"]["diagnostics"]
                            .as_array()
                            .map(|数组项| 数组项.as_slice())
                            .unwrap_or(&[]);
                        从诊断提取未解析依赖(诊断列表)
                    } else {
                        Vec::new()
                    },
                    教学诊断: if 请求对象.method == "textDocument/codeAction" {
                        请求对象.params["context"]["diagnostics"]
                            .as_array()
                            .map(|数组项| {
                                数组项
                                    .iter()
                                    .filter(|诊断对象| {
                                        crate::响应映射::诊断文本::是教学诊断(诊断对象)
                                    })
                                    .cloned()
                                    .collect()
                            })
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    },
                },
            );
        }

        // 替换 URI 为虚拟 URI，并转换请求中的位置
        let mut 请求参数体 = 请求对象.params.clone();
        let 条目 = if self.文件类型受支持(&原始资源定位) {
            self.缓存.查询原文(&原始资源定位)
        } else {
            None
        };

        if let Some(条目) = &条目 {
            // 1. 替换 textDocument.uri
            if let Some(资源定位字段) = 请求参数体
                .get_mut("textDocument")
                .and_then(|表项| 表项.get_mut("uri"))
            {
                *资源定位字段 = Value::String(条目.虚拟资源定位.clone());
            }

            // 2. 按方法转换位置参数（母语坐标 → 英文坐标）
            match 请求对象.method.as_str() {
                "textDocument/completion"
                | "textDocument/hover"
                | "textDocument/definition"
                | "textDocument/references"
                | "textDocument/rename"
                | "textDocument/documentHighlight"
                | "textDocument/signatureHelp" => {
                    if let Some(位置) = 请求参数体.get_mut("position") {
                        *位置 = 位置转英文(条目, 位置);
                    }
                }
                "textDocument/codeAction" => {
                    if let Some(跨度) = 请求参数体.get_mut("range") {
                        *跨度 = 跨度转英文(条目, 跨度);
                    }
                    // context.diagnostics 来自我们发布的中文诊断，同样需要转换
                    if let Some(诊断清单) = 请求参数体
                        .get_mut("context")
                        .and_then(|数组值项| 数组值项.get_mut("diagnostics"))
                        .and_then(|诊断对象| 诊断对象.as_array_mut())
                    {
                        for 诊断项 in 诊断清单.iter_mut() {
                            if let Some(跨度) = 诊断项.get_mut("range") {
                                *跨度 = 跨度转英文(条目, 跨度);
                            }
                        }
                    }
                }
                _ => {}
            }

            // 3. rename 的 newName：中文 → 英文（不在关键字映射中则保持原样）
            if 请求对象.method == "textDocument/rename"
                && let Some(中文名称) = 请求参数体.get("newName").and_then(|值项| 值项.as_str())
            {
                let 英文名称 = self
                    .缓存
                    .关键词映射表()
                    .get(中文名称)
                    .or_else(|| self.缓存.别名映射表().get(中文名称))
                    .cloned()
                    .unwrap_or_else(|| 中文名称.to_string());
                请求参数体["newName"] = Value::String(英文名称);
            }
        }

        let 分析器消息 = json!({
            "jsonrpc": "2.0",
            "id": 分析器请求号,
            "method": 请求对象.method,
            "params": 请求参数体
        });
        self.分析器实例.send(&分析器消息)
    }
}

/// 判断 URI 是否以任一受支持的方言扩展名结尾
fn 文件类型受支持(资源定位: &str, 扩展名列表: &[String]) -> bool {
    扩展名列表.iter().any(|扩展| 资源定位.ends_with(扩展))
}

/// 从 codeAction 请求上下文诊断中提取未声明的 crate 名
///
/// 消息可能是英文原文（rust-analyzer 直发）或翻译后的母语文本
///（经我方 publish 后由客户端回传）：两者都保留反引号包裹的路径
/// 内容，提取逻辑统一；母语消息按本地化短语表判定未解析导入。
fn 从诊断提取未解析依赖(诊断列表: &[Value]) -> Vec<String> {
    let 已翻译词组 = crate::本地化::全局().取文("lsp_phrase_unresolved_import");
    let mut 应答结果 = Vec::new();
    for 诊断项 in 诊断列表 {
        let Some(消息体) = 诊断项.get("message").and_then(|值项| 值项.as_str()) else {
            continue;
        };
        let 是否未解析 = i18n_rust_engine::诊断::是未解析导入消息(消息体)
            || 消息体.contains(已翻译词组.as_str());
        if !是否未解析 {
            continue;
        }
        for 片段 in i18n_rust_engine::诊断::抽取反引号首段(消息体) {
            if matches!(
                片段.as_str(),
                "std" | "core" | "alloc" | "self" | "super" | "crate" | "proc_macro"
            ) || 片段
                .chars()
                .next()
                .is_some_and(|字符项| 字符项.is_ascii_digit())
            {
                continue;
            }
            if !应答结果.contains(&片段) {
                应答结果.push(片段);
            }
        }
    }
    应答结果
}

/// 计算虚拟项目临时目录（按用户 + 进程实例隔离）并拒绝符号链接
///
/// 固定共享的 /tmp 路径在多用户机器上可被预创建为符号链接，
/// 后续写文件会跟随链接覆写任意位置；按用户名隔离并校验规避此风险。
/// 同一用户的多个编辑器实例各起一个 LSP 代理进程，再叠加 PID 后缀
/// 避免互相同目录覆写 Cargo.toml / src 内容；启动时清理已死进程
/// 的残留目录（见 [`清理陈旧虚拟目录`]）防止无限累积。
fn 虚拟临时目录() -> anyhow::Result<PathBuf> {
    let 用户名 = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "default".to_string());
    let 安全用户名: String = 用户名
        .chars()
        .map(|字符项| {
            if 字符项.is_alphanumeric() || 字符项 == '_' {
                字符项
            } else {
                '_'
            }
        })
        .collect();
    清理陈旧虚拟目录(&安全用户名);
    let 目录 = std::env::temp_dir().join(format!(
        "i18n_lsp_virtual_{}_{}",
        安全用户名,
        std::process::id()
    ));
    // 两道校验，缺一不可：
    // 1. 创建前——可预测名（用户 + PID）可被本地攻击者预创建为符号链接；
    // 2. 创建后、任何写文件之前——校验与创建之间存在 TOCTOU 窗口，
    //    攻击者可在此期间把路径换成符号链接，而 create_dir_all 会跟随它。
    //    此时 lstat（symlink_metadata）仍报告符号链接本身，立即拒绝可保证
    //    尚无任何文件写入发生在链接目标处。
    if 是符号链接(&目录) {
        anyhow::bail!(
            "{}",
            crate::本地化::全局()
                .取文带参("lsp_err_temp_symlink", &[&目录.display().to_string()])
        );
    }
    std::fs::create_dir_all(&目录).map_err(|条目项| {
        anyhow::anyhow!(
            "创建 LSP 虚拟项目目录失败：{}（{}）",
            目录.display(),
            条目项
        )
    })?;
    if 是符号链接(&目录) {
        anyhow::bail!(
            "{}",
            crate::本地化::全局()
                .取文带参("lsp_err_temp_symlink", &[&目录.display().to_string()])
        );
    }
    Ok(目录)
}

/// 路径自身是否为符号链接（lstat 语义，不跟随末段链接）
fn 是符号链接(路径: &Path) -> bool {
    路径
        .symlink_metadata()
        .map(|目录项| 目录项.file_type().is_symlink())
        .unwrap_or(false)
}

/// 清理同用户的残留虚拟/镜像目录：仅删除名称中带 PID 后缀且进程已死的目录
///
/// 虚拟目录名 `i18n_lsp_virtual_<用户>_<PID>`；镜像目录名
/// `i18n_lsp_mirror_<用户>_<PID>_<项目哈希>`（PID 段后还有哈希段）。
/// 尽力而为：任何失败（目录列举失败、无法解析 PID、删除失败）都静默跳过，
/// 不影响本实例启动。存活检查对 Unix（kill 0 信号）与 Windows
/// （OpenProcess 语义的 tasklist 查询不可移植，退回 mtime 启发式）分别处理。
fn 清理陈旧虚拟目录(安全用户名: &str) {
    let 前缀列表 = [
        format!("i18n_lsp_virtual_{}_", 安全用户名),
        format!("i18n_lsp_mirror_{}_", 安全用户名),
    ];
    let Ok(条目列表) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for 条目 in 条目列表.flatten() {
        let 名称值 = 条目.file_name();
        let Some(名称串) = 名称值.to_str() else {
            continue;
        };
        // 仅处理本用户且带 PID 后缀的目录；无后缀的旧版目录不动（避免误删）
        let Some(剩余) = 前缀列表
            .iter()
            .find_map(|前缀项| 名称串.strip_prefix(前缀项.as_str()))
        else {
            continue;
        };
        // 兼容上轮 rename 后未删除干净的 `.stale` 残留；镜像名 PID 后还有
        // 哈希段，取首个下划线之前的部分为 PID
        let 剩余 = 剩余.strip_suffix(".stale").unwrap_or(剩余);
        let 进程号串 = 剩余.split('_').next().unwrap_or(剩余);
        if 进程号串 == std::process::id().to_string() {
            continue; // 当前进程自己的目录
        }
        let Ok(进程号) = 进程号串.parse::<u32>() else {
            continue;
        };
        if 进程存活(进程号) {
            continue;
        }
        // 进程已死：目录是残留，先原子改名再删除（TOCTOU 防护）：
        // 检查与删除之间 PID 可能被系统回收并由新进程重建同名目录，
        // rename 把旧目录移走后再删，新进程创建的是新路径，不受影响
        let 路径 = 条目.path();
        if 路径
            .symlink_metadata()
            .map(|目录项| 目录项.file_type().is_symlink())
            .unwrap_or(true)
        {
            continue; // 符号链接不跟随删除
        }
        let 陈旧路径 = 条目.path().with_file_name(format!("{名称串}.stale"));
        if std::fs::rename(&路径, &陈旧路径).is_ok() {
            let _ = std::fs::remove_dir_all(&陈旧路径);
        }
    }
}

/// 响应 rust-analyzer 的 workspace/configuration 请求
///
/// - `completion.snippets = custom`：方法补全附带括号；
/// - `checkOnSave`/`check.enable = false`：禁用 rust-analyzer 自跑的
///   cargo check（flycheck）。该检查针对无第三方依赖、无过程宏的虚拟
///   项目，结果按 workspace 全部文件成批发布：依赖缺失（E0432/E0599）、
///   derive 不展开的级联错误（E0277 Display 等）在该环境下恒为假红。
///   编译级诊断由代理自跑虚拟/镜像检查提供（didSave/didOpen 触发）。
fn 分析器配置结果(数量: usize) -> Value {
    Value::Array(
        (0..数量)
            .map(|_| {
                json!({
                    "completion": { "snippets": "custom" },
                    "checkOnSave": { "enable": false },
                    "check": { "enable": false }
                })
            })
            .collect(),
    )
}

/// 判断 PID 是否存活（仅用于残留目录清理，误判代价低）
///
/// Unix：kill 0 信号探活；返回 -1（进程不存在或无权限）时保守视为存活，
/// 宁可残留目录下轮再清，也不误删活进程的文件。
fn 进程存活(进程号: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: `kill` 为 POSIX 标准 C 函数，签名与声明一致（pid_t/int 均为 i32）；
        // 本进程不安装 signal handler，调用本身不改变内存状态；sig=0 只做权限与
        // 存在性检查、不递送任何信号，故无内存安全或状态破坏风险。
        unsafe extern "C" {
            fn kill(进程号: i32, sig: i32) -> i32;
        }
        // 不实际发信号（sig=0），仅做存在性检查
        unsafe { kill(进程号 as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        // 非 Unix 平台无廉价探活手段：保守视为存活，不删除
        let _ = 进程号;
        true
    }
}

/// 清理超时的待映射请求：向客户端应答错误，避免永久等待与条目泄漏
fn 清理过期请求(
    待处理项: &Arc<std::sync::Mutex<HashMap<i64, 待处理请求信息>>>,
    发送端: &crossbeam_channel::Sender<Message>,
) {
    let 当前时刻 = std::time::Instant::now();
    let mut 已过期: Vec<lsp_server::RequestId> = Vec::new();
    {
        let mut 待处理表 = match 待处理项.lock() {
            Ok(守卫项) => 守卫项,
            Err(_) => return,
        };
        待处理表.retain(|_标识, 信息| {
            if 当前时刻.duration_since(信息.创建时刻) > 请求超时 {
                已过期.push(信息.原始请求号.clone());
                false
            } else {
                true
            }
        });
    }
    for 请求编号 in 已过期 {
        log::warn!("{}", crate::本地化::全局().取文("lsp_warn_request_timeout"));
        let 响应体 = Response {
            id: 请求编号,
            result: None,
            error: Some(lsp_server::ResponseError {
                code: -32603,
                message: "rust-analyzer did not respond in time".to_string(),
                data: None,
            }),
        };
        let _ = 发送端.send(Message::Response(响应体));
    }
}

/// 清空全部待映射请求：向客户端应答错误（rust-analyzer 崩溃重启后调用，
/// 旧进程的请求永远不会有响应，必须主动应答避免客户端永久等待）
fn 失败全部待处理(
    待处理项: &Arc<std::sync::Mutex<HashMap<i64, 待处理请求信息>>>,
    发送端: &crossbeam_channel::Sender<Message>,
) {
    let 标识列表: Vec<lsp_server::RequestId> = {
        let mut 待处理表 = match 待处理项.lock() {
            Ok(守卫项) => 守卫项,
            Err(_) => return,
        };
        待处理表
            .drain()
            .map(|(_标识, 信息)| 信息.原始请求号)
            .collect()
    };
    for 请求编号 in 标识列表 {
        let 响应体 = Response {
            id: 请求编号,
            result: None,
            error: Some(lsp_server::ResponseError {
                code: -32603,
                message: "rust-analyzer restarted".to_string(),
                data: None,
            }),
        };
        let _ = 发送端.send(Message::Response(响应体));
    }
}

/// 调用系统 rustfmt 格式化英文代码
///
/// 通过 stdin 传入源码、stdout 取回格式化结果。
/// 新版 rustfmt（1.9+）不传文件参数时从 stdin 读取，需配合 `--emit stdout`；
/// rustfmt 不存在、源码含语法错误或输出非 UTF-8 时返回 None。
fn 运行rustfmt(源码文本: &str, 制表位宽度: u64) -> Option<String> {
    let mut 子进程 = std::process::Command::new("rustfmt")
        .arg("--emit")
        .arg("stdout")
        .arg("--edition")
        .arg("2021")
        .arg("--config")
        .arg(format!("tab_spaces={}", 制表位宽度.max(1)))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    if let Some(mut stdin) = 子进程.stdin.take() {
        stdin.write_all(源码文本.as_bytes()).ok()?;
        // stdin 在此处 drop，关闭管道使 rustfmt 结束输出
    }
    let 进程输出 = 子进程.wait_with_output().ok()?;
    if !进程输出.status.success() {
        return None;
    }
    String::from_utf8(进程输出.stdout).ok()
}

/// 计算母语文本末尾的 LSP 位置（最后一行、最后一列的 UTF-16 长度）
///
/// 用于生成覆盖整个文档的格式化 TextEdit 的 range 终点。
fn 文本末尾位置(源码内容: &str) -> Value {
    let 行数 = 源码内容.matches('\n').count() as u64 + 1;
    let 末行 = 源码内容.rsplit('\n').next().unwrap_or("");
    let 列数 = 末行
        .chars()
        .map(|字符项| 字符项.len_utf16() as u64)
        .sum::<u64>();
    json!({ "line": 行数 - 1, "character": 列数 })
}

/// 将增量变更（range + text）应用到母语文本
///
/// LSP 位置按 UTF-16 code unit 计数；越界位置钳制到行尾/文末，
/// 防御客户端发来异常 range 时 panic。
fn 应用增量变更(源码内容: &mut String, 跨度: &Value, 相关文本: &str) {
    let 起始偏移 = 位置转偏移(源码内容, &跨度["start"]);
    let 末尾偏移 = 位置转偏移(源码内容, &跨度["end"]).max(起始偏移);
    源码内容.replace_range(起始偏移..末尾偏移, 相关文本);
}

/// LSP 位置（line/character，UTF-16）→ 文本字节偏移
///
/// 行号越界钳制到最后一行；列号越界钳制到行尾（含换行符前）。
fn 位置转偏移(源码内容: &str, 位置: &Value) -> usize {
    let 行号 = 位置["line"].as_u64().unwrap_or(0) as u32;
    let 列单元 = 位置["character"].as_u64().unwrap_or(0) as u32;
    let mut 行首位置 = 0usize;
    for (当前行, (行序号, _)) in 源码内容.match_indices('\n').enumerate() {
        if 当前行 as u32 == 行号 {
            break;
        }
        行首位置 = 行序号 + 1;
    }
    // 行内按 UTF-16 单元前进，列号用尽或到达行尾（不含换行符）即停
    let 行文本 = &源码内容[行首位置..];
    let mut utf16列数 = 0u32;
    for (循环序号, 字符项) in 行文本.char_indices() {
        if 字符项 == '\n' || utf16列数 >= 列单元 {
            return 行首位置 + 循环序号;
        }
        utf16列数 += 字符项.len_utf16() as u32;
    }
    行首位置 + 行文本.len()
}

/// 将 LSP 位置（position）从母语坐标转换为英文坐标
///
/// 当前翻译逐行替换关键字、行数保持不变（行映射为 1:1），
/// 因此仅列号需要按列偏移映射转换。
fn 位置转英文(条目: &翻译条目, 位置: &Value) -> Value {
    let 行号 = 位置["line"].as_u64().unwrap_or(0) as u32;
    let 列号 = 位置["character"].as_u64().unwrap_or(0) as u32;
    // 调用方已持有条目：直接走无锁单函数，避免再按 URI 查缓存（2-4 锁）
    let 英文列号 = crate::翻译缓存::中文列转英文列(条目, 行号, 列号);
    json!({ "line": 行号, "character": 英文列号 })
}

/// 将 LSP 范围（range）从母语坐标转换为英文坐标
fn 跨度转英文(条目: &翻译条目, 跨度: &Value) -> Value {
    let 起始偏移 = 位置转英文(条目, &跨度["start"]);
    let 末尾偏移 = 位置转英文(条目, &跨度["end"]);
    json!({ "start": 起始偏移, "end": 末尾偏移 })
}

/// 处理来自 rust-analyzer 的消息并转发给客户端
fn 处理分析器消息(
    消息体: &Value,
    映射器实例: &Arc<响应映射器>,
    发送端: &crossbeam_channel::Sender<Message>,
    待处理项: &Arc<std::sync::Mutex<HashMap<i64, 待处理请求信息>>>,
    回复发送端: &crate::分析器::发送器,
    内置诊断: &Arc<std::sync::Mutex<HashMap<String, Vec<Value>>>>,
) {
    if let Some(请求编号) = 消息体.get("id").and_then(|值项| 值项.as_i64()) {
        // 是响应
        let 原始信息 = {
            let mut 待处理映射 = match 待处理项.lock() {
                Ok(守卫项) => 守卫项,
                Err(_) => return,
            };
            待处理映射.remove(&请求编号)
        };

        if let Some(信息) = 原始信息 {
            // rust-analyzer 返回错误时（如文件不存在），原样透传给客户端
            if 消息体.get("error").is_some() {
                let 错误取值 = 消息体["error"].clone();
                let 响应体 = Response {
                    id: 信息.原始请求号,
                    result: None,
                    error: Some(lsp_server::ResponseError {
                        code: 错误取值["code"].as_i64().unwrap_or(-32603) as i32,
                        message: 错误取值["message"]
                            .as_str()
                            .map(String::from)
                            .unwrap_or_else(|| crate::本地化::全局().取文("lsp_internal_error")),
                        data: 错误取值.get("data").cloned(),
                    }),
                };
                let _ = 发送端.send(Message::Response(响应体));
                return;
            }

            let 应答结果 = 消息体.get("result").cloned().unwrap_or(Value::Null);
            let 映射后结果 = match 信息.method.as_str() {
                "textDocument/completion" => {
                    映射器实例.映射补全响应(&应答结果, &信息.原始资源定位)
                }
                "textDocument/hover" => {
                    映射器实例.映射悬停响应(&应答结果, &信息.原始资源定位)
                }
                "textDocument/signatureHelp" => 映射器实例.映射签名帮助响应(&应答结果),
                "textDocument/definition" => 映射器实例.映射定义响应(&应答结果),
                "textDocument/references" => 映射器实例.映射引用响应(&应答结果),
                "textDocument/documentSymbol" => {
                    映射器实例.映射文档符号响应(&应答结果, &信息.原始资源定位)
                }
                "textDocument/codeAction" => {
                    let 映射结果 =
                        映射器实例.映射代码操作响应(&应答结果, &信息.原始资源定位);
                    // 未解析导入错误时注入“添加依赖”快捷修复（cargo add）
                    let 映射结果 = 映射器实例.注入添加依赖动作(&映射结果, &信息.未解析依赖);
                    // 教学诊断（全角标点/教学 lint）注入一键修复动作
                    映射器实例.注入教学动作(&映射结果, &信息.教学诊断, &信息.原始资源定位)
                }
                "codeAction/resolve" => 映射器实例.映射代码操作解析响应(&应答结果),
                "textDocument/rename" => 映射器实例.映射重命名响应(&应答结果),
                "textDocument/documentHighlight" => {
                    映射器实例.映射文档高亮响应(&应答结果, &信息.原始资源定位)
                }
                "textDocument/semanticTokens/full" | "textDocument/semanticTokens/range" => {
                    映射器实例.映射语义记号响应(&应答结果, &信息.原始资源定位)
                }
                _ => 应答结果,
            };

            let 响应体 = Response {
                id: 信息.原始请求号,
                result: Some(映射后结果),
                error: None,
            };
            let _ = 发送端.send(Message::Response(响应体));
        } else if 消息体
            .get("method")
            .and_then(|值项| 值项.as_str())
            .is_some()
        {
            // 待映射表中没有对应记录：这是 rust-analyzer 主动发来的请求
            // （如 workspace/configuration、workspace/diagnostic/refresh）。
            // 必须把响应回发给 rust-analyzer 本身，否则它会一直等待。
            let 协议方法名 = 消息体["method"].as_str().unwrap_or("");
            let 应答结果 = if 协议方法名 == "workspace/configuration" {
                let 数量 = 消息体["params"]["items"]
                    .as_array()
                    .map(|数组项| 数组项.len())
                    .unwrap_or(0);
                分析器配置结果(数量)
            } else {
                Value::Null
            };
            let 响应体 = json!({
                "jsonrpc": "2.0",
                "id": 请求编号,
                "result": 应答结果
            });
            let _ = 回复发送端.send(&响应体);
        }
    } else if let Some(协议方法名) = 消息体.get("method").and_then(|值项| 值项.as_str())
    {
        // 是通知
        match 协议方法名 {
            "textDocument/publishDiagnostics" => {
                if let Some(请求参数体) = 消息体.get("params") {
                    // 只转发虚拟母语文件的诊断：
                    // rust-analyzer 可能对其他项目文件发布诊断，
                    // 这些与母语代码无关，不应发给客户端。
                    let 诊断资源定位 = 请求参数体["uri"].as_str().unwrap_or("");
                    if !映射器实例.是虚拟资源定位(诊断资源定位) {
                        return;
                    }
                    let mut 映射结果 = 映射器实例.映射诊断(请求参数体);
                    // 合并全角标点教学诊断（方言坐标，与内置诊断同格式）：
                    // 全角标点在转译后的虚拟文本中保留，扫描结果经列映射
                    // 还原为方言坐标，随 RA 发布链路同步下发——教学提示
                    // 与语法诊断共存，不因 publishDiagnostics 全量替换而闪烁
                    let mut 合并诊断 = 映射结果["diagnostics"]
                        .as_array()
                        .map(|数组项| 数组项.to_vec())
                        .unwrap_or_default();
                    let 合并资源定位 = 映射结果["uri"].as_str().unwrap_or("").to_string();
                    if let Some(条目) = 映射器实例.按原始取条目(&合并资源定位) {
                        // RA 的 E 系列编译错误在 cargo 项目内抑制：编译级诊断
                        // 以代理自跑的镜像/虚拟项目检查（rustc 口径）为准，
                        // RA 在虚拟项目上对第三方依赖与过程宏的类型推断恒为
                        // 假红（详见 抑制分析器编译错误 的说明）
                        合并诊断.retain(|诊断对象| {
                            !crate::响应映射::诊断文本::抑制分析器编译错误(
                                诊断对象,
                                &条目.原始路径,
                            )
                        });
                        // 教学诊断注入（全角标点 + 教学 lint；移除旧注入保证最新）
                        crate::响应映射::诊断文本::注入教学诊断(
                            &mut 合并诊断,
                            &条目,
                            映射器实例.lint词表(),
                            &映射器实例.歧义构造词集(),
                        );
                        映射结果["diagnostics"] = Value::Array(合并诊断.clone());
                    }
                    // 缓存映射后的内置诊断（方言坐标），供 cargo check 结果合并发布
                    if let Ok(mut 锁守卫) = 内置诊断.lock() {
                        锁守卫.insert(合并资源定位, 合并诊断);
                    }
                    let 通知消息 = Notification {
                        method: 协议方法名.to_string(),
                        params: 映射结果,
                    };
                    let _ = 发送端.send(Message::Notification(通知消息));
                }
            }
            _ => {
                let 通知消息 = Notification {
                    method: 协议方法名.to_string(),
                    params: 消息体.get("params").cloned().unwrap_or(Value::Null),
                };
                let _ = 发送端.send(Message::Notification(通知消息));
            }
        }
    }
}

/// 加载语言包：返回统一映射管理器（关键字/宏/派生/模块路径/别名全量）
///
/// 关键字与别名分离（与 CLI 统一管线对齐）：关键字在词法阶段无条件替换，
/// 标准库/第三方库标识符（别名）在词法转译后经声明位保护替换，
/// 避免用户声明与库别名撞名时被误替换（如 `让 新 = 5`）。
/// 模块路径映射随统一管线一并启用（use 语句路径段中文化）。
fn 加载语言包(
    语言包路径: &Path,
) -> anyhow::Result<i18n_rust_engine::映射管理::映射管理器> {
    let 映射表路径 = 语言包路径.join("映射表");
    if 映射表路径.exists() {
        match 映射源::加载关键词映射(语言包路径) {
            Ok(待处理表) => {
                // 旧"映射表"目录格式：仅有扁平关键字表，宏/派生/模块路径/别名表为空
                return Ok(
                    i18n_rust_engine::映射管理::映射管理器::自扁平映射新建(
                        待处理表,
                        HashMap::new(),
                        HashMap::new(),
                    ),
                );
            }
            Err(条目项) => log::warn!(
                "{}",
                crate::本地化::全局()
                    .取文带参("lsp_log_mappings_fallback", &[&条目项.to_string()])
            ),
        }
    }

    let 关键词路径 = 语言包路径.join("keywords.toml");
    if 关键词路径.exists() {
        // 复用 engine 统一加载器（与 CLI 完全同源）：关键字/别名分离、
        // stdlib 优先于第三方库、crates/*.toml 按文件名排序合并
        return i18n_rust_engine::映射管理::映射管理器::自目录加载(语言包路径).map_err(|条目项| {
            anyhow::anyhow!(
                "{}",
                crate::本地化::全局().取文带参("lsp_err_load_keywords", &[&条目项.to_string()])
            )
        });
    }

    log::warn!(
        "{}",
        crate::本地化::全局().取文("lsp_warn_builtin_fallback")
    );
    // 语言包目录缺失时的回退：物化 engine 编译期内嵌的完整中文语言包后
    // 走统一加载器（与 CLI 完全同源，含关键字/宏/别名全量）。
    // 不可再退回 创建内置关键词映射()：那是早期硬编码旧表
    // （54 个旧词、无宏表、无 `让` 等新关键字），会导致转译残缺——
    // 如 `打印行!` 不翻译报 cannot find macro、`让` 不翻译报语法错误。
    if let Some(管理器) = 加载内置中文后备() {
        return Ok(管理器);
    }
    // 极端兜底：物化失败时退回硬编码旧表（可能残缺，但保证可启动）
    Ok(
        i18n_rust_engine::映射管理::映射管理器::自扁平映射新建(
            映射源::创建内置关键词映射(),
            HashMap::new(),
            HashMap::new(),
        ),
    )
}

/// 从 engine 编译期内嵌的中文语言包物化出完整映射管理器
///
/// 将内嵌文件（keywords/stdlib/module_paths/crates/*.toml）写入临时目录，
/// 复用 [`映射管理器::自目录加载`] 统一加载，保证与磁盘语言包完全同源。
fn 加载内置中文后备() -> Option<i18n_rust_engine::映射管理::映射管理器> {
    let 目录 = tempfile::tempdir().ok()?;
    let 中文目录 = 目录.path().join("zh");
    std::fs::create_dir_all(&中文目录).ok()?;
    for (文件名段, 源码内容) in i18n_rust_engine::语言::内置语言文件("zh") {
        // crates/*.toml 等含子目录的文件：逐级创建父目录
        if let Some(父目录) = std::path::Path::new(文件名段).parent()
            && !父目录.as_os_str().is_empty()
        {
            std::fs::create_dir_all(中文目录.join(父目录)).ok()?;
        }
        std::fs::write(中文目录.join(文件名段), 源码内容).ok()?;
    }
    i18n_rust_engine::映射管理::映射管理器::自目录加载(&中文目录).ok()
}

#[cfg(test)]
mod 单元测试 {
    use super::*;
    use std::collections::HashMap;

    /// 语言包目录缺失时的回退必须加载完整中文表（含宏与 `让` 等新关键字），
    /// 而非硬编码旧表——否则 `打印行!` 不翻译报 cannot find macro、
    /// `让` 不翻译报语法错误（真实事故：扩展未找到语言包目录时触发）
    #[test]
    fn 测试_加载语言包后备完整() {
        let 管理器 = 加载语言包(Path::new("/不存在的目录")).expect("fallback 应成功");
        assert_eq!(
            管理器.关键词映射表.get("让").map(String::as_str),
            Some("let")
        );
        assert_eq!(
            管理器.取派生映射表().get("克隆").map(String::as_str),
            Some("Clone")
        );
        assert_eq!(
            管理器.取宏映射表().get("打印行").map(String::as_str),
            Some("println")
        );
        assert!(
            管理器.关键词映射表.len() >= 100,
            "完整关键字表应 ≥100，实际 {}",
            管理器.关键词映射表.len()
        );
        assert!(
            管理器.取宏映射表().len() >= 30,
            "宏表应 ≥30，实际 {}",
            管理器.取宏映射表().len()
        );
        assert!(!管理器.别名映射表.is_empty(), "别名表不应为空");
        assert!(!管理器.模块路径映射表.is_empty(), "模块路径表不应为空");
    }

    /// 默认扩展名列表覆盖全部内置语言包（引擎 lang-packs 单一来源），未知扩展名不匹配
    #[test]
    fn 测试_文件类型受支持_默认() {
        let 扩展名列表 = 默认扩展名();
        assert!(!扩展名列表.is_empty(), "内置语言包数量异常");
        assert!(文件类型受支持(
            "file:///project/src/main.zh",
            &扩展名列表
        ));
        // 单语分支：他语扩展名不再内置，应判为不支持
        assert!(!文件类型受支持(
            "file:///project/src/main.de",
            &扩展名列表
        ));
        assert!(!文件类型受支持(
            "file:///project/src/main.ru",
            &扩展名列表
        ));
        assert!(!文件类型受支持(
            "file:///project/src/main.ja",
            &扩展名列表
        ));
        assert!(!文件类型受支持(
            "file:///project/src/main.rs",
            &扩展名列表
        ));
        assert!(!文件类型受支持(
            "file:///project/src/main.xyz",
            &扩展名列表
        ));
    }

    /// 自定义扩展名列表生效
    #[test]
    fn 测试_文件类型受支持_自定义() {
        let 扩展名列表 = vec![".fr".to_string()];
        assert!(文件类型受支持(
            "file:///project/src/main.fr",
            &扩展名列表
        ));
        assert!(!文件类型受支持(
            "file:///project/src/main.zh",
            &扩展名列表
        ));
    }

    fn 构造测试缓存() -> (Arc<转译缓存>, tempfile::TempDir) {
        let 管理器 = i18n_rust_engine::映射管理::映射管理器::自扁平映射新建(
            HashMap::from([
                ("函数".into(), "fn".into()),
                ("让".into(), "let".into()),
                ("可变".into(), "mut".into()),
            ]),
            HashMap::new(),
            HashMap::new(),
        );
        let 临时目录句柄 = tempfile::tempdir().unwrap();
        let 缓存 = 转译缓存::新建缓存(管理器, 临时目录句柄.path().to_path_buf());
        (缓存, 临时目录句柄)
    }

    #[test]
    fn 测试_位置转英文() {
        let (缓存, _临时句柄) = 构造测试缓存();
        let (条目, _) = 缓存
            .更新文档("file:///test/main.zh", "让 x = 1;", 1)
            .unwrap();
        assert_eq!(条目.英文源码, "let x = 1;");

        // 中文列 0（"让" 起点）→ 英文列 0
        let 位置 = json!({ "line": 0, "character": 0 });
        let 英文位置项 = 位置转英文(&条目, &位置);
        assert_eq!(英文位置项["line"], 0);
        assert_eq!(英文位置项["character"], 0);

        // 中文列 3（"x" 末尾）→ 英文列 5（"让" 为 1 个 UTF-16 单元，"let" 占 3 列）
        let 位置 = json!({ "line": 0, "character": 3 });
        let 英文位置项 = 位置转英文(&条目, &位置);
        assert_eq!(英文位置项["character"], 5);

        // 中文列 2（"x" 起点）→ 英文列 4
        let 位置 = json!({ "line": 0, "character": 2 });
        let 英文位置项 = 位置转英文(&条目, &位置);
        assert_eq!(英文位置项["character"], 4);
    }

    #[test]
    fn 测试_跨度转英文() {
        let (缓存, _临时句柄) = 构造测试缓存();
        let (条目, _) = 缓存
            .更新文档("file:///test/main.zh", "让 x = 1;", 1)
            .unwrap();

        let 跨度 = json!({
            "start": { "line": 0, "character": 2 },
            "end": { "line": 0, "character": 3 }
        });
        let 英文位置项 = 跨度转英文(&条目, &跨度);
        assert_eq!(英文位置项["start"]["character"], 4);
        assert_eq!(英文位置项["end"]["character"], 5);
    }

    #[test]
    fn 测试_重命名全链路() {
        // 模拟完整闭环：中文请求 → 转换转发 → rust-analyzer 响应 → 映射回母语
        let (缓存, _临时句柄) = 构造测试缓存();
        let 映射器实例 = 响应映射器::新建映射器(缓存.clone());
        let (条目, _) = 缓存
            .更新文档("file:///test/main.zh", "函数 主() {}", 1)
            .unwrap();
        assert_eq!(条目.英文源码, "fn 主() {}");

        // —— 请求方向（与 转发请求 相同的转换逻辑）——
        let 请求参数 = json!({
            "textDocument": { "uri": "file:///test/main.zh" },
            "position": { "line": 0, "character": 0 },
            "newName": "函数"
        });

        let mut 请求参数体 = 请求参数.clone();
        // 1. URI 替换为虚拟 URI
        请求参数体["textDocument"]["uri"] = Value::String(条目.虚拟资源定位.clone());
        // 2. 位置转换为英文坐标
        请求参数体["position"] = 位置转英文(&条目, &请求参数体["position"]);
        // 3. newName 中文 → 英文
        let 英文名称 = 缓存.关键词映射表().get("函数").cloned().unwrap();
        请求参数体["newName"] = Value::String(英文名称);

        assert_eq!(
            请求参数体["textDocument"]["uri"].as_str().unwrap(),
            条目.虚拟资源定位
        );
        assert_eq!(请求参数体["position"]["character"], 0);
        assert_eq!(请求参数体["newName"].as_str().unwrap(), "fn");

        // —— 响应方向（rust-analyzer 返回虚拟文件编辑）——
        let 响应体 = json!({
            "changes": {
                条目.虚拟资源定位.clone(): [{
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 2 }
                    },
                    "newText": "fn"
                }]
            }
        });
        let 映射结果 = 映射器实例.映射重命名响应(&响应体);
        let 编辑 = &映射结果["changes"]["file:///test/main.zh"][0];

        // 客户端收到的编辑：URI 还原、位置为母语坐标、newText 恢复中文
        assert_eq!(编辑["range"]["start"]["character"], 0);
        assert_eq!(编辑["range"]["end"]["character"], 2);
        assert_eq!(编辑["newText"].as_str().unwrap(), "函数");
    }

    #[test]
    fn 测试_文本末尾位置() {
        // 空文档
        assert_eq!(文本末尾位置(""), json!({ "line": 0, "character": 0 }));
        // 单行 ASCII
        assert_eq!(文本末尾位置("ab"), json!({ "line": 0, "character": 2 }));
        // 中文按 UTF-16 计数（4 个汉字 = 4 个单元）
        assert_eq!(
            文本末尾位置("行\n中文测试"),
            json!({ "line": 1, "character": 4 })
        );
        // 末尾换行：最后一行是空行
        assert_eq!(文本末尾位置("行\n"), json!({ "line": 1, "character": 0 }));
    }

    #[test]
    fn 测试_位置转偏移() {
        let 文字 = "让 可变 x = 5;\n让 y = 10;";
        // 首行中文列：「可变」起点（列 2）
        let 位置取值 = json!({ "line": 0, "character": 2 });
        assert_eq!(位置转偏移(文字, &位置取值), "让 ".len());
        // 第二行起点
        let 位置取值 = json!({ "line": 1, "character": 0 });
        assert_eq!(位置转偏移(文字, &位置取值), "让 可变 x = 5;\n".len());
        // 列号越界钳制到行尾（不含换行符）
        let 位置取值 = json!({ "line": 0, "character": 999 });
        assert_eq!(位置转偏移(文字, &位置取值), "让 可变 x = 5;".len());
        // 行号越界钳制到最后一行
        let 位置取值 = json!({ "line": 99, "character": 2 });
        assert_eq!(位置转偏移(文字, &位置取值), "让 可变 x = 5;\n让 ".len());
    }

    #[test]
    fn 测试_应用增量变更() {
        // 中文行内插入
        let mut 文字 = "让 x = 5;".to_string();
        let 跨度 = json!({
            "start": { "line": 0, "character": 2 },
            "end": { "line": 0, "character": 2 }
        });
        应用增量变更(&mut 文字, &跨度, "可变 ");
        assert_eq!(文字, "让 可变 x = 5;");

        // 跨行删除替换
        let mut 文字 = "行一\n行二\n行三".to_string();
        let 跨度 = json!({
            "start": { "line": 0, "character": 1 },
            "end": { "line": 1, "character": 1 }
        });
        应用增量变更(&mut 文字, &跨度, "新");
        // 删除「一\n行」并插入「新」
        assert_eq!(文字, "行新二\n行三");

        // 异常 range（end < start）不 panic，退化为插入
        let mut 文字 = "abc".to_string();
        let 跨度 = json!({
            "start": { "line": 0, "character": 3 },
            "end": { "line": 0, "character": 1 }
        });
        应用增量变更(&mut 文字, &跨度, "X");
        assert_eq!(文字, "abcX");
    }

    /// 从诊断列表提取未声明 crate 候选：英文原文命中、保留名单过滤、
    /// 无关消息不命中、跨诊断去重
    #[test]
    fn 测试_从诊断提取未解析依赖() {
        let 诊断列表 = vec![
            json!({"message": "unresolved import `serde_json`"}),
            json!({"message": "use of undeclared crate or module `reqwest`"}),
            json!({"message": "unresolved import `std::collections`"}),
            json!({"message": "unresolved import `crate::模块`"}),
            json!({"message": "cannot find value `x` in this scope"}),
            json!({"message": "unresolved import `serde_json`"}),
        ];
        assert_eq!(
            从诊断提取未解析依赖(&诊断列表),
            vec!["serde_json".to_string(), "reqwest".to_string()]
        );

        // 无诊断 / 无匹配消息 → 空列表
        assert!(从诊断提取未解析依赖(&[]).is_empty());
        let 其他向量 = vec![json!({"message": "mismatched types"})];
        assert!(从诊断提取未解析依赖(&其他向量).is_empty());
    }

    /// 给 rust-analyzer 的 workspace/configuration 回复：每条目固定三项关键配置，
    /// 数量与请求的 items 对齐（数量错误会让 RA 对部分 workspace 缺省）
    #[test]
    fn 测试_分析器配置结果形状() {
        let 空结果 = 分析器配置结果(0);
        assert_eq!(空结果, json!([]));
        let 应答结果 = 分析器配置结果(2);
        let 数组值 = 应答结果.as_array().expect("应为数组");
        assert_eq!(数组值.len(), 2);
        for 数组项 in 数组值 {
            assert_eq!(数组项["completion"]["snippets"], json!("custom"));
            assert_eq!(数组项["checkOnSave"]["enable"], json!(false));
            assert_eq!(数组项["check"]["enable"], json!(false));
        }
    }

    /// 是符号链接：lstat 语义——悬空链接仍是链接；普通文件/不存在路径为 false
    #[test]
    fn 测试_符号链接语义() {
        let 临时目录句柄 = tempfile::tempdir().unwrap();
        let 文件名段 = 临时目录句柄.path().join("f");
        std::fs::write(&文件名段, b"x").unwrap();
        assert!(!是符号链接(&文件名段));
        assert!(!是符号链接(&临时目录句柄.path().join("absent")));

        let 链接 = 临时目录句柄.path().join("link");
        std::os::unix::fs::symlink(&文件名段, &链接).unwrap();
        assert!(是符号链接(&链接));

        // 悬空符号链接（目标不存在）仍须识别为链接
        let 悬空 = 临时目录句柄.path().join("dangling");
        std::os::unix::fs::symlink(临时目录句柄.path().join("gone"), &悬空).unwrap();
        assert!(是符号链接(&悬空));
    }

    /// PID 探活：当前进程必然存活（残留目录清理依据，误判会误删活进程目录）
    #[test]
    fn 测试_进程存活_自身() {
        assert!(进程存活(std::process::id()));
    }

    /// 虚拟临时目录：按用户+PID 隔离命名、实际创建、同进程重复调用稳定同目录
    #[test]
    fn 测试_虚拟临时目录_稳定() {
        let 目录 = 虚拟临时目录().expect("应能创建虚拟目录");
        assert!(目录.is_dir());
        let 名称值 = 目录.file_name().unwrap().to_string_lossy().into_owned();
        assert!(名称值.starts_with("i18n_lsp_virtual_"), "实际：{名称值}");
        assert!(名称值.ends_with(&std::process::id().to_string()));
        let 再次结果 = 虚拟临时目录().expect("重复调用应幂等");
        assert_eq!(再次结果, 目录);
    }

    // ===== 处理分析器消息：RA 消息分发的分支级单测 =====

    /// 构造分发单测所需的全部部件：
    /// （mapper, pending, client 发送端, client 接收端, builtin_diags, 空 RA 回发器,
    ///  (cache, tempdir, 虚拟 URI)）
    #[allow(clippy::type_complexity)]
    fn 分发测试台(
        源码文本: &str,
    ) -> (
        Arc<响应映射器>,
        Arc<std::sync::Mutex<HashMap<i64, 待处理请求信息>>>,
        crossbeam_channel::Sender<Message>,
        crossbeam_channel::Receiver<Message>,
        Arc<std::sync::Mutex<HashMap<String, Vec<Value>>>>,
        crate::分析器::发送器,
        (Arc<转译缓存>, tempfile::TempDir, String),
    ) {
        let (缓存, 临时目录句柄) = 构造测试缓存();
        let (条目, _) = 缓存
            .更新文档("file:///test/main.zh", 源码文本, 1)
            .expect("测试文档应可入缓存");
        let 映射器实例 = Arc::new(响应映射器::新建映射器(缓存.clone()));
        let 待处理项 = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let 内置映射 = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let (发送端, 接收端) = crossbeam_channel::unbounded();
        let 回复 = crate::分析器::发送器::测试用空发送器();
        (
            映射器实例,
            待处理项,
            发送端,
            接收端,
            内置映射,
            回复,
            (缓存, 临时目录句柄, 条目.虚拟资源定位.clone()),
        )
    }

    /// 登记一条转发请求记录
    fn 插入待处理(
        待处理项: &Arc<std::sync::Mutex<HashMap<i64, 待处理请求信息>>>,
        分析器请求号: i64,
        客户端标识: i32,
        协议方法名: &str,
        资源定位: &str,
    ) {
        待处理项.lock().unwrap().insert(
            分析器请求号,
            待处理请求信息 {
                原始请求号: lsp_server::RequestId::from(客户端标识),
                method: 协议方法名.to_string(),
                原始资源定位: 资源定位.to_string(),
                创建时刻: std::time::Instant::now(),
                未解析依赖: if 协议方法名 == "textDocument/codeAction" {
                    vec!["serde_json".to_string()]
                } else {
                    Vec::new()
                },
                教学诊断: Vec::new(),
            },
        );
    }

    /// 全部响应映射分支：null 结果也必须安全通过各自 mapper 并应答到客户端，
    /// 原始客户端请求 ID 必须原样带回
    #[test]
    fn 测试_处理分析器响应_方法分支() {
        let (映射器实例, 待处理项, 发送端, 接收端, 内置映射, 回复, _句柄) =
            分发测试台("函数 主() {\n    打印行!();\n}\n");
        let 方法列表 = [
            "textDocument/completion",
            "textDocument/hover",
            "textDocument/signatureHelp",
            "textDocument/definition",
            "textDocument/references",
            "textDocument/documentSymbol",
            "textDocument/codeAction",
            "codeAction/resolve",
            "textDocument/rename",
            "textDocument/documentHighlight",
            "textDocument/semanticTokens/full",
            "textDocument/semanticTokens/range",
            "textDocument/unknownFutureMethod",
        ];
        for (循环序号, 协议方法名) in 方法列表.iter().enumerate() {
            插入待处理(
                &待处理项,
                循环序号 as i64 + 1,
                循环序号 as i32 + 100,
                协议方法名,
                "file:///test/main.zh",
            );
        }
        for 循环序号 in 0..方法列表.len() {
            let 消息体 =
                json!({ "jsonrpc": "2.0", "id": 循环序号 as i64 + 1, "result": Value::Null });
            处理分析器消息(&消息体, &映射器实例, &发送端, &待处理项, &回复, &内置映射);
        }
        // 13 条全部应答（按发送顺序即为 pending 表插入顺序）
        let 已收集: Vec<Message> = 接收端.try_iter().collect();
        assert_eq!(已收集.len(), 方法列表.len());
        for (循环序号, message) in 已收集.iter().enumerate() {
            if let Message::Response(应答) = message {
                assert_eq!(应答.id, lsp_server::RequestId::from(循环序号 as i32 + 100));
                assert!(应答.error.is_none(), "分支 {循环序号} 不应产生错误应答");
            } else {
                panic!("期望 Response，实际收到 {message:?}");
            }
        }
    }

    /// RA 返回错误（如 -32601 方法未知）：错误体原样透传给客户端，
    /// 缺省错误码归一为 -32603
    #[test]
    fn 测试_处理分析器错误_透传() {
        let (映射器实例, 待处理项, 发送端, 接收端, 内置映射, 回复, _句柄) =
            分发测试台("函数 主() {}\n");
        插入待处理(
            &待处理项,
            1,
            100,
            "textDocument/hover",
            "file:///test/main.zh",
        );
        let 消息体 = json!({
            "jsonrpc": "2.0", "id": 1,
            "error": { "code": -32601, "message": "method not found" }
        });
        处理分析器消息(&消息体, &映射器实例, &发送端, &待处理项, &回复, &内置映射);
        match 接收端.recv().unwrap() {
            Message::Response(应答) => {
                assert_eq!(应答.id, lsp_server::RequestId::from(100));
                let 错误对象 = 应答.error.expect("应透传错误");
                assert_eq!(错误对象.code, -32601);
                assert_eq!(错误对象.message, "method not found");
            }
            其他向量 => panic!("期望 Response：{其他向量:?}"),
        }
        // 缺 code：归一 -32603；缺 message：用内置文案
        插入待处理(
            &待处理项,
            2,
            101,
            "textDocument/hover",
            "file:///test/main.zh",
        );
        let 消息体 = json!({ "jsonrpc": "2.0", "id": 2, "error": {} });
        处理分析器消息(&消息体, &映射器实例, &发送端, &待处理项, &回复, &内置映射);
        match 接收端.recv().unwrap() {
            Message::Response(应答) => {
                let 错误对象 = 应答.error.unwrap();
                assert_eq!(错误对象.code, -32603);
                assert!(!错误对象.message.is_empty(), "缺 message 应用内置文案");
            }
            其他向量 => panic!("期望 Response：{其他向量:?}"),
        }
    }

    /// publishDiagnostics：非虚拟 URI 丢弃；虚拟 URI 经映射后转发到母语 URI，
    /// 并写入 builtin_diags 缓存供 cargo check 合并
    #[test]
    fn 测试_处理分析器诊断_路由() {
        let (映射器实例, 待处理项, 发送端, 接收端, 内置映射, 回复, 测试台句柄) =
            分发测试台("函数 主() {\n    打印行!();\n}\n");
        let (_缓存, _临时句柄, 虚拟资源定位) = 测试台句柄;

        // 非虚拟 URI：直接丢弃，客户端收不到
        let 外部消息 = json!({
            "jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
            "params": { "uri": "file:///other/project/src/main.rs", "diagnostics": [] }
        });
        处理分析器消息(&外部消息, &映射器实例, &发送端, &待处理项, &回复, &内置映射);
        assert!(接收端.is_empty(), "非虚拟文件诊断必须丢弃");

        // 虚拟 URI：映射回母语 URI 并转发
        let 诊断项 = json!({
            "range": {
                "start": { "line": 1, "character": 4 },
                "end": { "line": 1, "character": 12 }
            },
            "severity": 1,
            "source": "rust-analyzer",
            "message": "mismatched types",
            "code": "E0308"
        });
        let 虚拟消息 = json!({
            "jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
            "params": { "uri": 虚拟资源定位, "diagnostics": [诊断项] }
        });
        处理分析器消息(&虚拟消息, &映射器实例, &发送端, &待处理项, &回复, &内置映射);
        match 接收端.recv().unwrap() {
            Message::Notification(通知项) => {
                assert_eq!(通知项.method, "textDocument/publishDiagnostics");
                assert_eq!(
                    通知项.params["uri"].as_str(),
                    Some("file:///test/main.zh"),
                    "URI 应还原为母语文件"
                );
                assert!(
                    !通知项.params["diagnostics"].as_array().unwrap().is_empty(),
                    "原始诊断应保留（非 cargo 项目不抑制）"
                );
            }
            其他向量 => panic!("期望 Notification：{其他向量:?}"),
        }
        // builtin_diags 以母语 URI 为键缓存
        let 锁守卫 = 内置映射.lock().unwrap();
        let 命中缓存 = 锁守卫.get("file:///test/main.zh").expect("应缓存内置诊断");
        assert!(!命中缓存.is_empty());
    }

    /// 其他通知（logMessage 等）原样转发给客户端
    #[test]
    fn 测试_处理分析器_其他通知转发() {
        let (映射器实例, 待处理项, 发送端, 接收端, 内置映射, 回复, _句柄) =
            分发测试台("函数 主() {}\n");
        let 消息体 = json!({
            "jsonrpc": "2.0", "method": "window/logMessage",
            "params": { "type": 3, "message": "indexed" }
        });
        处理分析器消息(&消息体, &映射器实例, &发送端, &待处理项, &回复, &内置映射);
        match 接收端.recv().unwrap() {
            Message::Notification(通知项) => {
                assert_eq!(通知项.method, "window/logMessage");
                assert_eq!(通知项.params["message"].as_str(), Some("indexed"));
            }
            其他向量 => panic!("期望 Notification：{其他向量:?}"),
        }
    }

    /// RA 主动请求（无 pending 记录）：workspace/configuration 按请求项数构造
    /// 结果回发给 RA（本用例空发送器 send 失败被忽略，验证分发不 panic 且
    /// 不会误转发给客户端）
    #[test]
    fn 测试_处理分析器_无请求应答路径() {
        let (映射器实例, 待处理项, 发送端, 接收端, 内置映射, 回复, _句柄) =
            分发测试台("函数 主() {}\n");
        let 消息体 = json!({
            "jsonrpc": "2.0", "id": 999, "method": "workspace/configuration",
            "params": { "items": [ {}, {} ] }
        });
        处理分析器消息(&消息体, &映射器实例, &发送端, &待处理项, &回复, &内置映射);
        assert!(接收端.is_empty(), "回发给 RA 的响应不应出现在客户端通道");
        // 无 method 的杂讯（既非响应也非通知）：静默忽略
        let 噪声消息 = json!({ "jsonrpc": "2.0" });
        处理分析器消息(&噪声消息, &映射器实例, &发送端, &待处理项, &回复, &内置映射);
        assert!(接收端.is_empty());
    }

    /// 超时清理：仅超过 请求超时 的条目被移除并主动应答超时错误；
    /// 新条目保留且不产生消息
    #[test]
    fn 测试_清理过期请求_仅陈旧() {
        let (_映射器, 待处理项, 发送端, 接收端, _内置, _回复, _句柄) =
            分发测试台("函数 主() {}\n");
        // 新条目
        插入待处理(
            &待处理项,
            1,
            100,
            "textDocument/hover",
            "file:///test/main.zh",
        );
        // 过期条目
        {
            let mut 待处理表 = 待处理项.lock().unwrap();
            待处理表.get_mut(&1).unwrap().创建时刻 = std::time::Instant::now();
            待处理表.insert(
                2,
                待处理请求信息 {
                    原始请求号: lsp_server::RequestId::from(101),
                    method: "textDocument/hover".to_string(),
                    原始资源定位: "file:///test/main.zh".to_string(),
                    创建时刻: std::time::Instant::now()
                        .checked_sub(请求超时 + std::time::Duration::from_secs(1))
                        .unwrap(),
                    未解析依赖: Vec::new(),
                    教学诊断: Vec::new(),
                },
            );
        }
        清理过期请求(&待处理项, &发送端);
        // 仅 1 条超时应答
        match 接收端.try_recv() {
            Ok(Message::Response(应答)) => {
                assert_eq!(应答.id, lsp_server::RequestId::from(101));
                assert_eq!(应答.error.unwrap().code, -32603);
            }
            其他向量 => panic!("期望超时 Response：{其他向量:?}"),
        }
        assert!(接收端.is_empty(), "新条目不应产生应答");
        let 待处理表 = 待处理项.lock().unwrap();
        assert!(待处理表.contains_key(&1), "新条目必须保留");
        assert!(!待处理表.contains_key(&2), "过期条目必须移除");
    }

    /// RA 崩溃重启：全部挂起请求排空，逐条收到 "restarted" 错误应答
    #[test]
    fn 测试_失败全部待处理_清空报错() {
        let (_映射器, 待处理项, 发送端, 接收端, _内置, _回复, _句柄) =
            分发测试台("函数 主() {}\n");
        插入待处理(
            &待处理项,
            1,
            100,
            "textDocument/hover",
            "file:///test/main.zh",
        );
        插入待处理(
            &待处理项,
            2,
            101,
            "textDocument/completion",
            "file:///test/main.zh",
        );
        失败全部待处理(&待处理项, &发送端);
        assert!(待处理项.lock().unwrap().is_empty(), "排空后表应为空");
        let mut 标识列表 = Vec::new();
        while let Ok(Message::Response(应答)) = 接收端.try_recv() {
            assert_eq!(
                应答.error.as_ref().unwrap().message,
                "rust-analyzer restarted"
            );
            标识列表.push(应答.id);
        }
        assert_eq!(标识列表.len(), 2, "两个挂起请求都应被应答");
    }
}
