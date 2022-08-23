pub mod caching;
pub mod errors;
pub mod ghostdag;
pub mod reachability;
pub mod relations;
pub mod statuses;
pub mod pruning;
pub mod block_window_cache;
pub mod daa;
pub mod headers;

use rocksdb::{DBWithThreadMode, MultiThreaded};
pub type DB = DBWithThreadMode<MultiThreaded>;
