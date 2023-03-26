mod access;
mod cache;
mod db;
mod errors;
mod item;
mod key;
mod writer;

pub mod prelude {
    use crate::{db, errors};

    pub use super::access::CachedDbAccess;
    pub use super::cache::Cache;
    pub use super::item::CachedDbItem;
    pub use super::key::{DbKey, SEP, SEP_SIZE};
    pub use super::writer::{BatchDbWriter, DbWriter, DirectDbWriter};
    pub use db::{delete_db, open_db, DB};
    pub use errors::{StoreError, StoreResult, StoreResultEmptyTuple, StoreResultExtensions};
}
