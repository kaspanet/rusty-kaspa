pub mod errors;
pub mod ghostdag;
pub mod reachability;
pub mod relations;
pub mod store;

use rocksdb::{DBWithThreadMode, MultiThreaded};
type DB = DBWithThreadMode<MultiThreaded>;
