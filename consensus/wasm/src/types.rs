//!
//! General-purpose types for WASM bindings
//!

use wasm_bindgen::prelude::*;

// export type HexString = string;

#[wasm_bindgen(typescript_custom_section)]
const TS_HEX_STRING: &'static str = r#"
/**
 * Internal `Opaque` type used for type restrictions.
 * 
 * @category General
 */
type Opaque<K, T> = T & { __TYPE__: K };

/**
 * A string restricted to contain only hexadecimal characters (typically representing for IDs or Hashes).
 * 
 * @category General
 */ 
export type HexString = Opaque<string, "HexString">
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "HexString")]
    pub type HexString;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "Array<string>")]
    pub type StringArray;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "Array<string> | undefined")]
    pub type StringArrayOrNone;
}
