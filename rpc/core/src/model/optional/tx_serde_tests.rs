//! Wire-format snapshot tests for the `Option<bytes-like>` fields on
//! `RpcOptional*` structs. These lock the JSON format so that refactors
//! (e.g. swapping `serde_nested_with` for handwritten helpers) cannot
//! silently change the on-wire representation seen by JSON-RPC clients.

use super::tx::{
    RpcOptionalTransaction, RpcOptionalTransactionInput, RpcOptionalTransactionInputVerboseData, RpcOptionalTransactionOutpoint,
    RpcOptionalTransactionVerboseData,
};
use crate::{RpcHash, RpcTransactionId};
use kaspa_consensus_core::{subnets::SubnetworkId, tx::TransactionId};

fn hash32(b: u8) -> RpcHash {
    RpcHash::from_bytes([b; 32])
}

fn tx_id(b: u8) -> RpcTransactionId {
    RpcTransactionId::from_bytes([b; 32])
}

fn outpoint_some() -> RpcOptionalTransactionOutpoint {
    RpcOptionalTransactionOutpoint { transaction_id: Some(TransactionId::from_bytes([0xAB; 32])), index: Some(7) }
}

fn outpoint_none() -> RpcOptionalTransactionOutpoint {
    RpcOptionalTransactionOutpoint { transaction_id: None, index: None }
}

fn verbose_data_some() -> RpcOptionalTransactionVerboseData {
    RpcOptionalTransactionVerboseData {
        transaction_id: Some(tx_id(0x11)),
        hash: Some(hash32(0x22)),
        compute_mass: Some(42),
        block_hash: Some(hash32(0x33)),
        block_time: Some(123456),
    }
}

fn verbose_data_none() -> RpcOptionalTransactionVerboseData {
    RpcOptionalTransactionVerboseData { transaction_id: None, hash: None, compute_mass: None, block_hash: None, block_time: None }
}

fn input_some() -> RpcOptionalTransactionInput {
    RpcOptionalTransactionInput {
        previous_outpoint: Some(outpoint_some()),
        signature_script: Some(vec![0x00, 0x01, 0x02, 0xff]),
        sequence: Some(99),
        sig_op_count: Some(1),
        compute_budget: Some(0),
        verbose_data: Some(RpcOptionalTransactionInputVerboseData { utxo_entry: None }),
    }
}

fn input_all_none() -> RpcOptionalTransactionInput {
    RpcOptionalTransactionInput {
        previous_outpoint: None,
        signature_script: None,
        sequence: None,
        sig_op_count: None,
        compute_budget: None,
        verbose_data: None,
    }
}

fn transaction_some() -> RpcOptionalTransaction {
    RpcOptionalTransaction {
        version: Some(1),
        inputs: vec![],
        outputs: vec![],
        lock_time: Some(0),
        subnetwork_id: Some(SubnetworkId::from_byte(0)),
        gas: Some(0),
        payload: Some(vec![0xde, 0xad, 0xbe, 0xef]),
        mass: Some(0),
        verbose_data: None,
    }
}

fn transaction_none_payload() -> RpcOptionalTransaction {
    RpcOptionalTransaction {
        version: None,
        inputs: vec![],
        outputs: vec![],
        lock_time: None,
        subnetwork_id: None,
        gas: None,
        payload: None,
        mass: None,
        verbose_data: None,
    }
}

#[test]
fn outpoint_json_some() {
    let json = serde_json::to_string(&outpoint_some()).unwrap();
    assert_eq!(json, r#"{"transactionId":"abababababababababababababababababababababababababababababababab","index":7}"#);
    let back: RpcOptionalTransactionOutpoint = serde_json::from_str(&json).unwrap();
    assert_eq!(back, outpoint_some());
}

#[test]
fn outpoint_json_none() {
    let json = serde_json::to_string(&outpoint_none()).unwrap();
    assert_eq!(json, r#"{"transactionId":null,"index":null}"#);
    let back: RpcOptionalTransactionOutpoint = serde_json::from_str(&json).unwrap();
    assert_eq!(back, outpoint_none());
}

#[test]
fn verbose_data_json_some() {
    let json = serde_json::to_string(&verbose_data_some()).unwrap();
    assert_eq!(
        json,
        concat!(
            r#"{"transactionId":"1111111111111111111111111111111111111111111111111111111111111111","#,
            r#""hash":"2222222222222222222222222222222222222222222222222222222222222222","#,
            r#""computeMass":42,"#,
            r#""blockHash":"3333333333333333333333333333333333333333333333333333333333333333","#,
            r#""blockTime":123456}"#,
        )
    );
    let back: RpcOptionalTransactionVerboseData = serde_json::from_str(&json).unwrap();
    let exp = verbose_data_some();
    assert_eq!(back.transaction_id, exp.transaction_id);
    assert_eq!(back.hash, exp.hash);
    assert_eq!(back.compute_mass, exp.compute_mass);
    assert_eq!(back.block_hash, exp.block_hash);
    assert_eq!(back.block_time, exp.block_time);
}

#[test]
fn verbose_data_json_none() {
    let json = serde_json::to_string(&verbose_data_none()).unwrap();
    assert_eq!(json, r#"{"transactionId":null,"hash":null,"computeMass":null,"blockHash":null,"blockTime":null}"#);
}

// `Option<Vec<u8>>` fields use `kaspa_utils::serde_bytes_optional`, matching
// the non-optional `RpcTransaction` (`payload`, `signature_script`) which
// serialize as hex strings. The previous `serde_nested_with` annotation was
// a silent no-op and produced JSON arrays of bytes — that was wrong; the
// expected shape on the wire is hex.

#[test]
fn input_json_signature_script_some() {
    let json = serde_json::to_string(&input_some()).unwrap();
    assert!(json.contains(r#""signatureScript":"000102ff""#), "got: {json}");
    let back: RpcOptionalTransactionInput = serde_json::from_str(&json).unwrap();
    assert_eq!(back.signature_script, Some(vec![0x00, 0x01, 0x02, 0xff]));
}

#[test]
fn input_json_signature_script_none() {
    let json = serde_json::to_string(&input_all_none()).unwrap();
    assert!(json.contains(r#""signatureScript":null"#), "got: {json}");
    let back: RpcOptionalTransactionInput = serde_json::from_str(&json).unwrap();
    assert_eq!(back.signature_script, None);
}

#[test]
fn transaction_json_payload_some() {
    let json = serde_json::to_string(&transaction_some()).unwrap();
    assert!(json.contains(r#""payload":"deadbeef""#), "got: {json}");
    let back: RpcOptionalTransaction = serde_json::from_str(&json).unwrap();
    assert_eq!(back.payload, Some(vec![0xde, 0xad, 0xbe, 0xef]));
}

#[test]
fn transaction_json_payload_none() {
    let json = serde_json::to_string(&transaction_none_payload()).unwrap();
    assert!(json.contains(r#""payload":null"#), "got: {json}");
    let back: RpcOptionalTransaction = serde_json::from_str(&json).unwrap();
    assert_eq!(back.payload, None);
}
