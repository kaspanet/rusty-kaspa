use std::sync::Arc;

use consensus::model::stores::DB;

pub fn create_temp_db() -> (tempfile::TempDir, Arc<DB>) {
    let db_tempdir = tempfile::tempdir().unwrap();
    let db_path = db_tempdir.path().to_owned();
    (db_tempdir, Arc::new(DB::open_default(db_path.to_str().unwrap()).unwrap()))
}
