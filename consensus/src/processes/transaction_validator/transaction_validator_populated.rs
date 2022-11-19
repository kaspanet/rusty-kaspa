use crate::constants::{MAX_SOMPI, SEQUENCE_LOCK_TIME_DISABLED, SEQUENCE_LOCK_TIME_MASK};
use consensus_core::{hashing::sighash::SigHashReusedValues, tx::PopulatedTransaction};
use txscript::TxScriptEngine;

use super::{
    errors::{TxResult, TxRuleError},
    TransactionValidator,
};

impl TransactionValidator {
    pub fn validate_populated_transaction_and_get_fee(&self, tx: &PopulatedTransaction, pov_daa_score: u64) -> TxResult<u64> {
        self.check_transaction_coinbase_maturity(tx, pov_daa_score)?;
        let total_in = self.check_transaction_input_amounts(tx)?;
        let total_out = Self::check_transaction_output_values(tx, total_in)?;
        Self::check_sequence_lock(tx, pov_daa_score)?;
        Self::check_sig_op_counts(tx)?;
        self.check_scripts(tx)?;

        Ok(total_in - total_out)
    }

    fn check_transaction_coinbase_maturity(&self, tx: &PopulatedTransaction, pov_daa_score: u64) -> TxResult<()> {
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

    fn check_transaction_input_amounts(&self, tx: &PopulatedTransaction) -> TxResult<u64> {
        let mut total: u64 = 0;
        for entry in &tx.entries {
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

    fn check_transaction_output_values(tx: &PopulatedTransaction, total_in: u64) -> TxResult<u64> {
        // There's no need to check for overflow here because it was already checked by check_transaction_output_value_ranges
        let total_out: u64 = tx.outputs().iter().map(|out| out.value).sum();
        if total_in < total_out {
            return Err(TxRuleError::SpendTooHigh(total_out, total_in));
        }

        Ok(total_out)
    }

    fn check_sequence_lock(tx: &PopulatedTransaction, pov_daa_score: u64) -> TxResult<()> {
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

    fn check_sig_op_counts(_tx: &PopulatedTransaction) -> TxResult<()> {
        // TODO: Implement this
        Ok(())
    }

    fn check_scripts(&self, tx: &PopulatedTransaction) -> TxResult<()> {
        let mut reused_values = SigHashReusedValues::new();
        for (i, (input, entry)) in tx.populated_inputs().enumerate() {
            // TODO: this is a temporary implementation and not ready for consensus since any invalid signature
            let mut engine = TxScriptEngine::from_transaction_input(tx, input, i, entry, &mut reused_values, &self.sig_cache)
                .map_err(TxRuleError::SignatureInvalid)?;
            engine.execute().map_err(TxRuleError::SignatureInvalid)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use consensus_core::subnets::SubnetworkId;
    use consensus_core::tx::{PopulatedTransaction, TransactionId, UtxoEntry};
    use consensus_core::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
    use core::str::FromStr;
    use smallvec::SmallVec;

    use crate::{params::MAINNET_PARAMS, processes::transaction_validator::TransactionValidator};

    #[test]
    fn check_signature_test() {
        let mut params = MAINNET_PARAMS.clone();
        params.max_tx_inputs = 10;
        params.max_tx_outputs = 15;
        let tv = TransactionValidator::new(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.ghostdag_k,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity,
        );

        let prev_tx_id = TransactionId::from_str("746915c8dfc5e1550eacbe1d87625a105750cf1a65aaddd1baa60f8bcf7e953c").unwrap();

        let mut bytes = [0u8; 66];
        faster_hex::hex_decode("4176cf2ee56b3eed1e8da083851f41cae11532fc70a63ca1ca9f17bc9a4c2fd3dcdf60df1c1a57465f0d112995a6f289511c8e0a79c806fb79165544a439d11c0201".as_bytes(), &mut bytes).unwrap();
        let signature_script = Vec::from(bytes.to_vec());

        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("20e1d5835e09f3c3dad209debcb7b3bf3fb0e0d9642471f5db36c9ea58338b06beac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_1 = SmallVec::from(bytes.to_vec());

        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("200749c89953b463d1e186a16a941f9354fa3fff313c391149e47961b95dd4df28ac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_2 = SmallVec::from(bytes.to_vec());

        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 1 },
                signature_script,
                sequence: 0,
                sig_op_count: 1,
            }],
            vec![
                TransactionOutput { value: 10360487799, script_public_key: ScriptPublicKey::new(0, script_pub_key_2.clone()) },
                TransactionOutput { value: 10518958752, script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()) },
            ],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let populated_tx = PopulatedTransaction::new(
            &tx,
            vec![UtxoEntry {
                amount: 20879456551,
                script_public_key: ScriptPublicKey::new(0, script_pub_key_1),
                block_daa_score: 32022768,
                is_coinbase: false,
            }],
        );

        tv.check_scripts(&populated_tx).expect("Signature check failed");
    }

    #[test]
    fn check_incorrect_signature_test() {
        let mut params = MAINNET_PARAMS.clone();
        params.max_tx_inputs = 10;
        params.max_tx_outputs = 15;
        let tv = TransactionValidator::new(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.ghostdag_k,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity,
        );

        let prev_tx_id = TransactionId::from_str("746915c8dfc5e1550eacbe1d87625a105750cf1a65aaddd1baa60f8bcf7e953c").unwrap();

        let mut bytes = [0u8; 66];
        faster_hex::hex_decode("4176cf2ee56b3eed1e8da083851f41cae11532fc70a63ca1ca9f17bc9a4c2fd3dcdf60df1c1a57465f0d112995a6f289511c8e0a79c806fb79165544a439d11c0201".as_bytes(), &mut bytes).unwrap();
        let signature_script = Vec::from(bytes.to_vec());

        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("20e1d5835e09f3c3dad209debcb7b3bf3fb0e0d9642471f5db36c9ea58338b06beac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_1 = SmallVec::from(bytes.to_vec());

        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("200749c89953b463d1e186a16a941f9354fa3fff313c391149e47961b95dd4df28ac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_2 = SmallVec::from(bytes.to_vec());

        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 1 },
                signature_script,
                sequence: 0,
                sig_op_count: 1,
            }],
            vec![
                TransactionOutput { value: 10360487799, script_public_key: ScriptPublicKey::new(0, script_pub_key_2.clone()) },
                TransactionOutput { value: 10518958752, script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()) },
            ],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let populated_tx = PopulatedTransaction::new(
            &tx,
            vec![UtxoEntry {
                amount: 20879456551,
                script_public_key: ScriptPublicKey::new(0, script_pub_key_2),
                block_daa_score: 32022768,
                is_coinbase: false,
            }],
        );

        assert!(tv.check_scripts(&populated_tx).is_err(), "Failing Signature Test Failed");
    }
}
