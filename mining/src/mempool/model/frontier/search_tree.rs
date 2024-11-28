use super::feerate_key::FeerateTransactionKey;
use std::iter::FusedIterator;
use sweep_bptree::tree::visit::{DescendVisit, DescendVisitResult};
use sweep_bptree::tree::{Argument, SearchArgument};
use sweep_bptree::{BPlusTree, NodeStoreVec};

type FeerateKey = FeerateTransactionKey;

/// A struct for implementing "weight space" search using the SearchArgument customization.
/// The weight space is the range `[0, total_weight)` and each key has a "logical" interval allocation
/// within this space according to its tree position and weight.
///
/// We implement the search efficiently by maintaining subtree weights which are updated with each
/// element insertion/removal. Given a search query `p ∈ [0, total_weight)` we then find the corresponding
/// element in log time by walking down from the root and adjusting the query according to subtree weights.
/// For instance if the query point is `123.56` and the top 3 subtrees have weights `120, 10.5 ,100` then we
/// recursively query the middle subtree with the point `123.56 - 120 = 3.56`.
///
/// See SearchArgument implementation below for more details.
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
        // Search algorithm: Locate the next subtree to visit by iterating through `arguments`
        // and subtracting the query until the correct range is found
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

/// Visitor struct which accumulates the prefix weight up to a provided key (inclusive) in log time.
///
/// The basic idea is to use the subtree weights stored in the tree for walking down from the root
/// to the leaf (corresponding to the searched key), and accumulating all weights proceeding the walk-down path
struct PrefixWeightVisitor<'a> {
    /// The key to search up to
    key: &'a FeerateKey,
    /// This field accumulates the prefix weight during the visit process
    accumulated_weight: f64,
}

impl<'a> PrefixWeightVisitor<'a> {
    pub fn new(key: &'a FeerateKey) -> Self {
        Self { key, accumulated_weight: Default::default() }
    }

    /// Returns the index of the first `key ∈ keys` such that `key > self.key`. If no such key
    /// exists, the returned index will be the length of `keys`.
    fn search_in_keys(&self, keys: &[FeerateKey]) -> usize {
        match keys.binary_search(self.key) {
            Err(idx) => {
                // self.key is not in keys, idx is the index of the following key
                idx
            }
            Ok(idx) => {
                // Exact match, return the following index
                idx + 1
            }
        }
    }
}

impl DescendVisit<FeerateKey, (), FeerateWeight> for PrefixWeightVisitor<'_> {
    type Result = f64;

    fn visit_inner(&mut self, keys: &[FeerateKey], arguments: &[FeerateWeight]) -> DescendVisitResult<Self::Result> {
        let idx = self.search_in_keys(keys);
        // Invariants:
        //      a. arguments.len() == keys.len() + 1 (n inner node keys are the separators between n+1 subtrees)
        //      b. idx <= keys.len() (hence idx < arguments.len())

        // Based on the invariants, we first accumulate all the subtree weights up to idx
        for argument in arguments.iter().take(idx) {
            self.accumulated_weight += argument.weight();
        }

        // ..and then go down to the idx'th subtree
        DescendVisitResult::GoDown(idx)
    }

    fn visit_leaf(&mut self, keys: &[FeerateKey], _values: &[()]) -> Option<Self::Result> {
        // idx is the index of the key following self.key
        let idx = self.search_in_keys(keys);
        // Accumulate all key weights up to idx (which is inclusive if self.key ∈ tree)
        for key in keys.iter().take(idx) {
            self.accumulated_weight += key.weight();
        }
        // ..and return the final result
        Some(self.accumulated_weight)
    }
}

type InnerTree = BPlusTree<NodeStoreVec<FeerateKey, (), FeerateWeight>>;

/// A transaction search tree sorted by feerate order and searchable for probabilistic weighted sampling.
///
/// All `log(n)` expressions below are in base 64 (based on constants chosen within the sweep_bptree crate).
///
/// The tree has the following properties:
///     1. Linear time ordered access (ascending / descending)
///     2. Insertions/removals in log(n) time
///     3. Search for a weight point `p ∈ [0, total_weight)` in log(n) time
///     4. Compute the prefix weight of a key, i.e., the sum of weights up to that key (inclusive)
///        according to key order, in log(n) time
///     5. Access the total weight in O(1) time. The total weight has numerical stability since it
///        is recomputed from subtree weights for each item insertion/removal
///
/// Computing the prefix weight is a crucial operation if the tree is used for random sampling and
/// the tree is highly imbalanced in terms of weight variance.
/// See [`Frontier::sample_inplace()`](crate::mempool::model::frontier::Frontier::sample_inplace)
/// for more details.  
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

    /// Inserts a key into the tree in log(n) time. Returns `false` if the key was already in the tree.
    pub fn insert(&mut self, key: FeerateKey) -> bool {
        self.tree.insert(key, ()).is_none()
    }

    /// Remove a key from the tree in log(n) time. Returns `false` if the key was not in the tree.
    pub fn remove(&mut self, key: &FeerateKey) -> bool {
        self.tree.remove(key).is_some()
    }

    /// Search for a weight point `query ∈ [0, total_weight)` in log(n) time
    pub fn search(&self, query: f64) -> &FeerateKey {
        self.tree.get_by_argument(query).expect("clamped").0
    }

    /// Access the total weight in O(1) time
    pub fn total_weight(&self) -> f64 {
        self.tree.root_argument().weight()
    }

    /// Computes the prefix weight of a key, i.e., the sum of weights up to that key (inclusive)
    /// according to key order, in log(n) time
    pub fn prefix_weight(&self, key: &FeerateKey) -> f64 {
        self.tree.descend_visit(PrefixWeightVisitor::new(key)).unwrap()
    }

    /// Iterate the tree in descending key order (going down from the
    /// highest key). Linear in the number of keys *actually* iterated.
    pub fn descending_iter(&self) -> impl DoubleEndedIterator<Item = &FeerateKey> + ExactSizeIterator + FusedIterator {
        self.tree.iter().rev().map(|(key, ())| key)
    }

    /// Iterate the tree in ascending key order (going up from the
    /// lowest key). Linear in the number of keys *actually* iterated.
    pub fn ascending_iter(&self) -> impl DoubleEndedIterator<Item = &FeerateKey> + ExactSizeIterator + FusedIterator {
        self.tree.iter().map(|(key, ())| key)
    }

    /// The lowest key in the tree (by key order)
    pub fn first(&self) -> Option<&FeerateKey> {
        self.tree.first().map(|(k, ())| k)
    }

    /// The highest key in the tree (by key order)
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
    use std::ops::Sub;

    #[test]
    fn test_feerate_weight_queries() {
        let mut tree = SearchTree::new();
        let mass = 2000;
        // The btree stores N=64 keys at each node/leaf, so we make sure the tree has more than
        // 64^2 keys in order to trigger at least a few intermediate tree nodes
        let fees = vec![[123, 113, 10_000, 1000, 2050, 2048]; 64 * (64 + 1)].into_iter().flatten().collect_vec();

        #[allow(clippy::mutable_key_type)]
        let mut s = HashSet::with_capacity(fees.len());
        for (i, fee) in fees.iter().copied().enumerate() {
            let key = build_feerate_key(fee, mass, i as u64);
            s.insert(key.clone());
            tree.insert(key);
        }

        // Randomly remove 1/6 of the items
        let remove = s.iter().take(fees.len() / 6).cloned().collect_vec();
        for r in remove {
            s.remove(&r);
            tree.remove(&r);
        }

        // Collect to vec and sort for reference
        let mut v = s.into_iter().collect_vec();
        v.sort();

        // Test reverse iteration
        for (expected, item) in v.iter().rev().zip(tree.descending_iter()) {
            assert_eq!(&expected, &item);
            assert!(expected.cmp(item).is_eq()); // Assert Ord equality as well
        }

        // Sweep through the tree and verify that weight search queries are handled correctly
        let eps: f64 = 0.001;
        let mut sum = 0.0;
        for expected in v.iter() {
            let weight = expected.weight();
            let eps = eps.min(weight / 3.0);
            let samples = [sum + eps, sum + weight / 2.0, sum + weight - eps];
            for sample in samples {
                let key = tree.search(sample);
                assert_eq!(expected, key);
                assert!(expected.cmp(key).is_eq()); // Assert Ord equality as well
            }
            sum += weight;
        }

        println!("{}, {}", sum, tree.total_weight());

        // Test clamped search bounds
        assert_eq!(tree.first(), Some(tree.search(f64::NEG_INFINITY)));
        assert_eq!(tree.first(), Some(tree.search(-1.0)));
        assert_eq!(tree.first(), Some(tree.search(-eps)));
        assert_eq!(tree.first(), Some(tree.search(0.0)));
        assert_eq!(tree.last(), Some(tree.search(sum)));
        assert_eq!(tree.last(), Some(tree.search(sum + eps)));
        assert_eq!(tree.last(), Some(tree.search(sum + 1.0)));
        assert_eq!(tree.last(), Some(tree.search(1.0 / 0.0)));
        assert_eq!(tree.last(), Some(tree.search(f64::INFINITY)));
        let _ = tree.search(f64::NAN);

        // Assert prefix weights
        let mut prefix = Vec::with_capacity(v.len());
        prefix.push(v[0].weight());
        for i in 1..v.len() {
            prefix.push(prefix[i - 1] + v[i].weight());
        }
        let eps = v.iter().map(|k| k.weight()).min_by(f64::total_cmp).unwrap() * 1e-4;
        for (expected_prefix, key) in prefix.into_iter().zip(v) {
            let prefix = tree.prefix_weight(&key);
            assert!(expected_prefix.sub(prefix).abs() < eps);
        }
    }

    #[test]
    fn test_tree_rev_iter() {
        let mut tree = SearchTree::new();
        let mass = 2000;
        let fees = vec![[123, 113, 10_000, 1000, 2050, 2048]; 64 * (64 + 1)].into_iter().flatten().collect_vec();
        let mut v = Vec::with_capacity(fees.len());
        for (i, fee) in fees.iter().copied().enumerate() {
            let key = build_feerate_key(fee, mass, i as u64);
            v.push(key.clone());
            tree.insert(key);
        }
        v.sort();

        for (expected, item) in v.into_iter().rev().zip(tree.descending_iter()) {
            assert_eq!(&expected, item);
            assert!(expected.cmp(item).is_eq()); // Assert Ord equality as well
        }
    }
}
