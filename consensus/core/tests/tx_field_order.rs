//! Transaction JSON deserialization must not depend on field order: serde
//! maps are unordered, and `serde_json::to_value` emits alphabetically sorted
//! keys, so `inputs`/`outputs` arrive before `version` on any `from_value`
//! roundtrip. Run this file with `--no-default-features` as well; that build
//! currently rejects out-of-order fields (no_std has no value buffering).

use kaspa_consensus_core::Hash;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{
    CovenantBinding, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput,
};

fn sample_v0() -> Transaction {
    Transaction::new(
        0,
        vec![TransactionInput::new(TransactionOutpoint::new(Hash::from_bytes([0x11; 32]), 0), vec![0xaa, 0xbb], 42, 3)],
        vec![TransactionOutput::new(100, ScriptPublicKey::new(0, vec![0x51].into()))],
        123,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![0xcc],
    )
}

fn sample_v1() -> Transaction {
    Transaction::new(
        1,
        vec![TransactionInput::new_with_compute_budget(
            TransactionOutpoint::new(Hash::from_bytes([0x11; 32]), 0),
            vec![0xaa, 0xbb],
            42,
            7,
        )],
        vec![TransactionOutput::with_covenant(
            100,
            ScriptPublicKey::new(0, vec![0x51].into()),
            Some(CovenantBinding::new(0, Hash::from_bytes([0x22; 32]))),
        )],
        123,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![0xcc],
    )
}

#[test]
fn json_transaction_deserialize_accepts_fields_before_version() {
    for tx in [sample_v0(), sample_v1()] {
        let value = serde_json::to_value(&tx).unwrap();
        let reordered = serde_json::json!({
            "inputs": value["inputs"],
            "outputs": value["outputs"],
            "lockTime": value["lockTime"],
            "version": value["version"],
            "subnetworkId": value["subnetworkId"],
            "gas": value["gas"],
            "payload": value["payload"],
            "mass": value["mass"],
            "id": value["id"],
        });
        let decoded: Transaction = serde_json::from_value(reordered).expect("field order must not affect transaction JSON");
        assert_eq!(decoded, tx);
    }
}
