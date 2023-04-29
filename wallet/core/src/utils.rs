use crate::tx::{Transaction, TransactionOutput};
use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::networktype::NetworkType;
use kaspa_consensus_core::{
    config::params::{Params, DEVNET_PARAMS, MAINNET_PARAMS},
    constants::*,
    mass::{self, MassCalculator},
};
use wasm_bindgen::prelude::*;

/// MINIMUM_RELAY_TRANSACTION_FEE specifies the minimum transaction fee for a transaction to be accepted to
/// the mempool and relayed. It is specified in sompi per 1kg (or 1000 grams) of transaction mass.
pub(crate) const MINIMUM_RELAY_TRANSACTION_FEE: u64 = 1000;

// /// MAXIMUM_STANDARD_TRANSACTION_MASS is the maximum mass allowed for transactions that
// /// are considered standard and will therefore be relayed and considered for mining.
// const MAXIMUM_STANDARD_TRANSACTION_MASS: u64 = 100_000;

/// minimum_required_transaction_relay_fee returns the minimum transaction fee required
/// for a transaction with the passed mass to be accepted into the mempool and relayed.
pub fn minimum_required_transaction_relay_fee(mass: u64) -> u64 {
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
/// It is exposed by [MiningManager] for use by transaction generators and wallets.
#[wasm_bindgen(js_name=isTransactionOutputDust)]
pub fn is_transaction_output_dust(transaction_output: &TransactionOutput) -> bool {
    // Unspendable outputs are considered dust.
    //
    // TODO: call script engine when available
    // if txscript.is_unspendable(transaction_output.script_public_key.script()) {
    //     return true
    // }
    // TODO: Remove this code when script engine is available
    if transaction_output.get_script_public_key().script().len() < 33 {
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
    let output = transaction_output.clone().try_into().unwrap();
    let total_serialized_size = mass::transaction_output_estimated_serialized_size(&output) + 148;

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
    let value = output.value;
    match value.checked_mul(1000) {
        Some(value_1000) => value_1000 / (3 * total_serialized_size) < MINIMUM_RELAY_TRANSACTION_FEE,
        None => (value as u128 * 1000 / (3 * total_serialized_size as u128)) < MINIMUM_RELAY_TRANSACTION_FEE as u128,
    }
}

pub fn calculate_mass(tx: &Transaction, params: &Params, estimate_signature_mass: bool) -> u64 {
    let mass_calculator = MassCalculator::new(params.mass_per_tx_byte, params.mass_per_script_pub_key_byte, params.mass_per_sig_op);
    let mut mass = mass_calculator.calc_tx_mass(&tx.try_into().unwrap());

    if !estimate_signature_mass {
        return mass;
    }

    //TODO: remove this sig_op_count mass calculation
    let sig_op_count = 1;
    mass += (sig_op_count * tx.inner().inputs.len() as u64) * params.mass_per_sig_op;

    let signature_mass = transaction_estimate_signature_mass(tx, params);
    mass + signature_mass
}

pub fn transaction_estimate_signature_mass(tx: &Transaction, params: &Params) -> u64 {
    let signature_script_size = 66; //params.max_signature_script_len;
    tx.inner().inputs.len() as u64 * signature_script_size * params.mass_per_tx_byte
}

pub fn calculate_minimum_transaction_fee(tx: &Transaction, params: &Params, estimate_signature_mass: bool) -> u64 {
    minimum_required_transaction_relay_fee(calculate_mass(tx, params, estimate_signature_mass))
}

/// find Consensus parameters for given Address
pub fn get_consensus_params_by_address(address: &Address) -> Params {
    match address.prefix {
        Prefix::Mainnet => MAINNET_PARAMS,
        _ => DEVNET_PARAMS,
    }
}

/// find Consensus parameters for given NetworkType
pub fn get_consensus_params_by_network(network: &NetworkType) -> Params {
    match network {
        NetworkType::Mainnet => MAINNET_PARAMS,
        _ => DEVNET_PARAMS,
    }
}
