//! Backward-compatibility tests for DB-persisted consensus-core structs across
//! the Toccata hardfork.
//!
//! Decodes bincode bytes produced by the pre-Toccata types (checked in under
//! `tests/fixtures/pre_toccata_db_compat_hex`) using the post-Toccata types,
//! then round-trips both the pre-Toccata values and values that use the new
//! covenant / `TxInputMass::ComputeBudget` variants introduced by Toccata.

use std::collections::HashMap;
use std::sync::LazyLock;

use kaspa_consensus_core::mass::ComputeBudget;
use kaspa_consensus_core::subnets::{SUBNETWORK_ID_COINBASE, SUBNETWORK_ID_NATIVE};
use kaspa_consensus_core::tx::{
    CovenantBinding, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, TxInputMass,
};
use kaspa_hashes::Hash;

const FIXTURE: &str = include_str!("fixtures/pre_toccata_db_compat_hex");

static FIXTURES: LazyLock<HashMap<&'static str, Vec<u8>>> = LazyLock::new(|| {
    FIXTURE
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let (name, hex) = line.split_once('\t').expect("fixture lines must be `name\\thex`");
            let mut bytes = vec![0u8; hex.len() / 2];
            faster_hex::hex_decode(hex.as_bytes(), &mut bytes).expect("fixture hex must decode");
            (name, bytes)
        })
        .collect()
});

fn fixture(name: &str) -> &'static [u8] {
    FIXTURES.get(name).unwrap_or_else(|| panic!("missing fixture `{name}`"))
}

fn spk(version: u16, script: &[u8]) -> ScriptPublicKey {
    ScriptPublicKey::new(version, script.iter().copied().collect())
}

/// Reconstructs the pre-Toccata coinbase transaction whose bincode is checked
/// in as `transaction_v0_coinbase`. Building via [`Transaction::new`] finalizes
/// a fresh post-Toccata cached id; structural eq against the deserialized
/// fixture then implicitly asserts that the post-Toccata `finalize()` produces
/// the same id as pre-Toccata for v0 transactions — a real cross-fork invariant.
fn expected_coinbase_v0() -> Transaction {
    Transaction::new(
        0,
        vec![TransactionInput::new(TransactionOutpoint::new(Hash::from_bytes([0; 32]), 0xffff_ffff), vec![0xaa, 0xbb], 0, 0)],
        vec![TransactionOutput::new(50_000_000_000, spk(0, &[0x51, 0x52]))],
        0,
        SUBNETWORK_ID_COINBASE,
        0,
        vec![0xc0, 0xff, 0xee],
    )
}

fn expected_regular_v0() -> Transaction {
    Transaction::new(
        0,
        vec![
            TransactionInput::new(TransactionOutpoint::new(Hash::from_bytes([0x11; 32]), 0), vec![0xde, 0xad, 0xbe, 0xef], 0x00ff_ff00_ff00_ff00, 1),
            TransactionInput::new(TransactionOutpoint::new(Hash::from_bytes([0x22; 32]), 7), vec![0x01, 0x02, 0x03, 0x04, 0x05], 0, 7),
        ],
        vec![
            TransactionOutput::new(1_000, spk(0, &[0x20, 0x21, 0x22])),
            TransactionOutput::new(2_000_000, spk(0, &[0x30])),
        ],
        12_345,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    )
}

#[test]
fn decode_pre_toccata_coinbase_transaction() {
    let decoded: Transaction = bincode::deserialize(fixture("transaction_v0_coinbase")).expect("decode pre-Toccata coinbase Transaction");
    assert_eq!(decoded, expected_coinbase_v0());
}


#[test]
fn decode_pre_toccata_regular_transaction() {
    let decoded: Transaction = bincode::deserialize(fixture("transaction_v0_regular")).expect("decode pre-Toccata regular Transaction");
    assert_eq!(decoded, expected_regular_v0());
}

#[test]
fn decode_pre_toccata_block_body() {
    let decoded: Vec<Transaction> = bincode::deserialize(fixture("block_body_v0")).expect("decode pre-Toccata BlockBody payload");
    let expected = vec![expected_coinbase_v0(), expected_regular_v0()];
    assert_eq!(decoded, expected);
}

#[test]
fn roundtrip_pre_toccata_regular_transaction() {
    let decoded: Transaction =
        bincode::deserialize(fixture("transaction_v0_regular")).expect("decode pre-Toccata regular Transaction");
    let re_encoded = bincode::serialize(&decoded).unwrap();
    let redecoded: Transaction = bincode::deserialize(&re_encoded).expect("re-decode Transaction");
    assert_eq!(decoded, redecoded);
}

#[test]
fn roundtrip_pre_toccata_block_body() {
    let decoded: Vec<Transaction> = bincode::deserialize(fixture("block_body_v0")).expect("decode pre-Toccata BlockBody payload");
    let re_encoded = bincode::serialize(&decoded).unwrap();
    let redecoded: Vec<Transaction> = bincode::deserialize(&re_encoded).expect("re-decode BlockBody payload");
    assert_eq!(decoded, redecoded);
}

#[test]
fn roundtrip_post_toccata_transaction_with_compute_budget_and_covenant() {
    let input = TransactionInput::new_with_mass(
        TransactionOutpoint::new(Hash::from_bytes([0x33; 32]), 2),
        vec![0xaa, 0xbb, 0xcc],
        42,
        TxInputMass::ComputeBudget(ComputeBudget(17)),
    );
    let output =
        TransactionOutput::with_covenant(999, spk(0, &[0x40, 0x41]), Some(CovenantBinding::new(0, Hash::from_bytes([0x77; 32]))));

    let tx = Transaction::new(1, vec![input], vec![output], 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let bytes = bincode::serialize(&tx).unwrap();
    let decoded: Transaction = bincode::deserialize(&bytes).unwrap();
    assert_eq!(decoded, tx);
}

#[test]
fn roundtrip_mixed_pre_and_post_toccata_block_body() {
    let tx_v0 = Transaction::new(
        0,
        vec![TransactionInput::new(TransactionOutpoint::new(Hash::from_bytes([0x44; 32]), 0), vec![0x01], 0, 3)],
        vec![TransactionOutput::new(10, spk(0, &[0x50]))],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );

    let tx_v1 = Transaction::new(
        1,
        vec![TransactionInput::new_with_mass(
            TransactionOutpoint::new(Hash::from_bytes([0x55; 32]), 1),
            vec![0x02, 0x03],
            0,
            TxInputMass::ComputeBudget(ComputeBudget(5)),
        )],
        vec![TransactionOutput::with_covenant(20, spk(0, &[0x60]), Some(CovenantBinding::new(0, Hash::from_bytes([0x88; 32]))))],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );

    let block_body: Vec<Transaction> = vec![tx_v0, tx_v1];
    let bytes = bincode::serialize(&block_body).unwrap();
    let decoded: Vec<Transaction> = bincode::deserialize(&bytes).unwrap();
    assert_eq!(decoded, block_body);
}
