//! Logger and logging macros
//!
//! For the macros to properly compile, the calling crate must add a dependency to
//! crate log (ie. `log.workspace = true`) when target architecture is not wasm32.

#[allow(unused_imports)]
pub use log::{Level, LevelFilter};
pub use workflow_log;

cfg_if::cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        use consts::*;

        mod appender;
        mod consts;
        mod logger;
    }
}

pub fn set_log_level(level: LevelFilter) {
    workflow_log::set_log_level(level);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn init_logger(log_dir: Option<&str>, filters: &str) {
    use crate::log::appender::AppenderSpec;
    use log4rs::{config::Root, Config};
    use std::iter::once;

    const CONSOLE_APPENDER: &str = "stdout";
    const LOG_FILE_APPENDER: &str = "log_file";
    const ERR_LOG_FILE_APPENDER: &str = "err_log_file";

    let level = LevelFilter::Info;
    let loggers = logger::Builder::new().root_level(level).parse_env(DEFAULT_LOGGER_ENV).parse_expression(filters).build();

    let mut stdout_appender = AppenderSpec::console(CONSOLE_APPENDER, None);
    let mut file_appender = log_dir.map(|x| AppenderSpec::roller(LOG_FILE_APPENDER, None, x, LOG_FILE_NAME));
    let mut err_file_appender =
        log_dir.map(|x| AppenderSpec::roller(ERR_LOG_FILE_APPENDER, Some(LevelFilter::Warn), x, ERR_LOG_FILE_NAME));
    let appenders = once(&mut stdout_appender).chain(&mut file_appender).chain(&mut err_file_appender).map(|x| x.appender());

    let config = Config::builder()
        .appenders(appenders)
        .loggers(loggers.items())
        .build(
            Root::builder()
                .appenders(once(&stdout_appender).chain(&file_appender).chain(&err_file_appender).map(|x| x.name))
                .build(loggers.root_level()),
        )
        .unwrap();

    let _handle = log4rs::init_config(config).unwrap();

    set_log_level(level);
}

/// Tries to init the global logger, but does not panic if it was already setup.
/// Should be used for tests.
#[cfg(not(target_arch = "wasm32"))]
pub fn try_init_logger(filters: &str) {
    use crate::log::appender::AppenderSpec;
    use log4rs::{config::Root, Config};

    const CONSOLE_APPENDER: &str = "stdout";

    let loggers = logger::Builder::new().root_level(LevelFilter::Info).parse_env(DEFAULT_LOGGER_ENV).parse_expression(filters).build();
    let mut stdout_appender = AppenderSpec::console(CONSOLE_APPENDER, None);
    let config = Config::builder()
        .appender(stdout_appender.appender())
        .loggers(loggers.items())
        .build(Root::builder().appender(CONSOLE_APPENDER).build(loggers.root_level()))
        .unwrap();
    let _ = log4rs::init_config(config);
}

#[cfg(target_arch = "wasm32")]
#[macro_export]
macro_rules! trace {
    ($($t:tt)*) => {
        if kaspa_core::log::workflow_log::log_level_enabled(log::Level::Trace) {
            kaspa_core::log::workflow_log::impls::trace_impl(None, &format_args!($($t)*));
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
        if kaspa_core::log::workflow_log::log_level_enabled(log::Level::Debug) {
            kaspa_core::log::workflow_log::impls::debug_impl(None, &format_args!($($t)*));
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
        if kaspa_core::log::workflow_log::log_level_enabled(log::Level::Info) {
            kaspa_core::log::workflow_log::impls::info_impl(None, &format_args!($($t)*));
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
        if kaspa_core::log::workflow_log::log_level_enabled(log::Level::Warn) {
            kaspa_core::log::workflow_log::impls::warn_impl(None, &format_args!($($t)*));
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
        if kaspa_core::log::workflow_log::log_level_enabled(log::Level::Error) {
            kaspa_core::log::workflow_log::impls::error_impl(None, &format_args!($($t)*));
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
