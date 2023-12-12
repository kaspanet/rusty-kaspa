cfg_if::cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        pub mod counters;
        pub mod middleware;
    }
}
