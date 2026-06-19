//!
//! Transaction mass calculator.
//!

use crate::error::Error;
use crate::result::Result;
use kaspa_consensus_client as kcc;
use kaspa_consensus_client::UtxoEntryReference;
use kaspa_consensus_core::mass::{
    ContextualMasses, Mass, MassCalculator as ConsensusMassCalculator, MassCofactors, NonContextualMasses,
    calc_storage_mass as consensus_calc_storage_mass,
};
pub use kaspa_consensus_core::mass::{
    transaction_estimated_serialized_size as transaction_serialized_byte_size,
    transaction_input_estimated_serialized_size as transaction_input_serialized_byte_size,
    transaction_output_estimated_serialized_size as transaction_output_serialized_byte_size,
};
use kaspa_consensus_core::tx::{ComputeCommit, SCRIPT_VECTOR_SIZE, Transaction, TransactionInput, TransactionOutput};
use kaspa_consensus_core::{config::params::Params, constants::*, subnets::SUBNETWORK_ID_SIZE};
use kaspa_hashes::HASH_SIZE;

// pub const ECDSA_SIGNATURE_SIZE: u64 = 64;
// pub const SCHNORR_SIGNATURE_SIZE: u64 = 64;
pub const SIGNATURE_SIZE: u64 = 1 + 64 + 1; //1 byte for OP_DATA_65 + 64 (length of signature) + 1 byte for sig hash type

/// MINIMUM_RELAY_TRANSACTION_FEE specifies the minimum transaction fee for a transaction to be accepted to
/// the mempool and relayed. It is specified in sompi per 1kg (or 1000 grams) of transaction mass.
/// The default is 100 sompi per gram.
pub(crate) const MINIMUM_RELAY_TRANSACTION_FEE_PER_KG: u64 = 100_000;
const MINIMUM_RELAY_TRANSACTION_FEE_PER_GRAM: u64 = MINIMUM_RELAY_TRANSACTION_FEE_PER_KG / 1000;

// TODO(post-toccata): remove this const and cleaup calling site, only use new 500_000 mass limit
/// MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA is the maximum mass allowed for transactions that
/// are considered standard and will therefore be relayed and considered for mining, before toccata activtion.
pub const MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA: u64 = 100_000;

pub const MAXIMUM_STANDARD_TRANSACTION_MASS_POST_TOCCATA: u64 = 500_000;

/// minimum_required_transaction_relay_fee returns the minimum transaction fee required
/// for a transaction with the passed mass to be accepted into the mempool and relayed.
pub fn calc_minimum_required_transaction_relay_fee(mass: u64) -> u64 {
    let mut minimum_fee = mass * MINIMUM_RELAY_TRANSACTION_FEE_PER_GRAM;

    if minimum_fee == 0 {
        minimum_fee = MINIMUM_RELAY_TRANSACTION_FEE_PER_KG;
    }

    // Set the minimum fee to the maximum possible value if the calculated
    // fee is not in the valid range for monetary amounts.
    minimum_fee = minimum_fee.min(MAX_SOMPI);

    minimum_fee
}

/// is_transaction_output_dust returns whether or not the passed transaction output
/// amount is considered dust or not based on the configured minimum transaction
/// relay fee.
///
/// Mempool does not reject dust outputs by threshold, but the wallet still uses this
/// heuristic to avoid creating change outputs that cost more to preserve than they are worth.
pub fn is_transaction_output_dust(transaction_output: &TransactionOutput) -> bool {
    // is spending script greater than P2PK std: 34 for schnorr and 35 for ecdsa
    if transaction_output.script_public_key.script().len() < 33 {
        return true;
    }

    // The total serialized size consists of the output and the associated
    // input script to redeem it. Since there is no input script
    // to redeem it yet, use the minimum size of a typical input script.
    //
    // Pay-to-pubkey bytes breakdown:
    //
    //  Output to pubkey (43 bytes):
    //   8 value, 1 script len, 34 script [1 OP_DATA_32,
    //   32 pubkey, 1 OP_CHECKSIG]
    //
    //  Input (105 bytes):
    //   36 prev outpoint, 1 script len, 64 script [1 OP_DATA_64,
    //   64 sig], 4 sequence
    //
    // The most common scripts are pay-to-pubkey, and as per the above
    // breakdown, the minimum size of a p2pk input script is 148 bytes. So
    // that figure is used.
    let total_serialized_size = transaction_output_serialized_byte_size(transaction_output) + 148;

    // The output is considered dust if the cost to the network to spend the
    // coins is more than 1/3 of the minimum free transaction relay fee.
    // SAFETY: it is guaranteed that `total_serialized_size` is > 0
    transaction_output.value / (3 * total_serialized_size) < MINIMUM_RELAY_TRANSACTION_FEE_PER_GRAM
}

// The most common scripts are pay-to-pubkey, and as per the above
// breakdown, the minimum size of a p2pk input script is 148 bytes. So
// that figure is used.
pub const STANDARD_OUTPUT_SIZE_PLUS_MINIMUM_INPUT_SIZE: u64 = transaction_standard_output_serialized_byte_size() + 148;
pub const STANDARD_OUTPUT_SIZE_PLUS_MINIMUM_INPUT_SIZE_3X: u64 = STANDARD_OUTPUT_SIZE_PLUS_MINIMUM_INPUT_SIZE * 3;

pub const fn blank_transaction_serialized_byte_size() -> u64 {
    let mut size: u64 = 0;
    size += 2; // Tx version (u16)
    size += 8; // Number of inputs (u64)
    // ~ skip input size for blank tx
    size += 8; // number of outputs (u64)
    // ~ skip output size for blank tx
    size += 8; // lock time (u64)
    size += SUBNETWORK_ID_SIZE as u64;
    size += 8; // gas (u64)
    size += HASH_SIZE as u64; // payload hash

    size += 8; // length of the payload (u64)
    // ~ skip payload size for blank tx
    size
}

/// this returns the maximum std output byte size.
///
/// can over-count by one byte because assumes script size is 35 (ecdsa pubkey) which is wrong for schnorr
pub const fn transaction_standard_output_serialized_byte_size() -> u64 {
    let mut size: u64 = 0;
    size += 8; // value (u64)
    size += 2; // output.ScriptPublicKey.Version (u16)
    size += 8; // length of script public key (u64)
    //max script size as per SCRIPT_VECTOR_SIZE
    size += SCRIPT_VECTOR_SIZE as u64;
    size
}

pub struct MassCalculator {
    compute_mass_per_byte: u64,
    mass_per_script_pub_key_byte: u64,
    storage_mass_parameter: u64,
    mempool_mass_cofactors: MassCofactors,
}

impl MassCalculator {
    pub fn new(consensus_params: &Params) -> Self {
        Self {
            compute_mass_per_byte: consensus_params.mass_per_tx_byte,
            mass_per_script_pub_key_byte: consensus_params.mass_per_script_pub_key_byte,
            storage_mass_parameter: consensus_params.storage_mass_parameter,
            mempool_mass_cofactors: consensus_params.mempool_block_mass_cofactors().raw_post(),
        }
    }

    pub fn is_dust(&self, value: u64) -> bool {
        value / STANDARD_OUTPUT_SIZE_PLUS_MINIMUM_INPUT_SIZE_3X < MINIMUM_RELAY_TRANSACTION_FEE_PER_GRAM
    }

    pub(crate) fn blank_transaction_non_contextual_masses(&self) -> NonContextualMasses {
        let size = blank_transaction_serialized_byte_size();
        NonContextualMasses::new(size * self.compute_mass_per_byte, size * TRANSIENT_BYTE_TO_MASS_FACTOR)
    }

    pub(crate) fn calc_non_contextual_masses_for_payload(&self, payload_byte_size: usize) -> NonContextualMasses {
        let size = payload_byte_size as u64;
        NonContextualMasses::new(size * self.compute_mass_per_byte, size * TRANSIENT_BYTE_TO_MASS_FACTOR)
    }

    pub(crate) fn calc_non_contextual_masses_for_client_transaction_outputs(
        &self,
        outputs: &[TransactionOutput],
    ) -> NonContextualMasses {
        outputs
            // not efficient, but simplifies the flow
            .iter()
            .map(|output| self.calc_non_contextual_masses_for_client_transaction_output(output))
            .fold(NonContextualMasses::new(0, 0), |total, current| total + current)
    }

    pub(crate) fn calc_non_contextual_masses_for_client_transaction_output(&self, output: &TransactionOutput) -> NonContextualMasses {
        let size = transaction_output_serialized_byte_size(output);
        // version (u16) + script
        let spk_size = 2 + output.script_public_key.script().len() as u64;
        let compute_mass = size * self.compute_mass_per_byte + spk_size * self.mass_per_script_pub_key_byte;
        let transient_mass = size * TRANSIENT_BYTE_TO_MASS_FACTOR;
        NonContextualMasses::new(compute_mass, transient_mass)
    }

    pub(crate) fn calc_non_contextual_masses_for_client_transaction_input(
        &self,
        input: &TransactionInput,
        version: u16,
        minimum_signatures: u16,
    ) -> NonContextualMasses {
        let signature_size = SIGNATURE_SIZE * minimum_signatures.max(1) as u64;
        let size = transaction_input_serialized_byte_size(input, version) + signature_size;
        let compute_commit_mass = match input.compute_commit {
            ComputeCommit::SigopCount(sig_op_count) => sig_op_count.to_grams().value(),
            ComputeCommit::ComputeBudget(compute_budget) => compute_budget.to_grams().value(),
        };

        NonContextualMasses::new(compute_commit_mass + size * self.compute_mass_per_byte, size * TRANSIENT_BYTE_TO_MASS_FACTOR)
    }

    pub(crate) fn calc_standard_non_contextual_mass(&self, masses: &NonContextualMasses) -> u64 {
        masses.normalized_max(&self.mempool_mass_cofactors)
    }

    pub(crate) fn calc_standard_mass_for_parts(&self, non_contextual: &NonContextualMasses, storage_mass: u64) -> u64 {
        Mass::new(*non_contextual, ContextualMasses::new(storage_mass)).normalized_max(&self.mempool_mass_cofactors)
    }

    pub fn calc_minimum_transaction_fee_from_mass(&self, mass: u64) -> u64 {
        calc_minimum_required_transaction_relay_fee(mass)
    }

    pub fn calc_standard_mass(&self, masses: &Mass) -> u64 {
        masses.normalized_max(&self.mempool_mass_cofactors)
    }

    pub fn calc_minimum_relay_fee(&self, masses: &Mass) -> u64 {
        self.calc_minimum_relay_fee_with_additional_mass(masses, 0)
    }

    pub(crate) fn calc_minimum_relay_fee_with_additional_mass(&self, masses: &Mass, additional_mass: u64) -> u64 {
        let relay_mass = masses.non_contextual.normalized_max(&self.mempool_mass_cofactors).saturating_add(additional_mass);
        self.calc_minimum_transaction_fee_from_mass(relay_mass)
    }

    pub fn calc_unsigned_client_transaction_masses(&self, tx: &kcc::Transaction, minimum_signatures: u16) -> Result<Mass> {
        let cctx = Transaction::from(tx);
        let utxos = tx.utxo_entry_references()?;
        self.calc_unsigned_consensus_transaction_masses(&cctx, &utxos, minimum_signatures)
    }

    pub(crate) fn calc_unsigned_consensus_transaction_masses(
        &self,
        tx: &Transaction,
        utxos: &[UtxoEntryReference],
        minimum_signatures: u16,
    ) -> Result<Mass> {
        let mut estimated_tx = tx.clone();
        let signature_script_len = SIGNATURE_SIZE as usize * minimum_signatures.max(1) as usize;
        for input in estimated_tx.inputs.iter_mut() {
            input.signature_script.resize(input.signature_script.len() + signature_script_len, 0);
        }
        estimated_tx.finalize();
        let non_contextual =
            ConsensusMassCalculator::new(self.compute_mass_per_byte, self.mass_per_script_pub_key_byte, self.storage_mass_parameter)
                .calc_non_contextual_masses(&estimated_tx);
        let storage_mass = self.calc_storage_mass_for_transaction_parts(utxos, &tx.outputs).ok_or(Error::MassCalculationError)?;
        Ok(Mass::new(non_contextual, ContextualMasses::new(storage_mass)))
    }

    pub fn calc_storage_mass_for_transaction(&self, tx: &kcc::Transaction) -> Result<Option<u64>> {
        let utxos = tx.utxo_entry_references()?;
        let outputs = tx.outputs();
        Ok(self.calc_storage_mass_for_transaction_parts(&utxos, &outputs))
    }

    pub fn calc_storage_mass_for_transaction_parts(
        &self,
        inputs: &[UtxoEntryReference],
        outputs: &[TransactionOutput],
    ) -> Option<u64> {
        consensus_calc_storage_mass(
            false,
            inputs.iter().map(|entry| entry.into()),
            outputs.iter().map(|out| out.into()),
            self.storage_mass_parameter,
        )
    }

    pub fn calc_storage_mass_output_harmonic(&self, outputs: &[TransactionOutput]) -> Option<u64> {
        outputs
            .iter()
            .map(|out| self.storage_mass_parameter.checked_div(out.value))
            .try_fold(0u64, |total, current| current.and_then(|current| total.checked_add(current)))
    }

    pub fn calc_storage_mass_output_harmonic_single(&self, output_value: u64) -> u64 {
        self.storage_mass_parameter / output_value
    }

    pub fn calc_storage_mass_input_mean_arithmetic(&self, total_input_value: u64, number_of_inputs: u64) -> u64 {
        let mean_input_value = total_input_value / number_of_inputs;
        number_of_inputs.saturating_mul(self.storage_mass_parameter / mean_input_value)
    }

    // TODO(wallet-storage-mass-inconcistency): Stop using this estimate for generator fee/limit decisions. Once candidate
    // outputs are known, use calc_storage_mass_for_transaction_parts instead.
    pub fn calc_storage_mass(&self, output_harmonic: u64, total_input_value: u64, number_of_inputs: u64) -> u64 {
        let input_arithmetic = self.calc_storage_mass_input_mean_arithmetic(total_input_value, number_of_inputs);
        output_harmonic.saturating_sub(input_arithmetic)
    }
}
