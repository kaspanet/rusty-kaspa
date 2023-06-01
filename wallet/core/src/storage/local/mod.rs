pub mod collection;
pub mod interface;
pub mod iterators;
pub mod store;

pub use collection::Collection;
pub use store::Store;

pub const DEFAULT_WALLET_FOLDER: &str = "~/.kaspa/";
pub const DEFAULT_WALLET_NAME: &str = "kaspa";
pub const DEFAULT_WALLET_FILE: &str = "kaspa";
