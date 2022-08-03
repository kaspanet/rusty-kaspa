pub mod caching;
pub mod errors;
pub mod ghostdag;
pub mod reachability;
pub mod relations;

use rocksdb::{DBWithThreadMode, MultiThreaded};
type DB = DBWithThreadMode<MultiThreaded>;
