use rocksdb::{DBWithThreadMode, MultiThreaded};

pub type DB = DBWithThreadMode<MultiThreaded>;
