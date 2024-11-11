use std::{
    cmp::Reverse,
    collections::{hash_map::Entry::Vacant, BinaryHeap, HashSet},
    sync::Arc,
};

use itertools::Itertools;
use kaspa_consensus_core::{
    blockhash::{BlockHashes, ORIGIN},
    errors::pruning::{PruningImportError, PruningImportResult},
    header::Header,
    pruning::PruningPointProof,
    trusted::TrustedBlock,
    BlockHashMap, BlockHashSet, BlockLevel, HashMapCustomHasher,
};
use kaspa_core::{debug, trace};
use kaspa_hashes::Hash;
use kaspa_pow::calc_block_level;
use kaspa_utils::{binary_heap::BinaryHeapExtensions, vec::VecExtensions};
use rocksdb::WriteBatch;

use crate::{
    model::{
        services::reachability::ReachabilityService,
        stores::{
            ghostdag::{GhostdagData, GhostdagStore},
            headers::HeaderStore,
            reachability::StagingReachabilityStore,
            relations::StagingRelationsStore,
            selected_chain::SelectedChainStore,
            virtual_state::{VirtualState, VirtualStateStore},
        },
    },
    processes::{
        ghostdag::{mergeset::unordered_mergeset_without_selected_parent, ordering::SortableBlock},
        reachability::inquirer as reachability,
        relations::RelationsStoreExtensions,
    },
};

use super::PruningProofManager;

impl PruningProofManager {
    pub fn apply_proof(&self, mut proof: PruningPointProof, trusted_set: &[TrustedBlock]) -> PruningImportResult<()> {
        let pruning_point_header = proof[0].last().unwrap().clone();
        let pruning_point = pruning_point_header.hash;

        // Create a copy of the proof, since we're going to be mutating the proof passed to us
        let proof_sets = (0..=self.max_block_level)
            .map(|level| BlockHashSet::from_iter(proof[level as usize].iter().map(|header| header.hash)))
            .collect_vec();

        let mut trusted_gd_map: BlockHashMap<GhostdagData> = BlockHashMap::new();
        for tb in trusted_set.iter() {
            trusted_gd_map.insert(tb.block.hash(), tb.ghostdag.clone().into());
            let tb_block_level = calc_block_level(&tb.block.header, self.max_block_level);

            (0..=tb_block_level).for_each(|current_proof_level| {
                // If this block was in the original proof, ignore it
                if proof_sets[current_proof_level as usize].contains(&tb.block.hash()) {
                    return;
                }

                proof[current_proof_level as usize].push(tb.block.header.clone());
            });
        }

        proof.iter_mut().for_each(|level_proof| {
            level_proof.sort_by(|a, b| a.blue_work.cmp(&b.blue_work));
        });

        self.populate_reachability_and_headers(&proof);

        {
            let reachability_read = self.reachability_store.read();
            for tb in trusted_set.iter() {
                // Header-only trusted blocks are expected to be in pruning point past
                if tb.block.is_header_only() && !reachability_read.is_dag_ancestor_of(tb.block.hash(), pruning_point) {
                    return Err(PruningImportError::PruningPointPastMissingReachability(tb.block.hash()));
                }
            }
        }

        for (level, headers) in proof.iter().enumerate() {
            trace!("Applying level {} from the pruning point proof", level);
            let mut level_ancestors: HashSet<Hash> = HashSet::new();
            level_ancestors.insert(ORIGIN);

            for header in headers.iter() {
                let parents = Arc::new(
                    self.parents_manager
                        .parents_at_level(header, level as BlockLevel)
                        .iter()
                        .copied()
                        .filter(|parent| level_ancestors.contains(parent))
                        .collect_vec()
                        .push_if_empty(ORIGIN),
                );

                self.relations_stores.write()[level].insert(header.hash, parents.clone()).unwrap();

                if level == 0 {
                    let gd = if let Some(gd) = trusted_gd_map.get(&header.hash) {
                        gd.clone()
                    } else {
                        let calculated_gd = self.ghostdag_manager.ghostdag(&parents);
                        // Override the ghostdag data with the real blue score and blue work
                        GhostdagData {
                            blue_score: header.blue_score,
                            blue_work: header.blue_work,
                            selected_parent: calculated_gd.selected_parent,
                            mergeset_blues: calculated_gd.mergeset_blues,
                            mergeset_reds: calculated_gd.mergeset_reds,
                            blues_anticone_sizes: calculated_gd.blues_anticone_sizes,
                        }
                    };
                    self.ghostdag_store.insert(header.hash, Arc::new(gd)).unwrap();
                }

                level_ancestors.insert(header.hash);
            }
        }

        let virtual_parents = vec![pruning_point];
        let virtual_state = Arc::new(VirtualState {
            parents: virtual_parents.clone(),
            ghostdag_data: self.ghostdag_manager.ghostdag(&virtual_parents),
            ..VirtualState::default()
        });
        self.virtual_stores.write().state.set(virtual_state).unwrap();

        let mut batch = WriteBatch::default();
        self.body_tips_store.write().init_batch(&mut batch, &virtual_parents).unwrap();
        self.headers_selected_tip_store
            .write()
            .set_batch(&mut batch, SortableBlock { hash: pruning_point, blue_work: pruning_point_header.blue_work })
            .unwrap();
        self.selected_chain_store.write().init_with_pruning_point(&mut batch, pruning_point).unwrap();
        self.depth_store.insert_batch(&mut batch, pruning_point, ORIGIN, ORIGIN).unwrap();
        self.db.write(batch).unwrap();

        Ok(())
    }

    pub fn populate_reachability_and_headers(&self, proof: &PruningPointProof) {
        let capacity_estimate = self.estimate_proof_unique_size(proof);
        let mut dag = BlockHashMap::with_capacity(capacity_estimate);
        let mut up_heap = BinaryHeap::with_capacity(capacity_estimate);
        for header in proof.iter().flatten().cloned() {
            if let Vacant(e) = dag.entry(header.hash) {
                // pow passing has already been checked during validation
                let block_level = calc_block_level(&header, self.max_block_level);
                self.headers_store.insert(header.hash, header.clone(), block_level).unwrap();

                let mut parents = BlockHashSet::with_capacity(header.direct_parents().len() * 2);
                // We collect all available parent relations in order to maximize reachability information.
                // By taking into account parents from all levels we ensure that the induced DAG has valid
                // reachability information for each level-specific sub-DAG -- hence a single reachability
                // oracle can serve them all
                for level in 0..=self.max_block_level {
                    for parent in self.parents_manager.parents_at_level(&header, level) {
                        parents.insert(*parent);
                    }
                }

                struct DagEntry {
                    header: Arc<Header>,
                    parents: Arc<BlockHashSet>,
                }

                up_heap.push(Reverse(SortableBlock { hash: header.hash, blue_work: header.blue_work }));
                e.insert(DagEntry { header, parents: Arc::new(parents) });
            }
        }

        debug!("Estimated proof size: {}, actual size: {}", capacity_estimate, dag.len());

        for reverse_sortable_block in up_heap.into_sorted_iter() {
            // TODO: Convert to into_iter_sorted once it gets stable
            let hash = reverse_sortable_block.0.hash;
            let dag_entry = dag.get(&hash).unwrap();

            // Filter only existing parents
            let parents_in_dag = BinaryHeap::from_iter(
                dag_entry
                    .parents
                    .iter()
                    .cloned()
                    .filter(|parent| dag.contains_key(parent))
                    .map(|parent| SortableBlock { hash: parent, blue_work: dag.get(&parent).unwrap().header.blue_work }),
            );

            let reachability_read = self.reachability_store.upgradable_read();

            // Find the maximal parent antichain from the possibly redundant set of existing parents
            let mut reachability_parents: Vec<SortableBlock> = Vec::new();
            for parent in parents_in_dag.into_sorted_iter() {
                if reachability_read.is_dag_ancestor_of_any(parent.hash, &mut reachability_parents.iter().map(|parent| parent.hash)) {
                    continue;
                }

                reachability_parents.push(parent);
            }
            let reachability_parents_hashes =
                BlockHashes::new(reachability_parents.iter().map(|parent| parent.hash).collect_vec().push_if_empty(ORIGIN));
            let selected_parent = reachability_parents.iter().max().map(|parent| parent.hash).unwrap_or(ORIGIN);

            // Prepare batch
            let mut batch = WriteBatch::default();
            let mut reachability_relations_write = self.reachability_relations_store.write();
            let mut staging_reachability = StagingReachabilityStore::new(reachability_read);
            let mut staging_reachability_relations = StagingRelationsStore::new(&mut reachability_relations_write);

            // Stage
            staging_reachability_relations.insert(hash, reachability_parents_hashes.clone()).unwrap();
            let mergeset = unordered_mergeset_without_selected_parent(
                &staging_reachability_relations,
                &staging_reachability,
                selected_parent,
                &reachability_parents_hashes,
            );
            reachability::add_block(&mut staging_reachability, hash, selected_parent, &mut mergeset.iter().copied()).unwrap();

            // Commit
            let reachability_write = staging_reachability.commit(&mut batch).unwrap();
            staging_reachability_relations.commit(&mut batch).unwrap();

            // Write
            self.db.write(batch).unwrap();

            // Drop
            drop(reachability_write);
            drop(reachability_relations_write);
        }
    }
}
