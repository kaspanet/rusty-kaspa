use cfg_if::cfg_if;

pub mod error;
pub mod contract_runtime;

cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        pub mod result;
        mod utils;
        pub use utils::*;
    }
}
