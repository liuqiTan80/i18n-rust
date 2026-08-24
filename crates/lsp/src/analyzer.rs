//! 分析器连接模块
//!
//! 管理 rust-analyzer 子进程的生命周期：启动、消息收发、关闭。
//! 通过 stdin/stdout 以 LSP 协议（Content-Length 分帧）与之通信。

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::Value;

/// rust-analyzer 子进程管理器
pub struct AnalyzerConnection {
    /// 子进程句柄
    child: Option<Child>,
    /// 写入端（向 rust-analyzer 发送消息）
    writer: Arc<Mutex<std::process::ChildStdin>>,
    /// 消息接收通道（crossbeam，可克隆）
    receiver: crossbeam_channel::Receiver<Value>,
}

impl AnalyzerConnection {
    /// 启动 rust-analyzer 子进程
    pub fn start() -> anyhow::Result<Self> {
        let ra_path = find_rust_analyzer()?;
        log::info!(
            "{}",
            crate::ui::global().f("lsp_log_ra_starting", &[&ra_path.display().to_string()])
        );

        let mut child = Command::new(&ra_path)
            // 新版 rust-analyzer 已移除 --stdio 参数（stdio 为默认模式）
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

        let writer = Arc::new(Mutex::new(writer));
        let (sender, recv) = crossbeam_channel::unbounded();

        // 后台线程：持续读取 rust-analyzer 的 stdout
        thread::spawn(move || {
            let mut buf_reader = BufReader::new(reader);
            loop {
                match read_one_lsp_message(&mut buf_reader) {
                    Some(msg) => {
                        if sender.send(msg).is_err() {
                            break;
                        }
                    }
                    None => {
                        log::info!("{}", crate::ui::global().t("lsp_log_ra_output_ended"));
                        break;
                    }
                }
            }
        });

        Ok(Self {
            child: Some(child),
            writer,
            receiver: recv,
        })
    }

    /// 向 rust-analyzer 发送一条 JSON-RPC 消息
    pub fn send(&self, msg: &Value) -> anyhow::Result<()> {
        let text = serde_json::to_string(msg)?;
        let frame = format!("Content-Length: {}\r\n\r\n{}", text.len(), text);

        let mut writer = self
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("{}", crate::ui::global().t("lsp_err_write_lock")))?;
        writer.write_all(frame.as_bytes())?;
        writer.flush()?;

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

    /// 停止 rust-analyzer 子进程
    pub fn stop(&mut self) {
        if let Some(mut process) = self.child.take() {
            log::info!("{}", crate::ui::global().t("lsp_log_ra_stopping"));
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}

/// 独立的 rust-analyzer 消息发送器（供其他线程使用）
pub struct Sender {
    writer: Arc<Mutex<std::process::ChildStdin>>,
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
        writer.write_all(frame.as_bytes())?;
        writer.flush()?;

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
    // 0. 内置工具链（脱离 rustup 的 standalone 版本，版本自管）
    if let Some(p) = i18n_rust_engine::toolchain::find_toolchain_bin("rust-analyzer") {
        return Ok(p);
    }

    if let Ok(path) = std::env::var("RUST_ANALYZER_PATH") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return Ok(p);
        }
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

    // PATH 扫描：Windows 无 which 命令，直接按目录枚举（支持 .exe 后缀）
    if let Some(p) = find_in_path("rust-analyzer") {
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

/// 在 PATH 环境变量中查找可执行文件（跨平台）
///
/// Windows 下按 PATHEXT 依次追加后缀（.exe 等）探测；
/// 名称自带扩展名（如 xxx.exe）时不重复追加。
fn find_in_path(name: &str) -> Option<PathBuf> {
    let path_env = std::env::var("PATH").ok()?;
    let separator = if cfg!(windows) { ';' } else { ':' };
    let mut suffixes: Vec<String> = vec![String::new()];
    if cfg!(windows) {
        if let Ok(pathext) = std::env::var("PATHEXT") {
            suffixes = pathext
                .split(';')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
        }
        if !suffixes.iter().any(|s| s.eq_ignore_ascii_case(".exe")) {
            suffixes.push(".exe".to_string());
        }
    }
    for dir in path_env.split(separator) {
        if dir.is_empty() {
            continue;
        }
        for suffix in &suffixes {
            let candidate = if !suffix.is_empty() && Path::new(name).extension().is_none() {
                Path::new(dir).join(format!("{name}{suffix}"))
            } else {
                Path::new(dir).join(name)
            };
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
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

    /// PATH 扫描：临时目录注入 PATH 后应找到带平台后缀的二进制
    #[test]
    fn test_find_in_path() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let exe_name = if cfg!(windows) {
            "mytool.exe"
        } else {
            "mytool"
        };
        let bin = tmp.path().join(exe_name);
        std::fs::write(&bin, "x").expect("写入临时文件失败");
        let separator = if cfg!(windows) { ';' } else { ':' };
        unsafe {
            let old = std::env::var("PATH").unwrap_or_default();
            std::env::set_var("PATH", format!("{}{separator}{old}", tmp.path().display()));
            let found = find_in_path("mytool");
            std::env::set_var("PATH", old);
            // Windows 文件系统不区分大小写，PATHEXT 可能为大写（如 .EXE），仅比较文件名忽略大小写
            let found = found.expect("PATH 扫描应找到 mytool");
            assert!(
                found
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(exe_name),
                "PATH 扫描应找到 {bin:?}，实际 {found:?}"
            );
        }
    }
}
