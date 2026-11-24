//!
//! WASM bindings for KIP-21 sequencing commitments: verifying the witness returned by
//! `RpcClient.getSeqCommitLaneProof()` and recomputing lane keys and lane tips.
//!

use crate::model::*;
use kaspa_consensus_core::subnets::{SUBNETWORK_ID_SIZE, SubnetworkId};
use kaspa_rpc_macros::declare_typescript_wasm_interface as declare;
use kaspa_seq_commit::{
    hashing::{activity_digest_lane, activity_leaf, lane_key, lane_tip_next, mergeset_context_hash},
    types::{LaneTipInput, MergesetContext},
};
use kaspa_utils::hex::FromHex;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::from_value;
use std::{fmt::Display, str::FromStr};
use wasm_bindgen::prelude::*;
use workflow_wasm::serde::to_value;

#[cfg(all(test, target_arch = "wasm32"))]
mod tests;

fn js_error(err: impl Display) -> JsError {
    JsError::new(&err.to_string())
}

declare! {
    IVerifySeqCommitLaneProofArgs,
    r#"
    /**
     * @category Node RPC
     */
    export interface IVerifySeqCommitLaneProofArgs {
        /**
         * The response of `RpcClient.getSeqCommitLaneProof()`.
         */
        proof: IGetSeqCommitLaneProofResponse;
        /**
         * The lane key the proof was requested for.
         */
        laneKey: HexString;
        /**
         * The `acceptedIdMerkleRoot` (`seq_commit`) of the header of the block the proof was
         * requested for. The proof is only as trustworthy as this header.
         */
        seqCommit: HexString;
        /**
         * Optional: the selected parent header's `acceptedIdMerkleRoot`. When given, the proof
         * must also name it as its parent commitment.
         */
        parentSeqCommit?: HexString;
    }
    "#,
}

declare! {
    IVerifiedSeqCommitLane,
    r#"
    /**
     * @category Node RPC
     */
    export interface IVerifiedSeqCommitLane {
        /**
         * Whether the lane has an entry in the block's active-lanes tree, i.e. it was updated
         * within the finality window. It does not mean the block accepted transactions of the
         * lane: the tip only advances in blocks that do.
         */
        active: boolean;
        /**
         * The lane's verified entry, when `active`. `blueScore` is the blue score of the block
         * that last advanced the lane's tip (the queried block's own blue score only if that
         * block accepted transactions of the lane).
         */
        lane?: IRpcLaneEntry;
    }
    "#,
}

declare! {
    ISeqCommitLaneTipArgs,
    r#"
    /**
     * Inputs for advancing a lane tip over one chain block, as consensus does.
     *
     * @category Node RPC
     */
    export interface ISeqCommitLaneTipArgs {
        /**
         * The lane's previous tip if its entry is still live from this block's point of view,
         * otherwise the selected parent's `acceptedIdMerkleRoot` (new or expired lane).
         *
         * Taking the entry from the selected parent's verified proof, it is live when
         * `lane.blueScore >= blueScore - finalityDepth` (this block's blue score, and the
         * network's finality depth in blocks). An entry the parent still shows can already be
         * expired here, in which case the parent's `acceptedIdMerkleRoot` must be used.
         */
        parentRef: HexString;
        laneKey: HexString;
        /**
         * All of the lane's transactions accepted by the chain block, in acceptance order.
         */
        transactions: ISeqCommitLaneTransaction[];
        /**
         * The selected parent header's timestamp.
         */
        timestamp: bigint;
        /**
         * The chain block's DAA score.
         */
        daaScore: bigint;
        /**
         * The chain block's blue score.
         */
        blueScore: bigint;
    }

    /**
     * @category Node RPC
     */
    export interface ISeqCommitLaneTransaction {
        transactionId: HexString;
        version: number;
        /**
         * The transaction's position among all transactions accepted by the chain block
         * (across every lane), counted from 0: its index in the block's
         * `acceptedTransactions` as returned by `RpcClient.getVirtualChainFromBlockV2()`
         * (`chainBlockAcceptedTransactions`), which lists them in acceptance order.
         */
        mergeIndex: number;
    }
    "#,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerifyArgs {
    proof: ProofArgs,
    lane_key: RpcHash,
    seq_commit: RpcHash,
    parent_seq_commit: Option<RpcHash>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProofArgs {
    smt_proof: String,
    lane: Option<RpcLaneEntry>,
    payload_and_ctx_digest: RpcHash,
    parent_seq_commit: RpcHash,
    inactivity_shortcut: RpcHash,
}

#[derive(Serialize)]
struct Verified<'a> {
    active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    lane: Option<&'a RpcLaneEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaneTipArgs {
    parent_ref: RpcHash,
    lane_key: RpcHash,
    transactions: Vec<LaneTransaction>,
    timestamp: u64,
    daa_score: u64,
    blue_score: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaneTransaction {
    transaction_id: RpcHash,
    version: u16,
    merge_index: u32,
}

/// Verifies a KIP-21 lane proof returned by `RpcClient.getSeqCommitLaneProof()` against the
/// `acceptedIdMerkleRoot` (`seq_commit`) of the block's header.
///
/// Returns `{ active: true, lane }` when the proof shows the lane in the block's active-lanes tree
/// (updated within the finality window, not necessarily by this block), and `{ active: false }`
/// when it proves the lane has no entry there. Throws if the proof does not match the header.
///
/// @category Node RPC
#[wasm_bindgen(js_name = verifySeqCommitLaneProof)]
pub fn js_verify_seq_commit_lane_proof(args: IVerifySeqCommitLaneProofArgs) -> Result<IVerifiedSeqCommitLane, JsError> {
    let VerifyArgs { proof, lane_key, seq_commit, parent_seq_commit } = from_value(args.into()).map_err(js_error)?;
    let response = GetSeqCommitLaneProofResponse {
        smt_proof: Vec::<u8>::from_hex(&proof.smt_proof).map_err(|_| JsError::new("smtProof must be a hex string"))?,
        lane: proof.lane,
        payload_and_ctx_digest: proof.payload_and_ctx_digest,
        parent_seq_commit: proof.parent_seq_commit,
        inactivity_shortcut: proof.inactivity_shortcut,
        header: None,
    };
    let lane = response.verify(&lane_key, &seq_commit, parent_seq_commit.as_ref()).map_err(js_error)?;
    Ok(to_value(&Verified { active: lane.is_some(), lane }).map_err(js_error)?.into())
}

/// Returns the KIP-21 lane key of a subnetwork: its key in the active-lanes tree, as used by
/// `RpcClient.getSeqCommitLaneProof()`.
///
/// @category Node RPC
#[wasm_bindgen(js_name = seqCommitLaneKey)]
pub fn js_seq_commit_lane_key(subnetwork_id: &str) -> Result<String, JsError> {
    let subnetwork_id = SubnetworkId::from_str(subnetwork_id).map_err(js_error)?;
    let lane_id: &[u8; SUBNETWORK_ID_SIZE] = subnetwork_id.as_ref();
    Ok(lane_key(lane_id).to_string())
}

/// Advances a KIP-21 lane tip over one chain block from the lane's transactions accepted by that
/// block, as consensus does. Chaining this over the blocks where a lane was active lets a client
/// check that a set of transactions is exactly the lane's activity, by comparing the result with a
/// verified `lane.tip` (see `verifySeqCommitLaneProof()`).
///
/// @category Node RPC
#[wasm_bindgen(js_name = seqCommitLaneTipNext)]
pub fn js_seq_commit_lane_tip_next(args: ISeqCommitLaneTipArgs) -> Result<String, JsError> {
    let args: LaneTipArgs = from_value(args.into()).map_err(js_error)?;
    if args.transactions.is_empty() {
        return Err(JsError::new("a lane tip only advances in a block that accepts transactions of the lane"));
    }
    if args.transactions.windows(2).any(|pair| pair[0].merge_index >= pair[1].merge_index) {
        return Err(JsError::new("transactions must be in acceptance order (strictly increasing mergeIndex)"));
    }
    let activity_digest =
        activity_digest_lane(args.transactions.iter().map(|tx| activity_leaf(&tx.transaction_id, tx.version, tx.merge_index)));
    let context_hash =
        mergeset_context_hash(&MergesetContext { timestamp: args.timestamp, daa_score: args.daa_score, blue_score: args.blue_score });
    let tip = lane_tip_next(&LaneTipInput {
        parent_ref: &args.parent_ref,
        lane_key: &args.lane_key,
        activity_digest: &activity_digest,
        context_hash: &context_hash,
    });
    Ok(tip.to_string())
}
