use std::{collections::HashMap, sync::Arc};

use kaspa_merkle::MerkleWitness;

use crate::{header::Header, BlueWorkType};
use kaspa_hashes::Hash;
#[derive(Clone)]
pub struct UnverifiedHeader {
    pub version: u16,
    pub parents_by_level: Vec<Vec<Hash>>,
    pub hash_merkle_root: Hash,
    pub accepted_id_merkle_root: Hash,
    pub utxo_commitment: Hash,
    /// Timestamp is in milliseconds
    pub timestamp: u64,
    pub bits: u32,
    pub nonce: u64,
    pub daa_score: u64,
    pub blue_work: BlueWorkType,
    pub blue_score: u64,
    pub pruning_point: Hash,
}
impl UnverifiedHeader {
    pub fn hash(&self) -> Hash {
        let hdr = Header::from(self);
        hdr.hash
    }
}

impl From<UnverifiedHeader> for Header {
    fn from(header: UnverifiedHeader) -> Self {
        Self::new_finalized(
            header.version,
            header.parents_by_level,
            header.hash_merkle_root,
            header.accepted_id_merkle_root,
            header.utxo_commitment,
            header.timestamp,
            header.bits,
            header.nonce,
            header.daa_score,
            header.blue_work,
            header.blue_score,
            header.pruning_point,
        )
    }
}

impl From<&UnverifiedHeader> for Header {
    fn from(header: &UnverifiedHeader) -> Self {
        Self::new_finalized(
            header.version,
            header.parents_by_level.clone(),
            header.hash_merkle_root,
            header.accepted_id_merkle_root,
            header.utxo_commitment,
            header.timestamp,
            header.bits,
            header.nonce,
            header.daa_score,
            header.blue_work,
            header.blue_score,
            header.pruning_point,
        )
    }
}

impl From<&Header> for UnverifiedHeader {
    fn from(header: &Header) -> Self {
        Self {
            version: header.version,
            parents_by_level: header.parents_by_level.clone(),
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            utxo_commitment: header.utxo_commitment,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
        }
    }
}

impl From<Header> for UnverifiedHeader {
    fn from(header: Header) -> Self {
        Self {
            version: header.version,
            parents_by_level: header.parents_by_level,
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            utxo_commitment: header.utxo_commitment,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
        }
    }
}

impl From<Arc<Header>> for UnverifiedHeader {
    fn from(header: Arc<Header>) -> Self {
        Self {
            version: header.version,
            parents_by_level: header.parents_by_level.clone(),
            hash_merkle_root: header.hash_merkle_root,
            accepted_id_merkle_root: header.accepted_id_merkle_root,
            utxo_commitment: header.utxo_commitment,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            daa_score: header.daa_score,
            blue_work: header.blue_work,
            blue_score: header.blue_score,
            pruning_point: header.pruning_point,
        }
    }
}

#[derive(Clone)]
pub struct LegacyPochm {
    pub bfs_map: HashMap<Hash, UnverifiedHeader>,
    pub top: Hash,
    pub bottom: Hash,
}
impl LegacyPochm {
    pub fn new(bfs_vec: Vec<(Hash, Arc<Header>)>) -> Self {
        let top = bfs_vec.last().unwrap().0;
        let bottom = bfs_vec.first().unwrap().0;
        let mut bfs_map = HashMap::new();
        for (key, val) in bfs_vec.into_iter() {
            bfs_map.insert(key, UnverifiedHeader::from(val));
        }
        Self { bfs_map, top, bottom }
    }
    pub fn verify_bfs_path(&self, chain_purpoter: Hash) -> bool {
        let mut next = self.top;
        if next != self.bfs_map[&next].hash() {
            return false;
        }
        loop {
            if next == chain_purpoter {
                return true;
            }
            let next_hdr = self.bfs_map.get(&next);
            if let Some(next_hdr) = next_hdr {
                if next != next_hdr.hash() {
                    return false;
                }
                for &par in next_hdr.parents_by_level[0].iter() {
                    if par != self.bfs_map[&par].hash() {
                        return false;
                    }
                    next = *next_hdr.parents_by_level[0]
                        .iter()
                        .map(|blk| (blk, self.bfs_map[blk].blue_score))
                        .reduce(
                            |(blk, bscore), (max_blk, max_bscore)| {
                                if bscore > max_bscore {
                                    (blk, bscore)
                                } else {
                                    (max_blk, max_bscore)
                                }
                            },
                        )
                        .unwrap()
                        .0;
                }
            } else {
                return false;
            }
        }
    }
}
#[derive(Clone)]
pub struct PochmSegment {
    pub header: UnverifiedHeader,
    pub leaf_in_pchmr_witness: MerkleWitness,
}
#[derive(Clone)]
pub struct LogPochm {
    pub vec: Vec<PochmSegment>,
    // hash_to_pchmr_store: Arc<DbPchmrStore>, //temporary field
}
impl LogPochm {
    pub fn new() -> Self {
        let vec = vec![];
        Self { vec }
    }
    pub fn insert(&mut self, header: Arc<Header>, witness: MerkleWitness) {
        let header = UnverifiedHeader::from(header);
        self.vec.push(PochmSegment { header, leaf_in_pchmr_witness: witness })
    }
    pub fn get_path_origin(&self) -> Option<Hash> {
        self.vec.first().map(|seg| seg.header.hash())
    }
}

impl Default for LogPochm {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Clone)]
pub struct TxReceipt {
    pub tracked_tx_id: Hash,
    pub accepting_block_header: UnverifiedHeader,
    pub pochm: LogPochm,
    pub tx_acc_proof: MerkleWitness,
}
#[derive(Clone)]

pub struct ProofOfPublication {
    pub tracked_tx_hash: Hash,
    pub pub_block_header: UnverifiedHeader,
    pub pochm: LogPochm,
    pub tx_pub_proof: MerkleWitness,
    pub headers_path_to_selected: Vec<UnverifiedHeader>,
}
