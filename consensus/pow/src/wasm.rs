use js_sys::BigInt;
use kaspa_consensus_core::header::Header;
use wasm_bindgen::prelude::*;
use workflow_wasm::error::Error;
use workflow_wasm::jsvalue::*;
use workflow_wasm::result::Result;

#[wasm_bindgen(inspectable)]
pub struct State {
    inner: crate::State,
}

#[wasm_bindgen]
impl State {
    #[wasm_bindgen(constructor)]
    pub fn new(header: &Header) -> Self {
        Self { inner: crate::State::new(header) }
    }

    #[wasm_bindgen(getter)]
    pub fn target(&self) -> Result<BigInt> {
        self.inner.target.try_into().map_err(|err| Error::Custom(format!("{err:?}")))
    }

    #[wasm_bindgen(js_name=checkPow)]
    pub fn check_pow(&self, nonce_jsv: JsValue) -> Result<js_sys::Array> {
        let nonce = nonce_jsv.try_as_u64()?;
        let (c, v) = self.inner.check_pow(nonce);
        let array = js_sys::Array::new();
        array.push(&JsValue::from(c));
        array.push(&v.to_bigint().map_err(|err| Error::Custom(format!("{err:?}")))?.into());

        Ok(array)
    }
}
