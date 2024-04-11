use crate::imports::*;
use crate::result::Result;
use crate::TransactionOutpoint;
use crate::UtxoEntryReference;
use kaspa_utils::hex::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_TRANSACTION: &'static str = r#"
/**
 * Interface defines the structure of a transaction input.
 * 
 * @category Consensus
 */
export interface ITransactionInput {
    previousOutpoint: ITransactionOutpoint;
    signatureScript: HexString;
    sequence: bigint;
    sigOpCount: number;
    utxo?: UtxoEntryReference;

    /** Optional verbose data provided by RPC */
    verboseData?: ITransactionInputVerboseData;
}

/**
 * Option transaction input verbose data.
 * 
 * @category Node RPC
 */
export interface ITransactionInputVerboseData { }

"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "ITransactionInput")]
    pub type ITransactionInput;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInputInner {
    pub previous_outpoint: TransactionOutpoint,
    pub signature_script: Vec<u8>,
    pub sequence: u64,
    pub sig_op_count: u8,
    pub utxo: Option<UtxoEntryReference>,
}

impl TransactionInputInner {
    pub fn new(
        previous_outpoint: TransactionOutpoint,
        signature_script: Vec<u8>,
        sequence: u64,
        sig_op_count: u8,
        utxo: Option<UtxoEntryReference>,
    ) -> Self {
        Self { previous_outpoint, signature_script, sequence, sig_op_count, utxo }
    }
}

/// Represents a Kaspa transaction input
/// @category Consensus
#[derive(Clone, Debug, Serialize, Deserialize, CastFromJs)]
#[wasm_bindgen(inspectable)]
pub struct TransactionInput {
    inner: Arc<Mutex<TransactionInputInner>>,
}

impl TransactionInput {
    pub fn new(
        previous_outpoint: TransactionOutpoint,
        signature_script: Vec<u8>,
        sequence: u64,
        sig_op_count: u8,
        utxo: Option<UtxoEntryReference>,
    ) -> Self {
        let inner = TransactionInputInner::new(previous_outpoint, signature_script, sequence, sig_op_count, utxo);
        Self { inner: Arc::new(Mutex::new(inner)) }
    }

    pub fn new_with_inner(inner: TransactionInputInner) -> Self {
        Self { inner: Arc::new(Mutex::new(inner)) }
    }

    pub fn inner(&self) -> MutexGuard<'_, TransactionInputInner> {
        self.inner.lock().unwrap()
    }

    pub fn sig_op_count(&self) -> u8 {
        self.inner().sig_op_count
    }

    pub fn utxo(&self) -> Option<UtxoEntryReference> {
        self.inner().utxo.clone()
    }
}

#[wasm_bindgen]
impl TransactionInput {
    #[wasm_bindgen(constructor)]
    pub fn constructor(value: &ITransactionInput) -> Result<TransactionInput> {
        Self::try_owned_from(value)
    }

    #[wasm_bindgen(getter = previousOutpoint)]
    pub fn get_previous_outpoint(&self) -> TransactionOutpoint {
        self.inner().previous_outpoint.clone()
    }

    #[wasm_bindgen(setter = previousOutpoint)]
    pub fn set_previous_outpoint(&mut self, js_value: &JsValue) -> Result<()> {
        match js_value.try_into() {
            Ok(outpoint) => {
                self.inner().previous_outpoint = outpoint;
                Ok(())
            }
            Err(_) => Err(Error::custom("invalid outpoint script".to_string())),
        }
    }

    #[wasm_bindgen(getter = signatureScript)]
    pub fn get_signature_script_as_hex(&self) -> String {
        self.inner().signature_script.to_hex()
    }

    #[wasm_bindgen(setter = signatureScript)]
    pub fn set_signature_script_from_js_value(&mut self, js_value: JsValue) -> Result<()> {
        match js_value.try_as_vec_u8() {
            Ok(signature) => {
                self.set_signature_script(signature);
                Ok(())
            }
            Err(_) => Err(Error::custom("invalid signature script".to_string())),
        }
    }

    #[wasm_bindgen(getter = sequence)]
    pub fn get_sequence(&self) -> u64 {
        self.inner().sequence
    }

    #[wasm_bindgen(setter = sequence)]
    pub fn set_sequence(&mut self, sequence: u64) {
        self.inner().sequence = sequence;
    }

    #[wasm_bindgen(getter = sigOpCount)]
    pub fn get_sig_op_count(&self) -> u8 {
        self.inner().sig_op_count
    }

    #[wasm_bindgen(setter = sigOpCount)]
    pub fn set_sig_op_count(&mut self, sig_op_count: u8) {
        self.inner().sig_op_count = sig_op_count;
    }

    #[wasm_bindgen(getter = utxo)]
    pub fn get_utxo(&self) -> Option<UtxoEntryReference> {
        self.inner().utxo.clone()
    }
}

impl TransactionInput {
    pub fn set_signature_script(&self, signature_script: Vec<u8>) {
        self.inner().signature_script = signature_script;
    }

    pub fn script_public_key(&self) -> Option<ScriptPublicKey> {
        self.utxo().map(|utxo_ref| utxo_ref.utxo.script_public_key.clone())
    }
}

impl AsRef<TransactionInput> for TransactionInput {
    fn as_ref(&self) -> &TransactionInput {
        self
    }
}

impl TryCastFromJs for TransactionInput {
    type Error = Error;
    fn try_cast_from(value: impl AsRef<JsValue>) -> std::result::Result<Cast<Self>, Self::Error> {
        Self::resolve_cast(&value, || {
            if let Some(object) = Object::try_from(value.as_ref()) {
                let previous_outpoint: TransactionOutpoint = object.get_value("previousOutpoint")?.as_ref().try_into()?;
                let signature_script = object.get_vec_u8("signatureScript")?;
                let sequence = object.get_u64("sequence")?;
                let sig_op_count = object.get_u8("sigOpCount")?;
                let utxo = object.try_get_cast::<UtxoEntryReference>("utxo")?.map(Cast::into_owned);
                Ok(TransactionInput::new(previous_outpoint, signature_script, sequence, sig_op_count, utxo).into())
            } else {
                Err("TransactionInput must be an object".into())
            }
        })
    }
}

impl From<cctx::TransactionInput> for TransactionInput {
    fn from(tx_input: cctx::TransactionInput) -> Self {
        TransactionInput::new(
            tx_input.previous_outpoint.into(),
            tx_input.signature_script,
            tx_input.sequence,
            tx_input.sig_op_count,
            None,
        )
    }
}

impl From<&TransactionInput> for cctx::TransactionInput {
    fn from(tx_input: &TransactionInput) -> Self {
        let inner = tx_input.inner();
        cctx::TransactionInput::new(
            inner.previous_outpoint.clone().into(),
            inner.signature_script.clone(),
            inner.sequence,
            inner.sig_op_count,
        )
    }
}
