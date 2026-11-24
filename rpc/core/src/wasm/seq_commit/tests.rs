use super::*;
use crate::wasm::message::IGetSeqCommitLaneProofResponse;
use js_sys::{Object, Reflect};
use kaspa_consensus_core::{
    BlueWorkType,
    header::{CompressedParents, Header},
};
use kaspa_hashes::{Hash, HasherBase, SeqCommitActiveNode, SeqCommitMerkleBranch};
use kaspa_seq_commit::{
    hashing::{activity_root_hash, seq_state_root, smt_leaf_hash},
    types::{SeqState, SmtLeafInput},
};
use kaspa_smt::tree::SparseMerkleTree;
use wasm_bindgen_test::wasm_bindgen_test;
use workflow_wasm::extensions::ObjectExtension;

/// Three active lanes with tips derived from `salt`; the JS response for `queried`, with a header committing to it.
fn response_with(queried: u8, salt: u8) -> (JsValue, Hash, Header) {
    let entries: Vec<(Hash, RpcLaneEntry)> = [1u8, 2, 3]
        .iter()
        .map(|&b| (lane_key(&[b; 20]), RpcLaneEntry { tip: Hash::from_bytes([b ^ salt; 32]), blue_score: 1000 + b as u64 }))
        .collect();
    let mut tree = SparseMerkleTree::<SeqCommitActiveNode>::new();
    for (key, entry) in &entries {
        tree.insert(*key, smt_leaf_hash(&SmtLeafInput { lane_tip: &entry.tip, blue_score: entry.blue_score }));
    }
    let key = lane_key(&[queried; 20]);
    let payload_and_ctx_digest = Hash::from_bytes([0xd0; 32]);
    let parent_seq_commit = Hash::from_bytes([0xd1; 32]);
    let inactivity_shortcut = Hash::from_bytes([0xd2; 32]);
    let activity_root = activity_root_hash(&inactivity_shortcut, &tree.root());
    let state_root = seq_state_root(&SeqState { activity_root: &activity_root, payload_and_ctx_digest: &payload_and_ctx_digest });
    let mut hasher = SeqCommitMerkleBranch::new();
    hasher.update(parent_seq_commit).update(state_root);
    let header = Header::new_finalized(
        1,
        CompressedParents::try_from(vec![vec![Hash::from_bytes([0xa0; 32])]]).unwrap(),
        Hash::from_bytes([0xa1; 32]),
        hasher.finalize(),
        Hash::from_bytes([0xa2; 32]),
        1_700_000_000_000,
        0x1e7fffff,
        7,
        500,
        BlueWorkType::from_u64(12345),
        400,
        Hash::from_bytes([0xa3; 32]),
    );
    let response = GetSeqCommitLaneProofResponse {
        smt_proof: tree.prove(&key).unwrap().to_bytes(),
        lane: entries.iter().find(|(k, _)| *k == key).map(|(_, entry)| entry.clone()),
        payload_and_ctx_digest,
        parent_seq_commit,
        inactivity_shortcut,
        header: Some((&header).into()),
    };
    (JsValue::from(IGetSeqCommitLaneProofResponse::try_from(response).unwrap()), key, header)
}

fn response_for(queried: u8) -> (JsValue, Hash, Header) {
    response_with(queried, 0)
}

fn verify(proof: &JsValue, key: Hash, args: &[(&str, Hash)]) -> Result<JsValue, JsError> {
    let object = Object::new();
    object.set("proof", proof).unwrap();
    object.set("laneKey", &JsValue::from_str(&key.to_string())).unwrap();
    for (name, value) in args {
        object.set(name, &JsValue::from_str(&value.to_string())).unwrap();
    }
    js_verify_seq_commit_lane_proof(IVerifySeqCommitLaneProofArgs::from(JsValue::from(object))).map(JsValue::from)
}

#[wasm_bindgen_test]
fn verifies_active_and_absent_lanes_against_the_header() {
    let (proof, key, header) = response_for(2);
    let lane = verify(&proof, key, &[("blockHash", header.hash)]).ok().unwrap();
    assert_eq!(Reflect::get(&lane, &"tip".into()).unwrap().as_string(), Some(Hash::from_bytes([2; 32]).to_string()));
    assert_eq!(Reflect::get(&lane, &"blueScore".into()).unwrap(), JsValue::from(js_sys::BigInt::from(1002u64)));

    let (proof, key, header) = response_for(9);
    assert!(verify(&proof, key, &[("blockHash", header.hash)]).ok().unwrap().is_undefined());
}

#[wasm_bindgen_test]
fn block_hash_is_required() {
    let (proof, key, header) = response_for(2);
    assert!(verify(&proof, key, &[]).is_err());
    assert!(verify(&proof, key, &[("seqCommit", header.accepted_id_merkle_root)]).is_err());
}

#[wasm_bindgen_test]
fn rejects_a_header_from_another_block() {
    let (proof, key, _) = response_for(2);
    assert!(verify(&proof, key, &[("blockHash", Hash::from_bytes([0xee; 32]))]).is_err());
}

#[wasm_bindgen_test]
fn rejects_a_forged_self_consistent_response() {
    let (_, key, real) = response_for(2);
    let (forged, _, forged_header) = response_with(2, 0x40);
    assert!(verify(&forged, key, &[("blockHash", forged_header.hash)]).is_ok());
    assert!(verify(&forged, key, &[("blockHash", real.hash)]).is_err());
    // The header's `hash` field is not trusted.
    let header = Reflect::get(&forged, &"header".into()).unwrap();
    Reflect::set(&header, &"hash".into(), &JsValue::from_str(&real.hash.to_string())).unwrap();
    assert!(verify(&forged, key, &[("blockHash", real.hash)]).is_err());
}

#[wasm_bindgen_test]
fn seq_commit_with_a_header_must_match_it() {
    let (proof, key, header) = response_for(2);
    assert!(verify(&proof, key, &[("blockHash", header.hash), ("seqCommit", header.accepted_id_merkle_root)]).is_ok());
    assert!(verify(&proof, key, &[("blockHash", header.hash), ("seqCommit", Hash::from_bytes([0xee; 32]))]).is_err());
    // A matching seqCommit does not bypass the blockHash check.
    let (forged, _, forged_header) = response_with(2, 0x40);
    assert!(verify(&forged, key, &[("blockHash", header.hash), ("seqCommit", forged_header.accepted_id_merkle_root)]).is_err());
}

#[wasm_bindgen_test]
fn seq_commit_replaces_a_missing_header() {
    let (proof, key, header) = response_for(2);
    Reflect::delete_property(&Object::from(proof.clone()), &"header".into()).unwrap();
    assert!(verify(&proof, key, &[("blockHash", header.hash)]).is_err());
    assert!(verify(&proof, key, &[("blockHash", header.hash), ("seqCommit", header.accepted_id_merkle_root)]).is_ok());
    assert!(verify(&proof, key, &[("blockHash", header.hash), ("seqCommit", Hash::from_bytes([0xee; 32]))]).is_err());
}

#[wasm_bindgen_test]
fn rejects_altered_proofs() {
    let (proof, key, header) = response_for(2);
    let at = [("blockHash", header.hash)];
    assert!(verify(&proof, lane_key(&[3; 20]), &at).is_err());

    let lane = Reflect::get(&proof, &"lane".into()).unwrap();
    Reflect::set(&lane, &"tip".into(), &JsValue::from_str(&Hash::from_bytes([7; 32]).to_string())).unwrap();
    assert!(verify(&proof, key, &at).is_err());

    // An active lane cannot pass as absent.
    let (proof, key, _) = response_for(2);
    Reflect::delete_property(&Object::from(proof.clone()), &"lane".into()).unwrap();
    assert!(verify(&proof, key, &at).is_err());

    let (proof, key, _) = response_for(2);
    Reflect::set(&proof, &"smtProof".into(), &js_sys::Array::new()).unwrap();
    assert!(verify(&proof, key, &at).is_err());
}

#[wasm_bindgen_test]
fn checks_the_parent_commitment_when_given() {
    let (proof, key, header) = response_for(2);
    assert!(verify(&proof, key, &[("blockHash", header.hash), ("parentSeqCommit", Hash::from_bytes([0xd1; 32]))]).is_ok());
    assert!(verify(&proof, key, &[("blockHash", header.hash), ("parentSeqCommit", Hash::from_bytes([0xee; 32]))]).is_err());
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
