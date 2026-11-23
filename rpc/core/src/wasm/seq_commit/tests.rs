use super::*;
use crate::wasm::message::IGetSeqCommitLaneProofResponse;
use js_sys::{Object, Reflect};
use kaspa_hashes::{Hash, HasherBase, SeqCommitActiveNode, SeqCommitMerkleBranch};
use kaspa_seq_commit::{
    hashing::{activity_root_hash, seq_state_root, smt_leaf_hash},
    types::{SeqState, SmtLeafInput},
};
use kaspa_smt::tree::SparseMerkleTree;
use wasm_bindgen_test::wasm_bindgen_test;
use workflow_wasm::extensions::ObjectExtension;

/// A lanes tree with three active lanes; returns the JS proof for `queried`, its lane key and the block's `seq_commit`.
fn proof_and_commit(queried: u8) -> (JsValue, Hash, Hash) {
    let entries: Vec<(Hash, RpcLaneEntry)> = [1u8, 2, 3]
        .iter()
        .map(|&b| (lane_key(&[b; 20]), RpcLaneEntry { tip: Hash::from_bytes([b; 32]), blue_score: 1000 + b as u64 }))
        .collect();
    let mut tree = SparseMerkleTree::<SeqCommitActiveNode>::new();
    for (key, entry) in &entries {
        tree.insert(*key, smt_leaf_hash(&SmtLeafInput { lane_tip: &entry.tip, blue_score: entry.blue_score }));
    }
    let key = lane_key(&[queried; 20]);
    let response = GetSeqCommitLaneProofResponse {
        smt_proof: tree.prove(&key).unwrap().to_bytes(),
        lane: entries.iter().find(|(k, _)| *k == key).map(|(_, entry)| entry.clone()),
        payload_and_ctx_digest: Hash::from_bytes([0xd0; 32]),
        parent_seq_commit: Hash::from_bytes([0xd1; 32]),
        inactivity_shortcut: Hash::from_bytes([0xd2; 32]),
    };
    let activity_root = activity_root_hash(&response.inactivity_shortcut, &tree.root());
    let state_root =
        seq_state_root(&SeqState { activity_root: &activity_root, payload_and_ctx_digest: &response.payload_and_ctx_digest });
    let mut hasher = SeqCommitMerkleBranch::new();
    hasher.update(response.parent_seq_commit).update(state_root);
    let proof = JsValue::from(IGetSeqCommitLaneProofResponse::try_from(response).unwrap());
    (proof, key, hasher.finalize())
}

fn verify_args(proof: &JsValue, key: Hash, seq_commit: Hash) -> IVerifySeqCommitLaneProofArgs {
    let args = Object::new();
    args.set("proof", proof).unwrap();
    args.set("laneKey", &JsValue::from_str(&key.to_string())).unwrap();
    args.set("seqCommit", &JsValue::from_str(&seq_commit.to_string())).unwrap();
    IVerifySeqCommitLaneProofArgs::from(JsValue::from(args))
}

#[wasm_bindgen_test]
fn verifies_active_and_absent_lanes() {
    let (proof, key, seq_commit) = proof_and_commit(2);
    let verified = JsValue::from(js_verify_seq_commit_lane_proof(verify_args(&proof, key, seq_commit)).ok().unwrap());
    assert_eq!(Reflect::get(&verified, &"active".into()).unwrap(), JsValue::TRUE);
    let lane = Reflect::get(&verified, &"lane".into()).unwrap();
    assert_eq!(Reflect::get(&lane, &"tip".into()).unwrap().as_string(), Some(Hash::from_bytes([2; 32]).to_string()));
    assert_eq!(Reflect::get(&lane, &"blueScore".into()).unwrap(), JsValue::from(js_sys::BigInt::from(1002u64)));

    let (proof, key, seq_commit) = proof_and_commit(9);
    let verified = JsValue::from(js_verify_seq_commit_lane_proof(verify_args(&proof, key, seq_commit)).ok().unwrap());
    assert_eq!(Reflect::get(&verified, &"active".into()).unwrap(), JsValue::FALSE);
    assert!(!Reflect::has(&verified, &"lane".into()).unwrap());
}

#[wasm_bindgen_test]
fn rejects_proofs_that_do_not_match_the_header() {
    let (proof, key, seq_commit) = proof_and_commit(2);
    assert!(js_verify_seq_commit_lane_proof(verify_args(&proof, key, Hash::from_bytes([0xee; 32]))).is_err());
    assert!(js_verify_seq_commit_lane_proof(verify_args(&proof, lane_key(&[3; 20]), seq_commit)).is_err());

    let lane = Reflect::get(&proof, &"lane".into()).unwrap();
    Reflect::set(&lane, &"tip".into(), &JsValue::from_str(&Hash::from_bytes([7; 32]).to_string())).unwrap();
    assert!(js_verify_seq_commit_lane_proof(verify_args(&proof, key, seq_commit)).is_err());

    // Hiding an active lane does not pass as a non-inclusion proof.
    let (proof, key, seq_commit) = proof_and_commit(2);
    Reflect::delete_property(&Object::from(proof.clone()), &"lane".into()).unwrap();
    assert!(js_verify_seq_commit_lane_proof(verify_args(&proof, key, seq_commit)).is_err());

    let (proof, key, seq_commit) = proof_and_commit(2);
    Reflect::set(&proof, &"smtProof".into(), &JsValue::from_str("zz")).unwrap();
    assert!(js_verify_seq_commit_lane_proof(verify_args(&proof, key, seq_commit)).is_err());
}

#[wasm_bindgen_test]
fn checks_the_parent_commitment_when_given() {
    let (proof, key, seq_commit) = proof_and_commit(2);
    let with_parent = |parent: Hash| {
        let args = JsValue::from(verify_args(&proof, key, seq_commit));
        Reflect::set(&args, &"parentSeqCommit".into(), &JsValue::from_str(&parent.to_string())).unwrap();
        js_verify_seq_commit_lane_proof(IVerifySeqCommitLaneProofArgs::from(args))
    };
    assert!(with_parent(Hash::from_bytes([0xd1; 32])).is_ok());
    assert!(with_parent(Hash::from_bytes([0xee; 32])).is_err());
}

#[wasm_bindgen_test]
fn lane_key_matches_consensus() {
    let golden = Hash::from_bytes([
        0x57, 0xc7, 0xe5, 0x2c, 0x76, 0x02, 0xb3, 0x66, 0xb3, 0xf6, 0x62, 0xad, 0xdc, 0x36, 0x12, 0x96, 0x77, 0xd4, 0x84, 0x4b, 0x84,
        0x04, 0x68, 0xcc, 0xaa, 0x96, 0x31, 0x10, 0x6b, 0xea, 0x88, 0x97,
    ]);
    assert_eq!(js_seq_commit_lane_key(&"42".repeat(20)).ok(), Some(golden.to_string()));
    assert!(js_seq_commit_lane_key("42").is_err());
}

#[wasm_bindgen_test]
fn lane_tip_matches_consensus() {
    let parent_ref = Hash::from_bytes([1; 32]);
    let key = lane_key(&[5; 20]);
    let tx = |id: u8, merge_index: u32| {
        let tx = Object::new();
        tx.set("transactionId", &JsValue::from_str(&Hash::from_bytes([id; 32]).to_string())).unwrap();
        tx.set("version", &JsValue::from(1)).unwrap();
        tx.set("mergeIndex", &JsValue::from(merge_index)).unwrap();
        JsValue::from(tx)
    };
    let args = |transactions: js_sys::Array| {
        let args = Object::new();
        args.set("parentRef", &JsValue::from_str(&parent_ref.to_string())).unwrap();
        args.set("laneKey", &JsValue::from_str(&key.to_string())).unwrap();
        args.set("transactions", &transactions).unwrap();
        args.set("timestamp", &js_sys::BigInt::from(1_700_000_000_000u64)).unwrap();
        args.set("daaScore", &js_sys::BigInt::from(500u64)).unwrap();
        args.set("blueScore", &js_sys::BigInt::from(400u64)).unwrap();
        ISeqCommitLaneTipArgs::from(JsValue::from(args))
    };

    let expected = lane_tip_next(&LaneTipInput {
        parent_ref: &parent_ref,
        lane_key: &key,
        activity_digest: &activity_digest_lane(
            [activity_leaf(&Hash::from_bytes([8; 32]), 1, 3), activity_leaf(&Hash::from_bytes([9; 32]), 1, 7)].into_iter(),
        ),
        context_hash: &mergeset_context_hash(&MergesetContext { timestamp: 1_700_000_000_000, daa_score: 500, blue_score: 400 }),
    });
    assert_eq!(js_seq_commit_lane_tip_next(args(js_sys::Array::of2(&tx(8, 3), &tx(9, 7)))).ok(), Some(expected.to_string()));
    assert!(js_seq_commit_lane_tip_next(args(js_sys::Array::of2(&tx(9, 7), &tx(8, 3)))).is_err());
    assert!(js_seq_commit_lane_tip_next(args(js_sys::Array::new())).is_err());
}
