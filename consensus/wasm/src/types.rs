//!
//! General-purpose types for WASM bindings
//!

use wasm_bindgen::prelude::*;

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
