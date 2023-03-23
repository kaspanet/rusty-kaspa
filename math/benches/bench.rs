use criterion::measurement::WallTime;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkGroup, Criterion};
use rand_chacha::{
    rand_core::{RngCore, SeedableRng},
    ChaCha8Rng,
};

use kaspa_math::{construct_uint, Uint3072};
construct_uint!(Uint128, 2);
construct_uint!(Uint256, 4);

// Big enough to make the cache not very useful
const ITERS_3072: usize = 128;
const ITERS_256: usize = (3072 / 256) * ITERS_3072;
const ITERS_128: usize = ITERS_256 * (256 / 128);

#[inline(always)]
fn bench_op<T, U, F>(group: &mut BenchmarkGroup<WallTime>, rhs: &[T], lhs: &[U], op: F, name: &str)
where
    T: Copy,
    U: Copy,
    F: Fn(T, U) -> T,
{
    group.bench_function(name, |b| {
        b.iter(|| {
            for (&a, &b) in rhs.iter().zip(lhs.iter()) {
                black_box(op(a, b));
            }
        });
    });
}

fn bench_uint128(c: &mut Criterion) {
    let mut rng = ChaCha8Rng::from_seed([42u8; 32]);

    let u128_one: Vec<_> = (0..ITERS_128).map(|_| (rng.next_u64() as u128) << 64 | rng.next_u64() as u128).collect();
    let u128_two: Vec<_> = (0..ITERS_128).map(|_| (rng.next_u64() as u128) << 64 | rng.next_u64() as u128).collect();
    let shifts: Vec<_> = (0..ITERS_128).map(|_| rng.next_u32() % 128 * 8).collect();
    let u64s: Vec<_> = (0..ITERS_128).map(|_| rng.next_u64()).collect();

    let mut u128_c = c.benchmark_group("u128");

    bench_op(&mut u128_c, &u128_one, &u128_two, |a, b| a + b, "add");
    bench_op(&mut u128_c, &u128_one, &u64s, |a, b| a + (b as u128), "addition u64");
    bench_op(&mut u128_c, &u128_one, &u128_two, |a, b| a * b, "multiplication");
    bench_op(&mut u128_c, &u128_one, &u64s, |a, b| a * (b as u128), "multiplication u64");
    bench_op(&mut u128_c, &u128_one, &u128_two, |a, b| a / b, "division");
    bench_op(&mut u128_c, &u128_one, &u64s, |a, b| a / (b as u128), "u64 division");
    bench_op(&mut u128_c, &u128_one, &shifts, |a, b| a << b, "left shift");
    bench_op(&mut u128_c, &u128_one, &shifts, |a, b| a >> b, "right shift");
    u128_c.finish();

    let mut uint128_c = c.benchmark_group("Uint128");

    let uint128_one: Vec<_> = u128_one.into_iter().map(Uint128::from_u128).collect();
    let uint128_two: Vec<_> = u128_two.into_iter().map(Uint128::from_u128).collect();
    bench_op(&mut uint128_c, &uint128_one, &uint128_two, |a, b| a + b, "add");
    bench_op(&mut uint128_c, &uint128_one, &u64s, |a, b| a + b, "addition u64");
    bench_op(&mut uint128_c, &uint128_one, &uint128_two, |a, b| a * b, "multiplication");
    bench_op(&mut uint128_c, &uint128_one, &u64s, |a, b| a * b, "multiplication u64");
    bench_op(&mut uint128_c, &uint128_one, &uint128_two, |a, b| a / b, "division");
    bench_op(&mut uint128_c, &uint128_one, &u64s, |a, b| a / b, "u64 division");
    bench_op(&mut uint128_c, &uint128_one, &shifts, |a, b| a << b, "left shift");
    bench_op(&mut uint128_c, &uint128_one, &shifts, |a, b| a >> b, "right shift");
    uint128_c.finish();
}

fn bench_uint256(c: &mut Criterion) {
    let mut rng = ChaCha8Rng::from_seed([42u8; 32]);
    let mut buf = [0u8; 32];
    let uint256_one: Vec<_> = (0..ITERS_256)
        .map(|_| {
            rng.fill_bytes(&mut buf);
            Uint256::from_le_bytes(buf)
        })
        .collect();
    let uint256_two: Vec<_> = (0..ITERS_256)
        .map(|_| {
            rng.fill_bytes(&mut buf);
            Uint256::from_le_bytes(buf)
        })
        .collect();
    let shifts: Vec<_> = (0..ITERS_256).map(|_| rng.next_u32() % 256 * 8).collect();
    let u64s: Vec<_> = (0..ITERS_256).map(|_| rng.next_u64()).collect();

    let mut uint256_c = c.benchmark_group("Uint256");
    bench_op(&mut uint256_c, &uint256_one, &uint256_two, |a, b| a + b, "add");
    bench_op(&mut uint256_c, &uint256_one, &u64s, |a, b| a + b, "addition u64");
    bench_op(&mut uint256_c, &uint256_one, &uint256_two, |a, b| a * b, "multiplication");
    bench_op(&mut uint256_c, &uint256_one, &u64s, |a, b| a * b, "multiplication u64");
    bench_op(&mut uint256_c, &uint256_one, &uint256_two, |a, b| a / b, "division");
    bench_op(&mut uint256_c, &uint256_one, &u64s, |a, b| a / b, "u64 division");
    bench_op(&mut uint256_c, &uint256_one, &shifts, |a, b| a << b, "left shift");
    bench_op(&mut uint256_c, &uint256_one, &shifts, |a, b| a >> b, "right shift");
    uint256_c.finish();
}

fn bench_uint3072(c: &mut Criterion) {
    let mut rng = ChaCha8Rng::from_seed([42u8; 32]);
    let mut buf = [0u8; 384];
    let uint3072_one: Vec<_> = (0..ITERS_3072)
        .map(|_| {
            rng.fill_bytes(&mut buf);
            Uint3072::from_le_bytes(buf)
        })
        .collect();
    let uint3072_two: Vec<_> = (0..ITERS_3072)
        .map(|_| {
            rng.fill_bytes(&mut buf);
            Uint3072::from_le_bytes(buf)
        })
        .collect();
    let shifts: Vec<_> = (0..ITERS_3072).map(|_| rng.next_u32() % 3072 * 8).collect();
    let u64s: Vec<_> = (0..ITERS_3072).map(|_| rng.next_u64()).collect();
    const PRIME: Uint3072 = {
        let mut max = Uint3072::MAX;
        max.0[0] -= 1103716;
        max
    };

    let mut uint3072_c = c.benchmark_group("Uint3072");
    bench_op(&mut uint3072_c, &uint3072_one, &uint3072_two, |a, b| a + b, "add");
    bench_op(&mut uint3072_c, &uint3072_one, &u64s, |a, b| a + b, "addition u64");
    bench_op(&mut uint3072_c, &uint3072_one, &uint3072_two, |a, b| a * b, "multiplication");
    bench_op(&mut uint3072_c, &uint3072_one, &u64s, |a, b| a * b, "multiplication u64");
    bench_op(&mut uint3072_c, &uint3072_one, &uint3072_two, |a, b| a / b, "division");
    bench_op(&mut uint3072_c, &uint3072_one, &u64s, |a, b| a / b, "u64 division");
    bench_op(&mut uint3072_c, &uint3072_one, &shifts, |a, b| a << b, "left shift");
    bench_op(&mut uint3072_c, &uint3072_one, &shifts, |a, b| a >> b, "right shift");
    uint3072_c.bench_function("mod_inv Muhash prime", |b| {
        b.iter(|| {
            for &a in &uint3072_one[..uint3072_one.len() / 4] {
                black_box(a.mod_inverse(PRIME));
            }
        });
    });
    uint3072_c.finish();
}

criterion_group!(benches, bench_uint128, bench_uint256, bench_uint3072);
criterion_main!(benches);
