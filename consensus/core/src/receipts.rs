use kaspa_merkle::MerkleWitness;

use kaspa_hashes::Hash;

/// A compact receipt that attests a tracked transaction’s **acceptance**:
/// Attesting directly via the sequencing commitment down from a posterity block
#[derive(Clone)]
pub struct TxReceipt {
    pub tracked_tx_id: Hash,
    pub posterity_block: Hash,
    pub parent_of_accepting_blk_sequencing_commitment: Hash,
    pub state_roots_chain_to_acepting_blk: Vec<Hash>,
    pub accepting_blk_payload_and_ctx_digest: Hash,
    pub accepting_blk_activity_root: Hash,

    pub tx_acceptance_proof: MerkleWitness,
}
