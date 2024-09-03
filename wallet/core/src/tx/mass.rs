//!
//! Transaction mass calculator.
//!

use crate::error::Error;
use crate::result::Result;
use crate::utxo::NetworkParams;
use kaspa_consensus_client as kcc;
use kaspa_consensus_client::UtxoEntryReference;
use kaspa_consensus_core::mass::{calc_storage_mass as consensus_calc_storage_mass, Kip9Version};
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutput, SCRIPT_VECTOR_SIZE};
use kaspa_consensus_core::{config::params::Params, constants::*, subnets::SUBNETWORK_ID_SIZE};
use kaspa_hashes::HASH_SIZE;

// pub const ECDSA_SIGNATURE_SIZE: u64 = 64;
// pub const SCHNORR_SIGNATURE_SIZE: u64 = 64;
pub const SIGNATURE_SIZE: u64 = 1 + 64 + 1; //1 byte for OP_DATA_65 + 64 (length of signature) + 1 byte for sig hash type

/// MINIMUM_RELAY_TRANSACTION_FEE specifies the minimum transaction fee for a transaction to be accepted to
/// the mempool and relayed. It is specified in sompi per 1kg (or 1000 grams) of transaction mass.
pub(crate) const MINIMUM_RELAY_TRANSACTION_FEE: u64 = 1000;

/// MAXIMUM_STANDARD_TRANSACTION_MASS is the maximum mass allowed for transactions that
/// are considered standard and will therefore be relayed and considered for mining.
pub const MAXIMUM_STANDARD_TRANSACTION_MASS: u64 = 100_000;

/// minimum_required_transaction_relay_fee returns the minimum transaction fee required
/// for a transaction with the passed mass to be accepted into the mempool and relayed.
pub fn calc_minimum_required_transaction_relay_fee(mass: u64) -> u64 {
    // Calculate the minimum fee for a transaction to be allowed into the
    // mempool and relayed by scaling the base fee. MinimumRelayTransactionFee is in
    // sompi/kg so multiply by mass (which is in grams) and divide by 1000 to get
    // minimum sompis.
    let mut minimum_fee = (mass * MINIMUM_RELAY_TRANSACTION_FEE) / 1000;

    if minimum_fee == 0 {
        minimum_fee = MINIMUM_RELAY_TRANSACTION_FEE;
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
/// Dust is defined in terms of the minimum transaction relay fee. In particular,
/// if the cost to the network to spend coins is more than 1/3 of the minimum
/// transaction relay fee, it is considered dust.
///
/// It is exposed by `MiningManager` for use by transaction generators and wallets.
pub fn is_transaction_output_dust(transaction_output: &TransactionOutput) -> bool {
    // Unspendable outputs are considered dust.
    //
    // TODO: call script engine when available
    // if txscript.is_unspendable(transaction_output.script_public_key.script()) {
    //     return true
    // }
    // TODO: Remove this code when script engine is available
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
    // let output = transaction_output.clone().try_into().unwrap();
    let total_serialized_size = transaction_output_serialized_byte_size(transaction_output) + 148;

    // The output is considered dust if the cost to the network to spend the
    // coins is more than 1/3 of the minimum free transaction relay fee.
    // mp.config.MinimumRelayTransactionFee is in sompi/KB, so multiply
    // by 1000 to convert to bytes.
    //
    // Using the typical values for a pay-to-pubkey transaction from
    // the breakdown above and the default minimum free transaction relay
    // fee of 1000, this equates to values less than 546 sompi being
    // considered dust.
    //
    // The following is equivalent to (value/total_serialized_size) * (1/3) * 1000
    // without needing to do floating point math.
    //
    // Since the multiplication may overflow a u64, 2 separate calculation paths
    // are considered to avoid overflowing.
    let value = transaction_output.value;
    match value.checked_mul(1000) {
        Some(value_1000) => value_1000 / (3 * total_serialized_size) < MINIMUM_RELAY_TRANSACTION_FEE,
        None => (value as u128 * 1000 / (3 * total_serialized_size as u128)) < MINIMUM_RELAY_TRANSACTION_FEE as u128,
    }
}

// The most common scripts are pay-to-pubkey, and as per the above
// breakdown, the minimum size of a p2pk input script is 148 bytes. So
// that figure is used.
pub const STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE: u64 = transaction_standard_output_serialized_byte_size() + 148;
pub const STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE_3X: u64 = STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE * 3;

// pub fn is_standard_output_amount_dust(value: u64) -> bool {
//     match value.checked_mul(1000) {
//         Some(value_1000) => value_1000 / STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE_3X < MINIMUM_RELAY_TRANSACTION_FEE,
//         None => (value as u128 * 1000 / STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE_3X as u128) < MINIMUM_RELAY_TRANSACTION_FEE as u128,
//     }
// }

// pub fn is_standard_output_amount_dust(network_params: &NetworkParams, value: u64) -> bool {
// pub fn is_dust(_network_params: &NetworkParams, value: u64) -> bool {
//     // if let Some(dust_threshold_sompi) = network_params.dust_threshold_sompi {
//     //     return value < dust_threshold_sompi;
//     // } else {
//     match value.checked_mul(1000) {
//         Some(value_1000) => value_1000 / STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE_3X < MINIMUM_RELAY_TRANSACTION_FEE,
//         None => (value as u128 * 1000 / STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE_3X as u128) < MINIMUM_RELAY_TRANSACTION_FEE as u128,
//     }
//     // }
// }

// transaction_estimated_serialized_size is the estimated size of a transaction in some
// serialization. This has to be deterministic, but not necessarily accurate, since
// it's only used as the size component in the transaction and block mass limit
// calculation.
pub fn transaction_serialized_byte_size(tx: &Transaction) -> u64 {
    // let inner = tx.inner();

    let mut size: u64 = 0;
    size += 2; // Tx version (u16)
    size += 8; // Number of inputs (u64)
    let inputs_size: u64 = tx.inputs.iter().map(transaction_input_serialized_byte_size).sum();
    size += inputs_size;

    size += 8; // number of outputs (u64)
    let outputs_size: u64 = tx.outputs.iter().map(transaction_output_serialized_byte_size).sum();
    size += outputs_size;

    size += 8; // lock time (u64)
    size += SUBNETWORK_ID_SIZE as u64;
    size += 8; // gas (u64)
    size += HASH_SIZE as u64; // payload hash

    size += 8; // length of the payload (u64)
    size += tx.payload.len() as u64;
    size
}

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

fn transaction_input_serialized_byte_size(input: &TransactionInput) -> u64 {
    let mut size = 0;
    size += outpoint_estimated_serialized_size();

    size += 8; // length of signature script (u64)
    size += input.signature_script.len() as u64;

    size += 8; // sequence (uint64)
    size
}

const fn outpoint_estimated_serialized_size() -> u64 {
    let mut size: u64 = 0;
    size += HASH_SIZE as u64; // Previous tx ID
    size += 4; // Index (u32)
    size
}

pub fn transaction_output_serialized_byte_size(output_inner: &TransactionOutput) -> u64 {
    let mut size: u64 = 0;
    size += 8; // value (u64)
    size += 2; // output.ScriptPublicKey.Version (u16)
    size += 8; // length of script public key (u64)
    size += output_inner.script_public_key.script().len() as u64;
    size
}

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
    mass_per_tx_byte: u64,
    mass_per_script_pub_key_byte: u64,
    mass_per_sig_op: u64,
    storage_mass_parameter: u64,
    kip9_version: Kip9Version,
}

impl MassCalculator {
    pub fn new(consensus_params: &Params, network_params: &NetworkParams) -> Self {
        Self {
            mass_per_tx_byte: consensus_params.mass_per_tx_byte,
            mass_per_script_pub_key_byte: consensus_params.mass_per_script_pub_key_byte,
            mass_per_sig_op: consensus_params.mass_per_sig_op,
            storage_mass_parameter: consensus_params.storage_mass_parameter,
            kip9_version: network_params.kip9_version(),
        }
    }

    pub fn is_dust(&self, value: u64) -> bool {
        match value.checked_mul(1000) {
            Some(value_1000) => value_1000 / STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE_3X < MINIMUM_RELAY_TRANSACTION_FEE,
            None => (value as u128 * 1000 / STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE_3X as u128) < MINIMUM_RELAY_TRANSACTION_FEE as u128,
        }
    }

    pub fn calc_compute_mass_for_signed_consensus_transaction(&self, tx: &Transaction) -> u64 {
        let payload_len = tx.payload.len();
        self.blank_transaction_compute_mass()
            + self.calc_compute_mass_for_payload(payload_len)
            + self.calc_compute_mass_for_client_transaction_outputs(&tx.outputs)
            + self.calc_compute_mass_for_client_transaction_inputs(&tx.inputs)
    }

    pub(crate) fn blank_transaction_compute_mass(&self) -> u64 {
        blank_transaction_serialized_byte_size() * self.mass_per_tx_byte
    }

    pub(crate) fn calc_compute_mass_for_payload(&self, payload_byte_size: usize) -> u64 {
        payload_byte_size as u64 * self.mass_per_tx_byte
    }

    pub(crate) fn calc_compute_mass_for_client_transaction_outputs(&self, outputs: &[TransactionOutput]) -> u64 {
        outputs.iter().map(|output| self.calc_compute_mass_for_client_transaction_output(output)).sum()
    }

    pub(crate) fn calc_compute_mass_for_client_transaction_inputs(&self, inputs: &[TransactionInput]) -> u64 {
        inputs.iter().map(|input| self.calc_compute_mass_for_client_transaction_input(input)).sum::<u64>()
    }

    pub(crate) fn calc_compute_mass_for_client_transaction_output(&self, output: &TransactionOutput) -> u64 {
        // +2 for u16 version
        self.mass_per_script_pub_key_byte * (2 + output.script_public_key.script().len() as u64)
            + transaction_output_serialized_byte_size(output) * self.mass_per_tx_byte
    }

    pub(crate) fn calc_compute_mass_for_client_transaction_input(&self, input: &TransactionInput) -> u64 {
        input.sig_op_count as u64 * self.mass_per_sig_op + transaction_input_serialized_byte_size(input) * self.mass_per_tx_byte
    }

    pub(crate) fn calc_compute_mass_for_signature(&self, minimum_signatures: u16) -> u64 {
        SIGNATURE_SIZE * self.mass_per_tx_byte * minimum_signatures.max(1) as u64
    }

    pub fn calc_signature_compute_mass_for_inputs(&self, number_of_inputs: usize, minimum_signatures: u16) -> u64 {
        SIGNATURE_SIZE * self.mass_per_tx_byte * minimum_signatures.max(1) as u64 * number_of_inputs as u64
    }

    pub fn calc_minimum_transaction_fee_from_mass(&self, mass: u64) -> u64 {
        calc_minimum_required_transaction_relay_fee(mass)
    }

    pub fn calc_compute_mass_for_unsigned_consensus_transaction(&self, tx: &Transaction, minimum_signatures: u16) -> u64 {
        self.calc_compute_mass_for_signed_consensus_transaction(tx)
            + self.calc_signature_compute_mass_for_inputs(tx.inputs.len(), minimum_signatures)
    }

    // provisional
    #[inline(always)]
    pub fn calc_fee_for_mass(&self, mass: u64) -> u64 {
        mass
    }

    pub fn combine_mass(&self, compute_mass: u64, storage_mass: u64) -> u64 {
        match self.kip9_version {
            Kip9Version::Alpha => compute_mass.saturating_add(storage_mass),
            Kip9Version::Beta => compute_mass.max(storage_mass),
        }
    }

    /// Calculates the overall mass of this transaction, combining both compute and storage masses.
    pub fn calc_overall_mass_for_unsigned_client_transaction(&self, tx: &kcc::Transaction, minimum_signatures: u16) -> Result<u64> {
        let cctx = Transaction::from(tx);
        let storage_mass = self.calc_storage_mass_for_transaction(tx)?.ok_or(Error::MassCalculationError)?;
        let compute_mass = self.calc_compute_mass_for_unsigned_consensus_transaction(&cctx, minimum_signatures);
        Ok(self.combine_mass(compute_mass, storage_mass))
    }

    pub fn calc_overall_mass_for_unsigned_consensus_transaction(
        &self,
        tx: &Transaction,
        utxos: &[UtxoEntryReference],
        minimum_signatures: u16,
    ) -> Result<u64> {
        let storage_mass = self.calc_storage_mass_for_transaction_parts(utxos, &tx.outputs).ok_or(Error::MassCalculationError)?;
        let compute_mass = self.calc_compute_mass_for_unsigned_consensus_transaction(tx, minimum_signatures);
        Ok(self.combine_mass(compute_mass, storage_mass))
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
            inputs.iter().map(|entry| entry.amount()),
            outputs.iter().map(|out| out.value),
            self.kip9_version,
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

    pub fn calc_storage_mass(&self, output_harmonic: u64, total_input_value: u64, number_of_inputs: u64) -> u64 {
        let input_arithmetic = self.calc_storage_mass_input_mean_arithmetic(total_input_value, number_of_inputs);
        output_harmonic.saturating_sub(input_arithmetic)
    }
}
