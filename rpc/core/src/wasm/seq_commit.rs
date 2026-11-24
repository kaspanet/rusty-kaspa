//!
//! WASM bindings for KIP-21 lane proofs, lane keys and lane tips.
//!

use crate::model::{
    lane_proof::{verify_seq_commit_lane_proof, verify_seq_commit_lane_proof_at_block},
    *,
};
use kaspa_consensus_core::subnets::{SUBNETWORK_ID_SIZE, SubnetworkId};
use kaspa_rpc_macros::declare_typescript_wasm_interface as declare;
use kaspa_seq_commit::{
    hashing::{activity_digest_lane, activity_leaf, lane_key, lane_tip_next, mergeset_context_hash},
    types::{LaneTipInput, MergesetContext},
};
use serde::Deserialize;
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
         * Response of `RpcClient.getSeqCommitLaneProof()`.
         */
        proof: IGetSeqCommitLaneProofResponse;
        laneKey: HexString;
        /**
         * The requested block, from a trusted source; `proof.header` must hash to it
         * (its `hash` field is not trusted).
         */
        blockHash: HexString;
        /**
         * Required if `proof.header` is absent (older nodes): the `acceptedIdMerkleRoot` of
         * `blockHash`'s header, from a trusted source. Otherwise it must equal the header's.
         */
        seqCommit?: HexString;
        /**
         * If given, the selected parent's `acceptedIdMerkleRoot` must match `proof.parentSeqCommit`.
         */
        parentSeqCommit?: HexString;
    }
    "#,
}

declare! {
    ISeqCommitLaneTipArgs,
    r#"
    /**
     * @category Consensus
     */
    export interface ISeqCommitLaneTipArgs {
        /**
         * The lane's previous tip if live (`lane.blueScore >= blueScore - finalityDepth`),
         * otherwise the selected parent's `acceptedIdMerkleRoot`.
         */
        parentRef: HexString;
        laneKey: HexString;
        /**
         * The lane's transactions accepted by the block, in acceptance order.
         */
        transactions: ISeqCommitLaneTransaction[];
        /**
         * The selected parent's timestamp.
         */
        timestamp: bigint;
        /**
         * This block's DAA score.
         */
        daaScore: bigint;
        /**
         * This block's blue score.
         */
        blueScore: bigint;
    }

    /**
     * @category Consensus
     */
    export interface ISeqCommitLaneTransaction {
        transactionId: HexString;
        version: number;
        /**
         * Index in the block's `acceptedTransactions` from `getVirtualChainFromBlockV2()` (all lanes).
         */
        mergeIndex: number;
    }
    "#,
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "IRpcLaneEntry | undefined")]
    pub type IRpcLaneEntryOrUndefined;
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerifyArgs {
    proof: GetSeqCommitLaneProofResponse,
    lane_key: RpcHash,
    block_hash: RpcHash,
    seq_commit: Option<RpcHash>,
    parent_seq_commit: Option<RpcHash>,
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

/// Verifies a `getSeqCommitLaneProof()` response. Returns the lane's entry, or `undefined`
/// if the proof shows the lane has no entry at the block. Throws if the proof is invalid.
///
/// @category Node RPC
#[wasm_bindgen(js_name = verifySeqCommitLaneProof)]
pub fn js_verify_seq_commit_lane_proof(args: IVerifySeqCommitLaneProofArgs) -> Result<IRpcLaneEntryOrUndefined, JsError> {
    let VerifyArgs { proof, lane_key, block_hash, seq_commit, parent_seq_commit } = from_value(args.into()).map_err(js_error)?;
    let lane = match (&proof.header, seq_commit) {
        (Some(header), Some(seq_commit)) if header.accepted_id_merkle_root != seq_commit => {
            return Err(JsError::new("seqCommit does not match proof.header"));
        }
        (Some(_), _) => {
            verify_seq_commit_lane_proof_at_block(&proof, &lane_key, &block_hash, parent_seq_commit.as_ref()).map_err(js_error)?
        }
        (None, Some(seq_commit)) => {
            verify_seq_commit_lane_proof(&proof, &lane_key, &seq_commit, parent_seq_commit.as_ref()).map_err(js_error)?
        }
        (None, None) => return Err(JsError::new("seqCommit is required when the proof has no header")),
    };
    Ok(match lane {
        Some(lane) => to_value(lane).map_err(js_error)?,
        None => JsValue::UNDEFINED,
    }
    .into())
}

/// Returns the KIP-21 lane key of a subnetwork.
///
/// @category Consensus
#[wasm_bindgen(js_name = seqCommitLaneKey)]
pub fn js_seq_commit_lane_key(subnetwork_id: &str) -> Result<String, JsError> {
    let subnetwork_id = SubnetworkId::from_str(subnetwork_id).map_err(js_error)?;
    let lane_id: &[u8; SUBNETWORK_ID_SIZE] = subnetwork_id.as_ref();
    Ok(lane_key(lane_id).to_string())
}

/// Computes a lane's next tip over one selected-chain block that accepted transactions of the lane.
///
/// @category Consensus
#[wasm_bindgen(js_name = seqCommitLaneTipNext)]
pub fn js_seq_commit_lane_tip_next(args: ISeqCommitLaneTipArgs) -> Result<String, JsError> {
    let args: LaneTipArgs = from_value(args.into()).map_err(js_error)?;
    if args.transactions.is_empty() {
        return Err(JsError::new("transactions must not be empty"));
    }
    if args.transactions.windows(2).any(|pair| pair[0].merge_index >= pair[1].merge_index) {
        return Err(JsError::new("transactions must be in increasing mergeIndex order"));
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
