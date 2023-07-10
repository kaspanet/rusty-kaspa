use criterion::{black_box, criterion_group, criterion_main, Criterion};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_COINBASE;
use kaspa_consensus_core::tx::{Transaction, TransactionId, TransactionInput, TransactionOutpoint};

fn serialize_benchmark(c: &mut Criterion) {
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
                signature_script: vec![],
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
                signature_script: vec![],
                sequence: u64::MAX,
                sig_op_count: 0,
            },
        ],
        vec![],
        0,
        SUBNETWORK_ID_COINBASE,
        0,
        vec![9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    );

    c.bench_function("Serialize Transaction", |b| b.iter(|| bincode::serialize(black_box(&transaction))));
}

fn deserialize_benchmark(c: &mut Criterion) {
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
                signature_script: vec![],
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
                signature_script: vec![],
                sequence: u64::MAX,
                sig_op_count: 0,
            },
        ],
        vec![],
        0,
        SUBNETWORK_ID_COINBASE,
        0,
        vec![9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    );
    let serialized = bincode::serialize(&transaction).unwrap();

    c.bench_function("Deserialize Transaction", |b| b.iter(|| bincode::deserialize::<Transaction>(black_box(&serialized))));
}

criterion_group!(benches, serialize_benchmark, deserialize_benchmark);
criterion_main!(benches);
