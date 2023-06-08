pub const DEFAULT_LOGGER_ENV: &str = "RUST_LOG";

pub const LOG_FILE_NAME: &str = "rusty-kaspa.log";
pub const ERR_LOG_FILE_NAME: &str = "rusty-kaspa_err.log";

pub const LOG_ARCHIVE_SUFFIX: &str = ".{}.gz";

pub const LOG_FILE_BASE_ROLLS: u32 = 1;
pub const LOG_FILE_MAX_ROLLS: u32 = 8;
pub const LOG_FILE_MAX_SIZE: u64 = 100_000_000;

/// Console (stdout) log line pattern, with offset from the local time to UTC (UTC being +00:00)
pub const LOG_LINE_PATTERN_COLORED: &str = "{d(%Y-%m-%d %H:%M:%S%.3f%:z)} [{h({({l}):5.5})}] {m}{n}";
/// File log line pattern, with offset from the local time to UTC (UTC being +00:00)
pub const LOG_LINE_PATTERN: &str = "{d(%Y-%m-%d %H:%M:%S%.3f%:z)} [{({l}):5.5}] {m}{n}";
