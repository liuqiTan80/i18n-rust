//! CLI 侧诊断翻译子系统：cargo/rustc 编译诊断的教学化翻译管线
//!
//! 与 LSP 侧 `response_map/diag_text.rs` 是姊妹职责：从 rustc JSON 诊断
//! 解析 → 教学化翻译（错误消息 + 类型/模块路径中文化）→ 位置回译
//!（英文产物坐标 → 母语源码坐标，含子模块与 `#[path]` 注解行换算）。
//!
//! 供运行与检查子命令共用：单文件项目走 [`运行直接编译`] /
//! [`检查直接编译`] 直调 rustc 快速路径（绕开 cargo 索引开销），
//! 多文件或有依赖的项目回退 cargo（[`翻译进度行`] 翻译进度行、
//! [`翻译编译诊断`] 翻译 JSON 诊断）。
//!
//! 模块边界：本模块只消费转译结果（源码 + 列映射 + 行映射 + 项目上下文），
//! 不负责转译编排（见 `main.rs` 的 `transpile_project_files()` 等）。

use crate::{
    临时守卫, 内置语言, 扩展名转语言代码, 界面, 解析rustc, 语言包根目录, 语言管理
};
use i18n_rust_engine::映射管理器::映射管理器;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 判断是否可单文件直调 rustc：源码目录下仅一个方言文件且 Cargo.toml 无依赖
///
/// 教学单文件项目（仅 main.zh）直调 rustc 绕开 cargo：无索引网络开销、
/// 无需 Cargo.lock 预生成，编译诊断格式与 cargo 完全一致；
/// 多文件（模块声明引用）或有依赖的项目回退 cargo 流程。
pub(crate) fn 可用直接编译(项目根: &Path, 源文件: &Path) -> bool {
    // 动态方言扩展名（内置 + 用户安装），避免硬编码列表与
    // `rzc lang install` 安装的新语言包脱节（新增语言走不了快速路径）
    let 扩展名序列 = 语言管理::全部可用扩展名();
    let 是方言文件 = |名称: &str| {
        扩展名序列
            .iter()
            .any(|单个| 名称.ends_with(&format!(".{单个}")))
    };
    // 方言文件计数：源码目录全层级递归（多层模块布局）+ 项目根顶层
    //（教学项目 src/main.zh 为主，项目根也可能放 main.zh）；
    // 超过 1 个视为多文件项目
    let mut 方言计数 = crate::收集方言文件(&项目根.join("src"), &扩展名序列).len();
    if let Ok(目录项集) = fs::read_dir(项目根) {
        for 目录项 in 目录项集.flatten() {
            if 目录项.file_type().is_ok_and(|类型值| 类型值.is_file()) {
                let 名称 = 目录项.file_name().to_string_lossy().to_string();
                if 是方言文件(&名称) {
                    方言计数 += 1;
                }
            }
        }
    }
    if 方言计数 != 1 {
        return false;
    }
    // 入口文件必须是 src/main.<方言扩展名>（聚合 main.rs 已写入）
    let 是主入口 = 源文件
        .file_name()
        .and_then(|片段| 片段.to_str())
        .is_some_and(|名称| {
            let (主名, 后缀) = 名称.rsplit_once('.').unwrap_or((名称, ""));
            主名 == "main" && 扩展名序列.iter().any(|单个| 单个 == 后缀)
        });
    if !是主入口 {
        return false;
    }
    // Cargo.toml 的依赖节非空（有第三方依赖）时回退 cargo
    let 清单文件 = 项目根.join("Cargo.toml");
    if let Ok(清单内容) = fs::read_to_string(&清单文件)
        && let Some(依赖段) = 清单内容.split("[dependencies]").nth(1)
    {
        // 依赖行形如 `rand = "0.8"`；注释/空行/子表头不算依赖
        let 有依赖 = 依赖段.lines().any(|原始行| {
            let 裁剪后 = 原始行.trim();
            !裁剪后.is_empty()
                && !裁剪后.starts_with('#')
                && !裁剪后.starts_with('[')
                && 裁剪后.contains('=')
        });
        if 有依赖 {
            return false;
        }
    }
    true
}

/// 单文件直调 rustc 运行：编译（--error-format=json）→ 翻译诊断 → 运行 exe
#[allow(clippy::too_many_arguments)]
pub(crate) fn 运行直接编译(
    界面表: &界面::界面消息表,
    项目根: &Path,
    源路径: &Path,
    语言包参数: &Option<PathBuf>,
    映射器: &映射管理器,
    源码: &str,
    源文件: &Path,
    列位映射参数: &i18n_rust_engine::列位映射::列位映射表,
    入口行映射参数: &[usize],
    项目上下文参数: &i18n_rust_engine::别名::项目上下文,
) -> anyhow::Result<std::process::ExitCode> {
    let 可执行文件 = 临时守卫::安全临时路径(&format!(
        "rzc-run-{}-{}.exe",
        临时守卫::安全用户段(),
        std::process::id()
    ))?;
    let 输出 = Command::new(解析rustc())
        .args(["--edition", "2024", "--error-format=json"])
        .arg(源路径)
        .arg("-o")
        .arg(&可执行文件)
        .output()
        .map_err(|错误值| anyhow::anyhow!("rustc 启动失败: {错误值}"))?;
    let 标准错误文本 = String::from_utf8_lossy(&输出.stderr).to_string();
    let 编译输出 = format!(
        "{}\n{}",
        String::from_utf8_lossy(&输出.stdout),
        标准错误文本
    );
    let 编译成功 = 输出.status.success();
    if !编译输出.trim().is_empty() {
        let _ = 翻译编译诊断(
            &编译输出,
            &标准错误文本,
            &诊断上下文 {
                界面表,
                语言包参数,
                项目根,
                映射器,
                源码,
                源文件,
                入口产物: 源路径.to_path_buf(),
                列位映射: Some(列位映射参数),
                入口行映射: Some(入口行映射参数),
                项目上下文: Some(项目上下文参数),
            },
            编译成功,
            true,
            // 直调 rustc：编译失败文本由本函数输出（非流式透传）
            false,
        );
    }
    if !编译成功 {
        return Ok(std::process::ExitCode::FAILURE);
    }
    // 编译成功：运行程序并传播退出码（无论成败都清理临时 exe）。
    // 标准错误流改为管道逐行过滤：恐慌框头/消息与 cargo 进度本地化，
    // 其余字节原样透传（有损解码，不因非 UTF-8 输出中断）；
    // 标准输出/标准输入保持继承，交互式程序不受影响
    let mut 进程句柄 = Command::new(&可执行文件)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|错误值| {
            let _ = fs::remove_file(&可执行文件);
            anyhow::anyhow!("运行失败: {错误值}")
        })?;
    if let Some(错误管道) = 进程句柄.stderr.take() {
        use std::io::{BufRead, BufReader};
        let mut 读取器 = BufReader::new(错误管道);
        let mut 翻译器 = 流翻译器::新建();
        let mut 行缓冲: Vec<u8> = Vec::new();
        loop {
            行缓冲.clear();
            if let Ok(读取数) = 读取器.read_until(b'\n', &mut 行缓冲) {
                if 读取数 == 0 {
                    break;
                }
                let 原始片段 = String::from_utf8_lossy(&行缓冲);
                let 文本行 = 原始片段.trim_end_matches(['\n', '\r']);
                eprintln!("{}", 翻译器.翻译(文本行, 界面表));
            } else {
                break;
            }
        }
    }
    let 进程状态值 = 进程句柄.wait().map_err(|错误值| {
        let _ = fs::remove_file(&可执行文件);
        anyhow::anyhow!("运行失败: {错误值}")
    })?;
    let _ = fs::remove_file(&可执行文件);
    Ok(进程状态值
        .code()
        .map(|代码值| std::process::ExitCode::from(代码值 as u8))
        .unwrap_or_else(|| std::process::ExitCode::FAILURE))
}

/// 单文件直调 rustc 检查：只生成编译元数据，不生成可执行文件
#[allow(clippy::too_many_arguments)]
pub(crate) fn 检查直接编译(
    界面表: &界面::界面消息表,
    项目根: &Path,
    源路径: &Path,
    语言包参数: &Option<PathBuf>,
    映射器: &映射管理器,
    源码: &str,
    源文件: &Path,
    列位映射参数: &i18n_rust_engine::列位映射::列位映射表,
    入口行映射参数: &[usize],
    项目上下文参数: &i18n_rust_engine::别名::项目上下文,
) -> anyhow::Result<std::process::ExitCode> {
    let 输出 = Command::new(解析rustc())
        .args([
            "--edition",
            "2024",
            "--error-format=json",
            "--emit=metadata",
        ])
        .arg(源路径)
        .output()
        .map_err(|错误值| anyhow::anyhow!("rustc 启动失败: {错误值}"))?;
    let 进程退出码 = if 输出.status.success() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    };
    let 标准错误文本 = String::from_utf8_lossy(&输出.stderr).to_string();
    let 编译输出 = format!(
        "{}\n{}",
        String::from_utf8_lossy(&输出.stdout),
        标准错误文本
    );
    let _ = 翻译编译诊断(
        &编译输出,
        &标准错误文本,
        &诊断上下文 {
            界面表,
            语言包参数,
            项目根,
            映射器,
            源码,
            源文件,
            入口产物: 源路径.to_path_buf(),
            列位映射: Some(列位映射参数),
            入口行映射: Some(入口行映射参数),
            项目上下文: Some(项目上下文参数),
        },
        输出.status.success(),
        false,
        // 直调 rustc 检查：诊断输出由本函数负责（非流式透传）
        false,
    );
    Ok(进程退出码)
}

/// 翻译 cargo 的人类可读进度行（JSON 模式下这些行仍输出到标准错误流）
///
/// 命中固定前缀（编译/完成/运行等）时翻译；
/// 其余行（程序标准错误流等）原样返回。
pub(crate) fn 翻译进度行(行文本: &str, 界面表: &界面::界面消息表) -> String {
    // cargo 进度行带行首缩进（如 `   Compiling ...`），先去除空白再匹配前缀
    let 裁剪后 = 行文本.trim_start();
    for (前缀, 键名) in [
        ("Compiling ", "cargo_progress_compiling"),
        ("Checking ", "cargo_progress_checking"),
        ("Finished ", "cargo_progress_finished"),
        ("Running ", "cargo_progress_running"),
        ("error: ", "cargo_progress_error"),
    ] {
        if let Some(剩余) = 裁剪后.strip_prefix(前缀) {
            // 错误摘要（如「无法编译某包，因 N 个先前的错误」）
            // 二次翻译固定短语，其余保留原文
            if 键名 == "cargo_progress_error" {
                let 剩余文本 = 剩余
                    .strip_prefix("could not compile ")
                    .map(|片段| {
                        // 尾部「因 N 个先前的错误」一并本地化：
                        // 如 `abc (bin "abc") due to 9 previous errors`
                        let 片段裁剪 = 片段.trim_start();
                        if let Some((主段, 因由段)) = 片段裁剪.split_once(" due to ") {
                            let 数量 = 因由段.split_whitespace().next().unwrap_or("");
                            format!(
                                "{}{}",
                                界面表.取带参消息("cargo_progress_could_not_compile", &[主段]),
                                界面表.取带参消息("cargo_progress_due_to", &[数量])
                            )
                        } else {
                            界面表.取带参消息("cargo_progress_could_not_compile", &[片段裁剪])
                        }
                    })
                    .unwrap_or_else(|| 剩余.to_string());
                return 界面表.取带参消息(键名, &[&剩余文本]);
            }
            return 界面表.取带参消息(键名, &[剩余.trim_start()]);
        }
    }
    行文本.to_string()
}

/// 程序标准错误流式翻译器：cargo 进度行 + 运行时恐慌输出的本地化
///
/// rustc 1.98 实测的程序恐慌输出形态（框头 → 消息 → 提示三行）：
/// ```
/// `thread 'main' (621725) panicked at src/main.rs:8:5:`
/// `index out of bounds: the len is 3 but the index is 5`
/// `note: run with RUST_BACKTRACE=1 environment variable to display a backtrace`
/// ```
/// 框头：主线程名映射为各语言主函数词（尖括号自定义名保留）、
/// 线程标识 `(621725) ` 剥离；消息六型（下标越界 / 字节索引越界 ×2 /
/// 字符边界 / 引用单元格借用冲突 ×2）按 ui.toml 模板本地化；提示行固定翻译；
/// 其余行回退 cargo 进度翻译（无匹配则原样返回）。
pub(crate) struct 流翻译器 {
    /// 上一行是恐慌框头：下一行按恐慌消息匹配
    期待恐慌消息: bool,
    /// cargo 纯文本错误行本地化用消息表（无值 = 只处理恐慌/进度行）
    ///
    /// 直调 rustc 运行时管道里只有被运行程序的标准错误流，无消息表时
    /// 不得拿 `error:` 行去查表（会把程序自己打印的错误误当 cargo 错误）。
    工具管理器: Option<i18n_rust_engine::诊断::错误消息管理器>,
    /// 待译清单登记语言（空串取当前界面语言）
    缺口语言: String,
    /// 程序已开始运行（见过运行标记行）：之后的错误行属于程序输出
    程序已启动: bool,
}

impl 流翻译器 {
    pub(crate) fn 新建() -> Self {
        Self {
            期待恐慌消息: false,
            工具管理器: None,
            缺口语言: String::new(),
            程序已启动: false,
        }
    }

    /// 启用 cargo 纯文本错误行本地化（仅 `cargo run` 流式透传通道使用）
    pub(crate) fn 带工具文本(
        mut self,
        管理器: i18n_rust_engine::诊断::错误消息管理器,
        语言码: impl Into<String>,
    ) -> Self {
        self.工具管理器 = Some(管理器);
        self.缺口语言 = 语言码.into();
        self
    }

    /// 翻译一行程序或 cargo 标准错误流（无匹配时原样返回）
    pub(crate) fn 翻译(
        &mut self, 行文本: &str, 界面表: &界面::界面消息表
    ) -> String {
        // 框头之后的第一行是恐慌消息；无论命中与否都退出该状态
        if std::mem::take(&mut self.期待恐慌消息)
            && let Some(消息文本) = 翻译恐慌消息(行文本, 界面表)
        {
            return 消息文本;
        }
        if let Some(框头) = 翻译恐慌框头(行文本, 界面表) {
            self.期待恐慌消息 = true;
            return 框头;
        }
        if 行文本.trim_end() == 恐慌提示行 {
            return 界面表.取消息("panic_note");
        }
        // cargo 的运行标记之后，标准错误流属于被运行的程序：
        // 不再按 cargo 自身错误处理（避免误译程序打印的 `error:` 行）
        if 行文本.trim_start().starts_with("Running ") {
            self.程序已启动 = true;
        } else if !self.程序已启动
            && let Some(本地化文本) =
                本地化工具行(行文本, self.工具管理器.as_ref(), 界面表, &self.缺口语言)
        {
            return 本地化文本;
        }
        翻译进度行(行文本, 界面表)
    }
}

/// 程序恐慌输出末尾的提示行（rustc 稳定文本，原样匹配）
const 恐慌提示行: &str =
    "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace";

///
/// 返回无值表示该行不是恐慌框头（由调用方原样透传）。
fn 翻译恐慌框头(行文本: &str, 界面表: &界面::界面消息表) -> Option<String> {
    let 剩余 = 行文本.trim_start().strip_prefix("thread '")?;
    let (线程标签, 名称后续) = 剩余.split_once('\'')?;
    // rustc 1.98 起框头含线程标识（如 `thread 'main' (621725) panicked at ...`），
    // 旧格式无标识；两种形态都归一为「线程名 + 恐慌位置」
    let 名称之后 = if let Some(编号及后续) = 名称后续.strip_prefix(" (") {
        编号及后续
            .split_once(") ")
            .map(|(_, 尾段)| 尾段)
            .unwrap_or_else(|| 名称后续)
    } else {
        名称后续
    };
    // 剥离线程标识后剩余文本以「恐慌位置」开头（无标识时带前导空格），
    // 先统一裁剪左侧再匹配
    let 定位文本 = 名称之后.trim_start().strip_prefix("panicked at ")?;
    let 定位文本 = 定位文本.trim_end().strip_suffix(':')?;
    if 线程标签.is_empty() || 定位文本.is_empty() {
        return None;
    }
    let 显示名 = if 线程标签 == "main" {
        界面表.取消息("panic_thread_main")
    } else {
        线程标签.to_string()
    };
    Some(界面表.取带参消息("panic_header", &[&显示名, 定位文本]))
}

/// 翻译恐慌消息（六型；未识别返回无值由调用方原样透传）
fn 翻译恐慌消息(行文本: &str, 界面表: &界面::界面消息表) -> Option<String> {
    let 消息行 = 行文本.trim();
    if 消息行 == "RefCell already borrowed" {
        return Some(界面表.取消息("panic_msg_borrow"));
    }
    if 消息行 == "RefCell already mutably borrowed" {
        return Some(界面表.取消息("panic_msg_borrow_mut"));
    }
    if let Some(剩余) = 消息行.strip_prefix("index out of bounds: the len is ") {
        let (长度段, 索引段) = 剩余.split_once(" but the index is ")?;
        return Some(界面表.取带参消息("panic_msg_index_oob", &[长度段.trim(), 索引段.trim()]));
    }
    if let Some(剩余) = 消息行.strip_prefix("start byte index ") {
        return 翻译字节索引消息(剩余, 界面表, true);
    }
    if let Some(剩余) = 消息行.strip_prefix("end byte index ") {
        return 翻译字节索引消息(剩余, 界面表, false);
    }
    None
}

/// 字节索引恐慌消息：越界（`... is out of bounds for string of length N`）或
/// 非字符边界（`... is not a char boundary; it is inside 'C' (bytes A..B of string)`）
fn 翻译字节索引消息(
    剩余: &str,
    界面表: &界面::界面消息表,
    是否起点: bool,
) -> Option<String> {
    let (索引段, 尾部) = 剩余.split_once(' ')?;
    if let Some(长度段) = 尾部.strip_prefix("is out of bounds for string of length ") {
        let 键名 = if 是否起点 {
            "panic_msg_byte_start_oob"
        } else {
            "panic_msg_byte_end_oob"
        };
        return Some(界面表.取带参消息(键名, &[索引段.trim(), 长度段.trim()]));
    }
    if let Some(内部段) = 尾部.strip_prefix("is not a char boundary; it is inside '") {
        let (字符值, 区段) = 内部段.split_once("' (bytes ")?;
        let (起点段, 终点段) = 区段.split_once("..")?;
        let 终点段 = 终点段.strip_suffix(" of string)")?;
        return Some(界面表.取带参消息(
            "panic_msg_char_boundary",
            &[索引段.trim(), 字符值, 起点段.trim(), 终点段.trim()],
        ));
    }
    None
}

/// 诊断翻译上下文：封装 `翻译编译诊断` 的共享引用参数，
/// 避免 8+ 位置参数的可读性灾难
pub(crate) struct 诊断上下文<'甲> {
    pub(crate) 界面表: &'甲 界面::界面消息表,
    pub(crate) 语言包参数: &'甲 Option<PathBuf>,
    pub(crate) 项目根: &'甲 Path,
    pub(crate) 映射器: &'甲 映射管理器,
    pub(crate) 源码: &'甲 str,
    pub(crate) 源文件: &'甲 Path,
    /// 本次实际编译的**入口产物**路径（请求文件的转译产物，如 `src/main.rs`、
    /// `src/工具.rs`、`examples/演示.rs`）。
    ///
    /// 诊断归属按「被编译目标」而非「请求文件」判定：`rzc check src/工具.zh`
    /// 时 cargo 仍编译整个目标，`src/main.rs` 里的报错属于 `main.zh`，
    /// 不能因为用户请求的是 `工具.zh` 就把它标成 `工具.zh`（旧实现硬编码
    /// `src/main.rs` 作入口，等于把入口钉死在默认主入口名上）。
    pub(crate) 入口产物: PathBuf,
    /// 转译产物的列映射：把 rustc 诊断的（英文产物）列号回译到母语源码列号。
    ///
    /// rustc 看到的是转译后的英文源码，其列号对母语源码无效
    ///（如母语关键字换成英文关键字后整行右移）。无值表示不做映射，保持旧行为。
    pub(crate) 列位映射: Option<&'甲 i18n_rust_engine::列位映射::列位映射表>,
    /// 入口磁盘产物行 → 引擎直出行映射（见
    /// [`注解产物`](包::注解产物)）。
    ///
    /// 入口产物写盘前经 `#[path]` 注解插入整行（每个非 ASCII `模块 xxx;`
    /// 一行），而 `列映射` 以未注解的引擎直出产物为基准；
    /// 回译前须先用本映射把 rustc 的磁盘行号换算回引擎直出行号。
    /// 无值表示无注解（磁盘产物与引擎直出逐行一致）。
    pub(crate) 入口行映射: Option<&'甲 [usize]>,
    /// 项目级声明上下文：诊断重放（[`解析方言上下文`]）须与
    /// 写盘转译共享同一上下文，否则列映射与磁盘产物不一致（#8）
    pub(crate) 项目上下文: Option<&'甲 i18n_rust_engine::别名::项目上下文>,
}

/// 解析 cargo --message-format=json 输出并翻译为教学化诊断（检查与运行共用）
///
/// 返回是否成功输出了翻译后的教学诊断；调用方据此决定是否回退原始文本。
/// `cargo_ok=false` 且无可解析诊断时原样输出 cargo 消息，绝不虚报“编译成功”。
/// `silent_success=true`（运行场景）时，无诊断且编译成功保持静默——
/// 程序已运行，不再提示“编译成功”。
/// `streamed_output=true`（cargo 运行场景）时，cargo 消息与程序输出均已实时
/// 透传：「失败且无可解析诊断」不再补打“编译错误”标签——构建可能已成功，
/// 失败发生在程序运行阶段（如恐慌退出），补打空标签会误导。
pub(crate) fn 翻译编译诊断(
    编译输出: &str,
    标准错误文本: &str,
    共享上下文: &诊断上下文<'_>,
    编译成功: bool,
    静默成功: bool,
    流式输出: bool,
) -> bool {
    let 已处理 = 翻译编译诊断实现(
        编译输出,
        标准错误文本,
        共享上下文,
        编译成功,
        静默成功,
        流式输出,
    );
    // 本轮若有英文残留（未命中消息表），统一提示一次并落盘待译清单；
    // 逐条加标记会污染诊断输出，故只在本轮结束时汇总
    报告翻译缺口(共享上下文.界面表);
    已处理
}

/// 提示并持久化本轮登记的英文残留（无登记时静默）
///
/// 单元测试进程只清空登记表不落盘：测试用样例语言代码登记的残留不该写进
/// 用户主目录（落盘/读取/去重逻辑另有临时目录用例覆盖，见 `诊断::待译清单`）。
pub(crate) fn 报告翻译缺口(界面表: &界面::界面消息表) {
    use i18n_rust_engine::诊断::待译清单;
    let 已记录 = 待译清单::取出缺口();
    if 已记录.is_empty() {
        return;
    }
    if cfg!(test) {
        return;
    }
    let 目录 = 待译清单::清单目录();
    待译清单::落盘缺口至目录(&目录, &已记录);
    eprintln!(
        "{}",
        界面表.取带参消息(
            "translation_gap_notice",
            &[&已记录.len().to_string(), &目录.display().to_string()]
        )
    );
}

/// cargo 纯文本错误行的前缀与本地化模板（模板含 `{}` 占位符，与进度行同源）
///
/// `help:` 是工具给的可执行下一步（如 rustup 未配默认工具链时的
/// `run 'rustup default stable'`），不接就只剩一行英文指引留在母语输出里
const 工具行标记表: [(&str, &str); 5] = [
    ("error:", "cargo_progress_error"),
    ("warning:", "tool_prefix_warning"),
    ("note:", "tool_prefix_note"),
    ("help:", "tool_prefix_help"),
    ("Caused by:", "tool_prefix_caused_by"),
];

/// 交回进度行翻译器处理的 cargo 固定句（已有专门本地化，补表无意义）
const 工具行跳过表: [&str; 2] = ["could not compile ", "aborting due to "];

/// 按请求文件扩展名取诊断语言代码（无扩展名/未知时跟随界面语言）
pub(crate) fn 诊断语言代码(源文件: &Path) -> String {
    源文件
        .extension()
        .and_then(|扩展| 扩展.to_str())
        .and_then(扩展名转语言代码)
        .unwrap_or_else(界面::探测界面语言)
}

/// 加载消息表：`--lang-pack` 目录 > 项目内 `lang-packs/<lang>/` > 全局语言目录 > 内置
///
/// 文件不存在或加载失败时降级到内置表（只警告不阻断）；返回无值
/// 意味着连内置表都无法解析。
pub(crate) fn 载入错误管理器(
    界面表: &界面::界面消息表,
    语言包参数: &Option<PathBuf>,
    项目根: &Path,
    语言码: &str,
) -> Option<i18n_rust_engine::诊断::错误消息管理器> {
    use i18n_rust_engine::诊断::错误消息管理器;
    let 错误消息路径 = if let Some(路径段) = 语言包参数 {
        路径段.join("errors.toml")
    } else if 语言包根目录(项目根)
        .join(语言码)
        .join("errors.toml")
        .exists()
    {
        语言包根目录(项目根).join(语言码).join("errors.toml")
    } else {
        语言管理::全局语言目录().join(语言码).join("errors.toml")
    };
    if 错误消息路径.exists() {
        // 加载失败时降级到内置表，不因错误消息文件损坏阻断诊断展示
        let 文件载入 = 错误消息管理器::从文件载入(&错误消息路径);
        if 文件载入.is_ok() {
            return 文件载入.ok();
        }
        if let Err(错误值) = 文件载入 {
            eprintln!(
                "{}",
                界面表.取带参消息("load_error_msg_failed", &[&错误值.to_string()])
            );
        }
    }
    // 文件路径不可用/加载失败时回退内置语言包（未知语言代码自动回退中文）
    let 内置数据 = 内置语言::获取内置数据(语言码);
    let 字符串载入 = 错误消息管理器::从字符串载入(内置数据.错误内容);
    if 字符串载入.is_ok() {
        return 字符串载入.ok();
    }
    if let Err(错误值) = 字符串载入 {
        eprintln!(
            "{}",
            界面表.取带参消息("warn_builtin_errors_failed", &[&错误值.to_string()])
        );
    }
    None
}

/// 用消息表渲染一句工具错误正文
///
/// 返回无值表示未命中且不兜底（调用方继续原流程）；「未命中即登记」为真时
/// 未命中返回原文（绝不吞诊断）并登记待译清单。判「是否已译」只看消息表
/// 是否命中，不看渲染后还剩多少英文词：德/法/西/葡等拉丁语系译文里夹着
/// 路径与包名是常态，按词串判会把已译好的句子打回英文。JSON 通道的
/// 登记点同此口径（见翻译器的缺口登记：未命中或残段未被 `{qN}` 消费才记）。
fn 渲染工具详情(
    详情: &str,
    管理器: &i18n_rust_engine::诊断::错误消息管理器,
    缺口语言: &str,
    记录未命中: bool,
) -> Option<String> {
    use i18n_rust_engine::诊断::{填充动态占位, 待译清单};
    let Some((消息条目, 残段)) = 管理器.按消息查询(详情) else {
        if !记录未命中 {
            return None;
        }
        待译清单::登记缺口(缺口语言, 待译清单::工具级, "", 详情);
        return Some(详情.to_string());
    };
    let mut 成品文本 = 消息条目.消息模板.clone();
    if let Some(尾部) = 残段 {
        let (填充后, 已消费) = 填充动态占位(&成品文本, &尾部);
        成品文本 = 填充后;
        if !已消费 {
            // 模板没有对应占位符，英文残段拼回尾巴（兜底）→ 记待译
            成品文本.push_str(尾部.文字());
            待译清单::登记缺口(缺口语言, 待译清单::工具级, "", 详情);
        }
    }
    Some(成品文本)
}

/// 本地化单行 cargo 纯文本错误；非错误形态行返回无值（调用方继续原流程）
///
/// 保留行首缩进；映射器为无值（无消息表）时不处理，避免把
/// 被运行程序自己打印的 `error:` 行误标。已知固定汇总句返回无值，
/// 由调用方交给进度行翻译器（那里已有「无法编译」的专门处理）。
fn 本地化工具行(
    行文本: &str,
    管理器: Option<&i18n_rust_engine::诊断::错误消息管理器>,
    界面表: &界面::界面消息表,
    缺口语言: &str,
) -> Option<String> {
    let 管理器 = 管理器?;
    let 缩进量 = 行文本.len() - 行文本.trim_start().len();
    let 正文 = 行文本.trim_start();
    let 命中 = 工具行标记表.iter().find(|(前缀, _)| 正文.starts_with(前缀));
    let (标记, 键名) = if let Some((标记值, 键值)) = 命中 {
        (*标记值, *键值)
    } else {
        // 无标记的缩进行：cargo 把错误链的因由正文写在 `Caused by:` 下一行。
        // 只在消息表命中时换译，未命中交回进度行翻译（缩进输出也可能是
        // 被运行程序自己的打印，不能当成工具错误登记待译）
        if 缩进量 > 0 {
            return 渲染工具详情(正文, 管理器, 缺口语言, false)
                .map(|译文| format!("{}{}", " ".repeat(缩进量), 译文));
        }
        return None;
    };
    let 详情 = 正文[标记.len()..].trim_start();
    if 详情.is_empty() {
        // 光杆标记行（cargo 的 `Caused by:` 单独占一行）：只换标签，正文在缩进续行
        return Some(界面表.取带参消息(键名, &[""]).trim_end().to_string());
    }
    if 工具行跳过表.iter().any(|跳过句| 详情.starts_with(跳过句)) {
        return None;
    }
    let 已渲染 = 渲染工具详情(详情, 管理器, 缺口语言, true)?;
    Some(format!(
        "{}{}",
        " ".repeat(缩进量),
        界面表.取带参消息(键名, &[&已渲染])
    ))
}

/// 本地化 cargo 自身错误（非 JSON 通道：清单解析、无编译目标、依赖拉不到等）
///
/// rustc 诊断走 JSON 翻译链，cargo 自己的报错是纯文本，旧行为原样输出英文——
/// 新手最先撞的就是这一类（不在项目里、Cargo.toml 写坏、没声明编译目标）。
///
/// 逐行交给 [`本地化工具行`]：命中消息表则换本地化前缀输出译文，
/// 未命中保留原文并登记待译清单（工具通道）。无标记前缀的辅助行
///（如缩进后的补充说明）不单独登记，避免把进度行刷进清单。
pub(crate) fn 本地化工具文本(
    全文: &str,
    管理器: &i18n_rust_engine::诊断::错误消息管理器,
    界面表: &界面::界面消息表,
    缺口语言: &str,
) -> String {
    let mut 成品 = String::with_capacity(全文.len());
    for (序号, 原始行) in 全文.lines().enumerate() {
        if 序号 > 0 {
            成品.push('\n');
        }
        if let Some(本地化文本) = 本地化工具行(原始行, Some(管理器), 界面表, 缺口语言)
        {
            成品.push_str(&本地化文本);
        } else {
            成品.push_str(&翻译进度行(原始行, 界面表));
        }
    }
    成品
}

fn 翻译编译诊断实现(
    编译输出: &str,
    标准错误文本: &str,
    共享上下文: &诊断上下文<'_>,
    编译成功: bool,
    静默成功: bool,
    流式输出: bool,
) -> bool {
    use i18n_rust_engine::诊断::{
        建议包名, 构建类型映射, 解析诊断输出, 诊断翻译器
    };
    let 界面表 = 共享上下文.界面表;
    let 语言包参数 = 共享上下文.语言包参数;
    let 项目根 = 共享上下文.项目根;
    let 映射器 = 共享上下文.映射器;
    let 源文件 = 共享上下文.源文件;

    // 未解析导入提取：诊断展示后附带 `rzc add` 加依赖提示（教学化引导）
    let 未解析包集 = 提取未解析包(编译输出);
    // 本项目自身可引入的包名（包名 / 库节名）：命中它的候选不能
    // 建议 `rzc add`（Cargo 拒绝包依赖自己），见 [`输出依赖提示`]
    let 本地名集 = 本地包名集(项目根);
    // 库入口源文件按请求文件的扩展名推断（`src/main.zh` → `src/lib.zh`）：
    // 同项目内的方言文件通常同一种语言，且库入口正是方言入口名之一
    let 库入口 = 源文件
        .extension()
        .and_then(|扩展| 扩展.to_str())
        .map(|扩展| {
            let 库源 = 项目根.join("src").join(format!("lib.{扩展}"));
            (format!("src/lib.{扩展}"), 库源.is_file())
        });
    let 依赖提示 = || {
        输出依赖提示(
            &未解析包集,
            界面表,
            &本地名集,
            库入口
                .as_ref()
                .map(|(路径段, 是否存在)| (路径段.as_str(), *是否存在)),
        )
    };

    // 按语言代码选择错误消息表（与 cargo 纯文本通道共用同一张表）
    let 语言码 = 诊断语言代码(源文件);
    // 类型映射（英文 → 母语）：供诊断消息中的类型/特征名中文化（如
    // `std::fmt::Display` → `标准库::格式化::可显示`）。构建逻辑下沉引擎，
    // 与 LSP 共用同一来源，避免两侧漂移（关键字表类型节反转为主，别名/模块路径补充）。
    let 反向映射 = 构建类型映射(映射器);
    let 翻译器 = 载入错误管理器(界面表, 语言包参数, 项目根, &语言码).map(|翻译管理器| {
        诊断翻译器::新建(翻译管理器, 反向映射.clone()).带待译语言(语言码.clone())
    });

    // cargo 自身错误（纯文本通道）本地化：清单写坏、不在项目里、
    // 未声明编译目标这类新手高频错误旧行为是直接喷英文
    let 编译错误文本 = || {
        if let Some(译器) = &翻译器 {
            本地化工具文本(标准错误文本.trim(), 译器.取译表(), 界面表, &语言码)
        } else {
            标准错误文本.trim().to_string()
        }
    };

    let 原文件名 = 源文件
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let mut 诊断集 = 解析诊断输出(编译输出);
    // 保留 error/warning；不要求有错误码——无码的解析错误（如缺括号）
    // 也必须显示，否则会被静默吞掉导致假“编译成功”
    诊断集.retain(|单条| 单条.级别 == "error" || 单条.级别 == "warning");
    // rustc 的汇总元消息（`aborting due to N previous error(s)`）：无代码位置、
    // 无教学内容，具体错误已逐条列出；省略它可以避免英文残句与伪“错误”行
    // 混入诊断列表（数字无法用消息表占位符捕获，故不在翻译层处理）
    诊断集.retain(|单条| !单条.诊断消息.starts_with("aborting due to "));
    let mut 已见码集 = HashSet::new();
    诊断集.retain(|单条| {
        if let Some(码) = &单条.诊断码 {
            已见码集.insert(码.码值.clone())
        } else {
            true // 无错误码的诊断（解析错误等）不去重，直接保留
        }
    });

    // rustc 的「若要使用名为 X 的第三方库，请 `cargo add X`」建议：X 命中本项目
    // 包名时是**错建议**——Cargo 不允许一个包依赖自己，读者照着做只会撞
    // `cannot add "calc" as a dependency to itself`。引擎侧没有项目上下文（不知道
    // 包名），故在 CLI 按本地包名剔除这条子诊断；真能走通的动作由
    // 依赖提示函数给出。引擎已按同一形状丢弃非 ASCII 名（母语模块误提取），
    // 这里复用它的提取函数，不另写一套解析。
    if !本地名集.is_empty() {
        for 单条 in &mut 诊断集 {
            单条.子诊断.retain(|子项| {
                !建议包名(&子项.诊断消息)
                    .is_some_and(|候选名| 本地名集.iter().any(|本地名| 本地名 == 候选名))
            });
        }
    }

    if 诊断集.is_empty() {
        if 编译成功 {
            if !静默成功 {
                println!("{}", 界面表.取消息("success_compile"));
            }
        } else if !流式输出 {
            // cargo 失败但无可解析的 JSON 诊断（Cargo.toml 语法错误、
            // 链接错误等）：本地化后输出 cargo 消息，绝不虚报“编译成功”；
            // 流式输出场景跳过（见函数文档，避免运行时失败被误标）
            eprintln!("{}", 界面表.取带参消息("compile_error", &[&编译错误文本()]));
        }
        依赖提示();
        return false;
    }

    if let Some(译器) = &翻译器 {
        let mut 教学列表 = 译器.批量翻译(&诊断集);
        let mut 已见教学码集 = HashSet::new();
        教学列表.retain(|单条| {
            单条
                .错误码
                .as_ref()
                .is_none_or(|码| 已见教学码集.insert(码.clone()))
        });
        // 诊断定位回译：入口产物（src/main.rs）用调用方传入的源码与列映射；
        // 多文件项目的子模块产物（如 src/接口.rs）解析回母语源文件
        // （src/接口.zh）后用其源码与列映射单独回译——修复子模块错误被统一
        // 误标为入口文件且源码行/列号错位的问题。
        let 方言扩展名文本 = 源文件
            .extension()
            .and_then(|扩展| 扩展.to_str())
            .unwrap_or("zh");
        let mut 修复器 = 诊断定位修复器::新建(共享上下文, &原文件名, 方言扩展名文本);
        for 单条 in &mut 教学列表 {
            修复器.修复(单条);
        }
        if 教学列表.is_empty() {
            if 编译成功 {
                if !静默成功 {
                    println!("{}", 界面表.取消息("success_compile"));
                }
            } else if !流式输出 {
                eprintln!("{}", 界面表.取带参消息("compile_error", &[&编译错误文本()]));
            }
            依赖提示();
            return false;
        }
        println!(
            "{}",
            i18n_rust_engine::诊断::教学诊断::批量作为文字输出(&教学列表)
        );
        依赖提示();
        true
    } else {
        // 无翻译表：输出 JSON 中的原始消息，保证诊断不丢失
        for 行文本 in 编译输出.lines() {
            if let Ok(原始值) = serde_json::from_str::<serde_json::Value>(行文本) {
                if let Some(消息值) = 原始值.get("message") {
                    println!("{}", 消息值.as_str().unwrap_or(""));
                }
            } else if !行文本.trim().is_empty() {
                println!("{}", 行文本);
            }
        }
        依赖提示();
        false
    }
}

/// 诊断文件上下文：rustc 报告的（英文产物）文件解析回母语源文件后
/// 得到的显示名、源码与列映射，供行号/列号/源行统一回译
struct 诊断文件上下文 {
    显示名称: String,
    源码: String,
    列位映射: i18n_rust_engine::列位映射::列位映射表,
    /// 磁盘产物行 → 引擎直出行映射（`#[path]` 注解插入行时非恒等）。
    ///
    /// 托管布局下入口与子模块产物都由
    /// [`注解产物模块路径并返回行映射`](引擎库::产物布局::注解产物模块路径并返回行映射)
    /// 注解，插入行数随该文件的模块声明而异，故按产物自身重算。
    行映射: Vec<usize>,
}

/// 解析 rustc 报告的诊断文件路径为实际路径：
/// 绝对路径原样；相对路径先相对项目根（cargo 的当前目录），再相对当前目录。
/// 无法定位时返回无值。
fn 解析产物路径(项目根: &Path, 名称: &str) -> Option<PathBuf> {
    let 候选路径 = Path::new(名称);
    if 候选路径.is_absolute() {
        return Some(候选路径.to_path_buf());
    }
    let 根下路径 = 项目根.join(候选路径);
    if 根下路径.exists() {
        return Some(根下路径);
    }
    std::env::current_dir().ok().map(|目录| 目录.join(候选路径))
}

/// 把 rustc 报告的诊断文件（转译产物路径）解析为母语源文件上下文。
///
/// 多文件项目中 rustc 报告的是子模块转译产物（如 src/接口.rs），其行列是
/// 英文产物的坐标、源码行也是英文；须找到同名方言源文件（src/接口.zh）并用
/// 其源码重建列映射。对应方言文件不存在（手写 .rs、第三方库源码等）时
/// 返回无值，调用方保持 rustc 原始定位，绝不误标成入口文件。
fn 解析方言上下文(
    项目根: &Path,
    方言扩展名: &str,
    产物文件: &str,
    映射器: &映射管理器,
    项目上下文参数: Option<&i18n_rust_engine::别名::项目上下文>,
) -> Option<诊断文件上下文> {
    let 绝对产物 = 解析产物路径(项目根, 产物文件)?;
    // 托管布局：rustc 报的是 `.rz/gen/src/接口.rs`（且父目录与 `#[path]` 值
    // 直接拼接，形如 `src/../.rz/gen/src/接口.rs`），剥掉托管前缀才是源文件
    // 所在的源码树位置，否则同名方言源文件永远找不到（诊断停在英文坐标）
    let 绝对产物 = i18n_rust_engine::产物布局::去托管前缀(项目根, &绝对产物).unwrap_or(绝对产物);
    // 产物名以 .rs 结尾时换成方言扩展名（src/接口.rs → src/接口.zh）
    let 源路径 = if 绝对产物.extension().is_some_and(|扩展| 扩展 == "rs") {
        绝对产物.with_extension(方言扩展名)
    } else {
        绝对产物
    };
    let 源码 = fs::read_to_string(&源路径).ok()?;
    // 现场重放转译管线取列映射：与写盘时同一管线（同一项目上下文，
    // 确定性输出），保证列号回译与磁盘上的转译产物一致；静默重放：
    // 教学告警（lint/全角/Unicode）已在写盘转译时输出过，此处重放不得重复告警
    let 转译结果 = i18n_rust_engine::静默带项目转译管线(&源码, 映射器, 项目上下文参数);
    let 列位映射 =
        i18n_rust_engine::列位映射::列位映射表::回放生成(&源码, &转译结果.流程映射);
    // 子模块产物写盘时按托管布局插入了 `#[path]` 行：列映射以未注解的引擎
    // 直出产物为基准，须先用同一函数的行映射换算回直出行号，否则注解越多、
    // 行号偏移越大（多文件项目下的错行）。产物路径按同一规则重算，
    // 与 `transpile_project_files()` 写盘时的注解保持一致。
    let 产物 = i18n_rust_engine::产物布局::产物路径(项目根, &源路径);
    let 行映射 = {
        let 扩展名序列 = 语言管理::全部可用扩展名();
        crate::注解产物(&转译结果.输出, 项目根, &源路径, &产物, false, &扩展名序列).1
    };
    Some(诊断文件上下文 {
        显示名称: 源路径
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        源码,
        列位映射,
        行映射,
    })
}

/// 诊断定位回译器：把教学诊断的位置从英文产物坐标回译到母语源码坐标
///
/// 入口产物用调用方传入的源码/列映射；项目内其他方言文件的转译产物按
/// 文件名解析回母语源文件，用其源码与列映射单独回译；非方言文件保持
/// rustc 原始定位。
pub(crate) struct 诊断定位修复器<'甲> {
    /// 入口文件的母语源码
    源码: &'甲 str,
    /// 入口产物的列映射（rustc 列号 → 母语列号）
    列位映射: Option<&'甲 i18n_rust_engine::列位映射::列位映射表>,
    /// 入口磁盘产物行 → 引擎直出行映射（含 `#[path]` 注解插入行时非恒等）
    入口行映射: Option<&'甲 [usize]>,
    项目根: &'甲 Path,
    映射器: &'甲 映射管理器,
    /// 入口文件名（如 main.zh）
    原文件名: &'甲 str,
    /// 被编译入口产物的规范路径（本次请求文件的产物），用于识别入口产物的诊断
    入口规范路径: Option<PathBuf>,
    /// 入口文件的方言扩展名（如中文扩展名），用于把产物 .rs 还原为源文件
    方言扩展名: &'甲 str,
    /// 项目级声明上下文（诊断重放与写盘转译一致，见 [`诊断上下文::项目上下文`]）
    项目上下文: Option<&'甲 i18n_rust_engine::别名::项目上下文>,
    /// 诊断文件 → 母语源文件上下文缓存（无值表示非方言文件）
    文件上下文缓存: HashMap<String, Option<诊断文件上下文>>,
}

impl<'甲> 诊断定位修复器<'甲> {
    pub(crate) fn 新建(
        共享上下文: &'甲 诊断上下文<'甲>,
        原文件名: &'甲 str,
        方言扩展名: &'甲 str,
    ) -> Self {
        Self {
            源码: 共享上下文.源码,
            列位映射: 共享上下文.列位映射,
            入口行映射: 共享上下文.入口行映射,
            项目根: 共享上下文.项目根,
            映射器: 共享上下文.映射器,
            原文件名,
            // 按被编译目标而非固定 `src/main.rs`：请求文件非入口时（如
            // `rzc check src/工具.zh`，产物 src/工具.rs），入口产物是它自己
            入口规范路径: 共享上下文.入口产物.canonicalize().ok(),
            方言扩展名,
            项目上下文: 共享上下文.项目上下文,
            文件上下文缓存: HashMap::new(),
        }
    }

    /// 回译一条教学诊断的全部位置（含子诊断）
    fn 修复(&mut self, 教学项: &mut i18n_rust_engine::诊断::教学诊断) {
        for 定位项 in &mut 教学项.位置集 {
            self.修复位置(定位项);
        }
        for 子项 in &mut 教学项.子诊断 {
            self.修复(子项);
        }
    }

    pub(crate) fn 修复位置(&mut self, 定位项: &mut i18n_rust_engine::诊断::诊断位置) {
        if self.是入口产物(&定位项.文件名称) {
            定位项.文件名称 = self.原文件名.to_string();
            // 先把 rustc 的（英文产物）行列回译到母语源码坐标，
            // 再用回译后的行号取源码行——顺序不可颠倒，否则源码行与列号错位。
            if let Some(列映射表) = self.列位映射 {
                // 磁盘产物含 `#[path]` 注解插入行时，行号先换算回引擎直出行
                //（列映射以引擎直出产物为基准，从 1 开始计数换算后传入）
                let 直出行号 = self
                    .入口行映射
                    .and_then(|映射| {
                        映射
                            .get(定位项.起始行号.saturating_sub(1) as usize)
                            .copied()
                    })
                    .map(|直出行| 直出行 as u32 + 1)
                    .unwrap_or(定位项.起始行号);
                let (回译行, 回译列) = 列映射表.回译坐标(直出行号, 定位项.起始列号);
                定位项.起始行号 = 回译行;
                定位项.起始列号 = 回译列;
            }
            定位项.源码文本 = 取中文源码行(self.源码, 定位项.起始行号);
            return;
        }
        let 缓存值 = self
            .文件上下文缓存
            .entry(定位项.文件名称.clone())
            .or_insert_with(|| {
                解析方言上下文(
                    self.项目根,
                    self.方言扩展名,
                    &定位项.文件名称,
                    self.映射器,
                    self.项目上下文,
                )
            });
        let Some(文件上下文) = 缓存值 else {
            // 非方言文件（手写 .rs、第三方源码等）：保持 rustc 原始定位
            return;
        };
        定位项.文件名称 = 文件上下文.显示名称.clone();
        // 注解插入行先行换算（映射从 1 开始计数），再回译列号
        let 直出行号 = 文件上下文
            .行映射
            .get(定位项.起始行号.saturating_sub(1) as usize)
            .copied()
            .map(|直出行| 直出行 as u32 + 1)
            .unwrap_or(定位项.起始行号);
        let (回译行, 回译列) = 文件上下文.列位映射.回译坐标(直出行号, 定位项.起始列号);
        定位项.起始行号 = 回译行;
        定位项.起始列号 = 回译列;
        定位项.源码文本 = 取中文源码行(&文件上下文.源码, 定位项.起始行号);
    }

    /// 判断诊断文件是否为本次编译入口（请求文件）的转译产物
    fn 是入口产物(&self, 名称: &str) -> bool {
        let Some(入口规范) = self.入口规范路径.as_ref() else {
            return false;
        };
        解析产物路径(self.项目根, 名称)
            .and_then(|绝对产物| 绝对产物.canonicalize().ok())
            .is_some_and(|绝对产物| &绝对产物 == 入口规范)
    }
}

/// 从 cargo --message-format=json 输出提取未声明的包名
///
/// 识别未解析导入（E0432）与解析失败（E0433）诊断，
/// 候选提取复用引擎共享逻辑（已去重，排除标准库与保留路径）。
pub(crate) fn 提取未解析包(编译输出: &str) -> Vec<String> {
    use i18n_rust_engine::诊断::{是否未解析导入消息, 未解析包候选};
    let mut 汇集结果: Vec<String> = Vec::new();
    for 行文本 in 编译输出.lines() {
        let Ok(条目值) = serde_json::from_str::<serde_json::Value>(行文本) else {
            continue;
        };
        // cargo JSON 流中诊断嵌套在 compiler-message 条目里；
        // 兼容直接的诊断对象两种形态
        let 消息对象 = if 条目值.get("reason").is_some() {
            条目值.get("message")
        } else {
            Some(&条目值)
        };
        let Some(消息值) = 消息对象 else {
            continue;
        };
        let 级别值 = 消息值
            .get("level")
            .and_then(|内部值| 内部值.as_str())
            .unwrap_or("");
        if 级别值 != "error" {
            continue;
        }
        let 代码值 = 消息值
            .get("code")
            .and_then(|编码项| 编码项.get("code"))
            .and_then(|内部值| 内部值.as_str())
            .unwrap_or("");
        let 消息文本 = 消息值
            .get("message")
            .and_then(|内部值| 内部值.as_str())
            .unwrap_or("");
        if !matches!(代码值, "E0432" | "E0433") && !是否未解析导入消息(消息文本) {
            continue;
        }
        for 段 in 未解析包候选(消息文本) {
            if !汇集结果.contains(&段) {
                汇集结果.push(段);
            }
        }
        // 码命中但消息文本未命中时（消息格式变化），退化到纯首段提取
        if matches!(代码值, "E0432" | "E0433") && 未解析包候选(消息文本).is_empty() {
            use i18n_rust_engine::诊断::取反引号首段;
            for 段 in 取反引号首段(消息文本) {
                // 非 ASCII 段不是包名（母语标识符误提取），
                // 与 未解析包候选 的过滤保持一致
                if !matches!(
                    段.as_str(),
                    "std" | "core" | "alloc" | "self" | "super" | "crate" | "proc_macro"
                ) && !段
                    .chars()
                    .next()
                    .is_some_and(|字符值| 字符值.is_ascii_digit())
                    && 段.is_ascii()
                    && !汇集结果.contains(&段)
                {
                    汇集结果.push(段);
                }
            }
        }
    }
    汇集结果
}

/// 本项目自身可被引入的包名（包名与库节名）
///
/// Cargo 把包名里的 `-` 换成 `_` 当作包名，库节名可再覆盖一次。
/// rustc 分不出「自己的库还没生成产物」和「没声明的第三方库」，两种情况都报
/// E0432 并建议 `cargo add`——拿包名去加依赖会被 Cargo 直接拒绝
/// （实测会提示「不能把包加为自身依赖」），所以这条建议
/// 在本地包名上**必然失败**。无 Cargo.toml 或 TOML 非法时返回空，不改任何行为。
fn 本地包名集(项目根: &Path) -> Vec<String> {
    let mut 名称集 = Vec::new();
    let Ok(清单文本) = fs::read_to_string(项目根.join("Cargo.toml")) else {
        return 名称集;
    };
    let Ok(清单值) = toml::from_str::<toml::Value>(&清单文本) else {
        return 名称集;
    };
    let 原始名集 = [
        清单值
            .get("package")
            .and_then(|包项| 包项.get("name"))
            .and_then(|内部值| 内部值.as_str()),
        清单值
            .get("lib")
            .and_then(|库项| 库项.get("name"))
            .and_then(|内部值| 内部值.as_str()),
    ];
    for 名称 in 原始名集.iter().flatten() {
        let 包名 = 名称.replace('-', "_");
        if !名称集.contains(&包名) {
            名称集.push(包名);
        }
    }
    名称集
}

/// 输出未解析导入的加依赖提示（无候选时静默）
///
/// 候选命中本项目包名时**不能**建议 `rzc add`（见 [`本地包名集`]）：
/// 那样只会把读者引向一条 Cargo 必然拒绝的命令。此时按库入口在不在分两种说法——
/// 有 `src/lib.<扩展名>` 就指向「先检查它生成 src/lib.rs」，没有就点明这个文件
/// 还不存在、`use 包名::…` 引用的正是它编译出的库。其余候选照旧走第三方库提示。
fn 输出依赖提示(
    包清单: &[String],
    界面表: &界面::界面消息表,
    本地名集: &[String],
    库入口: Option<(&str, bool)>,
) {
    for 名称 in 包清单 {
        if !本地名集.iter().any(|本地名| 本地名 == 名称) {
            eprintln!(
                "{}",
                界面表.取带参消息("hint_add_dependency", &[名称, 名称])
            );
            continue;
        }
        // 本地包名：只给「生成/新建库入口」这一条真能走通的路。拿不到扩展名
        // （无从推断 `src/lib.<ext>`）时宁可不说话，也不给一条 Cargo 会拒绝的命令。
        let Some((路径段, 是否存在)) = 库入口 else {
            continue;
        };
        if 是否存在 {
            eprintln!(
                "{}",
                界面表.取带参消息("hint_own_crate_lib", &[名称, 名称, 路径段])
            );
        } else {
            eprintln!(
                "{}",
                界面表.取带参消息("hint_own_crate_no_lib", &[名称, 名称, 路径段, 路径段])
            );
        }
    }
}

fn 取中文源码行(源码: &str, 行号: u32) -> Option<String> {
    if 行号 == 0 {
        return None;
    }
    源码
        .lines()
        .nth((行号 - 1) as usize)
        .map(|单行| 单行.to_string())
}

#[cfg(test)]
mod 母语测试 {
    use super::*;
    use i18n_rust_engine::模块路径::注解非母语模块并返回行映射;

    /// 测试用界面消息：直接读仓库语言包（绕开用户主目录全局安装副本的
    /// 缺键干扰，保证断言与仓库内容一致）
    fn 取测试界面(语言码: &str) -> 界面::界面消息表 {
        let 目录 = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../engine/lang-packs")
            .join(语言码);
        界面::界面消息表::按显式目录建表(&目录)
    }

    /// 加载内置中文映射管理器（测试诊断回译用）
    fn 中文映射器() -> 映射管理器 {
        let 内置数据 = 内置语言::获取内置数据("zh");
        映射管理器::从内置载入(
            内置数据.关键字内容,
            内置数据.模块路径内容,
            内置数据.标准库内容,
            内置数据.三方库数据,
        )
        .expect("内置中文语言包应可加载")
    }

    /// 单文件直调编译的启用条件：仅 main.zh 且无依赖
    #[test]
    fn 可用直接编译单文件无依赖() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path();
        fs::create_dir_all(根.join("src")).unwrap();
        fs::write(
            根.join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(根.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        assert!(可用直接编译(根, &根.join("src/main.zh")));
    }

    /// 有第三方依赖时回退 cargo（依赖行 `rand = "0.8"`）
    #[test]
    fn 可用直接编译有依赖时回退() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path();
        fs::create_dir_all(根.join("src")).unwrap();
        fs::write(
            根.join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n[dependencies]\nrand = \"0.8\"\n",
        )
        .unwrap();
        fs::write(根.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        assert!(!可用直接编译(根, &根.join("src/main.zh")));
    }

    /// 多文件项目（源码目录下有第二个方言文件）回退 cargo
    #[test]
    fn 可用直接编译多方言文件时回退() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path();
        fs::create_dir_all(根.join("src")).unwrap();
        fs::write(
            根.join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(根.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        fs::write(
            根.join("src/数学.zh"),
            "函数 加(甲: 整数, 乙: 整数) -> 整数 { 甲 + 乙 }\n",
        )
        .unwrap();
        assert!(!可用直接编译(根, &根.join("src/main.zh")));
    }

    /// 入口产物含 #[path] 注解时，诊断回译先换算行号再列映射
    ///（回归：多模块入口中所有诊断行号被注解行整体顶偏移）
    #[test]
    fn 定位修复器入口行映射() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path();
        fs::create_dir_all(根.join("src")).unwrap();
        let 源码 = "// 头\n模块 数学;\n\n函数 主函数() {\n    让 _ = 未定义的名字;\n}\n";
        let 入口 = 根.join("src/main.zh");
        fs::write(&入口, 源码).unwrap();

        let 映射器 = 中文映射器();
        let 转译成品 = i18n_rust_engine::转译管线(源码, &映射器);
        let 列位映射 =
            i18n_rust_engine::列位映射::列位映射表::回放生成(源码, &转译成品.流程映射);
        let (产物文本, 行映射) = 注解非母语模块并返回行映射(&转译成品.输出);
        // 磁盘产物（编译器看到的就是它）：注解插在第 2 行，其后行号 +1
        fs::write(根.join("src/main.rs"), &产物文本).unwrap();

        let 界面表 = 界面::界面消息表::按语言建表("zh");
        let 共享上下文 = 诊断上下文 {
            界面表: &界面表,
            语言包参数: &None,
            项目根: 根,
            映射器: &映射器,
            源码,
            源文件: &入口,
            入口产物: 入口.with_extension("rs"),
            列位映射: Some(&列位映射),
            入口行映射: Some(&行映射),
            项目上下文: None,
        };
        let mut 修复器 = 诊断定位修复器::新建(&共享上下文, "main.zh", "zh");
        // 编译器报磁盘第 6 行第 13 列（`未定义的名字` 首字符）
        let mut 定位项 = i18n_rust_engine::诊断::诊断位置 {
            文件名称: "src/main.rs".to_string(),
            起始行号: 6,
            起始列号: 13,
            结束行号: 6,
            结束列号: 18,
            源码文本: None,
            标签文本: None,
            是否主因: true,
        };
        修复器.修复位置(&mut 定位项);

        // 回译到母语源码：第 5 行第 11 列（母语关键字与英文关键字的列差已补回）
        assert_eq!(定位项.文件名称, "main.zh");
        assert_eq!(定位项.起始行号, 5);
        assert_eq!(定位项.起始列号, 11);
        assert_eq!(定位项.源码文本.as_deref(), Some("    让 _ = 未定义的名字;"));
    }

    /// 托管布局子模块产物（`.rz/gen/src/…`）的诊断仍须回译到母语源文件
    ///
    /// 回归：④ 生成物迁出源码目录后，编译器报的是托管产物路径，且是父目录与
    /// `#[path]` 值的未规整拼接（`src/../.rz/gen/src/数学.rs`）；剥前缀 +
    /// 按产物自身重算行映射缺一不可，否则诊断停在英文坐标。
    #[test]
    fn 定位修复器托管产物回译() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path();
        fs::create_dir_all(根.join("src/数学")).unwrap();
        fs::create_dir_all(根.join(".rz/gen/src")).unwrap();
        // 子模块自己还有子模块：产物会插入 `#[path]` 行，行映射非恒等
        fs::write(
            根.join("src/数学/工具.zh"),
            "函数 恒等(甲: 整数) -> 整数 { 甲 }\n",
        )
        .unwrap();
        let 模块源码 = "模块 工具;\n\n函数 加(甲: 整数, 乙: 整数) -> 整数 {\n    让 _ = 未定义的名字;\n    甲 + 乙\n}\n";
        let 模块文件 = 根.join("src/数学.zh");
        fs::write(&模块文件, 模块源码).unwrap();

        let 映射器 = 中文映射器();
        let 转译成品 = i18n_rust_engine::转译管线(模块源码, &映射器);
        let 产物 = i18n_rust_engine::产物布局::产物路径(根, &模块文件);
        assert_eq!(产物, 根.join(".rz/gen/src/数学.rs"));
        let 扩展名序列 = 语言管理::全部可用扩展名();
        let (产物文本, 行映射) =
            crate::注解产物(&转译成品.输出, 根, &模块文件, &产物, false, &扩展名序列);
        // 托管目录镜像源码布局：子模块之间仍与旧布局逐字一致（`数学/工具.rs`）
        assert!(
            产物文本.contains("#[path = \"数学/工具.rs\"]"),
            "{产物文本}"
        );
        fs::write(&产物, &产物文本).unwrap();
        // 源文件里报错在第 4 行，磁盘产物被 `#[path]` 顶到第 5 行
        let 磁盘行号 = 产物文本
            .lines()
            .position(|单行| 单行.contains("未定义的名字"))
            .expect("产物应含报错行")
            + 1;
        assert_eq!(磁盘行号, 5);
        assert_ne!(行映射[4], 5, "行映射应把磁盘行换算回直出行");

        let 入口 = 根.join("src/main.zh");
        fs::write(&入口, "函数 主函数() {}\n").unwrap();
        let 入口源码 = "函数 主函数() {}\n";
        let 界面表 = 界面::界面消息表::按语言建表("zh");
        let 共享上下文 = 诊断上下文 {
            界面表: &界面表,
            语言包参数: &None,
            项目根: 根,
            映射器: &映射器,
            源码: 入口源码,
            源文件: &入口,
            入口产物: 入口.with_extension("rs"),
            列位映射: None,
            入口行映射: None,
            项目上下文: None,
        };
        let mut 修复器 = 诊断定位修复器::新建(&共享上下文, "main.zh", "zh");
        let mut 定位项 = i18n_rust_engine::诊断::诊断位置 {
            文件名称: "src/../.rz/gen/src/数学.rs".to_string(),
            起始行号: 磁盘行号 as u32,
            起始列号: 13,
            结束行号: 磁盘行号 as u32,
            结束列号: 18,
            源码文本: None,
            标签文本: None,
            是否主因: true,
        };
        修复器.修复位置(&mut 定位项);

        assert_eq!(定位项.文件名称, "数学.zh");
        assert_eq!(定位项.起始行号, 4);
        assert_eq!(定位项.起始列号, 11);
        assert_eq!(定位项.源码文本.as_deref(), Some("    让 _ = 未定义的名字;"));
    }

    /// 编译输出流中提取未声明包：E0432/E0433 命中，标准库与重复项排除
    #[test]
    fn 提取未解析包规则() {
        let 首行 = r#"{"reason":"compiler-message","message":{"message":"unresolved import `serde_json`","code":{"code":"E0432"},"level":"error"}}"#;
        let 次行 = r#"{"reason":"compiler-message","message":{"message":"failed to resolve: use of undeclared crate or module `tokio`","code":{"code":"E0433"},"level":"error"}}"#;
        let 三行 = r#"{"reason":"compiler-message","message":{"message":"unresolved import `std::collections`","code":{"code":"E0432"},"level":"error"}}"#;
        let 四行 = r#"{"reason":"compiler-message","message":{"message":"unresolved import `serde_json`","code":{"code":"E0432"},"level":"error"}}"#;
        let 五行 = r#"{"reason":"compiler-message","message":{"message":"unused variable `x`","code":{"code":"E0432"},"level":"warning"}}"#;
        let 输出文本 = [首行, 次行, 三行, 四行, 五行].join("\n");
        assert_eq!(提取未解析包(&输出文本), vec!["serde_json", "tokio"]);
    }

    /// 本项目可引入的包名：包名按 Cargo 规则把 `-` 换成 `_`，库节名另计
    ///
    /// 这组名决定「`use X::…` 解析失败」该不该建议 `rzc add X`：Cargo 不允许一个包
    /// 依赖自己，命中本地名时那条命令必然失败。清单读不出来（无清单、格式
    /// 非法）时返回空，等于不改任何既有行为，而不是猜一个名字去压制正确提示。
    #[test]
    fn 本地包名集规则() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path();

        fs::write(
            根.join("Cargo.toml"),
            "[package]\nname = \"my-calc\"\nversion = \"0.1.0\"\n\n[lib]\nname = \"calc_lib\"\n",
        )
        .unwrap();
        assert_eq!(本地包名集(根), vec!["my_calc", "calc_lib"]);

        // 无库节时只有包名一种叫法
        fs::write(
            根.join("Cargo.toml"),
            "[package]\nname = \"calc\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        assert_eq!(本地包名集(根), vec!["calc"]);

        fs::write(根.join("Cargo.toml"), "[package]\nname = ").unwrap();
        assert!(本地包名集(根).is_empty(), "非法配置不该给出任何名字");
        assert!(本地包名集(&根.join("nope")).is_empty(), "无清单不该报错");
    }

    /// 流式翻译器：恐慌三行序列（含线程标识框头 → 消息 → 提示）本地化
    #[test]
    fn 流翻译器恐慌序列() {
        let 界面表 = 取测试界面("zh");
        let mut 翻译器实例 = 流翻译器::新建();
        assert_eq!(
            翻译器实例.翻译("thread 'main' (621725) panicked at a.rs:1:50:", &界面表),
            "线程 '主函数' 恐慌于 a.rs:1:50:"
        );
        assert_eq!(
            翻译器实例.翻译(
                "index out of bounds: the len is 3 but the index is 5",
                &界面表
            ),
            "下标越界：长度是 3，但下标是 5"
        );
        assert_eq!(
            翻译器实例.翻译(
                "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace",
                &界面表
            ),
            "提示：设置 RUST_BACKTRACE=1 环境变量可显示回溯"
        );
    }

    /// 框头变体：旧格式（无线程标识）与命名线程（`<unnamed>` 保留不译）
    #[test]
    fn 流翻译器框头变体() {
        let 界面表 = 取测试界面("zh");
        let mut 翻译器实例 = 流翻译器::新建();
        assert_eq!(
            翻译器实例.翻译("thread 'main' panicked at b.rs:2:3:", &界面表),
            "线程 '主函数' 恐慌于 b.rs:2:3:"
        );
        assert_eq!(
            翻译器实例.翻译("thread '<unnamed>' (42) panicked at c.rs:9:9:", &界面表),
            "线程 '<unnamed>' 恐慌于 c.rs:9:9:"
        );
        // 消息未识别（如解包无值）：原样透传，且不残留状态影响后续行
        assert_eq!(
            翻译器实例.翻译("called `Option::unwrap()` on a `None` value", &界面表),
            "called `Option::unwrap()` on a `None` value"
        );
    }

    /// 六型恐慌消息：越界起点/终点、字符边界、引用单元格借用冲突两向
    #[test]
    fn 流翻译器恐慌消息文本() {
        let 界面表 = 取测试界面("zh");
        let 框头 = "thread 'main' panicked at a.rs:1:1:";
        let mut 翻译器实例 = 流翻译器::新建();
        let _ = 翻译器实例.翻译(框头, &界面表);
        assert_eq!(
            翻译器实例.翻译(
                "start byte index 10 is out of bounds for string of length 6",
                &界面表
            ),
            "切片起点字节索引 10 超出字符串长度 6"
        );
        let mut 翻译器实例 = 流翻译器::新建();
        let _ = 翻译器实例.翻译(框头, &界面表);
        assert_eq!(
            翻译器实例.翻译(
                "end byte index 10 is out of bounds for string of length 6",
                &界面表
            ),
            "切片终点字节索引 10 超出字符串长度 6"
        );
        let mut 翻译器实例 = 流翻译器::新建();
        let _ = 翻译器实例.翻译(框头, &界面表);
        assert_eq!(
            翻译器实例.翻译(
                "start byte index 1 is not a char boundary; it is inside '你' (bytes 0..3 of string)",
                &界面表
            ),
            "字节索引 1 不是字符边界；它位于 '你'（字节 0..3）内部"
        );
        let mut 翻译器实例 = 流翻译器::新建();
        let _ = 翻译器实例.翻译(框头, &界面表);
        assert_eq!(
            翻译器实例.翻译("RefCell already borrowed", &界面表),
            "引用单元格已被借用"
        );
        let mut 翻译器实例 = 流翻译器::新建();
        let _ = 翻译器实例.翻译(框头, &界面表);
        assert_eq!(
            翻译器实例.翻译("RefCell already mutably borrowed", &界面表),
            "引用单元格已被可变借用"
        );
    }

    /// 英文界面：恐慌输出除线程标识剥离外保持英文原文（不引入本地化偏差）
    #[test]
    fn 流翻译器英文不变() {
        let 界面表 = 取测试界面("en");
        let mut 翻译器实例 = 流翻译器::新建();
        assert_eq!(
            翻译器实例.翻译("thread 'main' (621725) panicked at a.rs:1:50:", &界面表),
            "thread 'main' panicked at a.rs:1:50:"
        );
        assert_eq!(
            翻译器实例.翻译(
                "index out of bounds: the len is 3 but the index is 5",
                &界面表
            ),
            "index out of bounds: the len is 3 but the index is 5"
        );
    }

    /// 非恐慌行回退进度翻译（完成行等本地化）
    #[test]
    fn 流翻译器回退进度行() {
        let 界面表 = 取测试界面("zh");
        let mut 翻译器实例 = 流翻译器::新建();
        let 输出文本 = 翻译器实例.翻译("    Finished `dev` profile [unoptimized]", &界面表);
        assert!(输出文本.contains("编译完成"), "应本地化进度行：{输出文本}");
    }

    /// 进度五种前缀：编译/检查/完成/运行/错误摘要均本地化，
    /// 错误摘要的 `could not compile … due to N` 二段短语也翻译；
    /// 无匹配行与非「无法编译」的错误行原样透传
    #[test]
    fn 翻译进度行各前缀() {
        let 界面表 = 取测试界面("zh");
        assert!(翻译进度行("   Compiling serde v1.0.0", &界面表).contains("正在编译 serde"));
        assert!(翻译进度行("    Checking myapp v0.1.0", &界面表).contains("正在检查 myapp"));
        assert!(翻译进度行("Running `target/debug/t`", &界面表).contains("正在运行"));
        let 汇总行 = 翻译进度行(
            "error: could not compile `myapp` (bin \"myapp\") due to 2 previous errors",
            &界面表,
        );
        assert!(汇总行.contains("无法编译"), "应翻译无法编译：{汇总行}");
        assert!(汇总行.contains("2"), "应保留错误数量：{汇总行}");
        // 无法编译且无「因」尾段
        assert!(翻译进度行("error: could not compile `myapp`", &界面表).contains("无法编译"));
        // 错误行但不是「无法编译」：其余部分原样保留
        let 原样行 = 翻译进度行("error: some custom failure", &界面表);
        assert!(
            原样行.contains("some custom failure"),
            "未识别错误摘要应保留：{原样行}"
        );
        // 完全无匹配：原样返回
        assert_eq!(翻译进度行("hello world", &界面表), "hello world");
    }

    /// 英文的工具通道逐字保留构建工具自己的措辞（恒等语言不得改写英文原话）
    ///
    /// 同一条链上的五个前缀键里，`warning:`/`note:`/`help:`/`Caused by:` 本来就是
    /// 构建工具原话，只有 `cargo_progress_error 键` 曾写作大写 `Error:`、
    /// `cargo_progress_finished 键` 曾写作 `Build finished {}`——两者都发生在「构建工具的
    /// 英文行被换成另一句英文」这条路径上：英语读者对照构建工具自己的输出与文档时对不上，
    /// 英文教程第十八章贴的 `Finished dev profile …` 也和实机输出不一致。
    /// 现统一为原话，此测试把口径钉住（新增前缀键时同样要能对上）。
    #[test]
    fn 英文工具通道保留原措辞() {
        let 英文界面 = 取测试界面("en");
        let 用例集 = [
            (
                "error: could not compile `myapp`",
                "error: could not compile `myapp`",
            ),
            (
                "error: the trait bound `i32`: `MyTrait` is not satisfied",
                "error: the trait bound `i32`: `MyTrait` is not satisfied",
            ),
            (
                "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.57s",
                "Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.57s",
            ),
            ("   Compiling serde v1.0.0", "Compiling serde v1.0.0"),
            ("    Checking myapp v0.1.0", "Checking myapp v0.1.0"),
            ("Running `target/debug/t`", "Running `target/debug/t`"),
        ];
        for (输入行, 预期行) in 用例集 {
            let 实得 = 翻译进度行(输入行, &英文界面);
            assert_eq!(实得, 预期行, "英文进度行应保持原有措辞：{输入行}");
        }

        // 工具通道其余前缀（错误前缀之外的四个）同样逐字
        let 管理器 = i18n_rust_engine::诊断::错误消息管理器::从字符串载入(
            内置语言::获取内置数据("en").错误内容,
        )
        .expect("内置英文错误消息表应可加载");
        for 行文本 in [
            "warning: unused variable: `x`",
            "note: the lint level is defined here",
            "help: consider using a `let` binding",
            "Caused by: no targets specified in the manifest",
        ] {
            let 实得 =
                本地化工具行(行文本, Some(&管理器), &英文界面, "en").expect("标记行应被处理");
            assert_eq!(
                实得, 行文本,
                "英文工具通道不得改写已有前缀：{行文本} → {实得}"
            );
        }

        // 对照：中文仍走母语前缀（这条链不是只对英文生效）
        let 中文界面 = 取测试界面("zh");
        assert!(
            翻译进度行("error: some custom failure", &中文界面).starts_with("错误："),
            "中文前缀应仍为母语"
        );
        assert!(
            翻译进度行("    Finished `dev` profile", &中文界面).contains("编译完成"),
            "中文完成行应仍为母语"
        );
    }

    /// 框头无值分支：缺尾冒号 / 空线程名 / 非框头行均不匹配，原样透传
    #[test]
    fn 恐慌框头拒绝畸形() {
        let 界面表 = 取测试界面("zh");
        assert!(翻译恐慌框头("thread 'main' panicked at a.rs:1:1", &界面表).is_none());
        assert!(翻译恐慌框头("thread '' panicked at a.rs:1:1:", &界面表).is_none());
        assert!(翻译恐慌框头("not a panic line", &界面表).is_none());
        // 字节索引消息：缺尾结构时返回无（不硬译半截）
        assert!(翻译字节索引消息("10 unexpected tail", &界面表, true).is_none());
        assert!(翻译恐慌消息("totally unknown message", &界面表).is_none());
    }

    /// 源码行取值：0 行与越界返回无，正常行返回原文
    #[test]
    fn 取中文源码行边界() {
        let 源码 = "第一行\n第二行\n";
        assert_eq!(取中文源码行(源码, 0), None);
        assert_eq!(取中文源码行(源码, 1).as_deref(), Some("第一行"));
        assert_eq!(取中文源码行(源码, 3), None);
    }

    /// 产物路径解析：绝对路径原样；相对路径先查项目根；不存在再回退当前目录
    #[test]
    fn 解析产物路径顺序() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path();
        let 绝对路径 = if cfg!(windows) {
            PathBuf::from("C:/abs/main.rs")
        } else {
            PathBuf::from("/abs/main.rs")
        };
        assert_eq!(
            解析产物路径(根, &绝对路径.to_string_lossy()),
            Some(绝对路径)
        );
        fs::write(根.join("in-root.rs"), b"").unwrap();
        assert_eq!(解析产物路径(根, "in-root.rs"), Some(根.join("in-root.rs")));
        // 项目根下不存在：回退当前工作目录拼接（始终有值）
        let 实得 = 解析产物路径(根, "nowhere-12345.rs").expect("应回退当前目录");
        assert!(实得.ends_with("nowhere-12345.rs"));
    }

    /// 构造最小诊断回译上下文（列映射/行映射均为无）
    fn 最小上下文<'甲>(
        界面表: &'甲 界面::界面消息表,
        根: &'甲 Path,
        映射器: &'甲 映射管理器,
        源文件: &'甲 Path,
        源码: &'甲 str,
    ) -> 诊断上下文<'甲> {
        诊断上下文 {
            界面表,
            语言包参数: &None,
            项目根: 根,
            映射器,
            源码,
            源文件,
            入口产物: 源文件.with_extension("rs"),
            列位映射: None,
            入口行映射: None,
            项目上下文: None,
        }
    }

    /// 非方言产物（不存在的第三方 .rs）：定位保持编译器原样，绝不误标入口文件
    #[test]
    fn 修复位置非方言直传() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path();
        fs::create_dir_all(根.join("src")).unwrap();
        fs::write(根.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        fs::write(根.join("src/main.rs"), "fn main() {}\n").unwrap();
        let 入口 = 根.join("src/main.zh");
        let 映射器 = 中文映射器();
        let 界面表 = 界面::界面消息表::按语言建表("zh");
        let 共享上下文 = 最小上下文(&界面表, 根, &映射器, &入口, "函数 主函数() {}\n");
        let mut 修复器 = 诊断定位修复器::新建(&共享上下文, "main.zh", "zh");
        let 外部路径 = if cfg!(windows) {
            "C:/nonexistent-crate-9f3/src/lib.rs".to_string()
        } else {
            "/nonexistent-crate-9f3/src/lib.rs".to_string()
        };
        let mut 定位项 = i18n_rust_engine::诊断::诊断位置 {
            文件名称: 外部路径.clone(),
            起始行号: 7,
            起始列号: 3,
            结束行号: 7,
            结束列号: 9,
            源码文本: Some("orig".to_string()),
            标签文本: None,
            是否主因: true,
        };
        修复器.修复位置(&mut 定位项);
        assert_eq!(定位项.文件名称, 外部路径);
        assert_eq!(定位项.起始行号, 7);
        assert_eq!(定位项.起始列号, 3);
        assert_eq!(定位项.源码文本.as_deref(), Some("orig"));
    }

    /// 子模块方言产物（src/数学.rs）：解析回同名 .zh 源文件并重建坐标/源码行
    #[test]
    fn 修复位置子模块方言() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path();
        fs::create_dir_all(根.join("src")).unwrap();
        let 子源码 = "函数 加(甲: 整数, 乙: 整数) -> 整数 { 甲 + 乙 }\n";
        fs::write(根.join("src/数学.zh"), 子源码).unwrap();
        fs::write(根.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        let 映射器 = 中文映射器();
        let 子成品 = i18n_rust_engine::转译管线(子源码, &映射器);
        fs::write(根.join("src/数学.rs"), &子成品.输出).unwrap();
        fs::write(根.join("src/main.rs"), "fn main() {}\n").unwrap();
        let 入口 = 根.join("src/main.zh");
        let 界面表 = 界面::界面消息表::按语言建表("zh");
        let 共享上下文 = 最小上下文(&界面表, 根, &映射器, &入口, "函数 主函数() {}\n");
        let mut 修复器 = 诊断定位修复器::新建(&共享上下文, "main.zh", "zh");
        let mut 定位项 = i18n_rust_engine::诊断::诊断位置 {
            文件名称: "src/数学.rs".to_string(),
            起始行号: 1,
            起始列号: 1,
            结束行号: 1,
            结束列号: 1,
            源码文本: None,
            标签文本: None,
            是否主因: true,
        };
        修复器.修复位置(&mut 定位项);
        assert_eq!(定位项.文件名称, "数学.zh");
        assert_eq!(定位项.源码文本.as_deref(), Some(子源码.trim_end()));
    }

    /// 诊断归属按**被编译目标**而非请求文件：请求工具文件时
    /// 构建仍编译入口产物，那里的报错属于主入口，
    /// 不得因为请求的是工具文件就标成工具文件（旧实现硬编码入口为
    /// 入口产物，反过来把非入口请求的其他文件报错全部误归入口）
    #[test]
    fn 修复位置按编译目标归属() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path();
        fs::create_dir_all(根.join("src")).unwrap();
        let 主源码 = "函数 主函数() { 让 甲 = 1; }\n";
        let 工具源码 = "函数 加(甲: 整数) -> 整数 { 甲 }\n";
        fs::write(根.join("src/main.zh"), 主源码).unwrap();
        fs::write(根.join("src/main.rs"), "fn main() { let a = 1; }\n").unwrap();
        fs::write(根.join("src/工具.zh"), 工具源码).unwrap();
        fs::write(根.join("src/工具.rs"), "fn add(a: i32) -> i32 { a }\n").unwrap();
        // 请求的是工具源文件（其产物为入口），主产物的报错属于另一个目标文件
        let 入口 = 根.join("src/工具.zh");
        let 映射器 = 中文映射器();
        let 界面表 = 界面::界面消息表::按语言建表("zh");
        let 共享上下文 = 最小上下文(&界面表, 根, &映射器, &入口, 工具源码);
        let mut 修复器 = 诊断定位修复器::新建(&共享上下文, "工具.zh", "zh");
        let mut 定位项 = i18n_rust_engine::诊断::诊断位置 {
            文件名称: "src/main.rs".to_string(),
            起始行号: 1,
            起始列号: 19,
            结束行号: 1,
            结束列号: 20,
            源码文本: None,
            标签文本: None,
            是否主因: true,
        };
        修复器.修复位置(&mut 定位项);
        assert_eq!(定位项.文件名称, "main.zh", "应归到报错所在的源文件");
        assert_eq!(
            定位项.源码文本.as_deref(),
            Some(主源码.trim_end()),
            "源码行取自主入口而非工具文件"
        );
    }

    /// 未解析导入的构建 JSON：翻译器返回存在教学诊断（真），并提取未声明包
    #[test]
    fn 翻译诊断未解析导入教学() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path();
        fs::create_dir_all(根.join("src")).unwrap();
        fs::write(根.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        fs::write(根.join("src/main.rs"), "fn main() {}\n").unwrap();
        let 入口 = 根.join("src/main.zh");
        let 映射器 = 中文映射器();
        let 界面表 = 界面::界面消息表::按语言建表("zh");
        let 共享上下文 = 最小上下文(&界面表, 根, &映射器, &入口, "函数 主函数() {}\n");
        let 诊断文本 = r#"{"reason":"compiler-message","message":{"message":"unresolved import `serde_json`","code":{"code":"E0432"},"level":"error","spans":[],"children":[]}}"#;
        assert!(翻译编译诊断(
            诊断文本,
            "",
            &共享上下文,
            false,
            true,
            false
        ));
    }

    /// 项目内语言包消息表损坏：打印加载警告后降级内置表，
    /// 诊断仍正常展示（不因消息文件损坏阻断）
    #[test]
    fn 本地错误表损坏回退() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path();
        fs::create_dir_all(根.join("src")).unwrap();
        fs::create_dir_all(根.join("lang-packs/zh")).unwrap();
        fs::write(
            根.join("lang-packs/zh/errors.toml"),
            "this is = = not valid toml [[[\n",
        )
        .unwrap();
        fs::write(根.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        fs::write(根.join("src/main.rs"), "fn main() {}\n").unwrap();
        let 入口 = 根.join("src/main.zh");
        let 映射器 = 中文映射器();
        let 界面表 = 界面::界面消息表::按语言建表("zh");
        let 共享上下文 = 最小上下文(&界面表, 根, &映射器, &入口, "函数 主函数() {}\n");
        let 诊断文本 = r#"{"reason":"compiler-message","message":{"message":"unresolved import `serde_json`","code":{"code":"E0432"},"level":"error","spans":[],"children":[]}}"#;
        assert!(翻译编译诊断(
            诊断文本,
            "",
            &共享上下文,
            false,
            true,
            false
        ));
    }

    /// 无可解析诊断时：编译成功打印成功行（静默时不出声）；
    /// 编译失败输出原始构建摘要（流式时跳过，避免运行时失败误标）
    #[test]
    fn 翻译诊断空分支() {
        let 临时 = tempfile::tempdir().unwrap();
        let 根 = 临时.path();
        fs::create_dir_all(根.join("src")).unwrap();
        fs::write(根.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        fs::write(根.join("src/main.rs"), "fn main() {}\n").unwrap();
        let 入口 = 根.join("src/main.zh");
        let 映射器 = 中文映射器();
        let 界面表 = 界面::界面消息表::按语言建表("zh");
        let 共享上下文 = 最小上下文(&界面表, 根, &映射器, &入口, "函数 主函数() {}\n");
        // 成功 + 非静默
        assert!(!翻译编译诊断("", "", &共享上下文, true, false, false));
        // 成功 + 静默
        assert!(!翻译编译诊断("", "", &共享上下文, true, true, false));
        // 失败 + 非流式：输出构建摘要
        assert!(!翻译编译诊断(
            "",
            "linker `cc` not found",
            &共享上下文,
            false,
            true,
            false
        ));
        // 失败 + 流式：跳过摘要
        assert!(!翻译编译诊断(
            "",
            "should not appear",
            &共享上下文,
            false,
            true,
            true
        ));
        // 非 JSON 杂讯不构成诊断：成功路径
        assert!(!翻译编译诊断(
            "   Compiling t v0.1.0",
            "",
            &共享上下文,
            true,
            true,
            false
        ));
    }

    /// 内置中文消息表（纯文本通道与 JSON 通道共用同一张表）
    fn 中文错误管理器() -> i18n_rust_engine::诊断::错误消息管理器 {
        i18n_rust_engine::诊断::错误消息管理器::从字符串载入(
            内置语言::获取内置数据("zh").错误内容,
        )
        .expect("内置中文错误消息表应可加载")
    }

    /// 构建工具自身错误（纯文本通道）本地化：新手高频的四类都要换译
    #[test]
    fn 本地化工具行翻译编译错误() {
        let 管理器 = 中文错误管理器();
        let 界面表 = 取测试界面("zh");
        let 用例集 = [
            (
                "error: could not find `Cargo.toml` in `/home/oop` or any parent directory",
                "错误：在「/home/oop」及其上级目录中都未找到 `Cargo.toml`",
            ),
            (
                "error: failed to parse manifest at `/x/Cargo.toml`",
                "错误：解析清单文件失败：「/x/Cargo.toml」",
            ),
            (
                "error: no targets specified in the manifest",
                "错误：清单中没有指定编译目标",
            ),
            (
                "error: no matching package named `foo` found",
                "错误：找不到名为「foo」的包",
            ),
            (
                "error: current package believes it's in a workspace when it's not:",
                "错误：当前包以为自己在某个工作空间内，但实际不是：",
            ),
            // 工具链包装器未配默认工具链：新手装完 Rust 后撞上的第一句，
            // 工具名走通配段捕获，行首前缀同样要本地化
            (
                "error: rustup could not choose a version of cargo to run, because one wasn't specified explicitly, and no default is configured.",
                "错误：rustup 无法为「cargo」选出要运行的版本：未显式指定，且没有配置默认工具链。",
            ),
            (
                "help: run 'rustup default stable' to download the latest stable release of Rust and set it as your default toolchain.",
                "提示：运行「rustup default stable」下载最新的 Rust 稳定版，并把它设为默认工具链。",
            ),
        ];
        for (输入行, 预期行) in 用例集 {
            let 实得 = 本地化工具行(输入行, Some(&管理器), &界面表, "zh")
                .unwrap_or_else(|| panic!("应命中消息表：{输入行}"));
            assert_eq!(实得, 预期行, "行：{输入行}");
        }
    }

    /// 未命中的构建错误行：原文照旧输出（绝不吞诊断）+ 前缀本地化 + 登记待译
    #[test]
    fn 本地化工具行回退并记录缺口() {
        use i18n_rust_engine::诊断::待译清单;
        let 管理器 = 中文错误管理器();
        let 界面表 = 取测试界面("zh");
        // 独占语言代码：登记表是进程全局的，避免与其他并行测试互相干扰
        let 缺口语言 = "tool-gap-test-lang";
        let 输入行 = "error: unsupported subfeatures in target `x86_64-unknown-linux`";
        let 实得 = 本地化工具行(输入行, Some(&管理器), &界面表, 缺口语言).expect("标记行应被处理");
        assert_eq!(
            实得,
            format!("错误：{}", 输入行.trim_start_matches("error: ")),
            "未译句子应保留原文，仅换行首前缀"
        );
        assert!(
            待译清单::查看缺口()
                .iter()
                .any(|(语言码, channel, _, 消息原文)| 语言码 == 缺口语言
                    && channel == 待译清单::工具级
                    && 消息原文.starts_with("unsupported subfeatures")),
            "未命中应登记工具通道待译清单"
        );
        // 无消息表时完全不介入：避免把被运行程序自己打印的错误行误标
        assert_eq!(
            本地化工具行(输入行, None, &界面表, 缺口语言),
            None,
            "映射器为无时应交回原有流程"
        );
        // 已有专门处理的固定汇总句：交回进度行翻译器
        assert_eq!(
            本地化工具行(
                "error: could not compile `t` (bin \"t\") due to 1 previous error",
                Some(&管理器),
                &界面表,
                缺口语言,
            ),
            None
        );
    }

    /// 英文（恒等语言）未命中消息表：原句就是母语输出，不得登记待译
    ///
    /// 起因：英文包不携带「消息翻译」节，按消息查询在英文下永远未命中。
    /// 旧行为于是给每句英文都记一条缺口：英语用户每次报错都多看到一行
    /// 待译提示（它在英文下恒为真），同时往用户主目录的待译清单堆英文原句
    ///（实测积到 1000+ 行且永远补不动）。
    /// 对照分支拿同一张英文消息表只换登记语言：非恒等语言必须照记，
    /// 证明「不登记」只源于语言判定，不是这句消息本身被过滤。
    #[test]
    fn 恒等语言不记录缺口() {
        use i18n_rust_engine::诊断::待译清单;
        let 英文管理器 = i18n_rust_engine::诊断::错误消息管理器::从字符串载入(
            内置语言::获取内置数据("en").错误内容,
        )
        .expect("内置英文错误消息表应可加载");
        let 英文界面 = 取测试界面("en");
        let 输入行 =
            "error: crate name `计算器` passed to `--extern` is not a valid ASCII identifier";

        let 实得 =
            本地化工具行(输入行, Some(&英文管理器), &英文界面, "en").expect("标记行应被处理");
        // 正文与前缀都逐字保留构建工具原句：英文的错误前缀与
        // 同路径的警告/提示前缀一样是原话的小写形
        assert_eq!(
            实得, "error: crate name `计算器` passed to `--extern` is not a valid ASCII identifier",
            "英文下应逐字保留原有句子"
        );
        assert!(
            !待译清单::查看缺口()
                .iter()
                .any(|(语言码, _, _, 消息原文)| 语言码 == "en" && 消息原文.contains("--extern")),
            "恒等语言未命中不算待译，否则英语用户被告知去补一份不存在的译文"
        );

        // 对照：同一张表、同一句，只把登记语言换成非恒等→必须入库
        let 对照语言 = "tool-identity-contrast";
        本地化工具行(输入行, Some(&英文管理器), &英文界面, 对照语言).expect("标记行应被处理");
        assert!(
            待译清单::查看缺口()
                .iter()
                .any(|(语言码, channel, _, _)| 语言码 == 对照语言
                    && channel == 待译清单::工具级),
            "非恒等语言未命中仍应登记工具通道待译清单"
        );

        // 中文已补该条译文：命中消息表，既不回退原文也不记待译
        let 中文实得 = 本地化工具行(输入行, Some(&中文错误管理器()), &取测试界面("zh"), "zh")
            .expect("中文应命中消息表");
        assert!(
            中文实得.contains("不是合法的 ASCII 标识符"),
            "中文应给母语译文：{中文实得}"
        );
    }

    /// 构建错误链：`Caused by:` 标签行与缩进续行都要换译
    ///
    /// 构建工具把因由拆成「标记行 + 缩进行」两行输出，只处理标记行会留下
    /// 整段英文尾巴；而缩进行的查表只在命中时换译，未命中不得登记待译
    ///（缩进输出也可能是被运行程序自己的打印）。
    #[test]
    fn 本地化工具行处理原因链() {
        use i18n_rust_engine::诊断::待译清单;
        let 管理器 = 中文错误管理器();
        let 界面表 = 取测试界面("zh");
        assert_eq!(
            本地化工具行("Caused by:", Some(&管理器), &界面表, "zh").as_deref(),
            Some("原因："),
            "光杆标记行也应换标签（正文在下一行）"
        );
        assert_eq!(
            本地化工具行(
                "  failed to read `/no/such/dir/Cargo.toml`",
                Some(&管理器),
                &界面表,
                "zh"
            )
            .as_deref(),
            Some("  读取「/no/such/dir/Cargo.toml」失败"),
            "缩进续行命中时应换译并保留缩进"
        );
        let 缺口语言 = "tool-indent-gap-lang";
        assert_eq!(
            本地化工具行(
                "  some indented program output",
                Some(&管理器),
                &界面表,
                缺口语言
            ),
            None,
            "未命中的缩进行应交回原有流程"
        );
        assert!(
            !待译清单::查看缺口()
                .iter()
                .any(|(语言码, _, _, _)| 语言码 == 缺口语言),
            "缩进行未命中不得登记待译清单"
        );
    }

    /// 整段构建输出：标记行走消息表，进度行仍走进度翻译，其他行原样
    #[test]
    fn 本地化工具文本保留进度与普通行() {
        let 管理器 = 中文错误管理器();
        let 界面表 = 取测试界面("zh");
        let 原文 = "error: no targets specified in the manifest\n\
                    note: this sentence is definitely not in any message table\n\
                    未命中的普通说明行";
        let 实得 = 本地化工具文本(原文, &管理器, &界面表, "zh");
        let 行列表: Vec<&str> = 实得.lines().collect();
        assert_eq!(行列表[0], "错误：清单中没有指定编译目标");
        // 提示句不在表内：保留原文，只换前缀
        assert_eq!(
            行列表[1],
            "说明：this sentence is definitely not in any message table"
        );
        assert_eq!(行列表[2], "未命中的普通说明行");
    }
}
