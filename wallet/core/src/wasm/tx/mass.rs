use crate::imports::NetworkParams;
use crate::result::Result;
use crate::tx::mass;
use kaspa_consensus_client::*;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkIdT};
use wasm_bindgen::prelude::*;
use workflow_wasm::convert::*;

/// `calculateTransactionMass()` returns the mass of the passed transaction.
/// If the transaction is invalid, or the mass can not be calculated
/// the function throws an error.
///
/// @category Wallet SDK
///
#[wasm_bindgen(js_name = calculateTransactionMass)]
pub fn calculate_unsigned_transaction_mass(network_id: NetworkIdT, tx: &TransactionT, minimum_signatures: Option<u16>) -> Result<u64> {
    let tx = Transaction::try_cast_from(tx)?;
    let network_id = NetworkId::try_owned_from(network_id)?;
    let consensus_params = Params::from(network_id);
    let network_params = NetworkParams::from(network_id);
    let mc = mass::MassCalculator::new(&consensus_params, network_params);
    mc.calc_overall_mass_for_unsigned_client_transaction(tx.as_ref(), minimum_signatures.unwrap_or(1))
}

/// `updateTransactionMass()` updates the mass property of the passed transaction.
/// If the transaction is invalid, or the mass is larger than transaction mass allowed
/// by the network, the function throws an error.
///
/// This is the same as `calculateTransactionMass()` but modifies the supplied
/// `Transaction` object.
///
/// @category Wallet SDK
///
#[wasm_bindgen(js_name = updateTransactionMass)]
pub fn update_unsigned_transaction_mass(network_id: NetworkIdT, tx: &Transaction, minimum_signatures: Option<u16>) -> Result<()> {
    let network_id = NetworkId::try_owned_from(network_id)?;
    let consensus_params = Params::from(network_id);
    let network_params = NetworkParams::from(network_id);
    let mc = mass::MassCalculator::new(&consensus_params, network_params);
    let mass = mc.calc_overall_mass_for_unsigned_client_transaction(tx, minimum_signatures.unwrap_or(1))?;
    tx.set_mass(mass);
    Ok(())
}

/// `calculateTransactionFee()` returns minimum fees needed for the transaction to be
/// accepted by the network. If the transaction is invalid or the mass can not be calculated,
/// the function throws an error.
///
/// @category Wallet SDK
///
#[wasm_bindgen(js_name = calculateTransactionFee)]
pub fn calculate_unsigned_transaction_fee(network_id: NetworkIdT, tx: &TransactionT, minimum_signatures: Option<u16>) -> Result<u64> {
    let tx = Transaction::try_cast_from(tx)?;
    let network_id = NetworkId::try_owned_from(network_id)?;
    let consensus_params = Params::from(network_id);
    let network_params = NetworkParams::from(network_id);
    let mc = mass::MassCalculator::new(&consensus_params, network_params);
    let mass = mc.calc_overall_mass_for_unsigned_client_transaction(tx.as_ref(), minimum_signatures.unwrap_or(1))?;
    let fee = mc.calc_fee_for_mass(mass);
    Ok(fee)
}
