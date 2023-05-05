pub const DEFAULT_LOGGER_ENV: &str = "RUST_LOG";

pub const LOG_FILE_NAME: &str = "rusty-kaspa.log";
pub const ERR_LOG_FILE_NAME: &str = "rusty-kaspa_err.log";

pub const LOG_ARCHIVE_SUFFIX: &str = ".{}.gz";

pub const LOG_FILE_BASE_ROLLS: u32 = 1;
pub const LOG_FILE_MAX_ROLLS: u32 = 8;
pub const LOG_FILE_MAX_SIZE: u64 = 5 * 1_000_000;

pub const LOG_LINE_PATTERN_COLORED: &str = "{d(%Y-%m-%dT%H:%M:%S %Z):19.19}Z [{h({({l}):5.5})}] {m}{n}";
pub const LOG_LINE_PATTERN: &str = "{d(%Y-%m-%dT%H:%M:%S %Z):19.19}Z [{({l}):5.5}] {m}{n}";
