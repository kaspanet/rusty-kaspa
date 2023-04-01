use kaspa_consensus_core::tx::{TransactionId, TransactionIndexType};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, MutexGuard};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionOutpointInner {
    pub transaction_id: TransactionId,
    pub index: TransactionIndexType,
}

/// Represents a Kaspa transaction outpoint
// #[derive(Eq, Hash, PartialEq, Debug, Clone)]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct TransactionOutpoint {
    // #[wasm_bindgen(js_name = transactionId)]
    inner: Arc<Mutex<TransactionOutpointInner>>,
}

impl TransactionOutpoint {
    pub fn inner(&self) -> MutexGuard<'_, TransactionOutpointInner> {
        self.inner.lock().unwrap()
    }

    //     pub fn new(transaction_id: TransactionId, index: u32) -> Self {
    //         Self { inner : Arc::new(Mutex::new( TransactionOutpointInner { transaction_id, index })) }
    //     }
}

#[wasm_bindgen]
impl TransactionOutpoint {
    #[wasm_bindgen(constructor)]
    pub fn new(transaction_id: &TransactionId, index: u32) -> Self {
        Self { inner: Arc::new(Mutex::new(TransactionOutpointInner { transaction_id: *transaction_id, index })) }
    }

    #[wasm_bindgen(getter, js_name = transactionId)]
    pub fn get_transaction_id(&self) -> TransactionId {
        self.inner().transaction_id
    }

    #[wasm_bindgen(setter, js_name = transactionId)]
    pub fn set_transaction_id(&self, transaction_id: &TransactionId) {
        self.inner().transaction_id = *transaction_id;
    }

    #[wasm_bindgen(getter, js_name = index)]
    pub fn get_index(&self) -> TransactionIndexType {
        self.inner().index
    }
    #[wasm_bindgen(setter, js_name = index)]
    pub fn set_index(&self, index: TransactionIndexType) {
        self.inner().index = index;
    }
}

impl std::fmt::Display for TransactionOutpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.lock().unwrap();
        write!(f, "({}, {})", inner.transaction_id, inner.index)
    }
}
