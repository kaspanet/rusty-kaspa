use std::sync::Arc;

use consensus_core::{block::Block, hashing};
use misc::merkle::calc_merkle_root;

use crate::errors::{BlockProcessResult, RuleError};

use super::BlockBodyProcessor;

impl BlockBodyProcessor {
    pub fn validate_body_in_isolation(block: &Block) -> BlockProcessResult<()> {
        Self::check_hash_merkle_tree(block)
    }

    fn check_hash_merkle_tree(block: &Block) -> BlockProcessResult<()> {
        let tx_hashes = block
            .transactions
            .iter()
            .map(|tx| hashing::tx::hash(tx));
        let calculated = calc_merkle_root(tx_hashes);
        if calculated != block.header.hash_merkle_root {
            return Err(RuleError::BadMerkleRoot(block.header.hash_merkle_root, calculated));
        }
        Ok(())
    }
}
