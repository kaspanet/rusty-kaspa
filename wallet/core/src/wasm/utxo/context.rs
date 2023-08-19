use crate::imports::*;
use crate::result::Result;
// use crate::error::Error;
use crate::utxo as native;
use crate::utxo::{
    UtxoContextBinding,
    UtxoContextId,
    // UtxoEntryId,
    // UtxoEntryReference,
    //    UtxoEntryReferenceExtension,
};
use crate::wasm::utxo::UtxoProcessor;
use kaspa_hashes::Hash;
// use serde_wasm_bindgen::from_value;
// use workflow_wasm::prelude::*;
// use kaspa_rpc_core::GetUtxosByAddressesResponse;
// use kaspa_wrpc_client::wasm::RpcClient;
// use workflow_wasm::serde::from_value;

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
    #[wasm_bindgen(constructor)]
    pub fn ctor(processor: &JsValue, optional_hash_id: JsValue) -> Result<UtxoContext> {
        let binding = if optional_hash_id.is_falsy() {
            UtxoContextBinding::default()
        } else {
            let hash = Hash::try_from(optional_hash_id)?;
            UtxoContextBinding::Id(UtxoContextId::new(hash))
        };

        let processor = ref_from_abi!(UtxoProcessor, processor)?;
        let inner = native::UtxoContext::new(processor.inner(), binding);
        Ok(UtxoContext { inner })
    }

    // pub fn from(processor: &JsValue, id: Hash, utxo_by_address_response: JsValue) -> Result<UtxoContext> {
    //     let processor = ref_from_abi!(UtxoProcessor, processor)?;
    //     let r: GetUtxosByAddressesResponse = from_value(utxo_by_address_response)?;
    //     let mut entries = r.entries.into_iter().map(|entry| entry.into()).collect::<Vec<UtxoEntryReference>>();
    //     entries.sort_by_key(|e| e.amount());
    //     let binding = UtxoContextBinding::Id(UtxoContextId::new(id));
    //     let inner = native::UtxoContext::new_with_mature_entries(processor.inner(), binding, entries);
    //     Ok(UtxoContext { inner })
    // }

    // pub fn remove(&self, ids: Array) -> Result<Array> {
    //     let vec = ids
    //         .to_vec()
    //         .iter()
    //         .map(UtxoEntryId::try_from)
    //         .collect::<std::result::Result<Vec<UtxoEntryId>, kaspa_consensus_wasm::error::Error>>()?;

    //     let mut context = self.inner.context();

    //     let mut removed = vec![];
    //     for id in vec.iter() {
    //         if let Some(entry) = context.map.remove(id) {
    //             removed.push(entry)
    //         }
    //     }

    //     for entry in removed.iter() {
    //         if context.consumed.remove(&entry.id()).is_none() {
    //             context.mature.retain(|entry| entry.id() != entry.id());
    //         }
    //     }

    //     Ok(removed.into_iter().map(JsValue::from).collect::<Array>())
    // }

    // #[wasm_bindgen(constructor)]

    #[wasm_bindgen(js_name=calculateBalance)]
    pub async fn calculate_balance(&self) -> crate::wasm::wallet::Balance {
        self.inner.calculate_balance().await.into()
    }
}

impl From<native::UtxoContext> for UtxoContext {
    fn from(inner: native::UtxoContext) -> Self {
        Self { inner }
    }
}

// pub struct UtxoContextCreateArgs {
//     utxo_processor : UtxoProcessor
// }

// impl TryFrom<JsValue> for UtxoContextCreateArgs {
//     type Error = Error;
//     fn try_from(value: JsValue) -> std::result::Result<Self, Self::Error> {
//         if let Some(object) = Object::try_from(&value) {

//             let processor = UtxoProcessor::try_from(object.get_value())
//             let rpc = object.get_value("rpc")?;
//             let rpc = ref_from_abi!(RpcClient, &rpc)?;

//             let network_id = object.get::<NetworkId>("network_id")?;

//             // let network_id = NetworkId::try_from(object.get("network_id"))?;
//             // let

//             Ok(UtxoProcessorCreateArgs { rpc, network_id })
//         } else {
//             Err(Error::custom("UtxoProcessor: suppliedd value must be an object"))
//         }
//     }
// }
