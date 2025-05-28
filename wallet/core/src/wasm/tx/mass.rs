use crate::result::Result;
use crate::tx::{mass, MAXIMUM_STANDARD_TRANSACTION_MASS};
use js_sys::Array;
use kaspa_consensus_client::*;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::mass::{calc_storage_mass, UtxoCell};
use kaspa_consensus_core::network::{NetworkId, NetworkIdT};
use kaspa_wasm_core::types::NumberArray;
use wasm_bindgen::prelude::*;
use workflow_wasm::convert::*;

/// `maximumStandardTransactionMass()` returns the maximum transaction
/// size allowed by the network.
///
/// @category Wallet SDK
/// @see {@link calculateTransactionMass}
/// @see {@link updateTransactionMass}
/// @see {@link calculateTransactionFee}
#[wasm_bindgen(js_name = maximumStandardTransactionMass)]
pub fn maximum_standard_transaction_mass() -> u64 {
    MAXIMUM_STANDARD_TRANSACTION_MASS
}

/// `calculateTransactionMass()` returns the mass of the passed transaction.
/// If the transaction is invalid, or the mass can not be calculated
/// the function throws an error.
///
/// The mass value must not exceed the maximum standard transaction mass
/// that can be obtained using `maximumStandardTransactionMass()`.
///
/// @category Wallet SDK
/// @see {@link maximumStandardTransactionMass}
///
#[wasm_bindgen(js_name = calculateTransactionMass)]
pub fn calculate_unsigned_transaction_mass(network_id: NetworkIdT, tx: &TransactionT, minimum_signatures: Option<u16>) -> Result<u64> {
    let tx = Transaction::try_cast_from(tx)?;
    let network_id = NetworkId::try_owned_from(network_id)?;
    let consensus_params = Params::from(network_id);
    let mc = mass::MassCalculator::new(&consensus_params);
    mc.calc_overall_mass_for_unsigned_client_transaction(tx.as_ref(), minimum_signatures.unwrap_or(1))
}

/// `updateTransactionMass()` updates the mass property of the passed transaction.
/// If the transaction is invalid, the function throws an error.
///
/// The function returns `true` if the mass is within the maximum standard transaction mass and
/// the transaction mass is updated. Otherwise, the function returns `false`.
///
/// This is similar to `calculateTransactionMass()` but modifies the supplied
/// `Transaction` object.
///
/// @category Wallet SDK
/// @see {@link maximumStandardTransactionMass}
/// @see {@link calculateTransactionMass}
/// @see {@link calculateTransactionFee}
///
#[wasm_bindgen(js_name = updateTransactionMass)]
pub fn update_unsigned_transaction_mass(network_id: NetworkIdT, tx: &Transaction, minimum_signatures: Option<u16>) -> Result<bool> {
    let network_id = NetworkId::try_owned_from(network_id)?;
    let consensus_params = Params::from(network_id);
    let mc = mass::MassCalculator::new(&consensus_params);
    let mass = mc.calc_overall_mass_for_unsigned_client_transaction(tx, minimum_signatures.unwrap_or(1))?;
    if mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
        Ok(false)
    } else {
        tx.set_mass(mass);
        Ok(true)
    }
}

/// `calculateTransactionFee()` returns minimum fees needed for the transaction to be
/// accepted by the network. If the transaction is invalid or the mass can not be calculated,
/// the function throws an error. If the mass exceeds the maximum standard transaction mass,
/// the function returns `undefined`.
///
/// @category Wallet SDK
/// @see {@link maximumStandardTransactionMass}
/// @see {@link calculateTransactionMass}
/// @see {@link updateTransactionMass}
///
#[wasm_bindgen(js_name = calculateTransactionFee)]
pub fn calculate_unsigned_transaction_fee(
    network_id: NetworkIdT,
    tx: &TransactionT,
    minimum_signatures: Option<u16>,
) -> Result<Option<u64>> {
    let tx = Transaction::try_cast_from(tx)?;
    let network_id = NetworkId::try_owned_from(network_id)?;
    let consensus_params = Params::from(network_id);
    let mc = mass::MassCalculator::new(&consensus_params);
    let mass = mc.calc_overall_mass_for_unsigned_client_transaction(tx.as_ref(), minimum_signatures.unwrap_or(1))?;
    if mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
        Ok(None)
    } else {
        Ok(Some(mc.calc_fee_for_mass(mass)))
    }
}

/// `calculateStorageMass()` is a helper function to compute the storage mass of inputs and outputs.
/// This function can be use to calculate the storage mass of transaction inputs and outputs.
/// Note that the storage mass is only a component of the total transaction mass. You are not
/// meant to use this function by itself and should use `calculateTransactionMass()` instead.
/// This function purely exists for diagnostic purposes and to help with complex algorithms that
/// may require a manual UTXO selection for identifying UTXOs and outputs needed for low storage mass.
///
/// @category Wallet SDK
/// @see {@link maximumStandardTransactionMass}
/// @see {@link calculateTransactionMass}
///
#[wasm_bindgen(js_name = calculateStorageMass)]
pub fn calculate_storage_mass(network_id: NetworkIdT, input_values: &NumberArray, output_values: &NumberArray) -> Result<Option<u64>> {
    let network_id = NetworkId::try_owned_from(network_id)?;
    let consensus_params = Params::from(network_id);

    let input_values =
        Array::from(input_values).to_vec().iter().map(|v| UtxoCell::new(1, v.as_f64().unwrap() as u64)).collect::<Vec<UtxoCell>>();
    let output_values =
        Array::from(output_values).to_vec().iter().map(|v| UtxoCell::new(1, v.as_f64().unwrap() as u64)).collect::<Vec<UtxoCell>>();

    let storage_mass =
        calc_storage_mass(false, input_values.into_iter(), output_values.into_iter(), consensus_params.storage_mass_parameter);

    Ok(storage_mass)
}
