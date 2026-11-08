//! 分析器连接模块
//!
//! 管理 rust-analyzer 子进程的生命周期：启动、消息收发、关闭。
//! 通过标准输入/标准输出以语言服务协议（内容长度字段分帧）与之通信。

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::Value;

/// rust-analyzer 子进程管理器
///
/// 支持崩溃自动重启：消息出口（接收端）与崩溃标志不随进程重启而变化，
/// 转发线程无需重建；重启仅替换进程句柄与写入端，并由代理主循环
/// 重新执行初始化、初始化完成、工作区与文档同步握手。
pub struct 分析器连接 {
    /// 子进程句柄（重启/停止时替换）
    子进程句柄: Mutex<Option<Child>>,
    /// 写入端（向 rust-analyzer 发送消息；锁内发送，重启时替换）
    写入端: Arc<Mutex<Option<ChildStdin>>>,
    /// 统一消息出口（不随重启变化，转发线程持续读取）
    消息出口: crossbeam_channel::Sender<Value>,
    /// 消息接收通道（消息出口的接收端，可克隆）
    消息接收器: crossbeam_channel::Receiver<Value>,
    /// 读取线程检测到输出结束（进程崩溃/异常退出）置位，
    /// 供代理主循环检测并触发自动重启
    已崩溃: Arc<AtomicBool>,
    /// 当前进程的主动停止抑制标志：每次生成子进程时新建并替换（停止时置位当前
    /// 标志，读取线程持各自标志），避免重启后旧进程输出结束与新标志竞态
    停止标志: Mutex<Arc<AtomicBool>>,
    /// 启动阶段失败原因（rust-analyzer 未安装/不可执行）
    ///
    /// 启动失败不能直接让进程退出：代理必须先完成语言服务协议握手才能把原因告知
    /// 客户端，否则编辑器只显示“服务端初始化失败”，用户无从
    /// 得知到底是缺 rust-analyzer 还是语言包坏了。
    启动错误: Mutex<Option<String>>,
}

impl 分析器连接 {
    /// 启动 rust-analyzer 子进程
    ///
    /// 启动失败不返回错误（见 [`自身::启动失败原因`]）：失败原因先存入
    /// `启动错误`，由代理在握手后转给客户端。
    pub fn 启动() -> Self {
        let (消息出口, 消息接收器) = crossbeam_channel::unbounded();
        let 连接实例 = Self {
            子进程句柄: Mutex::new(None),
            写入端: Arc::new(Mutex::new(None)),
            消息出口,
            消息接收器,
            已崩溃: Arc::new(AtomicBool::new(false)),
            停止标志: Mutex::new(Arc::new(AtomicBool::new(false))),
            启动错误: Mutex::new(None),
        };
        if let Err(错误值) = 连接实例.生成子进程() {
            log::error!("{错误值}");
            if let Ok(mut 槽位) = 连接实例.启动错误.lock() {
                *槽位 = Some(错误值.to_string());
            }
        }
        连接实例
    }

    /// 启动阶段是否失败（返回失败原因）
    ///
    /// 非 `无` 时子进程从未就绪，代理应立即把原因报给客户端并退出，
    /// 而不是在主循环里继续转发消息。
    pub fn 启动失败原因(&self) -> Option<String> {
        self.启动错误.lock().ok().and_then(|槽位| 槽位.clone())
    }

    /// 启动（或崩溃后重新启动）rust-analyzer 子进程并挂接消息管道
    fn 生成子进程(&self) -> anyhow::Result<()> {
        let 分析器路径 = 查找分析器()?;
        log::info!(
            "{}",
            crate::界面消息::全局界面()
                .取带参消息("lsp_log_ra_starting", &[&分析器路径.display().to_string()])
        );
        let mut 命令实例 = Command::new(&分析器路径);
        // 新版 rust-analyzer 已移除 `--stdio` 参数（标准输入输出为默认模式）
        self.生成命令(&mut 命令实例)
    }

    /// 用指定命令启动子进程（测试配置下注入假进程用）
    fn 生成命令(&self, 命令实例: &mut Command) -> anyhow::Result<()> {
        let mut 子进程实例 = 命令实例
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|错误值| {
                anyhow::anyhow!(
                    "{}",
                    crate::界面消息::全局界面()
                        .取带参消息("lsp_err_ra_start", &[&错误值.to_string()])
                )
            })?;

        let 写入端实例 = 子进程实例.stdin.take().ok_or_else(|| {
            anyhow::anyhow!("{}", crate::界面消息::全局界面().取消息("lsp_err_ra_stdin"))
        })?;
        let 读取端实例 = 子进程实例.stdout.take().ok_or_else(|| {
            anyhow::anyhow!(
                "{}",
                crate::界面消息::全局界面().取消息("lsp_err_ra_stdout")
            )
        })?;

        // 新进程的停止抑制标志：读取线程持各自标志，停止时只置位当前标志
        let 停止标志 = Arc::new(AtomicBool::new(false));
        *self.子进程句柄.lock().map_err(|_| {
            anyhow::anyhow!(
                "{}",
                crate::界面消息::全局界面().取消息("lsp_err_write_lock")
            )
        })? = Some(子进程实例);
        *self.写入端.lock().map_err(|_| {
            anyhow::anyhow!(
                "{}",
                crate::界面消息::全局界面().取消息("lsp_err_write_lock")
            )
        })? = Some(写入端实例);
        *self.停止标志.lock().map_err(|_| {
            anyhow::anyhow!(
                "{}",
                crate::界面消息::全局界面().取消息("lsp_err_write_lock")
            )
        })? = 停止标志.clone();

        // 后台线程：持续读取 rust-analyzer 的标准输出，转发到统一消息出口；
        // 输出结束（进程崩溃/被停止）时退出线程并报告崩溃状态
        let 消息出口 = self.消息出口.clone();
        let 已崩溃 = self.已崩溃.clone();
        thread::spawn(move || {
            let mut 缓冲读取器实例 = BufReader::new(读取端实例);
            loop {
                if let Some(消息) = 读取一条协议消息(&mut 缓冲读取器实例) {
                    if 消息出口.send(消息).is_err() {
                        break;
                    }
                } else {
                    log::info!(
                        "{}",
                        crate::界面消息::全局界面().取消息("lsp_log_ra_output_ended")
                    );
                    // 非主动停止视为异常退出：置崩溃标志供主循环自动重启
                    if !停止标志.load(Ordering::SeqCst) {
                        已崩溃.store(true, Ordering::SeqCst);
                    }
                    break;
                }
            }
        });

        Ok(())
    }

    /// 向 rust-analyzer 发送一条 JSON-RPC 消息
    pub fn 发送消息(&self, 消息: &Value) -> anyhow::Result<()> {
        let 文本串 = serde_json::to_string(消息)?;
        let 帧文本 = format!("Content-Length: {}\r\n\r\n{}", 文本串.len(), 文本串);

        let mut 写入端实例 = self.写入端.lock().map_err(|_| {
            anyhow::anyhow!(
                "{}",
                crate::界面消息::全局界面().取消息("lsp_err_write_lock")
            )
        })?;
        let 标准输入实例 = 写入端实例.as_mut().ok_or_else(|| {
            anyhow::anyhow!(
                "{}",
                crate::界面消息::全局界面().取消息("lsp_err_ra_not_running")
            )
        })?;
        标准输入实例.write_all(帧文本.as_bytes())?;
        标准输入实例.flush()?;

        log::debug!("-> rust-analyzer: {}", 截短(&文本串, 200));
        Ok(())
    }

    /// 获取消息接收通道的克隆（可在多线程中共享）
    pub fn 消息通道(&self) -> crossbeam_channel::Receiver<Value> {
        self.消息接收器.clone()
    }

    /// 获取用于后台线程回发消息的发送端副本
    ///
    /// 转发线程需要应答 rust-analyzer 主动发来的请求
    /// （如 `workspace/诊断/refresh`），因此需要独立于
    /// 主线程的发送能力。
    pub fn 取发送端副本(&self) -> 发送端 {
        发送端 {
            写入端: self.写入端.clone(),
        }
    }

    /// 尝试非阻塞接收一条来自 rust-analyzer 的消息
    pub fn 尝试接收(&self) -> Option<Value> {
        self.消息接收器.try_recv().ok()
    }

    /// rust-analyzer 是否已异常退出（读取线程报告，等待自动重启）
    pub fn 是否崩溃(&self) -> bool {
        self.已崩溃.load(Ordering::SeqCst)
    }

    /// 清除崩溃标志（自动重启失败时调用，避免主循环忙循环）
    pub fn 清除崩溃(&self) {
        self.已崩溃.store(false, Ordering::SeqCst);
    }

    /// 重启 rust-analyzer 子进程（崩溃后由代理主循环调用）
    ///
    /// 消息出口（接收端）不重建：转发线程持续消费；
    /// 重握手由调用方（代理主循环）完成。
    pub fn 重启(&self) -> anyhow::Result<()> {
        self.停止();
        self.已崩溃.store(false, Ordering::SeqCst);
        self.生成子进程()
    }

    /// 停止 rust-analyzer 子进程（置当前进程的停止标志，输出结束不再报告崩溃）
    pub fn 停止(&self) {
        if let Some(标志) = self.停止标志.lock().ok().map(|守卫| 守卫.clone()) {
            标志.store(true, Ordering::SeqCst);
        }
        if let Some(mut 进程实例) = self.子进程句柄.lock().ok().and_then(|mut 槽位| 槽位.take())
        {
            log::info!(
                "{}",
                crate::界面消息::全局界面().取消息("lsp_log_ra_stopping")
            );
            let _ = 进程实例.kill();
            let _ = 进程实例.wait();
        }
    }
}

/// 独立的 rust-analyzer 消息发送端（供其他线程使用）
pub struct 发送端 {
    写入端: Arc<Mutex<Option<ChildStdin>>>,
}

impl 发送端 {
    /// 向 rust-analyzer 发送一条 JSON-RPC 消息
    pub fn 发送消息(&self, 消息: &Value) -> anyhow::Result<()> {
        let 文本串 = serde_json::to_string(消息)?;
        let 帧文本 = format!("Content-Length: {}\r\n\r\n{}", 文本串.len(), 文本串);

        let mut 写入端实例 = self.写入端.lock().map_err(|_| {
            anyhow::anyhow!(
                "{}",
                crate::界面消息::全局界面().取消息("lsp_err_write_lock")
            )
        })?;
        let 标准输入实例 = 写入端实例.as_mut().ok_or_else(|| {
            anyhow::anyhow!(
                "{}",
                crate::界面消息::全局界面().取消息("lsp_err_ra_not_running")
            )
        })?;
        标准输入实例.write_all(帧文本.as_bytes())?;
        标准输入实例.flush()?;

        log::debug!("-> rust-analyzer: {}", 截短(&文本串, 200));
        Ok(())
    }
}

impl Drop for 分析器连接 {
    fn drop(&mut self) {
        self.停止();
    }
}

#[cfg(test)]
impl 发送端 {
    /// 测试用空发送端：无 rust-analyzer 子进程，发送恒返回"未运行"错误。
    ///
    /// 供服务端消息分发单测构造参数——这些用例只验证
    /// 响应映射/通知路由，不需要真实 rust-analyzer 回环（回环由端到端测试覆盖）。
    pub(crate) fn 测试用空端() -> Self {
        Self {
            写入端: Arc::new(Mutex::new(None)),
        }
    }
}

/// 单条协议消息正文的硬上限（16 MiB）
///
/// 协议的内容长度头由对端声明，按声明值直接分配缓冲会让一条畸形帧
/// （如声明长度接近 2^64）触发 OOM/abort。
/// 正常 LSP 消息远小于此值，超限视为协议错误并终止读取。
const 最大协议正文字节数: usize = 16 * 1024 * 1024;

/// 从缓冲读取器中读取一条语言服务协议消息（内容长度字段分帧）
fn 读取一条协议消息<读源: BufRead>(读取器: &mut 读源) -> Option<Value> {
    let mut 消息头行 = String::new();
    let mut 正文字节数: Option<usize> = None;

    loop {
        消息头行.clear();
        match 读取器.read_line(&mut 消息头行) {
            Ok(0) => return None,
            Ok(_) => {}
            Err(错误值) => {
                log::error!(
                    "{}",
                    crate::界面消息::全局界面()
                        .取带参消息("lsp_err_read_output", &[&错误值.to_string()])
                );
                return None;
            }
        }

        let 裁剪后行 = 消息头行.trim();
        if 裁剪后行.is_empty() {
            break;
        }

        if let Some(长度文本) = 裁剪后行.strip_prefix("Content-Length:")
            && let Ok(字节数) = 长度文本.trim().parse::<usize>()
        {
            正文字节数 = Some(字节数);
        }
    }

    let 正文字节数 = 正文字节数?;

    if 正文字节数 > 最大协议正文字节数 {
        log::error!(
            "rust-analyzer 声明的消息体 {} 字节超过上限 {}，拒绝读取",
            正文字节数,
            最大协议正文字节数
        );
        return None;
    }

    let mut 字节缓冲 = vec![0u8; 正文字节数];
    if std::io::Read::read_exact(读取器, &mut 字节缓冲).is_err() {
        log::error!(
            "{}",
            crate::界面消息::全局界面().取消息("lsp_err_read_body")
        );
        return None;
    }

    let 文本串 = String::from_utf8(字节缓冲).ok()?;
    log::debug!("<- rust-analyzer: {}", 截短(&文本串, 200));

    serde_json::from_str(&文本串).ok()
}

/// 查找 rust-analyzer 可执行文件
///
/// 优先级：
/// 0. 内置工具链（`~/.rz/toolchain/bin`，`rzc install toolchain` 安装的独立版）
/// 1. `RUST_ANALYZER_PATH` 环境变量
/// 2. `rustup which --toolchain stable`（真实路径，不受项目 `rust-toolchain.toml` 影响）
/// 3. `PATH` 扫描（跨平台，替代 Unix 专属的查找命令；Windows 追加 `PATHEXT` 后缀）
/// 4. 常见安装位置（含 `~/.cargo/bin`，Windows 回退 `USERPROFILE`）
fn 查找分析器() -> anyhow::Result<PathBuf> {
    // 0. 用户显式指定的环境变量（最高优先级，覆盖内置与 `PATH`）
    if let Ok(路径文本) = std::env::var("RUST_ANALYZER_PATH") {
        let 候选路径 = PathBuf::from(&路径文本);
        if 候选路径.exists() {
            return Ok(候选路径);
        }
    }

    // 1. 内置工具链（脱离 rustup 的独立版本，版本自管）
    if let Some(候选路径) = i18n_rust_engine::工具链::查找工具链程序("rust-analyzer")
    {
        return Ok(候选路径);
    }

    // `rustup which` 返回真实二进制路径（非包装脚本），避免被项目 `rust-toolchain.toml`
    // （如 1.85 无 rust-analyzer 组件）导致 `Unknown binary` 启动失败
    if let Ok(进程输出) = Command::new("rustup")
        .args(["which", "--toolchain", "stable", "rust-analyzer"])
        .output()
        && 进程输出.status.success()
    {
        let 路径文本 = String::from_utf8_lossy(&进程输出.stdout).trim().to_string();
        if !路径文本.is_empty() {
            let 候选路径 = PathBuf::from(&路径文本);
            if 候选路径.exists() {
                return Ok(候选路径);
            }
        }
    }

    // `PATH` 扫描（引擎工具链定位统一实现：含 Windows `PATHEXT` 探测）
    if let Some(候选路径) = i18n_rust_engine::工具链::查找工具链程序("rust-analyzer")
    {
        return Ok(候选路径);
    }

    let 候选路径表 = [
        主目录路径("cargo/bin/rust-analyzer"),
        主目录路径(".cargo/bin/rust-analyzer"),
        PathBuf::from("/usr/local/bin/rust-analyzer"),
        PathBuf::from("/usr/bin/rust-analyzer"),
    ];
    for 候选路径 in 候选路径表 {
        if 候选路径.exists() {
            return Ok(候选路径);
        }
    }

    anyhow::bail!(
        "{}",
        crate::界面消息::全局界面().取消息("lsp_err_ra_not_found")
    )
}

fn 主目录路径(相对路径: &str) -> PathBuf {
    // `HOME`（Unix）优先，`USERPROFILE`（Windows）回退，保证跨平台定位到用户主目录
    let 主目录值 = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if 主目录值.is_empty() {
        PathBuf::from(相对路径)
    } else {
        PathBuf::from(主目录值).join(相对路径)
    }
}

fn 截短(文本串: &str, 最大长度: usize) -> String {
    if 文本串.len() <= 最大长度 {
        文本串.to_string()
    } else {
        // 按字符边界安全截断，避免切到多字节字符（中文）中间
        let mut 边界位置 = 最大长度;
        while 边界位置 > 0 && !文本串.is_char_boundary(边界位置) {
            边界位置 -= 1;
        }
        format!("{}...", &文本串[..边界位置])
    }
}

#[cfg(test)]
mod 母语测试 {
    use super::*;
    use std::io::Cursor;
    use tempfile::NamedTempFile;

    /// 合法内容长度分帧消息可解析出 JSON
    #[test]
    fn 读取合法消息成功() {
        let 数据 = "Content-Length: 17\r\n\r\n{\"method\":\"test\"}";
        let mut 游标实例 = Cursor::new(数据);
        let 消息值 = 读取一条协议消息(&mut 游标实例).expect("应解析出消息");
        assert_eq!(消息值["method"], "test");
    }

    /// 消息头中缺少内容长度字段时返回 `无`
    #[test]
    fn 缺失长度头返回无() {
        let 数据 = "Content-Type: application/json\r\n\r\n{}";
        let mut 游标实例 = Cursor::new(数据);
        assert!(读取一条协议消息(&mut 游标实例).is_none());
    }

    /// 消息体长度不足（提前结束输入）时返回 `无`
    #[test]
    fn 消息体不足返回无() {
        let 数据 = "Content-Length: 100\r\n\r\n{\"method\":\"test\"}";
        let mut 游标实例 = Cursor::new(数据);
        assert!(读取一条协议消息(&mut 游标实例).is_none());
    }

    /// 消息体为非 UTF-8 字节时返回 `无`
    #[test]
    fn 非法字节返回无() {
        let 数据 = b"Content-Length: 2\r\n\r\n\xFF\xFE";
        let mut 游标实例 = Cursor::new(数据);
        assert!(读取一条协议消息(&mut 游标实例).is_none());
    }

    /// 输入为空（立即结束）时返回 `无`
    #[test]
    fn 输入结束返回无() {
        let mut 游标实例 = Cursor::new("");
        assert!(读取一条协议消息(&mut 游标实例).is_none());
    }

    /// 短文本原样返回
    #[test]
    fn 短文本不截短() {
        assert_eq!(截短("abc", 10), "abc");
        assert_eq!(截短("hello world", 5), "hello...");
    }

    /// 截断长度落在多字节字符中间时回退到字符边界
    #[test]
    fn 截短落在字符边界() {
        assert_eq!(截短("中文测试", 3), "中...");
        assert_eq!(截短("中文测试", 4), "中...");
        assert_eq!(截短("中文测试", 6), "中文...");
    }

    /// `RUST_ANALYZER_PATH` 环境变量优先返回
    #[test]
    fn 分析器路径环境变量优先() {
        let 临时文件实例 = NamedTempFile::new().expect("创建临时文件失败");
        unsafe {
            std::env::set_var("RUST_ANALYZER_PATH", 临时文件实例.path());
        }
        let 找到路径 = 查找分析器().expect("应通过环境变量找到");
        assert_eq!(找到路径, 临时文件实例.path());
        unsafe {
            std::env::remove_var("RUST_ANALYZER_PATH");
        }
    }

    /// `PATH` 扫描由引擎工具链定位统一实现
    #[test]
    fn 工具链查找不存在程序返回无() {
        // 内置与 `PATH` 都不存在的二进制名应返回 `无`（不崩溃）
        let 名称 = format!("__ra_nonexistent__{}", std::process::id());
        assert!(i18n_rust_engine::工具链::查找工具链程序(&名称).is_none());
    }

    /// 构造空连接（未启动任何子进程）
    fn 空白连接() -> 分析器连接 {
        let (消息出口, 消息接收器) = crossbeam_channel::unbounded();
        分析器连接 {
            子进程句柄: Mutex::new(None),
            写入端: Arc::new(Mutex::new(None)),
            消息出口,
            消息接收器,
            已崩溃: Arc::new(AtomicBool::new(false)),
            停止标志: Mutex::new(Arc::new(AtomicBool::new(false))),
            启动错误: Mutex::new(None),
        }
    }

    /// 等待崩溃标志置位（最多 2 秒）
    ///
    /// 仅 `Unix系统` 配置的崩溃测试使用（靠 `sh -c` 起假进程），
    /// 不加门控会在 Windows 上被判为死代码。
    #[cfg(unix)]
    fn 等待崩溃(连接实例: &分析器连接) -> bool {
        for _ in 0..40 {
            if 连接实例.是否崩溃() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }

    /// 子进程异常退出（输出结束且非主动停止）时置崩溃标志
    #[cfg(unix)]
    #[test]
    fn 异常退出置崩溃标志() {
        let 连接实例 = 空白连接();
        连接实例
            .生成命令(Command::new("sh").arg("-c").arg("exit 0"))
            .expect("启动假进程失败");
        assert!(等待崩溃(&连接实例), "进程立即退出应触发崩溃标志");
        // 模拟重启：停止旧进程（其输出结束不再报崩溃）+ 复位标志 + 启动新进程
        连接实例.停止();
        连接实例.清除崩溃();
        连接实例
            .生成命令(Command::new("sh").arg("-c").arg("exit 1"))
            .expect("二次启动失败");
        assert!(!连接实例.是否崩溃(), "重启后标志应复位");
        assert!(等待崩溃(&连接实例), "新进程退出应再次置位");
    }

    /// 主动停止后的输出结束不置崩溃标志
    #[cfg(unix)]
    #[test]
    fn 主动停止不置崩溃标志() {
        let 连接实例 = 空白连接();
        连接实例
            .生成命令(Command::new("sh").arg("-c").arg("sleep 5"))
            .expect("启动假进程失败");
        连接实例.停止();
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(!连接实例.是否崩溃(), "主动停止不应报告崩溃");
    }

    /// 未启动（重启窗口内）发送返回明确错误而非崩溃
    #[test]
    fn 未运行时发送失败() {
        let 连接实例 = 空白连接();
        let 错误值 = 连接实例
            .发送消息(&serde_json::from_str(r#"{"method":"test"}"#).unwrap())
            .unwrap_err();
        assert!(
            错误值.to_string().contains("不可用")
                || 错误值.to_string().contains("lsp_err_ra_not_running")
        );
    }
}
