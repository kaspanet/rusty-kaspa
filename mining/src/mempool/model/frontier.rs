use super::feerate_key::FeerateTransactionKey;
use arg::FeerateWeight;
use itertools::Either;
use rand::{distributions::Uniform, prelude::Distribution, Rng};
use std::collections::HashSet;
use sweep_bptree::{BPlusTree, NodeStoreVec};

pub mod arg {
    use sweep_bptree::tree::{Argument, SearchArgument};

    type FeerateKey = super::FeerateTransactionKey;

    #[derive(Clone, Copy, Debug, Default)]
    pub struct FeerateWeight(f64);

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
}

pub type FrontierTree = BPlusTree<NodeStoreVec<FeerateTransactionKey, (), FeerateWeight>>;

/// Management of the transaction pool frontier, that is, the set of transactions in
/// the transaction pool which have no mempool ancestors and are essentially ready
/// to enter the next block template.
pub struct Frontier {
    /// Frontier transactions sorted by feerate order and searchable for weight sampling
    search_tree: FrontierTree,

    /// Total sampling weight: Σ_{tx in frontier}(tx.fee/tx.mass)^alpha
    total_weight: f64,

    /// Total masses: Σ_{tx in frontier} tx.mass
    total_mass: u64,
}

impl Default for Frontier {
    fn default() -> Self {
        Self { search_tree: FrontierTree::new(Default::default()), total_weight: Default::default(), total_mass: Default::default() }
    }
}

impl Frontier {
    pub fn insert(&mut self, key: FeerateTransactionKey) -> bool {
        let (weight, mass) = (key.weight(), key.mass);
        if self.search_tree.insert(key, ()).is_none() {
            self.total_weight += weight;
            self.total_mass += mass;
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, key: &FeerateTransactionKey) -> bool {
        let (weight, mass) = (key.weight(), key.mass);
        if self.search_tree.remove(key).is_some() {
            self.total_weight -= weight;
            self.total_mass -= mass;
            true
        } else {
            false
        }
    }

    pub fn sample<'a, R>(&'a self, rng: &'a mut R, amount: u32) -> impl Iterator<Item = FeerateTransactionKey> + 'a
    where
        R: Rng + ?Sized,
    {
        let length = self.search_tree.len() as u32;
        if length <= amount {
            return Either::Left(self.search_tree.iter().map(|(k, _)| k.clone()));
        }
        let mut total_weight = self.total_weight;
        let mut distr = Uniform::new(0f64, total_weight);
        let mut down_iter = self.search_tree.iter().rev();
        let mut top = down_iter.next().expect("amount < length").0;
        let mut cache = HashSet::new();
        Either::Right((0..amount).map(move |_| {
            let query = distr.sample(rng);
            let mut item = self.search_tree.get_by_argument(query).expect("clamped").0;
            while !cache.insert(item.tx.id()) {
                if top == item {
                    // Narrow the search to reduce further sampling collisions
                    total_weight -= top.weight();
                    distr = Uniform::new(0f64, total_weight);
                    top = down_iter.next().expect("amount < length").0;
                }
                let query = distr.sample(rng);
                item = self.search_tree.get_by_argument(query).expect("clamped").0;
            }
            item.clone()
        }))
    }

    pub fn len(&self) -> usize {
        self.search_tree.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::candidate_tx::CandidateTransaction, Policy, TransactionsSelector};
    use itertools::Itertools;
    use kaspa_consensus_core::{
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{Transaction, TransactionInput, TransactionOutpoint},
    };
    use kaspa_hashes::{HasherBase, TransactionID};
    use rand::thread_rng;
    use std::{collections::HashMap, sync::Arc};

    fn generate_unique_tx(i: u64) -> Arc<Transaction> {
        let mut hasher = TransactionID::new();
        let prev = hasher.update(i.to_le_bytes()).clone().finalize();
        let input = TransactionInput::new(TransactionOutpoint::new(prev, 0), vec![], 0, 0);
        Arc::new(Transaction::new(0, vec![input], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![]))
    }

    fn build_feerate_key(fee: u64, mass: u64, id: u64) -> FeerateTransactionKey {
        FeerateTransactionKey::new(fee, mass, generate_unique_tx(id))
    }

    fn stage_two_sampling(container: impl IntoIterator<Item = FeerateTransactionKey>) -> Vec<Transaction> {
        let set = container.into_iter().map(CandidateTransaction::from_key).collect_vec();
        let mut selector = TransactionsSelector::new(Policy::new(500_000), set);
        selector.select_transactions()
    }

    #[test]
    pub fn test_two_stage_sampling() {
        let mut rng = thread_rng();
        let cap = 1000;
        let mut map = HashMap::with_capacity(cap);
        for i in 0..cap as u64 {
            let fee: u64 = if i % (cap as u64 / 100) == 0 { 1000000 } else { rng.gen_range(1..10000) };
            let mass: u64 = 1650;
            let tx = generate_unique_tx(i);
            map.insert(tx.id(), FeerateTransactionKey::new(fee.max(mass), mass, tx));
        }

        let len = cap;
        let mut frontier = Frontier::default();
        for item in map.values().take(len).cloned() {
            frontier.insert(item).then_some(()).unwrap();
        }

        let stage_one = frontier.sample(&mut rng, 10_000);
        let stage_two = stage_two_sampling(stage_one);
        stage_two.into_iter().map(|k| k.gas).sum::<u64>();
    }

    #[test]
    fn test_feerate_weight_queries() {
        let mut btree = FrontierTree::new(Default::default());
        let mass = 2000;
        // The btree stores N=64 keys at each node/leaf, so we make sure the tree has more than
        // 64^2 keys in order to trigger at least a few intermediate tree nodes
        let fees = vec![[123, 113, 10_000, 1000, 2050, 2048]; 64 * (64 + 1)].into_iter().flatten().collect_vec();
        let mut v = Vec::with_capacity(fees.len());
        for (i, fee) in fees.iter().copied().enumerate() {
            let key = build_feerate_key(fee, mass, i as u64);
            v.push(key.clone());
            btree.insert(key, ());
        }
        v.sort();
        let eps: f64 = 0.001;
        let mut sum = 0.0;
        for expected in v {
            let weight = expected.weight();
            let eps = eps.min(weight / 3.0);
            let samples = [sum + eps, sum + weight / 2.0, sum + weight - eps];
            for sample in samples {
                let key = btree.get_by_argument(sample).unwrap().0;
                assert_eq!(&expected, key);
                assert!(expected.cmp(key).is_eq()); // Assert Ord equality as well
            }
            sum += weight;
        }

        // Test clamped search bounds
        assert_eq!(btree.first(), btree.get_by_argument(f64::NEG_INFINITY));
        assert_eq!(btree.first(), btree.get_by_argument(-1.0));
        assert_eq!(btree.first(), btree.get_by_argument(-eps));
        assert_eq!(btree.first(), btree.get_by_argument(0.0));
        assert_eq!(btree.last(), btree.get_by_argument(sum));
        assert_eq!(btree.last(), btree.get_by_argument(sum + eps));
        assert_eq!(btree.last(), btree.get_by_argument(sum + 1.0));
        assert_eq!(btree.last(), btree.get_by_argument(1.0 / 0.0));
        assert_eq!(btree.last(), btree.get_by_argument(f64::INFINITY));
        assert!(btree.get_by_argument(f64::NAN).is_some());
    }

    #[test]
    fn test_btree_rev_iter() {
        let mut btree = FrontierTree::new(Default::default());
        let mass = 2000;
        let fees = vec![[123, 113, 10_000, 1000, 2050, 2048]; 64 * (64 + 1)].into_iter().flatten().collect_vec();
        let mut v = Vec::with_capacity(fees.len());
        for (i, fee) in fees.iter().copied().enumerate() {
            let key = build_feerate_key(fee, mass, i as u64);
            v.push(key.clone());
            btree.insert(key, ());
        }
        v.sort();

        for (expected, item) in v.into_iter().rev().zip(btree.iter().rev()) {
            assert_eq!(&expected, item.0);
            assert!(expected.cmp(item.0).is_eq()); // Assert Ord equality as well
        }
    }
}
