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
use kaspa_database::prelude::{CachePolicy, ConnBuilder, StoreResultEmptyTuple, StoreResultExtensions};
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
    pub fn validate_pruning_point_proof(
        &self,
        proof: &PruningPointProof,
        proof_metadata: &PruningProofMetadata,
    ) -> PruningImportResult<()> {
        if proof.len() != self.max_block_level as usize + 1 {
            return Err(PruningImportError::ProofNotEnoughLevels(self.max_block_level as usize + 1));
        }

        // Initialize the stores for the proof
        let mut proof_stores_and_processes = self.init_validate_pruning_point_proof_stores_and_processes(proof)?;
        let proof_pp_header = proof[0].last().expect("checked if empty");
        let proof_pp = proof_pp_header.hash;
        let proof_pp_level = calc_block_level(proof_pp_header, self.max_block_level);
        let proof_selected_tip_by_level =
            self.populate_stores_for_validate_pruning_point_proof(proof, &mut proof_stores_and_processes, true)?;
        let proof_ghostdag_stores = proof_stores_and_processes.ghostdag_stores;

        // Get the proof for the current consensus and recreate the stores for it
        // This is expected to be fast because if a proof exists, it will be cached.
        // If no proof exists, this is empty
        let mut current_consensus_proof = self.get_pruning_point_proof();
        if current_consensus_proof.is_empty() {
            // An empty proof can only happen if we're at genesis. We're going to create a proof for this case that contains the genesis header only
            let genesis_header = self.headers_store.get_header(self.genesis_hash).unwrap();
            current_consensus_proof = Arc::new((0..=self.max_block_level).map(|_| vec![genesis_header.clone()]).collect_vec());
        }
        let mut current_consensus_stores_and_processes =
            self.init_validate_pruning_point_proof_stores_and_processes(&current_consensus_proof)?;
        let _ = self.populate_stores_for_validate_pruning_point_proof(
            &current_consensus_proof,
            &mut current_consensus_stores_and_processes,
            false,
        )?;
        let current_consensus_ghostdag_stores = current_consensus_stores_and_processes.ghostdag_stores;

        let pruning_read = self.pruning_point_store.read();
        let relations_read = self.relations_stores.read();
        let current_pp = pruning_read.get().unwrap().pruning_point;
        let current_pp_header = self.headers_store.get_header(current_pp).unwrap();

        // The accumulated blue work of current consensus from the pruning point onward
        let pruning_period_work =
            self.headers_selected_tip_store.read().get().unwrap().blue_work.saturating_sub(current_pp_header.blue_work);
        // The claimed blue work of the prover from his pruning point and up to the triggering relay block. This work
        // will eventually be verified if the proof is accepted so we can treat it as trusted
        let prover_claimed_pruning_period_work = proof_metadata.relay_block_blue_work.saturating_sub(proof_pp_header.blue_work);

        for (level_idx, selected_tip) in proof_selected_tip_by_level.iter().copied().enumerate() {
            let level = level_idx as BlockLevel;
            self.validate_proof_selected_tip(selected_tip, level, proof_pp_level, proof_pp, proof_pp_header)?;

            let proof_selected_tip_gd = proof_ghostdag_stores[level_idx].get_compact_data(selected_tip).unwrap();

            // Next check is to see if this proof is "better" than what's in the current consensus
            // Step 1 - look at only levels that have a full proof (least 2m blocks in the proof)
            if proof_selected_tip_gd.blue_score < 2 * self.pruning_proof_m {
                continue;
            }

            // Step 2 - if we can find a common ancestor between the proof and current consensus
            // we can determine if the proof is better. The proof is better if the blue work* difference between the
            // old current consensus's tips and the common ancestor is less than the blue work difference between the
            // proof's tip and the common ancestor.
            if let Some((proof_common_ancestor_gd, common_ancestor_gd)) = self.find_proof_and_consensus_common_ancestor_ghostdag_data(
                &proof_ghostdag_stores,
                &current_consensus_ghostdag_stores,
                selected_tip,
                level,
                proof_selected_tip_gd,
            ) {
                let proof_level_blue_work_diff = proof_selected_tip_gd.blue_work.saturating_sub(proof_common_ancestor_gd.blue_work);
                for parent in self.parents_manager.parents_at_level(&current_pp_header, level).iter().copied() {
                    // Not all parents by level are guaranteed to be GD populated, but at least one of them will (the proof level selected tip)
                    if let Some(parent_blue_work) = current_consensus_ghostdag_stores[level_idx].get_blue_work(parent).unwrap_option()
                    {
                        let parent_blue_work_diff = parent_blue_work.saturating_sub(common_ancestor_gd.blue_work);
                        if parent_blue_work_diff.saturating_add(pruning_period_work)
                            >= proof_level_blue_work_diff.saturating_add(prover_claimed_pruning_period_work)
                        {
                            return Err(PruningImportError::PruningProofInsufficientBlueWork);
                        }
                    }
                }

                return Ok(());
            }
        }

        if current_pp == self.genesis_hash {
            // If the proof has better tips and the current pruning point is still
            // genesis, we consider the proof state to be better.
            return Ok(());
        }

        // If we got here it means there's no level with shared blocks
        // between the proof and the current consensus. In this case we
        // consider the proof to be better if it has at least one level
        // with 2*self.pruning_proof_m blue blocks where consensus doesn't.
        for level in (0..=self.max_block_level).rev() {
            let level_idx = level as usize;

            let proof_selected_tip = proof_selected_tip_by_level[level_idx];
            let proof_selected_tip_gd = proof_ghostdag_stores[level_idx].get_compact_data(proof_selected_tip).unwrap();
            if proof_selected_tip_gd.blue_score < 2 * self.pruning_proof_m {
                continue;
            }

            match relations_read[level_idx].get_parents(current_pp).unwrap_option() {
                Some(parents) => {
                    if parents.iter().copied().any(|parent| {
                        current_consensus_ghostdag_stores[level_idx].get_blue_score(parent).unwrap() < 2 * self.pruning_proof_m
                    }) {
                        return Ok(());
                    }
                }
                None => {
                    // If the current pruning point doesn't have a parent at this level, we consider the proof state to be better.
                    return Ok(());
                }
            }
        }

        drop(pruning_read);
        drop(relations_read);
        drop(proof_stores_and_processes.db_lifetime);
        drop(current_consensus_stores_and_processes.db_lifetime);

        Err(PruningImportError::PruningProofNotEnoughHeaders)
    }

    fn init_validate_pruning_point_proof_stores_and_processes(
        &self,
        proof: &PruningPointProof,
    ) -> PruningImportResult<TempProofContext> {
        if proof[0].is_empty() {
            return Err(PruningImportError::PruningProofNotEnoughHeaders);
        }

        // [Crescendo]: decide on ghostdag K based on proof pruning point DAA score
        let proof_pp_daa_score = proof[0].last().expect("checked if empty").daa_score;
        let ghostdag_k = self.ghostdag_k.get(proof_pp_daa_score);

        let headers_estimate = self.estimate_proof_unique_size(proof);

        let (db_lifetime, db) = kaspa_database::create_temp_db!(ConnBuilder::default().with_files_limit(10)).unwrap();
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

                headers_store.insert(header.hash, header.clone(), header_level).unwrap_or_exists();

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
        proof_selected_tip: Hash,
        level: BlockLevel,
        proof_pp_level: BlockLevel,
        proof_pp: Hash,
        proof_pp_header: &Header,
    ) -> PruningImportResult<()> {
        // A proof selected tip of some level has to be the proof suggested prunint point itself if its level
        // is lower or equal to the pruning point level, or a parent of the pruning point on the relevant level
        // otherwise.
        if level <= proof_pp_level {
            if proof_selected_tip != proof_pp {
                return Err(PruningImportError::PruningProofSelectedTipIsNotThePruningPoint(proof_selected_tip, level));
            }
        } else if !self.parents_manager.parents_at_level(proof_pp_header, level).contains(&proof_selected_tip) {
            return Err(PruningImportError::PruningProofSelectedTipNotParentOfPruningPoint(proof_selected_tip, level));
        }

        Ok(())
    }

    // find_proof_and_consensus_common_chain_ancestor_ghostdag_data returns an option of a tuple
    // that contains the ghostdag data of the proof and current consensus common ancestor. If no
    // such ancestor exists, it returns None.
    fn find_proof_and_consensus_common_ancestor_ghostdag_data(
        &self,
        proof_ghostdag_stores: &[Arc<DbGhostdagStore>],
        current_consensus_ghostdag_stores: &[Arc<DbGhostdagStore>],
        proof_selected_tip: Hash,
        level: BlockLevel,
        proof_selected_tip_gd: CompactGhostdagData,
    ) -> Option<(CompactGhostdagData, CompactGhostdagData)> {
        let mut proof_current = proof_selected_tip;
        let mut proof_current_gd = proof_selected_tip_gd;
        loop {
            match current_consensus_ghostdag_stores[level as usize].get_compact_data(proof_current).unwrap_option() {
                Some(current_gd) => {
                    break Some((proof_current_gd, current_gd));
                }
                None => {
                    proof_current = proof_current_gd.selected_parent;
                    if proof_current.is_origin() {
                        break None;
                    }
                    proof_current_gd = proof_ghostdag_stores[level as usize].get_compact_data(proof_current).unwrap();
                }
            };
        }
    }
}
