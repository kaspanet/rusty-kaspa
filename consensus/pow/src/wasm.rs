use kaspa_consensus_core::wasm::Header;
use kaspa_math::wasm::Uint256;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct State {
    inner: crate::State,
}

#[wasm_bindgen]
impl State {
    #[wasm_bindgen(constructor)]
    pub fn new(header: &Header) -> Self {
        Self { inner: crate::State::new(header.inner()) }
    }

    #[wasm_bindgen(js_name=checkPow)]
    pub fn check_pow(&self, nonce: u64) -> js_sys::Array {
        let (c, v) = self.inner.check_pow(nonce);
        let array = js_sys::Array::new();
        array.push(&JsValue::from(c));
        array.push(&Uint256::from(v).into());

        array
    }
}
