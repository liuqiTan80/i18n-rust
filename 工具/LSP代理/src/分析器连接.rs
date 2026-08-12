//! 分析器连接模块
//!
//! 管理 rust-analyzer 子进程的生命周期：启动、消息收发、关闭。
//! 通过 stdin/stdout 以 LSP 协议（Content-Length 分帧）与之通信。

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::Value;

/// rust-analyzer 子进程管理器
pub struct 分析器连接 {
    /// 子进程句柄
    子进程: Option<Child>,
    /// 写入端（向 rust-analyzer 发送消息）
    写入端: Arc<Mutex<std::process::ChildStdin>>,
    /// 消息接收通道（crossbeam，可克隆）
    消息接收: crossbeam_channel::Receiver<Value>,
    /// 是否仍在运行
    运行标志: Arc<Mutex<bool>>,
}

impl 分析器连接 {
    /// 启动 rust-analyzer 子进程
    pub fn 启动() -> anyhow::Result<Self> {
        let ra路径 = 查找rust分析器()?;
        log::info!("启动 rust-analyzer: {:?}", ra路径);

        let mut 子进程 = Command::new(&ra路径)
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| anyhow::anyhow!("启动 rust-analyzer 失败: {}", e))?;

        let 写入端 = 子进程.stdin.take()
            .ok_or_else(|| anyhow::anyhow!("无法获取 rust-analyzer stdin"))?;
        let 读取端 = 子进程.stdout.take()
            .ok_or_else(|| anyhow::anyhow!("无法获取 rust-analyzer stdout"))?;

        let 写入端 = Arc::new(Mutex::new(写入端));
        let (发送端, 接收端) = crossbeam_channel::unbounded();
        let 运行标志 = Arc::new(Mutex::new(true));
        let 运行标志克隆 = 运行标志.clone();

        // 后台线程：持续读取 rust-analyzer 的 stdout
        thread::spawn(move || {
            let mut 缓冲读取 = BufReader::new(读取端);
            loop {
                match 读取一条LSP消息(&mut 缓冲读取) {
                    Some(消息) => {
                        if 发送端.send(消息).is_err() {
                            break;
                        }
                    }
                    None => {
                        log::info!("rust-analyzer 输出已结束");
                        break;
                    }
                }
            }
            if let Ok(mut 标志) = 运行标志克隆.lock() {
                *标志 = false;
            }
        });

        Ok(Self {
            子进程: Some(子进程),
            写入端,
            消息接收: 接收端,
            运行标志,
        })
    }

    /// 向 rust-analyzer 发送一条 JSON-RPC 消息
    pub fn 发送(&self, 消息: &Value) -> anyhow::Result<()> {
        let 文本 = serde_json::to_string(消息)?;
        let 帧 = format!("Content-Length: {}\r\n\r\n{}", 文本.len(), 文本);

        let mut 写入 = self.写入端.lock()
            .map_err(|_| anyhow::anyhow!("写入端锁获取失败"))?;
        写入.write_all(帧.as_bytes())?;
        写入.flush()?;

        log::debug!("-> rust-analyzer: {}", 截断(&文本, 200));
        Ok(())
    }

    /// 获取消息接收通道的克隆（可在多线程中共享）
    pub fn 消息通道(&self) -> crossbeam_channel::Receiver<Value> {
        self.消息接收.clone()
    }

    /// 尝试非阻塞接收一条来自 rust-analyzer 的消息
    pub fn 尝试接收(&self) -> Option<Value> {
        self.消息接收.try_recv().ok()
    }

    /// 阻塞接收一条来自 rust-analyzer 的消息
    pub fn 阻塞接收(&self) -> Option<Value> {
        self.消息接收.recv().ok()
    }

    /// 检查 rust-analyzer 是否仍在运行
    pub fn 是否运行中(&self) -> bool {
        self.运行标志.lock().map(|r| *r).unwrap_or(false)
    }

    /// 停止 rust-analyzer 子进程
    pub fn 停止(&mut self) {
        if let Some(mut 进程) = self.子进程.take() {
            log::info!("正在停止 rust-analyzer...");
            let _ = 进程.kill();
            let _ = 进程.wait();
        }
    }
}

impl Drop for 分析器连接 {
    fn drop(&mut self) {
        self.停止();
    }
}

/// 从 BufReader 中读取一条 LSP 消息（Content-Length 分帧）
fn 读取一条LSP消息<R: BufRead>(读取器: &mut R) -> Option<Value> {
    let mut 头部行 = String::new();
    let mut 内容长度: Option<usize> = None;

    loop {
        头部行.clear();
        match 读取器.read_line(&mut 头部行) {
            Ok(0) => return None,
            Ok(_) => {}
            Err(e) => {
                log::error!("读取 rust-analyzer 输出失败: {}", e);
                return None;
            }
        }

        let trimmed = 头部行.trim();
        if trimmed.is_empty() {
            break;
        }

        if let Some(长度_str) = trimmed.strip_prefix("Content-Length:") {
            if let Ok(n) = 长度_str.trim().parse::<usize>() {
                内容长度 = Some(n);
            }
        }
    }

    let 长度 = 内容长度?;

    let mut 缓冲 = vec![0u8; 长度];
    if std::io::Read::read_exact(读取器, &mut 缓冲).is_err() {
        log::error!("读取 rust-analyzer 消息体失败");
        return None;
    }

    let 文本 = String::from_utf8(缓冲).ok()?;
    log::debug!("<- rust-analyzer: {}", 截断(&文本, 200));

    serde_json::from_str(&文本).ok()
}

/// 查找 rust-analyzer 可执行文件
fn 查找rust分析器() -> anyhow::Result<PathBuf> {
    if let Ok(路径) = std::env::var("RUST_ANALYZER_PATH") {
        let p = PathBuf::from(&路径);
        if p.exists() {
            return Ok(p);
        }
    }

    if let Ok(输出) = Command::new("which").arg("rust-analyzer").output() {
        if 输出.status.success() {
            let s = String::from_utf8_lossy(&输出.stdout).trim().to_string();
            if !s.is_empty() {
                return Ok(PathBuf::from(s));
            }
        }
    }

    let 候选 = [
        home路径("cargo/bin/rust-analyzer"),
        PathBuf::from("/usr/local/bin/rust-analyzer"),
        PathBuf::from("/usr/bin/rust-analyzer"),
    ];
    for p in 候选 {
        if p.exists() {
            return Ok(p);
        }
    }

    anyhow::bail!("未找到 rust-analyzer。请安装或设置 RUST_ANALYZER_PATH 环境变量")
}

fn home路径(相对: &str) -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(相对)
    } else {
        PathBuf::from(相对)
    }
}

fn 截断(s: &str, 最大长度: usize) -> String {
    if s.len() <= 最大长度 {
        s.to_string()
    } else {
        format!("{}...", &s[..最大长度])
    }
}
