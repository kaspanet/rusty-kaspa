use super::interval::Interval;
use super::{tree::*, *};
use crate::model::stores::reachability::{ReachabilityStore, ReachabilityStoreReader};
use kaspa_consensus_core::blockhash;
use kaspa_hashes::Hash;

/// Init the reachability store to match the state required by the algorithmic layer.
/// The function first checks the store for possibly being initialized already.
pub fn init(store: &mut (impl ReachabilityStore + ?Sized)) -> Result<()> {
    init_with_params(store, blockhash::ORIGIN, Interval::maximal())
}

pub(super) fn init_with_params(store: &mut (impl ReachabilityStore + ?Sized), origin: Hash, capacity: Interval) -> Result<()> {
    if store.has(origin)? {
        return Ok(());
    }
    store.init(origin, capacity)?;
    Ok(())
}

type HashIterator<'a> = &'a mut dyn Iterator<Item = Hash>;

/// Add a block to the DAG reachability data structures and persist using the provided `store`.
pub fn add_block(
    store: &mut (impl ReachabilityStore + ?Sized),
    new_block: Hash,
    selected_parent: Hash,
    mergeset_iterator: HashIterator,
) -> Result<()> {
    add_block_with_params(store, new_block, selected_parent, mergeset_iterator, None, None)
}

fn add_block_with_params(
    store: &mut (impl ReachabilityStore + ?Sized),
    new_block: Hash,
    selected_parent: Hash,
    mergeset_iterator: HashIterator,
    reindex_depth: Option<u64>,
    reindex_slack: Option<u64>,
) -> Result<()> {
    add_tree_block(
        store,
        new_block,
        selected_parent,
        reindex_depth.unwrap_or(crate::constants::perf::DEFAULT_REINDEX_DEPTH),
        reindex_slack.unwrap_or(crate::constants::perf::DEFAULT_REINDEX_SLACK),
    )?;
    add_dag_block(store, new_block, mergeset_iterator)?;
    Ok(())
}

fn add_dag_block(store: &mut (impl ReachabilityStore + ?Sized), new_block: Hash, mergeset_iterator: HashIterator) -> Result<()> {
    // Update the future covering set for blocks in the mergeset
    for merged_block in mergeset_iterator {
        insert_to_future_covering_set(store, merged_block, new_block)?;
    }
    Ok(())
}

/// Deletes a block permanently from the DAG reachability structures while
/// keeping full reachability info for all other blocks. That is, for any other
/// B, C ∈ G, DAG/chain queries are guaranteed to return the same results as
/// before the deletion.
pub fn delete_block(store: &mut (impl ReachabilityStore + ?Sized), block: Hash, mergeset_iterator: HashIterator) -> Result<()> {
    let interval = store.get_interval(block)?;
    let parent = store.get_parent(block)?;
    let children = store.get_children(block)?;

    /* Algo:
        1. Find child index of block at parent
        2. Replace child with its children
        3. Update parent as new parent of children
        4. For each block in the mergeset, find index of `block` in the future-covering-set and replace it with its children
        5. Extend interval of first and last children as much as possible
        6. Delete block
    */

    let block_index = match binary_search_descendant(store, store.get_children(parent)?.as_slice(), block)? {
        SearchOutput::NotFound(_) => return Err(ReachabilityError::DataInconsistency),
        SearchOutput::Found(hash, i) => {
            debug_assert_eq!(hash, block);
            i
        }
    };

    store.replace_child(parent, block, block_index, &children)?;

    for child in children.iter().copied() {
        store.set_parent(child, parent)?;
    }

    for merged_block in mergeset_iterator {
        match binary_search_descendant(store, store.get_future_covering_set(merged_block)?.as_slice(), block)? {
            SearchOutput::NotFound(_) => return Err(ReachabilityError::DataInconsistency),
            SearchOutput::Found(hash, i) => {
                debug_assert_eq!(hash, block);
                store.replace_future_covering_item(merged_block, block, i, &children)?;
            }
        }
    }

    match children.len() {
        0 => {
            // No children, give the capacity to the sibling on the left
            if block_index > 0 {
                let sibling = store.get_children(parent)?[block_index - 1];
                let sibling_interval = store.get_interval(sibling)?;
                store.set_interval(sibling, Interval::new(sibling_interval.start, interval.end))?;
            }
        }
        1 => {
            // Give full interval capacity to the only child
            store.set_interval(children[0], interval)?;
        }
        _ => {
            // Split the extra capacity between the first and last children
            let first_child = children[0];
            let first_interval = store.get_interval(first_child)?;
            store.set_interval(first_child, Interval::new(interval.start, first_interval.end))?;

            let last_child = children.last().copied().expect("len > 1");
            let last_interval = store.get_interval(last_child)?;
            store.set_interval(last_child, Interval::new(last_interval.start, interval.end))?;
        }
    }

    store.delete(block)?;

    Ok(())
}

fn insert_to_future_covering_set(store: &mut (impl ReachabilityStore + ?Sized), merged_block: Hash, new_block: Hash) -> Result<()> {
    match binary_search_descendant(store, store.get_future_covering_set(merged_block)?.as_slice(), new_block)? {
        // We expect the query to not succeed, and to only return the correct insertion index.
        // The existences of a `future covering item` (`FCI`) which is a chain ancestor of `new_block`
        // contradicts `merged_block ∈ mergeset(new_block)`. Similarly, the existence of an FCI
        // which `new_block` is a chain ancestor of, contradicts processing order.
        SearchOutput::Found(_, _) => Err(ReachabilityError::DataInconsistency),
        SearchOutput::NotFound(i) => {
            store.insert_future_covering_item(merged_block, new_block, i)?;
            Ok(())
        }
    }
}

/// Hint to the reachability algorithm that `hint` is a candidate to become
/// the `virtual selected parent` (`sink`). This might affect internal reachability heuristics such
/// as moving the reindex point. The consensus runtime is expected to call this function
/// for a new header selected tip which is `header only` / `pending UTXO verification`, or for a completely resolved `sink`.
pub fn hint_virtual_selected_parent(store: &mut (impl ReachabilityStore + ?Sized), hint: Hash) -> Result<()> {
    try_advancing_reindex_root(
        store,
        hint,
        crate::constants::perf::DEFAULT_REINDEX_DEPTH,
        crate::constants::perf::DEFAULT_REINDEX_SLACK,
    )
}

/// Checks if the `this` block is a strict chain ancestor of the `queried` block (i.e., `this ∈ chain(queried)`).
/// Note that this results in `false` if `this == queried`
pub fn is_strict_chain_ancestor_of(store: &(impl ReachabilityStoreReader + ?Sized), this: Hash, queried: Hash) -> Result<bool> {
    Ok(store.get_interval(this)?.strictly_contains(store.get_interval(queried)?))
}

/// Checks if `this` block is a chain ancestor of `queried` block (i.e., `this ∈ chain(queried) ∪ {queried}`).
/// Note that we use the graph theory convention here which defines that a block is also an ancestor of itself.
pub fn is_chain_ancestor_of(store: &(impl ReachabilityStoreReader + ?Sized), this: Hash, queried: Hash) -> Result<bool> {
    Ok(store.get_interval(this)?.contains(store.get_interval(queried)?))
}

/// Returns true if `this` is a DAG ancestor of `queried` (i.e., `queried ∈ future(this) ∪ {this}`).
/// Note: this method will return true if `this == queried`.
/// The complexity of this method is `O(log(|future_covering_set(this)|))`
pub fn is_dag_ancestor_of(store: &(impl ReachabilityStoreReader + ?Sized), this: Hash, queried: Hash) -> Result<bool> {
    // First, check if `this` is a chain ancestor of queried
    if is_chain_ancestor_of(store, this, queried)? {
        return Ok(true);
    }
    // Otherwise, use previously registered future blocks to complete the
    // DAG reachability test
    match binary_search_descendant(store, store.get_future_covering_set(this)?.as_slice(), queried)? {
        SearchOutput::Found(_, _) => Ok(true),
        SearchOutput::NotFound(_) => Ok(false),
    }
}

/// Finds the tree child of `ancestor` which is also a chain ancestor of `descendant`.
pub fn get_next_chain_ancestor(store: &(impl ReachabilityStoreReader + ?Sized), descendant: Hash, ancestor: Hash) -> Result<Hash> {
    if descendant == ancestor {
        // The next ancestor does not exist
        return Err(ReachabilityError::BadQuery);
    }
    if !is_strict_chain_ancestor_of(store, ancestor, descendant)? {
        // `ancestor` isn't actually a chain ancestor of `descendant`, so by def
        // we cannot find the next ancestor as well
        return Err(ReachabilityError::BadQuery);
    }

    get_next_chain_ancestor_unchecked(store, descendant, ancestor)
}

/// Note: it is important to keep the unchecked version for internal module use,
/// since in some scenarios during reindexing `ancestor` might have a modified
/// interval which was not propagated yet.
pub(super) fn get_next_chain_ancestor_unchecked(
    store: &(impl ReachabilityStoreReader + ?Sized),
    descendant: Hash,
    ancestor: Hash,
) -> Result<Hash> {
    match binary_search_descendant(store, store.get_children(ancestor)?.as_slice(), descendant)? {
        SearchOutput::Found(hash, _) => Ok(hash),
        SearchOutput::NotFound(_) => Err(ReachabilityError::BadQuery),
    }
}

enum SearchOutput {
    NotFound(usize), // `usize` is the position to insert at
    Found(Hash, usize),
}

fn binary_search_descendant(
    store: &(impl ReachabilityStoreReader + ?Sized),
    ordered_hashes: &[Hash],
    descendant: Hash,
) -> Result<SearchOutput> {
    if cfg!(debug_assertions) {
        // This is a linearly expensive assertion, keep it debug only
        assert_hashes_ordered(store, ordered_hashes);
    }

    // `Interval::end` represents the unique number allocated to this block
    let point = store.get_interval(descendant)?.end;

    // We use an `unwrap` here since otherwise we need to implement `binary_search`
    // ourselves, which is not worth the effort given that this would be an unrecoverable
    // error anyhow
    match ordered_hashes.binary_search_by_key(&point, |c| store.get_interval(*c).unwrap().start) {
        Ok(i) => Ok(SearchOutput::Found(ordered_hashes[i], i)),
        Err(i) => {
            // `i` is where `point` was expected (i.e., point < ordered_hashes[i].interval.start),
            // so we expect `ordered_hashes[i - 1].interval` to be the only candidate to contain `point`
            if i > 0 && is_chain_ancestor_of(store, ordered_hashes[i - 1], descendant)? {
                Ok(SearchOutput::Found(ordered_hashes[i - 1], i - 1))
            } else {
                Ok(SearchOutput::NotFound(i))
            }
        }
    }
}

fn assert_hashes_ordered(store: &(impl ReachabilityStoreReader + ?Sized), ordered_hashes: &[Hash]) {
    let intervals: Vec<Interval> = ordered_hashes.iter().cloned().map(|c| store.get_interval(c).unwrap()).collect();
    debug_assert!(intervals.as_slice().windows(2).all(|w| w[0].end < w[1].start))
}

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;
    use crate::{
        model::stores::{
            children::ChildrenStore,
            reachability::{DbReachabilityStore, MemoryReachabilityStore, StagingReachabilityStore},
            relations::{DbRelationsStore, MemoryRelationsStore, RelationsStore, StagingRelationsStore},
        },
        processes::reachability::{interval::Interval, tests::gen::generate_complex_dag},
    };
    use itertools::Itertools;
    use kaspa_consensus_core::blockhash::ORIGIN;
    use kaspa_database::prelude::ConnBuilder;
    use kaspa_database::{create_temp_db, prelude::CachePolicy};
    use parking_lot::RwLock;
    use rand::seq::IteratorRandom;
    use rocksdb::WriteBatch;
    use std::{iter::once, ops::Deref};

    #[test]
    fn test_add_tree_blocks() {
        // Arrange
        let mut store = MemoryReachabilityStore::new();

        // Act
        let root: Hash = 1.into();
        TreeBuilder::new(&mut store)
            .init_with_params(root, Interval::new(1, 15))
            .add_block(2.into(), root)
            .add_block(3.into(), 2.into())
            .add_block(4.into(), 2.into())
            .add_block(5.into(), 3.into())
            .add_block(6.into(), 5.into())
            .add_block(7.into(), 1.into())
            .add_block(8.into(), 6.into())
            .add_block(9.into(), 6.into())
            .add_block(10.into(), 6.into())
            .add_block(11.into(), 6.into());

        // Assert
        store.validate_intervals(root).unwrap();
    }

    #[test]
    fn test_add_early_blocks() {
        // Arrange
        let mut store = MemoryReachabilityStore::new();

        // Act
        let root: Hash = 1.into();
        let mut builder = TreeBuilder::new_with_params(&mut store, 2, 5);
        builder.init_with_params(root, Interval::maximal());
        for i in 2u64..100 {
            builder.add_block(i.into(), (i / 2).into());
        }

        // Should trigger an earlier than reindex root allocation
        builder.add_block(100.into(), 2.into());
        store.validate_intervals(root).unwrap();
    }

    #[derive(Clone)]
    pub struct DagTestCase {
        genesis: u64,
        blocks: Vec<(u64, Vec<u64>)>, // All blocks other than genesis
        expected_past_relations: Vec<(u64, u64)>,
        expected_anticone_relations: Vec<(u64, u64)>,
    }

    impl DagTestCase {
        /// Returns all block ids other than genesis
        pub fn ids(&self) -> impl Iterator<Item = u64> + '_ {
            self.blocks.iter().map(|(i, _)| *i)
        }
    }

    /// Runs a DAG test-case by adding all blocks and then removing them while verifying full
    /// reachability and relations state frequently between operations.
    /// Note: runtime is quadratic in the number of blocks so should be used with mildly small DAGs (~50)
    fn run_dag_test_case<S: RelationsStore + ChildrenStore + ?Sized, V: ReachabilityStore + ?Sized>(
        relations: &mut S,
        reachability: &mut V,
        test: &DagTestCase,
    ) {
        // Add blocks
        {
            let mut builder = DagBuilder::new(reachability, relations);
            builder.init();
            builder.add_block(DagBlock::new(test.genesis.into(), vec![ORIGIN]));
            for (block, parents) in test.blocks.iter() {
                builder.add_block(DagBlock::new((*block).into(), parents.iter().map(|&i| i.into()).collect()));
            }
        }

        // Assert tree intervals and DAG relations
        reachability.validate_intervals(ORIGIN).unwrap();
        validate_relations(relations).unwrap();

        // Assert genesis future
        for block in test.ids() {
            assert!(reachability.in_past_of(test.genesis, block));
        }

        // Assert expected futures
        for (x, y) in test.expected_past_relations.iter().copied() {
            assert!(reachability.in_past_of(x, y));
        }

        // Assert expected anticones
        for (x, y) in test.expected_anticone_relations.iter().copied() {
            assert!(reachability.are_anticone(x, y));
        }

        let mut hashes_ref = subtree(reachability, ORIGIN);
        let hashes = hashes_ref.iter().copied().collect_vec();
        assert_eq!(test.blocks.len() + 1, hashes.len());
        let chain_closure_ref = build_chain_closure(reachability, &hashes);
        let dag_closure_ref = build_transitive_closure(relations, reachability, &hashes);

        for block in test.ids().choose_multiple(&mut rand::thread_rng(), test.blocks.len()).into_iter().chain(once(test.genesis)) {
            DagBuilder::new(reachability, relations).delete_block(block.into());
            hashes_ref.remove(&block.into());
            reachability.validate_intervals(ORIGIN).unwrap();
            validate_relations(relations).unwrap();
            validate_closures(relations, reachability, &chain_closure_ref, &dag_closure_ref, &hashes_ref);
        }
    }

    /// Runs a DAG test-case with full verification using the staging store mechanism.
    /// Note: runtime is quadratic in the number of blocks so should be used with mildly small DAGs (~50)
    fn run_dag_test_case_with_staging(test: &DagTestCase) {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10)).unwrap();
        let cache_policy = CachePolicy::Count(test.blocks.len() / 3);
        let reachability = RwLock::new(DbReachabilityStore::new(db.clone(), cache_policy, cache_policy));
        let mut relations = DbRelationsStore::with_prefix(db.clone(), &[], CachePolicy::Empty, CachePolicy::Empty);

        // Add blocks via a staging store
        {
            let mut staging_reachability = StagingReachabilityStore::new(reachability.upgradable_read());
            let mut staging_relations = StagingRelationsStore::new(&mut relations);
            let mut builder = DagBuilder::new(&mut staging_reachability, &mut staging_relations);
            builder.init();
            builder.add_block(DagBlock::new(test.genesis.into(), vec![ORIGIN]));
            for (block, parents) in test.blocks.iter() {
                builder.add_block(DagBlock::new((*block).into(), parents.iter().map(|&i| i.into()).collect()));
            }

            // Commit the staging changes
            {
                let mut batch = WriteBatch::default();
                let reachability_write = staging_reachability.commit(&mut batch).unwrap();
                staging_relations.commit(&mut batch).unwrap();
                db.write(batch).unwrap();
                drop(reachability_write);
            }
        }

        let reachability_read = reachability.read();

        // Assert tree intervals and DAG relations
        reachability_read.validate_intervals(ORIGIN).unwrap();
        validate_relations(&relations).unwrap();

        // Assert genesis future
        for block in test.ids() {
            assert!(reachability_read.in_past_of(test.genesis, block));
        }

        // Assert expected futures
        for (x, y) in test.expected_past_relations.iter().copied() {
            assert!(reachability_read.in_past_of(x, y));
        }

        // Assert expected anticones
        for (x, y) in test.expected_anticone_relations.iter().copied() {
            assert!(reachability_read.are_anticone(x, y));
        }

        let mut hashes_ref = subtree(reachability_read.deref(), ORIGIN);
        let hashes = hashes_ref.iter().copied().collect_vec();
        assert_eq!(test.blocks.len() + 1, hashes.len());
        let chain_closure_ref = build_chain_closure(reachability_read.deref(), &hashes);
        let dag_closure_ref = build_transitive_closure(&relations, reachability_read.deref(), &hashes);

        drop(reachability_read);

        let mut batch = WriteBatch::default();
        let mut staging_reachability = StagingReachabilityStore::new(reachability.upgradable_read());
        let mut staging_relations = StagingRelationsStore::new(&mut relations);

        for (i, block) in
            test.ids().choose_multiple(&mut rand::thread_rng(), test.blocks.len()).into_iter().chain(once(test.genesis)).enumerate()
        {
            DagBuilder::new(&mut staging_reachability, &mut staging_relations).delete_block(block.into());
            hashes_ref.remove(&block.into());
            staging_reachability.validate_intervals(ORIGIN).unwrap();
            validate_relations(&staging_relations).unwrap();
            validate_closures(&staging_relations, &staging_reachability, &chain_closure_ref, &dag_closure_ref, &hashes_ref);

            // Once in a while verify the underlying store
            if i % (test.blocks.len() / 3) == 0 || i == test.blocks.len() - 1 {
                // Commit the staging changes
                {
                    let reachability_write = staging_reachability.commit(&mut batch).unwrap();
                    staging_relations.commit(&mut batch).unwrap();
                    db.write(batch).unwrap();
                    drop(reachability_write);
                }

                // Verify the underlying store
                {
                    let reachability_read = reachability.read();
                    reachability_read.validate_intervals(ORIGIN).unwrap();
                    validate_relations(&relations).unwrap();
                    validate_closures(&relations, reachability_read.deref(), &chain_closure_ref, &dag_closure_ref, &hashes_ref);
                }

                // Recapture staging stores
                batch = WriteBatch::default();
                staging_reachability = StagingReachabilityStore::new(reachability.upgradable_read());
                staging_relations = StagingRelationsStore::new(&mut relations);
            }
        }
    }

    #[test]
    fn test_dag_building_and_removal() {
        let manual_test = DagTestCase {
            genesis: 1,
            blocks: vec![
                (2, vec![1]),
                (3, vec![1]),
                (4, vec![2, 3]),
                (5, vec![4]),
                (6, vec![1]),
                (7, vec![5, 6]),
                (8, vec![1]),
                (9, vec![1]),
                (10, vec![7, 8, 9]),
                (11, vec![1]),
                (12, vec![11, 10]),
            ],
            expected_past_relations: vec![(2, 4), (2, 5), (2, 7), (5, 10), (6, 10), (10, 12), (11, 12)],
            expected_anticone_relations: vec![(2, 3), (2, 6), (3, 6), (5, 6), (3, 8), (11, 2), (11, 4), (11, 6), (11, 9)],
        };

        let generate_complex = |bps| {
            let target_blocks = 50; // verification is quadratic so a larger target takes relatively long
            let (genesis, blocks) = generate_complex_dag(2.0, bps, target_blocks);
            assert_eq!(target_blocks as usize, blocks.len());
            DagTestCase {
                genesis,
                blocks,
                expected_past_relations: Default::default(),
                expected_anticone_relations: Default::default(),
            }
        };

        for test in once(manual_test).chain([2.0, 3.0, 4.0].map(generate_complex)) {
            // Run the test case with memory stores
            let mut reachability = MemoryReachabilityStore::new();
            let mut relations = MemoryRelationsStore::new();
            run_dag_test_case(&mut relations, &mut reachability, &test);

            // Run with direct DB stores
            let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10)).unwrap();
            let cache_policy = CachePolicy::Count(test.blocks.len() / 3);
            let mut reachability = DbReachabilityStore::new(db.clone(), cache_policy, cache_policy);
            let mut relations = DbRelationsStore::new(db, 0, cache_policy, cache_policy);
            run_dag_test_case(&mut relations, &mut reachability, &test);

            // Run with a staging process
            run_dag_test_case_with_staging(&test);
        }
    }
}
