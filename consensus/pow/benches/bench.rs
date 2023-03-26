use criterion::{black_box, criterion_group, criterion_main, Criterion};

use kaspa_hashes::Hash;
use kaspa_pow::{matrix::Matrix, xoshiro::XoShiRo256PlusPlus};

// Big enough to make the cache not very useful
const ITERS: usize = 1024;

fn bench_pow(c: &mut Criterion) {
    let mut gen = XoShiRo256PlusPlus::new(Hash::from_bytes([42; 32]));
    let gen_hash = |gen: &mut XoShiRo256PlusPlus| Hash::from_le_u64([gen.u64(), gen.u64(), gen.u64(), gen.u64()]);
    let matrices: Vec<_> = (0..ITERS).map(|_| Matrix::generate(gen_hash(&mut gen))).collect();
    let hashes: Vec<_> = (0..ITERS).map(|_| gen_hash(&mut gen)).collect();

    c.bench_function("Compute Rank", |b| {
        b.iter(|| {
            for matrix in &matrices {
                black_box(matrix.compute_rank());
            }
        });
    });

    c.bench_function("HeavyHash", |b| {
        b.iter(|| {
            for (matrix, &hash) in matrices.iter().zip(hashes.iter()) {
                black_box(matrix.heavy_hash(hash));
            }
        });
    });
}

criterion_group!(benches, bench_pow);
criterion_main!(benches);
