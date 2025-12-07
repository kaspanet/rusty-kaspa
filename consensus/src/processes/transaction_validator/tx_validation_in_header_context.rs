//! Groups transaction validations that depend on the containing header and/or
//! its past headers (but do not depend on UTXO state or other transactions in
//! the containing block)

use super::{
    errors::{TxResult, TxRuleError},
    TransactionValidator,
};
use crate::constants::LOCK_TIME_THRESHOLD;
use kaspa_consensus_core::tx::Transaction;

pub(crate) enum LockTimeType {
    Finalized,
    DaaScore,
    Time,
}

pub(crate) enum LockTimeArg {
    Finalized,
    DaaScore(u64),
    MedianTime(u64),
}

impl TransactionValidator {
    pub(crate) fn validate_tx_in_header_context_with_args(
        &self,
        tx: &Transaction,
        ctx_daa_score: u64,
        ctx_block_time: u64,
    ) -> TxResult<()> {
        self.validate_tx_in_header_context(
            tx,
            match Self::get_lock_time_type(tx) {
                LockTimeType::Finalized => LockTimeArg::Finalized,
                LockTimeType::DaaScore => LockTimeArg::DaaScore(ctx_daa_score),
                LockTimeType::Time => LockTimeArg::MedianTime(ctx_block_time),
            },
        )
    }

    pub(crate) fn validate_tx_in_header_context(&self, tx: &Transaction, lock_time_arg: LockTimeArg) -> TxResult<()> {
        self.check_tx_is_finalized(tx, lock_time_arg)
    }

    pub(crate) fn get_lock_time_type(tx: &Transaction) -> LockTimeType {
        match tx.lock_time {
            // Lock time of zero means the transaction is finalized.
            0 => LockTimeType::Finalized,

            // The lock time field of a transaction is either a block DAA score at
            // which the transaction is finalized or a timestamp depending on if the
            // value is before the LOCK_TIME_THRESHOLD. When it is under the
            // threshold it is a DAA score
            t if t < LOCK_TIME_THRESHOLD => LockTimeType::DaaScore,

            // ..and when equal or above the threshold it represents time
            _t => LockTimeType::Time,
        }
    }

    fn check_tx_is_finalized(&self, tx: &Transaction, lock_time_arg: LockTimeArg) -> TxResult<()> {
        let block_time_or_daa_score = match lock_time_arg {
            LockTimeArg::Finalized => return Ok(()),
            LockTimeArg::DaaScore(ctx_daa_score) => ctx_daa_score,
            LockTimeArg::MedianTime(ctx_block_time) => ctx_block_time,
        };

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
