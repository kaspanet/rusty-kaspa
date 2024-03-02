use crate::imports::*;
use crate::message::*;
use kaspa_wallet_keys::privatekey::PrivateKey;
use kaspa_wallet_keys::publickey::PublicKey;
use kaspa_wasm_core::types::HexString;

#[wasm_bindgen(typescript_custom_section)]
const TS_MESSAGE_TYPES: &'static str = r#"
/**
 * Interface declaration for {@link signMessage} function arguments.
 * 
 * @category Message Signing
 */
export interface ISignMessage {
    message: string;
    privateKey: PrivateKey | string;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Object, typescript_type = "ISignMessage")]
    pub type ISignMessage;
}

/// Signs a message with the given private key
/// @category Message Signing
#[wasm_bindgen(js_name = signMessage)]
pub fn js_sign_message(value: ISignMessage) -> Result<HexString, Error> {
    if let Some(object) = Object::try_from(&value) {
        let private_key = object.get_cast::<PrivateKey>("privateKey")?;
        let raw_msg = object.get_string("message")?;
        let mut privkey_bytes = [0u8; 32];
        privkey_bytes.copy_from_slice(&private_key.as_ref().secret_bytes());
        let pm = PersonalMessage(&raw_msg);
        let sig_vec = sign_message(&pm, &privkey_bytes)?;
        privkey_bytes.zeroize();
        Ok(faster_hex::hex_string(sig_vec.as_slice()).into())
    } else {
        Err(Error::custom("Failed to parse input"))
    }
}

#[wasm_bindgen(typescript_custom_section)]
const TS_MESSAGE_TYPES: &'static str = r#"
/**
 * Interface declaration for {@link verifyMessage} function arguments.
 * 
 * @category Message Signing
 */
export interface IVerifyMessage {
    message: string;
    signature: HexString;
    publicKey: PublicKey | string;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Object, typescript_type = "IVerifyMessage")]
    pub type IVerifyMessage;
}

/// Verifies with a public key the signature of the given message
/// @category Message Signing
#[wasm_bindgen(js_name = verifyMessage, skip_jsdoc)]
pub fn js_verify_message(value: IVerifyMessage) -> Result<bool, Error> {
    if let Some(object) = Object::try_from(&value) {
        let public_key = object.get_cast::<PublicKey>("publicKey")?;
        let raw_msg = object.get_string("message")?;
        let signature = object.get_string("signature")?;

        let pm = PersonalMessage(&raw_msg);
        let mut signature_bytes = [0u8; 64];
        faster_hex::hex_decode(signature.as_bytes(), &mut signature_bytes)?;

        Ok(verify_message(&pm, &signature_bytes.to_vec(), &public_key.as_ref().xonly_public_key).is_ok())
    } else {
        Err(Error::custom("Failed to parse input"))
    }
}
