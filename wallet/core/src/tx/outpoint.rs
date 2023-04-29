use crate::imports::*;
use kaspa_hashes::Hash;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct TransactionOutpointInner {
    pub transaction_id: TransactionId,
    pub index: TransactionIndexType,
}

/// Represents a Kaspa transaction outpoint
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct TransactionOutpoint {
    inner: Arc<Mutex<TransactionOutpointInner>>,
}

impl TransactionOutpoint {
    pub fn inner(&self) -> MutexGuard<'_, TransactionOutpointInner> {
        self.inner.lock().unwrap()
    }
}

#[wasm_bindgen]
impl TransactionOutpoint {
    #[wasm_bindgen(constructor)]
    pub fn new(transaction_id: &str, index: u32) -> crate::Result<TransactionOutpoint> {
        Ok(Self { inner: Arc::new(Mutex::new(TransactionOutpointInner { transaction_id: Hash::from_str(transaction_id)?, index })) })
    }

    #[wasm_bindgen(js_name = "getId")]
    pub fn id_string(&self) -> String {
        format!("{}-{}", self.get_transaction_id(), self.get_index())
    }

    #[wasm_bindgen(getter, js_name = transactionId)]
    pub fn get_transaction_id(&self) -> String {
        self.inner().transaction_id.to_string()
    }

    #[wasm_bindgen(setter, js_name = transactionId)]
    pub fn set_transaction_id(&self, transaction_id: &str) -> Result<(), Error> {
        self.inner().transaction_id = Hash::from_str(transaction_id)?;
        Ok(())
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
            let transaction_id: TransactionId = object.get("transactionId")?.try_into()?;
            let index = object.get_u32("index")?;
            Ok(TransactionOutpoint::new(&transaction_id.to_string(), index)?)
        } else {
            Err("outpoint is not an object".into())
        }
    }
}

impl TryFrom<cctx::TransactionOutpoint> for TransactionOutpoint {
    type Error = Error;
    fn try_from(outpoint: cctx::TransactionOutpoint) -> Result<Self, Self::Error> {
        let transaction_id = outpoint.transaction_id.to_string();
        let index = outpoint.index;
        TransactionOutpoint::new(&transaction_id, index)
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
