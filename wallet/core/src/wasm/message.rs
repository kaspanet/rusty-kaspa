use crate::imports::*;
use crate::message::*;
use kaspa_consensus_wasm::{PrivateKey, PublicKey};

/// Signs a message with the given private key
/// @param {object} value - an object containing { message: String, privateKey: String|PrivateKey }
/// @returns {String} the signature, in hex string format
#[wasm_bindgen(js_name = signMessage, skip_jsdoc)]
pub fn js_sign_message(value: JsValue) -> Result<String, Error> {
    if let Some(object) = Object::try_from(&value) {
        let private_key = object.get::<PrivateKey>("privateKey")?;
        let raw_msg = object.get_string("message")?;
        let mut privkey_bytes = [0u8; 32];

        privkey_bytes.copy_from_slice(&private_key.secret_bytes());

        let pm = PersonalMessage(&raw_msg);

        let sig_vec = sign_message(&pm, &privkey_bytes)?;

        Ok(faster_hex::hex_string(sig_vec.as_slice()))
    } else {
        Err(Error::custom("Failed to parse input"))
    }
}

/// Verifies with a public key the signature of the given message
/// @param {object} value - an object containing { message: String, signature: String, publicKey: String|PublicKey }
/// @returns {bool} true if the signature can be verified with the given public key and message, false otherwise
#[wasm_bindgen(js_name = verifyMessage, skip_jsdoc)]
pub fn js_verify_message(value: JsValue) -> Result<bool, Error> {
    if let Some(object) = Object::try_from(&value) {
        let public_key = object.get::<PublicKey>("publicKey")?;
        let raw_msg = object.get_string("message")?;
        let signature = object.get_string("signature")?;

        let pm = PersonalMessage(&raw_msg);
        let mut signature_bytes = [0u8; 64];
        faster_hex::hex_decode(signature.as_bytes(), &mut signature_bytes)?;

        Ok(verify_message(&pm, &signature_bytes.to_vec(), &public_key.into()).is_ok())
    } else {
        Err(Error::custom("Failed to parse input"))
    }
}
