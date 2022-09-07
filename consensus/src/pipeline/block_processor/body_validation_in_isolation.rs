use consensus_core::{block::Block, merkle::calc_hash_merkle_root};

use crate::errors::{BlockProcessResult, RuleError};

use super::BlockBodyProcessor;

impl BlockBodyProcessor {
    pub fn validate_body_in_isolation(block: &Block) -> BlockProcessResult<()> {
        Self::check_hash_merkle_tree(block)
    }

    fn check_hash_merkle_tree(block: &Block) -> BlockProcessResult<()> {
        let calculated = calc_hash_merkle_root(block.transactions.iter());
        if calculated != block.header.hash_merkle_root {
            return Err(RuleError::BadMerkleRoot(block.header.hash_merkle_root, calculated));
        }
        Ok(())
    }
}
