use cfg_if::cfg_if;

pub mod error;

cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        pub mod result;
        mod imports;
        mod signable;
        mod txscript;
        mod utils;

        pub use signable::*;
        pub use txscript::*;
        pub use utils::*;
    }

}
