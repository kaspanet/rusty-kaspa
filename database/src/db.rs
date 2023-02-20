use rocksdb::{DBWithThreadMode, MultiThreaded};
use std::{path::PathBuf, sync::Arc};

/// The DB type used for Kaspad stores
pub type DB = DBWithThreadMode<MultiThreaded>;

/// Creates or loads an existing DB from the provided directory path.
pub fn open_db(db_path: PathBuf, create_if_missing: bool, parallelism: usize) -> Arc<DB> {
    let mut opts = rocksdb::Options::default();
    if parallelism > 1 {
        opts.increase_parallelism(parallelism as i32);
    }
    opts.create_if_missing(create_if_missing);
    let db = Arc::new(DB::open(&opts, db_path.to_str().unwrap()).unwrap());
    db
}

/// Deletes an existing DB if it exists
pub fn delete_db(db_dir: PathBuf) {
    if !db_dir.exists() {
        return;
    }
    let options = rocksdb::Options::default();
    let path = db_dir.to_str().unwrap();
    DB::destroy(&options, path).expect("DB is expected to be deletable");
}
