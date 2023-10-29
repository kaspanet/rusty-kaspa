use crate::prelude::DB;
use std::sync::Weak;
use tempfile::TempDir;

#[derive(Default)]
pub struct DbLifetime {
    weak_db_ref: Weak<DB>,
    optional_tempdir: Option<TempDir>,
}

impl DbLifetime {
    pub fn new(tempdir: TempDir, weak_db_ref: Weak<DB>) -> Self {
        Self { optional_tempdir: Some(tempdir), weak_db_ref }
    }

    /// Tracks the DB reference and makes sure all strong refs are cleaned up
    /// but does not remove the DB from disk when dropped.
    pub fn without_destroy(weak_db_ref: Weak<DB>) -> Self {
        Self { optional_tempdir: None, weak_db_ref }
    }
}

impl Drop for DbLifetime {
    fn drop(&mut self) {
        for _ in 0..16 {
            if self.weak_db_ref.strong_count() > 0 {
                // Sometimes another thread is shuting-down and cleaning resources
                std::thread::sleep(std::time::Duration::from_millis(1000));
            } else {
                break;
            }
        }
        assert_eq!(self.weak_db_ref.strong_count(), 0, "DB is expected to have no strong references when lifetime is dropped");
        if let Some(dir) = self.optional_tempdir.take() {
            let options = rocksdb::Options::default();
            let path_buf = dir.path().to_owned();
            let path = path_buf.to_str().unwrap();
            <rocksdb::DBWithThreadMode<rocksdb::MultiThreaded>>::destroy(&options, path)
                .expect("DB is expected to be deletable since there are no references to it");
        }
    }
}

pub fn get_kaspa_tempdir() -> TempDir {
    let global_tempdir = std::env::temp_dir();
    let kaspa_tempdir = global_tempdir.join("rusty-kaspa");
    std::fs::create_dir_all(kaspa_tempdir.as_path()).unwrap();
    let db_tempdir = tempfile::tempdir_in(kaspa_tempdir.as_path()).unwrap();
    db_tempdir
}

/// Creates a DB within a temp directory under `<OS SPECIFIC TEMP DIR>/kaspa-rust`
/// Callers must keep the `TempDbLifetime` guard for as long as they wish the DB to exist.
#[macro_export]
macro_rules! create_temp_db {
    ($conn_builder: expr) => {{
        let db_tempdir = $crate::utils::get_kaspa_tempdir();
        let db_path = db_tempdir.path().to_owned();
        let db = $conn_builder.with_db_path(db_path).build().unwrap();
        ($crate::utils::DbLifetime::new(db_tempdir, std::sync::Arc::downgrade(&db)), db)
    }};
}

/// Creates a DB within the provided directory path.
/// Callers must keep the `TempDbLifetime` guard for as long as they wish the DB instance to exist.
#[macro_export]
macro_rules! create_permanent_db {
    ($db_path: expr, $conn_builder: expr) => {{
        let db_dir = std::path::PathBuf::from($db_path);
        if let Err(e) = std::fs::create_dir(db_dir.as_path()) {
            match e.kind() {
                std::io::ErrorKind::AlreadyExists => panic!("The directory {db_dir:?} already exists"),
                _ => panic!("{e}"),
            }
        }
        let db = $conn_builder.with_db_path(db_dir).build().unwrap();
        ($crate::utils::DbLifetime::without_destroy(std::sync::Arc::downgrade(&db)), db)
    }};
}

/// Loads an existing DB from the provided directory path.
/// Callers must keep the `TempDbLifetime` guard for as long as they wish the DB instance to exist.
#[macro_export]
macro_rules! load_existing_db {
    ($db_path: expr, $conn_builder: expr) => {{
        let db_dir = std::path::PathBuf::from($db_path);
        let db = $conn_builder.with_db_path(db_dir).with_create_if_missing(false).build().unwrap();
        ($crate::utils::DbLifetime::without_destroy(std::sync::Arc::downgrade(&db)), db)
    }};
}
