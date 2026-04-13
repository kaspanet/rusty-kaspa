//! Backward-compatibility tests for DB-persisted consensus-core structs across
//! the Toccata hardfork.
//!
//! Decodes bincode bytes produced by the pre-Toccata types (checked in under
//! `tests/fixtures/pre_toccata_db_compat.hex`) using the post-Toccata types,
//! then round-trips both the pre-Toccata values and values that use the new
//! covenant / `TxInputMass::ComputeBudget` variants introduced by Toccata.

use std::collections::HashMap;
use std::sync::LazyLock;

use kaspa_consensus_core::mass::{ComputeBudget, SigopCount};
use kaspa_consensus_core::subnets::{SUBNETWORK_ID_COINBASE, SUBNETWORK_ID_NATIVE};
use kaspa_consensus_core::tx::{
    CovenantBinding, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, TxInputMass, UtxoEntry,
};
use kaspa_hashes::Hash;

const FIXTURE: &str = include_str!("fixtures/pre_toccata_db_compat.hex");

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

const EXPECTED_UTXO_AMOUNT: u64 = 0x0123_4567_89ab_cdef;
const EXPECTED_UTXO_DAA_SCORE: u64 = 42;
const EXPECTED_UTXO_SPK_SCRIPT: &[u8] = &[0x76, 0xa9, 0x14, 0x01, 0x02, 0x03];
const EXPECTED_COINBASE_PAYLOAD: &[u8] = &[0xc0, 0xff, 0xee];
const EXPECTED_REGULAR_IN1_SEQ: u64 = 0x00ff_ff00_ff00_ff00;

// -----------------------------------------------------------------------------
// Decode fixtures produced by the pre-Toccata types.
// -----------------------------------------------------------------------------

#[test]
fn decode_pre_toccata_utxo_entry() {
    let bytes = fixture("utxo_entry_v0");

    let decoded: UtxoEntry = bincode::deserialize(bytes).expect("decode pre-Toccata UtxoEntry");

    assert_eq!(decoded.amount, EXPECTED_UTXO_AMOUNT, "amount");
    assert_eq!(decoded.block_daa_score, EXPECTED_UTXO_DAA_SCORE, "block_daa_score");
    assert!(decoded.is_coinbase, "is_coinbase");
    assert_eq!(decoded.script_public_key.version, 0, "spk version");
    assert_eq!(decoded.script_public_key.script(), EXPECTED_UTXO_SPK_SCRIPT, "spk script");
    assert_eq!(decoded.covenant_id, None, "covenant_id");
}

#[test]
fn decode_pre_toccata_coinbase_transaction() {
    let bytes = fixture("transaction_v0_coinbase");

    let decoded: Transaction = bincode::deserialize(bytes).expect("decode pre-Toccata coinbase Transaction");

    assert_eq!(decoded.version, 0, "version");
    assert_eq!(decoded.subnetwork_id, SUBNETWORK_ID_COINBASE, "subnetwork_id");
    assert_eq!(decoded.gas, 0, "gas");
    assert_eq!(decoded.lock_time, 0, "lock_time");
    assert_eq!(decoded.payload, EXPECTED_COINBASE_PAYLOAD, "payload");

    assert_eq!(decoded.inputs.len(), 1, "inputs.len");
    let i0 = &decoded.inputs[0];
    assert_eq!(i0.previous_outpoint.transaction_id, Hash::from_bytes([0; 32]), "coinbase outpoint hash");
    assert_eq!(i0.previous_outpoint.index, 0xffff_ffff, "coinbase outpoint index");
    assert_eq!(i0.signature_script, vec![0xaa, 0xbb], "coinbase signature_script");
    assert_eq!(i0.sequence, 0, "coinbase sequence");
    assert_eq!(i0.mass, TxInputMass::SigopCount(SigopCount(0)), "coinbase input mass");

    assert_eq!(decoded.outputs.len(), 1, "outputs.len");
    let o0 = &decoded.outputs[0];
    assert_eq!(o0.value, 50_000_000_000, "output value");
    assert_eq!(o0.script_public_key, spk(0, &[0x51, 0x52]), "output spk");
    assert_eq!(o0.covenant, None, "output covenant");

    let expected_coinbase_id = Hash::from_bytes([
        0xb4, 0x51, 0x8f, 0xe7, 0xb6, 0xea, 0x3a, 0xd3, 0x19, 0xbe, 0x3d, 0x39, 0x87, 0x7c, 0x14, 0x19, 0xaa, 0x93, 0xa1, 0xd5, 0x1e,
        0x86, 0x5f, 0x0a, 0x79, 0x08, 0x60, 0x33, 0x53, 0xc3, 0x72, 0x51,
    ]);
    assert_eq!(decoded.id(), expected_coinbase_id, "cached id");
}

#[test]
fn decode_pre_toccata_regular_transaction() {
    let bytes = fixture("transaction_v0_regular");

    let decoded: Transaction = bincode::deserialize(bytes).expect("decode pre-Toccata regular Transaction");

    assert_eq!(decoded.version, 0, "version");
    assert_eq!(decoded.subnetwork_id, SUBNETWORK_ID_NATIVE, "subnetwork_id");
    assert_eq!(decoded.gas, 0, "gas");
    assert_eq!(decoded.lock_time, 12345, "lock_time");
    assert!(decoded.payload.is_empty(), "payload");

    assert_eq!(decoded.inputs.len(), 2, "inputs.len");
    let i0 = &decoded.inputs[0];
    assert_eq!(i0.previous_outpoint.transaction_id, Hash::from_bytes([0x11; 32]), "i0 outpoint hash");
    assert_eq!(i0.previous_outpoint.index, 0, "i0 outpoint index");
    assert_eq!(i0.signature_script, vec![0xde, 0xad, 0xbe, 0xef], "i0 signature_script");
    assert_eq!(i0.sequence, EXPECTED_REGULAR_IN1_SEQ, "i0 sequence");
    assert_eq!(i0.mass, TxInputMass::SigopCount(SigopCount(1)), "i0 mass");

    let i1 = &decoded.inputs[1];
    assert_eq!(i1.previous_outpoint.transaction_id, Hash::from_bytes([0x22; 32]), "i1 outpoint hash");
    assert_eq!(i1.previous_outpoint.index, 7, "i1 outpoint index");
    assert_eq!(i1.signature_script, vec![0x01, 0x02, 0x03, 0x04, 0x05], "i1 signature_script");
    assert_eq!(i1.sequence, 0, "i1 sequence");
    assert_eq!(i1.mass, TxInputMass::SigopCount(SigopCount(7)), "i1 mass");

    assert_eq!(decoded.outputs.len(), 2, "outputs.len");
    assert_eq!(decoded.outputs[0].value, 1_000, "o0 value");
    assert_eq!(decoded.outputs[0].script_public_key, spk(0, &[0x20, 0x21, 0x22]), "o0 spk");
    assert_eq!(decoded.outputs[0].covenant, None, "o0 covenant");
    assert_eq!(decoded.outputs[1].value, 2_000_000, "o1 value");
    assert_eq!(decoded.outputs[1].script_public_key, spk(0, &[0x30]), "o1 spk");
    assert_eq!(decoded.outputs[1].covenant, None, "o1 covenant");

    let expected_regular_id = Hash::from_bytes([
        0x79, 0xdf, 0x6f, 0x61, 0xb8, 0xa0, 0xbd, 0x9f, 0xbf, 0xb3, 0xc5, 0xd7, 0x45, 0x81, 0xee, 0x8c, 0x1b, 0xcc, 0x4a, 0xe4, 0x2e,
        0x42, 0xba, 0x69, 0x91, 0xac, 0x30, 0xf7, 0x1c, 0xe0, 0xc1, 0x26,
    ]);
    assert_eq!(decoded.id(), expected_regular_id, "cached id");
}

#[test]
fn decode_pre_toccata_block_body() {
    let bytes = fixture("block_body_v0");

    let decoded: Vec<Transaction> = bincode::deserialize(bytes).expect("decode pre-Toccata BlockBody payload");

    assert_eq!(decoded.len(), 2, "block body length");
    assert!(decoded[0].is_coinbase(), "first tx must be coinbase");
    assert_eq!(decoded[0].subnetwork_id, SUBNETWORK_ID_COINBASE, "first tx subnetwork");
    assert_eq!(decoded[1].subnetwork_id, SUBNETWORK_ID_NATIVE, "second tx subnetwork");
    assert_eq!(decoded[1].inputs.len(), 2, "second tx inputs");
    assert_eq!(decoded[1].outputs.len(), 2, "second tx outputs");
}

// -----------------------------------------------------------------------------
// Round-trip pre-Toccata fixtures through the post-Toccata serializer.
// -----------------------------------------------------------------------------

#[test]
fn roundtrip_pre_toccata_utxo_entry() {
    let decoded: UtxoEntry = bincode::deserialize(fixture("utxo_entry_v0")).expect("decode pre-Toccata UtxoEntry");
    let re_encoded = bincode::serialize(&decoded).unwrap();
    let redecoded: UtxoEntry = bincode::deserialize(&re_encoded).expect("re-decode UtxoEntry");
    assert_eq!(decoded, redecoded);
}

#[test]
fn roundtrip_pre_toccata_regular_transaction() {
    let decoded: Transaction = bincode::deserialize(fixture("transaction_v0_regular")).expect("decode pre-Toccata regular Transaction");
    let re_encoded = bincode::serialize(&decoded).unwrap();
    let redecoded: Transaction = bincode::deserialize(&re_encoded).expect("re-decode Transaction");

    assert_eq!(decoded.version, redecoded.version, "version");
    assert_eq!(decoded.inputs.len(), redecoded.inputs.len(), "inputs.len");
    for (a, b) in decoded.inputs.iter().zip(redecoded.inputs.iter()) {
        assert_eq!(a.previous_outpoint, b.previous_outpoint, "previous_outpoint");
        assert_eq!(a.signature_script, b.signature_script, "signature_script");
        assert_eq!(a.sequence, b.sequence, "sequence");
        assert_eq!(a.mass, b.mass, "mass");
    }
    assert_eq!(decoded.outputs, redecoded.outputs, "outputs");
    assert_eq!(decoded.lock_time, redecoded.lock_time, "lock_time");
    assert_eq!(decoded.subnetwork_id, redecoded.subnetwork_id, "subnetwork_id");
    assert_eq!(decoded.gas, redecoded.gas, "gas");
    assert_eq!(decoded.payload, redecoded.payload, "payload");
    assert_eq!(decoded.id(), redecoded.id(), "cached id");
}

#[test]
fn roundtrip_pre_toccata_block_body() {
    let decoded: Vec<Transaction> = bincode::deserialize(fixture("block_body_v0")).expect("decode pre-Toccata BlockBody payload");
    let re_encoded = bincode::serialize(&decoded).unwrap();
    let redecoded: Vec<Transaction> = bincode::deserialize(&re_encoded).expect("re-decode BlockBody payload");
    assert_eq!(decoded.len(), redecoded.len());
    for (a, b) in decoded.iter().zip(redecoded.iter()) {
        assert_eq!(a.id(), b.id(), "tx id");
        assert_eq!(a.inputs.len(), b.inputs.len(), "inputs.len");
        assert_eq!(a.outputs, b.outputs, "outputs");
    }
}

// -----------------------------------------------------------------------------
// Round-trip values using the post-Toccata covenant / TxInputMass variants.
// -----------------------------------------------------------------------------

#[test]
fn roundtrip_post_toccata_utxo_entry_with_covenant_id() {
    let utxo = UtxoEntry::new(1_000, spk(0, &[0x01, 0x02]), 777, false, Some(Hash::from_bytes([0x5a; 32])));
    let bytes = bincode::serialize(&utxo).unwrap();
    let decoded: UtxoEntry = bincode::deserialize(&bytes).unwrap();
    assert_eq!(utxo, decoded);
    assert_eq!(decoded.covenant_id, Some(Hash::from_bytes([0x5a; 32])));
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

    assert_eq!(decoded.version, 1, "version");
    assert_eq!(decoded.inputs.len(), 1, "inputs.len");
    assert_eq!(decoded.inputs[0].mass, TxInputMass::ComputeBudget(ComputeBudget(17)), "compute budget mass round-trip");
    assert_eq!(decoded.outputs.len(), 1, "outputs.len");
    assert_eq!(
        decoded.outputs[0].covenant,
        Some(CovenantBinding::new(0, Hash::from_bytes([0x77; 32]))),
        "covenant binding round-trip"
    );
    assert_eq!(decoded.id(), tx.id(), "cached id");
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

    let block_body: Vec<Transaction> = vec![tx_v0.clone(), tx_v1.clone()];
    let bytes = bincode::serialize(&block_body).unwrap();
    let decoded: Vec<Transaction> = bincode::deserialize(&bytes).unwrap();

    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].version, 0);
    assert_eq!(decoded[0].inputs[0].mass, TxInputMass::SigopCount(SigopCount(3)));
    assert_eq!(decoded[0].outputs[0].covenant, None);
    assert_eq!(decoded[1].version, 1);
    assert_eq!(decoded[1].inputs[0].mass, TxInputMass::ComputeBudget(ComputeBudget(5)));
    assert_eq!(decoded[1].outputs[0].covenant, Some(CovenantBinding::new(0, Hash::from_bytes([0x88; 32]))));
    assert_eq!(decoded[0].id(), tx_v0.id());
    assert_eq!(decoded[1].id(), tx_v1.id());
}
