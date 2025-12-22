use std::str::FromStr;

use itertools::Itertools;
use kaspa_consensus::config::genesis::GENESIS;
use kaspa_consensus::params::{BlockrateParams, ForkActivation, Params, MAINNET_PARAMS, MEDIAN_TIME_SAMPLED_WINDOW_SIZE};
use kaspa_consensus::params::{MAX_DIFFICULTY_TARGET, MAX_DIFFICULTY_TARGET_AS_F64};
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::constants::STORAGE_MASS_PARAMETER;
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::network::NetworkType::Mainnet;
use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_consensus_core::trusted::{ExternalGhostdagData, TrustedBlock};
use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_consensus_core::{BlockHashMap, BlueWorkType};
use kaspa_hashes::Hash;
use kaspa_math::Uint256;
use kaspa_utils::hex::ToHex;
use serde::{Deserialize, Serialize};

use kaspa_consensus_core::KType;

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct RPCBlock {
    pub Header: RPCBlockHeader,
    pub Transactions: Vec<RPCTransaction>,
    pub VerboseData: RPCBlockVerboseData,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct RPCTransaction {
    pub Version: u16,
    pub Inputs: Vec<RPCTransactionInput>,
    pub Outputs: Vec<RPCTransactionOutput>,
    pub LockTime: u64,
    pub SubnetworkID: String,
    pub Gas: u64,
    pub Payload: String,

    #[serde(default)]
    pub Mass: u64,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct RPCTransactionOutput {
    pub Amount: u64,
    pub ScriptPublicKey: RPCScriptPublicKey,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct RPCScriptPublicKey {
    pub Version: u16,
    pub Script: String,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct RPCTransactionInput {
    pub PreviousOutpoint: RPCOutpoint,
    pub SignatureScript: String,
    pub Sequence: u64,
    pub SigOpCount: u8,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct RPCOutpoint {
    pub TransactionID: String,
    pub Index: u32,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct RPCBlockHeader {
    pub Version: u16,
    pub Parents: Vec<RPCBlockLevelParents>,
    pub HashMerkleRoot: String,
    pub AcceptedIDMerkleRoot: String,
    pub UTXOCommitment: String,
    pub Timestamp: u64,
    pub Bits: u32,
    pub Nonce: u64,
    pub DAAScore: u64,
    pub BlueScore: u64,
    pub BlueWork: String,
    pub PruningPoint: String,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct RPCBlockLevelParents {
    pub ParentHashes: Vec<String>,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct RPCBlockVerboseData {
    pub Hash: String,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct JsonBlockWithTrustedData {
    pub Block: RPCBlock,
    pub GHOSTDAG: JsonGHOSTDAGData,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct JsonGHOSTDAGData {
    pub BlueScore: u64,
    pub BlueWork: String,
    pub SelectedParent: String,
    pub MergeSetBlues: Vec<String>,
    pub MergeSetReds: Vec<String>,
    pub BluesAnticoneSizes: Vec<JsonBluesAnticoneSizes>,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct JsonBluesAnticoneSizes {
    pub BlueHash: String,
    pub AnticoneSize: KType,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct JsonOutpointUTXOEntryPair {
    pub Outpoint: RPCOutpoint,
    pub UTXOEntry: RPCUTXOEntry,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct RPCUTXOEntry {
    pub Amount: u64,
    pub ScriptPublicKey: RPCScriptPublicKey,
    pub BlockDAAScore: u64,
    pub IsCoinbase: bool,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct KaspadGoParams {
    pub K: KType,
    pub TimestampDeviationTolerance: u64,
    pub TargetTimePerBlock: u64,
    pub MaxBlockParents: u8,
    pub DifficultyAdjustmentWindowSize: usize,
    pub MergeSetSizeLimit: u64,
    pub MergeDepth: u64,
    pub FinalityDuration: u64,
    pub CoinbasePayloadScriptPublicKeyMaxLength: u8,
    pub MaxCoinbasePayloadLength: usize,
    pub MassPerTxByte: u64,
    pub MassPerSigOp: u64,
    pub MassPerScriptPubKeyByte: u64,
    pub MaxBlockMass: u64,
    pub DeflationaryPhaseDaaScore: u64,
    pub PreDeflationaryPhaseBaseSubsidy: u64,
    pub SkipProofOfWork: bool,
    pub MaxBlockLevel: u8,
    pub PruningProofM: u64,
    pub BlockrateParams: Option<BlockrateParams>,
    pub CrescendoActivation: Option<ForkActivation>,
    pub storage_mass_parameter: Option<u64>,
    pub max_difficulty_target: Option<Uint256>,
    pub max_difficulty_target_f64: Option<f64>,
    pub past_median_time_window_size: Option<u64>,
}

impl KaspadGoParams {
    pub fn into_params(self) -> Params {
        Params {
            dns_seeders: &[],
            net: NetworkId { network_type: Mainnet, suffix: None },
            genesis: GENESIS,
            timestamp_deviation_tolerance: self.TimestampDeviationTolerance,
            pre_crescendo_target_time_per_block: self.TargetTimePerBlock / 1_000_000,
            max_difficulty_target: self.max_difficulty_target.unwrap_or(MAX_DIFFICULTY_TARGET),
            max_difficulty_target_f64: self.max_difficulty_target_f64.unwrap_or(MAX_DIFFICULTY_TARGET_AS_F64),
            difficulty_window_size: self.DifficultyAdjustmentWindowSize as u64,
            past_median_time_window_size: self.past_median_time_window_size.unwrap_or(MEDIAN_TIME_SAMPLED_WINDOW_SIZE),
            min_difficulty_window_size: self.DifficultyAdjustmentWindowSize,
            coinbase_payload_script_public_key_max_len: self.CoinbasePayloadScriptPublicKeyMaxLength,
            max_coinbase_payload_len: self.MaxCoinbasePayloadLength,
            max_tx_inputs: MAINNET_PARAMS.max_tx_inputs,
            max_tx_outputs: MAINNET_PARAMS.max_tx_outputs,
            max_signature_script_len: MAINNET_PARAMS.max_signature_script_len,
            max_script_public_key_len: MAINNET_PARAMS.max_script_public_key_len,
            mass_per_tx_byte: self.MassPerTxByte,
            mass_per_script_pub_key_byte: self.MassPerScriptPubKeyByte,
            mass_per_sig_op: self.MassPerSigOp,
            max_block_mass: self.MaxBlockMass,
            storage_mass_parameter: self.storage_mass_parameter.unwrap_or(STORAGE_MASS_PARAMETER),
            deflationary_phase_daa_score: self.DeflationaryPhaseDaaScore,
            pre_deflationary_phase_base_subsidy: self.PreDeflationaryPhaseBaseSubsidy,
            skip_proof_of_work: self.SkipProofOfWork,
            max_block_level: self.MaxBlockLevel,
            pruning_proof_m: self.PruningProofM,
            blockrate: self.BlockrateParams.unwrap_or(BlockrateParams::new::<10>()),
            crescendo_activation: ForkActivation::always(),
        }
    }
}

fn hex_decode(src: &str) -> Vec<u8> {
    if src.is_empty() {
        return Vec::new();
    }
    let mut dst: Vec<u8> = vec![0; src.len() / 2];
    faster_hex::hex_decode(src.as_bytes(), &mut dst).unwrap();
    dst
}

pub fn params_to_kaspad_go_params(params: &Params) -> KaspadGoParams {
    KaspadGoParams {
        K: params.ghostdag_k(),
        TimestampDeviationTolerance: params.timestamp_deviation_tolerance,
        TargetTimePerBlock: params.target_time_per_block() * 1_000_000,
        MaxBlockParents: params.max_block_parents(),
        DifficultyAdjustmentWindowSize: params.difficulty_window_size(),
        MergeSetSizeLimit: params.mergeset_size_limit(),
        MergeDepth: params.merge_depth(),
        FinalityDuration: params.finality_duration_in_milliseconds(),
        CoinbasePayloadScriptPublicKeyMaxLength: params.coinbase_payload_script_public_key_max_len,
        MaxCoinbasePayloadLength: params.max_coinbase_payload_len,
        MassPerTxByte: params.mass_per_tx_byte,
        MassPerSigOp: params.mass_per_sig_op,
        MassPerScriptPubKeyByte: params.mass_per_script_pub_key_byte,
        MaxBlockMass: params.max_block_mass,
        DeflationaryPhaseDaaScore: params.deflationary_phase_daa_score,
        PreDeflationaryPhaseBaseSubsidy: params.pre_deflationary_phase_base_subsidy,
        SkipProofOfWork: params.skip_proof_of_work,
        MaxBlockLevel: params.max_block_level,
        PruningProofM: params.pruning_proof_m,
        BlockrateParams: Some(params.blockrate.clone()),
        CrescendoActivation: Some(params.crescendo_activation),
        storage_mass_parameter: Some(params.storage_mass_parameter),
        max_difficulty_target: Some(params.max_difficulty_target),
        max_difficulty_target_f64: Some(params.max_difficulty_target_f64),
        past_median_time_window_size: Some(params.past_median_time_window_size),
    }
}

pub fn rpc_header_to_header(rpc_header: &RPCBlockHeader) -> Header {
    Header::new_finalized(
        rpc_header.Version,
        rpc_header
            .Parents
            .iter()
            .map(|item| item.ParentHashes.iter().map(|parent| Hash::from_str(parent).unwrap()).collect::<Vec<Hash>>())
            .collect::<Vec<Vec<Hash>>>()
            .try_into()
            .unwrap(),
        Hash::from_str(&rpc_header.HashMerkleRoot).unwrap(),
        Hash::from_str(&rpc_header.AcceptedIDMerkleRoot).unwrap(),
        Hash::from_str(&rpc_header.UTXOCommitment).unwrap(),
        rpc_header.Timestamp,
        rpc_header.Bits,
        rpc_header.Nonce,
        rpc_header.DAAScore,
        BlueWorkType::from_hex(&rpc_header.BlueWork).unwrap(),
        rpc_header.BlueScore,
        Hash::from_str(&rpc_header.PruningPoint).unwrap(),
    )
}

pub fn rpc_block_to_block(rpc_block: RPCBlock) -> Block {
    let header = rpc_header_to_header(&rpc_block.Header);
    assert_eq!(header.hash, Hash::from_str(&rpc_block.VerboseData.Hash).unwrap());
    Block::new(
        header,
        rpc_block
            .Transactions
            .iter()
            .map(|tx| {
                Transaction::new_with_mass(
                    tx.Version,
                    tx.Inputs
                        .iter()
                        .map(|input| TransactionInput {
                            previous_outpoint: TransactionOutpoint {
                                transaction_id: Hash::from_str(&input.PreviousOutpoint.TransactionID).unwrap(),
                                index: input.PreviousOutpoint.Index,
                            },
                            signature_script: hex_decode(&input.SignatureScript),
                            sequence: input.Sequence,
                            sig_op_count: input.SigOpCount,
                        })
                        .collect(),
                    tx.Outputs
                        .iter()
                        .map(|output| TransactionOutput {
                            value: output.Amount,
                            script_public_key: ScriptPublicKey::from_vec(
                                output.ScriptPublicKey.Version,
                                hex_decode(&output.ScriptPublicKey.Script),
                            ),
                        })
                        .collect(),
                    tx.LockTime,
                    SubnetworkId::from_str(&tx.SubnetworkID).unwrap(),
                    tx.Gas,
                    hex_decode(&tx.Payload),
                    tx.Mass,
                )
            })
            .collect(),
    )
}

pub fn json_trusted_line_to_block_and_gd(line: String) -> TrustedBlock {
    let json_block_with_trusted: JsonBlockWithTrustedData = serde_json::from_str(&line).unwrap();
    let block = rpc_block_to_block(json_block_with_trusted.Block);

    let gd = ExternalGhostdagData {
        blue_score: json_block_with_trusted.GHOSTDAG.BlueScore,
        blue_work: BlueWorkType::from_hex(&json_block_with_trusted.GHOSTDAG.BlueWork).unwrap(),
        selected_parent: Hash::from_str(&json_block_with_trusted.GHOSTDAG.SelectedParent).unwrap(),
        mergeset_blues: json_block_with_trusted
            .GHOSTDAG
            .MergeSetBlues
            .into_iter()
            .map(|hex| Hash::from_str(&hex).unwrap())
            .collect_vec(),

        mergeset_reds: json_block_with_trusted
            .GHOSTDAG
            .MergeSetReds
            .into_iter()
            .map(|hex| Hash::from_str(&hex).unwrap())
            .collect_vec(),

        blues_anticone_sizes: BlockHashMap::from_iter(
            json_block_with_trusted
                .GHOSTDAG
                .BluesAnticoneSizes
                .into_iter()
                .map(|e| (Hash::from_str(&e.BlueHash).unwrap(), e.AnticoneSize)),
        ),
    };

    TrustedBlock::new(block, gd)
}

pub fn json_line_to_utxo_pairs(line: String) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    let json_pairs: Vec<JsonOutpointUTXOEntryPair> = serde_json::from_str(&line).unwrap();
    json_pairs
        .iter()
        .map(|json_pair| {
            (
                TransactionOutpoint {
                    transaction_id: Hash::from_str(&json_pair.Outpoint.TransactionID).unwrap(),
                    index: json_pair.Outpoint.Index,
                },
                UtxoEntry {
                    amount: json_pair.UTXOEntry.Amount,
                    script_public_key: ScriptPublicKey::from_vec(
                        json_pair.UTXOEntry.ScriptPublicKey.Version,
                        hex_decode(&json_pair.UTXOEntry.ScriptPublicKey.Script),
                    ),
                    block_daa_score: json_pair.UTXOEntry.BlockDAAScore,
                    is_coinbase: json_pair.UTXOEntry.IsCoinbase,
                },
            )
        })
        .collect_vec()
}

pub fn json_line_to_block(line: String) -> Block {
    let rpc_block: RPCBlock = serde_json::from_str(&line).unwrap();
    rpc_block_to_block(rpc_block)
}

pub fn block_to_rpc_block(block: Block) -> RPCBlock {
    RPCBlock {
        Header: RPCBlockHeader {
            Version: block.header.version,
            Parents: block
                .header
                .parents_by_level
                .expanded_iter()
                .map(|p| RPCBlockLevelParents { ParentHashes: p.iter().map(|h| h.to_string()).collect() })
                .collect(),
            HashMerkleRoot: block.header.hash_merkle_root.to_string(),
            AcceptedIDMerkleRoot: block.header.accepted_id_merkle_root.to_string(),
            UTXOCommitment: block.header.utxo_commitment.to_string(),
            Timestamp: block.header.timestamp,
            Bits: block.header.bits,
            Nonce: block.header.nonce,
            DAAScore: block.header.daa_score,
            BlueScore: block.header.blue_score,
            BlueWork: block.header.blue_work.to_hex(),
            PruningPoint: block.header.pruning_point.to_string(),
        },
        Transactions: block
            .transactions
            .iter()
            .map(|tx| RPCTransaction {
                Version: tx.version,
                Inputs: tx
                    .inputs
                    .iter()
                    .map(|input| RPCTransactionInput {
                        PreviousOutpoint: RPCOutpoint {
                            TransactionID: input.previous_outpoint.transaction_id.to_string(),
                            Index: input.previous_outpoint.index,
                        },
                        SignatureScript: input.signature_script.to_hex(),
                        Sequence: input.sequence,
                        SigOpCount: input.sig_op_count,
                    })
                    .collect(),
                Outputs: tx
                    .outputs
                    .iter()
                    .map(|output| RPCTransactionOutput {
                        Amount: output.value,
                        ScriptPublicKey: RPCScriptPublicKey {
                            Version: output.script_public_key.version,
                            Script: output.script_public_key.script().to_hex(),
                        },
                    })
                    .collect(),
                LockTime: tx.lock_time,
                SubnetworkID: tx.subnetwork_id.to_string(),
                Gas: tx.gas,
                Payload: tx.payload.to_hex(),
                Mass: tx.mass(),
            })
            .collect(),
        VerboseData: RPCBlockVerboseData { Hash: block.hash().to_string() },
    }
}
