mod access;
mod cache;
mod db;
mod errors;
mod item;
mod key;
mod writer;

pub mod registry;
mod set_access;
pub mod utils;

pub mod prelude {
    use crate::{db, errors};

    pub use super::access::CachedDbAccess;
    pub use super::cache::{Cache, CachePolicy};
    pub use super::item::{CachedDbItem, CachedDbSetItem};
    pub use super::key::DbKey;
    pub use super::set_access::{CachedDbSetAccess, DbSetAccess, ReadLock};
    pub use super::writer::{BatchDbWriter, DbWriter, DirectDbWriter, DirectWriter, MemoryWriter};
    pub use db::{delete_db, ConnBuilder, DB};
    pub use errors::{StoreError, StoreResult, StoreResultEmptyTuple, StoreResultExtensions};
}
