use std::collections::HashMap;

use kaspa_hashes::{Hash, SeqCommitmentMerkleBranchHash};
use kaspa_txscript::seq_commit_accessor::SeqCommitAccessor;
use zk_covenant_rollup_core::{action::Action, seq_commit::seq_commitment_leaf, state::State};

use crate::mock_tx::MockTx;

/// Mock implementation of SeqCommitAccessor for testing
pub struct MockSeqCommitAccessor(pub HashMap<Hash, Hash>);

impl SeqCommitAccessor for MockSeqCommitAccessor {
    fn is_chain_ancestor_from_pov(&self, block_hash: Hash) -> Option<bool> {
        self.0.contains_key(&block_hash).then_some(true)
    }

    fn seq_commitment_within_depth(&self, block_hash: Hash) -> Option<Hash> {
        self.0.get(&block_hash).copied()
    }
}

/// Result of building a mock chain
pub struct MockChain {
    pub block_hashes: Vec<Hash>,
    pub block_txs: Vec<Vec<MockTx>>,
    pub accessor: MockSeqCommitAccessor,
    pub final_seq_commit: Hash,
    pub final_state: State,
}

/// Build a mock chain with the given number of blocks
pub fn build_mock_chain(chain_len: u32, initial_seq_commit: Hash, mut state: State) -> MockChain {
    let block_hashes: Vec<Hash> = (1..=chain_len).map(|i| Hash::from_u64_word(i as u64)).collect();
    let block_txs: Vec<Vec<MockTx>> = (0..chain_len).map(crate::mock_tx::create_mock_block_txs).collect();

    let mut seq_commit = initial_seq_commit;
    let mut accessor_map = HashMap::new();

    println!("\n=== Processing blocks ===");
    for (block_idx, (block_hash, txs)) in block_hashes.iter().zip(block_txs.iter()).enumerate() {
        // Compute tx leaf digests
        let tx_digests: Vec<Hash> = txs
            .iter()
            .map(|tx| {
                let leaf = seq_commitment_leaf(&tx.tx_id(), tx.version());
                Hash::from_bytes(bytemuck::cast_slice(&leaf).try_into().unwrap())
            })
            .collect();

        // Update seq_commitment
        seq_commit = calc_accepted_id_merkle_root(seq_commit, tx_digests.into_iter());
        accessor_map.insert(*block_hash, seq_commit);
        println!("  Block {} seq_commit: {}", block_idx, seq_commit);

        // Execute valid actions
        for tx in txs {
            if tx.is_valid_action() {
                if let MockTx::V1 { payload, .. } = tx {
                    if let Ok(action) = Action::try_from(payload.action_raw) {
                        let output = action.execute();
                        state.add_new_result(action, output);
                        println!("  Block {}: {:?} -> {}", block_idx, action, output);
                    }
                }
            }
        }
    }

    MockChain {
        block_hashes,
        block_txs,
        accessor: MockSeqCommitAccessor(accessor_map),
        final_seq_commit: seq_commit,
        final_state: state,
    }
}

pub fn calc_accepted_id_merkle_root(prev: Hash, tx_digests: impl ExactSizeIterator<Item = Hash>) -> Hash {
    kaspa_merkle::merkle_hash_with_hasher(
        prev,
        kaspa_merkle::calc_merkle_root_with_hasher::<SeqCommitmentMerkleBranchHash, true>(tx_digests),
        SeqCommitmentMerkleBranchHash::new(),
    )
}

pub fn from_bytes(arr: [u8; 32]) -> [u32; 8] {
    let mut out = [0; 8];
    bytemuck::bytes_of_mut(&mut out).copy_from_slice(&arr);
    out
}
