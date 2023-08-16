use crate::imports::*;
use crate::result::Result;
use crate::utxo as native;
use crate::utxo::{
    UtxoContextBinding,
    UtxoContextId,
    UtxoEntryId,
    UtxoEntryReference,
    //    UtxoEntryReferenceExtension,
};
use crate::wasm::utxo::UtxoProcessor;
use kaspa_hashes::Hash;
// use serde_wasm_bindgen::from_value;
// use workflow_wasm::prelude::*;
use kaspa_rpc_core::GetUtxosByAddressesResponse;
use workflow_wasm::serde::*;

#[derive(Clone)]
#[wasm_bindgen(inspectable)]
pub struct UtxoContext {
    inner: native::UtxoContext,
}

impl UtxoContext {
    pub fn inner(&self) -> &native::UtxoContext {
        &self.inner
    }
}

#[wasm_bindgen]
impl UtxoContext {
    pub fn js_remove(&self, ids: Array) -> Result<Array> {
        let vec = ids
            .to_vec()
            .iter()
            .map(UtxoEntryId::try_from)
            .collect::<std::result::Result<Vec<UtxoEntryId>, kaspa_consensus_wasm::error::Error>>()?;

        let mut context = self.inner.context();

        let mut removed = vec![];
        for id in vec.iter() {
            if let Some(entry) = context.map.remove(id) {
                removed.push(entry)
            }
        }

        for entry in removed.iter() {
            if context.consumed.remove(&entry.id()).is_none() {
                context.mature.retain(|entry| entry.id() != entry.id());
            }
        }

        Ok(removed.into_iter().map(JsValue::from).collect::<Array>())
    }

    // #[wasm_bindgen(constructor)]
    pub fn from(processor: &JsValue, id: Hash, utxo_by_address_response: JsValue) -> Result<UtxoContext> {
        let processor = ref_from_abi!(UtxoProcessor, processor)?;
        let r: GetUtxosByAddressesResponse = from_value(utxo_by_address_response)?;
        let mut entries = r.entries.into_iter().map(|entry| entry.into()).collect::<Vec<UtxoEntryReference>>();
        entries.sort_by_key(|e| e.amount());
        let binding = UtxoContextBinding::Id(UtxoContextId::new(id));
        let inner = native::UtxoContext::new_with_mature_entries(processor.inner(), binding, entries);
        Ok(UtxoContext { inner })
    }

    #[wasm_bindgen(js_name=calculateBalance)]
    pub async fn js_calculate_balance(&self) -> crate::wasm::wallet::Balance {
        self.inner.calculate_balance().await.into()
    }
}
