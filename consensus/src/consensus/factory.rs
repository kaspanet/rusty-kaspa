#[cfg(feature = "devnet-prealloc")]
use super::utxo_set_override::{set_genesis_utxo_commitment_from_config, set_initial_utxo_set};
use super::{ctl::Ctl, Consensus};
use crate::{model::stores::U64Key, pipeline::ProcessingCounters};
use itertools::Itertools;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_notify::root::ConsensusNotificationRoot;
use kaspa_consensusmanager::{ConsensusFactory, ConsensusInstance, DynConsensusCtl, SessionLock};
use kaspa_core::{debug, time::unix_now};
use kaspa_database::{
    prelude::{BatchDbWriter, CachedDbAccess, CachedDbItem, DirectDbWriter, StoreError, StoreResult, StoreResultExtensions, DB},
    registry::DatabaseStorePrefixes,
};

use kaspa_txscript::caches::TxScriptCacheCounters;
use parking_lot::RwLock;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, error::Error, fs, path::PathBuf, sync::Arc};

#[derive(Serialize, Deserialize, Clone)]
pub struct ConsensusEntry {
    key: u64,
    directory_name: String,
    creation_timestamp: u64,
}

impl ConsensusEntry {
    pub fn new(key: u64, directory_name: String, creation_timestamp: u64) -> Self {
        Self { key, directory_name, creation_timestamp }
    }

    pub fn from_key(key: u64) -> Self {
        Self { key, directory_name: format!("consensus-{:0>3}", key), creation_timestamp: unix_now() }
    }
}

pub enum ConsensusEntryType {
    Existing(ConsensusEntry),
    New(ConsensusEntry),
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct MultiConsensusMetadata {
    current_consensus_key: Option<u64>,
    staging_consensus_key: Option<u64>,
    /// Max key used for a consensus entry
    max_key_used: u64,
    /// Memorizes whether this node was recently an archive node
    is_archival_node: bool,
    /// General serialized properties to be used cross DB versions
    props: HashMap<Vec<u8>, Vec<u8>>,
    /// The DB scheme version
    version: u32,
}

#[derive(Clone)]
pub struct MultiConsensusManagementStore {
    db: Arc<DB>,
    entries: CachedDbAccess<U64Key, ConsensusEntry>,
    metadata: CachedDbItem<MultiConsensusMetadata>,
}

impl MultiConsensusManagementStore {
    pub fn new(db: Arc<DB>) -> Self {
        let mut store = Self {
            db: db.clone(),
            entries: CachedDbAccess::new(db.clone(), 16, DatabaseStorePrefixes::ConsensusEntries.into()),
            metadata: CachedDbItem::new(db, DatabaseStorePrefixes::MultiConsensusMetadata.into()),
        };
        store.init();
        store
    }

    fn init(&mut self) {
        if self.metadata.read().unwrap_option().is_none() {
            let mut batch = WriteBatch::default();
            let metadata = MultiConsensusMetadata::default();
            self.metadata.write(BatchDbWriter::new(&mut batch), &metadata).unwrap();
            self.db.write(batch).unwrap();
        }
    }

    /// The entry type signifies whether the returned entry is an existing/new consensus
    pub fn active_consensus_entry(&mut self) -> StoreResult<ConsensusEntryType> {
        let mut metadata = self.metadata.read()?;
        match metadata.current_consensus_key {
            Some(key) => Ok(ConsensusEntryType::Existing(self.entries.read(key.into())?)),
            None => {
                metadata.max_key_used += 1; // Capture the slot
                let key = metadata.max_key_used;
                self.metadata.write(DirectDbWriter::new(&self.db), &metadata)?;
                Ok(ConsensusEntryType::New(ConsensusEntry::from_key(key)))
            }
        }
    }

    pub fn save_new_active_consensus(&mut self, entry: ConsensusEntry) -> StoreResult<()> {
        let key = entry.key;
        if self.entries.has(key.into())? {
            return Err(StoreError::KeyAlreadyExists(format!("{key}")));
        }
        let mut batch = WriteBatch::default();
        self.entries.write(BatchDbWriter::new(&mut batch), key.into(), entry)?;
        self.metadata.update(BatchDbWriter::new(&mut batch), |mut data| {
            data.current_consensus_key = Some(key);
            data
        })?;
        self.db.write(batch)?;
        Ok(())
    }

    pub fn new_staging_consensus_entry(&mut self) -> StoreResult<ConsensusEntry> {
        let mut metadata = self.metadata.read()?;

        // TODO: handle the case where `staging_consensus_key` is already some (perhaps from a previous interrupted run)

        metadata.max_key_used += 1;
        let new_key = metadata.max_key_used;
        metadata.staging_consensus_key = Some(new_key);
        let new_entry = ConsensusEntry::from_key(new_key);

        let mut batch = WriteBatch::default();
        self.metadata.write(BatchDbWriter::new(&mut batch), &metadata)?;
        self.entries.write(BatchDbWriter::new(&mut batch), new_key.into(), new_entry.clone())?;
        self.db.write(batch)?;

        Ok(new_entry)
    }

    pub fn commit_staging_consensus(&mut self) -> StoreResult<()> {
        self.metadata.update(DirectDbWriter::new(&self.db), |mut data| {
            assert!(data.staging_consensus_key.is_some());
            data.current_consensus_key = data.staging_consensus_key.take();
            data
        })?;
        Ok(())
    }

    pub fn cancel_staging_consensus(&mut self) -> StoreResult<()> {
        self.metadata.update(DirectDbWriter::new(&self.db), |mut data| {
            data.staging_consensus_key = None;
            data
        })?;
        Ok(())
    }

    fn iterator(&self) -> impl Iterator<Item = Result<ConsensusEntry, Box<dyn Error>>> + '_ {
        self.entries.iterator().map(|iter_result| match iter_result {
            Ok((_, entry)) => Ok(entry),
            Err(e) => Err(e),
        })
    }

    fn iterate_non_active(&self) -> impl Iterator<Item = Result<ConsensusEntry, Box<dyn Error>>> + '_ {
        let current_consensus_key = self.metadata.read().unwrap().current_consensus_key;
        self.iterator().filter(move |entry_result| {
            if let Ok(entry) = entry_result {
                return Some(entry.key) != current_consensus_key;
            }

            true
        })
    }

    fn delete_entry(&mut self, entry: ConsensusEntry) -> StoreResult<()> {
        self.entries.delete(DirectDbWriter::new(&self.db), entry.key.into())
    }

    pub fn is_archival_node(&self) -> StoreResult<bool> {
        match self.metadata.read() {
            Ok(data) => Ok(data.is_archival_node),
            Err(StoreError::KeyNotFound(_)) => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub fn set_is_archival_node(&mut self, is_archival_node: bool) {
        let mut metadata = self.metadata.read().unwrap();
        metadata.is_archival_node = is_archival_node;
        let mut batch = WriteBatch::default();
        self.metadata.write(BatchDbWriter::new(&mut batch), &metadata).unwrap();
    }
}

pub struct Factory {
    management_store: Arc<RwLock<MultiConsensusManagementStore>>,
    config: Config,
    db_root_dir: PathBuf,
    db_parallelism: usize,
    notification_root: Arc<ConsensusNotificationRoot>,
    counters: Arc<ProcessingCounters>,
    tx_script_cache_counters: Arc<TxScriptCacheCounters>,
}

impl Factory {
    pub fn new(
        management_db: Arc<DB>,
        config: &Config,
        db_root_dir: PathBuf,
        db_parallelism: usize,
        notification_root: Arc<ConsensusNotificationRoot>,
        counters: Arc<ProcessingCounters>,
        tx_script_cache_counters: Arc<TxScriptCacheCounters>,
    ) -> Self {
        let mut config = config.clone();
        #[cfg(feature = "devnet-prealloc")]
        set_genesis_utxo_commitment_from_config(&mut config);
        config.process_genesis = false;
        let management_store = Arc::new(RwLock::new(MultiConsensusManagementStore::new(management_db)));
        management_store.write().set_is_archival_node(config.is_archival);
        let factory =
            Self { management_store, config, db_root_dir, db_parallelism, notification_root, counters, tx_script_cache_counters };
        factory.clean_non_active_consensus_entries();
        factory
    }

    pub fn clean_non_active_consensus_entries(&self) {
        if self.config.is_archival {
            return;
        }

        let mut write_guard = self.management_store.write();
        let entries_to_delete = write_guard.iterate_non_active().collect_vec();
        for entry_result in entries_to_delete.iter() {
            let entry = entry_result.as_ref().unwrap();
            let dir = self.db_root_dir.join(entry.directory_name.clone());
            if dir.exists() {
                fs::remove_dir_all(dir).unwrap();
            }
        }

        for entry_result in entries_to_delete {
            write_guard.delete_entry(entry_result.unwrap()).unwrap();
        }
    }
}

impl ConsensusFactory for Factory {
    fn new_active_consensus(&self) -> (ConsensusInstance, DynConsensusCtl) {
        assert!(!self.notification_root.is_closed());

        let mut config = self.config.clone();
        let mut is_new_consensus = false;
        let entry = match self.management_store.write().active_consensus_entry().unwrap() {
            ConsensusEntryType::Existing(entry) => {
                config.process_genesis = false;
                entry
            }
            ConsensusEntryType::New(entry) => {
                // Configure to process genesis only if this is a brand new consensus
                config.process_genesis = true;
                is_new_consensus = true;
                entry
            }
        };

        let dir = self.db_root_dir.join(entry.directory_name.clone());
        let db = kaspa_database::prelude::ConnBuilder::default().with_db_path(dir).with_parallelism(self.db_parallelism).build();

        let session_lock = SessionLock::new();
        let consensus = Arc::new(Consensus::new(
            db.clone(),
            Arc::new(config),
            session_lock.clone(),
            self.notification_root.clone(),
            self.counters.clone(),
            self.tx_script_cache_counters.clone(),
            entry.creation_timestamp,
        ));

        // We write the new active entry only once the instance was created successfully.
        // This way we can safely avoid processing genesis in future process runs
        if is_new_consensus {
            #[cfg(feature = "devnet-prealloc")]
            set_initial_utxo_set(&self.config.initial_utxo_set, consensus.clone(), self.config.params.genesis.hash);
            self.management_store.write().save_new_active_consensus(entry).unwrap();
        }

        (ConsensusInstance::new(session_lock, consensus.clone()), Arc::new(Ctl::new(self.management_store.clone(), db, consensus)))
    }

    fn new_staging_consensus(&self) -> (ConsensusInstance, DynConsensusCtl) {
        assert!(!self.notification_root.is_closed());

        let entry = self.management_store.write().new_staging_consensus_entry().unwrap();
        let dir = self.db_root_dir.join(entry.directory_name);
        let db = kaspa_database::prelude::ConnBuilder::default().with_db_path(dir).with_parallelism(self.db_parallelism).build();

        let session_lock = SessionLock::new();
        let consensus = Arc::new(Consensus::new(
            db.clone(),
            Arc::new(self.config.to_builder().skip_adding_genesis().build()),
            session_lock.clone(),
            self.notification_root.clone(),
            self.counters.clone(),
            self.tx_script_cache_counters.clone(),
            entry.creation_timestamp,
        ));

        (ConsensusInstance::new(session_lock, consensus.clone()), Arc::new(Ctl::new(self.management_store.clone(), db, consensus)))
    }

    fn close(&self) {
        debug!("Consensus factory: closing");
        self.notification_root.close();
    }
}
