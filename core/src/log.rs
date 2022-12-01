/// TODO: implement a proper logger with reused macro logic

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
        #[allow(unused_unsafe)]
        let _ = format_args!($($t)*); // Dummy code for using the variables
        // Disable trace until we implement log-level cmd configuration
        // unsafe { println!("TRACE: {}",&format_args!($($t)*).to_string()) }
    };
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
        #[allow(unused_unsafe)]
        unsafe { println!("INFO: {}",&format_args!($($t)*).to_string()) }
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
        #[allow(unused_unsafe)]
        unsafe { println!("WARN: {}",&format_args!($($t)*).to_string()) }
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
        #[allow(unused_unsafe)]
        unsafe { println!("ERROR: {}",&format_args!($($t)*).to_string()) }
    )
}
