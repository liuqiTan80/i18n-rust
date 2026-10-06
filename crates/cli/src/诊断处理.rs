//! CLI 侧诊断翻译子系统：cargo/rustc 编译诊断的教学化翻译管线
//!
//! 与 LSP 侧 `response_map/diag_text.rs` 是姊妹职责：从 rustc JSON 诊断
//! 解析 → 教学化翻译（错误消息 + 类型/模块路径中文化）→ 位置回译
//!（英文产物坐标 → 母语源码坐标，含子模块与 `#[path]` 注解行换算）。
//!
//! 供 `run` / `check` 子命令共用：单文件项目走 [`直调rustc运行`] /
//! [`直调rustc检查`] 直调 rustc 快速路径（绕开 cargo 索引开销），
//! 多文件或有依赖的项目回退 cargo（[`翻译cargo进度`] 翻译进度行、
//! [`翻译cargo诊断`] 翻译 JSON 诊断）。
//!
//! 模块边界：本模块只消费转译结果（源码 + 列映射 + 行映射 + 项目上下文），
//! 不负责转译编排（见 `main.zh` 的 `转译项目文件` 等）。
//!
//! 注：`std`/`serde_json` 类型与方法链走英文透传。已翻转中文 ABI（含 crate 根 `main.zh`）：
//! `crate::{按扩展名取语言代码, 语言包根目录, 解析rustc路径, 收集方言文件}`、
//! `crate::本地化::界面::全局/按语言加载/按显式目录加载/取文/取文带参/检测界面语言`、
//! `crate::内置语言::获取内置数据`、`crate::临时守护::{安全临时路径, 安全用户段}`、
//! `crate::语言包工具::{全部可用扩展名, 全局语言目录}`、
//! `i18n_rust_engine::{映射管理::映射管理器, 列映射::列映射表, 别名替换::项目上下文,
//! 模块路径::标注非西文模块并行号, 诊断::{...}, 转译管线, 转译管线静默并项目}`。
//! 所有面向用户的匹配字符串（cargo 前缀、panic 原文、ui 键、JSON/TOML 夹具、
//! 断言期望输出）逐字保真。

use crate::{
    临时守护, 内置语言, 按扩展名取语言代码, 本地化, 解析rustc路径, 语言包工具, 语言包根目录,
};
use i18n_rust_engine::映射管理::映射管理器;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 判断是否可单文件直调 rustc：src/ 下仅一个方言文件且 Cargo.toml 无依赖
///
/// 教学单文件项目（仅 main.zh）直调 rustc 绕开 cargo：无索引网络开销、
/// 无需 Cargo.lock 预生成，编译诊断格式与 cargo 完全一致；
/// 多文件（mod 引用）或有依赖的项目回退 cargo 流程。
pub(crate) fn 能否直调rustc(项目根: &Path, 入口文件: &Path) -> bool {
    // 动态方言扩展名（内置 + 用户安装），避免硬编码列表与
    // `rzc lang install` 安装的新语言包脱节（新增语言走不了快速路径）
    let 扩展名列表 = 语言包工具::全部可用扩展名();
    let 是方言文件 = |名字: &str| {
        扩展名列表
            .iter()
            .any(|后缀| 名字.ends_with(&format!(".{后缀}")))
    };
    // 方言文件计数：src/ 全层级递归（多层模块布局）+ 项目根顶层
    //（教学项目 src/main.zh 为主，项目根也可能放 main.zh）；
    // 超过 1 个视为多文件项目
    let mut 方言计数 = crate::收集方言文件(&项目根.join("src"), &扩展名列表).len();
    if let Ok(目录项列表) = fs::read_dir(项目根) {
        for 目录项 in 目录项列表.flatten() {
            if 目录项.file_type().is_ok_and(|文件类型| 文件类型.is_file()) {
                let 名字 = 目录项.file_name().to_string_lossy().to_string();
                if 是方言文件(&名字) {
                    方言计数 += 1;
                }
            }
        }
    }
    if 方言计数 != 1 {
        return false;
    }
    // 入口文件必须是 src/main.<方言扩展名>（聚合 main.rs 已写入）
    let 是主入口 = 入口文件
        .file_name()
        .and_then(|原始名| 原始名.to_str())
        .is_some_and(|名字| {
            let (主名, 后缀名) = 名字.rsplit_once('.').unwrap_or((名字, ""));
            主名 == "main" && 扩展名列表.iter().any(|后缀| 后缀 == 后缀名)
        });
    if !是主入口 {
        return false;
    }
    // Cargo.toml 的 [dependencies] 非空（有第三方依赖）时回退 cargo
    let 库构建清单 = 项目根.join("Cargo.toml");
    if let Ok(内容) = fs::read_to_string(&库构建清单)
        && let Some(依赖段) = 内容.split("[dependencies]").nth(1)
    {
        // 依赖行形如 `rand = "0.8"`；注释/空行/子表头不算依赖
        let 有依赖 = 依赖段.lines().any(|每行| {
            let 去空格 = 每行.trim();
            !去空格.is_empty()
                && !去空格.starts_with('#')
                && !去空格.starts_with('[')
                && 去空格.contains('=')
        });
        if 有依赖 {
            return false;
        }
    }
    true
}

/// 单文件直调 rustc 运行：编译（--error-format=json）→ 翻译诊断 → 运行 exe
#[allow(clippy::too_many_arguments)]
pub(crate) fn 直调rustc运行(
    界面: &本地化::界面,
    项目根: &Path,
    源码路径: &Path,
    语言包: &Option<PathBuf>,
    管理器: &映射管理器,
    源码: &str,
    入口文件: &Path,
    列映射: &i18n_rust_engine::列映射::列映射表,
    入口行映射: &[usize],
    项目声明: &i18n_rust_engine::别名替换::项目上下文,
) -> anyhow::Result<std::process::ExitCode> {
    let 可执行文件 = 临时守护::安全临时路径(&format!(
        "rzc-run-{}-{}.exe",
        临时守护::安全用户段(),
        std::process::id()
    ))?;
    let 命令输出 = Command::new(解析rustc路径())
        .args(["--edition", "2024", "--error-format=json"])
        .arg(源码路径)
        .arg("-o")
        .arg(&可执行文件)
        .output()
        .map_err(|错| anyhow::anyhow!("rustc 启动失败: {错}"))?;
    let 错误文本 = String::from_utf8_lossy(&命令输出.stderr).to_string();
    let 编译器输出 = format!(
        "{}\n{}",
        String::from_utf8_lossy(&命令输出.stdout),
        错误文本
    );
    let 是否成功 = 命令输出.status.success();
    if !编译器输出.trim().is_empty() {
        let _ = 翻译cargo诊断(
            &编译器输出,
            &错误文本,
            &诊断上下文 {
                界面,
                语言包,
                项目根,
                管理器,
                源码,
                入口文件,
                列映射: Some(列映射),
                入口行映射: Some(入口行映射),
                项目: Some(项目声明),
            },
            是否成功,
            true,
            // 直调 rustc：编译失败文本由本函数输出（非流式透传）
            false,
        );
    }
    if !是否成功 {
        return Ok(std::process::ExitCode::FAILURE);
    }
    // 编译成功：运行程序并传播退出码（无论成败都清理临时 exe）。
    // stderr 改为管道逐行过滤：panic 框头/消息与 cargo 进度本地化，
    // 其余字节原样透传（lossy 解码，不因非 UTF-8 输出中断）；
    // stdout/stdin 保持继承，交互式程序不受影响
    let mut 子进程 = match Command::new(&可执行文件)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(子) => 子,
        Err(错) => {
            let _ = std::fs::remove_file(&可执行文件);
            return Err(anyhow::anyhow!("运行失败: {错}"));
        }
    };
    if let Some(错误流) = 子进程.stderr.take() {
        use std::io::{BufRead, BufReader};
        let mut 读取器 = BufReader::new(错误流);
        let mut 流翻译器 = 流式翻译器::新建();
        let mut 缓冲区: Vec<u8> = Vec::new();
        loop {
            缓冲区.clear();
            match 读取器.read_until(b'\n', &mut 缓冲区) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let 原始文本 = String::from_utf8_lossy(&缓冲区);
                    let 每行 = 原始文本.trim_end_matches(['\n', '\r']);
                    eprintln!("{}", 流翻译器.翻译一行(每行, 界面));
                }
            }
        }
    }
    let 退出状态 = match 子进程.wait() {
        Ok(状态) => 状态,
        Err(错) => {
            let _ = std::fs::remove_file(&可执行文件);
            return Err(anyhow::anyhow!("运行失败: {错}"));
        }
    };
    let _ = std::fs::remove_file(&可执行文件);
    Ok(退出状态
        .code()
        .map(|返回码| std::process::ExitCode::from(返回码 as u8))
        .unwrap_or(std::process::ExitCode::FAILURE))
}

/// 单文件直调 rustc 检查：编译（--emit=metadata，不生成可执行文件）
#[allow(clippy::too_many_arguments)]
pub(crate) fn 直调rustc检查(
    界面: &本地化::界面,
    项目根: &Path,
    源码路径: &Path,
    语言包: &Option<PathBuf>,
    管理器: &映射管理器,
    源码: &str,
    入口文件: &Path,
    列映射: &i18n_rust_engine::列映射::列映射表,
    入口行映射: &[usize],
    项目声明: &i18n_rust_engine::别名替换::项目上下文,
) -> anyhow::Result<std::process::ExitCode> {
    let 命令输出 = Command::new(解析rustc路径())
        .args([
            "--edition",
            "2024",
            "--error-format=json",
            "--emit=metadata",
        ])
        .arg(源码路径)
        .output()
        .map_err(|错| anyhow::anyhow!("rustc 启动失败: {错}"))?;
    let 退出码 = if 命令输出.status.success() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    };
    let 错误文本 = String::from_utf8_lossy(&命令输出.stderr).to_string();
    let 编译器输出 = format!(
        "{}\n{}",
        String::from_utf8_lossy(&命令输出.stdout),
        错误文本
    );
    let _ = 翻译cargo诊断(
        &编译器输出,
        &错误文本,
        &诊断上下文 {
            界面,
            语言包,
            项目根,
            管理器,
            源码,
            入口文件,
            列映射: Some(列映射),
            入口行映射: Some(入口行映射),
            项目: Some(项目声明),
        },
        命令输出.status.success(),
        false,
        // 直调 rustc 检查：诊断输出由本函数负责（非流式透传）
        false,
    );
    Ok(退出码)
}

/// 翻译 cargo 的人类可读进度行（json 模式下这些行仍输出到 stderr）
///
/// 命中固定前缀（Compiling/Finished/Running 等）时翻译；
/// 其余行（程序 stderr 等）原样返回。
pub(crate) fn 翻译cargo进度(每行: &str, 界面: &本地化::界面) -> String {
    // cargo 进度行带行首缩进（如 "   Compiling ..."），先去除空白再匹配前缀
    let 已去除 = 每行.trim_start();
    for (前缀, 消息键) in [
        ("Compiling ", "cargo_progress_compiling"),
        ("Checking ", "cargo_progress_checking"),
        ("Finished ", "cargo_progress_finished"),
        ("Running ", "cargo_progress_running"),
        ("error: ", "cargo_progress_error"),
    ] {
        if let Some(余文本) = 已去除.strip_prefix(前缀) {
            // error 摘要（如 "could not compile `__` due to N previous error"）
            // 二次翻译固定短语，其余保留原文
            if 消息键 == "cargo_progress_error" {
                let 余文本 = 余文本
                    .strip_prefix("could not compile ")
                    .map(|余| {
                        // 尾部 "due to N previous errors" 一并本地化：
                        // 如 `abc` (bin "abc") due to 9 previous errors
                        let 已去除 = 余.trim_start();
                        match 已去除.split_once(" due to ") {
                            Some((主体部分, 尾部部分)) => {
                                let 数量 = 尾部部分.split_whitespace().next().unwrap_or("");
                                format!(
                                    "{}{}",
                                    界面.取文带参(
                                        "cargo_progress_could_not_compile",
                                        &[主体部分]
                                    ),
                                    界面.取文带参("cargo_progress_due_to", &[数量])
                                )
                            }
                            None => {
                                界面.取文带参("cargo_progress_could_not_compile", &[已去除])
                            }
                        }
                    })
                    .unwrap_or_else(|| 余文本.to_string());
                return 界面.取文带参(消息键, &[&余文本]);
            }
            return 界面.取文带参(消息键, &[余文本.trim_start()]);
        }
    }
    每行.to_string()
}

/// 程序 stderr 流式翻译器：cargo 进度行 + 运行时 panic 输出的本地化
///
/// rustc 1.98 实测的程序 panic 输出形态（框头 → 消息 → note 三行）：
/// ```text
/// thread 'main' (621725) panicked at src/main.rs:8:5:
/// index out of bounds: the len is 3 but the index is 5
/// note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
/// ```
/// 框头：线程名 `main` 映射为各语言主函数词（`<unnamed>` 等自定义名保留）、
/// 线程 ID `(621725) ` 剥离；消息六型（下标越界 / 字节索引越界 ×2 /
/// 字符边界 / RefCell 借用冲突 ×2）按 ui.toml 模板本地化；note 行固定翻译；
/// 其余行回退 cargo 进度翻译（无匹配则原样返回）。
pub(crate) struct 流式翻译器 {
    /// 上一行是 panic 框头：下一行按 panic 消息匹配
    等待恐慌消息: bool,
}

impl 流式翻译器 {
    pub(crate) fn 新建() -> Self {
        Self {
            等待恐慌消息: false,
        }
    }

    /// 翻译一行程序/cargo stderr（无匹配时原样返回）
    pub(crate) fn 翻译一行(&mut self, 每行: &str, 界面: &本地化::界面) -> String {
        // 框头之后的第一行是 panic 消息；无论命中与否都退出该状态
        if std::mem::take(&mut self.等待恐慌消息)
            && let Some(消息) = 翻译恐慌消息(每行, 界面)
        {
            return 消息;
        }
        if let Some(框头) = 翻译恐慌框头(每行, 界面) {
            self.等待恐慌消息 = true;
            return 框头;
        }
        if 每行.trim_end() == 恐慌提示行 {
            return 界面.取文("panic_note");
        }
        翻译cargo进度(每行, 界面)
    }
}

/// 程序 panic 输出末尾的 note 行（rustc 稳定文本，原样匹配）
const 恐慌提示行: &str =
    "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace";

/// 翻译 panic 框头 `thread 'NAME' [(TID)] panicked at FILE:LINE:COL:`
///
/// 返回 None 表示该行不是 panic 框头（由调用方原样透传）。
fn 翻译恐慌框头(每行: &str, 界面: &本地化::界面) -> Option<String> {
    let 余文本 = 每行.trim_start().strip_prefix("thread '")?;
    let (线程名, 名余文本) = 余文本.split_once('\'')?;
    // rustc 1.98 起框头含线程 ID（如 `thread 'main' (621725) panicked at ...`），
    // 旧格式无 ID；两种形态都归一为「name + panicked at」
    let 之后文本 = match 名余文本.strip_prefix(" (") {
        Some(标识与余下) => 标识与余下
            .split_once(") ")
            .map(|(_, 余下)| 余下)
            .unwrap_or(名余文本),
        None => 名余文本,
    };
    // 剥离线程 ID 后 `after` 以 `panicked at ...` 开头（无 ID 时带前导空格），
    // trim_start 统一两种形态后再匹配
    let 定位 = 之后文本.trim_start().strip_prefix("panicked at ")?;
    let 定位 = 定位.trim_end().strip_suffix(':')?;
    if 线程名.is_empty() || 定位.is_empty() {
        return None;
    }
    let 显示名称 = if 线程名 == "main" {
        界面.取文("panic_thread_main")
    } else {
        线程名.to_string()
    };
    Some(界面.取文带参("panic_header", &[&显示名称, 定位]))
}

/// 翻译 panic 消息（六型；未识别返回 None 由调用方原样透传）
fn 翻译恐慌消息(每行: &str, 界面: &本地化::界面) -> Option<String> {
    let 每行 = 每行.trim();
    if 每行 == "RefCell already borrowed" {
        return Some(界面.取文("panic_msg_borrow"));
    }
    if 每行 == "RefCell already mutably borrowed" {
        return Some(界面.取文("panic_msg_borrow_mut"));
    }
    if let Some(余文本) = 每行.strip_prefix("index out of bounds: the len is ") {
        let (长度文本, 下标文本) = 余文本.split_once(" but the index is ")?;
        return Some(界面.取文带参("panic_msg_index_oob", &[长度文本.trim(), 下标文本.trim()]));
    }
    if let Some(余文本) = 每行.strip_prefix("start byte index ") {
        return 翻译字节索引消息(余文本, 界面, true);
    }
    if let Some(余文本) = 每行.strip_prefix("end byte index ") {
        return 翻译字节索引消息(余文本, 界面, false);
    }
    None
}

/// 字节索引 panic 消息：越界（`... is out of bounds for string of length N`）或
/// 非字符边界（`... is not a char boundary; it is inside 'C' (bytes A..B of string)`）
fn 翻译字节索引消息(
    余文本: &str, 界面: &本地化::界面, 是起点: bool
) -> Option<String> {
    let (下标文本, 尾部) = 余文本.split_once(' ')?;
    if let Some(长度文本) = 尾部.strip_prefix("is out of bounds for string of length ") {
        let 消息键 = if 是起点 {
            "panic_msg_byte_start_oob"
        } else {
            "panic_msg_byte_end_oob"
        };
        return Some(界面.取文带参(消息键, &[下标文本.trim(), 长度文本.trim()]));
    }
    if let Some(内部文本) = 尾部.strip_prefix("is not a char boundary; it is inside '") {
        let (char, 范围文本) = 内部文本.split_once("' (bytes ")?;
        let (甲, 乙) = 范围文本.split_once("..")?;
        let 乙 = 乙.strip_suffix(" of string)")?;
        return Some(界面.取文带参(
            "panic_msg_char_boundary",
            &[下标文本.trim(), char, 甲.trim(), 乙.trim()],
        ));
    }
    None
}

/// 诊断翻译上下文：封装 `翻译cargo诊断` 的共享引用参数，
/// 避免 8+ 位置参数的可读性灾难
pub(crate) struct 诊断上下文<'a> {
    pub(crate) 界面: &'a 本地化::界面,
    pub(crate) 语言包: &'a Option<PathBuf>,
    pub(crate) 项目根: &'a Path,
    pub(crate) 管理器: &'a 映射管理器,
    pub(crate) 源码: &'a str,
    pub(crate) 入口文件: &'a Path,
    /// 转译产物的列映射：把 rustc 诊断的（英文产物）列号回译到母语源码列号。
    ///
    /// rustc 看到的是转译后的英文源码，其列号对母语源码无效
    ///（如 `让` → `let` 后整行右移）。`无` 表示不做映射，保持旧行为。
    pub(crate) 列映射: Option<&'a i18n_rust_engine::列映射::列映射表>,
    /// 入口磁盘产物行 → 引擎直出行映射（见
    /// [`标注非西文模块并行号`](i18n_rust_engine::模块路径::标注非西文模块并行号)）。
    ///
    /// 入口产物写盘前经 `#[path]` 注解插入整行（每个非 ASCII `模块 xxx;`
    /// 一行），而 `列映射` 以未注解的引擎直出产物为基准；
    /// 回译前须先用本映射把 rustc 的磁盘行号换算回引擎直出行号。
    /// `无` 表示无注解（磁盘产物与引擎直出逐行一致）。
    pub(crate) 入口行映射: Option<&'a [usize]>,
    /// 项目级声明上下文：诊断重放（[`解析方言上下文`]）须与
    /// 写盘转译共享同一上下文，否则列映射与磁盘产物不一致（#8）
    pub(crate) 项目: Option<&'a i18n_rust_engine::别名替换::项目上下文>,
}

/// 解析 cargo --message-format=json 输出并翻译为教学化诊断（check 与 run 共用）
///
/// 返回是否成功输出了翻译后的教学诊断；调用方据此决定是否回退原始文本。
/// `cargo成功=false` 且无可解析诊断时原样输出 cargo 消息，绝不虚报“编译成功”。
/// `静默成功=true`（run 场景）时，无诊断且编译成功保持静默——
/// 程序已运行，不再提示“编译成功”。
/// `已流式输出=true`（cargo run 场景）时，cargo 消息与程序输出均已实时
/// 透传：「失败且无可解析诊断」不再补打“编译错误”标签——构建可能已成功，
/// 失败发生在程序运行阶段（如 panic 退出），补打空标签会误导。
pub(crate) fn 翻译cargo诊断(
    编译器输出: &str,
    错误文本: &str,
    上下文: &诊断上下文<'_>,
    cargo成功: bool,
    静默成功: bool,
    已流式输出: bool,
) -> bool {
    use i18n_rust_engine::诊断::{
        构建类型映射, 解析诊断输出, 诊断翻译器, 错误翻译管理器
    };
    let 界面 = 上下文.界面;
    let 语言包 = 上下文.语言包;
    let 项目根 = 上下文.项目根;
    let 管理器 = 上下文.管理器;
    let 入口文件 = 上下文.入口文件;

    // 未解析导入提取：诊断展示后附带 `rzc add` 加依赖提示（教学化引导）
    let 未解析包名列表 = 提取未解析包名(编译器输出);

    // 按语言代码选择错误消息：--lang-pack 目录 > 项目内 lang-packs/<lang>/ > 内置
    let 语言代码 = 入口文件
        .extension()
        .and_then(|后缀| 后缀.to_str())
        .and_then(按扩展名取语言代码)
        .unwrap_or_else(本地化::检测界面语言);
    let 错误消息路径 = if let Some(路径) = 语言包 {
        路径.join("errors.toml")
    } else if 语言包根目录(项目根)
        .join(&语言代码)
        .join("errors.toml")
        .exists()
    {
        语言包根目录(项目根).join(&语言代码).join("errors.toml")
    } else {
        语言包工具::全局语言目录()
            .join(&语言代码)
            .join("errors.toml")
    };
    // 类型映射（英文 → 母语）：供诊断消息中的类型/特征名中文化（如
    // `std::fmt::Display` → `标准库::格式化::可显示`）。构建逻辑下沉引擎，
    // 与 LSP 共用同一来源，避免两侧漂移（keywords [类型] 反转为主，别名/模块路径补充）。
    let 反向映射 = 构建类型映射(管理器);
    let 翻译器 = if 错误消息路径.exists() {
        // 加载失败时降级到内置表，不因错误消息文件损坏阻断诊断展示
        match 错误翻译管理器::自文件加载(&错误消息路径) {
            Ok(错误管理器) => Some(诊断翻译器::新建翻译器(
                错误管理器,
                反向映射.clone(),
            )),
            Err(错) => {
                eprintln!(
                    "{}",
                    界面.取文带参("load_error_msg_failed", &[&错.to_string()])
                );
                None
            }
        }
    } else {
        None
    };
    // 文件路径不可用/加载失败时回退内置语言包（未知语言代码自动回退中文）
    let 翻译器 = 翻译器.or_else(|| {
        let 内置 = 内置语言::获取内置数据(&语言代码);
        match 错误翻译管理器::从字符串载入(内置.错误文本) {
            Ok(错误管理器) => Some(诊断翻译器::新建翻译器(
                错误管理器,
                反向映射.clone(),
            )),
            Err(错) => {
                eprintln!(
                    "{}",
                    界面.取文带参("warn_builtin_errors_failed", &[&错.to_string()])
                );
                None
            }
        }
    });

    let 原始文件名 = 入口文件
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let mut 诊断列表 = 解析诊断输出(编译器输出);
    // 保留 error/warning；不要求有错误码——无码的解析错误（如缺括号）
    // 也必须显示，否则会被静默吞掉导致假“编译成功”
    诊断列表.retain(|诊断项| 诊断项.诊断级别 == "error" || 诊断项.诊断级别 == "warning");
    // rustc 的汇总元消息（"aborting due to N previous error(s)"）：无代码位置、
    // 无教学内容，具体错误已逐条列出；省略它可以避免英文残句与伪“错误”行
    // 混入诊断列表（数字无法用消息表占位符捕获，故不在翻译层处理）
    诊断列表.retain(|诊断项| !诊断项.诊断消息.starts_with("aborting due to "));
    let mut 已见错误码 = std::collections::HashSet::new();
    诊断列表.retain(|诊断项| {
        if let Some(ref 错误码) = 诊断项.诊断码 {
            已见错误码.insert(错误码.码值.clone())
        } else {
            true // 无错误码的诊断（解析错误等）不去重，直接保留
        }
    });

    if 诊断列表.is_empty() {
        if cargo成功 {
            if !静默成功 {
                println!("{}", 界面.取文("success_compile"));
            }
        } else if !已流式输出 {
            // cargo 失败但无可解析的 JSON 诊断（Cargo.toml 语法错误、
            // 链接错误等）：原样输出 cargo 消息，绝不虚报“编译成功”；
            // 已流式输出场景跳过（见函数文档，避免运行时失败被误标）
            eprintln!("{}", 界面.取文带参("compile_error", &[错误文本.trim()]));
        }
        打印依赖提示(&未解析包名列表, 界面);
        return false;
    }

    if let Some(ref 翻译器) = 翻译器 {
        let mut 教学列表 = 翻译器.批量翻译(&诊断列表);
        let mut 已见教学码 = std::collections::HashSet::new();
        教学列表.retain(|教学| {
            教学
                .错误码
                .as_ref()
                .is_none_or(|错误码| 已见教学码.insert(错误码.clone()))
        });
        // 诊断定位回译：入口产物（src/main.rs）用调用方传入的源码与列映射；
        // 多文件项目的子模块产物（如 src/接口.rs）解析回母语源文件
        // （src/接口.zh）后用其源码与列映射单独回译——修复子模块错误被统一
        // 误标为入口文件且源码行/列号错位的问题。
        let 方言后缀 = 入口文件
            .extension()
            .and_then(|后缀| 后缀.to_str())
            .unwrap_or("zh");
        let mut 回译者 = 诊断定位回译器::新建(上下文, &原始文件名, 方言后缀);
        for 教学 in &mut 教学列表 {
            回译者.回译位置列表(教学);
        }
        if 教学列表.is_empty() {
            if cargo成功 {
                if !静默成功 {
                    println!("{}", 界面.取文("success_compile"));
                }
            } else if !已流式输出 {
                eprintln!("{}", 界面.取文带参("compile_error", &[错误文本.trim()]));
            }
            打印依赖提示(&未解析包名列表, 界面);
            return false;
        }
        println!(
            "{}",
            i18n_rust_engine::诊断::教学诊断::批量格式化为文本(&教学列表)
        );
        打印依赖提示(&未解析包名列表, 界面);
        true
    } else {
        // 无翻译表：输出 JSON 中的原始 message，保证诊断不丢失
        for 每行 in 编译器输出.lines() {
            if let Ok(原始) = serde_json::from_str::<serde_json::Value>(每行) {
                if let Some(消息) = 原始.get("message") {
                    println!("{}", 消息.as_str().unwrap_or(""));
                }
            } else if !每行.trim().is_empty() {
                println!("{}", 每行);
            }
        }
        打印依赖提示(&未解析包名列表, 界面);
        false
    }
}

/// 诊断文件上下文：rustc 报告的（英文产物）文件解析回母语源文件后
/// 得到的显示名、源码与列映射，供行号/列号/源行统一回译
struct 诊断文件上下文 {
    显示名称: String,
    源码: String,
    列映射: i18n_rust_engine::列映射::列映射表,
}

/// 解析 rustc 报告的诊断文件路径为实际路径：
/// 绝对路径原样；相对路径先相对项目根（cargo 的 cwd），再相对当前目录。
/// 无法定位时返回 None。
fn 解析产物路径(项目根: &Path, 名字: &str) -> Option<PathBuf> {
    let 待解析 = Path::new(名字);
    if 待解析.is_absolute() {
        return Some(待解析.to_path_buf());
    }
    let 来自根 = 项目根.join(待解析);
    if 来自根.exists() {
        return Some(来自根);
    }
    std::env::current_dir().ok().map(|目录| 目录.join(待解析))
}

/// 把 rustc 报告的诊断文件（转译产物路径）解析为母语源文件上下文。
///
/// 多文件项目中 rustc 报告的是子模块转译产物（如 src/接口.rs），其行列是
/// 英文产物的坐标、源码行也是英文；须找到同名方言源文件（src/接口.zh）并用
/// 其源码重建列映射。对应方言文件不存在（手写 .rs、第三方库源码等）时
/// 返回 None，调用方保持 rustc 原始定位，绝不误标成入口文件。
fn 解析方言上下文(
    项目根: &Path,
    方言后缀: &str,
    产物文件: &str,
    管理器: &映射管理器,
    项目: Option<&i18n_rust_engine::别名替换::项目上下文>,
) -> Option<诊断文件上下文> {
    let 绝对 = 解析产物路径(项目根, 产物文件)?;
    // 产物名以 .rs 结尾时换成方言扩展名（src/接口.rs → src/接口.zh）
    let 源码路径 = if 绝对.extension().is_some_and(|后缀| 后缀 == "rs") {
        绝对.with_extension(方言后缀)
    } else {
        绝对
    };
    let 源码 = fs::read_to_string(&源码路径).ok()?;
    // 现场重放转译管线取列映射：与写盘时同一管线（同一项目上下文，
    // 确定性输出），保证列号回译与磁盘上的转译产物一致；静默重放：
    // 教学告警（lint/全角/Unicode）已在写盘转译时输出过，此处重放不得重复告警
    let 已转译 = i18n_rust_engine::转译管线静默并项目(&源码, 管理器, 项目);
    let 列映射 = i18n_rust_engine::列映射::列映射表::r#构建(&源码, &已转译.管线映射);
    Some(诊断文件上下文 {
        显示名称: 源码路径
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        源码,
        列映射,
    })
}

/// 诊断定位回译器：把教学诊断的位置从英文产物坐标回译到母语源码坐标
///
/// 入口产物用调用方传入的源码/列映射；项目内其他方言文件的转译产物按
/// 文件名解析回母语源文件，用其源码与列映射单独回译；非方言文件保持
/// rustc 原始定位。
pub(crate) struct 诊断定位回译器<'a> {
    /// 入口文件的母语源码
    源码: &'a str,
    /// 入口产物的列映射（rustc 列号 → 母语列号）
    列映射: Option<&'a i18n_rust_engine::列映射::列映射表>,
    /// 入口磁盘产物行 → 引擎直出行映射（含 `#[path]` 注解插入行时非恒等）
    入口行映射: Option<&'a [usize]>,
    项目根: &'a Path,
    管理器: &'a 映射管理器,
    /// 入口文件名（如 main.zh）
    入口文件名: &'a str,
    /// 入口产物的规范路径（src/main.rs），用于识别入口产物的诊断
    入口规范路径: Option<PathBuf>,
    /// 入口文件的方言扩展名（如 zh），用于把产物 .rs 还原为源文件
    方言后缀: &'a str,
    /// 项目级声明上下文（诊断重放与写盘转译一致，见 [`诊断上下文::项目`]）
    项目: Option<&'a i18n_rust_engine::别名替换::项目上下文>,
    /// 诊断文件 → 母语源文件上下文缓存（None 表示非方言文件）
    文件上下文: HashMap<String, Option<诊断文件上下文>>,
}

impl<'a> 诊断定位回译器<'a> {
    pub(crate) fn 新建(
        上下文: &'a 诊断上下文<'a>,
        入口文件名: &'a str,
        方言后缀: &'a str,
    ) -> Self {
        Self {
            源码: 上下文.源码,
            列映射: 上下文.列映射,
            入口行映射: 上下文.入口行映射,
            项目根: 上下文.项目根,
            管理器: 上下文.管理器,
            入口文件名,
            入口规范路径: 上下文.项目根.join("src/main.rs").canonicalize().ok(),
            方言后缀,
            项目: 上下文.项目,
            文件上下文: HashMap::new(),
        }
    }

    /// 回译一条教学诊断的全部位置（含子诊断）
    fn 回译位置列表(&mut self, 教学: &mut i18n_rust_engine::诊断::教学诊断) {
        for 定位 in &mut 教学.位置列表 {
            self.回译定位(定位);
        }
        for 子诊断 in &mut 教学.子诊断 {
            self.回译位置列表(子诊断);
        }
    }

    pub(crate) fn 回译定位(&mut self, 定位: &mut i18n_rust_engine::诊断::诊断位置) {
        if self.是入口产物(&定位.源文件名) {
            定位.源文件名 = self.入口文件名.to_string();
            // 先把 rustc 的（英文产物）行列回译到母语源码坐标，
            // 再用回译后的行号取源码行——顺序不可颠倒，否则源码行与列号错位。
            if let Some(列映射) = self.列映射 {
                // 磁盘产物含 `#[path]` 注解插入行时，行号先换算回引擎直出行
                //（列映射以引擎直出产物为基准，1-based 换算后传入）
                let 引擎行 = self
                    .入口行映射
                    .and_then(|映射表| 映射表.get(定位.起始行.saturating_sub(1) as usize).copied())
                    .map(|引擎| 引擎 as u32 + 1)
                    .unwrap_or(定位.起始行);
                let (行号, 列号) = 列映射.映射位置(引擎行, 定位.起始列);
                定位.起始行 = 行号;
                定位.起始列 = 列号;
            }
            定位.源码文本 = 取母语源码行(self.源码, 定位.起始行);
            return;
        }
        let 上下文 = self
            .文件上下文
            .entry(定位.源文件名.clone())
            .or_insert_with(|| {
                解析方言上下文(
                    self.项目根,
                    self.方言后缀,
                    &定位.源文件名,
                    self.管理器,
                    self.项目,
                )
            });
        let Some(上下文) = 上下文 else {
            // 非方言文件（手写 .rs、第三方源码等）：保持 rustc 原始定位
            return;
        };
        定位.源文件名 = 上下文.显示名称.clone();
        let (行号, 列号) = 上下文.列映射.映射位置(定位.起始行, 定位.起始列);
        定位.起始行 = 行号;
        定位.起始列 = 列号;
        定位.源码文本 = 取母语源码行(&上下文.源码, 定位.起始行);
    }

    /// 判断诊断文件是否为入口文件的转译产物（src/main.rs）
    fn 是入口产物(&self, 名字: &str) -> bool {
        let Some(入口规范路径) = self.入口规范路径.as_ref() else {
            return false;
        };
        解析产物路径(self.项目根, 名字)
            .and_then(|绝对| 绝对.canonicalize().ok())
            .is_some_and(|绝对| &绝对 == 入口规范路径)
    }
}

/// 从 cargo --message-format=json 输出提取未声明的 crate 名
///
/// 识别 unresolved import（E0432）与 failed to resolve（E0433）诊断，
/// 候选提取复用 engine 共享逻辑（已去重，排除标准库与保留路径）。
pub(crate) fn 提取未解析包名(编译器输出: &str) -> Vec<String> {
    use i18n_rust_engine::诊断::{是未解析导入消息, 未解析包候选};
    let mut 提取结果: Vec<String> = Vec::new();
    for 每行 in 编译器输出.lines() {
        let Ok(条目) = serde_json::from_str::<serde_json::Value>(每行) else {
            continue;
        };
        // cargo JSON 流中诊断嵌套在 compiler-message 条目里；
        // 兼容直接的诊断对象两种形态
        let 消息对象 = if 条目.get("reason").is_some() {
            条目.get("message")
        } else {
            Some(&条目)
        };
        let Some(消息) = 消息对象 else { continue };
        let 级别 = 消息.get("level").and_then(|值| 值.as_str()).unwrap_or("");
        if 级别 != "error" {
            continue;
        }
        let 错误码 = 消息
            .get("code")
            .and_then(|码对象| 码对象.get("code"))
            .and_then(|值| 值.as_str())
            .unwrap_or("");
        let 文本内容 = 消息.get("message").and_then(|值| 值.as_str()).unwrap_or("");
        if !matches!(错误码, "E0432" | "E0433") && !是未解析导入消息(文本内容) {
            continue;
        }
        for 段 in 未解析包候选(文本内容) {
            if !提取结果.contains(&段) {
                提取结果.push(段);
            }
        }
        // 码命中但消息文本未命中时（消息格式变化），退化到纯首段提取
        if matches!(错误码, "E0432" | "E0433") && 未解析包候选(文本内容).is_empty() {
            use i18n_rust_engine::诊断::抽取反引号首段;
            for 段 in 抽取反引号首段(文本内容) {
                // 非 ASCII 段不是 crate 名（母语标识符误提取），
                // 与 未解析包候选 的过滤保持一致
                if !matches!(
                    段.as_str(),
                    "std" | "core" | "alloc" | "self" | "super" | "crate" | "proc_macro"
                ) && !段
                    .chars()
                    .next()
                    .is_some_and(|每字符| 每字符.is_ascii_digit())
                    && 段.is_ascii()
                    && !提取结果.contains(&段)
                {
                    提取结果.push(段);
                }
            }
        }
    }
    提取结果
}

/// 输出未声明依赖的 `rzc add` 提示（无候选时静默）
fn 打印依赖提示(包名列表: &[String], 界面: &本地化::界面) {
    for 名字 in 包名列表 {
        eprintln!("{}", 界面.取文带参("hint_add_dependency", &[名字, 名字]));
    }
}

fn 取母语源码行(源码: &str, 行号: u32) -> Option<String> {
    if 行号 == 0 {
        return None;
    }
    源码
        .lines()
        .nth((行号 - 1) as usize)
        .map(|每行| 每行.to_string())
}

#[cfg(test)]
mod 单元测试 {
    use super::*;
    use i18n_rust_engine::模块路径::标注非西文模块并行号;

    /// 测试用界面消息：直接读仓库语言包（绕开 ~/.rz 全局安装副本的
    /// 缺键干扰，保证断言与仓库内容一致）
    fn 界面供测试(语言: &str) -> crate::本地化::界面 {
        let 目录 = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../engine/lang-packs")
            .join(语言);
        crate::本地化::界面::按显式目录加载(&目录)
    }

    /// 加载内置中文映射管理器（测试诊断回译用）
    fn 中文管理器() -> i18n_rust_engine::映射管理::映射管理器 {
        let 内置 = crate::内置语言::获取内置数据("zh");
        i18n_rust_engine::映射管理::映射管理器::自内置加载(
            内置.关键字文本,
            内置.模块路径文本,
            内置.标准库文本,
            内置.三方库数据,
        )
        .expect("内置中文语言包应可加载")
    }

    /// 单文件直调 rustc 的启用条件：仅 main.zh 且无依赖
    #[test]
    fn 测试能否直调rustc单文件无依赖() {
        let 目录 = tempfile::tempdir().unwrap();
        let 根 = 目录.path();
        std::fs::create_dir_all(根.join("src")).unwrap();
        std::fs::write(
            根.join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(根.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        assert!(能否直调rustc(根, &根.join("src/main.zh")));
    }

    /// 有第三方依赖时回退 cargo（依赖行 `rand = \"0.8\"`）
    #[test]
    fn 测试能否直调rustc依赖回退() {
        let 目录 = tempfile::tempdir().unwrap();
        let 根 = 目录.path();
        std::fs::create_dir_all(根.join("src")).unwrap();
        std::fs::write(
            根.join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n[dependencies]\nrand = \"0.8\"\n",
        )
        .unwrap();
        std::fs::write(根.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        assert!(!能否直调rustc(根, &根.join("src/main.zh")));
    }

    /// 多文件项目（src/ 下有第二个方言文件）回退 cargo
    #[test]
    fn 测试能否直调rustc多文件回退() {
        let 目录 = tempfile::tempdir().unwrap();
        let 根 = 目录.path();
        std::fs::create_dir_all(根.join("src")).unwrap();
        std::fs::write(
            根.join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(根.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        std::fs::write(
            根.join("src/数学.zh"),
            "函数 加(a: 整数, b: 整数) -> 整数 { a + b }\n",
        )
        .unwrap();
        assert!(!能否直调rustc(根, &根.join("src/main.zh")));
    }

    /// 入口产物含 #[path] 注解时，诊断回译先换算行号再列映射
    ///（回归：多模块 main.zh 中所有诊断行号被注解行整体顶偏移）
    #[test]
    fn 测试诊断定位回译器入口行映射() {
        let 目录 = tempfile::tempdir().unwrap();
        let 根 = 目录.path();
        std::fs::create_dir_all(根.join("src")).unwrap();
        let 源码 = "// 头\n模块 数学;\n\n函数 主函数() {\n    让 _ = 未定义的名字;\n}\n";
        let 入口 = 根.join("src/main.zh");
        std::fs::write(&入口, 源码).unwrap();

        let 管理器 = 中文管理器();
        let 已转译 = i18n_rust_engine::转译管线(源码, &管理器);
        let 列映射 = i18n_rust_engine::列映射::列映射表::r#构建(源码, &已转译.管线映射);
        let (已标注, 行号映射) = 标注非西文模块并行号(&已转译.产出);
        // 磁盘产物（rustc 看到的就是它）：注解插在第 2 行，其后行号 +1
        std::fs::write(根.join("src/main.rs"), &已标注).unwrap();

        let 界面 = crate::本地化::界面::按语言加载("zh");
        let 上下文 = 诊断上下文 {
            界面: &界面,
            语言包: &None,
            项目根: 根,
            管理器: &管理器,
            源码,
            入口文件: &入口,
            列映射: Some(&列映射),
            入口行映射: Some(&行号映射),
            项目: None,
        };
        let mut 回译者 = 诊断定位回译器::新建(&上下文, "main.zh", "zh");
        // rustc 报磁盘第 6 行第 13 列（`未定义的名字` 首字符）
        let mut 定位 = i18n_rust_engine::诊断::诊断位置 {
            源文件名: "src/main.rs".to_string(),
            起始行: 6,
            起始列: 13,
            结束行: 6,
            结束列: 18,
            源码文本: None,
            标签: None,
            是主跨度: true,
        };
        回译者.回译定位(&mut 定位);

        // 回译到母语源码：第 5 行第 11 列（`让` → `let` 的列差已补回）
        assert_eq!(定位.源文件名, "main.zh");
        assert_eq!(定位.起始行, 5);
        assert_eq!(定位.起始列, 11);
        assert_eq!(定位.源码文本.as_deref(), Some("    让 _ = 未定义的名字;"));
    }

    /// cargo JSON 流中提取未声明 crate：E0432/E0433 命中，标准库与重复项排除
    #[test]
    fn 测试提取未解析包名() {
        let 样本1 = r#"{"reason":"compiler-message","message":{"message":"unresolved import `serde_json`","code":{"code":"E0432"},"level":"error"}}"#;
        let 样本2 = r#"{"reason":"compiler-message","message":{"message":"failed to resolve: use of undeclared crate or module `tokio`","code":{"code":"E0433"},"level":"error"}}"#;
        let 样本3 = r#"{"reason":"compiler-message","message":{"message":"unresolved import `std::collections`","code":{"code":"E0432"},"level":"error"}}"#;
        let 样本4 = r#"{"reason":"compiler-message","message":{"message":"unresolved import `serde_json`","code":{"code":"E0432"},"level":"error"}}"#;
        let 样本5 = r#"{"reason":"compiler-message","message":{"message":"unused variable `x`","code":{"code":"E0432"},"level":"warning"}}"#;
        let 输出文本 = [样本1, 样本2, 样本3, 样本4, 样本5].join("\n");
        assert_eq!(提取未解析包名(&输出文本), vec!["serde_json", "tokio"]);
    }

    /// 流式翻译器：panic 三行序列（含线程 ID 框头 → 消息 → note）本地化
    #[test]
    fn 测试流式翻译器恐慌序列() {
        let 界面 = 界面供测试("zh");
        let mut 翻译器 = 流式翻译器::新建();
        assert_eq!(
            翻译器.翻译一行("thread 'main' (621725) panicked at a.rs:1:50:", &界面),
            "线程 '主函数' 恐慌于 a.rs:1:50:"
        );
        assert_eq!(
            翻译器.翻译一行(
                "index out of bounds: the len is 3 but the index is 5",
                &界面
            ),
            "下标越界：长度是 3，但下标是 5"
        );
        assert_eq!(
            翻译器.翻译一行(
                "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace",
                &界面
            ),
            "提示：设置 RUST_BACKTRACE=1 环境变量可显示回溯"
        );
    }

    /// 框头变体：旧格式（无线程 ID）与命名线程（`<unnamed>` 保留不译）
    #[test]
    fn 测试流式翻译器恐慌框头变体() {
        let 界面 = 界面供测试("zh");
        let mut 翻译器 = 流式翻译器::新建();
        assert_eq!(
            翻译器.翻译一行("thread 'main' panicked at b.rs:2:3:", &界面),
            "线程 '主函数' 恐慌于 b.rs:2:3:"
        );
        assert_eq!(
            翻译器.翻译一行("thread '<unnamed>' (42) panicked at c.rs:9:9:", &界面),
            "线程 '<unnamed>' 恐慌于 c.rs:9:9:"
        );
        // 消息未识别（如 unwrap None）：原样透传，且不残留状态影响后续行
        assert_eq!(
            翻译器.翻译一行("called `Option::unwrap()` on a `None` value", &界面),
            "called `Option::unwrap()` on a `None` value"
        );
    }

    /// 六型 panic 消息：越界 start/end、字符边界、RefCell 借用冲突两向
    #[test]
    fn 测试流式翻译器恐慌消息() {
        let 界面 = 界面供测试("zh");
        let 框头 = "thread 'main' panicked at a.rs:1:1:";
        let mut 翻译器 = 流式翻译器::新建();
        let _ = 翻译器.翻译一行(框头, &界面);
        assert_eq!(
            翻译器.翻译一行(
                "start byte index 10 is out of bounds for string of length 6",
                &界面
            ),
            "切片起点字节索引 10 超出字符串长度 6"
        );
        let mut 翻译器 = 流式翻译器::新建();
        let _ = 翻译器.翻译一行(框头, &界面);
        assert_eq!(
            翻译器.翻译一行(
                "end byte index 10 is out of bounds for string of length 6",
                &界面
            ),
            "切片终点字节索引 10 超出字符串长度 6"
        );
        let mut 翻译器 = 流式翻译器::新建();
        let _ = 翻译器.翻译一行(框头, &界面);
        assert_eq!(
            翻译器.翻译一行(
                "start byte index 1 is not a char boundary; it is inside '你' (bytes 0..3 of string)",
                &界面
            ),
            "字节索引 1 不是字符边界；它位于 '你'（字节 0..3）内部"
        );
        let mut 翻译器 = 流式翻译器::新建();
        let _ = 翻译器.翻译一行(框头, &界面);
        assert_eq!(
            翻译器.翻译一行("RefCell already borrowed", &界面),
            "引用单元格已被借用"
        );
        let mut 翻译器 = 流式翻译器::新建();
        let _ = 翻译器.翻译一行(框头, &界面);
        assert_eq!(
            翻译器.翻译一行("RefCell already mutably borrowed", &界面),
            "引用单元格已被可变借用"
        );
    }

    /// 非 panic 行回退 cargo 进度翻译（Finished 等本地化）
    #[test]
    fn 测试流式翻译器回退cargo进度() {
        let 界面 = 界面供测试("zh");
        let mut 翻译器 = 流式翻译器::新建();
        let 结果文本 = 翻译器.翻译一行("    Finished `dev` profile [unoptimized]", &界面);
        assert!(结果文本.contains("编译完成"), "应本地化进度行：{结果文本}");
    }

    /// cargo 进度五种前缀：编译/检查/完成/运行/错误摘要均本地化，
    /// 错误摘要的 "could not compile … due to N" 二段短语也翻译；
    /// 无匹配行与非 could-not-compile 的 error: 原样透传
    #[test]
    fn 测试翻译cargo进度前缀() {
        let 界面 = 界面供测试("zh");
        assert!(翻译cargo进度("   Compiling serde v1.0.0", &界面).contains("正在编译 serde"));
        assert!(翻译cargo进度("    Checking myapp v0.1.0", &界面).contains("正在检查 myapp"));
        assert!(翻译cargo进度("Running `target/debug/t`", &界面).contains("正在运行"));
        let 带尾摘要 = 翻译cargo进度(
            "error: could not compile `myapp` (bin \"myapp\") due to 2 previous errors",
            &界面,
        );
        assert!(带尾摘要.contains("无法编译"), "应翻译无法编译：{带尾摘要}");
        assert!(带尾摘要.contains("2"), "应保留错误数量：{带尾摘要}");
        // could not compile 无 due to 尾段
        assert!(翻译cargo进度("error: could not compile `myapp`", &界面).contains("无法编译"));
        // error: 但不是 could-not-compile：rest 原样保留
        let 原文 = 翻译cargo进度("error: some custom failure", &界面);
        assert!(
            原文.contains("some custom failure"),
            "未识别错误摘要应保留：{原文}"
        );
        // 完全无匹配：原样返回
        assert_eq!(翻译cargo进度("hello world", &界面), "hello world");
    }

    /// 框头 None 分支：缺尾冒号 / 空线程名 / 非框头行均不匹配，原样透传
    #[test]
    fn 测试翻译恐慌框头拒绝畸形() {
        let 界面 = 界面供测试("zh");
        assert!(翻译恐慌框头("thread 'main' panicked at a.rs:1:1", &界面).is_none());
        assert!(翻译恐慌框头("thread '' panicked at a.rs:1:1:", &界面).is_none());
        assert!(翻译恐慌框头("not a panic line", &界面).is_none());
        // 字节索引消息：缺尾结构时返回 None（不硬译半截）
        assert!(翻译字节索引消息("10 unexpected tail", &界面, true).is_none());
        assert!(翻译恐慌消息("totally unknown message", &界面).is_none());
    }

    /// 源码行取值：0 行与越界返回 None，正常行返回原文
    #[test]
    fn 测试取母语源码行边界() {
        let 源文本 = "第一行\n第二行\n";
        assert_eq!(取母语源码行(源文本, 0), None);
        assert_eq!(取母语源码行(源文本, 1).as_deref(), Some("第一行"));
        assert_eq!(取母语源码行(源文本, 3), None);
    }

    /// 产物路径解析：绝对路径原样；相对路径先查项目根；不存在再回退当前目录
    #[test]
    fn 测试解析产物路径顺序() {
        let 目录 = tempfile::tempdir().unwrap();
        let 根 = 目录.path();
        let 绝对路径 = if cfg!(windows) {
            std::path::PathBuf::from("C:/abs/main.rs")
        } else {
            std::path::PathBuf::from("/abs/main.rs")
        };
        assert_eq!(
            解析产物路径(根, &绝对路径.to_string_lossy()),
            Some(绝对路径)
        );
        std::fs::write(根.join("in-root.rs"), b"").unwrap();
        assert_eq!(解析产物路径(根, "in-root.rs"), Some(根.join("in-root.rs")));
        // 项目根下不存在：回退当前工作目录拼接（始终 Some）
        let 得到 = 解析产物路径(根, "nowhere-12345.rs").expect("应回退 cwd");
        assert!(得到.ends_with("nowhere-12345.rs"));
    }

    /// 构造最小诊断回译上下文（列映射/行映射均为 None）
    fn 最小上下文<'a>(
        界面: &'a crate::本地化::界面,
        根: &'a std::path::Path,
        管理器: &'a i18n_rust_engine::映射管理::映射管理器,
        入口文件: &'a std::path::Path,
        源码: &'a str,
    ) -> 诊断上下文<'a> {
        诊断上下文 {
            界面,
            语言包: &None,
            项目根: 根,
            管理器,
            源码,
            入口文件,
            列映射: None,
            入口行映射: None,
            项目: None,
        }
    }

    /// 非方言产物（不存在的第三方 .rs）：定位保持 rustc 原样，绝不误标入口文件
    #[test]
    fn 测试回译定位非方言透传() {
        let 目录 = tempfile::tempdir().unwrap();
        let 根 = 目录.path();
        std::fs::create_dir_all(根.join("src")).unwrap();
        std::fs::write(根.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        std::fs::write(根.join("src/main.rs"), "fn main() {}\n").unwrap();
        let 入口 = 根.join("src/main.zh");
        let 管理器 = 中文管理器();
        let 界面 = crate::本地化::界面::按语言加载("zh");
        let 上下文 = 最小上下文(&界面, 根, &管理器, &入口, "函数 主函数() {}\n");
        let mut 回译者 = 诊断定位回译器::新建(&上下文, "main.zh", "zh");
        let 外部路径 = if cfg!(windows) {
            "C:/nonexistent-crate-9f3/src/lib.rs".to_string()
        } else {
            "/nonexistent-crate-9f3/src/lib.rs".to_string()
        };
        let mut 定位 = i18n_rust_engine::诊断::诊断位置 {
            源文件名: 外部路径.clone(),
            起始行: 7,
            起始列: 3,
            结束行: 7,
            结束列: 9,
            源码文本: Some("orig".to_string()),
            标签: None,
            是主跨度: true,
        };
        回译者.回译定位(&mut 定位);
        assert_eq!(定位.源文件名, 外部路径);
        assert_eq!(定位.起始行, 7);
        assert_eq!(定位.起始列, 3);
        assert_eq!(定位.源码文本.as_deref(), Some("orig"));
    }

    /// 子模块方言产物（src/数学.rs）：解析回同名 .zh 源文件并重建坐标/源码行
    #[test]
    fn 测试回译定位子模块方言() {
        let 目录 = tempfile::tempdir().unwrap();
        let 根 = 目录.path();
        std::fs::create_dir_all(根.join("src")).unwrap();
        let 子源码 = "函数 加(甲: 整数, 乙: 整数) -> 整数 { 甲 + 乙 }\n";
        std::fs::write(根.join("src/数学.zh"), 子源码).unwrap();
        std::fs::write(根.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        let 管理器 = 中文管理器();
        let 子转译 = i18n_rust_engine::转译管线(子源码, &管理器);
        std::fs::write(根.join("src/数学.rs"), &子转译.产出).unwrap();
        std::fs::write(根.join("src/main.rs"), "fn main() {}\n").unwrap();
        let 入口 = 根.join("src/main.zh");
        let 界面 = crate::本地化::界面::按语言加载("zh");
        let 上下文 = 最小上下文(&界面, 根, &管理器, &入口, "函数 主函数() {}\n");
        let mut 回译者 = 诊断定位回译器::新建(&上下文, "main.zh", "zh");
        let mut 定位 = i18n_rust_engine::诊断::诊断位置 {
            源文件名: "src/数学.rs".to_string(),
            起始行: 1,
            起始列: 1,
            结束行: 1,
            结束列: 1,
            源码文本: None,
            标签: None,
            是主跨度: true,
        };
        回译者.回译定位(&mut 定位);
        assert_eq!(定位.源文件名, "数学.zh");
        assert_eq!(定位.源码文本.as_deref(), Some(子源码.trim_end()));
    }

    /// E0432 cargo JSON：翻译器返回存在教学诊断（true），并提取未声明 crate
    #[test]
    fn 测试翻译cargo诊断e0432教学() {
        let 目录 = tempfile::tempdir().unwrap();
        let 根 = 目录.path();
        std::fs::create_dir_all(根.join("src")).unwrap();
        std::fs::write(根.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        std::fs::write(根.join("src/main.rs"), "fn main() {}\n").unwrap();
        let 入口 = 根.join("src/main.zh");
        let 管理器 = 中文管理器();
        let 界面 = crate::本地化::界面::按语言加载("zh");
        let 上下文 = 最小上下文(&界面, 根, &管理器, &入口, "函数 主函数() {}\n");
        let str = r#"{"reason":"compiler-message","message":{"message":"unresolved import `serde_json`","code":{"code":"E0432"},"level":"error","spans":[],"children":[]}}"#;
        assert!(翻译cargo诊断(str, "", &上下文, false, true, false));
    }

    /// 项目内 lang-packs/zh/errors.toml 损坏：打印加载警告后降级内置表，
    /// 诊断仍正常展示（不因消息文件损坏阻断）
    #[test]
    fn 测试翻译cargo诊断本地错误损坏回退() {
        let 目录 = tempfile::tempdir().unwrap();
        let 根 = 目录.path();
        std::fs::create_dir_all(根.join("src")).unwrap();
        std::fs::create_dir_all(根.join("lang-packs/zh")).unwrap();
        std::fs::write(
            根.join("lang-packs/zh/errors.toml"),
            "this is = = not valid toml [[[\n",
        )
        .unwrap();
        std::fs::write(根.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        std::fs::write(根.join("src/main.rs"), "fn main() {}\n").unwrap();
        let 入口 = 根.join("src/main.zh");
        let 管理器 = 中文管理器();
        let 界面 = crate::本地化::界面::按语言加载("zh");
        let 上下文 = 最小上下文(&界面, 根, &管理器, &入口, "函数 主函数() {}\n");
        let str = r#"{"reason":"compiler-message","message":{"message":"unresolved import `serde_json`","code":{"code":"E0432"},"level":"error","spans":[],"children":[]}}"#;
        assert!(翻译cargo诊断(str, "", &上下文, false, true, false));
    }

    /// 无可解析诊断时：编译成功打印成功行（silent 时静默）；
    /// 编译失败输出原始 cargo 摘要（streamed 时跳过，避免运行时失败误标）
    #[test]
    fn 测试翻译cargo诊断空分支() {
        let 目录 = tempfile::tempdir().unwrap();
        let 根 = 目录.path();
        std::fs::create_dir_all(根.join("src")).unwrap();
        std::fs::write(根.join("src/main.zh"), "函数 主函数() {}\n").unwrap();
        std::fs::write(根.join("src/main.rs"), "fn main() {}\n").unwrap();
        let 入口 = 根.join("src/main.zh");
        let 管理器 = 中文管理器();
        let 界面 = crate::本地化::界面::按语言加载("zh");
        // 成功 + 非静默
        let 上下文 = 最小上下文(&界面, 根, &管理器, &入口, "函数 主函数() {}\n");
        assert!(!翻译cargo诊断("", "", &上下文, true, false, false));
        // 成功 + 静默
        assert!(!翻译cargo诊断("", "", &上下文, true, true, false));
        // 失败 + 非流式：输出 cargo 摘要
        assert!(!翻译cargo诊断(
            "",
            "linker `cc` not found",
            &上下文,
            false,
            true,
            false
        ));
        // 失败 + 流式：跳过摘要
        assert!(!翻译cargo诊断(
            "",
            "should not appear",
            &上下文,
            false,
            true,
            true
        ));
        // 非 JSON 杂讯不构成诊断：成功路径
        assert!(!翻译cargo诊断(
            "   Compiling t v0.1.0",
            "",
            &上下文,
            true,
            true,
            false
        ));
    }
}
