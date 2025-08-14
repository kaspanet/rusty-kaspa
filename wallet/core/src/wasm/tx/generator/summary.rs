use crate::imports::*;
use crate::tx::generator as core;

///
/// A class containing a summary produced by transaction {@link Generator}.
/// This class contains the number of transactions, the aggregated fees,
/// the aggregated UTXOs and the final transaction amount that includes
/// both network and QoS (priority) fees.
///
/// @see {@link createTransactions}, {@link IGeneratorSettingsObject}, {@link Generator}
/// @category Wallet SDK
///
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
    pub fn aggregate_fees(&self) -> BigInt {
        BigInt::from(self.inner.aggregate_fees())
    }

    #[wasm_bindgen(getter, js_name = mass)]
    pub fn aggregate_mass(&self) -> BigInt {
        BigInt::from(self.inner.aggregate_mass())
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
    pub fn final_transaction_id(&self) -> Option<String> {
        self.inner.final_transaction_id().map(|id| id.to_string())
    }
}

impl From<core::GeneratorSummary> for GeneratorSummary {
    fn from(inner: core::GeneratorSummary) -> Self {
        Self { inner }
    }
}
