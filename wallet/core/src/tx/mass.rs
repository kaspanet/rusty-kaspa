//!
//! Transaction mass calculator.
//!

use crate::utxo::NetworkParams;
use kaspa_consensus_client::UtxoEntryReference;
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutput, SCRIPT_VECTOR_SIZE};
use kaspa_consensus_core::{config::params::Params, constants::*, subnets::SUBNETWORK_ID_SIZE};
use kaspa_hashes::HASH_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MassCombinationStrategy {
    /// `MassCombinator::Add` adds the storage and compute mass.
    Add,
    /// `MassCombinator::Max` returns the maximum of the storage and compute mass.
    Max,
}

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
    mass_combination_strategy: MassCombinationStrategy,
}

impl MassCalculator {
    pub fn new(consensus_params: &Params, network_params: &NetworkParams) -> Self {
        Self {
            mass_per_tx_byte: consensus_params.mass_per_tx_byte,
            mass_per_script_pub_key_byte: consensus_params.mass_per_script_pub_key_byte,
            mass_per_sig_op: consensus_params.mass_per_sig_op,
            storage_mass_parameter: consensus_params.storage_mass_parameter,
            mass_combination_strategy: network_params.mass_combination_strategy,
        }
    }

    pub fn is_dust(&self, value: u64) -> bool {
        match value.checked_mul(1000) {
            Some(value_1000) => value_1000 / STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE_3X < MINIMUM_RELAY_TRANSACTION_FEE,
            None => (value as u128 * 1000 / STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE_3X as u128) < MINIMUM_RELAY_TRANSACTION_FEE as u128,
        }
    }

    pub fn calc_mass_for_transaction(&self, tx: &Transaction) -> u64 {
        self.blank_transaction_mass()
            + self.calc_mass_for_payload(tx.payload.len())
            + self.calc_mass_for_outputs(&tx.outputs)
            + self.calc_mass_for_inputs(&tx.inputs)
    }

    pub fn blank_transaction_mass(&self) -> u64 {
        blank_transaction_serialized_byte_size() * self.mass_per_tx_byte
    }

    pub fn calc_mass_for_payload(&self, payload_byte_size: usize) -> u64 {
        payload_byte_size as u64 * self.mass_per_tx_byte
    }

    pub fn calc_mass_for_outputs(&self, outputs: &[TransactionOutput]) -> u64 {
        outputs.iter().map(|output| self.calc_mass_for_output(output)).sum()
    }

    pub fn calc_mass_for_inputs(&self, inputs: &[TransactionInput]) -> u64 {
        inputs.iter().map(|input| self.calc_mass_for_input(input)).sum::<u64>()
    }

    pub fn calc_mass_for_output(&self, output: &TransactionOutput) -> u64 {
        self.mass_per_script_pub_key_byte * (2 + output.script_public_key.script().len() as u64)
            + transaction_output_serialized_byte_size(output) * self.mass_per_tx_byte
    }

    pub fn calc_mass_for_input(&self, input: &TransactionInput) -> u64 {
        input.sig_op_count as u64 * self.mass_per_sig_op + transaction_input_serialized_byte_size(input) * self.mass_per_tx_byte
    }

    pub fn calc_signature_mass(&self, minimum_signatures: u16) -> u64 {
        let minimum_signatures = std::cmp::max(1, minimum_signatures);
        SIGNATURE_SIZE * self.mass_per_tx_byte * minimum_signatures as u64
    }

    pub fn calc_signature_mass_for_inputs(&self, number_of_inputs: usize, minimum_signatures: u16) -> u64 {
        let minimum_signatures = std::cmp::max(1, minimum_signatures);
        SIGNATURE_SIZE * self.mass_per_tx_byte * minimum_signatures as u64 * number_of_inputs as u64
    }

    pub fn calc_minimum_transaction_fee_from_mass(&self, mass: u64) -> u64 {
        calc_minimum_required_transaction_relay_fee(mass)
    }

    pub fn calc_mass_for_signed_transaction(&self, tx: &Transaction, minimum_signatures: u16) -> u64 {
        self.calc_mass_for_transaction(tx) + self.calc_signature_mass_for_inputs(tx.inputs.len(), minimum_signatures)
    }

    pub fn calc_minium_transaction_relay_fee(&self, tx: &Transaction, minimum_signatures: u16) -> u64 {
        let mass = self.calc_mass_for_transaction(tx) + self.calc_signature_mass_for_inputs(tx.inputs.len(), minimum_signatures);
        calc_minimum_required_transaction_relay_fee(mass)
    }

    pub fn calc_tx_storage_fee(&self, is_coinbase: bool, inputs: &[UtxoEntryReference], outputs: &[TransactionOutput]) -> u64 {
        self.calc_fee_for_storage_mass(self.calc_storage_mass_for_transaction(is_coinbase, inputs, outputs).unwrap_or(u64::MAX))
    }

    pub fn calc_fee_for_storage_mass(&self, mass: u64) -> u64 {
        mass
    }

    pub fn combine_mass(&self, compute_mass: u64, storage_mass: u64) -> u64 {
        match self.mass_combination_strategy {
            MassCombinationStrategy::Add => compute_mass + storage_mass,
            MassCombinationStrategy::Max => std::cmp::max(compute_mass, storage_mass),
        }
    }

    pub fn calc_storage_mass_for_transaction(
        &self,
        is_coinbase: bool,
        inputs: &[UtxoEntryReference],
        outputs: &[TransactionOutput],
    ) -> Option<u64> {
        if is_coinbase {
            return Some(0);
        }
        /* The code below computes the following formula:

                max( 0 , C·( |O|/H(O) - |I|/A(I) ) )

        where C is the mass storage parameter, O is the set of output values, I is the set of
        input values, H(S) := |S|/sum_{s in S} 1 / s is the harmonic mean over the set S and
        A(S) := sum_{s in S} / |S| is the arithmetic mean.

        See the (to date unpublished) KIP-0009 for more details
        */

        // Since we are doing integer division, we perform the multiplication with C over the inner
        // fractions, otherwise we'll get a sum of zeros or ones.
        //
        // If sum of fractions overflowed (nearly impossible, requires 10^7 outputs for C = 10^12),
        // we return `None` indicating mass is incomputable

        let harmonic_outs = outputs
            .iter()
            .map(|out| self.storage_mass_parameter / out.value)
            .try_fold(0u64, |total, current| total.checked_add(current))?; // C·|O|/H(O)

        // Total supply is bounded, so a sum of existing UTXO entries cannot overflow (nor can it be zero)
        let sum_ins = inputs.iter().map(|entry| entry.amount()).sum::<u64>(); // |I|·A(I)
        let ins_len = inputs.len() as u64;
        let mean_ins = sum_ins / ins_len;

        // Inner fraction must be with C and over the mean value, in order to maximize precision.
        // We can saturate the overall expression at u64::MAX since we lower-bound the subtraction below by zero anyway
        let arithmetic_ins = ins_len.saturating_mul(self.storage_mass_parameter / mean_ins); // C·|I|/A(I)

        Some(harmonic_outs.saturating_sub(arithmetic_ins)) // max( 0 , C·( |O|/H(O) - |I|/A(I) ) )
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
