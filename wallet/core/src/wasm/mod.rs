//!
//!  WASM32 bindings for the wallet framework components.
//!

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        pub mod api;
        pub mod balance;
        pub mod dispatcher;
        // pub mod keys;
        pub mod message;
        pub mod notify;
        // pub mod privatekeygen;
        // pub mod publickeygen;
        pub mod signer;
        pub mod tx;
        pub mod utils;
        pub mod utxo;
        pub mod wallet;
        pub mod encryption;

        pub use self::balance::*;
        pub use self::dispatcher::*;
        // pub use self::keys::*;
        pub use self::message::*;
        pub use self::notify::*;
        // pub use self::privatekeygen::*;
        // pub use self::publickeygen::*;
        pub use self::signer::*;
        pub use self::tx::*;
        pub use self::utils::*;
        pub use self::utxo::*;
        pub use self::wallet::*;
        pub use self::encryption::*;
    }
    //  else if #[cfg(feature = "wasm32-keygen")] {
        // pub mod keys;
        // pub mod privatekeygen;
        // pub mod publickeygen;
        // pub use keys::*;
        // pub use privatekeygen::*;
        // pub use publickeygen::*;
    // }
}
