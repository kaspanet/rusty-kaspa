use criterion::{black_box, criterion_group, criterion_main, Criterion};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_COINBASE;
use kaspa_consensus_core::tx::{
    ScriptPublicKey, Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput,
};
use smallvec::smallvec;
use std::time::{Duration, Instant};

fn serialize_benchmark(c: &mut Criterion) {
    let script_public_key = ScriptPublicKey::new(
        0,
        smallvec![
            0x76, 0xa9, 0x21, 0x03, 0x2f, 0x7e, 0x43, 0x0a, 0xa4, 0xc9, 0xd1, 0x59, 0x43, 0x7e, 0x84, 0xb9, 0x75, 0xdc, 0x76, 0xd9,
            0x00, 0x3b, 0xf0, 0x92, 0x2c, 0xf3, 0xaa, 0x45, 0x28, 0x46, 0x4b, 0xab, 0x78, 0x0d, 0xba, 0x5e
        ],
    );
    let transaction = Transaction::new(
        0,
        vec![
            TransactionInput {
                previous_outpoint: TransactionOutpoint {
                    transaction_id: TransactionId::from_slice(&[
                        0x16, 0x5e, 0x38, 0xe8, 0xb3, 0x91, 0x45, 0x95, 0xd9, 0xc6, 0x41, 0xf3, 0xb8, 0xee, 0xc2, 0xf3, 0x46, 0x11,
                        0x89, 0x6b, 0x82, 0x1a, 0x68, 0x3b, 0x7a, 0x4e, 0xde, 0xfe, 0x2c, 0x00, 0x00, 0x00,
                    ]),
                    index: 0xffffffff,
                },
                signature_script: vec![1; 32],
                sequence: u64::MAX,
                sig_op_count: 0,
            },
            TransactionInput {
                previous_outpoint: TransactionOutpoint {
                    transaction_id: TransactionId::from_slice(&[
                        0x4b, 0xb0, 0x75, 0x35, 0xdf, 0xd5, 0x8e, 0x0b, 0x3c, 0xd6, 0x4f, 0xd7, 0x15, 0x52, 0x80, 0x87, 0x2a, 0x04,
                        0x71, 0xbc, 0xf8, 0x30, 0x95, 0x52, 0x6a, 0xce, 0x0e, 0x38, 0xc6, 0x00, 0x00, 0x00,
                    ]),
                    index: 0xffffffff,
                },
                signature_script: vec![1; 32],
                sequence: u64::MAX,
                sig_op_count: 0,
            },
        ],
        vec![
            TransactionOutput { value: 300, script_public_key: script_public_key.clone() },
            TransactionOutput { value: 300, script_public_key },
        ],
        0,
        SUBNETWORK_ID_COINBASE,
        0,
        vec![9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    );
    let size = bincode::serialized_size(&transaction).unwrap();
    let mut buf = Vec::with_capacity(size as usize);
    c.bench_function("Serialize Transaction", move |b| {
        b.iter_custom(|iters| {
            let start = Duration::default();
            (0..iters).fold(start, |acc, _| {
                let start = Instant::now();
                #[allow(clippy::unit_arg)]
                black_box(bincode::serialize_into(&mut buf, &transaction).unwrap());
                let elapsed = start.elapsed();
                buf.clear();
                acc + elapsed
            })
        })
    });
}

fn deserialize_benchmark(c: &mut Criterion) {
    let script_public_key = ScriptPublicKey::new(
        0,
        smallvec![
            0x76, 0xa9, 0x21, 0x03, 0x2f, 0x7e, 0x43, 0x0a, 0xa4, 0xc9, 0xd1, 0x59, 0x43, 0x7e, 0x84, 0xb9, 0x75, 0xdc, 0x76, 0xd9,
            0x00, 0x3b, 0xf0, 0x92, 0x2c, 0xf3, 0xaa, 0x45, 0x28, 0x46, 0x4b, 0xab, 0x78, 0x0d, 0xba, 0x5e
        ],
    );
    let transaction = Transaction::new(
        0,
        vec![
            TransactionInput {
                previous_outpoint: TransactionOutpoint {
                    transaction_id: TransactionId::from_slice(&[
                        0x16, 0x5e, 0x38, 0xe8, 0xb3, 0x91, 0x45, 0x95, 0xd9, 0xc6, 0x41, 0xf3, 0xb8, 0xee, 0xc2, 0xf3, 0x46, 0x11,
                        0x89, 0x6b, 0x82, 0x1a, 0x68, 0x3b, 0x7a, 0x4e, 0xde, 0xfe, 0x2c, 0x00, 0x00, 0x00,
                    ]),
                    index: 0xffffffff,
                },
                signature_script: vec![1; 32],
                sequence: u64::MAX,
                sig_op_count: 0,
            },
            TransactionInput {
                previous_outpoint: TransactionOutpoint {
                    transaction_id: TransactionId::from_slice(&[
                        0x4b, 0xb0, 0x75, 0x35, 0xdf, 0xd5, 0x8e, 0x0b, 0x3c, 0xd6, 0x4f, 0xd7, 0x15, 0x52, 0x80, 0x87, 0x2a, 0x04,
                        0x71, 0xbc, 0xf8, 0x30, 0x95, 0x52, 0x6a, 0xce, 0x0e, 0x38, 0xc6, 0x00, 0x00, 0x00,
                    ]),
                    index: 0xffffffff,
                },
                signature_script: vec![1; 32],
                sequence: u64::MAX,
                sig_op_count: 0,
            },
        ],
        vec![
            TransactionOutput { value: 300, script_public_key: script_public_key.clone() },
            TransactionOutput { value: 300, script_public_key },
        ],
        0,
        SUBNETWORK_ID_COINBASE,
        0,
        vec![9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    );
    let serialized = bincode::serialize(&transaction).unwrap();
    c.bench_function("Deserialize Transaction", |b| b.iter(|| black_box(bincode::deserialize::<Transaction>(&serialized))));
}

fn deserialize_script_public_key_benchmark(c: &mut Criterion) {
    let script_public_key = ScriptPublicKey::new(
        0,
        smallvec![
            0x76, 0xa9, 0x21, 0x03, 0x2f, 0x7e, 0x43, 0x0a, 0xa4, 0xc9, 0xd1, 0x59, 0x43, 0x7e, 0x84, 0xb9, 0x75, 0xdc, 0x76, 0xd9,
            0x00, 0x3b, 0xf0, 0x92, 0x2c, 0xf3, 0xaa, 0x45, 0x28, 0x46, 0x4b, 0xab, 0x78, 0x0d, 0xba, 0x5e
        ],
    );
    let serialized = bincode::serialize(&script_public_key).unwrap();
    c.bench_function("Deserialize ScriptPublicKey", |b| b.iter(|| black_box(bincode::deserialize::<ScriptPublicKey>(&serialized))));
}

fn serialize_script_public_key_benchmark(c: &mut Criterion) {
    let script_public_key = ScriptPublicKey::new(
        0,
        smallvec![
            0x76, 0xa9, 0x21, 0x03, 0x2f, 0x7e, 0x43, 0x0a, 0xa4, 0xc9, 0xd1, 0x59, 0x43, 0x7e, 0x84, 0xb9, 0x75, 0xdc, 0x76, 0xd9,
            0x00, 0x3b, 0xf0, 0x92, 0x2c, 0xf3, 0xaa, 0x45, 0x28, 0x46, 0x4b, 0xab, 0x78, 0x0d, 0xba, 0x5e
        ],
    );
    let size = bincode::serialized_size(&script_public_key).unwrap();
    let mut buf = Vec::with_capacity(size as usize);
    c.bench_function("Serialize ScriptPublicKey", move |b| {
        b.iter_custom(|iters| {
            let start = Duration::default();
            (0..iters).fold(start, |acc, _| {
                let start = Instant::now();
                #[allow(clippy::unit_arg)]
                black_box(bincode::serialize_into(&mut buf, &script_public_key).unwrap());
                let elapsed = start.elapsed();
                buf.clear();
                acc + elapsed
            })
        })
    });
}

criterion_group!(
    benches,
    serialize_benchmark,
    deserialize_benchmark,
    serialize_script_public_key_benchmark,
    deserialize_script_public_key_benchmark
);
criterion_main!(benches);
