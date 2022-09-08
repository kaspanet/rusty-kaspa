use std::{collections::HashSet, sync::Arc};

use consensus_core::{block::Block, merkle::calc_hash_merkle_root, tx::TransactionOutpoint};

use crate::errors::{BlockProcessResult, RuleError};

use super::BlockBodyProcessor;

impl BlockBodyProcessor {
    pub fn validate_body_in_isolation(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        Self::check_has_transactions(block)?;
        Self::check_hash_merkle_tree(block)?;
        Self::check_only_one_coinbase(block)?;
        self.check_coinbase_in_isolation(block)?;
        self.check_transactions_in_isolation(block)?;
        self.check_block_mass(block)?;
        self.check_block_double_spends(block)?;
        self.check_no_chained_transactions(block)
    }

    fn check_has_transactions(block: &Block) -> BlockProcessResult<()> {
        // We expect the outer flow to not queue blocks with no transactions for body validation,
        // but we still check it in case the outer flow changes.
        if block.transactions.is_empty() {
            return Err(RuleError::NoTransactions);
        }
        Ok(())
    }

    fn check_hash_merkle_tree(block: &Block) -> BlockProcessResult<()> {
        let calculated = calc_hash_merkle_root(block.transactions.iter());
        if calculated != block.header.hash_merkle_root {
            return Err(RuleError::BadMerkleRoot(block.header.hash_merkle_root, calculated));
        }
        Ok(())
    }

    fn check_only_one_coinbase(block: &Block) -> BlockProcessResult<()> {
        if !block.transactions[0].is_coinbase() {
            return Err(RuleError::FirstTxNotCoinbase);
        }

        if let Some(i) = block.transactions[1..]
            .iter()
            .position(|tx| tx.is_coinbase())
        {
            return Err(RuleError::MultipleCoinbases(i));
        }

        Ok(())
    }

    fn check_coinbase_in_isolation(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        match self
            .coinbase_manager
            .validate_coinbase_payload_in_isolation_and_extract_blue_score(&block.transactions[0])
        {
            Ok(blue_score) => {
                if blue_score != block.header.blue_score {
                    Err(RuleError::BadCoinbasePayloadBlueScore(blue_score, block.header.blue_score))
                } else {
                    Ok(())
                }
            }
            Err(e) => Err(RuleError::BadCoinbasePayload(e)),
        }
    }

    fn check_transactions_in_isolation(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        for tx in &block.transactions {
            if let Err(e) = self
                .transaction_validator
                .validate_tx_in_isolation(&tx)
            {
                return Err(RuleError::TxInIsolationValidationFailed(tx.id(), e));
            }
        }
        Ok(())
    }

    fn check_block_mass(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        let mut total_mass: u64 = 0;
        for tx in &block.transactions {
            total_mass += self.mass_calculator.calc_tx_mass(tx);
            if total_mass > self.max_block_mass {
                return Err(RuleError::ExceedsMassLimit(self.max_block_mass));
            }
        }
        Ok(())
    }

    fn check_block_double_spends(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        let mut existing = HashSet::new();
        for input in block
            .transactions
            .iter()
            .flat_map(|tx| &tx.inputs)
        {
            if !existing.insert(input.previous_outpoint) {
                return Err(RuleError::DoubleSpendInSameBlock(input.previous_outpoint));
            }
        }
        Ok(())
    }

    fn check_no_chained_transactions(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        let mut block_created_outpoints = HashSet::new();
        for tx in &block.transactions {
            for index in 0..tx.outputs.len() {
                block_created_outpoints.insert(TransactionOutpoint { transaction_id: tx.id(), index: index as u32 });
            }
        }

        for input in block
            .transactions
            .iter()
            .flat_map(|tx| &tx.inputs)
        {
            if block_created_outpoints.contains(&input.previous_outpoint) {
                return Err(RuleError::ChainedTransaction(input.previous_outpoint));
            }
        }
        Ok(())
    }
}
