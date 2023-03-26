use criterion::{black_box, criterion_group, criterion_main, Criterion};
use kaspa_hashes::*;
use rand::{thread_rng, Rng, RngCore};
use sha3::digest::{ExtendableOutput, Update, XofReader};
use sha3::{CShake256, CShake256Core};
use std::any::type_name;

fn test_bytes_hasher<H: Hasher>(c: &mut Criterion) {
    let mut rng = thread_rng();
    let buf: [u8; 32] = rng.gen();
    c.bench_function(&format!("32 bytes: {}", type_name::<H>()), |b| {
        b.iter(|| {
            let buf = black_box(buf);
            black_box(H::hash(buf));
        })
    });

    let mut buf = vec![0u8; 1024];
    rng.fill_bytes(&mut buf);
    c.bench_function(&format!("1024 bytes: {}", type_name::<H>()), |b| {
        b.iter(|| {
            black_box(buf.as_mut_slice());
            black_box(H::hash(&buf));
        })
    });
}

fn bench_hashers(c: &mut Criterion) {
    test_bytes_hasher::<TransactionHash>(c);
    test_bytes_hasher::<TransactionID>(c);
    test_bytes_hasher::<TransactionSigningHash>(c);
    test_bytes_hasher::<BlockHash>(c);
    test_bytes_hasher::<ProofOfWorkHash>(c);
    test_bytes_hasher::<MerkleBranchHash>(c);
    test_bytes_hasher::<MuHashElementHash>(c);
    test_bytes_hasher::<MuHashFinalizeHash>(c);
    test_bytes_hasher::<TransactionSigningHashECDSA>(c);
}

fn bench_pow_hash(c: &mut Criterion) {
    let mut rng = thread_rng();
    let timestamp: u64 = rng.gen();
    let pre_pow_hash = Hash::from_bytes(rng.gen());
    let nonce: u64 = rng.gen();
    c.bench_function("PoWHash including timestamp", |b| {
        b.iter(|| {
            let hasher = PowHash::new(black_box(pre_pow_hash), black_box(timestamp));
            black_box(hasher.finalize_with_nonce(black_box(nonce)));
        })
    });
    let hasher = PowHash::new(black_box(pre_pow_hash), black_box(timestamp));
    c.bench_function("PoWHash without timestamp", |b| {
        b.iter(|| {
            black_box(black_box(hasher.clone()).finalize_with_nonce(black_box(nonce)));
        })
    });

    c.bench_function("generic PoWHash including timestamp", |b| {
        b.iter(|| {
            let hasher = CShake256::from_core(CShake256Core::new(b"ProofOfWorkHash"))
                .chain(black_box(pre_pow_hash.as_bytes()))
                .chain(black_box(timestamp).to_le_bytes())
                .chain([0u8; 32])
                .chain(black_box(nonce).to_le_bytes());
            let mut hash = [0u8; 32];
            hasher.finalize_xof().read(&mut hash);
            black_box(hash);
        })
    });
    let hasher = CShake256::from_core(CShake256Core::new(b"ProofOfWorkHash"))
        .chain(black_box(pre_pow_hash.as_bytes()))
        .chain(black_box(timestamp).to_le_bytes())
        .chain([0u8; 32]);

    c.bench_function("generic PoWHash without timestamp", |b| {
        b.iter(|| {
            let hasher = black_box(hasher.clone()).chain(black_box(nonce).to_le_bytes());
            let mut hash = [0u8; 32];
            hasher.finalize_xof().read(&mut hash);
            black_box(hash);
        })
    });
}

fn bench_heavy_hash(c: &mut Criterion) {
    let mut rng = thread_rng();
    let in_hash = Hash::from_bytes(rng.gen());

    c.bench_function("KHeavyHash", |b| {
        b.iter(|| {
            black_box(KHeavyHash::hash(in_hash));
        })
    });

    let hasher = CShake256::from_core(CShake256Core::new(b"KHeavyHash"));
    c.bench_function("generic KHeavyHash without init", |b| {
        b.iter(|| {
            let hasher = hasher.clone().chain(in_hash.as_bytes());
            let mut hash = [0u8; 32];
            hasher.finalize_xof().read(&mut hash);
            black_box(hash);
        })
    });

    c.bench_function("generic KHeavyHash with init", |b| {
        b.iter(|| {
            let hasher = CShake256::from_core(CShake256Core::new(b"KHeavyHash")).chain(black_box(in_hash.as_bytes()));
            let mut hash = [0u8; 32];
            hasher.finalize_xof().read(&mut hash);
            black_box(hash);
        })
    });
}

criterion_group!(benches, bench_pow_hash, bench_heavy_hash, bench_hashers);
criterion_main!(benches);
