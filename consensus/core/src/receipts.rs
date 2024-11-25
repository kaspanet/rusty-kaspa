use std::{collections::HashMap, sync::Arc};

use kaspa_merkle::MerkleWitness;

use crate::header::Header;
use kaspa_hashes::Hash;

#[derive(Clone)]
pub struct LegacyPochm {
    pub bfs_map: HashMap<Hash, Arc<Header>>,
    pub top: Hash,
    // hash_to_pchmr_store: Arc<DbPchmrStore>, //temporary field
}
impl LegacyPochm {
    pub fn new(bfs_vec: Vec<(Hash, Arc<Header>)>) -> Self {
        let top = bfs_vec.last().unwrap().0;
        let mut bfs_map = HashMap::new();
        for (key, val) in bfs_vec.into_iter() {
            bfs_map.insert(key, val);
        }
        Self { bfs_map, top }
    }
    pub fn verify_bfs_path(_chain_purpoter: Hash) -> bool {
        unimplemented!();
    }
}
#[derive(Clone)]
pub struct PochmSegment {
    pub header: Arc<Header>,
    pub leaf_in_pchmr_witness: MerkleWitness,
}
#[derive(Clone)]
pub struct Pochm {
    pub vec: Vec<PochmSegment>,
    // hash_to_pchmr_store: Arc<DbPchmrStore>, //temporary field
}
impl Pochm {
    pub fn new() -> Self {
        let vec = vec![];
        Self { vec }
    }
    pub fn insert(&mut self, header: Arc<Header>, witness: MerkleWitness) {
        self.vec.push(PochmSegment { header, leaf_in_pchmr_witness: witness })
    }
    pub fn get_path_origin(&self) -> Option<Hash> {
        self.vec.first().map(|seg| seg.header.hash)
    }
}

impl Default for Pochm {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Clone)]
pub struct TxReceipt {
    pub tracked_tx_id: Hash,
    pub accepting_block_header: Arc<Header>,
    pub pochm: Pochm,
    pub tx_acc_proof: MerkleWitness,
}
#[derive(Clone)]

pub struct ProofOfPublication {
    pub tracked_tx_hash: Hash,
    pub pub_block_header: Arc<Header>,
    pub pochm: Pochm,
    pub tx_pub_proof: MerkleWitness,
    pub headers_path_to_selected: Vec<Arc<Header>>,
}
