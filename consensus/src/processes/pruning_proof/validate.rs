use std::{
    ops::DerefMut,
    sync::{atomic::Ordering, Arc},
};

use itertools::Itertools;
use kaspa_consensus_core::{
    blockhash::{BlockHashExtensions, BlockHashes, ORIGIN},
    errors::pruning::{PruningImportError, PruningImportResult},
    header::Header,
    pruning::{PruningPointProof, PruningProofMetadata},
    BlockLevel,
};
use kaspa_core::info;
use kaspa_database::prelude::{CachePolicy, ConnBuilder, StoreResultExt, StoreResultUnitExt};
use kaspa_hashes::Hash;
use kaspa_pow::{calc_block_level, calc_block_level_check_pow};
use kaspa_utils::vec::VecExtensions;
use parking_lot::lock_api::RwLock;
use rocksdb::WriteBatch;

use crate::{
    model::{
        services::reachability::MTReachabilityService,
        stores::{
            ghostdag::{CompactGhostdagData, DbGhostdagStore, GhostdagStore, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStore, HeaderStoreReader},
            headers_selected_tip::HeadersSelectedTipStoreReader,
            pruning::PruningStoreReader,
            reachability::{DbReachabilityStore, ReachabilityStoreReader},
            relations::{DbRelationsStore, RelationsStoreReader},
        },
    },
    processes::{ghostdag::protocol::GhostdagManager, reachability::inquirer as reachability, relations::RelationsStoreExtensions},
};

use super::{PruningProofManager, TempProofContext};

impl PruningProofManager {
    /// Validates an incoming pruning point proof against the current consensus.
    ///
    /// The function reconstructs temporary stores for both the
    /// challenger proof and the current (defender) consensus, validates all
    /// selected tips, and compares blue work including pruning-period work.
    ///
    /// Returns `Ok(())` if the proof is valid and superior, or an appropriate
    /// `PruningImportError` otherwise.
    pub fn validate_pruning_point_proof(
        &self,
        proof: &PruningPointProof,
        proof_metadata: &PruningProofMetadata,
    ) -> PruningImportResult<()> {
        if proof.len() != self.max_block_level as usize + 1 {
            return Err(PruningImportError::ProofNotEnoughLevels(self.max_block_level as usize + 1));
        }

        // Initialize the stores for the incoming pruning proof (the challenger)
        let mut challenger_stores_and_processes = self.init_validate_pruning_point_proof_stores_and_processes(proof)?;
        let challenger_pp_header = proof[0].last().expect("checked if empty");
        let challenger_selected_tip_by_level =
            self.populate_stores_for_validate_pruning_point_proof(proof, &mut challenger_stores_and_processes, true)?;
        let challenger_ghostdag_stores = challenger_stores_and_processes.ghostdag_stores;

        // Get the proof for the current consensus (the defender) and recreate the stores for it
        // This is expected to be fast because if a proof exists, it will be cached.
        // If no proof exists, this is empty
        let mut defender_proof = self.get_pruning_point_proof();
        if defender_proof.is_empty() {
            // An empty proof can only happen if we're at genesis. We're going to create a proof for this case that contains the genesis header only
            let genesis_header = self.headers_store.get_header(self.genesis_hash).unwrap();
            defender_proof = Arc::new((0..=self.max_block_level).map(|_| vec![genesis_header.clone()]).collect_vec());
        }
        let mut defender_stores_and_processes = self.init_validate_pruning_point_proof_stores_and_processes(&defender_proof)?;
        let defender_selected_tip_by_level =
            self.populate_stores_for_validate_pruning_point_proof(&defender_proof, &mut defender_stores_and_processes, false)?;
        let defender_ghostdag_stores = defender_stores_and_processes.ghostdag_stores;

        let pruning_read = self.pruning_point_store.read();
        let defender_pp = pruning_read.pruning_point().unwrap();
        let defender_pp_header = self.headers_store.get_header(defender_pp).unwrap();

        // The accumulated blue work of the defender's proof from the pruning point onward
        let defender_pruning_period_work =
            self.headers_selected_tip_store.read().get().unwrap().blue_work.saturating_sub(defender_pp_header.blue_work);
        // The claimed blue work of the challenger's proof from their pruning point and up to the triggering relay block. This work
        // will eventually be verified if the proof is accepted so we can treat it as trusted
        let challenger_claimed_pruning_period_work =
            proof_metadata.relay_block_blue_work.saturating_sub(challenger_pp_header.blue_work);

        for (level_idx, challenger_selected_tip_at_level) in challenger_selected_tip_by_level.iter().copied().enumerate() {
            let level = level_idx as BlockLevel;
            self.validate_proof_selected_tip(challenger_selected_tip_at_level, level, challenger_pp_header)?;

            let challenger_selected_tip_gd =
                challenger_ghostdag_stores[level_idx].get_compact_data(challenger_selected_tip_at_level).unwrap();

            // Next check is to see if the challenger's proof is "better" than the defender's
            // Step 1 - look at only levels that have a full proof (least 2m blocks in the proof)
            if challenger_selected_tip_gd.blue_score < 2 * self.pruning_proof_m {
                continue;
            }

            // Step 2 - if a common ancestor exists between the challenger and defender proofs,
            // compare their accumulated blue work from that ancestor onward.
            // The challenger proof is better iff the blue work difference from the ancestor
            // to the challenger's selected tip, plus its pruning-period work, is strictly
            // greater than the corresponding defender value.

            // Step 2 - if we can find a common ancestor between the challenger's proof and defender's proof
            // we can determine if the challenger's is better. The challenger proof is better if the blue work difference between the
            // defender's tips and the common ancestor, combined with the pruning period work, is less than the blue work difference between the
            // challenger's tip and the common ancestor (from its pov) combined with its own claimed pruning period work.
            if let Some((challenger_common_ancestor_gd, defender_common_ancestor_gd)) = self
                .find_challenger_and_defender_common_ancestor_ghostdag_data(
                    &challenger_ghostdag_stores,
                    &defender_ghostdag_stores,
                    challenger_selected_tip_at_level,
                    level,
                    challenger_selected_tip_gd,
                )
            {
                let defender_level_blue_work =
                    defender_ghostdag_stores[level_idx].get_blue_work(defender_selected_tip_by_level[level_idx]).unwrap();
                let challenger_level_blue_work_diff =
                    challenger_selected_tip_gd.blue_work.saturating_sub(challenger_common_ancestor_gd.blue_work);
                let defender_level_blue_work_diff = defender_level_blue_work.saturating_sub(defender_common_ancestor_gd.blue_work);
                if defender_level_blue_work_diff.saturating_add(defender_pruning_period_work)
                    >= challenger_level_blue_work_diff.saturating_add(challenger_claimed_pruning_period_work)
                {
                    return Err(PruningImportError::PruningProofInsufficientBlueWork);
                }

                return Ok(());
            }
        }

        if defender_pp == self.genesis_hash {
            // If the challenger has better tips and the defender's pruning point is still
            // genesis, we consider the challenger to be better.
            return Ok(());
        }

        // If we got here it means there's no level with shared blocks
        // between the challenger and the defender. In this case we
        // consider the challenger to be better if it has at least one level
        // with 2*self.pruning_proof_m blue blocks where the defender doesn't.
        for level in (0..=self.max_block_level).rev() {
            let level_idx = level as usize;

            let challenger_selected_tip = challenger_selected_tip_by_level[level_idx];
            let challenger_selected_tip_gd = challenger_ghostdag_stores[level_idx].get_compact_data(challenger_selected_tip).unwrap();
            if challenger_selected_tip_gd.blue_score < 2 * self.pruning_proof_m {
                continue;
            }

            if defender_ghostdag_stores[level_idx].get_blue_score(defender_selected_tip_by_level[level_idx]).unwrap()
                < 2 * self.pruning_proof_m
            {
                return Ok(());
            }
        }

        drop(pruning_read);
        drop(challenger_stores_and_processes.db_lifetime);
        drop(defender_stores_and_processes.db_lifetime);

        Err(PruningImportError::PruningProofNotEnoughHeaders)
    }

    fn init_validate_pruning_point_proof_stores_and_processes(
        &self,
        proof: &PruningPointProof,
    ) -> PruningImportResult<TempProofContext> {
        if proof[0].is_empty() {
            return Err(PruningImportError::PruningProofNotEnoughHeaders);
        }

        let ghostdag_k = self.ghostdag_k;

        let headers_estimate = self.estimate_proof_unique_size(proof);

        let (db_lifetime, db) = kaspa_database::create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let cache_policy = CachePolicy::Count(2 * self.pruning_proof_m as usize);
        let headers_store =
            Arc::new(DbHeadersStore::new(db.clone(), CachePolicy::Count(headers_estimate), CachePolicy::Count(headers_estimate)));
        let ghostdag_stores = (0..=self.max_block_level)
            .map(|level| Arc::new(DbGhostdagStore::new(db.clone(), level, cache_policy, cache_policy)))
            .collect_vec();
        let mut relations_stores =
            (0..=self.max_block_level).map(|level| DbRelationsStore::new(db.clone(), level, cache_policy, cache_policy)).collect_vec();
        let reachability_stores = (0..=self.max_block_level)
            .map(|level| Arc::new(RwLock::new(DbReachabilityStore::with_block_level(db.clone(), cache_policy, cache_policy, level))))
            .collect_vec();

        let reachability_services = (0..=self.max_block_level)
            .map(|level| MTReachabilityService::new(reachability_stores[level as usize].clone()))
            .collect_vec();

        let ghostdag_managers = ghostdag_stores
            .iter()
            .cloned()
            .enumerate()
            .map(|(level, ghostdag_store)| {
                GhostdagManager::with_level(
                    self.genesis_hash,
                    ghostdag_k,
                    ghostdag_store,
                    relations_stores[level].clone(),
                    headers_store.clone(),
                    reachability_services[level].clone(),
                    level as BlockLevel,
                    self.max_block_level,
                )
            })
            .collect_vec();

        {
            let mut batch = WriteBatch::default();
            for level in 0..=self.max_block_level {
                let level = level as usize;
                reachability::init(reachability_stores[level].write().deref_mut()).unwrap();
                relations_stores[level].insert_batch(&mut batch, ORIGIN, BlockHashes::new(vec![])).unwrap();
                ghostdag_stores[level].insert(ORIGIN, ghostdag_managers[level].origin_ghostdag_data()).unwrap();
            }

            db.write(batch).unwrap();
        }

        Ok(TempProofContext { db_lifetime, headers_store, ghostdag_stores, relations_stores, reachability_stores, ghostdag_managers })
    }

    fn populate_stores_for_validate_pruning_point_proof(
        &self,
        proof: &PruningPointProof,
        ctx: &mut TempProofContext,
        log_validating: bool,
    ) -> PruningImportResult<Vec<Hash>> {
        let headers_store = &ctx.headers_store;
        let ghostdag_stores = &ctx.ghostdag_stores;
        let mut relations_stores = ctx.relations_stores.clone();
        let reachability_stores = &ctx.reachability_stores;
        let ghostdag_managers = &ctx.ghostdag_managers;

        let proof_pp_header = proof[0].last().expect("checked if empty");
        let proof_pp = proof_pp_header.hash;

        let mut selected_tip_by_level = vec![None; self.max_block_level as usize + 1];
        for level in (0..=self.max_block_level).rev() {
            // Before processing this level, check if the process is exiting so we can end early
            if self.is_consensus_exiting.load(Ordering::Relaxed) {
                return Err(PruningImportError::PruningValidationInterrupted);
            }

            if log_validating {
                info!("Validating level {level} from the pruning point proof ({} headers)", proof[level as usize].len());
            }
            let level_idx = level as usize;
            let mut selected_tip = None;
            for (i, header) in proof[level as usize].iter().enumerate() {
                let (header_level, pow_passes) = calc_block_level_check_pow(header, self.max_block_level);
                if header_level < level {
                    return Err(PruningImportError::PruningProofWrongBlockLevel(header.hash, header_level, level));
                }
                if !pow_passes {
                    return Err(PruningImportError::ProofOfWorkFailed(header.hash, level));
                }

                headers_store.insert(header.hash, header.clone(), header_level).idempotent().unwrap();

                // filter out parents that do not appear at the pruning proof:
                let parents = self
                    .parents_manager
                    .parents_at_level(header, level)
                    .iter()
                    .copied()
                    .filter(|parent| ghostdag_stores[level_idx].has(*parent).unwrap())
                    .collect_vec();

                // Only the first block at each level is allowed to have no known parents
                if parents.is_empty() && i != 0 {
                    return Err(PruningImportError::PruningProofHeaderWithNoKnownParents(header.hash, level));
                }

                let parents: BlockHashes = parents.push_if_empty(ORIGIN).into();

                if relations_stores[level_idx].has(header.hash).unwrap() {
                    return Err(PruningImportError::PruningProofDuplicateHeaderAtLevel(header.hash, level));
                }

                relations_stores[level_idx].insert(header.hash, parents.clone()).unwrap();
                let ghostdag_data = Arc::new(ghostdag_managers[level_idx].ghostdag(&parents));
                ghostdag_stores[level_idx].insert(header.hash, ghostdag_data.clone()).unwrap();
                selected_tip = Some(match selected_tip {
                    Some(tip) => ghostdag_managers[level_idx].find_selected_parent([tip, header.hash]),
                    None => header.hash,
                });

                let mut reachability_mergeset = {
                    let reachability_read = reachability_stores[level_idx].read();
                    ghostdag_data
                        .unordered_mergeset_without_selected_parent()
                        .filter(|hash| reachability_read.has(*hash).unwrap())
                        .collect_vec() // We collect to vector so reachability_read can be released and let `reachability::add_block` use a write lock.
                        .into_iter()
                };
                reachability::add_block(
                    reachability_stores[level_idx].write().deref_mut(),
                    header.hash,
                    ghostdag_data.selected_parent,
                    &mut reachability_mergeset,
                )
                .unwrap();

                if selected_tip.unwrap() == header.hash {
                    reachability::hint_virtual_selected_parent(reachability_stores[level_idx].write().deref_mut(), header.hash)
                        .unwrap();
                }
            }

            if level < self.max_block_level {
                let block_at_depth_m_at_next_level = self
                    .block_at_depth(
                        &*ghostdag_stores[level_idx + 1],
                        selected_tip_by_level[level_idx + 1].unwrap(),
                        self.pruning_proof_m,
                    )
                    .unwrap();
                if !relations_stores[level_idx].has(block_at_depth_m_at_next_level).unwrap() {
                    return Err(PruningImportError::PruningProofMissingBlockAtDepthMFromNextLevel(level, level + 1));
                }
            }

            if selected_tip.unwrap() != proof_pp
                && !self.parents_manager.parents_at_level(proof_pp_header, level).contains(&selected_tip.unwrap())
            {
                return Err(PruningImportError::PruningProofMissesBlocksBelowPruningPoint(selected_tip.unwrap(), level));
            }

            selected_tip_by_level[level_idx] = selected_tip;
        }

        Ok(selected_tip_by_level.into_iter().map(|selected_tip| selected_tip.unwrap()).collect())
    }

    fn validate_proof_selected_tip(
        &self,
        proof_selected_tip_at_level: Hash,
        level: BlockLevel,
        proof_pp_header: &Header,
    ) -> PruningImportResult<()> {
        // A proof selected tip of some level has to be the proof suggested pruning point itself if its level
        // is lower or equal to the pruning point level, or a parent of the pruning point on the relevant level
        // otherwise.
        let proof_pp_level = calc_block_level(proof_pp_header, self.max_block_level);

        if level <= proof_pp_level {
            if proof_selected_tip_at_level != proof_pp_header.hash {
                return Err(PruningImportError::PruningProofSelectedTipIsNotThePruningPoint(proof_selected_tip_at_level, level));
            }
        } else if !self.parents_manager.parents_at_level(proof_pp_header, level).contains(&proof_selected_tip_at_level) {
            return Err(PruningImportError::PruningProofSelectedTipNotParentOfPruningPoint(proof_selected_tip_at_level, level));
        }

        Ok(())
    }

    // find_challenger_and_defender_common_ancestor_ghostdag_data returns an option of a tuple
    // that contains the ghostdag data of the challenger and defender's common ancestor. If no
    // such ancestor exists, it returns None.
    fn find_challenger_and_defender_common_ancestor_ghostdag_data(
        &self,
        challenger_ghostdag_stores: &[Arc<DbGhostdagStore>],
        defender_ghostdag_stores: &[Arc<DbGhostdagStore>],
        challenger_selected_tip: Hash,
        level: BlockLevel,
        challenger_selected_tip_gd: CompactGhostdagData,
    ) -> Option<(CompactGhostdagData, CompactGhostdagData)> {
        let mut current = challenger_selected_tip;
        let mut challenger_gd_of_current = challenger_selected_tip_gd;
        loop {
            match defender_ghostdag_stores[level as usize].get_compact_data(current).optional().unwrap() {
                Some(defender_gd_of_current) => {
                    break Some((challenger_gd_of_current, defender_gd_of_current));
                }
                None => {
                    current = challenger_gd_of_current.selected_parent;
                    if current.is_origin() {
                        break None;
                    }
                    challenger_gd_of_current = challenger_ghostdag_stores[level as usize].get_compact_data(current).unwrap();
                }
            };
        }
    }
}
