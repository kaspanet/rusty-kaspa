//! Logger and logging macros
//!
//! For the macros to properly compile, the calling crate must add a dependency to
//! crate log (ie. `log.workspace = true`) when target architecture is not wasm32.

#[allow(unused_imports)]
use log::{Level, LevelFilter};

cfg_if::cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        static mut LEVEL_FILTER : LevelFilter = LevelFilter::Trace;
        #[inline(always)]
        pub fn log_level_enabled(level: Level) -> bool {
            unsafe { LEVEL_FILTER >= level }
        }
        pub fn set_log_level(level: LevelFilter) {
            unsafe { LEVEL_FILTER = level };
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn init_logger(log_dir: Option<&str>, _filters: &str) {
    use log4rs::{
        append::{
            console::ConsoleAppender,
            rolling_file::{
                policy::compound::{roll::fixed_window::FixedWindowRoller, trigger::size::SizeTrigger, CompoundPolicy},
                RollingFileAppender,
            },
            Append,
        },
        config::{Appender, Root},
        encode::pattern::PatternEncoder,
        Config,
    };
    use std::{iter::once, path::PathBuf};

    const CONSOLE_APPENDER: &str = "stdout";

    const LOG_FILE_APPENDER: &str = "log_file";
    const LOG_FILE_NAME: &str = "rusty-kaspa.log";
    const LOG_FILE_NAME_PATTERN: &str = "rusty-kaspa.log.{}.gz";

    const LOG_LINE_PATTERN_COLORED: &str = "[{d(%Y-%m-%dT%H:%M:%S %Z)} {h({({l}):5.5})}] {m}{n}";
    const LOG_LINE_PATTERN: &str = "[{d(%Y-%m-%dT%H:%M:%S %Z)} {({l}):5.5}] {m}{n}";

    let level = log::LevelFilter::Info;

    let stdout_appender: (&'static str, Box<dyn Append>) = (
        CONSOLE_APPENDER,
        Box::new(ConsoleAppender::builder().encoder(Box::new(PatternEncoder::new(LOG_LINE_PATTERN_COLORED))).build()),
    );

    let file_appender: Option<(&'static str, Box<dyn Append>)> = log_dir.map(|x| {
        let trigger_size: u64 = 10 * 1024 * 1024 * 1024;
        let trigger = Box::new(SizeTrigger::new(trigger_size));

        let file_path = PathBuf::from(x).join(LOG_FILE_NAME);
        let roller_pattern = PathBuf::from(x).join(LOG_FILE_NAME_PATTERN);
        let roller_count = 10;
        let roller_base = 1;
        let roller =
            Box::new(FixedWindowRoller::builder().base(roller_base).build(roller_pattern.to_str().unwrap(), roller_count).unwrap());

        let compound_policy = Box::new(CompoundPolicy::new(trigger, roller));
        let file_appender = RollingFileAppender::builder()
            .encoder(Box::new(PatternEncoder::new(LOG_LINE_PATTERN)))
            .build(file_path, compound_policy)
            .unwrap();

        (LOG_FILE_APPENDER, Box::new(file_appender) as Box<dyn Append>)
    });

    let appender_names = once(&stdout_appender).chain(file_appender.iter()).map(|(name, _)| *name).collect::<Vec<_>>();
    let appenders =
        once(stdout_appender).chain(file_appender.into_iter()).map(|(name, appender)| Appender::builder().build(name, appender));

    let config = Config::builder().appenders(appenders).build(Root::builder().appenders(appender_names).build(level)).unwrap();

    let _handle = log4rs::init_config(config).unwrap();

    workflow_log::set_log_level(level);
}

/// Tries to init the global logger, but does not panic if it was already setup.
/// Should be used for tests.
#[cfg(not(target_arch = "wasm32"))]
pub fn try_init_logger(filters: &str) {
    let _ = env_logger::Builder::new()
        .format_target(false)
        .format_timestamp_secs()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .parse_filters(filters)
        .try_init();
}

#[cfg(target_arch = "wasm32")]
#[macro_export]
macro_rules! trace {
    ($($t:tt)*) => {
        if kaspa_core::log::log_level_enabled(log::Level::Trace) {
            kaspa_core::console::log(&format_args!($($t)*).to_string());
        }
    };
}

#[cfg(not(target_arch = "wasm32"))]
#[macro_export]
macro_rules! trace {
    ($($t:tt)*) => {
        log::trace!($($t)*);
    };
}

#[cfg(target_arch = "wasm32")]
#[macro_export]
macro_rules! debug {
    ($($t:tt)*) => (
        if kaspa_core::log::log_level_enabled(log::Level::Debug) {
            kaspa_core::console::log(&format_args!($($t)*).to_string());
        }
    )
}

#[cfg(not(target_arch = "wasm32"))]
#[macro_export]
macro_rules! debug {
    ($($t:tt)*) => (
        log::debug!($($t)*);
    )
}

#[cfg(target_arch = "wasm32")]
#[macro_export]
macro_rules! info {
    ($($t:tt)*) => (
        if kaspa_core::log::log_level_enabled(log::Level::Info) {
            kaspa_core::console::log(&format_args!($($t)*).to_string());
        }
    )
}

#[cfg(not(target_arch = "wasm32"))]
#[macro_export]
macro_rules! info {
    ($($t:tt)*) => (
        log::info!($($t)*);
    )
}

#[cfg(target_arch = "wasm32")]
#[macro_export]
macro_rules! warn {
    ($($t:tt)*) => (
        if kaspa_core::log::log_level_enabled(log::Level::Warn) {
            kaspa_core::console::warn(&format_args!($($t)*).to_string());
        }
    )
}

#[cfg(not(target_arch = "wasm32"))]
#[macro_export]
macro_rules! warn {
    ($($t:tt)*) => (
        log::warn!($($t)*);
    )
}

#[cfg(target_arch = "wasm32")]
#[macro_export]
macro_rules! error {
    ($($t:tt)*) => (
        if kaspa_core::log::log_level_enabled(log::Level::Error) {
            kaspa_core::console::error(&format_args!($($t)*).to_string());
        }
    )
}

#[cfg(not(target_arch = "wasm32"))]
#[macro_export]
macro_rules! error {
    ($($t:tt)*) => (
        log::error!($($t)*);
    )
}
