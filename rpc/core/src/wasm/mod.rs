//! WASM related conversions

pub mod convert;

cfg_if::cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        pub mod message;
        #[cfg(all(test, target_arch = "wasm32"))]
        mod tests;
    }
}
