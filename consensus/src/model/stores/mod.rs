pub mod caching;
pub mod errors;
pub mod ghostdag;
pub mod reachability;
pub mod relations;
pub mod statuses;

use rocksdb::{DBWithThreadMode, MultiThreaded};
pub type DB = DBWithThreadMode<MultiThreaded>;
