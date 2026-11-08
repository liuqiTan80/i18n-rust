//! 代理服务器模块
//!
//! 语言服务代理服务器的核心逻辑：
//! 1. 通过 lsp-server 与编辑器客户端建立语言服务连接
//! 2. 接收方言文件（`.zh` 等）变更，翻译后通知 rust-analyzer
//! 3. 转发 rust-analyzer 的响应与通知，并还原位置信息
//! 4. 把诊断消息翻译为对应语言

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use lsp_server::{Connection, Message, Notification, Request, Response};

use serde_json::Value;

use i18n_rust_engine::映射源;

use crate::分析器::分析器连接;
use crate::响应映射::响应映射器;
use crate::翻译缓存::{翻译条目, 路径转统一资源标识, 转译缓存};

/// 默认支持的方言文件扩展名（与内置语言包 lang_info.toml 的扩展名一致，
/// 单一来源：引擎 lang-packs 目录，避免清单与语言包漂移）；
/// 可通过命令行 `--extensions` 参数覆盖
fn 默认扩展名() -> Vec<String> {
    i18n_rust_engine::语言::内置语言扩展名()
        .into_iter()
        .map(|扩展值| format!(".{扩展值}"))
        .collect()
}

/// 语言服务代理服务器
pub struct 代理服务器 {
    /// 与编辑器客户端的语言服务连接
    客户端连接: Connection,
    /// 翻译缓存
    缓存: Arc<转译缓存>,
    /// rust-analyzer 子进程
    分析器: 分析器连接,
    /// 响应映射器
    响应映射器: Arc<响应映射器>,
    /// 自增请求标识
    请求计数器: Arc<std::sync::atomic::AtomicI64>,
    /// 待映射的 rust-analyzer 请求标识 → 原始客户端请求信息
    待处理请求表: 待处理请求表类型,
    /// 支持的方言文件扩展名列表（如 `.zh`、`.de`）
    支持扩展名表: Vec<String>,
    /// 上次重载时的模块集合版本号（初值 -1 保证首个文档打开时重载一次）
    上次模块版本: std::sync::atomic::AtomicI64,
    /// rust-analyzer 声明的语义着色能力（初始化响应透传给客户端，
    /// 保证着色图例与记号类型索引一致，否则变量等语义着色错乱）
    语义记号提供者: std::sync::Mutex<Option<Value>>,
    /// 最近一次转发的内置诊断（方言资源标识 → 诊断列表）：代理自跑编译检查
    /// 后合并发布，避免检查结果覆盖语法/类型等实时诊断
    内置诊断表: 内置诊断表类型,
    /// 编译检查是否在运行（保存频繁时跳过进行中的检查，避免并发卡锁）
    检查进行中: Arc<std::sync::atomic::AtomicBool>,
    /// 运行期间是否收到新的保存请求（检查完成后补跑一次，
    /// 合并连续保存的中间状态，避免最终诊断停留在旧版本）
    待补检查: Arc<std::sync::atomic::AtomicBool>,
}

/// 记录一个转发给 rust-analyzer 的请求的原始信息
#[derive(Debug, Clone)]
struct 待处理请求信息 {
    原始标识: lsp_server::RequestId,
    请求方法: String,
    原始资源标识: String,
    /// 转发时刻（用于超时清理，避免响应永不到达时条目无限累积）
    创建时刻: std::time::Instant,
    /// 代码操作请求上下文诊断中提取的未声明包名
    ///（响应时注入“添加依赖”快捷修复；非代码操作请求为空）
    未解析包名表: Vec<String>,
    /// 代码操作请求上下文中的教学诊断（全角标点/教学检查，方言坐标）：
    /// 响应时注入对应快捷修复（一键替换半角/忽略此行）；非代码操作请求为空
    教学诊断表: Vec<Value>,
}

/// 待映射请求表：rust-analyzer 请求标识 → 原始客户端请求信息
type 待处理请求表类型 = Arc<std::sync::Mutex<HashMap<i64, 待处理请求信息>>>;

/// 内置诊断表：方言资源标识 → 诊断列表
type 内置诊断表类型 = Arc<std::sync::Mutex<HashMap<String, Vec<Value>>>>;

/// 转发请求的等待超时：超过后向客户端应答错误并丢弃条目
const 请求超时: std::time::Duration = std::time::Duration::from_secs(60);

impl 代理服务器 {
    /// 创建并初始化代理服务器
    ///
    /// 扩展名列表：支持的方言文件扩展名（如 `.zh`），
    /// 传空列表时使用默认值（`.zh` / `.de`）。
    pub fn 新建(
        语言包路径: &Path,
        扩展名列表: &[String],
    ) -> anyhow::Result<(Self, lsp_server::IoThreads)> {
        // 1. 加载语言包（统一映射管理器，含模块路径映射）
        let 管理器 = 载入语言包(语言包路径)?;
        // 初始化诊断翻译器（errors.toml 消息表 + 由映射管理器构建的类型映射表，
        // 与命令行同源），供诊断映射与镜像检查翻译 rust-analyzer 诊断（E0004 等）
        crate::响应映射::初始化诊断翻译器(语言包路径, &管理器);
        log::info!(
            "{}",
            crate::界面消息::全局界面().取带参消息(
                "lsp_log_loaded_mappings",
                &[
                    &管理器.关键词表.len().to_string(),
                    &管理器.取宏表().len().to_string()
                ]
            )
        );

        // 2. 创建翻译缓存（临时目录按用户隔离，避免多用户共享系统临时目录）
        let 临时目录值 = 虚拟临时目录()?;
        let 缓存实例 = 转译缓存::新建(管理器, 临时目录值);

        // 3. 启动 rust-analyzer
        //
        // 启动失败（未安装/不可执行）不在此处返回错误：此刻尚未收到客户端
        // 的初始化请求，进程一旦退出，编辑器只能显示“服务端初始化失败”。
        // 失败原因先记入连接，由 `运行` 在握手后告知客户端。
        let 分析器实例 = 分析器连接::启动();

        // 4. 创建响应映射器
        let 映射器实例 = Arc::new(响应映射器::新建(缓存实例.clone()));

        // 5. 建立语言服务连接（标准输入输出）
        let (客户端连接, 输入输出线程) = Connection::stdio();

        let 服务器实例 = Self {
            客户端连接,
            缓存: 缓存实例,
            分析器: 分析器实例,
            响应映射器: 映射器实例,
            请求计数器: Arc::new(std::sync::atomic::AtomicI64::new(1000)),
            待处理请求表: Arc::new(std::sync::Mutex::new(HashMap::new())),
            支持扩展名表: if 扩展名列表.is_empty() {
                默认扩展名()
            } else {
                扩展名列表.to_vec()
            },
            上次模块版本: std::sync::atomic::AtomicI64::new(-1),
            语义记号提供者: std::sync::Mutex::new(None),
            内置诊断表: Arc::new(std::sync::Mutex::new(HashMap::new())),
            检查进行中: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            待补检查: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };

        Ok((服务器实例, 输入输出线程))
    }

    /// 运行服务器主循环
    pub fn 运行(self, 输入输出线程: lsp_server::IoThreads) -> anyhow::Result<()> {
        // 1. 等待初始化请求并握手
        let (初始化标识, 初始化参数) = self.握手()?;
        log::info!(
            "{}",
            crate::界面消息::全局界面().取消息("lsp_log_client_init")
        );

        // 2. 初始化 rust-analyzer
        //
        // 失败必须先把原因作为 `window/showMessage` 推给客户端，否则用户
        // 只看到“服务端初始化失败”与“连接已释放，待处理响应被拒绝”
        // 两条无法行动的提示。
        if let Some(错误原因) = self.分析器.启动失败原因() {
            return self.初始化失败(初始化标识, &错误原因);
        }
        if let Err(错误值) = self.初始化分析器(&初始化参数) {
            return self.初始化失败(初始化标识, &错误值.to_string());
        }

        // 3. 回复客户端初始化响应
        self.应答初始化(初始化标识)?;

        // 4. 启动 rust-analyzer 消息转发线程
        self.启动转发线程()?;

        // 5. 进入主循环
        self.主循环()?;

        // 6. 主循环结束后（收到关闭请求或客户端断开）清理资源。
        // 消息转发线程持有客户端发送端的克隆，并阻塞在 rust-analyzer
        // 的消息通道上；必须先停止 rust-analyzer 子进程让转发线程退出，
        // 再释放服务器（含客户端连接发送端），否则输入输出写入线程
        // 因通道永不关闭而无法结束，进程将无法退出。
        let 服务器实例 = self;
        服务器实例.分析器.停止();
        drop(服务器实例);

        // 7. 等待输入输出线程结束
        输入输出线程.join()?;

        Ok(())
    }

    /// 初始化失败时告知客户端并终止
    ///
    /// 顺序必须是「先通知、后错误响应」：客户端收到初始化错误响应会
    /// 立即释放连接（对应“连接已释放，待处理响应被拒绝”），之后再发的
    /// 任何消息均不可达。
    ///
    /// 返回错误让程序入口以非零码退出，并把原因留在标准错误（输出面板可见）。
    fn 初始化失败(
        &self, 标识: lsp_server::RequestId, 错误原因: &str
    ) -> anyhow::Result<()> {
        let 消息串 = crate::界面消息::全局界面().取带参消息("lsp_fail_notify", &[错误原因]);
        log::error!("{消息串}");
        // 失败时可能已经拉起子进程（如初始化超时），先收尾避免孤儿进程
        self.分析器.停止();
        let 通知项 = lsp_server::Notification::new(
            "window/showMessage".to_string(),
            serde_json::json!({ "type": 1, "message": 消息串 }),
        );
        let _ = self.客户端连接.sender.send(Message::Notification(通知项));
        let _ = self
            .客户端连接
            .sender
            .send(Message::Response(Response::new_err(
                标识,
                lsp_server::ErrorCode::InternalError as i32,
                错误原因.to_string(),
            )));
        anyhow::bail!("{错误原因}")
    }

    /// 握手：接收初始化请求并回复
    fn 握手(&self) -> anyhow::Result<(lsp_server::RequestId, Value)> {
        let 消息值 = self.客户端连接.receiver.recv().map_err(|错误值| {
            anyhow::anyhow!(
                "{}",
                crate::界面消息::全局界面()
                    .取带参消息("lsp_err_recv_initialize", &[&错误值.to_string()])
            )
        })?;

        match 消息值 {
            Message::Request(请求值) => {
                if 请求值.method == "initialize" {
                    let 标识 = 请求值.id.clone();
                    let 参数值 = 请求值.params.clone();
                    Ok((标识, 参数值))
                } else {
                    anyhow::bail!(
                        "{}",
                        crate::界面消息::全局界面()
                            .取带参消息("lsp_err_expect_initialize", &[&请求值.method])
                    )
                }
            }
            _ => anyhow::bail!(
                "{}",
                crate::界面消息::全局界面().取消息("lsp_err_expect_request")
            ),
        }
    }

    /// 向 rust-analyzer 发送初始化请求并等待响应
    fn 初始化分析器(&self, 参数值: &Value) -> anyhow::Result<()> {
        // 不透传客户端的根资源标识/工作区文件夹：
        // 客户端工作区（如整个项目）与母语文件无关，
        // 透传会导致 rust-analyzer 全量分析并发布海量诊断，
        // 阻塞代理到客户端的消息通道。
        // 改用虚拟项目目录作为 rust-analyzer 的工作区根，
        // 使其只分析自动生成的虚拟 `.rs` 文件。
        let _ = 参数值;
        let 虚拟项目 = self.缓存.虚拟项目资源标识();

        let 初始化请求 = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": 虚拟项目,
                "capabilities": {
                    "textDocument": {
                        "completion": {
                            "completionItem": {
                                // 启用代码片段：rust-analyzer 的方法/关键字补全
                                // 依赖它才能附带括号（如 `长度()`）与占位符
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
                    "uri": 虚拟项目,
                    "name": "i18n-virtual"
                }],
                // 虚拟项目无依赖/无构建脚本/无过程宏：关闭相应后台任务，
                // 减少启动与保存时的编译开销；诊断保留由代理自跑检查发布
                "initializationOptions": {
                    "cargo": { "buildScripts": { "enable": false } },
                    "procMacro": { "enable": false },
                    // 注：不启用 rust-analyzer 的保存即检查——其诊断在虚拟项目
                    // 上不可达（编译服务常驻但无诊断发布）；改为代理在保存时
                    // 自跑编译检查并发布（见 `触发编译检查`）
                    // 自定义代码片段：方法/关键字补全附带括号与占位符
                    //（如 `长度()`、`让 可变 ${1:x}`），配合片段支持使用
                    "completion": { "snippets": "custom" }
                }
            }
        });

        self.分析器.发送消息(&初始化请求)?;

        // 等待 rust-analyzer 的初始化响应（超时则直接报错，
        // 避免在半初始化状态下继续服务导致行为不可预测）
        let mut 初始化完成 = false;
        for _ in 0..200 {
            if let Some(消息项) = self.分析器.尝试接收() {
                if 消息项.get("id").and_then(|值| 值.as_i64()) == Some(0) {
                    // 保存 rust-analyzer 的语义着色能力声明：初始化响应
                    // 透传给客户端，保证着色图例与记号类型索引一致
                    //（否则变量/参数等语义记号的类型索引错位，着色乱或不显示）
                    let 提供者 = 消息项["result"]["capabilities"]["semanticTokensProvider"].clone();
                    if !提供者.is_null()
                        && let Ok(mut 槽位) = self.语义记号提供者.lock()
                    {
                        *槽位 = Some(提供者);
                    }
                    log::info!(
                        "{}",
                        crate::界面消息::全局界面().取消息("lsp_log_ra_init_done")
                    );
                    初始化完成 = true;
                    break;
                }
                // 初始化期间 rust-analyzer 可能主动请求配置
                //（`workspace/configuration`）：必须立即响应，否则它一直等待
                // 导致配置加载挂起。返回补全片段配置并禁用其自跑编译检查
                //（见 `分析器配置答复` 的说明），其余键维持默认。
                if 消息项.get("method").and_then(|值| 值.as_str())
                    == Some("workspace/configuration")
                {
                    let 条目数 = 消息项["params"]["items"]
                        .as_array()
                        .map(|数组值| 数组值.len())
                        .unwrap_or(0);
                    let 答复值 = 分析器配置答复(条目数);
                    let 响应值 = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": 消息项["id"].clone(),
                        "result": 答复值
                    });
                    if let Err(错误值) = self.分析器.发送消息(&响应值) {
                        log::warn!("配置响应发送失败: {错误值}");
                    }
                }
            }
            thread::sleep(std::time::Duration::from_millis(50));
        }
        if !初始化完成 {
            anyhow::bail!(
                "{}",
                crate::界面消息::全局界面().取消息("lsp_err_ra_init_timeout")
            );
        }

        // 注意：初始化完成通知不在本处发送，
        // 而是等客户端发来初始化完成通知时再转发（见 `处理客户端通知`），
        // 避免重复发送导致 rust-analyzer 报未处理的通知。

        Ok(())
    }

    /// 回复客户端初始化响应
    fn 应答初始化(&self, 标识: lsp_server::RequestId) -> anyhow::Result<()> {
        let mut 能力表 = serde_json::json!({
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
        // 透传 rust-analyzer 的语义着色能力（含着色图例），
        // 使客户端请求语义记号并正确渲染变量/参数等颜色
        if let Ok(槽位) = self.语义记号提供者.lock()
            && let Some(提供者) = 槽位.as_ref()
        {
            能力表["capabilities"]["semanticTokensProvider"] = 提供者.clone();
        }

        let 响应值 = Response {
            id: 标识,
            response_result: Ok(能力表),
        };

        self.客户端连接
            .sender
            .send(Message::Response(响应值))
            .map_err(|错误值| {
                anyhow::anyhow!(
                    "{}",
                    crate::界面消息::全局界面()
                        .取带参消息("lsp_err_send_initialize", &[&错误值.to_string()])
                )
            })?;
        Ok(())
    }

    /// 启动 rust-analyzer → 客户端的消息转发线程
    fn 启动转发线程(&self) -> anyhow::Result<()> {
        let 接收器 = self.分析器.消息通道();
        let 映射器 = self.响应映射器.clone();
        let 发送方 = self.客户端连接.sender.clone();
        let 待处理表 = self.待处理请求表.clone();
        // rust-analyzer 主动请求（如配置、诊断刷新）
        // 的响应需要回发给 rust-analyzer 本身，而非客户端。
        let 分析器发送端 = self.分析器.取发送端副本();
        // 内置诊断缓存：编译检查结果合并发布时使用
        let 内置诊断 = self.内置诊断表.clone();

        thread::spawn(move || {
            // 用带超时的接收代替阻塞接收，使无消息时也能周期性
            // 清理超时请求（rust-analyzer 挂起/丢弃请求时向客户端应答错误）
            let mut 上次清理 = std::time::Instant::now();
            loop {
                match 接收器.recv_timeout(std::time::Duration::from_secs(1)) {
                    Ok(消息值) => 处理分析器消息(
                        &消息值,
                        &映射器,
                        &发送方,
                        &待处理表,
                        &分析器发送端,
                        &内置诊断,
                    ),
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
                if 上次清理.elapsed() >= std::time::Duration::from_secs(10) {
                    清理超时请求(&待处理表, &发送方);
                    上次清理 = std::time::Instant::now();
                }
            }
            log::info!(
                "{}",
                crate::界面消息::全局界面().取消息("lsp_log_ra_thread_exit")
            );
        });

        Ok(())
    }

    /// 主消息循环
    ///
    /// 用带超时的接收轮询代替阻塞接收：空闲时周期性检查
    /// rust-analyzer 崩溃标志，触发自动重启（进程崩溃后编辑器内
    /// 功能会静默失效，必须主动恢复而非空转等待）。
    fn 主循环(&self) -> anyhow::Result<()> {
        loop {
            let 消息值 = match self
                .客户端连接
                .receiver
                .recv_timeout(std::time::Duration::from_millis(100))
            {
                Ok(消息项) => 消息项,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    // rust-analyzer 异常退出：自动重启并重做完整握手
                    if self.分析器.是否崩溃()
                        && let Err(错误值) = self.重启分析器()
                    {
                        log::error!(
                            "{}",
                            crate::界面消息::全局界面()
                                .取带参消息("lsp_err_ra_restart", &[&错误值.to_string()])
                        );
                        // 重启失败（如二进制被卸载）：通知客户端并暂停自动重试，
                        // 避免每 100 毫秒刷一次错误日志与提示
                        let 参数值 = serde_json::json!({
                            "type": 1,
                            "message": crate::界面消息::全局界面().取带参消息(
                                "lsp_err_ra_restart",
                                &[&错误值.to_string()]
                            )
                        });
                        let _ = self
                            .客户端连接
                            .sender
                            .send(Message::Notification(Notification {
                                method: "window/showMessage".to_string(),
                                params: 参数值,
                            }));
                        self.分析器.清除崩溃();
                    }
                    continue;
                }
                Err(_) => {
                    log::info!(
                        "{}",
                        crate::界面消息::全局界面().取消息("lsp_log_client_disconnected")
                    );
                    break;
                }
            };

            match 消息值 {
                Message::Request(请求值) => {
                    if 请求值.method == "shutdown" {
                        log::info!("{}", crate::界面消息::全局界面().取消息("lsp_log_shutdown"));
                        let 响应值 = Response {
                            id: 请求值.id,
                            response_result: Ok(Value::Null),
                        };
                        let _ = self.客户端连接.sender.send(Message::Response(响应值));
                        break;
                    }
                    // 单条请求处理失败（如 rust-analyzer 写入失败）只记录错误，
                    // 不退出服务器，避免一次偶发故障杀死整个会话
                    if let Err(错误值) = self.处理客户端请求(请求值) {
                        log::error!(
                            "{}",
                            crate::界面消息::全局界面()
                                .取带参消息("lsp_err_handle_request", &[&错误值.to_string()])
                        );
                    }
                }
                Message::Notification(通知值) => {
                    if let Err(错误值) = self.处理客户端通知(通知值) {
                        log::error!(
                            "{}",
                            crate::界面消息::全局界面().取带参消息(
                                "lsp_err_handle_notification",
                                &[&错误值.to_string()]
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
        log::warn!(
            "{}",
            crate::界面消息::全局界面().取消息("lsp_log_ra_crashed")
        );
        self.分析器.重启()?;
        // 重握手：初始化（含语义着色能力捕获）
        self.初始化分析器(&Value::Null)?;
        // 初始化完成通知：客户端不会重复发送，此处必须手动补发
        self.分析器.发送消息(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }))?;
        // 工作区：初始化请求已携带虚拟项目工作区文件夹，
        // 新进程按磁盘现状直接加载（`main.rs` 已由模块集合变化写盘），
        // 无需再发工作区变更通知
        // 重新打开所有已打开文档的虚拟文件（其磁盘副本与缓冲区不同步，
        // 必须重发打开通知提供内存副本）；兄弟模块未打开，不发打开通知
        // ——否则 rust-analyzer 以内存副本为准，后续磁盘更新通知不再生效
        for 条目值 in self.缓存.全部条目() {
            if !条目值.已打开 {
                continue;
            }
            let 消息项 = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": 条目值.虚拟资源标识,
                        "languageId": "rust",
                        "version": 条目值.文档版本,
                        "text": 条目值.分析器内容
                    }
                }
            });
            self.分析器.发送消息(&消息项)?;
        }
        // 旧进程遗留的待映射请求永远不会有响应：统一向客户端应答错误，
        // 避免客户端永久等待（否则编辑器内对应请求一直转圈）
        全部待处理失败应答(&self.待处理请求表, &self.客户端连接.sender);
        log::info!(
            "{}",
            crate::界面消息::全局界面().取消息("lsp_log_ra_restarted")
        );
        Ok(())
    }

    /// 处理客户端请求
    fn 处理客户端请求(&self, 请求值: Request) -> anyhow::Result<()> {
        log::debug!(
            "{}",
            crate::界面消息::全局界面().取带参消息(
                "lsp_log_client_request",
                &[&请求值.method, &format!("{:?}", 请求值.id)]
            )
        );

        match 请求值.method.as_str() {
            "initialize" => Ok(()), // 已在握手中处理
            // 格式化采用全文件替换策略，由代理自行处理（不转发 rust-analyzer）
            "textDocument/formatting" => self.处理格式化(请求值),
            "textDocument/completion"
            | "textDocument/hover"
            | "textDocument/definition"
            | "textDocument/references"
            | "textDocument/documentSymbol"
            | "textDocument/codeAction"
            | "textDocument/rename"
            | "textDocument/documentHighlight"
            | "textDocument/signatureHelp" => self.转发请求(请求值),
            _ => self.转发请求(请求值),
        }
    }

    /// 处理客户端通知
    fn 处理客户端通知(&self, 通知值: Notification) -> anyhow::Result<()> {
        log::debug!(
            "{}",
            crate::界面消息::全局界面()
                .取带参消息("lsp_log_client_notification", &[&通知值.method])
        );

        match 通知值.method.as_str() {
            "textDocument/didOpen" => self.处理文档打开(&通知值.params),
            "textDocument/didChange" => self.处理文档变更(&通知值.params),
            "textDocument/didClose" => self.处理文档关闭(&通知值.params),
            "textDocument/didSave" => self.处理文档保存(&通知值.params),
            "initialized" => {
                // 转发初始化完成通知给 rust-analyzer。
                // 虚拟项目工作区已在初始化请求中作为工作区文件夹
                // 声明：rust-analyzer 收到后统一加载，
                // 避免二次加载与工作区重载的并发竞态
                //（历史上重载模块的发送错误崩溃即源于此）。
                let 消息项 = serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "initialized",
                    "params": {}
                });
                self.分析器.发送消息(&消息项)
            }
            _ => {
                let 消息项 = serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": 通知值.method,
                    "params": 通知值.params
                });
                self.分析器.发送消息(&消息项)
            }
        }
    }

    /// 判断资源标识是否为受支持的方言文件
    fn 是受支持文件(&self, 资源标识: &str) -> bool {
        是受支持文件(资源标识, &self.支持扩展名表)
    }

    /// 模块集合变化时通知 rust-analyzer 重读聚合主文件
    ///
    /// 打开/关闭方言文件会重写虚拟项目的聚合主文件；rust-analyzer 的
    /// 文件系统监听对虚拟项目（临时目录下的临时目录）不可靠，其虚拟
    /// 文件系统中的主文件内容会停留在旧版本，导致模块文件被判定为
    /// “不属于模块树中的任何位置”（未链接文件误报）。此处显式以
    /// 文件变更监听（已变更）告知主文件已更新，让 rust-analyzer 重读该文件
    ///（实测比工作区先移除再添加的重载更快，且不触发重载竞态崩溃）。
    fn 模块变化时通知主文件更新(&self) -> anyhow::Result<()> {
        let 新版本 = self.缓存.模块版本() as i64;
        let 旧版本 = self
            .上次模块版本
            .swap(新版本, std::sync::atomic::Ordering::SeqCst);
        if 旧版本 == 新版本 {
            return Ok(());
        }
        let 主文件标识 =
            路径转统一资源标识(&self.缓存.虚拟项目目录().join("src").join("main.rs"));
        let 通知项 = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "workspace/didChangeWatchedFiles",
            "params": {
                "changes": [{ "uri": 主文件标识, "type": 2 }]
            }
        });
        self.分析器.发送消息(&通知项)
    }

    /// 其他虚拟条目的内容变化通知：按条目打开状态路由
    ///
    /// 打开中的文档在 rust-analyzer 中有对应的内存副本，必须用变更通知
    /// 全量同步（对已打开文档重复打开通知违反语言服务协议）；兄弟模块
    /// 未在编辑器中打开，其虚拟文件由代理直接写盘，用文件变更监听
    ///（已变更）让 rust-analyzer 从磁盘重读。
    fn 通知虚拟条目变化(&self, 条目列表: &[Arc<翻译条目>]) -> anyhow::Result<()> {
        for 条目值 in 条目列表 {
            let 分析器消息 = if 条目值.已打开 {
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/didChange",
                    "params": {
                        "textDocument": {
                            "uri": 条目值.虚拟资源标识,
                            "version": 条目值.文档版本
                        },
                        "contentChanges": [{ "text": 条目值.分析器内容 }]
                    }
                })
            } else {
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "workspace/didChangeWatchedFiles",
                    "params": {
                        "changes": [{ "uri": 条目值.虚拟资源标识, "type": 2 }]
                    }
                })
            };
            self.分析器.发送消息(&分析器消息)?;
        }
        Ok(())
    }

    /// 处理文档打开
    fn 处理文档打开(&self, 参数值: &Value) -> anyhow::Result<()> {
        let 文档 = &参数值["textDocument"];
        let 资源标识 = 文档["uri"].as_str().unwrap_or("");
        let 内容值 = 文档["text"].as_str().unwrap_or("");
        let 文档版本 = 文档["version"].as_i64().unwrap_or(1) as i32;

        if !self.是受支持文件(资源标识) {
            return Ok(());
        }

        let (条目值, 其他变更) = self.缓存.更新文档(资源标识, 内容值, 文档版本)?;

        // 模块集合变化时通知 rust-analyzer 重读聚合主文件，
        // 确保其识别新模块（虚拟项目的文件系统监听不可靠）。
        self.模块变化时通知主文件更新()?;

        // 模块集合变化可能导致其他文件被重写（其虚拟内容新增/移除
        // 包前缀）：打开中的用变更通知同步，兄弟模块用文件变更监听
        // 通知磁盘重读
        self.通知虚拟条目变化(&其他变更)?;

        let 分析器消息 = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": 条目值.虚拟资源标识,
                    "languageId": "rust",
                    "version": 文档版本,
                    "text": 条目值.分析器内容
                }
            }
        });
        self.分析器.发送消息(&分析器消息)?;
        log::info!(
            "{}",
            crate::界面消息::全局界面()
                .取带参消息("lsp_log_doc_opened", &[资源标识, &文档版本.to_string()])
        );

        // 打开即跑权威检查（真实项目镜像）：无需保存即可见编译级诊断；
        // 镜像不可用时回退虚拟项目检查（草稿虚拟为假时的兜底）
        self.触发编译检查(资源标识, false)?;
        Ok(())
    }

    /// 处理文档变更
    ///
    /// 代理声明增量同步（变更类型 2）以减少客户端→代理的传输量：
    /// 无范围的变更项为全量文本（兼容旧客户端），
    /// 带范围的按语言服务位置（UTF-16）逐项应用到缓存中的母语文本。
    /// 应用后仍全量重译，并以全量替换通知 rust-analyzer。
    fn 处理文档变更(&self, 参数值: &Value) -> anyhow::Result<()> {
        let 文档 = &参数值["textDocument"];
        let 资源标识 = 文档["uri"].as_str().unwrap_or("");
        let 文档版本 = 文档["version"].as_i64().unwrap_or(1) as i32;

        if !self.是受支持文件(资源标识) {
            return Ok(());
        }

        let Some(变更表) = 参数值["contentChanges"].as_array() else {
            return Ok(());
        };

        // 在缓存旧文本上按序应用变更，得到新全文
        let mut 内容值 = self
            .缓存
            .查询原始(资源标识)
            .map(|条目项| 条目项.母语内容.clone())
            .unwrap_or_default();
        for 变更项 in 变更表 {
            if let Some(文本串) = 变更项["text"].as_str() {
                if 变更项.get("range").is_none() {
                    // 全量替换
                    内容值 = 文本串.to_string();
                } else {
                    应用增量变更(&mut 内容值, &变更项["range"], 文本串);
                }
            }
        }

        let (条目值, 其他变更) = self.缓存.更新文档(资源标识, &内容值, 文档版本)?;
        let 分析器消息 = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {
                    "uri": 条目值.虚拟资源标识,
                    "version": 文档版本
                },
                "contentChanges": [{ "text": 条目值.分析器内容 }]
            }
        });
        self.分析器.发送消息(&分析器消息)?;
        // 兄弟模块集合可能增删（磁盘文件变化）：聚合主文件需重读；
        // 其他条目内容被重写时按打开状态分别同步
        self.模块变化时通知主文件更新()?;
        self.通知虚拟条目变化(&其他变更)?;
        Ok(())
    }

    /// 处理文档关闭
    fn 处理文档关闭(&self, 参数值: &Value) -> anyhow::Result<()> {
        let 资源标识 = 参数值["textDocument"]["uri"].as_str().unwrap_or("");
        if !self.是受支持文件(资源标识) {
            return Ok(());
        }

        if let Some(条目值) = self.缓存.查询原始(资源标识) {
            let 分析器消息 = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didClose",
                "params": {
                    "textDocument": { "uri": 条目值.虚拟资源标识 }
                }
            });
            self.分析器.发送消息(&分析器消息)?;
        }

        // 关闭文档会缩小模块集合，其余条目的虚拟内容可能被重写
        let 其他变更 = self.缓存.关闭文档(资源标识)?;

        // 模块集合变化时通知 rust-analyzer 重读聚合主文件（模块已移除）
        self.模块变化时通知主文件更新()?;

        // 其余条目按打开状态同步：打开中的用变更通知全量同步，
        // 兄弟模块（含刚降级的本条）用文件变更监听从磁盘重读
        self.通知虚拟条目变化(&其他变更)?;
        Ok(())
    }

    /// 处理文档保存
    fn 处理文档保存(&self, 参数值: &Value) -> anyhow::Result<()> {
        let 资源标识 = 参数值["textDocument"]["uri"].as_str().unwrap_or("");
        if !self.是受支持文件(资源标识) {
            return Ok(());
        }

        if let Some(条目值) = self.缓存.查询原始(资源标识) {
            let 分析器消息 = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didSave",
                "params": {
                    "textDocument": { "uri": 条目值.虚拟资源标识 },
                    "text": 条目值.分析器内容
                }
            });
            self.分析器.发送消息(&分析器消息)?;
        }

        // 代理自跑编译检查并发布诊断：先虚拟项目草稿（快速反馈），
        // 再真实项目镜像（权威，与 rzc 构建口径一致）。rust-analyzer
        // 的保存即检查在虚拟项目上诊断不可达（编译服务常驻但无发布），
        // 而所有权可视化依赖 E0382 等检查诊断（移动黄/使用红/生命周期绿）
        self.触发编译检查(资源标识, true)?;
        Ok(())
    }

    /// 异步执行编译检查并发布诊断
    ///
    /// 两条通路：
    /// - 虚拟项目草稿检查（`草稿虚拟` 为真时先跑）：教学场景无第三方
    ///   依赖，秒级反馈；
    /// - 真实项目镜像检查（权威）：项目树复制 + 方言转译产物覆盖 +
    ///   离线编译检查，复用真实构建目录的依赖产物，
    ///   诊断口径与 rzc 构建完全一致；镜像不可用时回退虚拟检查。
    ///
    /// `保存资源标识`：触发本次检查的方言文件（定位项目根）；
    /// `草稿虚拟`：是否先跑虚拟项目草稿检查（保存为真，
    /// 打开为假——镜像已含全部信息，草稿无必要）。
    fn 触发编译检查(
        &self, 保存资源标识: &str, 草稿虚拟: bool
    ) -> anyhow::Result<()> {
        let 缓存实例 = self.缓存.clone();
        let 映射器 = self.响应映射器.clone();
        let 发送方 = self.客户端连接.sender.clone();
        let 内置诊断 = self.内置诊断表.clone();
        let 检查进行中标志 = self.检查进行中.clone();
        let 待补检查标志 = self.待补检查.clone();
        let 扩展名表 = self.支持扩展名表.clone();
        // 教学检查词表索引由管理器缓存：跨线程只带一份引用计数，
        // 线程内按需取分桶索引（早先每次保存克隆整张词表一千余项）
        let 管理器 = 映射器.管理器共享();
        let 保存资源标识 = 保存资源标识.to_string();
        let 项目目录 = 缓存实例.虚拟项目目录();

        // 并发保护：上次检查未结束（可能卡在锁等待）时，标记待重跑
        // 并立即返回——检查完成后补跑一次合并中间状态，避免连续保存
        // 时中间检查被静默丢弃，最终诊断停留在旧版本
        if 检查进行中标志.swap(true, std::sync::atomic::Ordering::SeqCst) {
            待补检查标志.store(true, std::sync::atomic::Ordering::SeqCst);
            return Ok(());
        }

        std::thread::spawn(move || {
            // 循环执行：期间收到新保存请求（待补位置位）则补跑一次；
            // 待补标志由首轮消费，最多连跑两次，合并中间全部状态
            loop {
                待补检查标志.store(false, std::sync::atomic::Ordering::SeqCst);
                if 草稿虚拟 {
                    Self::运行编译检查一次(
                        &缓存实例,
                        &映射器,
                        &发送方,
                        &内置诊断,
                        &项目目录,
                    );
                }
                // 镜像检查（权威）；不可用时回退虚拟检查
                //（保存时已跑过草稿，无需重复）
                let 镜像可用 = crate::镜像检查::运行镜像检查(
                    &缓存实例,
                    &发送方,
                    &内置诊断,
                    &扩展名表,
                    &保存资源标识,
                    &管理器,
                );
                if !镜像可用 && !草稿虚拟 {
                    Self::运行编译检查一次(
                        &缓存实例,
                        &映射器,
                        &发送方,
                        &内置诊断,
                        &项目目录,
                    );
                }
                if !待补检查标志.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
            }
            检查进行中标志.store(false, std::sync::atomic::Ordering::SeqCst);
        });
        Ok(())
    }

    /// 执行一次编译检查并发布诊断（供 `触发编译检查` 的线程循环调用）
    ///
    /// 不负责检查进行中/待补检查标记（由调用方线程循环统一管理）。
    fn 运行编译检查一次(
        缓存实例: &Arc<转译缓存>,
        映射器: &Arc<响应映射器>,
        发送方: &crossbeam_channel::Sender<Message>,
        内置诊断: &内置诊断表类型,
        项目目录: &Path,
    ) {
        // 超时控制：编译命令在锁竞争等场景可能长时间不退出，
        // 轮询等待最多 30 秒后强杀，避免每次保存都挂起一个进程
        let 子进程值 = match std::process::Command::new("cargo")
            // 离线：虚拟项目无第三方依赖，跳过索引网络访问
            //（无锁文件时包管理器默认联网解析依赖，网络不可达会卡死）
            .args(["check", "--offline", "--message-format=json"])
            .current_dir(项目目录)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(子进程项) => 子进程项,
            Err(错误值) => {
                log::warn!("编译检查启动失败：{错误值}");
                return;
            }
        };
        let mut 子进程可选项 = Some(子进程值);
        let mut 输出项 = None;
        let mut 已超时 = false;
        for 序号 in 0..300 {
            match 子进程可选项.as_mut().map(|子进程项| 子进程项.try_wait()) {
                Some(Ok(Some(_))) => {
                    输出项 = 子进程可选项
                        .take()
                        .and_then(|子进程项| 子进程项.wait_with_output().ok());
                    break;
                }
                Some(Ok(None)) => {
                    if 序号 == 299 {
                        已超时 = true;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                _ => break,
            }
        }
        if 已超时 {
            log::warn!("编译检查超时，已终止");
            if let Some(mut 子进程项) = 子进程可选项.take() {
                let _ = 子进程项.kill();
                let _ = 子进程项.wait();
            }
        }
        let Some(输出数据) = 输出项 else {
            return;
        };

        // 解析编译消息行 → 按虚拟资源标识聚合（值 = (方言资源标识, 诊断列表)）
        let 标准输出文本 = String::from_utf8_lossy(&输出数据.stdout);
        let mut 按资源标识: HashMap<String, (String, Vec<Value>)> = HashMap::new();
        for 行文本 in 标准输出文本.lines() {
            let Ok(值项) = serde_json::from_str::<Value>(行文本) else {
                continue;
            };
            if 值项["reason"].as_str() != Some("compiler-message") {
                continue;
            }
            let 消息项 = &值项["message"];
            let Some(跨度项) = 消息项["spans"].as_array().and_then(|数组值| 数组值.first())
            else {
                continue;
            };
            let Some(文件名值) = 跨度项["file_name"].as_str() else {
                continue;
            };
            // 编译器可能输出相对路径（工作目录为虚拟项目目录），拼上项目目录
            let 文件路径 = if Path::new(文件名值).is_absolute() {
                PathBuf::from(文件名值)
            } else {
                项目目录.join(文件名值)
            };
            // 仅处理虚拟方言文件（聚合主文件、标准库等跳过）
            let Some(条目值) = 缓存实例.按虚拟资源标识查询(&路径转统一资源标识(&文件路径))
            else {
                continue;
            };
            let 行号 = 跨度项["line_start"].as_u64().unwrap_or(1).saturating_sub(1);
            let 列号 = 跨度项["column_start"]
                .as_u64()
                .unwrap_or(1)
                .saturating_sub(1);
            let 末行号 = 跨度项["line_end"]
                .as_u64()
                .unwrap_or(行号 + 1)
                .saturating_sub(1);
            let 末列号 = 跨度项["column_end"]
                .as_u64()
                .unwrap_or(列号 + 1)
                .saturating_sub(1);
            let 错误码值 = 消息项["code"]["code"].as_str().unwrap_or("").to_string();
            let 严重级 = match 消息项["level"].as_str() {
                Some("error") => 1,
                Some("warning") => 2,
                _ => 3,
            };
            let 诊断项 = serde_json::json!({
                "range": {
                    "start": { "line": 行号, "character": 列号 },
                    "end": { "line": 末行号, "character": 末列号 }
                },
                "severity": 严重级,
                "code": 错误码值,
                "message": 消息项["message"].as_str().unwrap_or(""),
            });
            let 目标项 = 按资源标识
                .entry(条目值.虚拟资源标识.clone())
                .or_insert_with(|| (条目值.原始资源标识.clone(), Vec::new()));
            目标项.1.push(诊断项);
        }

        // 合并内置诊断（语法/类型）后发布：同诊断码且同起始行视为重复。
        // 注意：诊断映射期望输入虚拟资源标识（内部还原为方言资源标识），
        // 传方言资源标识会导致位置映射查不到条目而丢失全部诊断。
        for (虚拟资源标识, (原始资源标识, mut 诊断表)) in 按资源标识 {
            if let Ok(守卫) = 内置诊断.lock()
                && let Some(内置项) = 守卫.get(&原始资源标识)
            {
                for 内置诊断项 in 内置项.clone() {
                    let 是重复 = 诊断表.iter().any(|诊断项| {
                        诊断项["code"] == 内置诊断项["code"]
                            && 诊断项["range"]["start"]["line"]
                                == 内置诊断项["range"]["start"]["line"]
                    });
                    if !是重复 {
                        诊断表.push(内置诊断项);
                    }
                }
            }
            let 参数值 = serde_json::json!({ "uri": 虚拟资源标识, "diagnostics": 诊断表 });
            let mut 映射后 = 映射器.映射诊断(&参数值);
            // 教学诊断注入（与分析器链路一致）：由条目内容直接计算，
            // 不依赖内置缓存时序（虚拟检查可能先于分析器首批发布，
            // 缓存为空时教学提示不能丢失）
            if let Some(条目值) = 缓存实例.查询原始(&原始资源标识) {
                let mut 最终诊断 = 映射后["diagnostics"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                crate::响应映射::诊断文本::注入教学诊断(
                    &mut 最终诊断,
                    &条目值,
                    映射器.教学词分桶(),
                    映射器.歧义构造器词集(),
                    映射器.英文回落映射(),
                    映射器.关键词映射(),
                );
                映射后["diagnostics"] = Value::Array(最终诊断);
            }
            let 通知项 = Notification {
                method: "textDocument/publishDiagnostics".to_string(),
                params: 映射后,
            };
            let _ = 发送方.send(Message::Notification(通知项));
        }
    }

    /// 处理代码格式化请求（`textDocument/formatting`）
    ///
    /// 采用全文件替换策略，不转发 rust-analyzer：
    /// 取缓存中的英文译文 → rustfmt 格式化 → 反向翻译回母语 →
    /// 返回覆盖整个文档的文本编辑。
    /// 文档未打开或 rustfmt 失败时返回空数组，不崩溃。
    fn 处理格式化(&self, 请求值: Request) -> anyhow::Result<()> {
        let 资源标识 = 请求值.params["textDocument"]["uri"].as_str().unwrap_or("");

        let 编辑表 = match self.缓存.查询原始(资源标识) {
            Some(条目值) => {
                let 制表宽度 = 请求值.params["options"]["tabSize"].as_u64().unwrap_or(4);
                match 运行格式化(&条目值.英文内容, 制表宽度) {
                    Some(格式化英文) => {
                        // 将格式化后的英文代码反向翻译为母语代码
                        //（按文档编辑映射精确删除代理添加的包前缀）
                        let 格式化母语 = self.缓存.反向转译(Some(资源标识), &格式化英文);
                        vec![serde_json::json!({
                            "range": {
                                "start": { "line": 0, "character": 0 },
                                "end": 文末位置(&条目值.母语内容)
                            },
                            "newText": 格式化母语
                        })]
                    }
                    None => Vec::new(),
                }
            }
            None => Vec::new(),
        };

        let 响应值 = Response {
            id: 请求值.id,
            response_result: Ok(Value::Array(编辑表)),
        };
        self.客户端连接
            .sender
            .send(Message::Response(响应值))
            .map_err(|错误值| {
                anyhow::anyhow!(
                    "{}",
                    crate::界面消息::全局界面()
                        .取带参消息("lsp_err_send_format", &[&错误值.to_string()])
                )
            })?;
        Ok(())
    }

    /// 转发请求到 rust-analyzer
    ///
    /// 除了将资源标识替换为虚拟文件资源标识，还将请求中的位置
    ///（位置/范围）从母语坐标转换为英文坐标，
    /// 并将重命名的新名字段翻译为英文。
    fn 转发请求(&self, 请求值: Request) -> anyhow::Result<()> {
        let 分析器标识 = self
            .请求计数器
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let 原始资源标识 = 请求值.params["textDocument"]["uri"]
            .as_str()
            .unwrap_or("")
            .to_string();

        {
            let mut 待处理表 = self.待处理请求表.lock().map_err(|_| {
                anyhow::anyhow!("{}", crate::界面消息::全局界面().取消息("lsp_err_lock"))
            })?;
            待处理表.insert(
                分析器标识,
                待处理请求信息 {
                    原始标识: 请求值.id,
                    请求方法: 请求值.method.clone(),
                    原始资源标识: 原始资源标识.clone(),
                    创建时刻: std::time::Instant::now(),
                    未解析包名表: if 请求值.method == "textDocument/codeAction" {
                        let 诊断表 = 请求值.params["context"]["diagnostics"]
                            .as_array()
                            .map(|数组值| &数组值[..])
                            .unwrap_or(&[]);
                        从诊断提取未解析包名(诊断表)
                    } else {
                        Vec::new()
                    },
                    教学诊断表: if 请求值.method == "textDocument/codeAction" {
                        请求值.params["context"]["diagnostics"]
                            .as_array()
                            .map(|数组值| {
                                数组值
                                    .iter()
                                    .filter(|诊断项| {
                                        crate::响应映射::诊断文本::是教学诊断(诊断项)
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

        // 替换资源标识为虚拟资源标识，并转换请求中的位置
        let mut 参数值 = 请求值.params.clone();
        let 条目选项 = if self.是受支持文件(&原始资源标识) {
            self.缓存.查询原始(&原始资源标识)
        } else {
            None
        };

        if let Some(条目值) = &条目选项 {
            // 1. 替换文本文档的资源标识字段
            if let Some(标识字段) = 参数值
                .get_mut("textDocument")
                .and_then(|文档值| 文档值.get_mut("uri"))
            {
                *标识字段 = Value::String(条目值.虚拟资源标识.clone());
            }

            // 2. 按方法转换位置参数（母语坐标 → 英文坐标）
            match 请求值.method.as_str() {
                "textDocument/completion"
                | "textDocument/hover"
                | "textDocument/definition"
                | "textDocument/references"
                | "textDocument/rename"
                | "textDocument/documentHighlight"
                | "textDocument/signatureHelp" => {
                    if let Some(位置值) = 参数值.get_mut("position") {
                        *位置值 = 位置转英文(条目值, 位置值);
                    }
                }
                "textDocument/codeAction" => {
                    if let Some(范围值) = 参数值.get_mut("range") {
                        *范围值 = 范围转英文(条目值, 范围值);
                    }
                    // 上下文诊断来自我们发布的中文诊断，同样需要转换
                    if let Some(诊断列表) = 参数值
                        .get_mut("context")
                        .and_then(|上下文值| 上下文值.get_mut("diagnostics"))
                        .and_then(|诊断值| 诊断值.as_array_mut())
                    {
                        for 诊断项 in 诊断列表.iter_mut() {
                            if let Some(范围值) = 诊断项.get_mut("range") {
                                *范围值 = 范围转英文(条目值, 范围值);
                            }
                        }
                    }
                }
                _ => {}
            }

            // 3. 重命名的新名字段：中文 → 英文（不在关键字映射中则保持原样）
            if 请求值.method == "textDocument/rename"
                && let Some(母语名) = 请求值.params.get("newName").and_then(|值| 值.as_str())
            {
                let 英文名 = self
                    .缓存
                    .关键词映射()
                    .get(母语名)
                    .or_else(|| self.缓存.标识符别名映射().get(母语名))
                    .cloned()
                    .unwrap_or_else(|| 母语名.to_string());
                参数值["newName"] = Value::String(英文名);
            }
        }

        let 分析器消息 = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 分析器标识,
            "method": 请求值.method,
            "params": 参数值
        });
        self.分析器.发送消息(&分析器消息)
    }
}

/// 判断资源标识是否以任一受支持的方言扩展名结尾
fn 是受支持文件(资源标识: &str, 扩展名表: &[String]) -> bool {
    扩展名表.iter().any(|扩展项| 资源标识.ends_with(扩展项))
}

/// 从代码操作请求上下文诊断中提取未声明的包名
///
/// 消息可能是英文原文（rust-analyzer 直发）或翻译后的母语文本
///（经我方发布后由客户端回传）：两者都保留反引号包裹的路径
/// 内容，提取逻辑统一；母语消息按本地化短语表判定未解析导入。
fn 从诊断提取未解析包名(诊断表: &[Value]) -> Vec<String> {
    let 翻译短语 = crate::界面消息::全局界面().取消息("lsp_phrase_unresolved_import");
    let mut 未解析表 = Vec::new();
    for 诊断项 in 诊断表 {
        let Some(消息串) = 诊断项.get("message").and_then(|值| 值.as_str()) else {
            continue;
        };
        let 是未解析 = i18n_rust_engine::诊断::是否未解析导入消息(消息串)
            || 消息串.contains(翻译短语.as_str());
        if !是未解析 {
            continue;
        }
        for 段 in i18n_rust_engine::诊断::取反引号首段(消息串) {
            if [
                "std",
                "core",
                "alloc",
                "self",
                "super",
                "crate",
                "proc_macro",
            ]
            .contains(&段.as_str())
                || 段
                    .chars()
                    .next()
                    .is_some_and(|字符值| 字符值.is_ascii_digit())
            {
                continue;
            }
            if !未解析表.contains(&段) {
                未解析表.push(段);
            }
        }
    }
    未解析表
}

/// 计算虚拟项目临时目录（按用户 + 进程实例隔离）并拒绝符号链接
///
/// 固定共享的临时路径在多用户机器上可被预创建为符号链接，
/// 后续写文件会跟随链接覆写任意位置；按用户名隔离并校验规避此风险。
/// 同一用户的多个编辑器实例各起一个语言服务代理进程，再叠加进程号后缀
/// 避免互相同目录覆写清单文件与源码内容；启动时清理已死进程
/// 的残留目录（见 `清理残留虚拟目录`）防止无限累积。
fn 虚拟临时目录() -> anyhow::Result<PathBuf> {
    let 用户名 = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "default".to_string());
    let 安全用户名: String = 用户名
        .chars()
        .map(|字符值| {
            if 字符值.is_alphanumeric() || 字符值 == '_' {
                字符值
            } else {
                '_'
            }
        })
        .collect();
    清理残留虚拟目录(&安全用户名);
    let 目录值 = std::env::temp_dir().join(format!(
        "i18n_lsp_virtual_{}_{}",
        安全用户名,
        std::process::id()
    ));
    // 两道校验，缺一不可：
    // 1. 创建前——可预测名（用户 + 进程号）可被本地攻击者预创建为符号链接；
    // 2. 创建后、任何写文件之前——校验与创建之间存在竞态窗口，
    //    攻击者可在此期间把路径换成符号链接，而创建目录会跟随它。
    //    此时符号链接元数据仍报告链接本身，立即拒绝可保证
    //    尚无任何文件写入发生在链接目标处。
    if 是符号链接(&目录值) {
        anyhow::bail!(
            "{}",
            crate::界面消息::全局界面()
                .取带参消息("lsp_err_temp_symlink", &[&目录值.display().to_string()])
        );
    }
    std::fs::create_dir_all(&目录值).map_err(|错误值| {
        anyhow::anyhow!(
            "创建 LSP 虚拟项目目录失败：{}（{}）",
            目录值.display(),
            错误值
        )
    })?;
    if 是符号链接(&目录值) {
        anyhow::bail!(
            "{}",
            crate::界面消息::全局界面()
                .取带参消息("lsp_err_temp_symlink", &[&目录值.display().to_string()])
        );
    }
    Ok(目录值)
}

/// 路径自身是否为符号链接（不跟随末段链接）
fn 是符号链接(路径值: &Path) -> bool {
    路径值
        .symlink_metadata()
        .map(|元数据值| 元数据值.file_type().is_symlink())
        .unwrap_or(false)
}

/// 清理同用户的残留虚拟/镜像目录：仅删除名称中带进程号后缀且进程已死的目录
///
/// 虚拟目录名 `i18n_lsp_virtual_<用户>_<进程号>`；镜像目录名
/// `i18n_lsp_mirror_<用户>_<进程号>_<项目哈希>`（进程号段后还有哈希段）。
/// 尽力而为：任何失败（目录列举失败、无法解析进程号、删除失败）都静默跳过，
/// 不影响本实例启动。存活检查对 Unix（零信号探活）与其余平台分别处理。
fn 清理残留虚拟目录(安全用户名: &str) {
    let 前缀表 = [
        format!("i18n_lsp_virtual_{}_", 安全用户名),
        format!("i18n_lsp_mirror_{}_", 安全用户名),
    ];
    let Ok(目录读取器) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for 目录项 in 目录读取器.flatten() {
        let 名称值 = 目录项.file_name();
        let Some(名称串) = 名称值.to_str() else {
            continue;
        };
        // 仅处理本用户且带进程号后缀的目录；无后缀的旧版目录不动（避免误删）
        let Some(剩余串) = 前缀表
            .iter()
            .find_map(|前缀项| 名称串.strip_prefix(前缀项.as_str()))
        else {
            continue;
        };
        // 兼容上轮改名后未删除干净的残留后缀；镜像名进程号后还有
        // 哈希段，取首个下划线之前的部分为进程号
        let 剩余串 = 剩余串.strip_suffix(".stale").unwrap_or(剩余串);
        let 进程号串 = 剩余串.split('_').next().unwrap_or(剩余串);
        if 进程号串 == std::process::id().to_string() {
            continue; // 当前进程自己的目录
        }
        let Ok(进程号值) = 进程号串.parse::<u32>() else {
            continue;
        };
        if 进程存活(进程号值) {
            continue;
        }
        // 进程已死：目录是残留，先原子改名再删除（竞态防护）：
        // 检查与删除之间进程号可能被系统回收并由新进程重建同名目录，
        // 改名把旧目录移走后再删，新进程创建的是新路径，不受影响
        let 路径值 = 目录项.path();
        if 路径值
            .symlink_metadata()
            .map(|元数据值| 元数据值.file_type().is_symlink())
            .unwrap_or(true)
        {
            continue; // 符号链接不跟随删除
        }
        let 残留路径 = 目录项.path().with_file_name(format!("{名称串}.stale"));
        if std::fs::rename(&路径值, &残留路径).is_ok() {
            let _ = std::fs::remove_dir_all(&残留路径);
        }
    }
}

/// 响应 rust-analyzer 的配置请求
///
/// - 补全片段设为自定义：方法补全附带括号；
/// - 保存即检查与检查开关设为假：禁用 rust-analyzer 自跑的
///   编译检查。该检查针对无第三方依赖、无过程宏的虚拟项目，
///   结果按工作区全部文件成批发布：依赖缺失、派生不展开的级联错误
///   在该环境下恒为假红。编译级诊断由代理自跑虚拟/镜像检查提供
///   （打开/保存时触发）。
fn 分析器配置答复(条目数: usize) -> Value {
    Value::Array(
        (0..条目数)
            .map(|_| {
                serde_json::json!({
                    "completion": { "snippets": "custom" },
                    "checkOnSave": { "enable": false },
                    "check": { "enable": false }
                })
            })
            .collect(),
    )
}

/// 判断进程号是否存活（仅用于残留目录清理，误判代价低）
///
/// Unix：零信号探活，命令退出成功即视为存活；探测本身失败时保守视为存活，
/// 宁可残留目录下轮再清，也不误删活进程的文件。
fn 进程存活(进程号值: u32) -> bool {
    #[cfg(unix)]
    {
        // 零信号探活：`kill -0 <进程号>` 成功即进程存在且有权限。
        // 外部 C 函数块不允许非 ASCII 名字，故改走系统必装的命令解释器
        // 内建命令；只在启动清理时调用，额外开销可忽略。
        std::process::Command::new("sh")
            .args(["-c", &format!("kill -0 {}", 进程号值)])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|状态值| 状态值.success())
            .unwrap_or(true)
    }
    #[cfg(not(unix))]
    {
        // 非 Unix 平台无廉价探活手段：保守视为存活，不删除
        let _ = 进程号值;
        true
    }
}

/// 清理超时的待映射请求：向客户端应答错误，避免永久等待与条目泄漏
fn 清理超时请求(
    待处理表: &待处理请求表类型, 发送方: &crossbeam_channel::Sender<Message>
) {
    let 当前时刻 = std::time::Instant::now();
    let mut 超时表: Vec<lsp_server::RequestId> = Vec::new();
    {
        let mut 请求表 = match 待处理表.lock() {
            Ok(守卫) => 守卫,
            Err(_) => return,
        };
        请求表.retain(|_标识, 信息项| {
            if 当前时刻.duration_since(信息项.创建时刻) > 请求超时 {
                超时表.push(信息项.原始标识.clone());
                false
            } else {
                true
            }
        });
    }
    for 标识 in 超时表 {
        log::warn!(
            "{}",
            crate::界面消息::全局界面().取消息("lsp_warn_request_timeout")
        );
        let 响应值 = Response {
            id: 标识,
            response_result: Err(lsp_server::ResponseError {
                code: -32603,
                message: "rust-analyzer 未在超时前响应".to_string(),
                data: None,
            }),
        };
        let _ = 发送方.send(Message::Response(响应值));
    }
}

/// 清空全部待映射请求：向客户端应答错误（rust-analyzer 崩溃重启后调用，
/// 旧进程的请求永远不会有响应，必须主动应答避免客户端永久等待）
fn 全部待处理失败应答(
    待处理表: &待处理请求表类型,
    发送方: &crossbeam_channel::Sender<Message>,
) {
    let 标识表: Vec<lsp_server::RequestId> = {
        let mut 请求表 = match 待处理表.lock() {
            Ok(守卫) => 守卫,
            Err(_) => return,
        };
        请求表
            .drain()
            .map(|(_标识, 信息项)| 信息项.原始标识)
            .collect()
    };
    for 标识 in 标识表 {
        let 响应值 = Response {
            id: 标识,
            response_result: Err(lsp_server::ResponseError {
                code: -32603,
                message: "rust-analyzer 已重启".to_string(),
                data: None,
            }),
        };
        let _ = 发送方.send(Message::Response(响应值));
    }
}

/// 调用系统 rustfmt 格式化英文代码
///
/// 通过标准输入传入源码、标准输出取回格式化结果。
/// 新版 rustfmt（1.9 以上）不传文件参数时从标准输入读取，需配合 `--emit stdout`；
/// rustfmt 不存在、源码含语法错误或输出非 UTF-8 时返回无。
fn 运行格式化(源文本: &str, 制表宽度: u64) -> Option<String> {
    let mut 子进程值 = std::process::Command::new("rustfmt")
        .arg("--emit")
        .arg("stdout")
        .arg("--edition")
        .arg("2021")
        .arg("--config")
        .arg(format!("tab_spaces={}", 制表宽度.max(1)))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    if let Some(mut 标准输入管道) = 子进程值.stdin.take() {
        标准输入管道.write_all(源文本.as_bytes()).ok()?;
        // 标准输入管道在此处丢弃，关闭管道使 rustfmt 结束输出
    }
    let 输出项 = 子进程值.wait_with_output().ok()?;
    if !输出项.status.success() {
        return None;
    }
    String::from_utf8(输出项.stdout).ok()
}

/// 计算母语文本末尾的语言服务位置（最后一行、最后一列的 UTF-16 长度）
///
/// 用于生成覆盖整个文档的格式化文本编辑的范围终点。
fn 文末位置(内容值: &str) -> Value {
    let 行数 = 内容值.matches('\n').count() as u64 + 1;
    let 末行 = 内容值.rsplit('\n').next().unwrap_or("");
    let 列数: u64 = 末行.chars().map(|字符值| 字符值.len_utf16() as u64).sum();
    serde_json::json!({ "line": 行数 - 1, "character": 列数 })
}

/// 将增量变更（范围 + 文本）应用到母语文本
///
/// 语言服务位置按 UTF-16 编码单元计数；越界位置钳制到行尾/文末，
/// 防御客户端发来异常范围时崩溃。
fn 应用增量变更(内容值: &mut String, 范围值: &Value, 文本串: &str) {
    let 起始偏移 = 位置转偏移(内容值, &范围值["start"]);
    let 结束偏移 = 位置转偏移(内容值, &范围值["end"]).max(起始偏移);
    内容值.replace_range(起始偏移..结束偏移, 文本串);
}

/// 语言服务位置（行/字符，UTF-16）→ 文本字节偏移
///
/// 行号越界钳制到最后一行；列号越界钳制到行尾（含换行符前）。
fn 位置转偏移(内容值: &str, 位置值: &Value) -> usize {
    let 行号 = 位置值["line"].as_u64().unwrap_or(0) as u32;
    let 字符号 = 位置值["character"].as_u64().unwrap_or(0) as u32;
    let 换行位置表: Vec<usize> = 内容值
        .match_indices('\n')
        .map(|(偏移值, _)| 偏移值)
        .collect();
    let 目标行 = 行号 as usize;
    let 行首偏移 = if 目标行 == 0 {
        0
    } else {
        match 换行位置表.get(目标行 - 1) {
            Some(偏移值) => 偏移值 + 1,
            None => 换行位置表.last().map(|偏移值| 偏移值 + 1).unwrap_or(0),
        }
    };
    // 行内按 UTF-16 单元前进，列号用尽或到达行尾（不含换行符）即停
    let 行内文本 = &内容值[行首偏移..];
    let mut 编码单元数 = 0u32;
    for (字节位置, 字符值) in 行内文本.char_indices() {
        if 字符值 == '\n' || 编码单元数 >= 字符号 {
            return 行首偏移 + 字节位置;
        }
        编码单元数 += 字符值.len_utf16() as u32;
    }
    行首偏移 + 行内文本.len()
}

/// 将语言服务位置从母语坐标转换为英文坐标
///
/// 当前翻译逐行替换关键字、行数保持不变（行映射为一一对应），
/// 因此仅列号需要按列偏移映射转换。
fn 位置转英文(条目值: &翻译条目, 位置值: &Value) -> Value {
    let 行号 = 位置值["line"].as_u64().unwrap_or(0) as u32;
    let 列号 = 位置值["character"].as_u64().unwrap_or(0) as u32;
    // 调用方已持有条目：直接走无锁单函数，避免再按资源标识查缓存（多次加锁）
    let 英文列 = crate::翻译缓存::母语列转英文列(条目值, 行号, 列号);
    serde_json::json!({ "line": 行号, "character": 英文列 })
}

/// 将语言服务范围从母语坐标转换为英文坐标
fn 范围转英文(条目值: &翻译条目, 范围值: &Value) -> Value {
    let 起点 = 位置转英文(条目值, &范围值["start"]);
    let 终点 = 位置转英文(条目值, &范围值["end"]);
    serde_json::json!({ "start": 起点, "end": 终点 })
}

/// 处理来自 rust-analyzer 的消息并转发给客户端
fn 处理分析器消息(
    消息值: &Value,
    映射器: &Arc<响应映射器>,
    发送方: &crossbeam_channel::Sender<Message>,
    待处理表: &待处理请求表类型,
    回发发送端: &crate::分析器::发送端,
    内置诊断: &内置诊断表类型,
) {
    if let Some(标识) = 消息值.get("id").and_then(|值| 值.as_i64()) {
        // 是响应
        let 原始信息 = {
            let mut 请求表 = match 待处理表.lock() {
                Ok(守卫) => 守卫,
                Err(_) => return,
            };
            请求表.remove(&标识)
        };

        if let Some(信息项) = 原始信息 {
            // rust-analyzer 返回错误时（如文件不存在），原样透传给客户端
            if 消息值.get("error").is_some() {
                let 错误值 = 消息值["error"].clone();
                let 响应值 = Response {
                    id: 信息项.原始标识,
                    response_result: Err(lsp_server::ResponseError {
                        code: 错误值["code"].as_i64().unwrap_or(-32603) as i32,
                        message: 错误值["message"].as_str().map(String::from).unwrap_or_else(
                            || crate::界面消息::全局界面().取消息("lsp_internal_error"),
                        ),
                        data: 错误值.get("data").cloned(),
                    }),
                };
                let _ = 发送方.send(Message::Response(响应值));
                return;
            }

            let 结果值 = 消息值.get("result").cloned().unwrap_or(Value::Null);
            let 映射后数据 = match 信息项.请求方法.as_str() {
                "textDocument/completion" => {
                    映射器.映射补全响应(&结果值, &信息项.原始资源标识)
                }
                "textDocument/hover" => 映射器.映射悬停响应(&结果值, &信息项.原始资源标识),
                "textDocument/signatureHelp" => 映射器.映射签名帮助响应(&结果值),
                "textDocument/definition" => 映射器.映射定义响应(&结果值),
                "textDocument/references" => 映射器.映射引用响应(&结果值),
                "textDocument/documentSymbol" => {
                    映射器.映射文档符号响应(&结果值, &信息项.原始资源标识)
                }
                "textDocument/codeAction" => {
                    let 映射后 = 映射器.映射代码操作响应(&结果值, &信息项.原始资源标识);
                    // 未解析导入错误时注入“添加依赖”快捷修复（包管理器安装）
                    let 映射后 = 映射器.注入添加依赖动作(&映射后, &信息项.未解析包名表);
                    // 教学诊断（全角标点/教学检查）注入一键修复动作
                    映射器.注入教学动作(&映射后, &信息项.教学诊断表, &信息项.原始资源标识)
                }
                "codeAction/resolve" => 映射器.映射代码操作解析响应(&结果值),
                "textDocument/rename" => 映射器.映射重命名响应(&结果值),
                "textDocument/documentHighlight" => {
                    映射器.映射文档高亮响应(&结果值, &信息项.原始资源标识)
                }
                "textDocument/semanticTokens/full" | "textDocument/semanticTokens/range" => {
                    映射器.映射语义记号响应(&结果值, &信息项.原始资源标识)
                }
                _ => 结果值,
            };

            let 响应值 = Response {
                id: 信息项.原始标识,
                response_result: Ok(映射后数据),
            };
            let _ = 发送方.send(Message::Response(响应值));
        } else if 消息值.get("method").and_then(|值| 值.as_str()).is_some() {
            // 待映射表中没有对应记录：这是 rust-analyzer 主动发来的请求
            //（如配置、诊断刷新）。必须把响应回发给 rust-analyzer 本身，
            // 否则它会一直等待。
            let 方法名 = 消息值["method"].as_str().unwrap_or("");
            let 结果值 = if 方法名 == "workspace/configuration" {
                let 条目数 = 消息值["params"]["items"]
                    .as_array()
                    .map(|数组值| 数组值.len())
                    .unwrap_or(0);
                分析器配置答复(条目数)
            } else {
                Value::Null
            };
            let 响应值 = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 标识,
                "result": 结果值
            });
            let _ = 回发发送端.发送消息(&响应值);
        }
    } else if let Some(方法名) = 消息值.get("method").and_then(|值| 值.as_str()) {
        // 是通知
        match 方法名 {
            "textDocument/publishDiagnostics" => {
                if let Some(参数值) = 消息值.get("params") {
                    // 只转发虚拟母语文件的诊断：
                    // rust-analyzer 可能对其他项目文件发布诊断，
                    // 这些与母语代码无关，不应发给客户端。
                    let 诊断资源标识 = 参数值["uri"].as_str().unwrap_or("");
                    if !映射器.是虚拟资源标识(诊断资源标识) {
                        return;
                    }
                    let mut 映射后 = 映射器.映射诊断(参数值);
                    // 合并全角标点教学诊断（方言坐标，与内置诊断同格式）：
                    // 全角标点在转译后的虚拟文本中保留，扫描结果经列映射
                    // 还原为方言坐标，随分析器发布链路同步下发——教学提示
                    // 与语法诊断共存，不因诊断全量替换而闪烁
                    let mut 合并诊断: Vec<Value> = 映射后["diagnostics"]
                        .as_array()
                        .map(|数组值| 数组值.to_vec())
                        .unwrap_or_default();
                    let 合并资源标识 = 映射后["uri"].as_str().unwrap_or("").to_string();
                    if let Some(条目值) = 映射器.原始条目(&合并资源标识) {
                        // 分析器的编译错误在编译项目中抑制：编译级诊断
                        // 以代理自跑的镜像/虚拟项目检查（编译器口径）为准，
                        // 分析器在虚拟项目上对第三方依赖与过程宏的类型推断恒为
                        // 假红（详见 `抑制分析器编译错误` 的说明）
                        合并诊断.retain(|诊断项| {
                            !crate::响应映射::诊断文本::抑制分析器编译错误(
                                诊断项,
                                &条目值.原始路径,
                            )
                        });
                        // 教学诊断注入（全角标点 + 教学检查；移除旧注入保证最新）
                        crate::响应映射::诊断文本::注入教学诊断(
                            &mut 合并诊断,
                            &条目值,
                            映射器.教学词分桶(),
                            映射器.歧义构造器词集(),
                            映射器.英文回落映射(),
                            映射器.关键词映射(),
                        );
                        映射后["diagnostics"] = Value::Array(合并诊断.clone());
                    }
                    // 缓存映射后的内置诊断（方言坐标），供编译检查结果合并发布
                    if let Ok(mut 守卫) = 内置诊断.lock() {
                        守卫.insert(合并资源标识, 合并诊断);
                    }
                    let 通知项 = Notification {
                        method: 方法名.to_string(),
                        params: 映射后,
                    };
                    let _ = 发送方.send(Message::Notification(通知项));
                }
            }
            _ => {
                let 通知项 = Notification {
                    method: 方法名.to_string(),
                    params: 消息值.get("params").cloned().unwrap_or(Value::Null),
                };
                let _ = 发送方.send(Message::Notification(通知项));
            }
        }
    }
}

/// 加载语言包：返回统一映射管理器（关键字/宏/派生/模块路径/别名全量）
///
/// 关键字与别名分离（与命令行统一管线对齐）：关键字在词法阶段无条件替换，
/// 标准库/第三方库标识符（别名）在词法转译后经声明位保护替换，
/// 避免用户声明与库别名撞名时被误替换（如 `让 新 = 5`）。
/// 模块路径映射随统一管线一并启用（使用语句路径段中文化）。
fn 载入语言包(
    语言包路径: &Path,
) -> anyhow::Result<i18n_rust_engine::映射管理器::映射管理器> {
    let 映射表路径 = 语言包路径.join("映射表");
    if 映射表路径.exists() {
        match 映射源::载入关键字映射(语言包路径) {
            Ok(映射表) => {
                // 旧“映射表”目录格式：仅有扁平关键字表，宏/派生/模块路径/别名表为空
                return Ok(
                    i18n_rust_engine::映射管理器::映射管理器::从扁表构造(
                        映射表,
                        HashMap::new(),
                        HashMap::new(),
                    ),
                );
            }
            Err(错误值) => log::warn!(
                "{}",
                crate::界面消息::全局界面()
                    .取带参消息("lsp_log_mappings_fallback", &[&错误值.to_string()])
            ),
        }
    }

    let 关键词路径 = 语言包路径.join("keywords.toml");
    if 关键词路径.exists() {
        // 复用引擎统一加载器（与命令行完全同源）：关键字/别名分离、
        // 标准库优先于第三方库、各包词表按文件名排序合并
        return i18n_rust_engine::映射管理器::映射管理器::从目录载入(语言包路径).map_err(
            |错误值| {
                anyhow::anyhow!(
                    "{}",
                    crate::界面消息::全局界面()
                        .取带参消息("lsp_err_load_keywords", &[&错误值.to_string()])
                )
            },
        );
    }

    log::warn!(
        "{}",
        crate::界面消息::全局界面().取消息("lsp_warn_builtin_fallback")
    );
    // 语言包目录缺失时的回退：物化引擎编译期内嵌的完整中文语言包后
    // 走统一加载器（与命令行完全同源，含关键字/宏/别名全量）。
    // 不可再退回创建内置关键字映射：那是早期硬编码旧表
    //（54 个旧词、无宏表、无 `让` 等新关键字），会导致转译残缺——
    // 如 `打印行!` 不翻译报找不到宏、`让` 不翻译报语法错误。
    if let Some(管理器) = 载入内置中文回退() {
        return Ok(管理器);
    }
    // 极端兜底：物化失败时退回硬编码旧表（可能残缺，但保证可启动）
    Ok(
        i18n_rust_engine::映射管理器::映射管理器::从扁表构造(
            映射源::创建内置关键字映射(),
            HashMap::new(),
            HashMap::new(),
        ),
    )
}

/// 从引擎编译期内嵌的中文语言包物化出完整映射管理器
///
/// 将内嵌文件（关键字/标准库/模块路径/各包词表）写入临时目录，
/// 复用统一加载器，保证与磁盘语言包完全同源。
fn 载入内置中文回退() -> Option<i18n_rust_engine::映射管理器::映射管理器> {
    let 临时目录值 = tempfile::tempdir().ok()?;
    let 中文目录 = 临时目录值.path().join("zh");
    std::fs::create_dir_all(&中文目录).ok()?;
    for (文件名值, 内容值) in i18n_rust_engine::语言::内置语言包文件("zh") {
        // 含子目录的文件（如各包词表）：逐级创建父目录
        if let Some(父级) = Path::new(文件名值).parent()
            && !父级.as_os_str().is_empty()
        {
            std::fs::create_dir_all(中文目录.join(父级)).ok()?;
        }
        std::fs::write(中文目录.join(文件名值), 内容值).ok()?;
    }
    i18n_rust_engine::映射管理器::映射管理器::从目录载入(&中文目录).ok()
}

#[cfg(test)]
mod 母语测试 {
    use super::*;
    use std::collections::HashMap;

    /// 语言包目录缺失时的回退必须加载完整中文表（含宏与 `让` 等新关键字），
    /// 而非硬编码旧表——否则 `打印行!` 不翻译报找不到宏、
    /// `让` 不翻译报语法错误（真实事故：扩展未找到语言包目录时触发）
    #[test]
    fn 语言包回退完整() {
        let 管理器 = 载入语言包(Path::new("/不存在的目录")).expect("回退应成功");
        assert_eq!(管理器.关键词表.get("让").map(String::as_str), Some("let"));
        assert_eq!(
            管理器.取派生特征表().get("克隆").map(String::as_str),
            Some("Clone")
        );
        assert_eq!(
            管理器.取宏表().get("打印行").map(String::as_str),
            Some("println")
        );
        assert!(
            管理器.关键词表.len() >= 100,
            "完整关键字表应 ≥100，实际 {}",
            管理器.关键词表.len()
        );
        assert!(
            管理器.取宏表().len() >= 30,
            "宏表应 ≥30，实际 {}",
            管理器.取宏表().len()
        );
        assert!(!管理器.标识符别名.is_empty(), "别名表不应为空");
        assert!(!管理器.模块路径表.is_empty(), "模块路径表不应为空");
    }

    /// 默认扩展名列表覆盖全部内置语言包（引擎语言包目录单一来源），未知扩展名不匹配
    #[test]
    fn 受支持文件默认扩展名() {
        let 扩展名表 = 默认扩展名();
        assert!(扩展名表.len() >= 10, "内置语言包数量异常");
        assert!(是受支持文件("file:///project/src/main.zh", &扩展名表));
        assert!(是受支持文件("file:///project/src/main.de", &扩展名表));
        assert!(是受支持文件("file:///project/src/main.ru", &扩展名表));
        assert!(是受支持文件("file:///project/src/main.ja", &扩展名表));
        assert!(!是受支持文件(
            "file:///project/src/main.rs",
            &扩展名表
        ));
        assert!(!是受支持文件(
            "file:///project/src/main.xyz",
            &扩展名表
        ));
    }

    /// 自定义扩展名列表生效
    #[test]
    fn 受支持文件自定义扩展名() {
        let 扩展名表 = vec![".fr".to_string()];
        assert!(是受支持文件("file:///project/src/main.fr", &扩展名表));
        assert!(!是受支持文件(
            "file:///project/src/main.zh",
            &扩展名表
        ));
    }

    fn 新建测试缓存() -> (Arc<转译缓存>, tempfile::TempDir) {
        let 管理器 = i18n_rust_engine::映射管理器::映射管理器::从扁表构造(
            HashMap::from([
                ("函数".into(), "fn".into()),
                ("让".into(), "let".into()),
                ("可变".into(), "mut".into()),
            ]),
            HashMap::new(),
            HashMap::new(),
        );
        let 临时目录值 = tempfile::tempdir().unwrap();
        let 缓存实例 = 转译缓存::新建(管理器, 临时目录值.path().to_path_buf());
        (缓存实例, 临时目录值)
    }

    #[test]
    fn 位置转英文测试() {
        let (缓存实例, _临时) = 新建测试缓存();
        let (条目值, _) = 缓存实例
            .更新文档("file:///test/main.zh", "让 x = 1;", 1)
            .unwrap();
        assert_eq!(条目值.英文内容, "let x = 1;");

        // 中文列 0（“让”起点）→ 英文列 0
        let 位置值 = serde_json::json!({ "line": 0, "character": 0 });
        let 英文位置 = 位置转英文(&条目值, &位置值);
        assert_eq!(英文位置["line"], 0);
        assert_eq!(英文位置["character"], 0);

        // 中文列 3（“x”末尾）→ 英文列 5（“让”为 1 个 UTF-16 单元，英文“让”占 3 列）
        let 位置值 = serde_json::json!({ "line": 0, "character": 3 });
        let 英文位置 = 位置转英文(&条目值, &位置值);
        assert_eq!(英文位置["character"], 5);

        // 中文列 2（“x”起点）→ 英文列 4
        let 位置值 = serde_json::json!({ "line": 0, "character": 2 });
        let 英文位置 = 位置转英文(&条目值, &位置值);
        assert_eq!(英文位置["character"], 4);
    }

    #[test]
    fn 范围转英文测试() {
        let (缓存实例, _临时) = 新建测试缓存();
        let (条目值, _) = 缓存实例
            .更新文档("file:///test/main.zh", "让 x = 1;", 1)
            .unwrap();

        let 范围值 = serde_json::json!({
            "start": { "line": 0, "character": 2 },
            "end": { "line": 0, "character": 3 }
        });
        let 英文范围 = 范围转英文(&条目值, &范围值);
        assert_eq!(英文范围["start"]["character"], 4);
        assert_eq!(英文范围["end"]["character"], 5);
    }

    #[test]
    fn 重命名全链路() {
        // 模拟完整闭环：中文请求 → 转换转发 → rust-analyzer 响应 → 映射回母语
        let (缓存实例, _临时) = 新建测试缓存();
        let 映射器 = 响应映射器::新建(缓存实例.clone());
        let (条目值, _) = 缓存实例
            .更新文档("file:///test/main.zh", "函数 主() {}", 1)
            .unwrap();
        assert_eq!(条目值.英文内容, "fn 主() {}");

        // —— 请求方向（与转发请求相同的转换逻辑）——
        let 请求载荷 = serde_json::json!({
            "textDocument": { "uri": "file:///test/main.zh" },
            "position": { "line": 0, "character": 0 },
            "newName": "函数"
        });

        let mut 参数值 = 请求载荷.clone();
        // 1. 资源标识替换为虚拟资源标识
        参数值["textDocument"]["uri"] = Value::String(条目值.虚拟资源标识.clone());
        // 2. 位置转换为英文坐标
        参数值["position"] = 位置转英文(&条目值, &参数值["position"]);
        // 3. 新名中文 → 英文
        let 英文名 = 缓存实例.关键词映射().get("函数").cloned().unwrap();
        参数值["newName"] = Value::String(英文名);

        assert_eq!(
            参数值["textDocument"]["uri"].as_str().unwrap(),
            条目值.虚拟资源标识
        );
        assert_eq!(参数值["position"]["character"], 0);
        assert_eq!(参数值["newName"].as_str().unwrap(), "fn");

        // —— 响应方向（rust-analyzer 返回虚拟文件编辑）——
        let 响应值 = serde_json::json!({
            "changes": {
                条目值.虚拟资源标识.clone(): [{
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 2 }
                    },
                    "newText": "fn"
                }]
            }
        });
        let 映射后 = 映射器.映射重命名响应(&响应值);
        let 编辑项 = &映射后["changes"]["file:///test/main.zh"][0];

        // 客户端收到的编辑：资源标识还原、位置为母语坐标、新文本恢复中文
        assert_eq!(编辑项["range"]["start"]["character"], 0);
        assert_eq!(编辑项["range"]["end"]["character"], 2);
        assert_eq!(编辑项["newText"].as_str().unwrap(), "函数");
    }

    #[test]
    fn 文末位置测试() {
        // 空文档
        assert_eq!(
            文末位置(""),
            serde_json::json!({ "line": 0, "character": 0 })
        );
        // 单行 ASCII
        assert_eq!(
            文末位置("ab"),
            serde_json::json!({ "line": 0, "character": 2 })
        );
        // 中文按 UTF-16 计数（4 个汉字 = 4 个单元）
        assert_eq!(
            文末位置("行\n中文测试"),
            serde_json::json!({ "line": 1, "character": 4 })
        );
        // 末尾换行：最后一行是空行
        assert_eq!(
            文末位置("行\n"),
            serde_json::json!({ "line": 1, "character": 0 })
        );
    }

    #[test]
    fn 位置转偏移测试() {
        let 文本串 = "让 可变 x = 5;\n让 y = 10;";
        // 首行中文列：「可变」起点（列 2）
        let 位置值 = serde_json::json!({ "line": 0, "character": 2 });
        assert_eq!(位置转偏移(文本串, &位置值), "让 ".len());
        // 第二行起点
        let 位置值 = serde_json::json!({ "line": 1, "character": 0 });
        assert_eq!(位置转偏移(文本串, &位置值), "让 可变 x = 5;\n".len());
        // 列号越界钳制到行尾（不含换行符）
        let 位置值 = serde_json::json!({ "line": 0, "character": 999 });
        assert_eq!(位置转偏移(文本串, &位置值), "让 可变 x = 5;".len());
        // 行号越界钳制到最后一行
        let 位置值 = serde_json::json!({ "line": 99, "character": 2 });
        assert_eq!(位置转偏移(文本串, &位置值), "让 可变 x = 5;\n让 ".len());
    }

    #[test]
    fn 应用增量变更测试() {
        // 中文行内插入
        let mut 文本串 = "让 x = 5;".to_string();
        let 范围值 = serde_json::json!({
            "start": { "line": 0, "character": 2 },
            "end": { "line": 0, "character": 2 }
        });
        应用增量变更(&mut 文本串, &范围值, "可变 ");
        assert_eq!(文本串, "让 可变 x = 5;");

        // 跨行删除替换
        let mut 文本串 = "行一\n行二\n行三".to_string();
        let 范围值 = serde_json::json!({
            "start": { "line": 0, "character": 1 },
            "end": { "line": 1, "character": 1 }
        });
        应用增量变更(&mut 文本串, &范围值, "新");
        // 删除「一\n行」并插入「新」
        assert_eq!(文本串, "行新二\n行三");

        // 异常范围（终点早于起点）不崩溃，退化为插入
        let mut 文本串 = "abc".to_string();
        let 范围值 = serde_json::json!({
            "start": { "line": 0, "character": 3 },
            "end": { "line": 0, "character": 1 }
        });
        应用增量变更(&mut 文本串, &范围值, "X");
        assert_eq!(文本串, "abcX");
    }

    /// 从诊断列表提取未声明包候选：英文原文命中、保留名单过滤、
    /// 无关消息不命中、跨诊断去重
    #[test]
    fn 从诊断提取未解析包名测试() {
        let 诊断表 = vec![
            serde_json::json!({"message": "unresolved import `serde_json`"}),
            serde_json::json!({"message": "use of undeclared crate or module `reqwest`"}),
            serde_json::json!({"message": "unresolved import `std::collections`"}),
            serde_json::json!({"message": "unresolved import `crate::模块`"}),
            serde_json::json!({"message": "cannot find value `x` in this scope"}),
            serde_json::json!({"message": "unresolved import `serde_json`"}),
        ];
        assert_eq!(
            从诊断提取未解析包名(&诊断表),
            vec!["serde_json".to_string(), "reqwest".to_string()]
        );

        // 无诊断 / 无匹配消息 → 空列表
        assert!(从诊断提取未解析包名(&[]).is_empty());
        let 其他诊断 = vec![serde_json::json!({"message": "mismatched types"})];
        assert!(从诊断提取未解析包名(&其他诊断).is_empty());
    }

    /// 给 rust-analyzer 的配置答复：每条目固定三项关键配置，
    /// 数量与请求的条目对齐（数量错误会让分析器对部分工作区缺省）
    #[test]
    fn 分析器配置答复形状() {
        let 空答复 = 分析器配置答复(0);
        assert_eq!(空答复, serde_json::json!([]));
        let 答复值 = 分析器配置答复(2);
        let 数组值 = 答复值.as_array().expect("应为数组");
        assert_eq!(数组值.len(), 2);
        for 项 in 数组值 {
            assert_eq!(项["completion"]["snippets"], serde_json::json!("custom"));
            assert_eq!(项["checkOnSave"]["enable"], serde_json::json!(false));
            assert_eq!(项["check"]["enable"], serde_json::json!(false));
        }
    }

    /// 是符号链接：不跟随末段链接——悬空链接仍是链接；普通文件/不存在路径为假
    ///
    /// 链接构造只在 Unix 上跑（Windows 建符号链接需开发者模式/管理员权限，
    /// 普通持续集成跑不上）；非链接分支三平台都测。
    #[test]
    fn 是符号链接语义() {
        let 临时 = tempfile::tempdir().unwrap();
        let 文件值 = 临时.path().join("f");
        std::fs::write(&文件值, b"x").unwrap();
        assert!(!是符号链接(&文件值));
        assert!(!是符号链接(&临时.path().join("absent")));

        #[cfg(unix)]
        {
            let 链接值 = 临时.path().join("link");
            std::os::unix::fs::symlink(&文件值, &链接值).unwrap();
            assert!(是符号链接(&链接值));

            // 悬空符号链接（目标不存在）仍须识别为链接
            let 悬空值 = 临时.path().join("dangling");
            std::os::unix::fs::symlink(临时.path().join("gone"), &悬空值).unwrap();
            assert!(是符号链接(&悬空值));
        }
    }

    /// 进程号探活：当前进程必然存活（残留目录清理依据，误判会误删活进程目录）
    #[test]
    fn 进程存活自身() {
        assert!(进程存活(std::process::id()));
    }

    /// 虚拟临时目录：按用户与进程号隔离命名、实际创建、同进程重复调用稳定同目录
    #[test]
    fn 虚拟临时目录形状与稳定() {
        let 目录值 = 虚拟临时目录().expect("应能创建虚拟目录");
        assert!(目录值.is_dir());
        let 名称值 = 目录值.file_name().unwrap().to_string_lossy().into_owned();
        assert!(名称值.starts_with("i18n_lsp_virtual_"), "实际：{名称值}");
        assert!(名称值.ends_with(&std::process::id().to_string()));
        let 再次目录 = 虚拟临时目录().expect("重复调用应幂等");
        assert_eq!(再次目录, 目录值);
    }

    // ===== 处理分析器消息：消息分发的分支级单测 =====

    /// 分发装置返回的部件组：映射器、待处理表、客户端通道两端、
    /// 内置诊断、空分析器回发端、以及（缓存、临时目录、虚拟资源标识）
    type 分发装置返回值 = (
        Arc<响应映射器>,
        待处理请求表类型,
        crossbeam_channel::Sender<Message>,
        crossbeam_channel::Receiver<Message>,
        内置诊断表类型,
        crate::分析器::发送端,
        (Arc<转译缓存>, tempfile::TempDir, String),
    );

    /// 构造分发单测所需的全部部件
    fn 分发装置(源文本: &str) -> 分发装置返回值 {
        let (缓存实例, 临时目录值) = 新建测试缓存();
        let (条目值, _) = 缓存实例
            .更新文档("file:///test/main.zh", 源文本, 1)
            .expect("测试文档应可入缓存");
        let 映射器 = Arc::new(响应映射器::新建(缓存实例.clone()));
        let 待处理表 = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let 内置诊断 = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let (发送端值, 接收端值) = crossbeam_channel::unbounded();
        let 回发端 = crate::分析器::发送端::测试用空端();
        (
            映射器,
            待处理表,
            发送端值,
            接收端值,
            内置诊断,
            回发端,
            (缓存实例, 临时目录值, 条目值.虚拟资源标识.clone()),
        )
    }

    /// 登记一条转发请求记录
    fn 登记待处理请求(
        待处理表: &待处理请求表类型,
        分析器标识: i64,
        客户端标识: i32,
        请求方法: &str,
        资源标识: &str,
    ) {
        待处理表.lock().unwrap().insert(
            分析器标识,
            待处理请求信息 {
                原始标识: lsp_server::RequestId::from(客户端标识),
                请求方法: 请求方法.to_string(),
                原始资源标识: 资源标识.to_string(),
                创建时刻: std::time::Instant::now(),
                未解析包名表: if 请求方法 == "textDocument/codeAction" {
                    vec!["serde_json".to_string()]
                } else {
                    Vec::new()
                },
                教学诊断表: Vec::new(),
            },
        );
    }

    /// 全部响应映射分支：空结果也必须安全通过各自映射器并应答到客户端，
    /// 原始客户端请求标识必须原样带回
    #[test]
    fn 处理分析器响应方法分支() {
        let (映射器, 待处理表, 发送端值, 接收端值, 内置诊断, 回发端, _装置) =
            分发装置("函数 主() {\n    打印行!();\n}\n");
        let 方法表 = [
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
        for (序号, 方法名) in 方法表.iter().enumerate() {
            登记待处理请求(
                &待处理表,
                序号 as i64 + 1,
                序号 as i32 + 100,
                方法名,
                "file:///test/main.zh",
            );
        }
        for 序号 in 0..方法表.len() {
            let 请求标识值 = 序号 as i64 + 1;
            let 消息项 = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 请求标识值,
                "result": Value::Null
            });
            处理分析器消息(&消息项, &映射器, &发送端值, &待处理表, &回发端, &内置诊断);
        }
        // 13 条全部应答（按发送顺序即为待处理表插入顺序）
        let 已收消息: Vec<Message> = 接收端值.try_iter().collect();
        assert_eq!(已收消息.len(), 方法表.len());
        for (序号, 消息项) in 已收消息.iter().enumerate() {
            match 消息项 {
                Message::Response(响应值) => {
                    assert_eq!(响应值.id, lsp_server::RequestId::from(序号 as i32 + 100));
                    assert!(
                        !响应值.response_result.is_err(),
                        "分支 {序号} 不应产生错误应答"
                    );
                }
                其他 => panic!("期望响应消息，实际收到 {其他:?}"),
            }
        }
    }

    /// 分析器返回错误（如方法未知）：错误体原样透传给客户端，
    /// 缺省错误码归一为内部错误
    #[test]
    fn 处理分析器错误透传() {
        let (映射器, 待处理表, 发送端值, 接收端值, 内置诊断, 回发端, _装置) =
            分发装置("函数 主() {}\n");
        登记待处理请求(
            &待处理表,
            1,
            100,
            "textDocument/hover",
            "file:///test/main.zh",
        );
        let 消息项 = serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "error": { "code": -32601, "message": "method not found" }
        });
        处理分析器消息(&消息项, &映射器, &发送端值, &待处理表, &回发端, &内置诊断);
        match 接收端值.recv().unwrap() {
            Message::Response(响应值) => {
                assert_eq!(响应值.id, lsp_server::RequestId::from(100));
                let 错误值 = 响应值.response_result.expect_err("应透传错误");
                assert_eq!(错误值.code, -32601);
                assert_eq!(错误值.message, "method not found");
            }
            其他 => panic!("期望响应消息：{其他:?}"),
        }
        // 缺错误码：归一内部错误；缺消息文本：用内置文案
        登记待处理请求(
            &待处理表,
            2,
            101,
            "textDocument/hover",
            "file:///test/main.zh",
        );
        let 消息项 = serde_json::json!({ "jsonrpc": "2.0", "id": 2, "error": {} });
        处理分析器消息(&消息项, &映射器, &发送端值, &待处理表, &回发端, &内置诊断);
        match 接收端值.recv().unwrap() {
            Message::Response(响应值) => {
                let 错误值 = 响应值.response_result.unwrap_err();
                assert_eq!(错误值.code, -32603);
                assert!(!错误值.message.is_empty(), "缺消息文本应用内置文案");
            }
            其他 => panic!("期望响应消息：{其他:?}"),
        }
    }

    /// 诊断发布：非虚拟资源标识丢弃；虚拟资源标识经映射后转发到母语资源标识，
    /// 并写入内置诊断缓存供编译检查合并
    #[test]
    fn 处理分析器诊断路由() {
        let (映射器, 待处理表, 发送端值, 接收端值, 内置诊断, 回发端, 装置值) =
            分发装置("函数 主() {\n    打印行!();\n}\n");
        let (_缓存值, _临时值, 虚拟资源标识) = 装置值;

        // 非虚拟资源标识：直接丢弃，客户端收不到
        let 外来消息 = serde_json::json!({
            "jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
            "params": { "uri": "file:///other/project/src/main.rs", "diagnostics": [] }
        });
        处理分析器消息(&外来消息, &映射器, &发送端值, &待处理表, &回发端, &内置诊断);
        assert!(接收端值.is_empty(), "非虚拟文件诊断必须丢弃");

        // 虚拟资源标识：映射回母语资源标识并转发
        let 诊断项 = serde_json::json!({
            "range": {
                "start": { "line": 1, "character": 4 },
                "end": { "line": 1, "character": 12 }
            },
            "severity": 1,
            "source": "rust-analyzer",
            "message": "mismatched types",
            "code": "E0308"
        });
        let 虚拟消息 = serde_json::json!({
            "jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
            "params": { "uri": 虚拟资源标识, "diagnostics": [诊断项] }
        });
        处理分析器消息(&虚拟消息, &映射器, &发送端值, &待处理表, &回发端, &内置诊断);
        match 接收端值.recv().unwrap() {
            Message::Notification(通知值) => {
                assert_eq!(通知值.method, "textDocument/publishDiagnostics");
                assert_eq!(
                    通知值.params["uri"].as_str(),
                    Some("file:///test/main.zh"),
                    "资源标识应还原为母语文件"
                );
                assert!(
                    !通知值.params["diagnostics"].as_array().unwrap().is_empty(),
                    "原始诊断应保留（非编译项目不抑制）"
                );
            }
            其他 => panic!("期望通知消息：{其他:?}"),
        }
        // 内置诊断以母语资源标识为键缓存
        let 守卫 = 内置诊断.lock().unwrap();
        let 缓存项 = 守卫.get("file:///test/main.zh").expect("应缓存内置诊断");
        assert!(!缓存项.is_empty());
    }

    /// 其他通知（日志消息等）原样转发给客户端
    #[test]
    fn 处理分析器其他通知转发() {
        let (映射器, 待处理表, 发送端值, 接收端值, 内置诊断, 回发端, _装置) =
            分发装置("函数 主() {}\n");
        let 消息项 = serde_json::json!({
            "jsonrpc": "2.0", "method": "window/logMessage",
            "params": { "type": 3, "message": "indexed" }
        });
        处理分析器消息(&消息项, &映射器, &发送端值, &待处理表, &回发端, &内置诊断);
        match 接收端值.recv().unwrap() {
            Message::Notification(通知值) => {
                assert_eq!(通知值.method, "window/logMessage");
                assert_eq!(通知值.params["message"].as_str(), Some("indexed"));
            }
            其他 => panic!("期望通知消息：{其他:?}"),
        }
    }

    /// 分析器主动请求（无待处理记录）：配置请求按请求项数构造结果回发给分析器
    ///（本用例空发送器发送失败被忽略，验证分发不崩溃且不会误转发给客户端）
    #[test]
    fn 处理分析器主动请求回发() {
        let (映射器, 待处理表, 发送端值, 接收端值, 内置诊断, 回发端, _装置) =
            分发装置("函数 主() {}\n");
        let 消息项 = serde_json::json!({
            "jsonrpc": "2.0", "id": 999, "method": "workspace/configuration",
            "params": { "items": [ {}, {} ] }
        });
        处理分析器消息(&消息项, &映射器, &发送端值, &待处理表, &回发端, &内置诊断);
        assert!(
            接收端值.is_empty(),
            "回发给分析器的响应不应出现在客户端通道"
        );
        // 无方法名的杂讯（既非响应也非通知）：静默忽略
        let 杂讯 = serde_json::json!({ "jsonrpc": "2.0" });
        处理分析器消息(&杂讯, &映射器, &发送端值, &待处理表, &回发端, &内置诊断);
        assert!(接收端值.is_empty());
    }

    /// 超时清理：仅超过请求超时的条目被移除并主动应答超时错误；
    /// 新条目保留且不产生消息
    #[test]
    fn 清理超时请求仅旧项() {
        let (_映射器, 待处理表, 发送端值, 接收端值, _内置诊断, _回发端, _装置) =
            分发装置("函数 主() {}\n");
        // 新条目
        登记待处理请求(
            &待处理表,
            1,
            100,
            "textDocument/hover",
            "file:///test/main.zh",
        );
        // 过期条目
        {
            let mut 请求表 = 待处理表.lock().unwrap();
            请求表.get_mut(&1).unwrap().创建时刻 = std::time::Instant::now();
            请求表.insert(
                2,
                待处理请求信息 {
                    原始标识: lsp_server::RequestId::from(101),
                    请求方法: "textDocument/hover".to_string(),
                    原始资源标识: "file:///test/main.zh".to_string(),
                    创建时刻: std::time::Instant::now()
                        .checked_sub(请求超时 + std::time::Duration::from_secs(1))
                        .unwrap(),
                    未解析包名表: Vec::new(),
                    教学诊断表: Vec::new(),
                },
            );
        }
        清理超时请求(&待处理表, &发送端值);
        // 仅 1 条超时应答
        match 接收端值.try_iter().next() {
            Some(Message::Response(响应值)) => {
                assert_eq!(响应值.id, lsp_server::RequestId::from(101));
                assert_eq!(响应值.response_result.unwrap_err().code, -32603);
            }
            其他 => panic!("期望超时响应消息：{其他:?}"),
        }
        assert!(接收端值.is_empty(), "新条目不应产生应答");
        let 请求表 = 待处理表.lock().unwrap();
        assert!(请求表.contains_key(&1), "新条目必须保留");
        assert!(!请求表.contains_key(&2), "过期条目必须移除");
    }

    /// 分析器崩溃重启：全部挂起请求排空，逐条收到“已重启”错误应答
    #[test]
    fn 全部待处理失败应答排空() {
        let (_映射器, 待处理表, 发送端值, 接收端值, _内置诊断, _回发端, _装置) =
            分发装置("函数 主() {}\n");
        登记待处理请求(
            &待处理表,
            1,
            100,
            "textDocument/hover",
            "file:///test/main.zh",
        );
        登记待处理请求(
            &待处理表,
            2,
            101,
            "textDocument/completion",
            "file:///test/main.zh",
        );
        全部待处理失败应答(&待处理表, &发送端值);
        assert!(待处理表.lock().unwrap().is_empty(), "排空后表应为空");
        let mut 标识表 = Vec::new();
        while let Some(Message::Response(响应值)) = 接收端值.try_iter().next() {
            assert_eq!(
                响应值.response_result.as_ref().unwrap_err().message,
                "rust-analyzer 已重启"
            );
            标识表.push(响应值.id);
        }
        assert_eq!(标识表.len(), 2, "两个挂起请求都应被应答");
    }
}
