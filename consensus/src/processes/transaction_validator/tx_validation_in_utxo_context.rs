use crate::constants::{MAX_SOMPI, SEQUENCE_LOCK_TIME_DISABLED, SEQUENCE_LOCK_TIME_MASK};
use kaspa_consensus_core::{
    hashing::sighash::{SigHashReusedValuesSync, SigHashReusedValuesUnsync},
    mass::{Gram, free_script_units_per_input},
    tx::{TransactionInput, TxInputMass, VerifiableTransaction},
};
use kaspa_txscript::{
    EngineCtx, EngineCtxSync, EngineCtxUnsync, EngineFlags, SeqCommitAccessor, TxScriptEngine, covenants::CovenantsContext,
};
use kaspa_txscript_errors::TxScriptError;
use rayon::ThreadPool;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::marker::Sync;

use super::{
    TransactionValidator,
    errors::{TxResult, TxRuleError},
};

/// The threshold above which we apply parallelism to input script processing
const CHECK_SCRIPTS_PARALLELISM_THRESHOLD: usize = 1;

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
        tx: &(impl VerifiableTransaction + Sync),
        pov_daa_score: u64,
        block_daa_score: u64,
        flags: TxValidationFlags,
        mass_and_feerate_threshold: Option<(u64, f64)>,
        seq_commit_accessor: Option<&dyn SeqCommitAccessor>,
    ) -> TxResult<u64> {
        self.check_transaction_coinbase_maturity(tx, pov_daa_score)?;
        let total_in = self.check_transaction_input_amounts(tx)?;
        let total_out = Self::check_transaction_output_values(tx, total_in)?;
        let fee = total_in - total_out;
        if flags != TxValidationFlags::SkipMassCheck {
            self.check_mass_commitment(tx)?;
        }
        Self::check_sequence_lock(tx, pov_daa_score)?;

        // The following call is not a consensus check (it could not be one in the first place since it uses a floating number)
        // but rather a mempool Replace by Fee validation rule. It is placed here purposely for avoiding unneeded script checks.
        Self::check_feerate_threshold(fee, mass_and_feerate_threshold)?;
        let covenants_ctx = self.check_covenant_info(tx, block_daa_score)?;

        match flags {
            TxValidationFlags::Full | TxValidationFlags::SkipMassCheck => {
                self.check_scripts(tx, covenants_ctx, block_daa_score, seq_commit_accessor)?;
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
            self.mass_calculator.calc_contextual_masses(tx).ok_or(TxRuleError::MassIncomputable)?.storage_mass;
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

    pub fn check_scripts(
        &self,
        tx: &(impl VerifiableTransaction + Sync),
        covenants_ctx: CovenantsContext,
        block_daa_score: u64,
        seq_commit_accessor: Option<&dyn SeqCommitAccessor>,
    ) -> TxResult<()> {
        let ctx = EngineCtx::new(&self.sig_cache).with_covenants_ctx(&covenants_ctx).with_seq_commit_accessor_opt(seq_commit_accessor);
        let covenants_enabled = self.covenants_activation.is_active(block_daa_score);
        let flags: EngineFlags =
            EngineFlags { covenants_enabled, sigop_script_units: Gram(self.mass_per_sig_op).to_script_units().value() };

        check_scripts(tx, ctx, flags)
    }

    fn check_covenant_info(&self, tx: &impl VerifiableTransaction, block_daa_score: u64) -> TxResult<CovenantsContext> {
        if !self.covenants_activation.is_active(block_daa_score) {
            return Ok(Default::default());
        }

        Ok(CovenantsContext::from_tx(tx)?)
    }
}

pub fn check_scripts(tx: &(impl VerifiableTransaction + Sync), ctx: EngineCtx<'_>, flags: EngineFlags) -> TxResult<()> {
    if tx.inputs().len() > CHECK_SCRIPTS_PARALLELISM_THRESHOLD {
        let reused_values = SigHashReusedValuesSync::new();
        check_scripts_par_iter(tx, ctx.with_reused(&reused_values), flags)
    } else {
        let reused_values = SigHashReusedValuesUnsync::new();
        check_scripts_sequential(tx, ctx.with_reused(&reused_values), flags)
    }
}

#[inline]
fn input_allowed_script_units(input: &TransactionInput, flags: EngineFlags) -> u64 {
    match input.mass {
        TxInputMass::SigopCount(count) => {
            (u8::from(count) as u64).saturating_mul(flags.sigop_script_units).saturating_add(free_script_units_per_input().value())
        }
        TxInputMass::ComputeBudget(compute_budget) => compute_budget.allowed_script_units().value(),
    }
}

pub fn check_scripts_sequential(tx: &impl VerifiableTransaction, ctx: EngineCtxUnsync<'_>, flags: EngineFlags) -> TxResult<()> {
    for (i, (input, entry)) in tx.populated_inputs().enumerate() {
        let allowed_script_units = input_allowed_script_units(input, flags);
        let mut vm =
            TxScriptEngine::from_transaction_input_with_allowed_script_units(tx, input, i, entry, ctx, flags, allowed_script_units);
        vm.execute().map_err(|err| map_script_err(err, input))?;
    }
    Ok(())
}

pub fn check_scripts_par_iter(tx: &(impl VerifiableTransaction + Sync), ctx: EngineCtxSync<'_>, flags: EngineFlags) -> TxResult<()> {
    (0..tx.inputs().len()).into_par_iter().try_for_each(|idx| {
        let (input, utxo) = tx.populated_input(idx);
        let allowed_script_units = input_allowed_script_units(input, flags);
        let mut vm =
            TxScriptEngine::from_transaction_input_with_allowed_script_units(tx, input, idx, utxo, ctx, flags, allowed_script_units);
        vm.execute().map_err(|err| map_script_err(err, input))
    })
}

pub fn check_scripts_par_iter_pool(
    tx: &(impl VerifiableTransaction + Sync),
    ctx: EngineCtxSync<'_>,
    flags: EngineFlags,
    pool: &ThreadPool,
) -> TxResult<()> {
    pool.install(|| check_scripts_par_iter(tx, ctx, flags))
}

fn map_script_err(script_err: TxScriptError, input: &TransactionInput) -> TxRuleError {
    if input.signature_script.is_empty() { TxRuleError::SignatureEmpty(script_err) } else { TxRuleError::SignatureInvalid(script_err) }
}

#[cfg(test)]
mod tests {
    use super::super::errors::TxRuleError;
    use super::{CHECK_SCRIPTS_PARALLELISM_THRESHOLD, TxValidationFlags, check_scripts};
    use crate::{params::MAINNET_PARAMS, processes::transaction_validator::TransactionValidator};
    use core::str::FromStr;
    use itertools::Itertools;
    use kaspa_consensus_core::mass::{ComputeBudget, free_script_units_per_input};
    use kaspa_consensus_core::sign::sign;
    use kaspa_consensus_core::subnets::SubnetworkId;
    use kaspa_consensus_core::tx::{
        MutableTransaction, PopulatedTransaction, ScriptPublicKey, ScriptVec, Transaction, TransactionId, TransactionInput,
        TransactionOutpoint, TransactionOutput, TxInputMass, UtxoEntry,
    };
    use kaspa_consensus_core::{
        config::params::ForkActivation,
        mass::{GRAMS_PER_COMPUTE_BUDGET_UNIT, MassCalculator, SCRIPT_UNITS_PER_COMPUTE_BUDGET_UNIT},
    };
    use kaspa_core::assert_match;
    use kaspa_txscript::opcodes::codes::OpDup;
    use kaspa_txscript::{
        EngineCtx, EngineFlags,
        opcodes::codes::{OpCheckSig, OpDrop},
        script_builder::ScriptBuilder,
    };
    use kaspa_txscript_errors::TxScriptError;
    use secp256k1::Secp256k1;
    use smallvec::SmallVec;
    use std::iter::once;

    /// Helper function to duplicate the last input
    fn duplicate_input(tx: &Transaction, entries: &[UtxoEntry]) -> (Transaction, Vec<UtxoEntry>) {
        let mut tx2 = tx.clone();
        let mut entries2 = entries.to_owned();
        tx2.inputs.push(tx2.inputs.last().unwrap().clone());
        entries2.push(entries2.last().unwrap().clone());
        (tx2, entries2)
    }

    /// Builds inputs whose script consumes exactly one compute-budget unit worth of script units,
    /// so the test can check per-input budget enforcement around that boundary.
    fn build_parallel_push_budget_test_tx(num_inputs: usize) -> (Transaction, Vec<UtxoEntry>) {
        assert!(num_inputs > CHECK_SCRIPTS_PARALLELISM_THRESHOLD);

        let script_public_key = ScriptPublicKey::new(
            0,
            ScriptBuilder::new()
                .add_data(&vec![1u8; SCRIPT_UNITS_PER_COMPUTE_BUDGET_UNIT as usize])
                .unwrap()
                .add_op(OpDup)
                .unwrap()
                .add_op(OpDrop)
                .unwrap()
                .drain()
                .into(),
        );
        let outputs = vec![TransactionOutput {
            value: 1,
            script_public_key: ScriptPublicKey::new(0, SmallVec::from_slice(&[0x51])),
            covenant: None,
        }];

        let inputs = (0..num_inputs)
            .map(|i| TransactionInput {
                previous_outpoint: TransactionOutpoint {
                    transaction_id: TransactionId::from_bytes([(i as u8).wrapping_add(1); 32]),
                    index: 0,
                },
                signature_script: vec![],
                sequence: 0,
                mass: TxInputMass::ComputeBudget(1.into()),
            })
            .collect_vec();

        let entries = (0..num_inputs)
            .map(|_| UtxoEntry {
                amount: 1,
                script_public_key: script_public_key.clone(),
                block_daa_score: 0,
                is_coinbase: false,
                covenant_id: None,
            })
            .collect_vec();

        (Transaction::new(1, inputs, outputs, 0, SubnetworkId::default(), 0, vec![]), entries)
    }

    #[test]
    fn check_scripts_parallel_budget_behavior() {
        let sig_cache = kaspa_txscript::caches::Cache::new(10_000);
        let flags = EngineFlags { covenants_enabled: true, ..Default::default() };

        // (a) One input alone is over budget when compute_budget=0 (allowed units per input = 999).
        let (mut tx, entries) = build_parallel_push_budget_test_tx(2);
        tx.inputs[0].mass = ComputeBudget(0).into();
        let populated_tx = PopulatedTransaction::new(&tx, entries);
        let result = check_scripts(&populated_tx, EngineCtx::new(&sig_cache), flags);
        assert_eq!(
            result,
            Err(TxRuleError::SignatureEmpty(TxScriptError::ExceededScriptUnitsLimit {
                used_units: SCRIPT_UNITS_PER_COMPUTE_BUDGET_UNIT,
                allowed_units: free_script_units_per_input().value()
            }))
        );

        // (b) A few inputs together are all independently under budget and should pass.
        let (tx, entries) = build_parallel_push_budget_test_tx(3);
        let mut tx = tx;
        tx.inputs.iter_mut().for_each(|input| input.mass = ComputeBudget(1).into());
        let populated_tx = PopulatedTransaction::new(&tx, entries);
        let result = check_scripts(&populated_tx, EngineCtx::new(&sig_cache), flags);
        assert!(result.is_ok());

        // (c) Everything is ok with a larger per-input budget as well.
        let (tx, entries) = build_parallel_push_budget_test_tx(3);
        let mut tx = tx;
        tx.inputs.iter_mut().for_each(|input| input.mass = ComputeBudget(10).into());
        let populated_tx = PopulatedTransaction::new(&tx, entries);
        let result = check_scripts(&populated_tx, EngineCtx::new(&sig_cache), flags);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_populated_transaction_sigop_budget_enforced_for_v0_and_v1_with_covenants_enabled() {
        let params = MAINNET_PARAMS.clone();
        let tv = TransactionValidator::new(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            params.ghostdag_k,
            Default::default(),
            MassCalculator::new(0, 0, 0, 0),
            ForkActivation::always(),
            params.mass_per_sig_op,
        );

        let schnorr_key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &[7u8; 32]).unwrap();
        let (x_only_pubkey, _) = schnorr_key.x_only_public_key();
        let two_sigops_script = ScriptBuilder::new()
            .add_op(OpDup) // We duplicate the signature
            .unwrap()
            .add_data(&x_only_pubkey.serialize())
            .unwrap()
            .add_op(OpCheckSig)
            .unwrap()
            .add_op(OpDrop)
            .unwrap()
            .add_data(&x_only_pubkey.serialize())
            .unwrap()
            .add_op(OpCheckSig)
            .unwrap()
            .drain();

        for version in [0u16, 1u16] {
            let sig_op_count = if version == 0 { 1 } else { 0 };
            let compute_budget = if version == 1 { (params.mass_per_sig_op / GRAMS_PER_COMPUTE_BUDGET_UNIT) as u16 } else { 0 };

            let input = TransactionInput {
                previous_outpoint: TransactionOutpoint {
                    transaction_id: TransactionId::from_bytes([version as u8 + 1; 32]),
                    index: 0,
                },
                signature_script: vec![],
                sequence: 0,
                mass: if TxInputMass::has_compute_budget_field(version) {
                    TxInputMass::ComputeBudget(compute_budget.into())
                } else {
                    TxInputMass::SigopCount(sig_op_count.into())
                },
            };
            let output = TransactionOutput {
                value: 1,
                script_public_key: ScriptPublicKey::new(0, SmallVec::from_slice(&[0x51])),
                covenant: None,
            };
            let tx = Transaction::new(version, vec![input], vec![output], 0, SubnetworkId::default(), 0, vec![]);

            let utxo_entry = UtxoEntry {
                amount: 1,
                script_public_key: ScriptPublicKey::new(0, SmallVec::from_slice(&two_sigops_script)),
                block_daa_score: 0,
                is_coinbase: false,
                covenant_id: None,
            };

            let signed_tx = sign(MutableTransaction::with_entries(tx, vec![utxo_entry]), schnorr_key);

            // Verify that `sign` didn't change the sig_op_count and compute_budget values.
            assert_eq!(signed_tx.tx.inputs[0].mass.sig_op_count().unwrap_or(0), sig_op_count);
            assert_eq!(signed_tx.tx.inputs[0].mass.compute_budget().unwrap_or(0), compute_budget);

            let verifiable_tx = signed_tx.as_verifiable();

            let result =
                tv.validate_populated_transaction_and_get_fee(&verifiable_tx, 0, 0, TxValidationFlags::SkipMassCheck, None, None);
            assert_match!(
                result,
                Err(TxRuleError::SignatureInvalid(TxScriptError::ExceededScriptUnitsLimit { .. })),
                "expected sigop budget enforcement for tx version {version}"
            );
        }
    }

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
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            params.ghostdag_k(),
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
                mass: TxInputMass::SigopCount(1.into()),
            }],
            vec![
                TransactionOutput { value: 10360487799, script_public_key: ScriptPublicKey::new(0, script_pub_key_2), covenant: None },
                TransactionOutput {
                    value: 10518958752,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()),
                    covenant: None,
                },
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
                covenant_id: None,
            }],
        );

        let flags = Default::default();
        tv.check_scripts(&populated_tx, Default::default(), flags, None).expect("Signature check failed");

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        // Duplicated sigs should fail due to wrong sighash
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2), Default::default(), flags, None),
            Err(TxRuleError::SignatureInvalid(TxScriptError::EvalFalse))
        );
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
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            params.ghostdag_k(),
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
                mass: TxInputMass::SigopCount(1.into()),
            }],
            vec![
                TransactionOutput {
                    value: 10360487799,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_2.clone()),
                    covenant: None,
                },
                TransactionOutput { value: 10518958752, script_public_key: ScriptPublicKey::new(0, script_pub_key_1), covenant: None },
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
                covenant_id: None,
            }],
        );

        let flags = Default::default();
        assert!(tv.check_scripts(&populated_tx, Default::default(), flags, None).is_err(), "Expecting signature check to fail");

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2), Default::default(), flags, None)
            .expect_err("Expecting signature check to fail");

        // Verify we are correctly testing the parallelism case (applied here as sanity for all tests)
        assert!(
            tx2.inputs.len() > CHECK_SCRIPTS_PARALLELISM_THRESHOLD,
            "The script tests must cover the case of a tx with inputs.len() > {}",
            CHECK_SCRIPTS_PARALLELISM_THRESHOLD
        );
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
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            params.ghostdag_k(),
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
                mass: TxInputMass::SigopCount(4.into()),
            }],
            vec![
                TransactionOutput {
                    value: 10000000000000,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_2),
                    covenant: None,
                },
                TransactionOutput {
                    value: 2792999990000,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()),
                    covenant: None,
                },
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
                covenant_id: None,
            }],
        );

        let flags = Default::default();
        tv.check_scripts(&populated_tx, Default::default(), flags, None).expect("Signature check failed");

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        // Duplicated sigs should fail due to wrong sighash
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2), Default::default(), flags, None),
            Err(TxRuleError::SignatureInvalid(TxScriptError::NullFail))
        );
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
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            params.ghostdag_k(),
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
                mass: TxInputMass::SigopCount(4.into()),
            }],
            vec![
                TransactionOutput {
                    value: 10000000000000,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_2),
                    covenant: None,
                },
                TransactionOutput {
                    value: 2792999990000,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()),
                    covenant: None,
                },
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
                covenant_id: None,
            }],
        );

        let flags = Default::default();
        assert_eq!(
            tv.check_scripts(&populated_tx, Default::default(), flags, None),
            Err(TxRuleError::SignatureInvalid(TxScriptError::NullFail))
        );

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2), Default::default(), flags, None),
            Err(TxRuleError::SignatureInvalid(TxScriptError::NullFail))
        );
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
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            params.ghostdag_k(),
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
                mass: TxInputMass::SigopCount(4.into()),
            }],
            vec![
                TransactionOutput {
                    value: 10000000000000,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_2),
                    covenant: None,
                },
                TransactionOutput {
                    value: 2792999990000,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()),
                    covenant: None,
                },
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
                covenant_id: None,
            }],
        );

        let flags = Default::default();
        assert_eq!(
            tv.check_scripts(&populated_tx, Default::default(), flags, None),
            Err(TxRuleError::SignatureInvalid(TxScriptError::NullFail))
        );

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2), Default::default(), flags, None),
            Err(TxRuleError::SignatureInvalid(TxScriptError::NullFail))
        );
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
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            params.ghostdag_k(),
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
                mass: TxInputMass::SigopCount(4.into()),
            }],
            vec![
                TransactionOutput {
                    value: 10000000000000,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_2),
                    covenant: None,
                },
                TransactionOutput {
                    value: 2792999990000,
                    script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()),
                    covenant: None,
                },
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
                covenant_id: None,
            }],
        );

        let flags = Default::default();
        assert_eq!(
            tv.check_scripts(&populated_tx, Default::default(), flags, None),
            Err(TxRuleError::SignatureInvalid(TxScriptError::EvalFalse))
        );

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2), Default::default(), flags, None),
            Err(TxRuleError::SignatureInvalid(TxScriptError::EvalFalse))
        );
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
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            params.ghostdag_k(),
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
                mass: TxInputMass::SigopCount(4.into()),
            }],
            vec![TransactionOutput {
                value: 2792999990000,
                script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()),
                covenant: None,
            }],
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
                covenant_id: None,
            }],
        );

        let flags = Default::default();
        assert_eq!(
            tv.check_scripts(&populated_tx, Default::default(), flags, None),
            Err(TxRuleError::SignatureInvalid(TxScriptError::SignatureScriptNotPushOnly))
        );

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2), Default::default(), flags, None),
            Err(TxRuleError::SignatureInvalid(TxScriptError::SignatureScriptNotPushOnly))
        );
    }

    #[test]
    fn test_sign() {
        let params = MAINNET_PARAMS.clone();
        let tv = TransactionValidator::new_for_tests(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity(),
            params.ghostdag_k(),
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
                    mass: TxInputMass::SigopCount(0.into()),
                },
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 1 },
                    signature_script: vec![],
                    sequence: 1,
                    mass: TxInputMass::SigopCount(0.into()),
                },
                TransactionInput {
                    previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 2 },
                    signature_script: vec![],
                    sequence: 2,
                    mass: TxInputMass::SigopCount(0.into()),
                },
            ],
            vec![
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()), covenant: None },
                TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()), covenant: None },
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
                covenant_id: None,
            },
            UtxoEntry {
                amount: 200,
                script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()),
                block_daa_score: 0,
                is_coinbase: false,
                covenant_id: None,
            },
            UtxoEntry {
                amount: 300,
                script_public_key: ScriptPublicKey::new(0, script_pub_key),
                block_daa_score: 0,
                is_coinbase: false,
                covenant_id: None,
            },
        ];
        let schnorr_key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key.secret_bytes()).unwrap();
        let signed_tx = sign(MutableTransaction::with_entries(unsigned_tx, entries), schnorr_key);
        let populated_tx = signed_tx.as_verifiable();
        assert_eq!(tv.check_scripts(&populated_tx, Default::default(), Default::default(), None), Ok(()));
    }
}
