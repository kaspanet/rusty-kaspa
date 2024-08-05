use super::feerate_key::FeerateTransactionKey;
use sweep_bptree::tree::visit::{DescendVisit, DescendVisitResult};
use sweep_bptree::tree::{Argument, SearchArgument};
use sweep_bptree::{BPlusTree, NodeStoreVec};

type FeerateKey = FeerateTransactionKey;

#[derive(Clone, Copy, Debug, Default)]
struct FeerateWeight(f64);

impl FeerateWeight {
    /// Returns the weight value
    pub fn weight(&self) -> f64 {
        self.0
    }
}

impl Argument<FeerateKey> for FeerateWeight {
    fn from_leaf(keys: &[FeerateKey]) -> Self {
        Self(keys.iter().map(|k| k.weight()).sum())
    }

    fn from_inner(_keys: &[FeerateKey], arguments: &[Self]) -> Self {
        Self(arguments.iter().map(|a| a.0).sum())
    }
}

impl SearchArgument<FeerateKey> for FeerateWeight {
    type Query = f64;

    fn locate_in_leaf(query: Self::Query, keys: &[FeerateKey]) -> Option<usize> {
        let mut sum = 0.0;
        for (i, k) in keys.iter().enumerate() {
            let w = k.weight();
            sum += w;
            if query < sum {
                return Some(i);
            }
        }
        // In order to avoid sensitivity to floating number arithmetics,
        // we logically "clamp" the search, returning the last leaf if the query
        // value is out of bounds
        match keys.len() {
            0 => None,
            n => Some(n - 1),
        }
    }

    fn locate_in_inner(mut query: Self::Query, _keys: &[FeerateKey], arguments: &[Self]) -> Option<(usize, Self::Query)> {
        for (i, a) in arguments.iter().enumerate() {
            if query >= a.0 {
                query -= a.0;
            } else {
                return Some((i, query));
            }
        }
        // In order to avoid sensitivity to floating number arithmetics,
        // we logically "clamp" the search, returning the last subtree if the query
        // value is out of bounds. Eventually this will lead to the return of the
        // last leaf (see locate_in_leaf as well)
        match arguments.len() {
            0 => None,
            n => Some((n - 1, arguments[n - 1].0)),
        }
    }
}

struct PrefixWeightVisitor<'a> {
    key: &'a FeerateKey,
    accumulated_weight: f64,
}

impl<'a> PrefixWeightVisitor<'a> {
    pub fn new(key: &'a FeerateKey) -> Self {
        Self { key, accumulated_weight: Default::default() }
    }

    fn search_in_keys(&self, keys: &[FeerateKey]) -> usize {
        match keys.binary_search(self.key) {
            Err(idx) => {
                // The idx is the place where a matching element could be inserted while maintaining
                // sorted order, go to left child
                idx
            }
            Ok(idx) => {
                // Exact match, go to right child.
                idx + 1
            }
        }
    }
}

impl<'a> DescendVisit<FeerateKey, (), FeerateWeight> for PrefixWeightVisitor<'a> {
    type Result = f64;

    fn visit_inner(&mut self, keys: &[FeerateKey], arguments: &[FeerateWeight]) -> DescendVisitResult<Self::Result> {
        let idx = self.search_in_keys(keys);
        // trace!("[visit_inner] {}, {}, {}", keys.len(), arguments.len(), idx);
        for argument in arguments.iter().take(idx) {
            self.accumulated_weight += argument.weight();
        }
        DescendVisitResult::GoDown(idx)
    }

    fn visit_leaf(&mut self, keys: &[FeerateKey], _values: &[()]) -> Option<Self::Result> {
        let idx = self.search_in_keys(keys);
        // trace!("[visit_leaf] {}, {}", keys.len(), idx);
        for key in keys.iter().take(idx) {
            self.accumulated_weight += key.weight();
        }
        Some(self.accumulated_weight)
    }
}

type InnerTree = BPlusTree<NodeStoreVec<FeerateKey, (), FeerateWeight>>;

pub struct SearchTree {
    tree: InnerTree,
}

impl Default for SearchTree {
    fn default() -> Self {
        Self { tree: InnerTree::new(Default::default()) }
    }
}

impl SearchTree {
    pub fn new() -> Self {
        Self { tree: InnerTree::new(Default::default()) }
    }

    pub fn len(&self) -> usize {
        self.tree.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn insert(&mut self, key: FeerateKey) -> bool {
        self.tree.insert(key, ()).is_none()
    }

    pub fn remove(&mut self, key: &FeerateKey) -> bool {
        self.tree.remove(key).is_some()
    }

    pub fn search(&self, query: f64) -> &FeerateKey {
        self.tree.get_by_argument(query).expect("clamped").0
    }

    pub fn total_weight(&self) -> f64 {
        self.tree.root_argument().weight()
    }

    pub fn prefix_weight(&self, key: &FeerateKey) -> f64 {
        self.tree.descend_visit(PrefixWeightVisitor::new(key)).unwrap()
    }

    pub fn descending_iter(&self) -> impl DoubleEndedIterator<Item = &FeerateKey> + ExactSizeIterator {
        self.tree.iter().rev().map(|(key, ())| key)
    }

    pub fn ascending_iter(&self) -> impl DoubleEndedIterator<Item = &FeerateKey> + ExactSizeIterator {
        self.tree.iter().map(|(key, ())| key)
    }

    pub fn first(&self) -> Option<&FeerateKey> {
        self.tree.first().map(|(k, ())| k)
    }

    pub fn last(&self) -> Option<&FeerateKey> {
        self.tree.last().map(|(k, ())| k)
    }
}

#[cfg(test)]
mod tests {
    use super::super::feerate_key::tests::build_feerate_key;
    use super::*;
    use itertools::Itertools;
    use std::collections::HashSet;

    #[test]
    fn test_feerate_weight_queries() {
        let mut btree = SearchTree::new();
        let mass = 2000;
        // The btree stores N=64 keys at each node/leaf, so we make sure the tree has more than
        // 64^2 keys in order to trigger at least a few intermediate tree nodes
        let fees = vec![[123, 113, 10_000, 1000, 2050, 2048]; 64 * (64 + 1)].into_iter().flatten().collect_vec();

        #[allow(clippy::mutable_key_type)]
        let mut s = HashSet::with_capacity(fees.len());
        for (i, fee) in fees.iter().copied().enumerate() {
            let key = build_feerate_key(fee, mass, i as u64);
            s.insert(key.clone());
            btree.insert(key);
        }

        // Randomly remove 1/6 of the items
        let remove = s.iter().take(fees.len() / 6).cloned().collect_vec();
        for r in remove {
            s.remove(&r);
            btree.remove(&r);
        }

        // Collect to vec and sort for reference
        let mut v = s.into_iter().collect_vec();
        v.sort();

        // Test reverse iteration
        for (expected, item) in v.iter().rev().zip(btree.descending_iter()) {
            assert_eq!(&expected, &item);
            assert!(expected.cmp(item).is_eq()); // Assert Ord equality as well
        }

        // Sweep through the tree and verify that weight search queries are handled correctly
        let eps: f64 = 0.001;
        let mut sum = 0.0;
        for expected in v {
            let weight = expected.weight();
            let eps = eps.min(weight / 3.0);
            let samples = [sum + eps, sum + weight / 2.0, sum + weight - eps];
            for sample in samples {
                let key = btree.search(sample);
                assert_eq!(&expected, key);
                assert!(expected.cmp(key).is_eq()); // Assert Ord equality as well
            }
            sum += weight;
        }

        println!("{}, {}", sum, btree.total_weight());

        // Test clamped search bounds
        assert_eq!(btree.first(), Some(btree.search(f64::NEG_INFINITY)));
        assert_eq!(btree.first(), Some(btree.search(-1.0)));
        assert_eq!(btree.first(), Some(btree.search(-eps)));
        assert_eq!(btree.first(), Some(btree.search(0.0)));
        assert_eq!(btree.last(), Some(btree.search(sum)));
        assert_eq!(btree.last(), Some(btree.search(sum + eps)));
        assert_eq!(btree.last(), Some(btree.search(sum + 1.0)));
        assert_eq!(btree.last(), Some(btree.search(1.0 / 0.0)));
        assert_eq!(btree.last(), Some(btree.search(f64::INFINITY)));
        let _ = btree.search(f64::NAN);
    }

    #[test]
    fn test_btree_rev_iter() {
        let mut btree = SearchTree::new();
        let mass = 2000;
        let fees = vec![[123, 113, 10_000, 1000, 2050, 2048]; 64 * (64 + 1)].into_iter().flatten().collect_vec();
        let mut v = Vec::with_capacity(fees.len());
        for (i, fee) in fees.iter().copied().enumerate() {
            let key = build_feerate_key(fee, mass, i as u64);
            v.push(key.clone());
            btree.insert(key);
        }
        v.sort();

        for (expected, item) in v.into_iter().rev().zip(btree.descending_iter()) {
            assert_eq!(&expected, item);
            assert!(expected.cmp(item).is_eq()); // Assert Ord equality as well
        }
    }
}
