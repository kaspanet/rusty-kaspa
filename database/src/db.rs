use rocksdb::{DBWithThreadMode, MultiThreaded};
use std::path::PathBuf;

pub use conn_builder::ConnBuilder;

mod conn_builder;

/// The DB type used for Kaspad stores
pub type DB = DBWithThreadMode<MultiThreaded>;

/// Deletes an existing DB if it exists
pub fn delete_db(db_dir: PathBuf) {
    if !db_dir.exists() {
        return;
    }
    let options = rocksdb::Options::default();
    let path = db_dir.to_str().unwrap();
    DB::destroy(&options, path).expect("DB is expected to be deletable");
}
