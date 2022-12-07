//! Logger and logging macros
//!
//! For the macros to properly compile, the calling crate must add a dependency to
//! crate log (ie. `log.workspace = true`) when target architecture is not wasm32.

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
        #[allow(unused_unsafe)]
        let _ = format_args!($($t)*); // Dummy code for using the variables
        // Disable trace until we implement log-level cmd configuration
        // unsafe { core::console::log(&format_args!($($t)*).to_string()) }
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
        #[allow(unused_unsafe)]
        unsafe { core::console::log(&format_args!($($t)*).to_string()) }
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
        #[allow(unused_unsafe)]
        unsafe { core::console::log(&format_args!($($t)*).to_string()) }
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
        #[allow(unused_unsafe)]
        unsafe { core::console::log(&format_args!($($t)*).to_string()) }
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
        #[allow(unused_unsafe)]
        unsafe { core::console::log(&format_args!($($t)*).to_string()) }
    )
}

#[cfg(not(target_arch = "wasm32"))]
#[macro_export]
macro_rules! error {
    ($($t:tt)*) => (
        log::error!($($t)*);
    )
}
