use criterion::{Criterion, black_box, criterion_group, criterion_main};

use aero_hashes::Hash;
use aero_math::Uint256;
use aero_pow::{Hasher, State};

fn bench_pow(c: &mut Criterion) {
    let state = State::from_parts(Hash::from_bytes([42; 32]), 1, Uint256::MAX);

    // Light mode: what every validating node pays per header (builds the 256 MiB cache on first use).
    let mut nonce = 0u64;
    c.bench_function("RandomX light (validation)", |b| {
        b.iter(|| {
            nonce = nonce.wrapping_add(1);
            black_box(state.calculate_pow(black_box(nonce)));
        });
    });

    // Fast mode: what a miner pays per hash. Needs ~2 GiB and builds the dataset on first use (slow).
    if std::env::var_os("AERO_BENCH_FAST").is_some() {
        let mut hasher = Hasher::fast();
        let mut nonce = 0u64;
        c.bench_function("RandomX fast (mining)", |b| {
            b.iter(|| {
                nonce = nonce.wrapping_add(1);
                black_box(state.calculate_pow_with(&mut hasher, black_box(nonce)));
            });
        });
    }
}

criterion_group!(benches, bench_pow);
criterion_main!(benches);
