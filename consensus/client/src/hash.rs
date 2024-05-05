use crate::imports::*;
use crate::result::Result;
use kaspa_hashes as native;
use kaspa_hashes::HasherBase;
use kaspa_wasm_core::types::BinaryT;

/// @category Wallet SDK
#[derive(Default, Clone)]
#[wasm_bindgen]
pub struct TransactionSigningHash {
    hasher: native::TransactionSigningHash,
}

#[wasm_bindgen]
impl TransactionSigningHash {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { hasher: native::TransactionSigningHash::new() }
    }

    pub fn update(&mut self, data: BinaryT) -> Result<()> {
        let data = JsValue::from(data).try_as_vec_u8()?;
        self.hasher.update(data);
        Ok(())
    }

    pub fn finalize(&self) -> String {
        self.hasher.clone().finalize().to_string()
    }
}

/// @category Wallet SDK
#[derive(Default, Clone)]
#[wasm_bindgen]
pub struct TransactionSigningHashECDSA {
    hasher: native::TransactionSigningHashECDSA,
}

#[wasm_bindgen]
impl TransactionSigningHashECDSA {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { hasher: native::TransactionSigningHashECDSA::new() }
    }

    pub fn update(&mut self, data: BinaryT) -> Result<()> {
        let data = JsValue::from(data).try_as_vec_u8()?;
        self.hasher.update(data);
        Ok(())
    }

    pub fn finalize(&self) -> String {
        self.hasher.clone().finalize().to_string()
    }
}
