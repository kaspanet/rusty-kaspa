extern crate alloc;
extern crate self as kaspa_wallet_core;

pub mod accounts;
pub mod address;
pub mod encryption;
pub mod error;
pub mod events;
mod imports;
pub mod network;
pub mod result;
pub mod runtime;
pub mod secret;
pub mod settings;
pub mod storage;
pub mod tx;
pub mod utils;
pub mod utxo;
pub mod wasm;

pub use accounts::dummy_address;
pub use address::AddressDerivationManager;
pub use events::{Events, SyncState};
pub use kaspa_addresses::{Address, Prefix as AddressPrefix};
pub use kaspa_wrpc_client::client::{ConnectOptions, ConnectStrategy};
pub use result::Result;
pub use settings::{DefaultSettings, SettingsStore, SettingsStoreT, WalletSettings};

pub type DynRpcApi = dyn kaspa_rpc_core::api::rpc::RpcApi;
pub type NotificationChannel = kaspa_utils::channel::Channel<kaspa_rpc_core::Notification>;
