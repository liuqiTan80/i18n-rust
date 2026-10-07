//! 分析器连接模块
//!
//! 管理 rust-analyzer 子进程的生命周期：启动、消息收发、关闭。
//! 通过 stdin/stdout 以 LSP 协议（Content-Length 分帧）与之通信。
//!
//! 注：外部 ABI 走透传（`std`/`crossbeam_channel`/`serde_json`/`anyhow`/`log`/
//! `i18n_rust_engine::工具链::查找工具链程序`/`crate::本地化::{全局, 取文, 取文带参}`）。
//! 本模块类型、构造、私有辅助函数、字段与局部全中文化；仅实例方法
//! `send/try_recv/message_channel/sender_clone/is_crashed/clear_crashed/restart/stop`
//! 刻意保持英文——它们镜像 crossbeam Channel API 语义，与同 crate 内
//! `rx.try_recv()`/`events.send()` 等真实通道方法同名，改名会与第三方方法混淆。

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::Value;

/// rust-analyzer 子进程管理器
///
/// 支持崩溃自动重启：消息出口（receiver）与崩溃标志不随进程重启而变化，
/// 转发线程无需重建；重启仅替换进程句柄与写入端，并由代理主循环
/// 重新执行 initialize/initialized/工作区/文档同步握手。
pub struct 分析器连接 {
    /// 子进程句柄（重启/停止时替换）
    子进程句柄: Mutex<Option<Child>>,
    /// 写入端（向 rust-analyzer 发送消息；锁内发送，重启时替换）
    写入端: Arc<Mutex<Option<std::process::ChildStdin>>>,
    /// 统一消息出口（不随重启变化，转发线程持续读取）
    消息出口: crossbeam_channel::Sender<Value>,
    /// 消息接收通道（消息出口 的接收端，可克隆）
    接收端: crossbeam_channel::Receiver<Value>,
    /// 读取线程检测到输出结束（进程崩溃/异常退出）置位，
    /// 供代理主循环检测并触发自动重启
    崩溃标志: Arc<AtomicBool>,
    /// 当前进程的主动停止抑制标志：每次 spawn 新建并替换（stop() 置位当前
    /// 标志，reader 线程持各自标志），避免重启后旧进程 EOF 与新标志竞态
    停止标志: Mutex<Arc<AtomicBool>>,
}

impl 分析器连接 {
    /// 启动 rust-analyzer 子进程
    pub fn 启动服务() -> anyhow::Result<Self> {
        let (消息出口, 接收端) = crossbeam_channel::unbounded();
        let 连接 = Self {
            子进程句柄: Mutex::new(None),
            写入端: Arc::new(Mutex::new(None)),
            消息出口,
            接收端,
            崩溃标志: Arc::new(AtomicBool::new(false)),
            停止标志: Mutex::new(Arc::new(AtomicBool::new(false))),
        };
        连接.启动子进程()?;
        Ok(连接)
    }

    /// 启动（或崩溃后重新启动）rust-analyzer 子进程并挂接消息管道
    fn 启动子进程(&self) -> anyhow::Result<()> {
        let 分析器路径 = 查找分析器程序()?;
        log::info!(
            "{}",
            crate::本地化::全局()
                .取文带参("lsp_log_ra_starting", &[&分析器路径.display().to_string()])
        );
        let mut 命令 = Command::new(&分析器路径);
        // 新版 rust-analyzer 已移除 --stdio 参数（stdio 为默认模式）
        self.用命令启动(&mut 命令)
    }

    /// 用指定命令启动子进程（#[cfg(test)] 注入假进程用）
    fn 用命令启动(&self, 命令: &mut Command) -> anyhow::Result<()> {
        let mut 子进程 = 命令
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|错误值| {
                anyhow::anyhow!(
                    "{}",
                    crate::本地化::全局().取文带参("lsp_err_ra_start", &[&错误值.to_string()])
                )
            })?;

        let 写入端 = 子进程
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("{}", crate::本地化::全局().取文("lsp_err_ra_stdin")))?;
        let 读取器 = 子进程.stdout.take().ok_or_else(|| {
            anyhow::anyhow!("{}", crate::本地化::全局().取文("lsp_err_ra_stdout"))
        })?;

        // 新进程的停止抑制标志：reader 线程持各自标志，stop() 只置位当前标志
        let 停止标志 = Arc::new(AtomicBool::new(false));
        *self.子进程句柄.lock().map_err(|_| {
            anyhow::anyhow!("{}", crate::本地化::全局().取文("lsp_err_write_lock"))
        })? = Some(子进程);
        *self.写入端.lock().map_err(|_| {
            anyhow::anyhow!("{}", crate::本地化::全局().取文("lsp_err_write_lock"))
        })? = Some(写入端);
        *self.停止标志.lock().map_err(|_| {
            anyhow::anyhow!("{}", crate::本地化::全局().取文("lsp_err_write_lock"))
        })? = 停止标志.clone();

        // 后台线程：持续读取 rust-analyzer 的 stdout，转发到统一消息出口；
        // 输出结束（进程崩溃/被停止）时退出线程并报告崩溃状态
        let 消息出口 = self.消息出口.clone();
        let 崩溃标志 = self.崩溃标志.clone();
        thread::spawn(move || {
            let mut 缓冲读取器 = BufReader::new(读取器);
            loop {
                match 读取单条分析器消息(&mut 缓冲读取器) {
                    Some(消息) => {
                        if 消息出口.send(消息).is_err() {
                            break;
                        }
                    }
                    None => {
                        log::info!("{}", crate::本地化::全局().取文("lsp_log_ra_output_ended"));
                        // 非主动停止视为异常退出：置崩溃标志供主循环自动重启
                        if !停止标志.load(Ordering::SeqCst) {
                            崩溃标志.store(true, Ordering::SeqCst);
                        }
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// 向 rust-analyzer 发送一条 JSON-RPC 消息
    pub fn send(&self, 消息: &Value) -> anyhow::Result<()> {
        let 文本串 = serde_json::to_string(消息)?;
        let 帧 = format!("Content-Length: {}\r\n\r\n{}", 文本串.len(), 文本串);

        let mut 写入端 = self
            .写入端
            .lock()
            .map_err(|_| anyhow::anyhow!("{}", crate::本地化::全局().取文("lsp_err_write_lock")))?;
        let 标准输入 = 写入端.as_mut().ok_or_else(|| {
            anyhow::anyhow!("{}", crate::本地化::全局().取文("lsp_err_ra_not_running"))
        })?;
        标准输入.write_all(帧.as_bytes())?;
        标准输入.flush()?;

        log::debug!("-> rust-analyzer: {}", 截断文本(&文本串, 200));
        Ok(())
    }

    /// 获取消息接收通道的克隆（可在多线程中共享）
    pub fn 取消息通道(&self) -> crossbeam_channel::Receiver<Value> {
        self.接收端.clone()
    }

    /// 获取用于后台线程回发消息的发送器克隆
    ///
    /// 转发线程需要应答 rust-analyzer 主动发来的请求
    /// （如 workspace/diagnostic/refresh），因此需要独立于
    /// 主线程的发送能力。
    pub fn 取发送器副本(&self) -> 发送器 {
        发送器 {
            写入端: self.写入端.clone(),
        }
    }

    /// 尝试非阻塞接收一条来自 rust-analyzer 的消息
    pub fn try_recv(&self) -> Option<Value> {
        self.接收端.try_recv().ok()
    }

    /// rust-analyzer 是否已异常退出（读取线程报告，等待自动重启）
    pub fn 检测崩溃(&self) -> bool {
        self.崩溃标志.load(Ordering::SeqCst)
    }

    /// 清除崩溃标志（自动重启失败时调用，避免主循环忙循环）
    pub fn 清除崩溃标志(&self) {
        self.崩溃标志.store(false, Ordering::SeqCst);
    }

    /// 重启 rust-analyzer 子进程（崩溃后由代理主循环调用）
    ///
    /// 消息出口（接收端）不重建：转发线程持续消费；
    /// 重握手由调用方（代理主循环）完成。
    pub fn 重启进程(&self) -> anyhow::Result<()> {
        self.停止进程();
        self.崩溃标志.store(false, Ordering::SeqCst);
        self.启动子进程()
    }

    /// 停止 rust-analyzer 子进程（置当前进程的停止标志，EOF 不再报告崩溃）
    pub fn 停止进程(&self) {
        if let Some(标志) = self.停止标志.lock().ok().map(|守卫| 守卫.clone()) {
            标志.store(true, Ordering::SeqCst);
        }
        if let Some(mut 进程) = self.子进程句柄.lock().ok().and_then(|mut 槽| 槽.take()) {
            log::info!("{}", crate::本地化::全局().取文("lsp_log_ra_stopping"));
            let _ = 进程.kill();
            let _ = 进程.wait();
        }
    }
}

/// 独立的 rust-analyzer 消息发送器（供其他线程使用）
pub struct 发送器 {
    写入端: Arc<Mutex<Option<std::process::ChildStdin>>>,
}

impl 发送器 {
    /// 向 rust-analyzer 发送一条 JSON-RPC 消息
    pub fn send(&self, 消息: &Value) -> anyhow::Result<()> {
        let 文本串 = serde_json::to_string(消息)?;
        let 帧 = format!("Content-Length: {}\r\n\r\n{}", 文本串.len(), 文本串);

        let mut 写入端 = self
            .写入端
            .lock()
            .map_err(|_| anyhow::anyhow!("{}", crate::本地化::全局().取文("lsp_err_write_lock")))?;
        let 标准输入 = 写入端.as_mut().ok_or_else(|| {
            anyhow::anyhow!("{}", crate::本地化::全局().取文("lsp_err_ra_not_running"))
        })?;
        标准输入.write_all(帧.as_bytes())?;
        标准输入.flush()?;

        log::debug!("-> rust-analyzer: {}", 截断文本(&文本串, 200));
        Ok(())
    }
}

impl Drop for 分析器连接 {
    fn drop(&mut self) {
        self.停止进程();
    }
}

#[cfg(test)]
impl 发送器 {
    /// 测试用空发送器：无 RA 子进程，`send` 恒返回"未运行"错误。
    ///
    /// 供 [`crate::server`] 的消息分发单测构造参数——这些用例只验证
    /// 响应映射/通知路由，不需要真实 rust-analyzer 回环（回环由 e2e 覆盖）。
    pub(crate) fn 测试用空发送器() -> Self {
        Self {
            写入端: Arc::new(Mutex::new(None)),
        }
    }
}

/// 从 BufReader 中读取一条 LSP 消息（Content-Length 分帧）
fn 读取单条分析器消息<R: BufRead>(读取器: &mut R) -> Option<Value> {
    let mut 消息头行 = String::new();
    let mut 内容长度: Option<usize> = None;

    loop {
        消息头行.clear();
        match 读取器.read_line(&mut 消息头行) {
            Ok(0) => return None,
            Ok(_) => {}
            Err(错误值) => {
                log::error!(
                    "{}",
                    crate::本地化::全局()
                        .取文带参("lsp_err_read_output", &[&错误值.to_string()])
                );
                return None;
            }
        }

        let 去空白 = 消息头行.trim();
        if 去空白.is_empty() {
            break;
        }

        if let Some(长度串) = 去空白.strip_prefix("Content-Length:")
            && let Ok(长度数值项) = 长度串.trim().parse::<usize>()
        {
            内容长度 = Some(长度数值项);
        }
    }

    let 长度 = 内容长度?;

    let mut 缓冲区 = vec![0u8; 长度];
    if std::io::Read::read_exact(读取器, &mut 缓冲区).is_err() {
        log::error!("{}", crate::本地化::全局().取文("lsp_err_read_body"));
        return None;
    }

    let 文本串 = String::from_utf8(缓冲区).ok()?;
    log::debug!("<- rust-analyzer: {}", 截断文本(&文本串, 200));

    serde_json::from_str(&文本串).ok()
}

/// 查找 rust-analyzer 可执行文件
///
/// 优先级：
/// 0. 内置工具链（~/.rz/toolchain/bin，`rzc install toolchain` 安装的 standalone 版）
/// 1. RUST_ANALYZER_PATH 环境变量
/// 2. rustup which --toolchain stable（真实路径，不受项目 rust-toolchain.toml 影响）
/// 3. PATH 扫描（跨平台，替代 Unix 专属 which；Windows 追加 PATHEXT 后缀）
/// 4. 常见安装位置（含 ~/.cargo/bin，Windows 回退 USERPROFILE）
fn 查找分析器程序() -> anyhow::Result<PathBuf> {
    // 0. 用户显式指定的环境变量（最高优先级，覆盖内置与 PATH）
    if let Ok(路径) = std::env::var("RUST_ANALYZER_PATH") {
        let 路径项 = PathBuf::from(&路径);
        if 路径项.exists() {
            return Ok(路径项);
        }
    }

    // 1. 内置工具链（脱离 rustup 的 standalone 版本，版本自管）
    if let Some(路径项) = i18n_rust_engine::工具链::查找工具链程序("rust-analyzer") {
        return Ok(路径项);
    }

    // rustup which 返回真实二进制路径（非 shim），避免被项目 rust-toolchain.toml
    // （如 1.85 无 rust-analyzer 组件）导致 Unknown binary 启动失败
    if let Ok(输出) = Command::new("rustup")
        .args(["which", "--toolchain", "stable", "rust-analyzer"])
        .output()
        && 输出.status.success()
    {
        let 文本项 = String::from_utf8_lossy(&输出.stdout).trim().to_string();
        if !文本项.is_empty() {
            let 路径项 = PathBuf::from(&文本项);
            if 路径项.exists() {
                return Ok(路径项);
            }
        }
    }

    // PATH 扫描（engine 工具链定位统一实现：含 Windows PATHEXT 探测）
    if let Some(路径项) = i18n_rust_engine::工具链::查找工具链程序("rust-analyzer") {
        return Ok(路径项);
    }

    let 候选 = [
        主目录路径("cargo/bin/rust-analyzer"),
        主目录路径(".cargo/bin/rust-analyzer"),
        PathBuf::from("/usr/local/bin/rust-analyzer"),
        PathBuf::from("/usr/bin/rust-analyzer"),
    ];
    for 路径候选项 in 候选 {
        if 路径候选项.exists() {
            return Ok(路径候选项);
        }
    }

    anyhow::bail!("{}", crate::本地化::全局().取文("lsp_err_ra_not_found"))
}

fn 主目录路径(相对路径: &str) -> PathBuf {
    // HOME（Unix）优先，USERPROFILE（Windows）回退，保证跨平台定位到用户主目录
    let 主目录 = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if 主目录.is_empty() {
        PathBuf::from(相对路径)
    } else {
        PathBuf::from(主目录).join(相对路径)
    }
}

fn 截断文本(文本项: &str, 最大长度: usize) -> String {
    if 文本项.len() <= 最大长度 {
        文本项.to_string()
    } else {
        // 按字符边界安全截断，避免切到多字节字符（中文）中间
        let mut 边界 = 最大长度;
        while 边界 > 0 && !文本项.is_char_boundary(边界) {
            边界 -= 1;
        }
        format!("{}...", &文本项[..边界])
    }
}

#[cfg(test)]
mod 单元测试 {
    use super::*;
    use std::io::Cursor;

    /// 合法 Content-Length 分帧消息可解析出 JSON
    #[test]
    fn 测试读取单条消息合法() {
        let 数据 = "Content-Length: 17\r\n\r\n{\"method\":\"test\"}";
        let mut 游标 = Cursor::new(数据);
        let 消息 = 读取单条分析器消息(&mut 游标).expect("应解析出消息");
        assert_eq!(消息["method"], "test");
    }

    /// 消息头中缺少 Content-Length 时返回 None
    #[test]
    fn 测试读取单条消息缺内容长度() {
        let 数据 = "Content-Type: application/json\r\n\r\n{}";
        let mut 游标 = Cursor::new(数据);
        assert!(读取单条分析器消息(&mut 游标).is_none());
    }

    /// 消息体长度不足（提前 EOF）时返回 None
    #[test]
    fn 测试读取单条消息体截断() {
        let 数据 = "Content-Length: 100\r\n\r\n{\"method\":\"test\"}";
        let mut 游标 = Cursor::new(数据);
        assert!(读取单条分析器消息(&mut 游标).is_none());
    }

    /// 消息体为非 UTF-8 字节时返回 None
    #[test]
    fn 测试读取单条消息非utf8() {
        let 数据 = b"Content-Length: 2\r\n\r\n\xFF\xFE";
        let mut 游标 = Cursor::new(数据.as_slice());
        assert!(读取单条分析器消息(&mut 游标).is_none());
    }

    /// 输入为 EOF 时返回 None
    #[test]
    fn 测试读取单条消息空输入返回无() {
        let mut 游标 = Cursor::new("");
        assert!(读取单条分析器消息(&mut 游标).is_none());
    }

    /// 短文本原样返回
    #[test]
    fn 测试截断短文本不变() {
        assert_eq!(截断文本("abc", 10), "abc");
        assert_eq!(截断文本("hello world", 5), "hello...");
    }

    /// 截断长度落在多字节字符中间时回退到字符边界
    #[test]
    fn 测试截断字符边界安全() {
        assert_eq!(截断文本("中文测试", 3), "中...");
        assert_eq!(截断文本("中文测试", 4), "中...");
        assert_eq!(截断文本("中文测试", 6), "中文...");
    }

    /// RUST_ANALYZER_PATH 环境变量优先返回
    #[test]
    fn 测试查找分析器环境变量优先() {
        let 临时 = tempfile::NamedTempFile::new().expect("创建临时文件失败");
        unsafe {
            std::env::set_var("RUST_ANALYZER_PATH", 临时.path());
        }
        let 找到 = 查找分析器程序().expect("应通过环境变量找到");
        assert_eq!(找到, 临时.path());
        unsafe {
            std::env::remove_var("RUST_ANALYZER_PATH");
        }
    }

    /// PATH 扫描由 engine 工具链定位统一实现（见 crates/engine/src/工具链.rs）
    #[test]
    fn 测试引擎查找工具链程序缺失() {
        // 内置与 PATH 都不存在的二进制名应返回 None（不 panic）
        let 名称 = format!("__ra_nonexistent__{}", std::process::id());
        assert!(i18n_rust_engine::工具链::查找工具链程序(&名称).is_none());
    }

    /// 构造空连接（未启动任何子进程）
    fn 构造空连接() -> 分析器连接 {
        let (消息出口, 接收端) = crossbeam_channel::unbounded();
        分析器连接 {
            子进程句柄: Mutex::new(None),
            写入端: Arc::new(Mutex::new(None)),
            消息出口,
            接收端,
            崩溃标志: Arc::new(AtomicBool::new(false)),
            停止标志: Mutex::new(Arc::new(AtomicBool::new(false))),
        }
    }

    /// 等待崩溃标志置位（最多 2 秒）
    fn 等待崩溃(连接: &分析器连接) -> bool {
        for _ in 0..40 {
            if 连接.检测崩溃() {
                return true;
            }
            thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }

    /// 子进程异常退出（EOF 且非主动停止）时置崩溃标志
    #[cfg(unix)]
    #[test]
    fn 测试异常退出置崩溃标志() {
        let 连接 = 构造空连接();
        连接
            .用命令启动(Command::new("sh").arg("-c").arg("exit 0"))
            .expect("启动假进程失败");
        assert!(等待崩溃(&连接), "进程立即退出应触发崩溃标志");
        // 模拟重启：停止旧进程（其 EOF 不再报崩溃）+ 复位标志 + 启动新进程
        连接.停止进程();
        连接.清除崩溃标志();
        连接
            .用命令启动(Command::new("sh").arg("-c").arg("exit 1"))
            .expect("二次启动失败");
        assert!(!连接.检测崩溃(), "重启后标志应复位");
        assert!(等待崩溃(&连接), "新进程退出应再次置位");
    }

    /// 主动 stop() 后的 EOF 不置崩溃标志
    #[cfg(unix)]
    #[test]
    fn 测试优雅停止不置崩溃标志() {
        let 连接 = 构造空连接();
        连接
            .用命令启动(Command::new("sh").arg("-c").arg("sleep 5"))
            .expect("启动假进程失败");
        连接.停止进程();
        thread::sleep(std::time::Duration::from_millis(200));
        assert!(!连接.检测崩溃(), "主动停止不应报告崩溃");
    }

    /// 未启动（重启窗口内）发送返回明确错误而非 panic
    #[test]
    fn 测试未运行发送报错() {
        let 连接 = 构造空连接();
        let 错误值 = 连接
            .send(&serde_json::json!({ "method": "test" }))
            .unwrap_err();
        assert!(
            错误值.to_string().contains("不可用")
                || 错误值.to_string().contains("lsp_err_ra_not_running")
        );
    }
}
