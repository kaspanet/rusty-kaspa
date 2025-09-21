use kaspa_merkle::MerkleWitness;
use std::{collections::HashMap, sync::Arc};

use crate::header::Header;
use kaspa_hashes::Hash;

/// A struct with the ability to attest blocks were on the selected chain
/// down from alleged_posterity until some chain block
/// Contains a segment of the selected chain structure down from the alleged_posterity.
/// The structure does not explicitely contain the block it testifies to, as many blocks can be testified
/// by the same ProofOfChainMembership instance
#[derive(Clone)]
pub struct ProofOfChainMembership {
    // a map for easy traversal on the alleged selected chain headers
    pub header_map: HashMap<Hash, Arc<Header>>,
    // the alleged posterity block the proof uses as its anchor
    pub alleged_posterity: Hash,
}
impl ProofOfChainMembership {
    fn find_selected_parent(&self, parents: Vec<Arc<Header>>) -> Hash {
        let max_bscore = parents.clone().into_iter().map(|h| h.blue_score).max().unwrap();
        parents.into_iter().filter(|parent| parent.blue_score == max_bscore).map(|h| h.hash).max().unwrap()
    }
    pub fn new(traversal_vec: Vec<Arc<Header>>, alleged_posterity: Hash) -> Self {
        assert_eq!(traversal_vec.last().unwrap().hash, alleged_posterity);
        let mut header_map = HashMap::new();
        for hdr in traversal_vec.into_iter() {
            header_map.insert(hdr.hash, hdr);
        }
        Self { header_map, alleged_posterity }
    }
    pub fn verify_path(&self, chain_purporter: Hash) -> bool {
        //verify top consistency and availability
        if self.header_map.get(&self.alleged_posterity).is_none_or(|hdr| hdr.hash != self.alleged_posterity) {
            return false;
        }
        let mut next_chain_blk = self.alleged_posterity;
        loop {
            if next_chain_blk == chain_purporter {
                return true;
            }
            //verify parents consistency and availability
            for &par in self.header_map.get(&next_chain_blk).unwrap().parents_by_level[0].iter() {
                if self.header_map.get(&par).is_none_or(|hdr| hdr.hash != par) {
                    return false;
                }
            }
            let parents = self.header_map.get(&next_chain_blk).unwrap().parents_by_level[0].clone();
            let parents_headers: Vec<Arc<Header>> = parents.iter().map(|p| self.header_map.get(p).unwrap().clone()).collect();
            next_chain_blk = self.find_selected_parent(parents_headers)
        }
    }
}
/// A legacy-style receipt that attests a tracked transaction was accepted  in a specific block,
/// accompanied by a ProofOfChainMembership for this blocks inclusion in the selected chain
/// selected chain up to a tip.
///
/// Intended for pre-crescendo transactions for which modern receipts via the sequencing commitments cannot be created

// TODO(relaxed): create functions and API to extract and verify legacy receipts for pre crescendo transactions
// Would only be relevant for archival nodes hence there is little urgency for such a feature if at all
#[derive(Clone)]
pub struct LegacyReceipt {
    pub tracked_tx_id: Hash,
    pub accepting_block_header: Arc<Header>,
    pub proof_of_chain_membership: ProofOfChainMembership,
    pub tx_acceptance_proof: MerkleWitness,
}

/// A struct proving **publication** of a transaction:
///  proofs inclusion in a specific block,
/// an explicit path  from the publishing block to a chain block
/// and a  chain-membership proof for that chain block,
#[derive(Clone)]

pub struct ProofOfPublication {
    pub tracked_tx_hash: Hash,
    pub publication_block_header: Arc<Header>,
    pub proof_of_chain_membership: ProofOfChainMembership,
    pub tx_publication_proof: MerkleWitness,
    pub headers_path_to_selected: Vec<Arc<Header>>,
}
/// A compact receipt that attests a tracked transaction’s **acceptance**:
/// Attesting directly via the sequencing commitment down from a posterity block
#[derive(Clone)]
pub struct TxReceipt {
    pub tracked_tx_id: Hash,
    pub posterity_block: Hash,
    pub initial_sequencing_commitment: Hash,
    // the accepted transactions merkle root segment of each sequencing commitment on path
    //from the accepting block to posterity
    pub accepted_tx_mroot_chain: Vec<Hash>,
    pub tx_acceptance_proof: MerkleWitness,
}
