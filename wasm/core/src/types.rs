//!
//! General-purpose types for WASM bindings
//!

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
        s.into()
    }
}

impl TryFrom<HexString> for String {
    type Error = &'static str;

    fn try_from(value: HexString) -> Result<String, Self::Error> {
        value.as_string().ok_or("Supplied value is not a string")
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "Array<string>")]
    pub type StringArray;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "HexString | Uint8Array")]
    pub type BinaryT;
}
