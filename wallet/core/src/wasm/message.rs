use crate::message::*;
use kaspa_consensus_wasm::{PrivateKey, PublicKey};
use secp256k1::XOnlyPublicKey;
use std::str::FromStr;
use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::ref_from_abi;

#[wasm_bindgen(js_name = signMessage)]
pub fn js_sign_message(raw_msg: String, privkey: JsValue) -> String {
    let mut privkey_bytes = [0u8; 32];

    if let Ok(privkey) = ref_from_abi!(PrivateKey, &privkey) {
        privkey_bytes.copy_from_slice(&privkey.secret_bytes());
    } else {
        faster_hex::hex_decode(privkey.as_string().unwrap_throw().as_bytes(), &mut privkey_bytes).unwrap_throw();
    };

    let pm = PersonalMessage(&raw_msg);

    let sig_result = sign_message(&pm, &privkey_bytes);
    let sig_vec = sig_result.unwrap_throw();

    faster_hex::hex_string(sig_vec.as_slice())
}

#[wasm_bindgen(js_name = verifyMessage)]
pub fn js_verify_message(raw_msg: String, signature: String, pubkey: JsValue) -> bool {
    let x_only_public_key = if let Ok(pubkey) = ref_from_abi!(PublicKey, &pubkey) {
        pubkey.into()
    } else {
        let pubkey_str = pubkey.as_string().unwrap_throw();
        XOnlyPublicKey::from_str(pubkey_str.as_str()).unwrap_throw()
    };

    let pm = PersonalMessage(&raw_msg);
    let mut signature_bytes = [0u8; 64];
    faster_hex::hex_decode(signature.as_bytes(), &mut signature_bytes).unwrap_throw();

    verify_message(&pm, &signature_bytes.to_vec(), &x_only_public_key).is_ok()
}
