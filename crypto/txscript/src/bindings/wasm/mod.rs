//!
//!  WASM32 bindings for the txscript framework components.
//!

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(any(feature = "wasm32-sdk", feature = "wasm32-core"))] {
        // pub mod opcodes;
        pub mod builder;

        pub use crate::bindings::opcodes::*;
        pub use self::builder::*;
    }
}
