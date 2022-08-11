use std::{env, fs, sync::Arc};

use consensus::model::stores::DB;

/// Creates a DB within a temp directory under `<OS SPECIFIC TEMP DIR>/kaspa-rust`
/// Callers must keep the `TempDir` guard for as long as they wish the DB to exist.
pub fn create_temp_db() -> (tempfile::TempDir, Arc<DB>) {
    let global_tempdir = env::temp_dir();
    let kaspa_tempdir = global_tempdir.join("kaspa-rust");
    fs::create_dir_all(kaspa_tempdir.as_path()).unwrap();

    let db_tempdir = tempfile::tempdir_in(kaspa_tempdir.as_path()).unwrap();
    let db_path = db_tempdir.path().to_owned();

    (db_tempdir, Arc::new(DB::open_default(db_path.to_str().unwrap()).unwrap()))
}
