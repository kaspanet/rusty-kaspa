use crate::imports::*;
use crate::runtime;
// use crate::iterator::*;

#[derive(Clone)]
#[wasm_bindgen]
pub struct Wallet {
    _inner: Arc<runtime::Wallet>,
}

// #[wasm_bindgen(constructor)]
// pub fn constructor(_js_value: JsValue) -> std::result::Result<Wallet, JsError> {
//     todo!();
//     // Ok(js_value.try_into()?)
// }
