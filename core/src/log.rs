//! Logger and logging macros
//!
//! For the macros to properly compile, the calling crate must add a dependency to
//! crate log (ie. `log.workspace = true`) when target architecture is not wasm32.

#[allow(unused_imports)]
use log::{Level,LevelFilter};

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

// TODO: enhance logger with parallel output to file, rotation, compression

#[cfg(not(target_arch = "wasm32"))]
pub fn init_logger(filters: &str) {
    env_logger::Builder::new()
        .format_target(false)
        .format_timestamp_secs()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .parse_filters(filters)
        .init();
}

#[cfg(target_arch = "wasm32")]
#[macro_export]
macro_rules! trace {
    ($($t:tt)*) => {
        if kaspa_core::log::log_level_enabled(Level::Trace) {
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
        if kaspa_core::log::log_level_enabled(Level::Debug) {
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
        if kaspa_core::log::log_level_enabled(Level::Info) {
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
        if kaspa_core::log::log_level_enabled(Level::Warn) {
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
        if kaspa_core::log::log_level_enabled(Level::Error) {
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
