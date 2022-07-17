
#[cfg(target_arch = "wasm32")]
#[macro_export]
macro_rules! trace {
    ($($t:tt)*) => (
        #[allow(unused_unsafe)]
        unsafe { core::console::log(&format_args!($($t)*).to_string()) } 
    )
}


#[cfg(not(target_arch = "wasm32"))]
#[macro_export]
macro_rules! trace {
    ($($t:tt)*) => (
        #[allow(unused_unsafe)]
        unsafe { println!("{}",&format_args!($($t)*).to_string()) } 
    )
}

