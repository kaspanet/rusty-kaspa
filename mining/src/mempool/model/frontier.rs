use crate::block_template::selector::ALPHA;

use super::feerate_key::FeerateTransactionKey;
use indexmap::IndexSet;
use itertools::Either;
use kaspa_consensus_core::tx::TransactionId;
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
    index: IndexSet<TransactionId>,

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
        self.index.insert(key.id);
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
        self.index.swap_remove(&key.id);
        if self.feerate_order.remove(key) {
            self.total_weight -= weight;
            self.total_mass -= mass;
            true
        } else {
            false
        }
    }

    fn sample_top_bucket<'a, R>(&'a self, rng: &'a mut R, overall_amount: u32) -> impl Iterator<Item = u32> + 'a
    where
        R: Rng + ?Sized,
    {
        let frontier_length = self.feerate_order.len() as u32;
        debug_assert!(overall_amount <= frontier_length);
        let sampling_ratio = overall_amount as f64 / frontier_length as f64;
        let distr = Uniform::new(0f64, 1f64);
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
                    // Flip a coin with the reversed probability
                    if distr.sample(rng) < (sample_approx_weight + weight) / self.total_weight {
                        Some(Some(self.index.get_index_of(&key.id).unwrap() as u32))
                    } else {
                        Some(None) // signals a continue but not a break
                    }
                }
            })
            .flatten()
    }

    pub fn sample<'a, R>(&'a self, rng: &'a mut R, overall_amount: u32) -> impl Iterator<Item = TransactionId> + 'a
    where
        R: Rng + ?Sized,
    {
        let frontier_length = self.feerate_order.len() as u32;
        if frontier_length <= overall_amount {
            return Either::Left(self.index.iter().copied());
        }

        // Based on values taken from `rand::seq::index::sample`
        const C: [f32; 2] = [270.0, 330.0 / 9.0];
        let j = if frontier_length < 500_000 { 0 } else { 1 };
        let indices = if (frontier_length as f32) < C[j] * (overall_amount as f32) {
            let initial = self.sample_top_bucket(rng, overall_amount).collect::<Vec<_>>();
            sample_inplace(rng, frontier_length, overall_amount, initial)
        } else {
            let initial = self.sample_top_bucket(rng, overall_amount).collect::<HashSet<_>>();
            sample_rejection(rng, frontier_length, overall_amount, initial)
        };

        Either::Right(indices.into_iter().map(|i| self.index.get_index(i as usize).copied().unwrap()))
    }

    pub(crate) fn len(&self) -> usize {
        self.index.len()
    }
}

/// Adaptation of `rand::seq::index::sample_inplace` for the case where there exists an
/// initial a priory sample to begin with
fn sample_inplace<R>(rng: &mut R, length: u32, amount: u32, initial: Vec<u32>) -> Vec<u32>
where
    R: Rng + ?Sized,
{
    debug_assert!(amount <= length);
    debug_assert!(initial.len() <= amount as usize);
    let mut indices: Vec<u32> = Vec::with_capacity(length as usize);
    indices.extend(0..length);
    let initial_len = initial.len() as u32;
    for (i, j) in initial.into_iter().enumerate() {
        debug_assert!(i <= (j as usize));
        debug_assert_eq!(j, indices[j as usize]);
        indices.swap(i, j as usize);
    }
    for i in initial_len..amount {
        let j: u32 = rng.gen_range(i..length);
        indices.swap(i as usize, j as usize);
    }
    indices.truncate(amount as usize);
    debug_assert_eq!(indices.len(), amount as usize);
    indices
}

/// Adaptation of `rand::seq::index::sample_rejection` for the case where there exists an
/// initial a priory sample to begin with
fn sample_rejection<R>(rng: &mut R, length: u32, amount: u32, mut initial: HashSet<u32>) -> Vec<u32>
where
    R: Rng + ?Sized,
{
    debug_assert!(amount < length);
    debug_assert!(initial.len() <= amount as usize);
    let distr = Uniform::new(0, length);
    let mut indices = Vec::with_capacity(amount as usize);
    indices.extend(initial.iter().copied());
    for _ in indices.len()..amount as usize {
        let mut pos = distr.sample(rng);
        while !initial.insert(pos) {
            pos = distr.sample(rng);
        }
        indices.push(pos);
    }

    assert_eq!(indices.len(), amount as usize);
    indices
}
