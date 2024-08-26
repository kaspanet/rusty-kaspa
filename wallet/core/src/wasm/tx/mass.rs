use crate::imports::NetworkParams;
use crate::result::Result;
use crate::tx::mass;
use kaspa_consensus_client::*;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkIdT};
use wasm_bindgen::prelude::*;
use workflow_wasm::convert::*;

/// `calculateTransactionMass()` returns the mass of the passed transaction.
/// If the transaction is invalid, the function throws an error.
/// If the mass is larger than the transaction mass allowed by the network, the function
/// returns `undefined` which can be treated as a mass overflow condition.
///
/// @category Wallet SDK
///
#[wasm_bindgen(js_name = calculateTransactionMass)]
pub fn calculate_transaction_mass(network_id: NetworkIdT, tx: &TransactionT) -> Result<Option<u64>> {
    let tx = Transaction::try_cast_from(tx)?;
    let network_id = NetworkId::try_owned_from(network_id)?;
    let consensus_params = Params::from(network_id);
    let network_params = NetworkParams::from(network_id);
    let mc = mass::MassCalculator::new(&consensus_params, network_params);
    mc.calc_tx_overall_mass(tx.as_ref())
}

/// `calculateTransactionFee()` returns minimum fees needed for the transaction to be
/// accepted by the network. If the transaction is invalid, the function throws an error.
/// If the mass of the transaction is larger than the maximum allowed by the network, the
/// function returns `undefined` which can be treated as a mass overflow condition.
///
/// @category Wallet SDK
///
#[wasm_bindgen(js_name = calculateTransactionFee)]
pub fn calculate_transaction_fee(network_id: NetworkIdT, tx: &TransactionT) -> Result<Option<u64>> {
    let tx = Transaction::try_cast_from(tx)?;
    let network_id = NetworkId::try_owned_from(network_id)?;
    let consensus_params = Params::from(network_id);
    let network_params = NetworkParams::from(network_id);
    let mc = mass::MassCalculator::new(&consensus_params, network_params);
    let fee = mc.calc_tx_overall_mass(tx.as_ref())?.map(|mass| mc.calc_fee_for_mass(mass));
    Ok(fee)
}
