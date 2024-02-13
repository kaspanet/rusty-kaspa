use cfg_if::cfg_if;

pub mod error;
mod imports;
mod outpoint;
mod output;
pub mod result;
mod utxo;
pub use outpoint::*;
pub use output::*;
pub use utxo::*;

cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        mod input;
        mod keypair;
        mod signable;
        mod signer;
        mod transaction;
        mod txscript;
        mod types;
        mod utils;
        mod version;

        pub use input::*;
        pub use keypair::*;
        pub use signable::*;
        pub use signer::*;
        pub use transaction::*;
        pub use txscript::*;
        pub use types::*;
        pub use utils::*;
        pub use version::*;
    }
}
