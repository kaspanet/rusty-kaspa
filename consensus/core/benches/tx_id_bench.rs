use criterion::{Criterion, black_box, criterion_group, criterion_main};
use kaspa_consensus_core::hashing::tx::{id_v0, id_v1};
use kaspa_consensus_core::{
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput},
};
use kaspa_hashes::Hash;
use smallvec::SmallVec;

fn create_test_transaction(version: u16) -> Transaction {
    // Parse script public key for a typical P2PK output
    let mut bytes = [0u8; 34];
    faster_hex::hex_decode("208325613d2eeaf7176ac6c670b13c0043156c427438ed72d74b7800862ad884e8ac".as_bytes(), &mut bytes).unwrap();
    let script_pub_key = SmallVec::from(bytes.to_vec());

    // Create a transaction with 1 input and 2 outputs (most common case)
    let inputs = vec![TransactionInput::new(
        TransactionOutpoint::new(
            Hash::from_bytes([
                0x16, 0x5e, 0x38, 0xe8, 0xb3, 0x91, 0x45, 0x95, 0xd9, 0xc6, 0x41, 0xf3, 0xb8, 0xee, 0xc2, 0xf3, 0x46, 0x11, 0x89,
                0x6b, 0x82, 0x1a, 0x68, 0x3b, 0x7a, 0x4e, 0xde, 0xfe, 0x2c, 0x00, 0x00, 0x00,
            ]),
            0,
        ),
        vec![
            0x41, 0x20, 0x83, 0x25, 0x61, 0x3d, 0x2e, 0xea, 0xf7, 0x17, 0x6a, 0xc6, 0xc6, 0x70, 0xb1, 0x3c, 0x00, 0x43, 0x15, 0x6c,
            0x42, 0x74, 0x38, 0xed, 0x72, 0xd7, 0x4b, 0x78, 0x00, 0x86, 0x2a, 0xd8, 0x84, 0xe8, 0xac, 0x20, 0x1c, 0x2e, 0x53, 0x0e,
            0x1f, 0x4e, 0x4d, 0xc4, 0x7e, 0x3b, 0x3e, 0x0d, 0x5f, 0x3e, 0x89, 0x0a, 0xb3, 0xe9, 0x4e, 0x2d, 0x7c, 0x6e, 0x3e, 0x37,
            0x8f, 0x8e, 0x4e, 0x52, 0xac,
        ],
        0,
        1,
    )];

    let outputs = vec![
        TransactionOutput {
            value: 100_000_000, // 1 KAS
            script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()),
            covenant: None,
        },
        TransactionOutput {
            value: 50_000_000, // 0.5 KAS (change)
            script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()),
            covenant: None,
        },
    ];

    Transaction::new(version, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, Vec::new())
}

fn create_test_transaction_with_payload(version: u16, payload_len: usize) -> Transaction {
    let mut tx = create_test_transaction(version);
    tx.payload = vec![0u8; payload_len];
    tx
}

fn payload_sizes() -> Vec<usize> {
    let mut sizes = Vec::new();

    sizes.push(0);

    let mut size = 32usize;
    while size <= 256 * 1024 {
        sizes.push(size);
        size *= 2;
    }

    sizes
}

fn benchmark_tx_id_comparison(c: &mut Criterion) {
    let tx_v1 = create_test_transaction(1);

    let mut group = c.benchmark_group("tx_id_comparison");

    group.bench_function("v0_legacy", |b| b.iter(|| black_box(id_v0(black_box(&tx_v1)))));
    group.bench_function("v1", |b| b.iter(|| black_box(id_v1(black_box(&tx_v1)))));

    group.finish();
}

fn benchmark_tx_id_payload_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("tx_id_payload_scaling");

    for payload_len in payload_sizes() {
        let tx = create_test_transaction_with_payload(1, payload_len);

        let label = format!("payload_{}b", payload_len);

        group.bench_function(format!("v0/{}", label), |b| b.iter(|| black_box(id_v0(black_box(&tx)))));

        group.bench_function(format!("v1/{}", label), |b| b.iter(|| black_box(id_v1(black_box(&tx)))));
    }

    group.finish();
}

criterion_group!(benches, benchmark_tx_id_comparison, benchmark_tx_id_payload_scaling);
criterion_main!(benches);
