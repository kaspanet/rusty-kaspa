use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::str::FromStr;

use hashes::Hash;

/// Placeholder for actual benchmarks
pub fn hash_benchmark(c: &mut Criterion) {
    c.bench_function("Hash::from_str", |b| {
        let hash_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af";
        b.iter(|| Hash::from_str(black_box(hash_str)))
    });
}

criterion_group!(benches, hash_benchmark);
criterion_main!(benches);
