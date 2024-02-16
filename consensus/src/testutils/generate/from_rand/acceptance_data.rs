use std::sync::Arc;

use itertools::Itertools;
use kaspa_consensus_core::{
    acceptance_data::{AcceptanceData, MergesetBlockAcceptanceData, TxEntry},
    tx::TransactionIndexType,
};
use rand::{rngs::SmallRng, seq::SliceRandom};

use crate::testutils::generate::from_rand::hash::generate_random_hash;

pub fn generate_random_acceptance_data_vec(
    rng: &mut SmallRng,
    len: usize,
    mergeset_size: usize,
    txs_per_block: TransactionIndexType,
    unaccepted_tx_ratio: f64,
) -> Vec<Arc<AcceptanceData>> {
    let mut acceptance_data_vec = Vec::with_capacity(len);
    for _ in 0..len {
        acceptance_data_vec.push(Arc::new(generate_random_acceptance_data(rng, mergeset_size, txs_per_block, unaccepted_tx_ratio)));
    }
    acceptance_data_vec
}

pub fn generate_random_acceptance_data(
    rng: &mut SmallRng,
    len: usize,
    txs_per_block: TransactionIndexType,
    unaccepted_tx_ratio: f64,
) -> AcceptanceData {
    let mut acceptance_data = AcceptanceData::with_capacity(len);
    for _ in 0..(len - 1) {
        acceptance_data.push(generate_random_mergeset_block_acceptance(rng, txs_per_block, unaccepted_tx_ratio));
    }
    acceptance_data
}

pub fn generate_random_mergeset_block_acceptance(
    rng: &mut SmallRng,
    tx_amount: TransactionIndexType,
    unaccepted_ratio: f64,
) -> MergesetBlockAcceptanceData {
    let accepted_indexes =
        (0..tx_amount - 1).collect_vec().choose_multiple(rng, (tx_amount as f64 * unaccepted_ratio) as usize).copied().collect_vec();
    MergesetBlockAcceptanceData {
        block_hash: generate_random_hash(rng),
        accepted_transactions: generate_random_tx_entries(rng, accepted_indexes.as_slice()),
    }
}

pub fn generate_random_tx_entries(rng: &mut SmallRng, indexes: &[TransactionIndexType]) -> Vec<TxEntry> {
    let mut tx_entries = Vec::with_capacity(indexes.len());
    for i in indexes.iter() {
        tx_entries.push(generate_random_tx_entry_with_index(rng, *i));
    }
    tx_entries
}

pub fn generate_random_tx_entry_with_index(rng: &mut SmallRng, index: TransactionIndexType) -> TxEntry {
    TxEntry { transaction_id: generate_random_hash(rng), index_within_block: index }
}
