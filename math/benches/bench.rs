use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rand_chacha::{
    rand_core::{RngCore, SeedableRng},
    ChaCha8Rng,
};
use std::time::UNIX_EPOCH;

use math::construct_uint;
construct_uint!(Uint128, 2);
construct_uint!(Uint256, 4);

// Big enough to make the cache not very useful
const ITERS_256: usize = 4 * 1024 * 1024;
const ITERS_128: usize = ITERS_256 * 2;

fn bench_uint128(c: &mut Criterion) {
    let time = UNIX_EPOCH.elapsed().unwrap().as_micros();
    let mut seed = [0u8; 32];
    seed[0..16].copy_from_slice(&time.to_ne_bytes());
    let mut rng = ChaCha8Rng::from_seed(seed);

    let u128_one: Vec<_> = (0..ITERS_128).map(|_| (rng.next_u64() as u128) << 64 | rng.next_u64() as u128).collect();
    let u128_two: Vec<_> = (0..ITERS_128).map(|_| (rng.next_u64() as u128) << 64 | rng.next_u64() as u128).collect();
    let shifts: Vec<_> = (0..ITERS_128).map(|_| rng.next_u32() % 1024).collect();

    let mut u128_c = c.benchmark_group("u128");

    u128_c.bench_function("addition", |b| {
        b.iter(|| {
            for (&a, &b) in u128_one.iter().zip(u128_two.iter()) {
                black_box(a + b);
            }
        });
    });
    u128_c.bench_function("addition u64", |b| {
        b.iter(|| {
            for (&a, &b) in u128_one.iter().zip(u128_two.iter()) {
                black_box(a + (b as u64 as u128));
            }
        });
    });
    u128_c.bench_function("multiplication", |b| {
        b.iter(|| {
            for (&a, &b) in u128_one.iter().zip(u128_two.iter()) {
                black_box(a * b);
            }
        });
    });
    u128_c.bench_function("division", |b| {
        b.iter(|| {
            for (&a, &b) in u128_one.iter().zip(u128_two.iter()) {
                black_box(a / b);
            }
        });
    });
    u128_c.bench_function("u64 division", |b| {
        b.iter(|| {
            for (&a, &b) in u128_one.iter().zip(u128_two.iter()) {
                black_box(a / (b as u64 as u128));
            }
        });
    });
    u128_c.bench_function("left shift", |b| {
        b.iter(|| {
            for (&a, &b) in u128_one.iter().zip(shifts.iter()) {
                black_box(a << b);
            }
        });
    });
    u128_c.bench_function("right shift", |b| {
        b.iter(|| {
            for (&a, &b) in u128_one.iter().zip(shifts.iter()) {
                black_box(a >> b);
            }
        });
    });
    u128_c.finish();

    let mut uint128_c = c.benchmark_group("Uint128");

    let uint128_one: Vec<_> = u128_one.into_iter().map(Uint128::from_u128).collect();
    let uint128_two: Vec<_> = u128_two.into_iter().map(Uint128::from_u128).collect();

    uint128_c.bench_function("addition", |b| {
        b.iter(|| {
            for (&a, &b) in uint128_one.iter().zip(uint128_two.iter()) {
                black_box(a + b);
            }
        });
    });
    uint128_c.bench_function("addition u64", |b| {
        b.iter(|| {
            for (&a, &b) in uint128_one.iter().zip(uint128_two.iter()) {
                black_box(a + b.as_u64());
            }
        });
    });
    uint128_c.bench_function("multiplication", |b| {
        b.iter(|| {
            for (&a, &b) in uint128_one.iter().zip(uint128_two.iter()) {
                black_box(a * b);
            }
        });
    });
    uint128_c.bench_function("division", |b| {
        b.iter(|| {
            for (&a, &b) in uint128_one.iter().zip(uint128_two.iter()) {
                black_box(a / b);
            }
        });
    });

    uint128_c.bench_function("u64 division", |b| {
        b.iter(|| {
            for (&a, &b) in uint128_one.iter().zip(uint128_two.iter()) {
                black_box(a / b.as_u64());
            }
        });
    });
    uint128_c.bench_function("left shift", |b| {
        b.iter(|| {
            for (&a, &b) in uint128_one.iter().zip(shifts.iter()) {
                black_box(a << b);
            }
        });
    });
    uint128_c.bench_function("right shift", |b| {
        b.iter(|| {
            for (&a, &b) in uint128_one.iter().zip(shifts.iter()) {
                black_box(a >> b);
            }
        });
    });
    uint128_c.finish();
}

fn bench_uint256(c: &mut Criterion) {
    let time = UNIX_EPOCH.elapsed().unwrap().as_micros();
    let mut seed = [0u8; 32];
    seed[0..16].copy_from_slice(&time.to_ne_bytes());
    let mut rng = ChaCha8Rng::from_seed(seed);

    let uint256_one: Vec<_> = (0..ITERS_256)
        .map(|_| {
            rng.fill_bytes(&mut seed);
            Uint256::from_le_bytes(seed)
        })
        .collect();
    let uint256_two: Vec<_> = (0..ITERS_256)
        .map(|_| {
            rng.fill_bytes(&mut seed);
            Uint256::from_le_bytes(seed)
        })
        .collect();
    let shifts: Vec<_> = (0..ITERS_256).map(|_| rng.next_u32() % 2048).collect();

    let mut uint256_c = c.benchmark_group("uint256");

    uint256_c.bench_function("multiplication", |b| {
        b.iter(|| {
            for (&a, &b) in uint256_one.iter().zip(uint256_two.iter()) {
                black_box(a * b);
            }
        });
    });
    uint256_c.bench_function("addition", |b| {
        b.iter(|| {
            for (&a, &b) in uint256_one.iter().zip(uint256_two.iter()) {
                black_box(a + b);
            }
        });
    });
    uint256_c.bench_function("division", |b| {
        b.iter(|| {
            for (&a, &b) in uint256_one.iter().zip(uint256_two.iter()) {
                black_box(a / b);
            }
        });
    });

    uint256_c.bench_function("u64 division", |b| {
        b.iter(|| {
            for (&a, &b) in uint256_one.iter().zip(uint256_two.iter()) {
                black_box(a / b.as_u64());
            }
        });
    });

    uint256_c.bench_function("left shift", |b| {
        b.iter(|| {
            for (&a, &b) in uint256_one.iter().zip(shifts.iter()) {
                black_box(a << b);
            }
        });
    });

    uint256_c.bench_function("right shift", |b| {
        b.iter(|| {
            for (&a, &b) in uint256_one.iter().zip(shifts.iter()) {
                black_box(a >> b);
            }
        });
    });

    uint256_c.finish();
}

criterion_group!(benches, bench_uint128, bench_uint256);
criterion_main!(benches);
