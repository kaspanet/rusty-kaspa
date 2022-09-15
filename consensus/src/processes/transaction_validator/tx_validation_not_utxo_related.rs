use consensus_core::tx::Transaction;

use crate::{constants::LOCK_TIME_THRESHOLD, model::stores::headers::HeaderStoreReader};

use super::{
    errors::{TxResult, TxRuleError},
    TransactionValidator,
};

impl<T: HeaderStoreReader> TransactionValidator<T> {
    pub fn utxo_free_tx_validation(&self, tx: &Transaction, ctx_daa_score: u64, ctx_block_time: u64) -> TxResult<()> {
        self.check_tx_is_finalized(tx, ctx_daa_score, ctx_block_time)
    }

    fn check_tx_is_finalized(&self, tx: &Transaction, ctx_daa_score: u64, ctx_block_time: u64) -> TxResult<()> {
        // Lock time of zero means the transaction is finalized.
        if tx.lock_time == 0 {
            return Ok(());
        }

        // The lock time field of a transaction is either a block DAA score at
        // which the transaction is finalized or a timestamp depending on if the
        // value is before the LOCK_TIME_THRESHOLD. When it is under the
        // threshold it is a DAA score.
        let block_time_or_daa_score = if tx.lock_time < LOCK_TIME_THRESHOLD { ctx_daa_score } else { ctx_block_time };
        if tx.lock_time < block_time_or_daa_score {
            return Ok(());
        }

        // At this point, the transaction's lock time hasn't occurred yet, but
        // the transaction might still be finalized if the sequence number
        // for all transaction inputs is maxed out.
        for (i, input) in tx.inputs.iter().enumerate() {
            if input.sequence != u64::MAX {
                return Err(TxRuleError::NotFinalized(i));
            }
        }

        Ok(())
    }
}
