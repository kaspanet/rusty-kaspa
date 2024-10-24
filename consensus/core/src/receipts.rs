use std::sync::Arc;

use kaspa_merkle::MerkleWitness;

use crate::header::Header;
use kaspa_hashes::Hash;

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
pub struct TxReceipt {
    pub tracked_tx_id: Hash,
    pub accepting_block_hash: Hash,
    pub pochm: Pochm,
    pub tx_acc_proof: MerkleWitness,
}
pub struct ProofOfPublication {
    pub tracked_tx_id: Hash,
    pub pub_block_hash: Hash,
    pub pochm: Pochm,
    pub tx_pub_proof: MerkleWitness,
    pub headers_chain_to_selected: Vec<Arc<Header>>,
}
