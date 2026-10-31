//! Transaction JSON deserialization must not depend on field order: serde
//! maps are unordered, and `serde_json::to_value` emits alphabetically sorted
//! keys, so `inputs`/`outputs` arrive before `version` on any `from_value`
//! roundtrip. Run with `--no-default-features` as well; both builds share
//! the union decoding path these tests exercise.

use kaspa_consensus_core::Hash;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{
    CovenantBinding, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput,
};
use serde_json::Value;

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

fn sample_v1_with_covenant(covenant: Option<CovenantBinding>) -> Transaction {
    Transaction::new(
        1,
        vec![TransactionInput::new_with_compute_budget(
            TransactionOutpoint::new(Hash::from_bytes([0x11; 32]), 0),
            vec![0xaa, 0xbb],
            42,
            7,
        )],
        vec![TransactionOutput::with_covenant(100, ScriptPublicKey::new(0, vec![0x51].into()), covenant)],
        123,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![0xcc],
    )
}

fn sample_v1() -> Transaction {
    sample_v1_with_covenant(Some(CovenantBinding::new(0, Hash::from_bytes([0x22; 32]))))
}

/// Reorders the serialized fields so `inputs`/`outputs` precede `version`.
fn inputs_first(value: &Value) -> Value {
    serde_json::json!({
        "inputs": value["inputs"],
        "outputs": value["outputs"],
        "lockTime": value["lockTime"],
        "version": value["version"],
        "subnetworkId": value["subnetworkId"],
        "gas": value["gas"],
        "payload": value["payload"],
        "mass": value["mass"],
        "id": value["id"],
    })
}

fn map_inputs(mut value: Value, f: impl Fn(&mut Value)) -> Value {
    for input in value["inputs"].as_array_mut().unwrap() {
        f(input);
    }
    value
}

fn map_outputs(mut value: Value, f: impl Fn(&mut Value)) -> Value {
    for output in value["outputs"].as_array_mut().unwrap() {
        f(output);
    }
    value
}

#[test]
fn json_transaction_deserialize_accepts_fields_before_version() {
    for tx in [sample_v0(), sample_v1()] {
        let reordered = inputs_first(&serde_json::to_value(&tx).unwrap());
        let decoded: Transaction = serde_json::from_value(reordered).expect("field order must not affect transaction JSON");
        assert_eq!(decoded, tx);
    }
}

#[test]
fn json_out_of_order_ignores_counterpart_version_fields() {
    // v0 elements carrying v1-only fields and vice versa decode like the
    // version-specific shapes, which ignore unknown fields
    let stray_covenant = serde_json::to_value(sample_v1()).unwrap()["outputs"][0]["covenant"].clone();

    let mut v0 = inputs_first(&serde_json::to_value(sample_v0()).unwrap());
    v0 = map_inputs(v0, |input| input["computeBudget"] = serde_json::json!(9));
    v0 = map_outputs(v0, |output| output["covenant"] = stray_covenant.clone());
    assert_eq!(serde_json::from_value::<Transaction>(v0).unwrap(), sample_v0());

    let v1 = inputs_first(&serde_json::to_value(sample_v1()).unwrap());
    let v1 = map_inputs(v1, |input| input["sigOpCount"] = serde_json::json!(5));
    assert_eq!(serde_json::from_value::<Transaction>(v1).unwrap(), sample_v1());
}

#[test]
fn json_out_of_order_v1_output_without_covenant_defaults_to_none() {
    let value = inputs_first(&serde_json::to_value(sample_v1()).unwrap());
    let value = map_outputs(value, |output| {
        output.as_object_mut().unwrap().remove("covenant");
    });
    let decoded: Transaction = serde_json::from_value(value).unwrap();
    // the id is content-derived and the JSON carries the covenant-bearing sample's id
    assert_eq!(decoded.outputs, sample_v1_with_covenant(None).outputs);
}

#[test]
fn json_out_of_order_reports_missing_version_specific_input_field() {
    let v0 = inputs_first(&serde_json::to_value(sample_v0()).unwrap());
    let v0 = map_inputs(v0, |input| {
        input.as_object_mut().unwrap().remove("sigOpCount");
    });
    let err = serde_json::from_value::<Transaction>(v0).unwrap_err();
    assert!(err.to_string().contains("sigOpCount"), "unexpected error: {err}");

    let v1 = inputs_first(&serde_json::to_value(sample_v1()).unwrap());
    let v1 = map_inputs(v1, |input| {
        input.as_object_mut().unwrap().remove("computeBudget");
    });
    let err = serde_json::from_value::<Transaction>(v1).unwrap_err();
    assert!(err.to_string().contains("computeBudget"), "unexpected error: {err}");
}
