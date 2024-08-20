use crate::imports::*;
use crate::result::Result;
use kaspa_addresses::*;
use kaspa_consensus_core::{
    network::{NetworkType, NetworkTypeT},
    tx::ScriptPublicKeyT,
};
use kaspa_txscript::{script_class::ScriptClass, standard};
use kaspa_utils::hex::ToHex;
use kaspa_wasm_core::types::{BinaryT, HexString};

/// Creates a new script to pay a transaction output to the specified address.
/// @category Wallet SDK
#[wasm_bindgen(js_name = payToAddressScript)]
pub fn pay_to_address_script(address: &AddressT) -> Result<ScriptPublicKey> {
    let address = Address::try_cast_from(address)?;
    Ok(standard::pay_to_address_script(address.as_ref()))
}

/// Takes a script and returns an equivalent pay-to-script-hash script.
/// @param redeem_script - The redeem script ({@link HexString} or Uint8Array).
/// @category Wallet SDK
#[wasm_bindgen(js_name = payToScriptHashScript)]
pub fn pay_to_script_hash_script(redeem_script: BinaryT) -> Result<ScriptPublicKey> {
    let redeem_script = redeem_script.try_as_vec_u8()?;
    Ok(standard::pay_to_script_hash_script(redeem_script.as_slice()))
}

/// Generates a signature script that fits a pay-to-script-hash script.
/// @param redeem_script - The redeem script ({@link HexString} or Uint8Array).
/// @param signature - The signature ({@link HexString} or Uint8Array).
/// @category Wallet SDK
#[wasm_bindgen(js_name = payToScriptHashSignatureScript)]
pub fn pay_to_script_hash_signature_script(redeem_script: BinaryT, signature: BinaryT) -> Result<HexString> {
    let redeem_script = redeem_script.try_as_vec_u8()?;
    let signature = signature.try_as_vec_u8()?;
    let script = standard::pay_to_script_hash_signature_script(redeem_script, signature)?;
    Ok(script.to_hex().into())
}

/// Returns the address encoded in a script public key.
/// @param script_public_key - The script public key ({@link ScriptPublicKey}).
/// @param network - The network type.
/// @category Wallet SDK
#[wasm_bindgen(js_name = addressFromScriptPublicKey)]
pub fn address_from_script_public_key(script_public_key: &ScriptPublicKeyT, network: &NetworkTypeT) -> Result<AddressOrUndefinedT> {
    let script_public_key = ScriptPublicKey::try_cast_from(script_public_key)?;
    let network_type = NetworkType::try_from(network)?;

    match standard::extract_script_pub_key_address(script_public_key.as_ref(), network_type.into()) {
        Ok(address) => Ok(AddressOrUndefinedT::from(JsValue::from(address))),
        Err(_) => Ok(AddressOrUndefinedT::from(JsValue::UNDEFINED)),
    }
}

/// Returns true if the script passed is a pay-to-pubkey.
/// @param script - The script ({@link HexString} or Uint8Array).
/// @category Wallet SDK
#[wasm_bindgen(js_name = isScriptPayToPubkey)]
pub fn is_script_pay_to_pubkey(script: BinaryT) -> Result<bool> {
    let script = script.try_as_vec_u8()?;
    Ok(ScriptClass::is_pay_to_pubkey(script.as_slice()))
}

/// Returns returns true if the script passed is an ECDSA pay-to-pubkey.
/// @param script - The script ({@link HexString} or Uint8Array).
/// @category Wallet SDK
#[wasm_bindgen(js_name = isScriptPayToPubkeyECDSA)]
pub fn is_script_pay_to_pubkey_ecdsa(script: BinaryT) -> Result<bool> {
    let script = script.try_as_vec_u8()?;
    Ok(ScriptClass::is_pay_to_pubkey_ecdsa(script.as_slice()))
}

/// Returns true if the script passed is a pay-to-script-hash (P2SH) format, false otherwise.
/// @param script - The script ({@link HexString} or Uint8Array).
/// @category Wallet SDK
#[wasm_bindgen(js_name = isScriptPayToScriptHash)]
pub fn is_script_pay_to_script_hash(script: BinaryT) -> Result<bool> {
    let script = script.try_as_vec_u8()?;
    Ok(ScriptClass::is_pay_to_script_hash(script.as_slice()))
}
