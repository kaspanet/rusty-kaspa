use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rand_chacha::{
    rand_core::{RngCore, SeedableRng},
    ChaCha8Rng,
};

use kaspa_muhash::MuHash;

fn bench_muhash(c: &mut Criterion) {
    let mut rng = ChaCha8Rng::from_seed([42u8; 32]);
    let mut rand_set = MuHash::new();

    let mut data = [0u8; 100];
    // Set the numerator and denominators.
    rng.fill_bytes(&mut data);
    rand_set.add_element(&data);
    rng.fill_bytes(&mut data);
    rand_set.remove_element(&data);

    rng.fill_bytes(&mut data);

    c.bench_function("MuHash::add_element", |b| {
        let mut muhash = MuHash::new();
        b.iter(|| {
            black_box(&mut data);
            muhash.add_element(&data);
        });
        black_box(muhash);
    });

    c.bench_function("MuHash::remove_element", |b| {
        let mut muhash = MuHash::new();
        b.iter(|| {
            black_box(&mut data);
            muhash.remove_element(&data);
        });
        black_box(muhash);
    });
    c.bench_function("MuHash::combine", |b| {
        let mut muhash = MuHash::new();
        b.iter(|| {
            black_box((&mut rand_set, &mut muhash));
            muhash.combine(&rand_set);
        });
        black_box(muhash);
    });

    c.bench_function("MuHash::clone", |b| {
        b.iter(|| {
            black_box(&mut rand_set);
            rand_set.clone()
        });
    });

    c.bench_function("MuHash::serialize worst", |b| {
        let mut muhash_serialized = [255u8; 384];
        //  make sure it's lower than the prime
        muhash_serialized[0..3].copy_from_slice(&[154, 40, 239]);
        muhash_serialized[192..195].copy_from_slice(&[153, 40, 239]);
        let muhash = MuHash::deserialize(muhash_serialized).unwrap();
        b.iter(|| black_box(muhash.clone()).serialize());
    });

    c.bench_function("MuHash::serialize best", |b| {
        let muhash = MuHash::new();
        b.iter(|| black_box(muhash.clone()).serialize())
    });

    c.bench_function("MuHash::serialize rand", |b| b.iter(|| black_box(rand_set.clone()).serialize()));

    c.bench_function("MuHash::finalize", |b| {
        b.iter(|| black_box(rand_set.clone()).finalize());
    });
}

criterion_group!(benches, bench_muhash);
criterion_main!(benches);
