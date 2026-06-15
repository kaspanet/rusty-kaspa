use std::{path::PathBuf, sync::Arc};

use kaspa_consensus::consensus::factory::MultiConsensusManagementStore;
use kaspa_consensus_core::network::NetworkId;
use kaspa_database::prelude::{ConnBuilder, DB};
use kaspad_lib::daemon::get_app_dir_from_args;

use crate::{Error, Result, args::DbSourceArgs};

const DEFAULT_DATA_DIR: &str = "datadir";
const CONSENSUS_DB: &str = "consensus";
const META_DB: &str = "meta";
const META_DB_FILE_LIMIT: i32 = 5;

#[derive(Debug, Clone)]
pub struct ResolvedConsensusDb {
    pub network: NetworkId,
    pub app_dir: Option<PathBuf>,
    pub meta_db_path: Option<PathBuf>,
    pub consensus_db_path: PathBuf,
    pub active_consensus_dir: Option<String>,
    pub archival: Option<bool>,
}

impl ResolvedConsensusDb {
    pub fn open_consensus_readonly(&self, files_limit: i32) -> Result<Arc<DB>> {
        open_readonly_db(self.consensus_db_path.clone(), files_limit)
    }
}

pub fn resolve_consensus_db(args: &DbSourceArgs) -> Result<ResolvedConsensusDb> {
    let network = args.network.network()?;

    if let Some(consensus_db_path) = args.consensus_db.clone() {
        return Ok(ResolvedConsensusDb {
            network,
            app_dir: None,
            meta_db_path: None,
            consensus_db_path,
            active_consensus_dir: None,
            archival: None,
        });
    }

    let kaspad_args = args.network.to_kaspad_args(args.appdir.as_ref().map(|path| path.display().to_string()))?;
    let app_dir = get_app_dir_from_args(&kaspad_args);
    let db_dir = app_dir.join(network.to_prefixed()).join(DEFAULT_DATA_DIR);
    let meta_db_path = db_dir.join(META_DB);
    let consensus_db_root = db_dir.join(CONSENSUS_DB);

    let meta_db = open_readonly_db(meta_db_path.clone(), META_DB_FILE_LIMIT)?;
    let management_store = MultiConsensusManagementStore::new_readonly(meta_db);
    let archival = management_store.is_archival_node()?;
    let active_consensus_dir = management_store.active_consensus_dir_name()?;
    let consensus_db_path = match active_consensus_dir.as_ref() {
        Some(active_consensus_dir) => consensus_db_root.join(active_consensus_dir),
        None if archival => consensus_db_root,
        None => return Err(Error::MissingActiveConsensus),
    };

    Ok(ResolvedConsensusDb {
        network,
        app_dir: Some(app_dir),
        meta_db_path: Some(meta_db_path),
        consensus_db_path,
        active_consensus_dir,
        archival: Some(archival),
    })
}

/// Opens a RocksDB instance in read-only mode for diagnostics.
///
/// RocksDB documents read-only opens against a concurrently open read-write DB
/// as undefined. This diagnostic still uses read-only mode because secondary
/// opens can stall on large kaspad DBs; live-node scans are therefore best-effort.
///
/// Custom WAL directories are not wired here, so scan such nodes only after a
/// clean shutdown if authoritative results are required.
pub fn open_readonly_db(db_path: PathBuf, files_limit: i32) -> Result<Arc<DB>> {
    Ok(ConnBuilder::default().with_db_path(db_path).with_files_limit(files_limit).build_readonly()?)
}
