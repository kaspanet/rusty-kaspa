use aero_hashes::*;
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rand::{Rng, RngCore, thread_rng};
use std::any::type_name;

fn test_bytes_hasher<H: Hasher>(c: &mut Criterion) {
    let mut rng = thread_rng();
    let buf: [u8; 32] = rng.r#gen();
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

criterion_group!(benches, bench_hashers);
criterion_main!(benches);
