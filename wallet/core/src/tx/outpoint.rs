use crate::imports::*;

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

impl TryFrom<JsValue> for TransactionOutpoint {
    type Error = Error;
    fn try_from(value: JsValue) -> Result<Self, Self::Error> {
        if value.is_object() {
            let object = Object::from(value);
            let transaction_id = object.get("transactionId")?.try_into()?;
            let index = object.get_u32("index")?;
            Ok(TransactionOutpoint::new(&transaction_id, index))
        } else {
            Err("outpoint is not an object".into())
        }
    }
}

impl TryFrom<cctx::TransactionOutpoint> for TransactionOutpoint {
    type Error = Error;
    fn try_from(outpoint: cctx::TransactionOutpoint) -> Result<Self, Self::Error> {
        let transaction_id = outpoint.transaction_id;
        let index = outpoint.index;
        Ok(TransactionOutpoint::new(&transaction_id, index))
    }
}

impl TryFrom<TransactionOutpoint> for cctx::TransactionOutpoint {
    type Error = Error;
    fn try_from(outpoint: TransactionOutpoint) -> Result<Self, Self::Error> {
        let inner = outpoint.inner();
        let transaction_id = inner.transaction_id;
        let index = inner.index;
        Ok(cctx::TransactionOutpoint::new(transaction_id, index))
    }
}
