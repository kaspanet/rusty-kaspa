extern crate alloc;
extern crate core;
extern crate self as kaspa_wallet_core;

pub mod derivation;
pub mod encryption;
pub mod error;
pub mod events;
mod imports;
pub mod message;
pub mod result;
pub mod rpc;
pub mod runtime;
pub mod secret;
pub mod settings;
pub mod storage;
pub mod tx;
pub mod utils;
pub mod utxo;
pub mod wasm;

pub use derivation::{AddressDerivationManager, AddressDerivationManagerTrait};
pub use events::{Events, SyncState};
pub use kaspa_addresses::{Address, Prefix as AddressPrefix};
pub use kaspa_wrpc_client::client::{ConnectOptions, ConnectStrategy};
pub use result::Result;
pub use settings::{DefaultSettings, SettingsStore, SettingsStoreT, WalletSettings};

/// Returns the version of the Wallet framework.
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
