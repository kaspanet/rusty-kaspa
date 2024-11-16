//!
//!  WASM32 bindings for the wallet framework components.
//!

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(any(feature = "wasm32-sdk", feature = "wasm32-core"))] {
        pub mod balance;
        pub mod message;
        pub mod notify;
        pub mod signer;
        pub mod tx;
        pub mod utils;
        pub mod utxo;
        pub mod encryption;
        pub mod cryptobox;

        pub use self::balance::*;
        pub use self::message::*;
        pub use self::notify::*;
        pub use self::signer::*;
        pub use self::tx::*;
        pub use self::utils::*;
        pub use self::utxo::*;
        pub use self::encryption::*;
        pub use self::cryptobox::*;
    }
}

cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        pub mod api;
        pub mod wallet;
        pub use self::wallet::*;
    }
}
