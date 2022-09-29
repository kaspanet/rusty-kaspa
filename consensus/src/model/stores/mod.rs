pub mod block_transactions;
pub mod block_window_cache;
pub mod caching;
pub mod daa;
pub mod depth;
pub mod errors;
pub mod ghostdag;
pub mod headers;
pub mod pruning;
pub mod reachability;
pub mod relations;
pub mod statuses;
pub mod past_pruning_points;

use rocksdb::{DBWithThreadMode, MultiThreaded};
pub type DB = DBWithThreadMode<MultiThreaded>;
