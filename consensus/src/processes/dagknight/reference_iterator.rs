use std::collections::BinaryHeap;

use kaspa_hashes::Hash;

use crate::{
    model::stores::{conflict_zone_coloring::ColoringData, headers::HeaderStoreReader},
    processes::{dagknight::umc_voting::ColoringReader, ghostdag::ordering::SortableBlock},
};

/// Lazy future-to-past iterator over the blue set of a colouring: the mergeset blues
/// of `coloring` itself and of every chain block along its selected-parent chain down
/// to `root`, the chain blocks, and `root` (yielded last).
///
/// Blocks are yielded by descending `SortableBlock` key (blue work, then hash), the
/// same order as sorting the collected set: blue work strictly increases along parent
/// edges, so every record sorts above its own mergeset blues. A chain record is read
/// only once its chain block is pulled, never ahead. The heap holds at most 2k+1
/// blocks at width k: expanding one record pushes at most k+1 mergeset blues, and at
/// most k blues of later chain records are still buffered, being concurrent with the
/// popped chain block.
pub struct ReferenceIterator<'a, R: ColoringReader, H: HeaderStoreReader> {
    root: Hash,
    chain: Hash,
    heap: BinaryHeap<SortableBlock>,
    coloring_reader: &'a R,
    headers_store: &'a H,
}

impl<'a, R: ColoringReader, H: HeaderStoreReader> ReferenceIterator<'a, R, H> {
    /// Iterates the blue set of `coloring` future-to-past. `coloring.selected_parent`
    /// must start a selected-parent chain that reaches `root`.
    pub fn new(coloring: &ColoringData, root: Hash, coloring_reader: &'a R, headers_store: &'a H) -> Self {
        let mut it = Self { root, chain: coloring.selected_parent, heap: BinaryHeap::new(), coloring_reader, headers_store };
        for &blue in coloring.mergeset_blues.iter() {
            it.heap.push(it.sortable_block(blue));
        }
        it
    }

    fn sortable_block(&self, block: Hash) -> SortableBlock {
        SortableBlock::new(block, self.headers_store.get_header(block).unwrap().blue_work)
    }
}

impl<'a, R: ColoringReader, H: HeaderStoreReader> Iterator for ReferenceIterator<'a, R, H> {
    type Item = Hash;

    fn next(&mut self) -> Option<Hash> {
        let block = self.heap.pop()?.hash;
        if block == self.chain && block != self.root {
            let data = self.coloring_reader.get_coloring_data(block);
            self.chain = data.selected_parent;
            let record = self.sortable_block(block);
            for &blue in data.mergeset_blues.iter() {
                let blue = self.sortable_block(blue);
                debug_assert!(blue < record, "mergeset blues sort below their record");
                self.heap.push(blue);
            }
        }
        Some(block)
    }
}

#[cfg(test)]
#[allow(clippy::arc_with_non_send_sync)]
mod tests {
    use std::{cell::RefCell, cmp::Reverse, collections::HashMap, sync::Arc};

    use kaspa_consensus_core::{BlockHashSet, HashMapCustomHasher, blockhash::ORIGIN, header::Header};
    use kaspa_hashes::Hash;
    use parking_lot::RwLock;

    use super::ReferenceIterator;
    use crate::{
        model::{
            services::reachability::{MTReachabilityService, ReachabilityService},
            stores::{
                conflict_zone_coloring::{ColoringData, MemoryConflictZoneColoringStore},
                ghostdag::{GhostdagStore, GhostdagStoreReader, MemoryGhostdagStore},
                headers::{HeaderStoreReader, MemoryHeaderStore},
                reachability::MemoryReachabilityStore,
                relations::MemoryRelationsStore,
            },
        },
        processes::{
            dagknight::{
                manager::ConflictZoneManager,
                umc_voting::{
                    ColoringReader,
                    test_fixtures::{MemoryColoringReader, make_cd},
                },
            },
            ghostdag::{ordering::SortableBlock, protocol::GhostdagManager},
            reachability::{
                relations::FutureIntersectRelations,
                tests::{DagBlock, DagBuilder, r#gen::generate_complex_dag},
            },
        },
    };

    fn header_with_work(headers: &MemoryHeaderStore, hash: Hash, blue_work: u64) {
        let mut header = Header::from_precomputed_hash(hash, vec![]);
        header.bits = 0x207fffff;
        header.blue_work = blue_work.into();
        headers.insert(Arc::new(header));
    }

    /// Blues of equal blue work belonging to consecutive chain records come out by
    /// descending hash, matching the sort order over the collected set.
    #[test]
    fn test_yields_equal_work_across_mergesets_by_hash() {
        let (r, p1, p2, c1, c0): (Hash, Hash, Hash, Hash, Hash) =
            (1_u64.into(), 2_u64.into(), 3_u64.into(), 4_u64.into(), 5_u64.into());

        let headers = MemoryHeaderStore::new();
        header_with_work(&headers, r, 0);
        header_with_work(&headers, p1, 4);
        header_with_work(&headers, p2, 4);
        header_with_work(&headers, c1, 5);
        header_with_work(&headers, c0, 9);

        let mut reader = MemoryColoringReader::default();
        reader.add(c1, make_cd(r, vec![r, p1], vec![], 2));
        reader.add(c0, make_cd(c1, vec![c1, p2], vec![], 4));
        let deciding = make_cd(c0, vec![c0], vec![], 5);

        let streamed: Vec<Hash> = ReferenceIterator::new(&deciding, r, &reader, &headers).collect();
        assert_eq!(streamed, vec![c0, c1, p2, p1, r], "equal work across two mergesets is ordered by hash, as the sort does");
    }

    /// A chain record is read only once its chain block is pulled: pulling two chain
    /// blocks reads exactly their two records, and none further.
    #[test]
    fn test_reads_only_pulled_chain_records() {
        let (r, c2, c3, c4): (Hash, Hash, Hash, Hash) = (1_u64.into(), 2_u64.into(), 3_u64.into(), 4_u64.into());

        let headers = MemoryHeaderStore::new();
        header_with_work(&headers, c2, 1);
        header_with_work(&headers, c3, 2);
        header_with_work(&headers, c4, 3);

        let mut reader = MemoryColoringReader::default();
        reader.add(c4, make_cd(c3, vec![c3], vec![], 3));
        reader.add(c3, make_cd(c2, vec![c2], vec![], 2));
        let deciding = make_cd(c4, vec![c4], vec![], 4);

        let mut it = ReferenceIterator::new(&deciding, r, &reader, &headers);
        assert_eq!(it.next(), Some(c4));
        assert_eq!(it.next(), Some(c3));
    }

    /// The blue set of a free-search colouring over a random DAG: the stream equals the
    /// collected-and-sorted set, its length equals the deciding record's blue score, the
    /// root is yielded last, and the out-of-order buffer stays within 2g+1.
    #[test]
    fn test_matches_sorted_blue_set_on_random_dags() {
        let (genesis_id, blocks) = generate_complex_dag(0.1, 10.0, 50);
        let genesis: Hash = genesis_id.into();

        let mut reachability = MemoryReachabilityStore::new();
        let mut relations = MemoryRelationsStore::new();
        let topology_ghostdag_store = Arc::new(MemoryGhostdagStore::new());
        let headers_store = Arc::new(MemoryHeaderStore::new());
        let topology_gd_manager = GhostdagManager::new(
            genesis,
            8,
            topology_ghostdag_store.clone(),
            relations.clone(),
            headers_store.clone(),
            reachability.clone(),
        );
        topology_ghostdag_store.insert(genesis, Arc::new(topology_gd_manager.genesis_ghostdag_data())).unwrap();

        let mut tips = BlockHashSet::new();
        tips.insert(genesis);
        header_with_work(&headers_store, genesis, 0);

        {
            let mut builder = DagBuilder::new(&mut reachability, &mut relations);
            builder.init();
            builder.add_block(DagBlock::new(genesis, vec![ORIGIN]));
            for (id, parents) in &blocks {
                let hash: Hash = (*id).into();
                let parent_hashes: Vec<Hash> = parents.iter().map(|&p| p.into()).collect();
                parent_hashes.iter().for_each(|ph| {
                    tips.remove(ph);
                });

                let gd = topology_gd_manager.ghostdag(&parent_hashes);
                builder.add_block_with_selected_parent(DagBlock::new(hash, parent_hashes.clone()), gd.selected_parent);
                tips.insert(hash);

                let mut header = Header::from_precomputed_hash(hash, parent_hashes);
                header.bits = 0x207fffff;
                header.blue_work = gd.blue_work;
                headers_store.insert(Arc::new(header));
                topology_ghostdag_store.insert(hash, Arc::new(gd)).unwrap();
            }
        }

        let reachability_service = MTReachabilityService::new(Arc::new(RwLock::new(reachability)));

        // Root: three selected-parent links below the largest-work tip
        let max_tip =
            tips.iter().copied().max_by_key(|&h| SortableBlock::new(h, headers_store.get_header(h).unwrap().blue_work)).unwrap();
        let mut root = max_tip;
        for _ in 0..3 {
            if root == genesis {
                break;
            }
            root = topology_ghostdag_store.get_selected_parent(root).unwrap();
        }

        // Zone tips: DAG tips in the root's future
        let zone_tips: Vec<Hash> = tips.iter().copied().filter(|&h| reachability_service.is_dag_ancestor_of(root, h)).collect();

        let sorted_blue_set = |reader: &ConflictZoneManager<
            MemoryConflictZoneColoringStore,
            MemoryHeaderStore,
            MemoryRelationsStore,
            MemoryReachabilityStore,
        >,
                               virtual_cd: &ColoringData|
         -> Vec<Hash> {
            let mut blue_set = BlockHashSet::new();
            blue_set.extend(virtual_cd.mergeset_blues.iter().copied());
            let mut curr = virtual_cd.selected_parent;
            while curr != root {
                blue_set.insert(curr);
                let cd = reader.get_coloring_data(curr);
                blue_set.extend(cd.mergeset_blues.iter().copied());
                curr = cd.selected_parent;
            }
            blue_set.insert(root);
            let mut blues: Vec<Hash> = blue_set.iter().copied().collect();
            blues.sort_by_cached_key(|block| Reverse(SortableBlock::new(*block, headers_store.get_header(*block).unwrap().blue_work)));
            blues
        };

        for g in 0..=3u16 {
            let dk_map = RefCell::new(HashMap::new());
            let coloring_store = Arc::new(MemoryConflictZoneColoringStore::new(dk_map));
            let manager = ConflictZoneManager::with_free_search(
                g,
                root,
                coloring_store,
                headers_store.clone(),
                FutureIntersectRelations::new(relations.clone(), reachability_service.clone(), root),
                reachability_service.clone(),
                true,
            );
            manager.fill_zone_data(&zone_tips);
            let virtual_cd = manager.k_colouring(&zone_tips, g, None);

            let mut it = ReferenceIterator::new(&virtual_cd, root, &manager, &*headers_store);
            let mut streamed = Vec::new();
            while let Some(block) = it.next() {
                assert!(it.heap.len() <= 2 * g as usize + 1, "width {g}: buffer bound");
                streamed.push(block);
            }
            assert_eq!(streamed, sorted_blue_set(&manager, &virtual_cd), "width {g}");
            assert_eq!(streamed.len(), virtual_cd.blue_score as usize, "width {g}");
            assert_eq!(streamed.last(), Some(&root), "width {g}");
        }
    }
}
