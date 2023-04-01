use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::{
    config::params::{Params, DEVNET_PARAMS, MAINNET_PARAMS},
    constants::*,
    mass::MassCalculator,
    tx::Transaction,
};
//use kaspa_consensus_core::mass::transaction_estimated_serialized_size;

/// MINIMUM_RELAY_TRANSACTION_FEE specifies the minimum transaction fee for a transaction to be accepted to
/// the mempool and relayed. It is specified in sompi per 1kg (or 1000 grams) of transaction mass.
pub(crate) const MINIMUM_RELAY_TRANSACTION_FEE: u64 = 1000;

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

pub fn calculate_mass(tx: &Transaction, params: &Params, estimate_signature_mass: bool) -> u64 {
    let mass_calculator = MassCalculator::new(params.mass_per_tx_byte, params.mass_per_script_pub_key_byte, params.mass_per_sig_op);
    let mass = mass_calculator.calc_tx_mass(tx);
    if !estimate_signature_mass {
        return mass;
    }
    let signature_mass = transaction_estimate_signature_mass(tx, params);

    mass + signature_mass
}

pub fn transaction_estimate_signature_mass(tx: &Transaction, params: &Params) -> u64 {
    let signature_script_size = 66; //params.max_signature_script_len;
    tx.inputs.len() as u64 * signature_script_size * params.mass_per_script_pub_key_byte
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
