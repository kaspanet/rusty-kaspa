//! Logger and logging macros
//!
//! For the macros to properly compile, the calling crate must add a dependency to
//! crate log (ie. `log.workspace = true`) when target architecture is not wasm32.

#[cfg(not(target_arch = "wasm32"))]
use consts::*;
#[allow(unused_imports)]
use log::{Level, LevelFilter};

#[cfg(not(target_arch = "wasm32"))]
mod appender;
#[cfg(not(target_arch = "wasm32"))]
mod consts;
#[cfg(not(target_arch = "wasm32"))]
mod logger;

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
    use std::iter::once;

    use crate::log::appender::AppenderSpecs;
    use log4rs::{config::Root, Config};

    const CONSOLE_APPENDER: &str = "stdout";
    const LOG_FILE_APPENDER: &str = "log_file";
    const ERR_LOG_FILE_APPENDER: &str = "err_log_file";

    let level = log::LevelFilter::Info;

    let mut stdout_appender = AppenderSpecs::console(CONSOLE_APPENDER, None);
    let mut file_appender = log_dir.map(|x| AppenderSpecs::roller(LOG_FILE_APPENDER, None, x, LOG_FILE_NAME));
    let mut err_file_appender =
        log_dir.map(|x| AppenderSpecs::roller(ERR_LOG_FILE_APPENDER, Some(LevelFilter::Warn), x, ERR_LOG_FILE_NAME));

    let appenders = once(&mut stdout_appender).chain(&mut file_appender).chain(&mut err_file_appender).map(|x| x.appender());

    // let root_appender_names = once(&stdout_appender).chain(&file_appender).map(|x| x.name);
    let config = Config::builder()
        .appenders(appenders)
        .build(
            Root::builder()
                .appenders(once(&stdout_appender).chain(&file_appender).chain(&err_file_appender).map(|x| x.name))
                .build(level),
        )
        .unwrap();

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
