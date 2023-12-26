use crate::imports::*;
use crate::result::Result;
use crate::utxo::balance as native;

#[wasm_bindgen]
pub struct Balance {
    inner: native::Balance,
}

#[wasm_bindgen]
impl Balance {
    #[wasm_bindgen(getter)]
    pub fn mature(&self) -> BigInt {
        self.inner.mature.into()
    }

    #[wasm_bindgen(getter)]
    pub fn pending(&self) -> BigInt {
        self.inner.pending.into()
    }

    pub fn as_strings(&self, network_type: JsValue) -> Result<BalanceStrings> {
        let network_type = NetworkType::try_from(network_type)?;
        Ok(native::BalanceStrings::from((&Some(self.inner.clone()), &network_type, None)).into())
    }
}

impl From<native::Balance> for Balance {
    fn from(inner: native::Balance) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
pub struct BalanceStrings {
    inner: native::BalanceStrings,
}

#[wasm_bindgen]
impl BalanceStrings {
    #[wasm_bindgen(getter)]
    pub fn mature(&self) -> JsValue {
        self.inner.mature.clone().into()
    }

    #[wasm_bindgen(getter)]
    pub fn pending(&self) -> JsValue {
        self.inner.pending.clone().into()
    }
}

impl From<native::BalanceStrings> for BalanceStrings {
    fn from(inner: native::BalanceStrings) -> Self {
        Self { inner }
    }
}
