//!
//! General-purpose types for WASM bindings
//!

use std::str;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_HEX_STRING: &'static str = r#"
/**
 * A string containing a hexadecimal representation of the data (typically representing for IDs or Hashes).
 * 
 * @category General
 */ 
export type HexString = string;
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "HexString")]
    pub type HexString;
}

impl From<String> for HexString {
    fn from(s: String) -> Self {
        JsValue::from(s).into()
    }
}

impl TryFrom<HexString> for String {
    type Error = &'static str;

    fn try_from(value: HexString) -> std::result::Result<String, Self::Error> {
        value.as_string().ok_or("Supplied value is not a string")
    }
}

impl From<&[u8]> for HexString {
    fn from(bytes: &[u8]) -> Self {
        let mut hex = vec![0u8; bytes.len() * 2];
        faster_hex::hex_encode(bytes, hex.as_mut_slice()).expect("The output is exactly twice the size of the input");
        let result = unsafe { str::from_utf8_unchecked(&hex) };
        JsValue::from(result).into()
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "Array<string>")]
    pub type StringArray;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "Array<number>")]
    pub type NumberArray;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "HexString | Uint8Array")]
    pub type BinaryT;
}
