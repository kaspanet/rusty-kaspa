use kaspa_merkle::MerkleWitness;

use kaspa_hashes::Hash;

/// A compact receipt that attests a tracked transaction’s **acceptance**:
/// Attesting directly via the sequencing commitment down from a posterity block
#[derive(Clone)]
pub struct TxReceipt {
    pub tracked_tx_id: Hash,
    pub posterity_block: Hash,
    pub initial_sequencing_commitment: Hash,
    // the accepted transactions merkle root segment of each sequencing commitment on path
    // from the accepting block to posterity
    pub accepted_tx_mroot_chain: Vec<Hash>,
    pub tx_acceptance_proof: MerkleWitness,
}
