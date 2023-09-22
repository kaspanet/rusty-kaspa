use crate::message::*;
use kaspa_consensus_wasm::{PrivateKey, PublicKey};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_name = signMessage)]
pub fn js_sign_message(raw_msg: String, privkey: JsValue) -> String {
    let mut privkey_bytes = [0u8; 32];

    let private_key = PrivateKey::try_from(privkey).unwrap_throw();
    privkey_bytes.copy_from_slice(&private_key.secret_bytes());

    let pm = PersonalMessage(&raw_msg);

    let sig_result = sign_message(&pm, &privkey_bytes);
    let sig_vec = sig_result.unwrap_throw();

    faster_hex::hex_string(sig_vec.as_slice())
}

#[wasm_bindgen(js_name = verifyMessage)]
pub fn js_verify_message(raw_msg: String, signature: String, pubkey: JsValue) -> bool {
    let public_key = PublicKey::try_from(pubkey).unwrap_throw();

    let pm = PersonalMessage(&raw_msg);
    let mut signature_bytes = [0u8; 64];
    faster_hex::hex_decode(signature.as_bytes(), &mut signature_bytes).unwrap_throw();

    verify_message(&pm, &signature_bytes.to_vec(), &public_key.into()).is_ok()
}
