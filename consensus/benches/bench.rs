use consensus_core::tx::{TransactionId, TransactionOutpoint};
use consensus_core::{BlockHasher, TransactionOutpointHasher};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hashes::Hash as KHash;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::str::FromStr;

/// placeholder for actual benchmarks.  
pub fn hash_benchmark(c: &mut Criterion) {
    c.bench_function("Hash::from_str", |b| {
        let hash_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af";
        b.iter(|| KHash::from_str(black_box(hash_str)))
    });
}

/// bench [`DefaultHasher`] for [`hashes::Hash`],
pub fn default_hasher_hash_benchmark(c: &mut Criterion) {
    c.bench_function("hash.hash (DefaultHasher)", |b| {
        let mut hasher = DefaultHasher::new();
        let hash = KHash::from_str("8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af").unwrap();
        b.iter(|| hash.hash(black_box(&mut hasher)));
    });
}

/// bench [`BlockHasher`] for [`hashes::Hash`],
pub fn block_hasher_hash_benchmark(c: &mut Criterion) {
    c.bench_function("hash.hash (BlockHasher)", |b| {
        let mut hasher = BlockHasher::new();
        let hash = KHash::from_str("8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af").unwrap();
        b.iter(|| hash.hash(black_box(&mut hasher)));
    });
}

/// bench [`DefaultHasher`] for [`TransactionOutpoint`],
pub fn default_hasher_transaction_outpoint_benchmark(c: &mut Criterion) {
    c.bench_function("tx_outpoint.hash (DefaultHasher)", |b| {
        let mut hasher = DefaultHasher::new();
        let tx_outpoint = TransactionOutpoint::new(
            TransactionId::from_str("8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af").unwrap(),
            124,
        );
        b.iter(|| tx_outpoint.hash(black_box(&mut hasher)));
    });
}

/// bench [`TransactionOutpointHasher`] for [`TransactionOutpoint`],
pub fn outpoint_hasher_hash_benchmark(c: &mut Criterion) {
    c.bench_function("tx_outpoint.hash (TransactionOutpointHasher)", |b| {
        let mut hasher = TransactionOutpointHasher::new();
        let tx_outpoint = TransactionOutpoint::new(
            TransactionId::from_str("8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af").unwrap(),
            124,
        );
        b.iter(|| tx_outpoint.hash(black_box(&mut hasher)));
    });
}

criterion_group!(
    benches,
    default_hasher_hash_benchmark,
    block_hasher_hash_benchmark,
    default_hasher_transaction_outpoint_benchmark,
    outpoint_hasher_hash_benchmark,
    hash_benchmark,
);
criterion_main!(benches);
