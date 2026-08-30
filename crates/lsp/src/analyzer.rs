//! 分析器连接模块
//!
//! 管理 rust-analyzer 子进程的生命周期：启动、消息收发、关闭。
//! 通过 stdin/stdout 以 LSP 协议（Content-Length 分帧）与之通信。

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
pub struct AnalyzerConnection {
    /// 子进程句柄（重启/停止时替换）
    child: Mutex<Option<Child>>,
    /// 写入端（向 rust-analyzer 发送消息；锁内发送，重启时替换）
    writer: Arc<Mutex<Option<std::process::ChildStdin>>>,
    /// 统一消息出口（不随重启变化，转发线程持续读取）
    events: crossbeam_channel::Sender<Value>,
    /// 消息接收通道（events 的接收端，可克隆）
    receiver: crossbeam_channel::Receiver<Value>,
    /// 读取线程检测到输出结束（进程崩溃/异常退出）置位，
    /// 供代理主循环检测并触发自动重启
    crashed: Arc<AtomicBool>,
    /// 当前进程的主动停止抑制标志：每次 spawn 新建并替换（stop() 置位当前
    /// 标志，reader 线程持各自标志），避免重启后旧进程 EOF 与新标志竞态
    stop_flag: Mutex<Arc<AtomicBool>>,
}

impl AnalyzerConnection {
    /// 启动 rust-analyzer 子进程
    pub fn start() -> anyhow::Result<Self> {
        let (events, receiver) = crossbeam_channel::unbounded();
        let conn = Self {
            child: Mutex::new(None),
            writer: Arc::new(Mutex::new(None)),
            events,
            receiver,
            crashed: Arc::new(AtomicBool::new(false)),
            stop_flag: Mutex::new(Arc::new(AtomicBool::new(false))),
        };
        conn.spawn_child()?;
        Ok(conn)
    }

    /// 启动（或崩溃后重新启动）rust-analyzer 子进程并挂接消息管道
    fn spawn_child(&self) -> anyhow::Result<()> {
        let ra_path = find_rust_analyzer()?;
        log::info!(
            "{}",
            crate::ui::global().f("lsp_log_ra_starting", &[&ra_path.display().to_string()])
        );
        let mut command = Command::new(&ra_path);
        // 新版 rust-analyzer 已移除 --stdio 参数（stdio 为默认模式）
        self.spawn_command(&mut command)
    }

    /// 用指定命令启动子进程（#[cfg(test)] 注入假进程用）
    fn spawn_command(&self, command: &mut Command) -> anyhow::Result<()> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| {
                anyhow::anyhow!(
                    "{}",
                    crate::ui::global().f("lsp_err_ra_start", &[&e.to_string()])
                )
            })?;

        let writer = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("{}", crate::ui::global().t("lsp_err_ra_stdin")))?;
        let reader = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("{}", crate::ui::global().t("lsp_err_ra_stdout")))?;

        // 新进程的停止抑制标志：reader 线程持各自标志，stop() 只置位当前标志
        let stop_flag = Arc::new(AtomicBool::new(false));
        *self
            .child
            .lock()
            .map_err(|_| anyhow::anyhow!("{}", crate::ui::global().t("lsp_err_write_lock")))? =
            Some(child);
        *self
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("{}", crate::ui::global().t("lsp_err_write_lock")))? =
            Some(writer);
        *self
            .stop_flag
            .lock()
            .map_err(|_| anyhow::anyhow!("{}", crate::ui::global().t("lsp_err_write_lock")))? =
            stop_flag.clone();

        // 后台线程：持续读取 rust-analyzer 的 stdout，转发到统一消息出口；
        // 输出结束（进程崩溃/被停止）时退出线程并报告崩溃状态
        let events = self.events.clone();
        let crashed = self.crashed.clone();
        thread::spawn(move || {
            let mut buf_reader = BufReader::new(reader);
            loop {
                match read_one_lsp_message(&mut buf_reader) {
                    Some(msg) => {
                        if events.send(msg).is_err() {
                            break;
                        }
                    }
                    None => {
                        log::info!("{}", crate::ui::global().t("lsp_log_ra_output_ended"));
                        // 非主动停止视为异常退出：置崩溃标志供主循环自动重启
                        if !stop_flag.load(Ordering::SeqCst) {
                            crashed.store(true, Ordering::SeqCst);
                        }
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// 向 rust-analyzer 发送一条 JSON-RPC 消息
    pub fn send(&self, msg: &Value) -> anyhow::Result<()> {
        let text = serde_json::to_string(msg)?;
        let frame = format!("Content-Length: {}\r\n\r\n{}", text.len(), text);

        let mut writer = self
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("{}", crate::ui::global().t("lsp_err_write_lock")))?;
        let stdin = writer.as_mut().ok_or_else(|| {
            anyhow::anyhow!("{}", crate::ui::global().t("lsp_err_ra_not_running"))
        })?;
        stdin.write_all(frame.as_bytes())?;
        stdin.flush()?;

        log::debug!("-> rust-analyzer: {}", truncate(&text, 200));
        Ok(())
    }

    /// 获取消息接收通道的克隆（可在多线程中共享）
    pub fn message_channel(&self) -> crossbeam_channel::Receiver<Value> {
        self.receiver.clone()
    }

    /// 获取用于后台线程回发消息的发送器克隆
    ///
    /// 转发线程需要应答 rust-analyzer 主动发来的请求
    /// （如 workspace/diagnostic/refresh），因此需要独立于
    /// 主线程的发送能力。
    pub fn sender_clone(&self) -> Sender {
        Sender {
            writer: self.writer.clone(),
        }
    }

    /// 尝试非阻塞接收一条来自 rust-analyzer 的消息
    pub fn try_recv(&self) -> Option<Value> {
        self.receiver.try_recv().ok()
    }

    /// rust-analyzer 是否已异常退出（读取线程报告，等待自动重启）
    pub fn is_crashed(&self) -> bool {
        self.crashed.load(Ordering::SeqCst)
    }

    /// 清除崩溃标志（自动重启失败时调用，避免主循环忙循环）
    pub fn clear_crashed(&self) {
        self.crashed.store(false, Ordering::SeqCst);
    }

    /// 重启 rust-analyzer 子进程（崩溃后由代理主循环调用）
    ///
    /// 消息出口（receiver）不重建：转发线程持续消费；
    /// 重握手由调用方（代理主循环）完成。
    pub fn restart(&self) -> anyhow::Result<()> {
        self.stop();
        self.crashed.store(false, Ordering::SeqCst);
        self.spawn_child()
    }

    /// 停止 rust-analyzer 子进程（置当前进程的停止标志，EOF 不再报告崩溃）
    pub fn stop(&self) {
        if let Some(flag) = self.stop_flag.lock().ok().map(|guard| guard.clone()) {
            flag.store(true, Ordering::SeqCst);
        }
        if let Some(mut process) = self.child.lock().ok().and_then(|mut slot| slot.take()) {
            log::info!("{}", crate::ui::global().t("lsp_log_ra_stopping"));
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}

/// 独立的 rust-analyzer 消息发送器（供其他线程使用）
pub struct Sender {
    writer: Arc<Mutex<Option<std::process::ChildStdin>>>,
}

impl Sender {
    /// 向 rust-analyzer 发送一条 JSON-RPC 消息
    pub fn send(&self, msg: &Value) -> anyhow::Result<()> {
        let text = serde_json::to_string(msg)?;
        let frame = format!("Content-Length: {}\r\n\r\n{}", text.len(), text);

        let mut writer = self
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("{}", crate::ui::global().t("lsp_err_write_lock")))?;
        let stdin = writer.as_mut().ok_or_else(|| {
            anyhow::anyhow!("{}", crate::ui::global().t("lsp_err_ra_not_running"))
        })?;
        stdin.write_all(frame.as_bytes())?;
        stdin.flush()?;

        log::debug!("-> rust-analyzer: {}", truncate(&text, 200));
        Ok(())
    }
}

impl Drop for AnalyzerConnection {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 从 BufReader 中读取一条 LSP 消息（Content-Length 分帧）
fn read_one_lsp_message<R: BufRead>(reader: &mut R) -> Option<Value> {
    let mut header_line = String::new();
    let mut content_length: Option<usize> = None;

    loop {
        header_line.clear();
        match reader.read_line(&mut header_line) {
            Ok(0) => return None,
            Ok(_) => {}
            Err(e) => {
                log::error!(
                    "{}",
                    crate::ui::global().f("lsp_err_read_output", &[&e.to_string()])
                );
                return None;
            }
        }

        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break;
        }

        if let Some(len_str) = trimmed.strip_prefix("Content-Length:")
            && let Ok(n) = len_str.trim().parse::<usize>()
        {
            content_length = Some(n);
        }
    }

    let len = content_length?;

    let mut buffer = vec![0u8; len];
    if std::io::Read::read_exact(reader, &mut buffer).is_err() {
        log::error!("{}", crate::ui::global().t("lsp_err_read_body"));
        return None;
    }

    let text = String::from_utf8(buffer).ok()?;
    log::debug!("<- rust-analyzer: {}", truncate(&text, 200));

    serde_json::from_str(&text).ok()
}

/// 查找 rust-analyzer 可执行文件
///
/// 优先级：
/// 0. 内置工具链（~/.rz/toolchain/bin，`rzc install toolchain` 安装的 standalone 版）
/// 1. RUST_ANALYZER_PATH 环境变量
/// 2. rustup which --toolchain stable（真实路径，不受项目 rust-toolchain.toml 影响）
/// 3. PATH 扫描（跨平台，替代 Unix 专属 which；Windows 追加 PATHEXT 后缀）
/// 4. 常见安装位置（含 ~/.cargo/bin，Windows 回退 USERPROFILE）
fn find_rust_analyzer() -> anyhow::Result<PathBuf> {
    // 0. 用户显式指定的环境变量（最高优先级，覆盖内置与 PATH）
    if let Ok(path) = std::env::var("RUST_ANALYZER_PATH") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return Ok(p);
        }
    }

    // 1. 内置工具链（脱离 rustup 的 standalone 版本，版本自管）
    if let Some(p) = i18n_rust_engine::toolchain::find_toolchain_bin("rust-analyzer") {
        return Ok(p);
    }

    // rustup which 返回真实二进制路径（非 shim），避免被项目 rust-toolchain.toml
    // （如 1.85 无 rust-analyzer 组件）导致 Unknown binary 启动失败
    if let Ok(output) = Command::new("rustup")
        .args(["which", "--toolchain", "stable", "rust-analyzer"])
        .output()
        && output.status.success()
    {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !s.is_empty() {
            let p = PathBuf::from(&s);
            if p.exists() {
                return Ok(p);
            }
        }
    }

    // PATH 扫描（engine 工具链定位统一实现：含 Windows PATHEXT 探测）
    if let Some(p) = i18n_rust_engine::toolchain::find_toolchain_bin("rust-analyzer") {
        return Ok(p);
    }

    let candidates = [
        home_path("cargo/bin/rust-analyzer"),
        home_path(".cargo/bin/rust-analyzer"),
        PathBuf::from("/usr/local/bin/rust-analyzer"),
        PathBuf::from("/usr/bin/rust-analyzer"),
    ];
    for p in candidates {
        if p.exists() {
            return Ok(p);
        }
    }

    anyhow::bail!("{}", crate::ui::global().t("lsp_err_ra_not_found"))
}

fn home_path(relative: &str) -> PathBuf {
    // HOME（Unix）优先，USERPROFILE（Windows）回退，保证跨平台定位到用户主目录
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if home.is_empty() {
        PathBuf::from(relative)
    } else {
        PathBuf::from(home).join(relative)
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        // 按字符边界安全截断，避免切到多字节字符（中文）中间
        let mut boundary = max_len;
        while boundary > 0 && !s.is_char_boundary(boundary) {
            boundary -= 1;
        }
        format!("{}...", &s[..boundary])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// 合法 Content-Length 分帧消息可解析出 JSON
    #[test]
    fn test_read_one_lsp_message_valid() {
        let data = "Content-Length: 17\r\n\r\n{\"method\":\"test\"}";
        let mut cursor = Cursor::new(data);
        let msg = read_one_lsp_message(&mut cursor).expect("应解析出消息");
        assert_eq!(msg["method"], "test");
    }

    /// 消息头中缺少 Content-Length 时返回 None
    #[test]
    fn test_read_one_lsp_message_missing_content_length() {
        let data = "Content-Type: application/json\r\n\r\n{}";
        let mut cursor = Cursor::new(data);
        assert!(read_one_lsp_message(&mut cursor).is_none());
    }

    /// 消息体长度不足（提前 EOF）时返回 None
    #[test]
    fn test_read_one_lsp_message_truncated_body() {
        let data = "Content-Length: 100\r\n\r\n{\"method\":\"test\"}";
        let mut cursor = Cursor::new(data);
        assert!(read_one_lsp_message(&mut cursor).is_none());
    }

    /// 消息体为非 UTF-8 字节时返回 None
    #[test]
    fn test_read_one_lsp_message_invalid_utf8() {
        let data = b"Content-Length: 2\r\n\r\n\xFF\xFE";
        let mut cursor = Cursor::new(data.as_slice());
        assert!(read_one_lsp_message(&mut cursor).is_none());
    }

    /// 输入为 EOF 时返回 None
    #[test]
    fn test_read_one_lsp_message_eof() {
        let mut cursor = Cursor::new("");
        assert!(read_one_lsp_message(&mut cursor).is_none());
    }

    /// 短文本原样返回
    #[test]
    fn test_truncate_short_text_unchanged() {
        assert_eq!(truncate("abc", 10), "abc");
        assert_eq!(truncate("hello world", 5), "hello...");
    }

    /// 截断长度落在多字节字符中间时回退到字符边界
    #[test]
    fn test_truncate_char_boundary_safe() {
        assert_eq!(truncate("中文测试", 3), "中...");
        assert_eq!(truncate("中文测试", 4), "中...");
        assert_eq!(truncate("中文测试", 6), "中文...");
    }

    /// RUST_ANALYZER_PATH 环境变量优先返回
    #[test]
    fn test_find_rust_analyzer_env_path() {
        let tmp = tempfile::NamedTempFile::new().expect("创建临时文件失败");
        unsafe {
            std::env::set_var("RUST_ANALYZER_PATH", tmp.path());
        }
        let found = find_rust_analyzer().expect("应通过环境变量找到");
        assert_eq!(found, tmp.path());
        unsafe {
            std::env::remove_var("RUST_ANALYZER_PATH");
        }
    }

    /// PATH 扫描由 engine 工具链定位统一实现（见 crates/engine/src/toolchain.rs）
    #[test]
    fn test_engine_find_toolchain_bin_missing() {
        // 内置与 PATH 都不存在的二进制名应返回 None（不 panic）
        let name = format!("__ra_nonexistent__{}", std::process::id());
        assert!(i18n_rust_engine::toolchain::find_toolchain_bin(&name).is_none());
    }

    /// 构造空连接（未启动任何子进程）
    fn empty_connection() -> AnalyzerConnection {
        let (events, receiver) = crossbeam_channel::unbounded();
        AnalyzerConnection {
            child: Mutex::new(None),
            writer: Arc::new(Mutex::new(None)),
            events,
            receiver,
            crashed: Arc::new(AtomicBool::new(false)),
            stop_flag: Mutex::new(Arc::new(AtomicBool::new(false))),
        }
    }

    /// 等待崩溃标志置位（最多 2 秒）
    fn wait_crashed(conn: &AnalyzerConnection) -> bool {
        for _ in 0..40 {
            if conn.is_crashed() {
                return true;
            }
            thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }

    /// 子进程异常退出（EOF 且非主动停止）时置崩溃标志
    #[cfg(unix)]
    #[test]
    fn test_crashed_flag_on_abrupt_exit() {
        let conn = empty_connection();
        conn.spawn_command(Command::new("sh").arg("-c").arg("exit 0"))
            .expect("启动假进程失败");
        assert!(wait_crashed(&conn), "进程立即退出应触发崩溃标志");
        // 模拟重启：停止旧进程（其 EOF 不再报崩溃）+ 复位标志 + 启动新进程
        conn.stop();
        conn.clear_crashed();
        conn.spawn_command(Command::new("sh").arg("-c").arg("exit 1"))
            .expect("二次启动失败");
        assert!(!conn.is_crashed(), "重启后标志应复位");
        assert!(wait_crashed(&conn), "新进程退出应再次置位");
    }

    /// 主动 stop() 后的 EOF 不置崩溃标志
    #[cfg(unix)]
    #[test]
    fn test_no_crash_flag_on_graceful_stop() {
        let conn = empty_connection();
        conn.spawn_command(Command::new("sh").arg("-c").arg("sleep 5"))
            .expect("启动假进程失败");
        conn.stop();
        thread::sleep(std::time::Duration::from_millis(200));
        assert!(!conn.is_crashed(), "主动停止不应报告崩溃");
    }

    /// 未启动（重启窗口内）发送返回明确错误而非 panic
    #[test]
    fn test_send_fails_when_not_running() {
        let conn = empty_connection();
        let err = conn
            .send(&serde_json::json!({ "method": "test" }))
            .unwrap_err();
        assert!(
            err.to_string().contains("不可用")
                || err.to_string().contains("lsp_err_ra_not_running")
        );
    }
}
