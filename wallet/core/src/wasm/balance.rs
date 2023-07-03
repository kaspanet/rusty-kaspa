use crate::imports::*;
use crate::runtime;
use js_sys::BigInt;

#[derive(Debug, Clone)]
#[wasm_bindgen]
pub struct Balance {
    #[wasm_bindgen(getter_with_clone)]
    pub mature: BigInt,
    #[wasm_bindgen(getter_with_clone)]
    pub pending: BigInt,
}

impl From<runtime::Balance> for Balance {
    fn from(balance: runtime::Balance) -> Self {
        Self { mature: balance.mature.into(), pending: balance.pending.into() }
    }
}
