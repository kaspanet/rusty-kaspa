use itertools::Itertools;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::trusted::TrustedBlock;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use serde::{Deserialize, Serialize};

pub use kaspa_consensus_core::trusted::ExternalGhostdagData as JtfGhostdagData;
pub use kaspa_rpc_core::{
    RpcBlock as JtfBlock, RpcHeader as JtfHeader, RpcTransactionOutpoint as JtfOutpoint, RpcUtxoEntry as JtfUtxoEntry,
};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JtfTrustedBlock {
    pub block: JtfBlock,
    pub ghostdag: JtfGhostdagData,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JtfOutpointUtxoEntryPair {
    pub outpoint: JtfOutpoint,
    pub entry: JtfUtxoEntry,
}

pub fn json_line_to_trusted_block(line: String) -> TrustedBlock {
    let jtf_trusted_block: JtfTrustedBlock = serde_json::from_str(&line).unwrap();
    let block: Block = jtf_trusted_block.block.try_into().unwrap();
    TrustedBlock::new(block, jtf_trusted_block.ghostdag)
}

pub fn json_line_to_utxo_pairs(line: String) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    let jtf_pairs: Vec<JtfOutpointUtxoEntryPair> = serde_json::from_str(&line).unwrap();
    jtf_pairs.iter().map(|pair| (pair.outpoint.into(), pair.entry.clone().into())).collect_vec()
}

pub fn json_line_to_block(line: String) -> Block {
    let jtf_block: JtfBlock = serde_json::from_str(&line).unwrap();
    jtf_block.try_into().unwrap()
}
