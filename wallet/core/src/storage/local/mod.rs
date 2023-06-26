pub mod cache;
pub mod collection;
pub mod interface;
pub mod storage;
pub mod streams;
pub mod wallet;

pub use collection::Collection;
pub use storage::Storage;
pub use wallet::Wallet;

pub const DEFAULT_STORAGE_FOLDER: &str = "~/.kaspa/";
// pub const DEFAULT_WALLET_NAME: &str = "kaspa";
pub const DEFAULT_WALLET_FILE: &str = "kaspa";
pub const DEFAULT_SETTINGS_FILE: &str = "settings";
