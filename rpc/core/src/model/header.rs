pub type RpcHeader = kaspa_consensus_core::header::Header;

// use kaspa_consensus_core::{hashing, BlueWorkType, header::Header};
// use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
// use kaspa_hashes::Hash;
// use serde::{Deserialize, Serialize};

// #[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
// #[serde(rename_all = "camelCase")]
// pub struct RpcHeader {
//     pub hash: Hash, // Cached hash
//     pub version: u16,
//     pub parents_by_level: Vec<Vec<Hash>>,
//     pub hash_merkle_root: Hash,
//     pub accepted_id_merkle_root: Hash,
//     pub utxo_commitment: Hash,
//     pub timestamp: u64, // Timestamp is in milliseconds
//     pub bits: u32,
//     pub nonce: u64,
//     pub daa_score: u64,

//     pub blue_work: BlueWorkType,
//     pub blue_score: u64,
//     pub pruning_point: Hash,
// }

// impl From<Header> for RpcHeader {
//     fn from(header: Header) -> Self {
//         Self {
//             hash: header.hash,
//             version: header.version,
//             parents_by_level: header.parents_by_level,
//             hash_merkle_root: header.hash_merkle_root,
//             accepted_id_merkle_root: header.accepted_id_merkle_root,
//             utxo_commitment: header.utxo_commitment,
//             timestamp: header.timestamp,
//             bits: header.bits,
//             nonce: header.nonce,
//             daa_score: header.daa_score,
//             blue_work: header.blue_work,
//             blue_score: header.blue_score,
//             pruning_point: header.pruning_point,
//         }
//     }
// }
