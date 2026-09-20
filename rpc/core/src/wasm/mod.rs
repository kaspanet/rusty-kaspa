//! WASM related conversions

pub mod convert;

cfg_if::cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        pub mod message;
    }
}
