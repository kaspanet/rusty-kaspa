use crate::imports::*;
use crate::tx::generator as core;

#[wasm_bindgen(inspectable)]
pub struct GeneratorSummary {
    inner: core::GeneratorSummary,
}

#[wasm_bindgen]
impl GeneratorSummary {
    #[wasm_bindgen(getter, js_name = networkType)]
    pub fn network_type(&self) -> NetworkType {
        self.inner.network_type()
    }

    #[wasm_bindgen(getter, js_name = utxos)]
    pub fn aggregated_utxos(&self) -> usize {
        self.inner.aggregated_utxos()
    }

    #[wasm_bindgen(getter, js_name = fees)]
    pub fn aggregated_fees(&self) -> BigInt {
        BigInt::from(self.inner.aggregated_fees())
    }

    #[wasm_bindgen(getter, js_name = transactions)]
    pub fn number_of_generated_transactions(&self) -> usize {
        self.inner.number_of_generated_transactions()
    }

    #[wasm_bindgen(getter, js_name = finalAmount)]
    pub fn final_transaction_amount(&self) -> Option<BigInt> {
        self.inner.final_transaction_amount().map(BigInt::from)
    }

    #[wasm_bindgen(getter, js_name = finalTransactionId)]
    pub fn final_transaction_id(&self) -> Option<TransactionId> {
        self.inner.final_transaction_id().map(Into::into)
    }
}

impl From<core::GeneratorSummary> for GeneratorSummary {
    fn from(inner: core::GeneratorSummary) -> Self {
        Self { inner }
    }
}
