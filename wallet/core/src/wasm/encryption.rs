#![allow(non_snake_case)]

use crate::encryption::*;
use crate::imports::*;
use base64::{engine::general_purpose, Engine as _};
use kaspa_wasm_core::types::BinaryT;
use kaspa_wasm_core::types::HexString;

/// WASM32 binding for `encryptXChaCha20Poly1305` function.
/// @returns The encrypted text as a base64 string.
/// @category Encryption
#[wasm_bindgen(js_name = "encryptXChaCha20Poly1305")]
pub fn js_encrypt_xchacha20poly1305(plainText: String, password: String) -> Result<String> {
    let secret = sha256_hash(password.as_bytes());
    let encrypted = encrypt_xchacha20poly1305(plainText.as_bytes(), &secret)?;
    Ok(general_purpose::STANDARD.encode(encrypted))
}

/// WASM32 binding for `decryptXChaCha20Poly1305` function.
/// @category Encryption
#[wasm_bindgen(js_name = "decryptXChaCha20Poly1305")]
pub fn js_decrypt_xchacha20poly1305(base64string: String, password: String) -> Result<String> {
    let secret = sha256_hash(password.as_bytes());
    let bytes = general_purpose::STANDARD.decode(base64string)?;
    let decrypted = decrypt_xchacha20poly1305(bytes.as_ref(), &secret)?;
    Ok(String::from_utf8(decrypted.as_ref().to_vec())?)
}

/// WASM32 binding for `SHA256` hash function.
/// @param data - The data to hash ({@link HexString} or Uint8Array).
/// @category Encryption
#[wasm_bindgen(js_name = "sha256FromBinary")]
pub fn js_sha256_hash_from_binary(data: BinaryT) -> Result<HexString> {
    let data = data.try_as_vec_u8()?;
    let hash = sha256_hash(&data);
    Ok(hash.as_ref().to_hex().into())
}

/// WASM32 binding for `SHA256` hash function.
/// @param {string} text - The text string to hash.
/// @category Encryption
#[wasm_bindgen(js_name = "sha256FromText")]
pub fn js_sha256_hash_from_text(text: String) -> Result<HexString> {
    let data = text.as_bytes();
    let hash = sha256_hash(data);
    Ok(hash.as_ref().to_hex().into())
}

/// WASM32 binding for `SHA256d` hash function.
/// @param data - The data to hash ({@link HexString} or Uint8Array).
/// @category Encryption
#[wasm_bindgen(js_name = "sha256dFromBinary")]
pub fn js_sha256d_hash_from_binary(data: BinaryT) -> Result<HexString> {
    let data = data.try_as_vec_u8()?;
    let hash = sha256d_hash(&data);
    Ok(hash.as_ref().to_hex().into())
}

/// WASM32 binding for `SHA256d` hash function.
/// @param {string} text - The text string to hash.
/// @category Encryption
#[wasm_bindgen(js_name = "sha256dFromText")]
pub fn js_sha256d_hash_from_text(text: String) -> Result<HexString> {
    let data = text.as_bytes();
    let hash = sha256d_hash(data);
    Ok(hash.as_ref().to_hex().into())
}

/// WASM32 binding for `argon2sha256iv` hash function.
/// @param data - The data to hash ({@link HexString} or Uint8Array).
/// @category Encryption
#[wasm_bindgen(js_name = "argon2sha256ivFromBinary")]
#[allow(non_snake_case)]
pub fn js_argon2_sha256iv_phash_from_binary(data: BinaryT, hashLength: usize) -> Result<HexString> {
    let data = data.try_as_vec_u8()?;
    let hash = argon2_sha256iv_hash(&data, hashLength)?;
    Ok(hash.as_ref().to_hex().into())
}

/// WASM32 binding for `argon2sha256iv` hash function.
/// @param text - The text string to hash.
/// @category Encryption
#[wasm_bindgen(js_name = "argon2sha256ivFromText")]
#[allow(non_snake_case)]
pub fn js_argon2_sha256iv_phash_from_text(text: String, byteLength: usize) -> Result<HexString> {
    let data = text.as_bytes();
    let hash = argon2_sha256iv_hash(data, byteLength)?;
    Ok(hash.as_ref().to_hex().into())
}
