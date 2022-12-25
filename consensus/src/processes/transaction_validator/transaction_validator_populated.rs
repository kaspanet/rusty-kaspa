use crate::constants::{MAX_SOMPI, SEQUENCE_LOCK_TIME_DISABLED, SEQUENCE_LOCK_TIME_MASK};
use consensus_core::{
    hashing::{
        sighash::{calc_schnorr_signature_hash, SigHashReusedValues},
        sighash_type::SIG_HASH_ALL,
    },
    tx::VerifiableTransaction,
};

use super::{
    errors::{TxResult, TxRuleError},
    SigCacheKey, TransactionValidator,
};

impl TransactionValidator {
    pub fn validate_populated_transaction_and_get_fee(&self, tx: &impl VerifiableTransaction, pov_daa_score: u64) -> TxResult<u64> {
        self.check_transaction_coinbase_maturity(tx, pov_daa_score)?;
        let total_in = self.check_transaction_input_amounts(tx)?;
        let total_out = Self::check_transaction_output_values(tx, total_in)?;
        Self::check_sequence_lock(tx, pov_daa_score)?;
        Self::check_sig_op_counts(tx)?;
        self.check_scripts(tx)?;

        Ok(total_in - total_out)
    }

    fn check_transaction_coinbase_maturity(&self, tx: &impl VerifiableTransaction, pov_daa_score: u64) -> TxResult<()> {
        if let Some((index, (input, entry))) = tx
            .populated_inputs()
            .enumerate()
            .find(|(_, (_, entry))| entry.is_coinbase && entry.block_daa_score + self.coinbase_maturity > pov_daa_score)
        {
            return Err(TxRuleError::ImmatureCoinbaseSpend(
                index,
                input.previous_outpoint,
                entry.block_daa_score,
                pov_daa_score,
                self.coinbase_maturity,
            ));
        }

        Ok(())
    }

    fn check_transaction_input_amounts(&self, tx: &impl VerifiableTransaction) -> TxResult<u64> {
        let mut total: u64 = 0;
        for (_, entry) in tx.populated_inputs() {
            if let Some(new_total) = total.checked_add(entry.amount) {
                total = new_total
            } else {
                return Err(TxRuleError::InputAmountOverflow);
            }

            if total > MAX_SOMPI {
                return Err(TxRuleError::InputAmountTooHigh);
            }
        }

        Ok(total)
    }

    fn check_transaction_output_values(tx: &impl VerifiableTransaction, total_in: u64) -> TxResult<u64> {
        // There's no need to check for overflow here because it was already checked by check_transaction_output_value_ranges
        let total_out: u64 = tx.outputs().iter().map(|out| out.value).sum();
        if total_in < total_out {
            return Err(TxRuleError::SpendTooHigh(total_out, total_in));
        }

        Ok(total_out)
    }

    fn check_sequence_lock(tx: &impl VerifiableTransaction, pov_daa_score: u64) -> TxResult<()> {
        let pov_daa_score: i64 = pov_daa_score as i64;
        if tx.populated_inputs().filter(|(input, _)| input.sequence & SEQUENCE_LOCK_TIME_DISABLED != SEQUENCE_LOCK_TIME_DISABLED).any(
            |(input, entry)| {
                // Given a sequence number, we apply the relative time lock
                // mask in order to obtain the time lock delta required before
                // this input can be spent.
                let relative_lock = (input.sequence & SEQUENCE_LOCK_TIME_MASK) as i64;

                // The relative lock-time for this input is expressed
                // in blocks so we calculate the relative offset from
                // the input's DAA score as its converted absolute
                // lock-time. We subtract one from the relative lock in
                // order to maintain the original lockTime semantics.
                //
                // Note: in the kaspad codebase there's a use in i64 in order to use the -1 value
                // as None. Here it's not needed, but we still use it to avoid breaking consensus.
                let lock_daa_score = entry.block_daa_score as i64 + relative_lock - 1;

                lock_daa_score >= pov_daa_score
            },
        ) {
            return Err(TxRuleError::SequenceLockConditionsAreNotMet);
        }
        Ok(())
    }

    fn check_sig_op_counts(_tx: &impl VerifiableTransaction) -> TxResult<()> {
        // TODO: Implement this
        Ok(())
    }

    fn check_scripts(&self, tx: &impl VerifiableTransaction) -> TxResult<()> {
        let mut reused_values = SigHashReusedValues::new();
        for (i, (input, entry)) in tx.populated_inputs().enumerate() {
            // TODO: this is a temporary implementation and not ready for consensus since any invalid signature
            // will crash the node. We need to replace it with a proper script engine once it's ready.
            let pk = &entry.script_public_key.script()[1..33];
            let pk = secp256k1::XOnlyPublicKey::from_slice(pk).unwrap();
            let sig = secp256k1::schnorr::Signature::from_slice(&input.signature_script[1..65]).unwrap();
            let sig_hash = calc_schnorr_signature_hash(tx, i, SIG_HASH_ALL, &mut reused_values);
            let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
            let sig_cache_key = SigCacheKey { signature: sig, pub_key: pk, message: msg };
            match self.sig_cache.get(&sig_cache_key) {
                Some(valid) => {
                    assert!(valid, "invalid signature in sig cache");
                }
                None => {
                    // TODO: Find a way to parallelize this part. This will be less trivial
                    // once this code is inside the script engine.
                    sig.verify(&msg, &pk).unwrap();
                    self.sig_cache.insert(sig_cache_key, true);
                }
            }
        }

        Ok(())
    }
}
