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
            ctx_daa_score,
            match Self::get_lock_time_type(tx) {
                LockTimeType::Finalized => LockTimeArg::Finalized,
                LockTimeType::DaaScore => LockTimeArg::DaaScore(ctx_daa_score),
                LockTimeType::Time => LockTimeArg::MedianTime(ctx_block_time),
            },
        )
    }

    pub(crate) fn validate_tx_in_header_context(
        &self,
        tx: &Transaction,
        ctx_daa_score: u64,
        lock_time_arg: LockTimeArg,
    ) -> TxResult<()> {
        self.check_transaction_payload(tx, ctx_daa_score)?;
        self.check_transaction_inputs_count_ctx(tx, ctx_daa_score)?;
        self.check_transaction_outputs_count_ctx(tx, ctx_daa_score)?;
        self.check_transaction_signature_scripts_ctx(tx, ctx_daa_score)?;
        self.check_transaction_script_public_keys_ctx(tx, ctx_daa_score)?;
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

    fn check_transaction_payload(&self, tx: &Transaction, ctx_daa_score: u64) -> TxResult<()> {
        // TODO (post HF): move back to in isolation validation
        if self.crescendo_activation.is_active(ctx_daa_score) {
            Ok(())
        } else {
            if !tx.is_coinbase() && !tx.payload.is_empty() {
                return Err(TxRuleError::NonCoinbaseTxHasPayload);
            }
            Ok(())
        }
    }

    fn check_transaction_outputs_count_ctx(&self, tx: &Transaction, ctx_daa_score: u64) -> TxResult<()> {
        // TODO (post HF): move back to in isolation validation
        if tx.is_coinbase() {
            // We already check coinbase outputs count vs. Ghostdag K + 2
            return Ok(());
        }
        if tx.outputs.len() > self.max_tx_outputs.get(ctx_daa_score) {
            return Err(TxRuleError::TooManyOutputs(tx.outputs.len(), self.max_tx_inputs.get(ctx_daa_score)));
        }

        Ok(())
    }

    fn check_transaction_inputs_count_ctx(&self, tx: &Transaction, ctx_daa_score: u64) -> TxResult<()> {
        // TODO (post HF): move back to in isolation validation
        if !tx.is_coinbase() && tx.inputs.is_empty() {
            return Err(TxRuleError::NoTxInputs);
        }

        if tx.inputs.len() > self.max_tx_inputs.get(ctx_daa_score) {
            return Err(TxRuleError::TooManyInputs(tx.inputs.len(), self.max_tx_inputs.get(ctx_daa_score)));
        }

        Ok(())
    }

    // The main purpose of this check is to avoid overflows when calculating transaction mass later.
    fn check_transaction_signature_scripts_ctx(&self, tx: &Transaction, ctx_daa_score: u64) -> TxResult<()> {
        // TODO (post HF): move back to in isolation validation
        if let Some(i) =
            tx.inputs.iter().position(|input| input.signature_script.len() > self.max_signature_script_len.get(ctx_daa_score))
        {
            return Err(TxRuleError::TooBigSignatureScript(i, self.max_signature_script_len.get(ctx_daa_score)));
        }

        Ok(())
    }

    // The main purpose of this check is to avoid overflows when calculating transaction mass later.
    fn check_transaction_script_public_keys_ctx(&self, tx: &Transaction, ctx_daa_score: u64) -> TxResult<()> {
        // TODO (post HF): move back to in isolation validation
        if let Some(i) =
            tx.outputs.iter().position(|out| out.script_public_key.script().len() > self.max_script_public_key_len.get(ctx_daa_score))
        {
            return Err(TxRuleError::TooBigScriptPublicKey(i, self.max_script_public_key_len.get(ctx_daa_score)));
        }

        Ok(())
    }
}
