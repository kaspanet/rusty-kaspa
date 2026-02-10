use std::{
    ops::{ControlFlow, DerefMut},
    sync::{Arc, atomic::Ordering},
};

use itertools::Itertools;
use kaspa_consensus_core::{
    BlockLevel, BlueWorkType,
    blockhash::{BlockHashExtensions, BlockHashes, ORIGIN},
    errors::pruning::{ProofWeakness, PruningImportError, PruningImportResult},
    header::Header,
    pruning::{PruningPointProof, PruningProofMetadata},
};
use kaspa_core::info;
use kaspa_database::{
    prelude::{CachePolicy, ConnBuilder, StoreResultUnitExt},
    utils::DbLifetime,
};
use kaspa_hashes::Hash;
use kaspa_pow::{calc_block_level, calc_block_level_check_pow};
use kaspa_utils::vec::VecExtensions;
use parking_lot::RwLock;
use rocksdb::WriteBatch;

use crate::{
    model::{
        services::reachability::MTReachabilityService,
        stores::{
            ghostdag::{DbGhostdagStore, GhostdagStore, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStore, HeaderStoreReader},
            headers_selected_tip::HeadersSelectedTipStoreReader,
            reachability::{DbReachabilityStore, ReachabilityStoreReader},
            relations::{DbRelationsStore, RelationsStoreReader},
        },
    },
    processes::{
        ghostdag::protocol::GhostdagManager, pruning_proof::GhostdagReaderExt, reachability::inquirer as reachability,
        relations::RelationsStoreExtensions,
    },
};

use super::PruningProofManager;

struct ProofContext {
    _headers_store: Arc<DbHeadersStore>,
    ghostdag_stores: Vec<Arc<DbGhostdagStore>>,
    _relations_stores: Vec<DbRelationsStore>,
    _reachability_stores: Vec<Arc<RwLock<DbReachabilityStore>>>,
    _ghostdag_managers:
        Vec<GhostdagManager<DbGhostdagStore, DbRelationsStore, MTReachabilityService<DbReachabilityStore>, DbHeadersStore>>,
    selected_tip_by_level: Vec<Hash>,

    pp_header: Arc<Header>,
    _pp_level: BlockLevel,

    _db_lifetime: DbLifetime,
}

struct ProofLevelContext<'a> {
    ghostdag_store: &'a DbGhostdagStore,
    selected_tip: Hash,
}

impl ProofLevelContext<'_> {
    /// Returns an option of the hash of the challenger and defender's common ancestor at this level.
    /// If no such ancestor exists, returns None.
    fn find_common_ancestor(challenger: &Self, defender: &Self) -> Option<Hash> {
        let mut current = challenger.selected_tip;
        let mut challenger_gd_of_current = challenger.ghostdag_store.get_compact_data(current).unwrap();
        loop {
            if defender.ghostdag_store.has(current).unwrap() {
                break Some(current);
            } else {
                current = challenger_gd_of_current.selected_parent;
                if current.is_origin() {
                    break None;
                }
                challenger_gd_of_current = challenger.ghostdag_store.get_compact_data(current).unwrap();
            };
        }
    }

    /// Returns the blue work difference between the level selected tip and `ancestor`
    fn blue_work_diff(&self, ancestor: Hash) -> BlueWorkType {
        self.ghostdag_store
            .get_blue_work(self.selected_tip)
            .unwrap()
            .saturating_sub(self.ghostdag_store.get_blue_work(ancestor).unwrap())
    }

    /// Returns the overall blue score for this level (essentially the level selected tip blue score)
    fn blue_score(&self) -> u64 {
        self.ghostdag_store.get_blue_score(self.selected_tip).unwrap()
    }
}

impl ProofContext {
    /// Build the full context from the proof
    fn from_proof(
        ppm: &PruningProofManager,
        proof: &PruningPointProof,
        log_validating: bool,
    ) -> Result<ControlFlow<(), ProofContext>, PruningImportError> {
        if proof.len() != ppm.max_block_level as usize + 1 {
            return Err(PruningImportError::ProofNotEnoughLevels(ppm.max_block_level as usize + 1));
        }

        if proof[0].is_empty() {
            return Err(PruningImportError::PruningProofNotEnoughHeaders);
        }

        let ghostdag_k = ppm.ghostdag_k;

        let headers_estimate = ppm.estimate_proof_unique_size(proof);

        //
        // Initialize stores
        //

        let (db_lifetime, db) = kaspa_database::create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let cache_policy = CachePolicy::Count(2 * ppm.pruning_proof_m as usize);
        let headers_store =
            Arc::new(DbHeadersStore::new(db.clone(), CachePolicy::Count(headers_estimate), CachePolicy::Count(headers_estimate)));
        let ghostdag_stores = (0..=ppm.max_block_level)
            .map(|level| Arc::new(DbGhostdagStore::new(db.clone(), level, cache_policy, cache_policy)))
            .collect_vec();
        let mut relations_stores =
            (0..=ppm.max_block_level).map(|level| DbRelationsStore::new(db.clone(), level, cache_policy, cache_policy)).collect_vec();
        let reachability_stores = (0..=ppm.max_block_level)
            .map(|level| Arc::new(RwLock::new(DbReachabilityStore::with_block_level(db.clone(), cache_policy, cache_policy, level))))
            .collect_vec();

        let reachability_services = (0..=ppm.max_block_level)
            .map(|level| MTReachabilityService::new(reachability_stores[level as usize].clone()))
            .collect_vec();

        let ghostdag_managers = ghostdag_stores
            .iter()
            .cloned()
            .enumerate()
            .map(|(level, ghostdag_store)| {
                GhostdagManager::with_level(
                    ppm.genesis_hash,
                    ghostdag_k,
                    ghostdag_store,
                    relations_stores[level].clone(),
                    headers_store.clone(),
                    reachability_services[level].clone(),
                    level as BlockLevel,
                    ppm.max_block_level,
                )
            })
            .collect_vec();

        {
            let mut batch = WriteBatch::default();
            for level in 0..=ppm.max_block_level {
                let level = level as usize;
                reachability::init(reachability_stores[level].write().deref_mut()).unwrap();
                relations_stores[level].insert_batch(&mut batch, ORIGIN, BlockHashes::new(vec![])).unwrap();
                ghostdag_stores[level].insert(ORIGIN, ghostdag_managers[level].origin_ghostdag_data()).unwrap();
            }

            db.write(batch).unwrap();
        }

        let proof_pp_header = proof[0].last().expect("checked if empty").clone();
        let proof_pp_level = calc_block_level(&proof_pp_header, ppm.max_block_level);
        let proof_pp = proof_pp_header.hash;

        //
        // Populate stores
        //

        let mut selected_tip_by_level = vec![None; ppm.max_block_level as usize + 1];
        for level in (0..=ppm.max_block_level).rev() {
            // Before processing this level, check if the process is exiting so we can end early
            if ppm.is_consensus_exiting.load(Ordering::Relaxed) {
                return Ok(ControlFlow::Break(()));
            }

            if log_validating {
                info!("Validating level {level} from the pruning point proof ({} headers)", proof[level as usize].len());
            }
            let level_idx = level as usize;
            let mut selected_tip =
                proof[level as usize].first().map(|header| header.hash).ok_or(PruningImportError::PruningProofNotEnoughHeaders)?;
            for (i, header) in proof[level as usize].iter().enumerate() {
                let (header_level, pow_passes) = calc_block_level_check_pow(header, ppm.max_block_level);
                if header_level < level {
                    return Err(PruningImportError::PruningProofWrongBlockLevel(header.hash, header_level, level));
                }
                if !ppm.skip_proof_of_work && !pow_passes {
                    return Err(PruningImportError::ProofOfWorkFailed(header.hash, level));
                }

                headers_store.insert(header.hash, header.clone(), header_level).idempotent().unwrap();

                // Filter out parents that do not appear at the pruning proof:
                let parents = ppm
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

                for &parent in parents.iter() {
                    if headers_store.get_header(parent).unwrap().blue_work >= header.blue_work {
                        return Err(PruningImportError::PruningProofInconsistentBlueWork(header.hash, level));
                    }
                }

                let parents: BlockHashes = parents.push_if_empty(ORIGIN).into();

                if relations_stores[level_idx].has(header.hash).unwrap() {
                    return Err(PruningImportError::PruningProofDuplicateHeaderAtLevel(header.hash, level));
                }

                relations_stores[level_idx].insert(header.hash, parents.clone()).unwrap();
                let ghostdag_data = Arc::new(ghostdag_managers[level_idx].ghostdag(&parents));
                ghostdag_stores[level_idx].insert(header.hash, ghostdag_data.clone()).unwrap();

                // Update the selected tip
                selected_tip = ghostdag_managers[level_idx].find_selected_parent([selected_tip, header.hash]);

                let mut level_reachability = reachability_stores[level_idx].write();
                let mut reachability_mergeset = ghostdag_data
                    .unordered_mergeset_without_selected_parent()
                    .filter(|hash| level_reachability.has(*hash).unwrap())
                    .collect_vec()
                    .into_iter();

                reachability::add_block(
                    level_reachability.deref_mut(),
                    header.hash,
                    ghostdag_data.selected_parent,
                    &mut reachability_mergeset,
                )
                .unwrap();

                if selected_tip == header.hash {
                    reachability::hint_virtual_selected_parent(level_reachability.deref_mut(), header.hash).unwrap();
                }
                drop(level_reachability);
            }

            if level < ppm.max_block_level {
                let block_at_depth_m_at_next_level = ghostdag_stores[level_idx + 1]
                    .block_at_depth(selected_tip_by_level[level_idx + 1].unwrap(), ppm.pruning_proof_m)
                    .unwrap();
                if !relations_stores[level_idx].has(block_at_depth_m_at_next_level).unwrap() {
                    return Err(PruningImportError::PruningProofMissingBlockAtDepthMFromNextLevel(level, level + 1));
                }
            }

            // The selected tip at a given level must be anchored to the pruning point:
            // - At levels ≤ the pruning-point level, the selected tip must be the pruning point itself.
            // - At higher levels, it must be a parent of the pruning point at that level.
            if level <= proof_pp_level {
                if selected_tip != proof_pp {
                    return Err(PruningImportError::PruningProofSelectedTipIsNotThePruningPoint(selected_tip, level));
                }
            } else if !ppm.parents_manager.parents_at_level(&proof_pp_header, level).contains(&selected_tip) {
                return Err(PruningImportError::PruningProofSelectedTipNotParentOfPruningPoint(selected_tip, level));
            }

            let tip_blue_score = ghostdag_stores[level_idx].get_blue_score(selected_tip).expect("tip expected");
            let level_root = proof[level_idx].first().expect("checked earlier").hash;
            if level_root != ppm.genesis_hash && tip_blue_score < 2 * ppm.pruning_proof_m {
                return Err(PruningImportError::PruningProofSelectedTipNotEnoughBlueScore(selected_tip, level, tip_blue_score));
            }

            selected_tip_by_level[level_idx] = Some(selected_tip);
        }

        let selected_tip_by_level = selected_tip_by_level.into_iter().map(|selected_tip| selected_tip.unwrap()).collect();

        let ctx = ProofContext {
            _db_lifetime: db_lifetime,
            _headers_store: headers_store,
            ghostdag_stores,
            _relations_stores: relations_stores,
            _reachability_stores: reachability_stores,
            _ghostdag_managers: ghostdag_managers,
            selected_tip_by_level,
            pp_header: proof_pp_header,
            _pp_level: proof_pp_level,
        };

        Ok(ControlFlow::Continue(ctx))
    }

    /// Returns a per-level context
    fn level(&self, level: BlockLevel) -> ProofLevelContext<'_> {
        ProofLevelContext {
            ghostdag_store: &self.ghostdag_stores[level as usize],
            selected_tip: self.selected_tip_by_level[level as usize],
        }
    }
}

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
        // Initialize the stores for the incoming pruning proof (the challenger)
        let challenger =
            ProofContext::from_proof(self, proof, true)?.continue_value().ok_or(PruningImportError::PruningValidationInterrupted)?;

        // Get the proof for the current consensus (the defender) and recreate the stores for it
        // This is expected to be fast because if a proof exists, it will be cached.
        let defender_proof = self.get_pruning_point_proof();
        let defender = ProofContext::from_proof(self, &defender_proof, false)
            .expect("local")
            .continue_value()
            .ok_or(PruningImportError::PruningValidationInterrupted)?;

        Ok(self.compare_proofs_inner(
            defender,
            challenger,
            self.headers_selected_tip_store.read().get().unwrap().blue_work,
            proof_metadata.relay_block_blue_work,
        )?)
    }

    /// Compares two MLS pruning proofs and determines whether the challenger supersedes the defender.
    ///
    /// The comparison is performed level-by-level, considering only levels that satisfy the
    /// ≥2M threshold. When a common ancestor exists at a given level, the proofs are
    /// compared by their accumulated blue work from that ancestor onward, including the
    /// respective pruning-period work; otherwise, if no common ancestor is found, the
    /// challenger is considered better only if it possesses a qualifying level where the
    /// defender does not.
    ///
    /// The challenger is considered better only if it is *strictly* superior according to
    /// these criteria. In case of equality, or when no strict advantage can be established,
    /// the defender is favored to preserve stability.
    fn compare_proofs_inner(
        &self,
        defender: ProofContext,
        challenger: ProofContext,
        defender_relay_blue_work: BlueWorkType,
        challenger_relay_blue_work: BlueWorkType,
    ) -> Result<(), ProofWeakness> {
        // The accumulated blue work of the defender's proof from the pruning point onward
        let defender_pruning_period_work = defender_relay_blue_work.saturating_sub(defender.pp_header.blue_work);

        // The claimed blue work of the challenger's proof from their pruning point and up to the triggering relay block. This work
        // will eventually be verified if the proof is accepted so we can treat it as trusted
        let challenger_claimed_pruning_period_work = challenger_relay_blue_work.saturating_sub(challenger.pp_header.blue_work);

        for level in 0..=self.max_block_level {
            // Init level ctxs
            let challenger_level_ctx = challenger.level(level);
            let defender_level_ctx = defender.level(level);

            // Next check is to see if the challenger's proof is "better" than the defender's
            // Step 1 - look only at levels that have a full proof (at least 2M blocks)
            if challenger_level_ctx.blue_score() < 2 * self.pruning_proof_m {
                continue;
            }

            // Step 2 - if a common ancestor exists between the challenger and defender proofs,
            // compare their accumulated blue work from that ancestor onward.
            // The challenger proof is better iff the blue work difference from the ancestor
            // to the challenger's selected tip, plus its pruning-period work, is strictly
            // greater than the corresponding defender value.
            if let Some(common_ancestor) = ProofLevelContext::find_common_ancestor(&challenger_level_ctx, &defender_level_ctx) {
                if defender_level_ctx.blue_work_diff(common_ancestor).saturating_add(defender_pruning_period_work)
                    >= challenger_level_ctx.blue_work_diff(common_ancestor).saturating_add(challenger_claimed_pruning_period_work)
                {
                    return Err(ProofWeakness::InsufficientBlueWork);
                }

                return Ok(());
            }
        }

        if defender.pp_header.hash == self.genesis_hash {
            // If the challenger has better tips and the defender's pruning point is still
            // genesis, we consider the challenger to be better.
            return Ok(());
        }

        // If we got here it means there's no level with shared blocks
        // between the challenger and the defender. In this case we
        // consider the challenger to be better if it has at least one level
        // with 2M blue blocks where the defender doesn't.
        for level in (0..=self.max_block_level).rev() {
            if challenger.level(level).blue_score() < 2 * self.pruning_proof_m {
                continue;
            }
            if defender.level(level).blue_score() < 2 * self.pruning_proof_m {
                return Ok(());
            }
        }

        drop(challenger);
        drop(defender);

        Err(ProofWeakness::NotEnoughHeaders)
    }

    /// Compares two MLS pruning proofs and determines whether the challenger supersedes the defender.
    ///
    /// See [`PruningProofManager::compare_proofs_inner`] for more details.
    ///
    /// Exposed here for local revalidation needs.
    pub(crate) fn _compare_proofs(
        &self,
        defender: &PruningPointProof,
        challenger: &PruningPointProof,
        defender_relay_blue_work: BlueWorkType,
        challenger_relay_blue_work: BlueWorkType,
    ) -> ControlFlow<(), Result<(), ProofWeakness>> {
        ControlFlow::Continue(self.compare_proofs_inner(
            ProofContext::from_proof(self, defender, false).expect("local")?,
            ProofContext::from_proof(self, challenger, false).expect("local")?,
            defender_relay_blue_work,
            challenger_relay_blue_work,
        ))
    }
}
