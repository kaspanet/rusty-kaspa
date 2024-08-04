use crate::Policy;

use feerate_key::FeerateTransactionKey;
use feerate_weight::{FeerateWeight, PrefixWeightVisitor};
use indexmap::IndexMap;
use kaspa_consensus_core::{block::TemplateTransactionSelector, tx::TransactionId};
use rand::{distributions::Uniform, prelude::Distribution, Rng};
use selectors::{SequenceSelector, TakeAllSelector, WeightTreeSelector};
use std::collections::HashSet;
use sweep_bptree::{BPlusTree, NodeStoreVec};

pub mod feerate_key;
pub mod feerate_weight;
mod selectors;

pub type FrontierTree = BPlusTree<NodeStoreVec<FeerateTransactionKey, (), FeerateWeight>>;

/// Management of the transaction pool frontier, that is, the set of transactions in
/// the transaction pool which have no mempool ancestors and are essentially ready
/// to enter the next block template.
pub struct Frontier {
    /// Frontier transactions sorted by feerate order and searchable for weight sampling
    search_tree: FrontierTree,

    /// Total masses: Σ_{tx in frontier} tx.mass
    total_mass: u64,
}

impl Default for Frontier {
    fn default() -> Self {
        Self { search_tree: FrontierTree::new(Default::default()), total_mass: Default::default() }
    }
}

impl Frontier {
    pub fn total_weight(&self) -> f64 {
        self.search_tree.root_argument().weight()
    }

    pub fn total_mass(&self) -> u64 {
        self.total_mass
    }

    pub fn insert(&mut self, key: FeerateTransactionKey) -> bool {
        let mass = key.mass;
        if self.search_tree.insert(key, ()).is_none() {
            self.total_mass += mass;
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, key: &FeerateTransactionKey) -> bool {
        let mass = key.mass;
        if self.search_tree.remove(key).is_some() {
            self.total_mass -= mass;
            true
        } else {
            false
        }
    }

    pub fn sample_inplace<R>(&self, rng: &mut R, policy: &Policy) -> IndexMap<TransactionId, FeerateTransactionKey>
    where
        R: Rng + ?Sized,
    {
        // TEMP
        if self.search_tree.is_empty() {
            return Default::default();
        }

        let mut distr = Uniform::new(0f64, self.total_weight());
        let mut down_iter = self.search_tree.iter().rev();
        let mut top = down_iter.next().unwrap().0;
        let mut cache = HashSet::new();
        let mut res = IndexMap::new();
        let mut total_mass: u64 = 0;
        let mut _collisions = 0;
        while cache.len() < self.search_tree.len() {
            let query = distr.sample(rng);
            let item = {
                let mut item = self.search_tree.get_by_argument(query).expect("clamped").0;
                while !cache.insert(item.tx.id()) {
                    _collisions += 1;
                    if top == item {
                        // Narrow the search to reduce further sampling collisions
                        top = down_iter.next().unwrap().0;
                        let remaining_weight = self.search_tree.descend_visit(PrefixWeightVisitor::new(top)).unwrap();
                        distr = Uniform::new(0f64, remaining_weight);
                    }
                    let query = distr.sample(rng);
                    item = self.search_tree.get_by_argument(query).expect("clamped").0;
                }
                item
            };
            if total_mass.saturating_add(item.mass) > policy.max_block_mass {
                break; // TODO
            }
            res.insert(item.tx.id(), item.clone());
            total_mass += item.mass;
        }
        // println!("Collisions: {collisions}, cache: {}", cache.len());
        res
    }

    pub fn build_selector(&self, policy: &Policy) -> Box<dyn TemplateTransactionSelector> {
        if self.total_mass <= policy.max_block_mass {
            // println!("take all");
            self.build_selector_take_all()
        } else if self.total_mass > policy.max_block_mass * 4 {
            // println!("sample inplace");
            let mut rng = rand::thread_rng();
            Box::new(SequenceSelector::new(self.sample_inplace(&mut rng, policy), policy.clone()))
        } else {
            // println!("legacy");
            Box::new(crate::TransactionsSelector::new(
                policy.clone(),
                self.search_tree
                    .iter()
                    .map(|(k, _)| k.clone())
                    .map(crate::model::candidate_tx::CandidateTransaction::from_key)
                    .collect(),
            ))
        }
    }

    pub fn build_selector_mutable_tree(&self) -> Box<dyn TemplateTransactionSelector> {
        let mut tree = FrontierTree::new(Default::default());
        for (key, ()) in self.search_tree.iter() {
            tree.insert(key.clone(), ());
        }
        Box::new(WeightTreeSelector::new(tree, Policy::new(500_000)))
    }

    pub fn build_selector_sample_inplace(&self) -> Box<dyn TemplateTransactionSelector> {
        let mut rng = rand::thread_rng();
        let policy = Policy::new(500_000);
        Box::new(SequenceSelector::new(self.sample_inplace(&mut rng, &policy), policy))
    }

    pub fn build_selector_take_all(&self) -> Box<dyn TemplateTransactionSelector> {
        Box::new(TakeAllSelector::new(self.search_tree.iter().map(|(k, _)| k.tx.clone()).collect()))
    }

    pub fn build_selector_legacy(&self) -> Box<dyn TemplateTransactionSelector> {
        Box::new(crate::TransactionsSelector::new(
            Policy::new(500_000),
            self.search_tree.iter().map(|(k, _)| k.clone()).map(crate::model::candidate_tx::CandidateTransaction::from_key).collect(),
        ))
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

    #[test]
    pub fn test_highly_irregular_sampling() {
        let mut rng = thread_rng();
        let cap = 1000;
        let mut map = HashMap::with_capacity(cap);
        for i in 0..cap as u64 {
            let mut fee: u64 = if i % (cap as u64 / 100) == 0 { 1000000 } else { rng.gen_range(1..10000) };
            if i == 0 {
                // Add an extremely large fee in order to create extremely high variance
                fee = 100_000_000 * 1_000_000; // 1M KAS
            }
            let mass: u64 = 1650;
            let key = build_feerate_key(fee, mass, i);
            map.insert(key.tx.id(), key);
        }

        let len = cap;
        let mut frontier = Frontier::default();
        for item in map.values().take(len).cloned() {
            frontier.insert(item).then_some(()).unwrap();
        }

        let _sample = frontier.sample_inplace(&mut rng, &Policy::new(500_000));
        // assert_eq!(100, sample.len());
    }

    #[test]
    fn test_feerate_weight_queries() {
        let mut btree = FrontierTree::new(Default::default());
        let mass = 2000;
        // The btree stores N=64 keys at each node/leaf, so we make sure the tree has more than
        // 64^2 keys in order to trigger at least a few intermediate tree nodes
        let fees = vec![[123, 113, 10_000, 1000, 2050, 2048]; 64 * (64 + 1)].into_iter().flatten().collect_vec();

        #[allow(clippy::mutable_key_type)]
        let mut s = HashSet::with_capacity(fees.len());
        for (i, fee) in fees.iter().copied().enumerate() {
            let key = build_feerate_key(fee, mass, i as u64);
            s.insert(key.clone());
            btree.insert(key, ());
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
        for (expected, item) in v.iter().rev().zip(btree.iter().rev()) {
            assert_eq!(&expected, &item.0);
            assert!(expected.cmp(item.0).is_eq()); // Assert Ord equality as well
        }

        // Sweep through the tree and verify that weight search queries are handled correctly
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

        println!("{}, {}", sum, btree.root_argument().weight());

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

    #[test]
    pub fn test_mempool_sampling_small() {
        let mut rng = thread_rng();
        let cap = 2000;
        let mut map = HashMap::with_capacity(cap);
        for i in 0..cap as u64 {
            let fee: u64 = rng.gen_range(1..1000000);
            let mass: u64 = 1650;
            let key = build_feerate_key(fee, mass, i);
            map.insert(key.tx.id(), key);
        }

        let len = cap;
        let mut frontier = Frontier::default();
        for item in map.values().take(len).cloned() {
            frontier.insert(item).then_some(()).unwrap();
        }

        let mut selector = frontier.build_selector(&Policy::new(500_000));
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();

        let mut selector = frontier.build_selector_legacy();
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();

        let mut selector = frontier.build_selector_mutable_tree();
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();

        let mut selector = frontier.build_selector_sample_inplace();
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();

        let mut selector = frontier.build_selector_take_all();
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();
    }
}
