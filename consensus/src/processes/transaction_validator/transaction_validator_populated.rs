use crate::constants::{MAX_SOMPI, SEQUENCE_LOCK_TIME_DISABLED, SEQUENCE_LOCK_TIME_MASK};
use kaspa_consensus_core::hashing::sighash::{SigHashReusedValues, SigHashReusedValuesSync};
use kaspa_consensus_core::{
    hashing::sighash::SigHashReusedValuesUnsync,
    mass::Kip9Version,
    tx::{TransactionInput, VerifiableTransaction},
};
use kaspa_core::warn;
use kaspa_txscript::caches::Cache;
use kaspa_txscript::{get_sig_op_count, SigCacheKey, TxScriptEngine};
use kaspa_txscript_errors::TxScriptError;
use rayon::iter::IntoParallelIterator;
use rayon::ThreadPool;
use std::sync::Arc;

use super::{
    errors::{TxResult, TxRuleError},
    TransactionValidator,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TxValidationFlags {
    /// Perform full validation including script verification
    Full,

    /// Perform fee and sequence/maturity validations but skip script checks. This is usually
    /// an optimization to be applied when it is known that scripts were already checked
    SkipScriptChecks,

    /// When validating mempool transactions, we just set this value ourselves
    SkipMassCheck,
}

impl TransactionValidator {
    pub fn validate_populated_transaction_and_get_fee(
        &self,
        tx: &(impl VerifiableTransaction + std::marker::Sync),
        pov_daa_score: u64,
        flags: TxValidationFlags,
        mass_and_feerate_threshold: Option<(u64, f64)>,
    ) -> TxResult<u64> {
        self.check_transaction_coinbase_maturity(tx, pov_daa_score)?;
        let total_in = self.check_transaction_input_amounts(tx)?;
        let total_out = Self::check_transaction_output_values(tx, total_in)?;
        let fee = total_in - total_out;
        if flags != TxValidationFlags::SkipMassCheck && pov_daa_score > self.storage_mass_activation_daa_score {
            // Storage mass hardfork was activated
            self.check_mass_commitment(tx)?;

            if pov_daa_score < self.storage_mass_activation_daa_score + 10 && self.storage_mass_activation_daa_score > 0 {
                warn!("--------- Storage mass hardfork was activated successfully!!! --------- (DAA score: {})", pov_daa_score);
            }
        }
        Self::check_sequence_lock(tx, pov_daa_score)?;

        // The following call is not a consensus check (it could not be one in the first place since it uses floating number)
        // but rather a mempool Replace by Fee validation rule. It was placed here purposely for avoiding unneeded script checks.
        Self::check_feerate_threshold(fee, mass_and_feerate_threshold)?;

        match flags {
            TxValidationFlags::Full | TxValidationFlags::SkipMassCheck => {
                Self::check_sig_op_counts::<_, SigHashReusedValuesUnsync>(tx)?;
                self.check_scripts(tx)?;
            }
            TxValidationFlags::SkipScriptChecks => {}
        }
        Ok(fee)
    }

    fn check_feerate_threshold(fee: u64, mass_and_feerate_threshold: Option<(u64, f64)>) -> TxResult<()> {
        // An actual check can only occur if some mass and threshold are provided,
        // otherwise, the check does not verify anything and exits successfully.
        if let Some((contextual_mass, feerate_threshold)) = mass_and_feerate_threshold {
            assert!(contextual_mass > 0);
            if fee as f64 / contextual_mass as f64 <= feerate_threshold {
                return Err(TxRuleError::FeerateTooLow);
            }
        }
        Ok(())
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

    fn check_mass_commitment(&self, tx: &impl VerifiableTransaction) -> TxResult<()> {
        let calculated_contextual_mass =
            self.mass_calculator.calc_tx_overall_mass(tx, None, Kip9Version::Alpha).ok_or(TxRuleError::MassIncomputable)?;
        let committed_contextual_mass = tx.tx().mass();
        if committed_contextual_mass != calculated_contextual_mass {
            return Err(TxRuleError::WrongMass(calculated_contextual_mass, committed_contextual_mass));
        }
        Ok(())
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

    fn check_sig_op_counts<T: VerifiableTransaction, Reused: SigHashReusedValues>(tx: &T) -> TxResult<()> {
        for (i, (input, entry)) in tx.populated_inputs().enumerate() {
            let calculated = get_sig_op_count::<T, Reused>(&input.signature_script, &entry.script_public_key);
            if calculated != input.sig_op_count as u64 {
                return Err(TxRuleError::WrongSigOpCount(i, input.sig_op_count as u64, calculated));
            }
        }
        Ok(())
    }

    pub fn check_scripts(&self, tx: &(impl VerifiableTransaction + std::marker::Sync)) -> TxResult<()> {
        check_scripts(&self.sig_cache, tx)
    }
}

pub fn check_scripts(sig_cache: &Cache<SigCacheKey, bool>, tx: &(impl VerifiableTransaction + Sync)) -> TxResult<()> {
    if tx.inputs().len() > 1 {
        check_scripts_par_iter(sig_cache, tx)
    } else {
        check_scripts_single_threaded(sig_cache, tx)
    }
}

pub fn check_scripts_single_threaded(sig_cache: &Cache<SigCacheKey, bool>, tx: &impl VerifiableTransaction) -> TxResult<()> {
    let reused_values = SigHashReusedValuesUnsync::new();
    for (i, (input, entry)) in tx.populated_inputs().enumerate() {
        let mut engine = TxScriptEngine::from_transaction_input(tx, input, i, entry, &reused_values, sig_cache)
            .map_err(|err| map_script_err(err, input))?;
        engine.execute().map_err(|err| map_script_err(err, input))?;
    }
    Ok(())
}

pub fn check_scripts_par_iter(
    sig_cache: &Cache<SigCacheKey, bool>,
    tx: &(impl VerifiableTransaction + std::marker::Sync),
) -> TxResult<()> {
    use rayon::iter::ParallelIterator;
    let reused_values = std::sync::Arc::new(SigHashReusedValuesSync::new());
    (0..tx.inputs().len())
        .into_par_iter()
        .try_for_each(|idx| {
            let reused_values = reused_values.clone(); // Clone the Arc to share ownership
            let (input, utxo) = tx.populated_input(idx);
            let mut engine = TxScriptEngine::from_transaction_input(tx, input, idx, utxo, &reused_values, sig_cache)?;
            engine.execute()
        })
        .map_err(TxRuleError::SignatureInvalid)
}

pub fn check_scripts_par_iter_thread(
    sig_cache: &Cache<SigCacheKey, bool>,
    tx: &(impl VerifiableTransaction + std::marker::Sync),
    pool: &ThreadPool,
) -> TxResult<()> {
    use rayon::iter::ParallelIterator;
    pool.install(|| {
        let reused_values = Arc::new(SigHashReusedValuesSync::new());
        (0..tx.inputs().len())
            .into_par_iter()
            .try_for_each(|idx| {
                let reused_values = reused_values.clone(); // Clone the Arc to share ownership
                let (input, utxo) = tx.populated_input(idx);
                let mut engine = TxScriptEngine::from_transaction_input(tx, input, idx, utxo, &reused_values, sig_cache)?;
                engine.execute()
            })
            .map_err(TxRuleError::SignatureInvalid)
    })
}

fn map_script_err(script_err: TxScriptError, input: &TransactionInput) -> TxRuleError {
    if input.signature_script.is_empty() {
        TxRuleError::SignatureEmpty(script_err)
    } else {
        TxRuleError::SignatureInvalid(script_err)
    }
}

#[cfg(test)]
mod tests {
    use super::super::errors::TxRuleError;
    use core::str::FromStr;
    use itertools::Itertools;
    use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
    use kaspa_consensus_core::sign::sign;
    use kaspa_consensus_core::subnets::SubnetworkId;
    use kaspa_consensus_core::tx::{MutableTransaction, PopulatedTransaction, ScriptVec, TransactionId, UtxoEntry};
    use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
    use kaspa_txscript_errors::TxScriptError;
    use secp256k1::Secp256k1;
    use smallvec::SmallVec;
    use std::iter::once;

    use crate::{params::MAINNET_PARAMS, processes::transaction_validator::TransactionValidator};

    #[test]
    fn check_signature_test() {
        let mut params = MAINNET_PARAMS.clone();
        params.max_tx_inputs = 10;
        params.max_tx_outputs = 15;
        let tv = TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.ghostdag_k,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity,
            Default::default(),
        );

        let prev_tx_id = TransactionId::from_str("746915c8dfc5e1550eacbe1d87625a105750cf1a65aaddd1baa60f8bcf7e953c").unwrap();

        let mut bytes = [0u8; 66];
        faster_hex::hex_decode("4176cf2ee56b3eed1e8da083851f41cae11532fc70a63ca1ca9f17bc9a4c2fd3dcdf60df1c1a57465f0d112995a6f289511c8e0a79c806fb79165544a439d11c0201".as_bytes(), &mut bytes).unwrap();
        let signature_script = bytes.to_vec();

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
                TransactionOutput { value: 10360487799, script_public_key: ScriptPublicKey::new(0, script_pub_key_2) },
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
        let tv = TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.ghostdag_k,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity,
            Default::default(),
        );

        // Taken from: 3f582463d73c77d93f278b7bf649bd890e75fe9bb8a1edd7a6854df1a2a2bfc1
        let prev_tx_id = TransactionId::from_str("746915c8dfc5e1550eacbe1d87625a105750cf1a65aaddd1baa60f8bcf7e953c").unwrap();

        let mut bytes = [0u8; 66];
        faster_hex::hex_decode("4176cf2ee56b3eed1e8da083851f41cae11532fc70a63ca1ca9f17bc9a4c2fd3dcdf60df1c1a57465f0d112995a6f289511c8e0a79c806fb79165544a439d11c0201".as_bytes(), &mut bytes).unwrap();
        let signature_script = bytes.to_vec();

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
                TransactionOutput { value: 10518958752, script_public_key: ScriptPublicKey::new(0, script_pub_key_1) },
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

    #[test]
    fn check_multi_signature_test() {
        let mut params = MAINNET_PARAMS.clone();
        params.max_tx_inputs = 10;
        params.max_tx_outputs = 15;
        let tv = TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.ghostdag_k,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity,
            Default::default(),
        );

        // Taken from: d839d29b549469d0f9a23e51febe68d4084967a6a477868b511a5a8d88c5ae06
        let prev_tx_id = TransactionId::from_str("63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff84").unwrap();

        let mut bytes = [0u8; 269];
        faster_hex::hex_decode("41ca6f8d104b47ca8ab133d98b3794b49f00ec5d2dce8253e78de035dfbc8f40a2fefa3086c3a181d9f1755a8f4ada4f8a4b8982b361853c8020009e1a752debce0141fdb58c2c25fcfe37d427967c34700f92e9eb1df0f2f9ff366444d92357ff35a270ee5445287031e4c0f72acda20876ccf918de1039a41e9b5f83b3737223f995014c875220ecdd9ec9f2c53ed8e5a170cc88354e133299022da55e1e8bd3c61d8b9dcbd7df2068f191b6aca3d9d8cfa2edb0c44a10fc87dc36b62e1d02228257ccdf979b1fce20b1503ef14aa6773ba3a1f012dbea2992e181766c35c5bc17465b5f57807540bf2006e161ced6b77c11b9a317080a899121a9c6df30a76490402f9a3b7e18bce97b54ae".as_bytes(), &mut bytes).unwrap();
        let signature_script = bytes.to_vec();

        let mut bytes = [0u8; 35];
        faster_hex::hex_decode("aa2071b6c2c604a8830a1484ba469e845c37bb0af32f044bc8fd0c892c8878419e8587".as_bytes(), &mut bytes)
            .unwrap();
        let script_pub_key_1 = SmallVec::from(bytes.to_vec());

        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("206c376f9da440494e18b283803698ed13249af93be3e99f58f42d7d82744d3d15ac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_2 = SmallVec::from(bytes.to_vec());

        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script,
                sequence: 0,
                sig_op_count: 4,
            }],
            vec![
                TransactionOutput { value: 10000000000000, script_public_key: ScriptPublicKey::new(0, script_pub_key_2) },
                TransactionOutput { value: 2792999990000, script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()) },
            ],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let populated_tx = PopulatedTransaction::new(
            &tx,
            vec![UtxoEntry {
                amount: 12793000000000,
                script_public_key: ScriptPublicKey::new(0, script_pub_key_1),
                block_daa_score: 36151168,
                is_coinbase: false,
            }],
        );
        tv.check_scripts(&populated_tx).expect("Signature check failed");
    }

    #[test]
    fn check_last_sig_incorrect_multi_signature_test() {
        let mut params = MAINNET_PARAMS.clone();
        params.max_tx_inputs = 10;
        params.max_tx_outputs = 15;
        let tv = TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.ghostdag_k,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity,
            Default::default(),
        );

        // Taken from: d839d29b549469d0f9a23e51febe68d4084967a6a477868b511a5a8d88c5ae06
        let prev_tx_id = TransactionId::from_str("63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff84").unwrap();

        let mut bytes = [0u8; 269];
        faster_hex::hex_decode("41ca6f8d104b47ca8ab133d98b3794b49f00ec5d2dce8253e78de035dfbc8f40a2fefa3086c3a181d9f1755a8f4ada4f8a4b8982b361853c8020009e1a752debce0141fdb58c2c25fcfe37d427967c34700f92e9eb1df0f2f9ff366444d92357ff3da270ee5445287031e4c0f72acda20876ccf918de1039a41e9b5f83b3737223f995014c875220ecdd9ec9f2c53ed8e5a170cc88354e133299022da55e1e8bd3c61d8b9dcbd7df2068f191b6aca3d9d8cfa2edb0c44a10fc87dc36b62e1d02228257ccdf979b1fce20b1503ef14aa6773ba3a1f012dbea2992e181766c35c5bc17465b5f57807540bf2006e161ced6b77c11b9a317080a899121a9c6df30a76490402f9a3b7e18bce97b54ae".as_bytes(), &mut bytes).unwrap();
        let signature_script = bytes.to_vec();

        let mut bytes = [0u8; 35];
        faster_hex::hex_decode("aa2071b6c2c604a8830a1484ba469e845c37bb0af32f044bc8fd0c892c8878419e8587".as_bytes(), &mut bytes)
            .unwrap();
        let script_pub_key_1 = SmallVec::from(bytes.to_vec());

        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("206c376f9da440494e18b283803698ed13249af93be3e99f58f42d7d82744d3d15ac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_2 = SmallVec::from(bytes.to_vec());

        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script,
                sequence: 0,
                sig_op_count: 4,
            }],
            vec![
                TransactionOutput { value: 10000000000000, script_public_key: ScriptPublicKey::new(0, script_pub_key_2) },
                TransactionOutput { value: 2792999990000, script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()) },
            ],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let populated_tx = PopulatedTransaction::new(
            &tx,
            vec![UtxoEntry {
                amount: 12793000000000,
                script_public_key: ScriptPublicKey::new(0, script_pub_key_1),
                block_daa_score: 36151168,
                is_coinbase: false,
            }],
        );

        assert!(tv.check_scripts(&populated_tx) == Err(TxRuleError::SignatureInvalid(TxScriptError::NullFail)));
    }

    #[test]
    fn check_first_sig_incorrect_multi_signature_test() {
        let mut params = MAINNET_PARAMS.clone();
        params.max_tx_inputs = 10;
        params.max_tx_outputs = 15;
        let tv = TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.ghostdag_k,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity,
            Default::default(),
        );

        // Taken from: d839d29b549469d0f9a23e51febe68d4084967a6a477868b511a5a8d88c5ae06
        let prev_tx_id = TransactionId::from_str("63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff84").unwrap();

        let mut bytes = [0u8; 269];
        faster_hex::hex_decode("41ca6f8d104b47ca8ab133d98b3794b49f00ec5d2dce8253e78de035dfbc8f41a2fefa3086c3a181d9f1755a8f4ada4f8a4b8982b361853c8020009e1a752debce0141fdb58c2c25fcfe37d427967c34700f92e9eb1df0f2f9ff366444d92357ff35a270ee5445287031e4c0f72acda20876ccf918de1039a41e9b5f83b3737223f995014c875220ecdd9ec9f2c53ed8e5a170cc88354e133299022da55e1e8bd3c61d8b9dcbd7df2068f191b6aca3d9d8cfa2edb0c44a10fc87dc36b62e1d02228257ccdf979b1fce20b1503ef14aa6773ba3a1f012dbea2992e181766c35c5bc17465b5f57807540bf2006e161ced6b77c11b9a317080a899121a9c6df30a76490402f9a3b7e18bce97b54ae".as_bytes(), &mut bytes).unwrap();
        let signature_script = bytes.to_vec();

        let mut bytes = [0u8; 35];
        faster_hex::hex_decode("aa2071b6c2c604a8830a1484ba469e845c37bb0af32f044bc8fd0c892c8878419e8587".as_bytes(), &mut bytes)
            .unwrap();
        let script_pub_key_1 = SmallVec::from(bytes.to_vec());

        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("206c376f9da440494e18b283803698ed13249af93be3e99f58f42d7d82744d3d15ac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_2 = SmallVec::from(bytes.to_vec());

        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script,
                sequence: 0,
                sig_op_count: 4,
            }],
            vec![
                TransactionOutput { value: 10000000000000, script_public_key: ScriptPublicKey::new(0, script_pub_key_2) },
                TransactionOutput { value: 2792999990000, script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()) },
            ],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let populated_tx = PopulatedTransaction::new(
            &tx,
            vec![UtxoEntry {
                amount: 12793000000000,
                script_public_key: ScriptPublicKey::new(0, script_pub_key_1),
                block_daa_score: 36151168,
                is_coinbase: false,
            }],
        );

        assert!(tv.check_scripts(&populated_tx) == Err(TxRuleError::SignatureInvalid(TxScriptError::NullFail)));
    }

    #[test]
    fn check_empty_incorrect_multi_signature_test() {
        let mut params = MAINNET_PARAMS.clone();
        params.max_tx_inputs = 10;
        params.max_tx_outputs = 15;
        let tv = TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.ghostdag_k,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity,
            Default::default(),
        );

        // Taken from: d839d29b549469d0f9a23e51febe68d4084967a6a477868b511a5a8d88c5ae06
        let prev_tx_id = TransactionId::from_str("63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff84").unwrap();

        let mut bytes = [0u8; 139];
        faster_hex::hex_decode("00004c875220ecdd9ec9f2c53ed8e5a170cc88354e133299022da55e1e8bd3c61d8b9dcbd7df2068f191b6aca3d9d8cfa2edb0c44a10fc87dc36b62e1d02228257ccdf979b1fce20b1503ef14aa6773ba3a1f012dbea2992e181766c35c5bc17465b5f57807540bf2006e161ced6b77c11b9a317080a899121a9c6df30a76490402f9a3b7e18bce97b54ae".as_bytes(), &mut bytes).unwrap();
        let signature_script = bytes.to_vec();

        let mut bytes = [0u8; 35];
        faster_hex::hex_decode("aa2071b6c2c604a8830a1484ba469e845c37bb0af32f044bc8fd0c892c8878419e8587".as_bytes(), &mut bytes)
            .unwrap();
        let script_pub_key_1 = SmallVec::from(bytes.to_vec());

        let mut bytes = [0u8; 34];
        faster_hex::hex_decode("206c376f9da440494e18b283803698ed13249af93be3e99f58f42d7d82744d3d15ac".as_bytes(), &mut bytes).unwrap();
        let script_pub_key_2 = SmallVec::from(bytes.to_vec());

        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script,
                sequence: 0,
                sig_op_count: 4,
            }],
            vec![
                TransactionOutput { value: 10000000000000, script_public_key: ScriptPublicKey::new(0, script_pub_key_2) },
                TransactionOutput { value: 2792999990000, script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()) },
            ],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let populated_tx = PopulatedTransaction::new(
            &tx,
            vec![UtxoEntry {
                amount: 12793000000000,
                script_public_key: ScriptPublicKey::new(0, script_pub_key_1),
                block_daa_score: 36151168,
                is_coinbase: false,
            }],
        );

        let result = tv.check_scripts(&populated_tx);
        assert!(result == Err(TxRuleError::SignatureInvalid(TxScriptError::EvalFalse)));
    }

    #[test]
    fn check_non_push_only_script_sig_test() {
        // We test a situation where the script itself is valid, but the script signature is not push only
        let params = MAINNET_PARAMS.clone();
        let tv = TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.ghostdag_k,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity,
            Default::default(),
        );

        let prev_tx_id = TransactionId::from_str("1111111111111111111111111111111111111111111111111111111111111111").unwrap();

        let mut bytes = [0u8; 2];
        faster_hex::hex_decode("5175".as_bytes(), &mut bytes).unwrap(); // OP_TRUE OP_DROP
        let signature_script = bytes.to_vec();

        let mut bytes = [0u8; 1];
        faster_hex::hex_decode("51".as_bytes(), &mut bytes) // OP_TRUE
            .unwrap();
        let script_pub_key_1 = SmallVec::from(bytes.to_vec());

        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script,
                sequence: 0,
                sig_op_count: 4,
            }],
            vec![TransactionOutput { value: 2792999990000, script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()) }],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let populated_tx = PopulatedTransaction::new(
            &tx,
            vec![UtxoEntry {
                amount: 12793000000000,
                script_public_key: ScriptPublicKey::new(0, script_pub_key_1),
                block_daa_score: 36151168,
                is_coinbase: false,
            }],
        );

        let result = tv.check_scripts(&populated_tx);
        assert!(result == Err(TxRuleError::SignatureInvalid(TxScriptError::SignatureScriptNotPushOnly)));
    }

    #[test]
    fn test_sign() {
        let params = MAINNET_PARAMS.clone();
        let tv = TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.ghostdag_k,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity,
            Default::default(),
        );

        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let (public_key, _) = public_key.x_only_public_key();
        let script_pub_key = once(0x20).chain(public_key.serialize()).chain(once(0xac)).collect_vec();
        let script_pub_key = ScriptVec::from_slice(&script_pub_key);

        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
        let unsigned_tx = Transaction::new(
            0,
            vec![
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                    signature_script: vec![],
                    sequence: 0,
                    sig_op_count: 0,
                },
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 1 },
                    signature_script: vec![],
                    sequence: 1,
                    sig_op_count: 0,
                },
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 2 },
                    signature_script: vec![],
                    sequence: 2,
                    sig_op_count: 0,
                },
            ],
            vec![
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) },
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) },
            ],
            1615462089000,
            SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let entries = vec![
            UtxoEntry {
                amount: 100,
                script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()),
                block_daa_score: 0,
                is_coinbase: false,
            },
            UtxoEntry {
                amount: 200,
                script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()),
                block_daa_score: 0,
                is_coinbase: false,
            },
            UtxoEntry {
                amount: 300,
                script_public_key: ScriptPublicKey::new(0, script_pub_key),
                block_daa_score: 0,
                is_coinbase: false,
            },
        ];
        let schnorr_key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key.secret_bytes()).unwrap();
        let signed_tx = sign(MutableTransaction::with_entries(unsigned_tx, entries), schnorr_key);
        let populated_tx = signed_tx.as_verifiable();
        assert_eq!(tv.check_scripts(&populated_tx), Ok(()));
        assert_eq!(TransactionValidator::check_sig_op_counts::<_, SigHashReusedValuesUnsync>(&populated_tx), Ok(()));
    }
}
