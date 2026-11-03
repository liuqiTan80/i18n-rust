//! 日志模块
//! 轻量结构化日志系统，支持 RZ_LOG 环境变量控制级别（debug/info/warn/error）。
//! 输出格式：`[时间戳] [级别] [模块名] 消息`（时间戳为 UTC，无第三方依赖）。
//! 提供 log_debug!、log_info!、log_warn!、log_error! 四个宏供其他模块使用。

use std::sync::Once;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// 日志级别（按严重程度递增，序数值用于比较过滤）
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    /// 调试级别：开发阶段详细信息
    Debug = 0,
    /// 信息级别：正常流程关键节点
    Info = 1,
    /// 警告级别：潜在问题但不影响运行
    Warn = 2,
    /// 错误级别：需要关注的异常情况
    Error = 3,
}

impl LogLevel {
    /// 从 RZ_LOG 环境变量取值解析（大小写不敏感，支持 warning 别名）
    pub fn parse_env(text: &str) -> Option<LogLevel> {
        match text.trim().to_ascii_lowercase().as_str() {
            "debug" => Some(Self::Debug),
            "info" => Some(Self::Info),
            "warn" | "warning" => Some(Self::Warn),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    /// 返回当前语言下的显示文字
    pub fn display_text(&self) -> String {
        let key = match self {
            Self::Debug => "log_level_debug",
            Self::Info => "log_level_info",
            Self::Warn => "log_level_warn",
            Self::Error => "log_level_error",
        };
        crate::语言::取消息(key)
    }

    /// 从序号还原日志级别（用于从 AtomicU8 读取）
    fn from_ordinal(ordinal: u8) -> LogLevel {
        match ordinal {
            0 => Self::Debug,
            1 => Self::Info,
            3 => Self::Error,
            _ => Self::Warn,
        }
    }
}

/// 默认级别的序数（警告）：`LEVEL_STORE` 的初值，无 RZ_LOG 时生效。
///
/// 写成字面量而不是 `LogLevel::Warn as u8` 是有意的——这样「初值 ↔ 枚举序数」的一致性
/// 才需要被测试钉住：枚举变体一旦重排（Debug/Info/Warn/Error 的序数值变了），
/// 默认级别会静默漂移成「调试」或「错误」，而 `test_default_level_is_warn` 会红。
const DEFAULT_LEVEL_ORDINAL: u8 = 2;
/// 当前生效的日志级别（默认警告：静默但保留关键输出）
static LEVEL_STORE: AtomicU8 = AtomicU8::new(DEFAULT_LEVEL_ORDINAL);
/// 是否已从环境变量初始化（幂等标记）
static INITIALIZED: Once = Once::new();
/// 是否已被程序化设置（置位后 RZ_LOG 不再覆盖，保证程序化优先级更高）
static LEVEL_SET_PROGRAMMATICALLY: AtomicBool = AtomicBool::new(false);

/// 从 RZ_LOG 环境变量初始化日志级别（幂等，仅首次生效）
///
/// 示例：`RZ_LOG=debug rzc run file.zh` 启用全部日志；
/// `RZ_LOG=error` 仅输出错误。
/// 若级别已被 [`set_log_level`] 程序化设置，则环境变量不覆盖。
/// `init()` 的取舍：程序化设置优先于 RZ_LOG
///
/// 单独拆出来是为了能直接断言——`init()` 本体的 `Once` 全进程只生效一次，
/// 用例若不是第一个调它的，断言就只是在验证「已经没人能改了」，而不是这条规则。
fn env_should_override(programmatically_set: bool) -> bool {
    !programmatically_set
}

pub fn init() {
    INITIALIZED.call_once(|| {
        if !env_should_override(LEVEL_SET_PROGRAMMATICALLY.load(Ordering::Relaxed)) {
            return;
        }
        if let Ok(val) = std::env::var("RZ_LOG")
            && let Some(level) = LogLevel::parse_env(&val)
        {
            LEVEL_STORE.store(level as u8, Ordering::Relaxed);
        }
    });
}

/// 程序化设置日志级别（优先级高于 RZ_LOG，测试与嵌入场景使用）
pub fn set_log_level(level: LogLevel) {
    LEVEL_SET_PROGRAMMATICALLY.store(true, Ordering::Relaxed);
    LEVEL_STORE.store(level as u8, Ordering::Relaxed);
}

/// 返回当前生效的日志级别
pub fn current_level() -> LogLevel {
    LogLevel::from_ordinal(LEVEL_STORE.load(Ordering::Relaxed))
}

/// 判断指定级别是否应输出（级别 >= 当前级别时才输出）
pub fn is_enabled(level: LogLevel) -> bool {
    level >= current_level()
}

/// 测试专用的日志级别守卫（纪律与 `语言::钉住语言` 一致）
///
/// 级别是进程全局（`LEVEL_STORE`），而 libtest 用多线程并发跑用例：任何
/// 「改级别」或「断言级别」的用例都必须持 [`test_log_level`] 返回的守卫，
/// 否则两侧互相翻转全局值，随机红（实测：把断言用例的窗口撑开 150 ms，
/// 与逐个建缓存就写一次级别的旧 `new_cache()` 并发跑，12/12 轮全红）。
/// Drop 时恢复进入前的级别，避免把状态留给后续用例。
#[cfg(test)]
pub(crate) struct LevelTestGuard {
    previous: LogLevel,
    _lock: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl Drop for LevelTestGuard {
    fn drop(&mut self) {
        set_log_level(self.previous);
    }
}

/// 串行化「改级别 / 断言级别」的用例；毒化由持锁方 panic 造成，状态仍可继续用
#[cfg(test)]
static LEVEL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
fn level_lock() -> std::sync::MutexGuard<'static, ()> {
    LEVEL_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 把级别钉到 `level` 并持锁到返回的守卫离开作用域，之后恢复原值
#[cfg(test)]
pub(crate) fn test_log_level(level: LogLevel) -> LevelTestGuard {
    quiet_for_tests();
    let lock = level_lock();
    let previous = current_level();
    set_log_level(level);
    LevelTestGuard {
        previous,
        _lock: lock,
    }
}

/// 需要安静日志的用例（构造缓存等）走这里：全进程只把级别压到 Error 一次
///
/// 旧做法是在共享辅助函数里每次调用都 `set_log_level(Error)`——那是个不持锁的
/// 环境写侧，任何断言级别的用例都在与它赛跑（实测撑开窗口后 12/12 轮红）。
/// 收敛成一次性写入，且这一次写入必须在持锁下进行，才与 [`test_log_level`] 互斥。
///
/// 用 `try_lock` 而不是阻塞式抢锁：拿不到锁说明有「断言级别」的用例正处在窗口
/// 里，那就跳过（本函数每个建缓存的用例都会再走一遍，下一次照样能落地）。这样
/// 既不会与它互翻全局值，也不可能自锁——持守卫的用例里再调 `new_cache()` 只是
/// 这次不静音，不会像阻塞式抢锁那样把 std 的非可重入 Mutex 变成死锁。
#[cfg(test)]
pub(crate) fn quiet_for_tests() {
    static QUIET_DONE: AtomicBool = AtomicBool::new(false);
    if QUIET_DONE.load(Ordering::Relaxed) {
        return; // 已静音：热路径上只是读一个原子量
    }
    let Ok(_lock) = LEVEL_TEST_LOCK.try_lock() else {
        return;
    };
    QUIET_DONE.store(true, Ordering::Relaxed);
    set_log_level(LogLevel::Error);
}

/// 生成一条日志行文本（不包含级别判断，供测试与复用）
pub fn format_log_line(level: LogLevel, module: &str, message: &str) -> String {
    format!(
        "[{}] [{}] [{}] {}",
        current_timestamp(),
        level.display_text(),
        module,
        message
    )
}

/// 写入一条日志到 stderr（级别不足时直接返回）
pub fn write_log(level: LogLevel, module: &str, message: &str) {
    if !is_enabled(level) {
        return;
    }
    eprintln!("{}", format_log_line(level, module, message));
}

// ============================================================
// 时间戳（UTC，手写日历转换，避免引入 chrono 依赖）
// ============================================================

/// 返回当前 UTC 时间戳，格式 `YYYY-MM-DD HH:MM:SS`
fn current_timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (year, month, day, hour, min, sec) = decompose_time(secs);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{min:02}:{sec:02}")
}

/// 将 Unix 秒数分解为 (年, 月, 日, 时, 分, 秒)（UTC）
/// 使用 Howard Hinnant 的 civil_from_days 算法，无第三方依赖
fn decompose_time(total_secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (total_secs / 86_400) as i64;
    let day_secs = total_secs % 86_400;

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_offset =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_offset + era * 400;
    let day_of_year = day_of_era - (365 * year_offset + year_offset / 4 - year_offset / 100);
    let month_offset = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_offset + 2) / 5 + 1) as u32;
    let month = (month_offset + if month_offset < 10 { 3 } else { -9 }) as u32;
    let year = if month <= 2 { year + 1 } else { year };

    (
        year,
        month,
        day,
        (day_secs / 3600) as u32,
        ((day_secs % 3600) / 60) as u32,
        (day_secs % 60) as u32,
    )
}

// ============================================================
// 日志宏（调用前先判断级别，避免无谓的 format! 开销）
// ============================================================

/// 输出调试级日志：`log_debug!("模块名", "消息 {}", 变量)`
#[macro_export]
macro_rules! log_debug {
    ($module:expr, $($arg:tt)*) => {{
        if $crate::logger::is_enabled($crate::logger::LogLevel::Debug) {
            $crate::logger::write_log($crate::logger::LogLevel::Debug, $module, &format!($($arg)*));
        }
    }};
}

/// 输出信息级日志：`log_info!("模块名", "消息 {}", 变量)`
#[macro_export]
macro_rules! log_info {
    ($module:expr, $($arg:tt)*) => {{
        if $crate::logger::is_enabled($crate::logger::LogLevel::Info) {
            $crate::logger::write_log($crate::logger::LogLevel::Info, $module, &format!($($arg)*));
        }
    }};
}

/// 输出警告级日志：`log_warn!("模块名", "消息 {}", 变量)`
#[macro_export]
macro_rules! log_warn {
    ($module:expr, $($arg:tt)*) => {{
        if $crate::logger::is_enabled($crate::logger::LogLevel::Warn) {
            $crate::logger::write_log($crate::logger::LogLevel::Warn, $module, &format!($($arg)*));
        }
    }};
}

/// 输出错误级日志：`log_error!("模块名", "消息 {}", 变量)`
#[macro_export]
macro_rules! log_error {
    ($module:expr, $($arg:tt)*) => {{
        if $crate::logger::is_enabled($crate::logger::LogLevel::Error) {
            $crate::logger::write_log($crate::logger::LogLevel::Error, $module, &format!($($arg)*));
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_parsing() {
        assert_eq!(LogLevel::parse_env("debug"), Some(LogLevel::Debug));
        assert_eq!(LogLevel::parse_env("INFO"), Some(LogLevel::Info));
        assert_eq!(LogLevel::parse_env("warn"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::parse_env("warning"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::parse_env("error"), Some(LogLevel::Error));
        assert_eq!(LogLevel::parse_env("verbose"), None);
        assert_eq!(LogLevel::parse_env(""), None);
    }

    #[test]
    fn test_default_level_is_warn() {
        // 约定：无 RZ_LOG 时默认「警告」——教学级(Info)静默、Warn 与 Error 保留。
        // 旧实现只有 `let _ = current_level();`，一句断言都没有，把初值改成别的
        // 数也照样全绿（变异实测），所以这里钉的是三方一致：初值常量 ↔ 枚举序数 ↔
        // from_ordinal 还原。
        assert_eq!(
            DEFAULT_LEVEL_ORDINAL,
            LogLevel::Warn as u8,
            "默认级别初值与 Warn 的序数脱钩了"
        );
        for level in [
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warn,
            LogLevel::Error,
        ] {
            assert_eq!(
                LogLevel::from_ordinal(level as u8),
                level,
                "{level:?} 序数往返"
            );
        }
        assert_eq!(
            LogLevel::from_ordinal(7),
            LogLevel::Warn,
            "未知序数须兜底为警告（保守：不放大也不静默）"
        );
    }

    #[test]
    fn test_is_enabled_filters_by_level() {
        // 级别是进程全局：改 + 断言必须持守卫，否则与并发用例互翻（见 LevelTestGuard）
        let _level = test_log_level(LogLevel::Warn);
        assert!(!is_enabled(LogLevel::Debug));
        assert!(!is_enabled(LogLevel::Info));
        assert!(is_enabled(LogLevel::Warn));
        assert!(is_enabled(LogLevel::Error));
    }

    #[test]
    fn test_time_decompose_epoch() {
        assert_eq!(decompose_time(0), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn test_time_decompose_known_dates() {
        // 2026-01-01T00:00:00Z 与 2026-08-13T00:00:00Z 的 Unix 秒
        assert_eq!(decompose_time(1_767_225_600), (2026, 1, 1, 0, 0, 0));
        assert_eq!(decompose_time(1_786_579_200), (2026, 8, 13, 0, 0, 0));
    }

    #[test]
    fn test_time_decompose_with_hms() {
        // 1970-01-01T12:34:56Z = 45296 秒
        assert_eq!(decompose_time(45_296), (1970, 1, 1, 12, 34, 56));
        // 闰年 2024-02-29（2024-01-01 = 1704067200，+59 天）
        assert_eq!(
            decompose_time(1_704_067_200 + 59 * 86_400),
            (2024, 2, 29, 0, 0, 0)
        );
    }

    #[test]
    fn test_format_log_line_structure() {
        // 级别文字走全局语言（`LogLevel::display_text` → `语言::取消息`）：断言中文
        // 「信息」就得持语言守卫，否则会与切语言的用例竞争——它把级别换成
        // `INFO`/`ИНФО`，本用例随机红（实测：工作区并行跑 release 全量时会咬）。
        let _lang = crate::语言::钉住语言("zh");
        let line = format_log_line(LogLevel::Info, "test_module", "test message 42");
        // 格式：[YYYY-MM-DD HH:MM:SS] [信息] [test_module] test message 42
        assert!(line.starts_with('['));
        assert!(line.contains("] [信息] [test_module] test message 42"));
        // 时间戳年份段
        let year_part = &line[1..5];
        assert!(year_part.chars().all(|c| c.is_ascii_digit()));
        assert!(line.contains("-"));
    }

    #[test]
    fn test_write_below_level_no_output() {
        // 级别设为错误时，信息级写入不应 panic（被过滤，无输出）
        let _level = test_log_level(LogLevel::Error);
        write_log(LogLevel::Info, "test_module", "不应输出");
        log_info!("test_module", "宏调用不应 panic {}", 1);
        log_debug!("test_module", "调试消息");
        assert!(is_enabled(LogLevel::Error));
    }

    #[test]
    fn test_macros_work_at_enabled_level() {
        // 调试级下四个宏都应放行；宏本身不 panic 只是下限，级别判定才是可断言的行为
        let _level = test_log_level(LogLevel::Debug);
        assert!(is_enabled(LogLevel::Debug));
        assert!(is_enabled(LogLevel::Info));
        assert!(is_enabled(LogLevel::Warn));
        assert!(is_enabled(LogLevel::Error));
        log_debug!("test_module", "调试 {}", "内容");
        log_info!("test_module", "信息 {}", 42);
        log_warn!("test_module", "警告");
        log_error!("test_module", "错误");
    }

    #[test]
    fn test_programmatic_level_not_overridden_by_init() {
        // 程序化设置后，init()（读 RZ_LOG）不得覆盖——契约：程序化优先级更高
        let _level = test_log_level(LogLevel::Error);
        assert!(LEVEL_SET_PROGRAMMATICALLY.load(Ordering::Relaxed));
        init();
        assert_eq!(current_level(), LogLevel::Error);
        // 规则本体走纯函数断言：上面的 init() 受 Once 约束，若不是本进程第一个
        // 调用它的用例，「没被覆盖」可能只是时序巧合，这里把判定逻辑单独钉死。
        assert!(!env_should_override(true), "已程序化设置时 RZ_LOG 不得覆盖");
        assert!(env_should_override(false), "未程序化设置时 RZ_LOG 应生效");
    }
}
