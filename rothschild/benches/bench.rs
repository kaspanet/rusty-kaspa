use criterion::{criterion_group, criterion_main, Criterion};
use rayon::prelude::*;

use kaspa_consensus_core::{
    constants::TX_VERSION,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput},
    Hash,
};

fn constuct_tx() -> Transaction {
    let inputs = vec![TransactionInput {
        previous_outpoint: TransactionOutpoint { transaction_id: Hash::from_bytes([0xFF; 32]), index: 0 },
        signature_script: vec![],
        sequence: 0,
        sig_op_count: 1,
    }];
    let outputs = vec![TransactionOutput { value: 10000, script_public_key: ScriptPublicKey::from_vec(0, vec![0xff; 35]) }];
    Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![])
}

fn construct_txs_serially() {
    let _ = (0..10000)
        .map(|_| {
            constuct_tx();
        })
        .collect::<Vec<_>>();
}

fn construct_txs_parallel() {
    let _ = (0..10000)
        .into_par_iter()
        .map(|_| {
            constuct_tx();
        })
        .collect::<Vec<_>>();
}

pub fn bench_compare_tx_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("compare txs");
    group.bench_function("Transaction::SerialCreation", |b| b.iter(construct_txs_serially));
    group.bench_function("Transaction::ParallelCreation", |b| b.iter(construct_txs_parallel));
    group.finish();
}

criterion_group!(benches, bench_compare_tx_generation);
criterion_main!(benches);
