use kaspa_merkle::MerkleWitness;
use std::{collections::HashMap, sync::Arc};

use crate::header::Header;
use kaspa_hashes::Hash;
#[derive(Clone)]
pub struct Pochm {
    pub hdr_map: HashMap<Hash, Arc<Header>>,
    pub top: Hash,
}
impl Pochm {
    fn find_selected_parent(&self, parents: Vec<Arc<Header>>) -> Hash {
        let max_bscore = parents.clone().into_iter().map(|h| h.blue_score).max().unwrap();
        parents.into_iter().filter(|parent| parent.blue_score == max_bscore).map(|h| h.hash).max().unwrap()
    }
    pub fn new(traversal_vec: Vec<Arc<Header>>) -> Self {
        let top = traversal_vec.last().unwrap().hash;
        let mut hdr_map = HashMap::new();
        for hdr in traversal_vec.into_iter() {
            hdr_map.insert(hdr.hash, hdr);
        }
        Self { hdr_map, top }
    }
    pub fn verify_path(&self, chain_purporter: Hash) -> bool {
        //verify top consistency and availability
        if self.hdr_map.get(&self.top).is_none_or(|hdr| hdr.hash != self.top) {
            return false;
        }
        let mut next_chain_blk = self.top;
        loop {
            if next_chain_blk == chain_purporter {
                return true;
            }
            //verify parents consistency and availability
            for &par in self.hdr_map.get(&next_chain_blk).unwrap().parents_by_level[0].iter() {
                if self.hdr_map.get(&par).is_none_or(|hdr| hdr.hash != par) {
                    return false;
                }
            }
            let parents = self.hdr_map.get(&next_chain_blk).unwrap().parents_by_level[0].clone();
            let parents_headers: Vec<Arc<Header>> = parents.iter().map(|p| self.hdr_map.get(p).unwrap().clone()).collect();
            next_chain_blk = self.find_selected_parent(parents_headers)
        }
    }
}
#[derive(Clone)]
pub struct LegacyReceipt {
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
pub struct TxReceipt {
    pub tracked_tx_id: Hash,
    pub posterity_block: Hash,
    pub init_sqc: Hash,
    pub atmr_chain: Vec<Hash>,
    pub tx_acc_proof: MerkleWitness,
}
