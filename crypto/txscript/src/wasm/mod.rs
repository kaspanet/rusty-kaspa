//!
//!  WASM32 bindings for the txscript framework components.
//!

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(any(feature = "wasm32-sdk", feature = "wasm32-core"))] {
        pub mod opcodes;
        pub mod builder;
        pub mod viewer;

        pub use self::opcodes::*;
        pub use self::builder::*;
        pub use self::viewer::*;
    }
}
