use crate::block_template::selector::ALPHA;

use super::feerate_key::FeerateTransactionKey;
use indexmap::IndexSet;
use itertools::Either;
use kaspa_utils::vec::VecExtensions;
use rand::{distributions::Uniform, prelude::Distribution, Rng};
use std::collections::{BTreeSet, HashSet};

/// Management of the transaction pool frontier, that is, the set of transactions in
/// the transaction pool which have no mempool ancestors and are essentially ready
/// to enter the next block template.
#[derive(Default)]
pub struct Frontier {
    /// Frontier transactions sorted by feerate order
    feerate_order: BTreeSet<FeerateTransactionKey>,

    /// Frontier transactions accessible via random access
    index: IndexSet<FeerateTransactionKey>,

    /// Total sampling weight: Σ_{tx in frontier}(tx.fee/tx.mass)^alpha
    total_weight: f64,

    /// Total masses: Σ_{tx in frontier} tx.mass
    total_mass: u64,
}

impl Frontier {
    // pub fn new() -> Self {
    //     Self { ..Default::default() }
    // }

    pub fn insert(&mut self, key: FeerateTransactionKey) -> bool {
        let (weight, mass) = (key.feerate().powi(ALPHA), key.mass);
        self.index.insert(key.clone());
        if self.feerate_order.insert(key) {
            self.total_weight += weight;
            self.total_mass += mass;
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, key: &FeerateTransactionKey) -> bool {
        let (weight, mass) = (key.feerate().powi(ALPHA), key.mass);
        self.index.swap_remove(key);
        if self.feerate_order.remove(key) {
            self.total_weight -= weight;
            self.total_mass -= mass;
            true
        } else {
            false
        }
    }

    fn sample_top_bucket<R>(&self, rng: &mut R, overall_amount: u32) -> (Vec<u32>, HashSet<u32>)
    where
        R: Rng + ?Sized,
    {
        let frontier_length = self.feerate_order.len() as u32;
        debug_assert!(overall_amount <= frontier_length);
        let sampling_ratio = overall_amount as f64 / frontier_length as f64;
        let distr = Uniform::new(0f64, 1f64);
        let mut filter = HashSet::new();
        let filter_ref = &mut filter;
        (
            self.feerate_order
                .iter()
                .rev()
                .map_while(move |key| {
                    let weight = key.feerate().powi(ALPHA);
                    let exclusive_total_weight = self.total_weight - weight;
                    let sample_approx_weight = exclusive_total_weight * sampling_ratio;
                    if weight < exclusive_total_weight / 100.0 {
                        None // break out map_while
                    } else {
                        let p = weight / self.total_weight;
                        let p_s = weight / (sample_approx_weight + weight);
                        debug_assert!(p <= p_s);
                        let idx = self.index.get_index_of(key).unwrap() as u32;
                        // Register this index as "already sampled"
                        filter_ref.insert(idx);
                        // Flip a coin with the reversed probability
                        if distr.sample(rng) < (sample_approx_weight + weight) / self.total_weight {
                            Some(Some(idx))
                        } else {
                            Some(None) // signals a continue but not a break
                        }
                    }
                })
                .flatten()
                .collect(),
            filter,
        )
    }

    pub fn sample<'a, R>(&'a self, rng: &'a mut R, overall_amount: u32) -> impl Iterator<Item = FeerateTransactionKey> + 'a
    where
        R: Rng + ?Sized,
    {
        let frontier_length = self.feerate_order.len() as u32;
        if frontier_length <= overall_amount {
            return Either::Left(self.index.iter().cloned());
        }

        // Based on values taken from `rand::seq::index::sample`
        const C: [f32; 2] = [270.0, 330.0 / 9.0];
        let j = if frontier_length < 500_000 { 0 } else { 1 };
        let indices = if (frontier_length as f32) < C[j] * (overall_amount as f32) {
            let (top, filter) = self.sample_top_bucket(rng, overall_amount);
            sample_inplace(rng, frontier_length, overall_amount - top.len() as u32, overall_amount, filter).chain(top)
        } else {
            let (top, filter) = self.sample_top_bucket(rng, overall_amount);
            sample_rejection(rng, frontier_length, overall_amount - top.len() as u32, overall_amount, filter).chain(top)
        };

        Either::Right(indices.into_iter().map(|i| self.index.get_index(i as usize).cloned().unwrap()))
    }

    pub(crate) fn len(&self) -> usize {
        self.index.len()
    }
}

/// Adaptation of `rand::seq::index::sample_inplace` for the case where there exists an
/// initial a priory sample to begin with
fn sample_inplace<R>(rng: &mut R, length: u32, amount: u32, capacity: u32, filter: HashSet<u32>) -> Vec<u32>
where
    R: Rng + ?Sized,
{
    debug_assert!(amount <= length);
    debug_assert!(filter.len() <= amount as usize);
    let mut indices: Vec<u32> = Vec::with_capacity(length.max(capacity) as usize);
    indices.extend(0..length);
    for i in 0..amount {
        let mut j: u32 = rng.gen_range(i..length);
        while filter.contains(&j) {
            j = rng.gen_range(i..length);
        }
        indices.swap(i as usize, j as usize);
    }
    indices.truncate(amount as usize);
    debug_assert_eq!(indices.len(), amount as usize);
    indices
}

/// Adaptation of `rand::seq::index::sample_rejection` for the case where there exists an
/// initial a priory sample to begin with
fn sample_rejection<R>(rng: &mut R, length: u32, amount: u32, capacity: u32, mut filter: HashSet<u32>) -> Vec<u32>
where
    R: Rng + ?Sized,
{
    debug_assert!(amount < length);
    debug_assert!(filter.len() <= amount as usize);
    let distr = Uniform::new(0, length);
    let mut indices = Vec::with_capacity(amount.max(capacity) as usize);
    for _ in indices.len()..amount as usize {
        let mut pos = distr.sample(rng);
        while !filter.insert(pos) {
            pos = distr.sample(rng);
        }
        indices.push(pos);
    }

    assert_eq!(indices.len(), amount as usize);
    indices
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

    fn stage_two_sampling(container: impl IntoIterator<Item = FeerateTransactionKey>) -> Vec<Transaction> {
        let set = container.into_iter().map(CandidateTransaction::from_key).collect_vec();
        let mut selector = TransactionsSelector::new(Policy::new(500_000), set);
        selector.select_transactions()
    }

    #[test]
    pub fn test_two_stage_sampling() {
        let mut rng = thread_rng();
        let cap = 1_000_000;
        let mut map = HashMap::with_capacity(cap);
        for i in 0..cap as u64 {
            let fee: u64 = rng.gen_range(1..100000);
            let mass: u64 = 1650;
            let tx = generate_unique_tx(i);
            map.insert(tx.id(), FeerateTransactionKey { fee: fee.max(mass), mass, tx });
        }

        let len = cap; // / 10;
        let mut frontier = Frontier::default();
        for item in map.values().take(len).cloned() {
            frontier.insert(item).then_some(()).unwrap();
        }

        let stage_one = frontier.sample(&mut rng, 10_000);
        let stage_two = stage_two_sampling(stage_one);
        stage_two.into_iter().map(|k| k.gas).sum::<u64>();
    }
}
