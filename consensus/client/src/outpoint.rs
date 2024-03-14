use crate::imports::*;
use crate::result::Result;

#[wasm_bindgen(typescript_custom_section)]
const TS_TRANSACTION_OUTPOINT: &'static str = r#"
/**
 * Interface defines the structure of a transaction outpoint (used by transaction input).
 * 
 * @category Consensus
 */
export interface ITransactionOutpoint {
    transactionId: HexString;
    index: number;
}
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub struct TransactionOutpointInner {
    pub transaction_id: TransactionId,
    pub index: TransactionIndexType,
}

impl std::fmt::Display for TransactionOutpointInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.transaction_id, self.index)
    }
}

impl TransactionOutpointInner {
    pub fn new(transaction_id: TransactionId, index: TransactionIndexType) -> Self {
        Self { transaction_id, index }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = self.transaction_id.as_bytes().to_vec();
        data.extend(self.index.to_be_bytes());
        data
    }
}

impl From<cctx::TransactionOutpoint> for TransactionOutpointInner {
    fn from(outpoint: cctx::TransactionOutpoint) -> Self {
        TransactionOutpointInner { transaction_id: outpoint.transaction_id, index: outpoint.index }
    }
}

impl TryFrom<&JsValue> for TransactionOutpointInner {
    type Error = Error;
    fn try_from(js_value: &JsValue) -> Result<Self, Self::Error> {
        if let Some(string) = js_value.as_string() {
            let vec = string.split('-').collect::<Vec<_>>();
            if vec.len() == 2 {
                let transaction_id: TransactionId = vec[0].parse()?;
                let id: u32 = vec[1].parse()?;
                Ok(TransactionOutpointInner::new(transaction_id, id))
            } else {
                Err(Error::InvalidTransactionOutpoint(string))
            }
        } else if let Some(object) = js_sys::Object::try_from(js_value) {
            let transaction_id: TransactionId = object.get_value("transactionId")?.try_into_owned()?;
            let index = object.get_u32("index")?;
            Ok(TransactionOutpointInner::new(transaction_id, index))
        } else {
            Err("outpoint is not an object".into())
        }
    }
}

/// Represents a Kaspa transaction outpoint.
/// NOTE: This struct is immutable - to create a custom outpoint
/// use the `TransactionOutpoint::new` constructor. (in JavaScript
/// use `new TransactionOutpoint(transactionId, index)`).
/// @category Consensus
#[derive(Clone, Debug, Serialize, Deserialize, CastFromJs)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct TransactionOutpoint {
    inner: Arc<TransactionOutpointInner>,
}

impl TransactionOutpoint {
    pub fn new(transaction_id: TransactionId, index: u32) -> TransactionOutpoint {
        Self { inner: Arc::new(TransactionOutpointInner { transaction_id, index }) }
    }

    #[inline(always)]
    pub fn inner(&self) -> &TransactionOutpointInner {
        &self.inner
    }

    #[inline(always)]
    pub fn transaction_id(&self) -> TransactionId {
        self.inner().transaction_id
    }

    #[inline(always)]
    pub fn index(&self) -> TransactionIndexType {
        self.inner().index
    }

    #[inline(always)]
    pub fn transaction_id_as_ref(&self) -> &TransactionId {
        &self.inner().transaction_id
    }

    #[inline(always)]
    pub fn id(&self) -> &TransactionOutpointInner {
        self.inner()
    }
}

#[cfg_attr(feature = "wasm32-sdk", wasm_bindgen)]
impl TransactionOutpoint {
    #[cfg_attr(feature = "wasm32-sdk", wasm_bindgen(constructor))]
    pub fn ctor(transaction_id: TransactionId, index: u32) -> TransactionOutpoint {
        Self { inner: Arc::new(TransactionOutpointInner { transaction_id, index }) }
    }

    #[cfg_attr(feature = "wasm32-sdk", wasm_bindgen(js_name = "getId"))]
    pub fn id_string(&self) -> String {
        format!("{}-{}", self.get_transaction_id_as_string(), self.get_index())
    }

    #[cfg_attr(feature = "wasm32-sdk", wasm_bindgen(getter, js_name = transactionId))]
    pub fn get_transaction_id_as_string(&self) -> String {
        self.inner().transaction_id.to_string()
    }

    #[cfg_attr(feature = "wasm32-sdk", wasm_bindgen(getter, js_name = index))]
    pub fn get_index(&self) -> TransactionIndexType {
        self.inner().index
    }
}

impl std::fmt::Display for TransactionOutpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner();
        write!(f, "({}, {})", inner.transaction_id, inner.index)
    }
}

impl TryFrom<&JsValue> for TransactionOutpoint {
    type Error = Error;
    fn try_from(js_value: &JsValue) -> Result<Self, Self::Error> {
        let inner: TransactionOutpointInner = js_value.as_ref().try_into()?;
        Ok(TransactionOutpoint { inner: Arc::new(inner) })
    }
}

impl From<cctx::TransactionOutpoint> for TransactionOutpoint {
    fn from(outpoint: cctx::TransactionOutpoint) -> Self {
        let transaction_id = outpoint.transaction_id;
        let index = outpoint.index;
        TransactionOutpoint::new(transaction_id, index)
    }
}

impl From<TransactionOutpoint> for cctx::TransactionOutpoint {
    fn from(outpoint: TransactionOutpoint) -> Self {
        let inner = outpoint.inner();
        let transaction_id = inner.transaction_id;
        let index = inner.index;
        cctx::TransactionOutpoint::new(transaction_id, index)
    }
}

impl TransactionOutpoint {
    pub fn simulated() -> Self {
        Self::new(TransactionId::from_slice(&rand::random::<[u8; kaspa_hashes::HASH_SIZE]>()), 0)
    }
}
