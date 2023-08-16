use crate::imports::*;
// use crate::result::Result;
use crate::utxo as native;

#[derive(Clone)]
#[wasm_bindgen(inspectable)]
pub struct UtxoProcessor {
    inner: native::UtxoProcessor,
}

impl UtxoProcessor {
    pub fn inner(&self) -> &native::UtxoProcessor {
        &self.inner
    }
}

#[wasm_bindgen]
impl UtxoProcessor {}
