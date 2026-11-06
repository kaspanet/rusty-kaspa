use std::{collections::HashSet, ops::RangeInclusive, sync::Arc};

use kaspa_consensus_core::{
    BlockHashSet,
    tx::{ScriptPublicKey, ScriptPublicKeys, TransactionOutpoint},
};
use kaspa_core::trace;
use kaspa_database::{
    prelude::{CachePolicy, DB, StoreResult},
    registry::DatabaseStorePrefixes,
};
use kaspa_hashes::Hash;
use kaspa_index_core::indexed_utxos::{
    BalanceByScriptPublicKey, CovenantUtxoChanges, CovenantUtxoPageCursor, OrderedCovenantUtxoEntriesPage, OrderedUtxoEntriesPage,
    UtxoPageCursor,
};

use crate::{
    IDENT,
    errors::{UtxoIndexError, UtxoIndexResult},
    model::UtxoSetByScriptPublicKey,
    stores::{
        covenant_ids::{DbUtxoSetByCovenantIdStore, UtxoSetByCovenantIdStore, UtxoSetByCovenantIdStoreReader},
        db_metadata::{DbUtxoIndexDbMetaStore, UtxoIndexDbMetaData, UtxoIndexDbMetaStore, UtxoIndexDbMetaStoreReader},
        script_public_keys::{DbUtxoSetByScriptPublicKeyStore, UtxoSetByScriptPublicKeyStore, UtxoSetByScriptPublicKeyStoreReader},
        supply::{CirculatingSupplyStore, CirculatingSupplyStoreReader, DbCirculatingSupplyStore},
        tips::{DbUtxoIndexTipsStore, UtxoIndexTipsStore, UtxoIndexTipsStoreReader},
    },
};

pub const UTXOINDEX_DB_VERSION: u16 = 1;

#[derive(Clone)]
pub struct Store {
    utxoindex_tips_store: DbUtxoIndexTipsStore,
    circulating_supply_store: DbCirculatingSupplyStore,
    db_version_store: DbUtxoIndexDbMetaStore,
    utxos_by_script_public_key_store: DbUtxoSetByScriptPublicKeyStore,
    utxos_by_covenant_id_store: DbUtxoSetByCovenantIdStore,
    covenant_tips_store: DbUtxoIndexTipsStore,
}

impl Store {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            utxoindex_tips_store: DbUtxoIndexTipsStore::new(db.clone(), DatabaseStorePrefixes::UtxoIndexScriptPublicKeyTips),
            circulating_supply_store: DbCirculatingSupplyStore::new(db.clone()),
            db_version_store: DbUtxoIndexDbMetaStore::new(db.clone()),
            // index rows are consumed by ordered scans over mostly distinct keys, so a read
            // cache would mostly pin the scanned key space in memory without reuse
            utxos_by_script_public_key_store: DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty),
            utxos_by_covenant_id_store: DbUtxoSetByCovenantIdStore::new(
                db.clone(),
                CachePolicy::Empty,
                DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty),
            ),
            covenant_tips_store: DbUtxoIndexTipsStore::new(db, DatabaseStorePrefixes::UtxoIndexCovenantTips),
        }
    }

    pub fn has_legacy_db_version(&self) -> UtxoIndexResult<bool> {
        // Older UTXO index databases may not have a db-version marker at all.
        // In that case we need to distinguish between:
        // 1) a fresh/empty DB (no rebuild prompt needed), and
        // 2) an existing pre-versioned DB (must rebuild before continuing).
        // Tips and circulating-supply are cheap sentinel stores that indicate
        // whether the UTXO index DB has already been populated.
        let tips_exist = match self.utxoindex_tips_store.get() {
            Ok(_) => true,
            Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => false,
            Err(err) => return Err(UtxoIndexError::StoreAccessError(err)),
        };

        let supply_exists = match self.circulating_supply_store.get() {
            Ok(_) => true,
            Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => false,
            Err(err) => return Err(UtxoIndexError::StoreAccessError(err)),
        };

        match self.db_version_store.get() {
            Ok(meta) => Ok(meta.version != UTXOINDEX_DB_VERSION),
            Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => Ok(tips_exist || supply_exists),
            Err(err) => Err(UtxoIndexError::StoreAccessError(err)),
        }
    }

    pub fn ensure_db_version_current(&mut self) -> UtxoIndexResult<()> {
        match self.db_version_store.get() {
            Ok(meta) if meta.version == UTXOINDEX_DB_VERSION => Ok(()),
            Ok(_) | Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => {
                self.db_version_store.set(UtxoIndexDbMetaData { version: UTXOINDEX_DB_VERSION })?;
                Ok(())
            }
            Err(err) => Err(UtxoIndexError::StoreAccessError(err)),
        }
    }

    pub fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey> {
        self.utxos_by_script_public_key_store.get_utxos_from_script_public_keys(script_public_keys)
    }

    pub fn get_utxos_from_script_public_keys_by_daa_score_page(
        &self,
        script_public_keys: Vec<ScriptPublicKey>,
        daa_score_range: RangeInclusive<u64>,
        cursor: Option<UtxoPageCursor>,
        limit: Option<usize>,
    ) -> UtxoIndexResult<OrderedUtxoEntriesPage> {
        self.utxos_by_script_public_key_store.get_utxos_from_script_public_keys_by_daa_score_page(
            script_public_keys,
            daa_score_range,
            cursor,
            limit,
        )
    }

    pub fn get_utxos_by_covenant_ids_page(
        &self,
        covenant_ids: Vec<Hash>,
        daa_score_range: RangeInclusive<u64>,
        script_public_keys: &[ScriptPublicKey],
        cursor: Option<CovenantUtxoPageCursor>,
        budget: Option<usize>,
    ) -> UtxoIndexResult<OrderedCovenantUtxoEntriesPage> {
        self.utxos_by_covenant_id_store.get_utxos_by_covenant_ids_page(
            covenant_ids,
            daa_score_range,
            script_public_keys,
            cursor,
            budget,
        )
    }

    pub fn get_balance_by_script_public_key(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<BalanceByScriptPublicKey> {
        self.utxos_by_script_public_key_store.get_balance_from_script_public_keys(script_public_keys)
    }

    // This can have a big memory footprint, so it should be used only for tests.
    pub fn get_all_outpoints(&self) -> StoreResult<HashSet<TransactionOutpoint>> {
        self.utxos_by_script_public_key_store.get_all_outpoints()
    }

    pub fn update_utxo_state(
        &mut self,
        to_add: &UtxoSetByScriptPublicKey,
        to_remove: &UtxoSetByScriptPublicKey,
        covenant_changes: &CovenantUtxoChanges,
        try_reset_on_err: bool,
    ) -> StoreResult<()> {
        // A UTXO entry can appear both in removed and in added (if the DAA score of the entry changed). Thus
        // we must first apply removals and then additions (so it will be re-added in the addition phase)
        let mut res = self.utxos_by_script_public_key_store.remove_utxo_entries(to_remove);

        if res.is_err() {
            if try_reset_on_err {
                self.delete_all()?;
            }
            return res;
        }

        // Now apply additions
        res = self.utxos_by_script_public_key_store.add_utxo_entries(to_add);

        if try_reset_on_err && res.is_err() {
            self.delete_all()?;
        };
        res?;

        // Covenant index writes mirror the script public key store flow; a failure here resets the
        // covenant store alone, since the script public key store is already committed and current
        let mut covenant_res = self.utxos_by_covenant_id_store.remove_utxo_entries(&covenant_changes.removed);
        if covenant_res.is_err() && try_reset_on_err {
            self.reset_covenant_store()?;
        }
        covenant_res?;

        covenant_res = self.utxos_by_covenant_id_store.add_utxo_entries(&covenant_changes.added);
        if covenant_res.is_err() && try_reset_on_err {
            self.reset_covenant_store()?;
        }

        covenant_res
    }

    /// Drops every covenant row and its tips so the next startup backfill re-runs.
    fn reset_covenant_store(&mut self) -> StoreResult<()> {
        self.utxos_by_covenant_id_store.delete_all()?;
        self.covenant_tips_store.remove()
    }

    /// Brings the covenant store up to date with the script public key store: when the covenant
    /// tips do not match the utxoindex tips, every covenant row is rebuilt from the
    /// script public key store contents and the covenant tips are set to the current tips.
    pub fn migrate_covenant_index(&mut self) -> UtxoIndexResult<()> {
        let tips = self.utxoindex_tips_store.get()?;
        if let Ok(covenant_tips) = self.covenant_tips_store.get()
            && *covenant_tips == *tips
        {
            return Ok(());
        }
        self.reset_covenant_store()?;
        self.utxos_by_covenant_id_store.backfill_from_spk_store()?;
        self.set_covenant_tips((*tips).clone())?;
        Ok(())
    }

    pub fn get_circulating_supply(&self) -> StoreResult<u64> {
        self.circulating_supply_store.get()
    }

    pub fn update_circulating_supply(&mut self, circulating_supply_diff: i64, try_reset_on_err: bool) -> StoreResult<u64> {
        let res = self.circulating_supply_store.update_circulating_supply(circulating_supply_diff);
        if try_reset_on_err && res.is_err() {
            self.delete_all()?;
        }
        res
    }

    pub fn insert_circulating_supply(&mut self, circulating_supply: u64, try_reset_on_err: bool) -> StoreResult<()> {
        let res = self.circulating_supply_store.insert(circulating_supply);
        if try_reset_on_err && res.is_err() {
            self.delete_all()?;
        }
        res
    }

    pub fn get_tips(&self) -> StoreResult<Arc<BlockHashSet>> {
        self.utxoindex_tips_store.get()
    }

    pub fn set_tips(&mut self, tips: BlockHashSet, try_reset_on_err: bool) -> StoreResult<()> {
        let res = self.utxoindex_tips_store.set_tips(tips);
        if try_reset_on_err && res.is_err() {
            self.delete_all()?;
        }
        res
    }

    pub fn set_covenant_tips(&mut self, tips: BlockHashSet) -> StoreResult<()> {
        self.covenant_tips_store.set_tips(tips)
    }

    /// Resets the utxoindex database:
    pub fn delete_all(&mut self) -> StoreResult<()> {
        // TODO: explore possibility of deleting and replacing whole db, currently there is an issue because of file lock and db being in an arc.
        trace!("[{0}] attempting to clear utxoindex database...", IDENT);

        // Clear all
        self.utxoindex_tips_store.remove()?;
        self.circulating_supply_store.remove()?;
        self.db_version_store.remove()?;
        self.utxos_by_script_public_key_store.delete_all()?;
        self.utxos_by_covenant_id_store.delete_all()?;
        self.covenant_tips_store.remove()?;

        trace!("[{0}] clearing utxoindex database - success!", IDENT);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{CompactUtxoEntry, CovenantUtxoChanges, UtxoEntryKeySuffixRecord};
    use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint};
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};

    fn covenant_changes(id: Hash, rows: Vec<(ScriptPublicKey, u64, TransactionOutpoint)>, removed: bool) -> CovenantUtxoChanges {
        let mut changes = CovenantUtxoChanges::new();
        let set = if removed { &mut changes.removed } else { &mut changes.added };
        set.insert(
            id,
            rows.into_iter().map(|(spk, daa_score, outpoint)| (spk, UtxoEntryKeySuffixRecord::new(daa_score, outpoint))).collect(),
        );
        changes
    }

    fn spk_set(id: Hash, rows: Vec<(ScriptPublicKey, u64, TransactionOutpoint, u64)>) -> UtxoSetByScriptPublicKey {
        let mut set = UtxoSetByScriptPublicKey::new();
        for (spk, daa_score, outpoint, amount) in rows {
            set.entry(spk)
                .or_default()
                .insert(UtxoEntryKeySuffixRecord::new(daa_score, outpoint), CompactUtxoEntry::new(amount, false, Some(id)));
        }
        set
    }

    fn tips(word: u64) -> BlockHashSet {
        BlockHashSet::from_iter([Hash::from_u64_word(word)])
    }

    #[test]
    fn test_covenant_parity_with_spk_store() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = Store::new(db);

        let id = Hash::from_slice(&[1u8; 32]);
        let spk = ScriptPublicKey::from_vec(0, vec![0x01]);
        let outpoint = TransactionOutpoint::new(Hash::from_u64_word(7), 0);

        let added = spk_set(id, vec![(spk.clone(), 4, outpoint, 1234)]);
        let covenant_added = covenant_changes(id, vec![(spk.clone(), 4, outpoint)], false);
        store.update_utxo_state(&added, &UtxoSetByScriptPublicKey::new(), &covenant_added, false).unwrap();

        let page = store.get_utxos_by_covenant_ids_page(vec![id], 0..=u64::MAX, &[], None, None).unwrap();
        assert_eq!(page.entries().len(), 1);
        let (entry_id, entry_spk, suffix, entry) = &page.entries()[0];
        assert_eq!(*entry_id, id);
        assert_eq!(*entry_spk, spk);
        assert_eq!(suffix.daa_score(), 4);
        assert_eq!(entry.amount, 1234);
    }

    #[test]
    fn test_daa_shift_replaces_covenant_row() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = Store::new(db);

        let id = Hash::from_slice(&[1u8; 32]);
        let spk = ScriptPublicKey::from_vec(0, vec![0x01]);
        let outpoint = TransactionOutpoint::new(Hash::from_u64_word(7), 0);

        let added = spk_set(id, vec![(spk.clone(), 1, outpoint, 100)]);
        let covenant_added = covenant_changes(id, vec![(spk.clone(), 1, outpoint)], false);
        store.update_utxo_state(&added, &UtxoSetByScriptPublicKey::new(), &covenant_added, false).unwrap();

        // same outpoint re-added under a new daa score, appearing in both removed and added
        let removed = spk_set(id, vec![(spk.clone(), 1, outpoint, 100)]);
        let readded = spk_set(id, vec![(spk.clone(), 2, outpoint, 100)]);
        let mut covenant_shifted = CovenantUtxoChanges::new();
        covenant_shifted.removed.insert(id, vec![(spk.clone(), UtxoEntryKeySuffixRecord::new(1, outpoint))]);
        covenant_shifted.added.insert(id, vec![(spk.clone(), UtxoEntryKeySuffixRecord::new(2, outpoint))]);
        store.update_utxo_state(&readded, &removed, &covenant_shifted, false).unwrap();

        let page = store.get_utxos_by_covenant_ids_page(vec![id], 0..=u64::MAX, &[], None, None).unwrap();
        assert_eq!(page.entries().len(), 1);
        assert_eq!(page.entries()[0].2.daa_score(), 2);
    }

    #[test]
    fn test_migration_rebuilds_covenant_rows_from_spk_store() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = Store::new(db);

        let id = Hash::from_slice(&[1u8; 32]);
        let spk = ScriptPublicKey::from_vec(0, vec![0x01]);
        let outpoint = TransactionOutpoint::new(Hash::from_u64_word(7), 0);

        let added = spk_set(id, vec![(spk.clone(), 4, outpoint, 1234)]);
        let covenant_added = covenant_changes(id, vec![(spk.clone(), 4, outpoint)], false);
        store.update_utxo_state(&added, &UtxoSetByScriptPublicKey::new(), &covenant_added, false).unwrap();
        store.set_tips(tips(1), false).unwrap();

        // no covenant tips were set by the update path, so the migration redoes the
        // covenant rows from the script public key store and yields the identical set
        store.migrate_covenant_index().unwrap();
        let page = store.get_utxos_by_covenant_ids_page(vec![id], 0..=u64::MAX, &[], None, None).unwrap();
        assert_eq!(page.entries().len(), 1);
        assert_eq!(page.entries()[0].3.amount, 1234);
    }

    #[test]
    fn test_migration_skips_when_covenant_tips_match() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = Store::new(db);

        let id = Hash::from_slice(&[1u8; 32]);
        let spk = ScriptPublicKey::from_vec(0, vec![0x01]);
        let outpoint = TransactionOutpoint::new(Hash::from_u64_word(7), 0);

        let added = spk_set(id, vec![(spk.clone(), 4, outpoint, 1234)]);
        let covenant_added = covenant_changes(id, vec![(spk.clone(), 4, outpoint)], false);
        store.update_utxo_state(&added, &UtxoSetByScriptPublicKey::new(), &covenant_added, false).unwrap();
        store.set_tips(tips(1), false).unwrap();
        store.migrate_covenant_index().unwrap();

        let removed = spk_set(id, vec![(spk.clone(), 4, outpoint, 1234)]);
        let covenant_removed = covenant_changes(id, vec![(spk.clone(), 4, outpoint)], true);
        store.update_utxo_state(&UtxoSetByScriptPublicKey::new(), &removed, &covenant_removed, false).unwrap();

        // an spk row re-added without its covenant counterpart leaves the covenant
        // store stale; with matching tips the migration leaves it untouched
        let readded = spk_set(id, vec![(spk.clone(), 4, outpoint, 1234)]);
        store.update_utxo_state(&readded, &UtxoSetByScriptPublicKey::new(), &CovenantUtxoChanges::new(), false).unwrap();
        store.migrate_covenant_index().unwrap();
        let page = store.get_utxos_by_covenant_ids_page(vec![id], 0..=u64::MAX, &[], None, None).unwrap();
        assert!(page.entries().is_empty());
    }

    #[test]
    fn test_migration_without_covenant_tips_drops_stale_rows() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = Store::new(db);

        let id = Hash::from_slice(&[1u8; 32]);
        let stale_id = Hash::from_slice(&[9u8; 32]);
        let spk = ScriptPublicKey::from_vec(0, vec![0x01]);
        let outpoint = TransactionOutpoint::new(Hash::from_u64_word(7), 0);

        let stale_added = covenant_changes(stale_id, vec![(spk.clone(), 5, outpoint)], false);
        store.update_utxo_state(&UtxoSetByScriptPublicKey::new(), &UtxoSetByScriptPublicKey::new(), &stale_added, false).unwrap();

        let added = spk_set(id, vec![(spk.clone(), 4, outpoint, 1234)]);
        let covenant_added = covenant_changes(id, vec![(spk.clone(), 4, outpoint)], false);
        store.update_utxo_state(&added, &UtxoSetByScriptPublicKey::new(), &covenant_added, false).unwrap();
        store.set_tips(tips(1), false).unwrap();

        store.migrate_covenant_index().unwrap();
        let page = store.get_utxos_by_covenant_ids_page(vec![stale_id], 0..=u64::MAX, &[], None, None).unwrap();
        assert!(page.entries().is_empty());
        let page = store.get_utxos_by_covenant_ids_page(vec![id], 0..=u64::MAX, &[], None, None).unwrap();
        assert_eq!(page.entries().len(), 1);
    }

    #[test]
    fn test_migration_backfills_when_covenant_tips_lag() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = Store::new(db);

        let id = Hash::from_slice(&[1u8; 32]);
        let stale_id = Hash::from_slice(&[9u8; 32]);
        let spk = ScriptPublicKey::from_vec(0, vec![0x01]);
        let outpoint = TransactionOutpoint::new(Hash::from_u64_word(7), 0);

        let added = spk_set(id, vec![(spk.clone(), 4, outpoint, 1234)]);
        let covenant_added = covenant_changes(id, vec![(spk.clone(), 4, outpoint)], false);
        store.update_utxo_state(&added, &UtxoSetByScriptPublicKey::new(), &covenant_added, false).unwrap();
        store.set_tips(tips(1), false).unwrap();
        store.migrate_covenant_index().unwrap();

        // a run without the covenant index advances the script public key store past the
        // covenant tips; stale covenant rows do not survive the next backfill
        let stale_added = covenant_changes(stale_id, vec![(spk.clone(), 5, outpoint)], false);
        store.update_utxo_state(&UtxoSetByScriptPublicKey::new(), &UtxoSetByScriptPublicKey::new(), &stale_added, false).unwrap();
        store.set_tips(tips(2), false).unwrap();
        store.migrate_covenant_index().unwrap();
        let page = store.get_utxos_by_covenant_ids_page(vec![stale_id], 0..=u64::MAX, &[], None, None).unwrap();
        assert!(page.entries().is_empty());
        let page = store.get_utxos_by_covenant_ids_page(vec![id], 0..=u64::MAX, &[], None, None).unwrap();
        assert_eq!(page.entries().len(), 1);
    }
}
