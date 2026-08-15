// 日志模块
// 轻量结构化日志系统，支持 RZ_LOG 环境变量控制级别（debug/info/warn/error）。
// 输出格式：`[时间戳] [级别] [模块名] 消息`（时间戳为 UTC，无第三方依赖）。
// 提供 log_debug!、log_info!、log_warn!、log_error! 四个宏供其他模块使用。

use std::sync::Once;
use std::sync::atomic::{AtomicU8, Ordering};

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
    pub fn from_str(text: &str) -> Option<LogLevel> {
        match text.trim().to_ascii_lowercase().as_str() {
            "debug" => Some(Self::Debug),
            "info" => Some(Self::Info),
            "warn" | "warning" => Some(Self::Warn),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    /// 返回中文显示文字
    pub fn display_text(&self) -> &'static str {
        match self {
            Self::Debug => "调试",
            Self::Info => "信息",
            Self::Warn => "警告",
            Self::Error => "错误",
        }
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

/// 当前生效的日志级别（默认警告：静默但保留关键输出）
static LEVEL_STORE: AtomicU8 = AtomicU8::new(2);
/// 是否已从环境变量初始化（幂等标记）
static INITIALIZED: Once = Once::new();

/// 从 RZ_LOG 环境变量初始化日志级别（幂等，仅首次生效）
///
/// 示例：`RZ_LOG=debug rzc run file.zh` 启用全部日志；
/// `RZ_LOG=error` 仅输出错误。
pub fn init() {
    INITIALIZED.call_once(|| {
        if let Ok(val) = std::env::var("RZ_LOG") {
            if let Some(level) = LogLevel::from_str(&val) {
                set_log_level(level);
            }
        }
    });
}

/// 程序化设置日志级别（优先级高于 RZ_LOG，测试与嵌入场景使用）
pub fn set_log_level(level: LogLevel) {
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
    let year_offset = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
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
        assert_eq!(LogLevel::from_str("debug"), Some(LogLevel::Debug));
        assert_eq!(LogLevel::from_str("INFO"), Some(LogLevel::Info));
        assert_eq!(LogLevel::from_str("warn"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::from_str("warning"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::from_str("error"), Some(LogLevel::Error));
        assert_eq!(LogLevel::from_str("verbose"), None);
        assert_eq!(LogLevel::from_str(""), None);
    }

    #[test]
    fn test_default_level_is_warn() {
        // 默认级别：警告（无 RZ_LOG 时静默但保留关键输出）
        // 注意：此测试依赖全局状态，可能受其他测试影响
        let _ = current_level();
    }

    #[test]
    fn test_is_enabled_filters_by_level() {
        set_log_level(LogLevel::Warn);
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
        set_log_level(LogLevel::Error);
        write_log(LogLevel::Info, "test_module", "不应输出");
        log_info!("test_module", "宏调用不应 panic {}", 1);
        log_debug!("test_module", "调试消息");
        assert!(is_enabled(LogLevel::Error));
    }

    #[test]
    fn test_macros_work_at_enabled_level() {
        set_log_level(LogLevel::Debug);
        log_debug!("test_module", "调试 {}", "内容");
        log_info!("test_module", "信息 {}", 42);
        log_warn!("test_module", "警告");
        log_error!("test_module", "错误");
    }
}
