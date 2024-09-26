//!
//! # Client-side consensus primitives.
//!
//! This crate offers client-side primitives mirroring the consensus layer of the Kaspa p2p node.
//! It declares structs such as [`Transaction`], [`TransactionInput`], [`TransactionOutput`],
//! [`TransactionOutpoint`], [`UtxoEntry`], and [`UtxoEntryReference`]
//! that are used by the Wallet subsystem as well as WASM bindings.
//!
//! Unlike raw consensus primitives (used for high-performance DAG processing) the primitives
//! offered in this crate are designed to be used in client-side applications. Their internal
//! data is typically wrapped into `Arc<Mutex<T>>`, allowing for easy sharing between
//! async / threaded environments and WASM bindings.
//!

pub mod error;
mod imports;
mod input;
mod outpoint;
mod output;
pub mod result;
mod serializable;
mod transaction;
mod utxo;
pub use input::*;
pub use outpoint::*;
pub use output::*;
pub use serializable::*;
pub use transaction::*;
pub use utxo::*;

cfg_if::cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        mod header;
        mod utils;
        mod hash;
        mod sign;

        pub use header::*;
        pub use utils::*;
        pub use hash::*;
        pub use sign::sign_with_multiple_v3;
    }
}
