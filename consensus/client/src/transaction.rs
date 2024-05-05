#![allow(non_snake_case)]

use crate::imports::*;
use crate::input::TransactionInput;
use crate::outpoint::TransactionOutpoint;
use crate::output::TransactionOutput;
use crate::result::Result;
use crate::serializable::{numeric, string};
use crate::utxo::{UtxoEntryId, UtxoEntryReference};
use ahash::AHashMap;
use kaspa_consensus_core::network::NetworkType;
use kaspa_consensus_core::network::NetworkTypeT;
use kaspa_consensus_core::subnets::{self, SubnetworkId};
use kaspa_consensus_core::tx::UtxoEntry;
use kaspa_txscript::extract_script_pub_key_address;
use kaspa_utils::hex::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_TRANSACTION: &'static str = r#"
/**
 * Interface defining the structure of a transaction.
 * 
 * @category Consensus
 */
export interface ITransaction {
    version: number;
    inputs: ITransactionInput[];
    outputs: ITransactionOutput[];
    lockTime: bigint;
    subnetworkId: HexString;
    gas: bigint;
    payload: HexString;

    /** Optional verbose data provided by RPC */
    verboseData?: ITransactionVerboseData;
}

/**
 * Optional transaction verbose data.
 * 
 * @category Node RPC
 */
export interface ITransactionVerboseData {
    transactionId : HexString;
    hash : HexString;
    mass : bigint;
    blockHash : HexString;
    blockTime : bigint;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "ITransaction")]
    pub type ITransaction;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInner {
    pub version: u16,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
    pub lock_time: u64,
    pub subnetwork_id: SubnetworkId,
    pub gas: u64,
    pub payload: Vec<u8>,

    // A field that is used to cache the transaction ID.
    // Always use the corresponding self.id() instead of accessing this field directly
    pub id: TransactionId,
}

/// Represents a Kaspa transaction.
/// This is an artificial construct that includes additional
/// transaction-related data such as additional data from UTXOs
/// used by transaction inputs.
/// @category Consensus
#[derive(Clone, Debug, Serialize, Deserialize, CastFromJs)]
#[wasm_bindgen(inspectable)]
pub struct Transaction {
    inner: Arc<Mutex<TransactionInner>>,
}

impl Transaction {
    pub fn new(
        id: Option<TransactionId>,
        version: u16,
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        lock_time: u64,
        subnetwork_id: SubnetworkId,
        gas: u64,
        payload: Vec<u8>,
    ) -> Result<Self> {
        let finalize = id.is_none();
        let tx = Self {
            inner: Arc::new(Mutex::new(TransactionInner {
                id: id.unwrap_or_default(),
                version,
                inputs,
                outputs,
                lock_time,
                subnetwork_id,
                gas,
                payload,
            })),
        };
        if finalize {
            tx.finalize()?;
        }
        Ok(tx)
    }

    pub fn new_with_inner(inner: TransactionInner) -> Self {
        Self { inner: Arc::new(Mutex::new(inner)) }
    }

    pub fn inner(&self) -> MutexGuard<'_, TransactionInner> {
        self.inner.lock().unwrap()
    }

    pub fn id(&self) -> TransactionId {
        self.inner().id
    }
}

#[wasm_bindgen]
impl Transaction {
    /// Determines whether or not a transaction is a coinbase transaction. A coinbase
    /// transaction is a special transaction created by miners that distributes fees and block subsidy
    /// to the previous blocks' miners, and specifies the script_pub_key that will be used to pay the current
    /// miner in future blocks.
    pub fn is_coinbase(&self) -> bool {
        self.inner().subnetwork_id == subnets::SUBNETWORK_ID_COINBASE
    }

    /// Recompute and finalize the tx id based on updated tx fields
    pub fn finalize(&self) -> Result<TransactionId> {
        let tx: cctx::Transaction = self.into();
        self.inner().id = tx.id();
        Ok(self.inner().id)
    }

    /// Returns the transaction ID
    #[wasm_bindgen(getter, js_name = id)]
    pub fn id_string(&self) -> String {
        self.inner().id.to_string()
    }

    #[wasm_bindgen(constructor)]
    pub fn constructor(js_value: &ITransaction) -> std::result::Result<Transaction, JsError> {
        Ok(js_value.try_into_owned()?)
    }

    #[wasm_bindgen(getter = inputs)]
    pub fn get_inputs_as_js_array(&self) -> Array {
        let inputs = self.inner.lock().unwrap().inputs.clone().into_iter().map(JsValue::from);
        Array::from_iter(inputs)
    }

    /// Returns a list of unique addresses used by transaction inputs.
    /// This method can be used to determine addresses used by transaction inputs
    /// in order to select private keys needed for transaction signing.
    pub fn addresses(&self, network_type: &NetworkTypeT) -> Result<kaspa_addresses::AddressArrayT> {
        let mut list = std::collections::HashSet::new();
        for input in &self.inner.lock().unwrap().inputs {
            if let Some(utxo) = input.get_utxo() {
                if let Some(address) = &utxo.utxo.address {
                    list.insert(address.clone());
                } else if let Ok(address) =
                    extract_script_pub_key_address(&utxo.utxo.script_public_key, NetworkType::try_from(network_type)?.into())
                {
                    list.insert(address);
                }
            }
        }
        Ok(Array::from_iter(list.into_iter().map(JsValue::from)).unchecked_into())
    }

    #[wasm_bindgen(setter = inputs)]
    pub fn set_inputs_from_js_array(&mut self, js_value: &JsValue) {
        let inputs = Array::from(js_value)
            .iter()
            .map(|js_value| {
                TransactionInput::try_owned_from(&js_value).unwrap_or_else(|err| panic!("invalid transaction input: {err}"))
            })
            .collect::<Vec<_>>();
        self.inner().inputs = inputs;
    }

    #[wasm_bindgen(getter = outputs)]
    pub fn get_outputs_as_js_array(&self) -> Array {
        let outputs = self.inner.lock().unwrap().outputs.clone().into_iter().map(JsValue::from);
        Array::from_iter(outputs)
    }

    #[wasm_bindgen(setter = outputs)]
    pub fn set_outputs_from_js_array(&mut self, js_value: &JsValue) {
        let outputs = Array::from(js_value)
            .iter()
            .map(|js_value| TransactionOutput::try_from(&js_value).unwrap_or_else(|err| panic!("invalid transaction output: {err}")))
            .collect::<Vec<_>>();
        self.inner().outputs = outputs;
    }

    #[wasm_bindgen(getter, js_name = version)]
    pub fn get_version(&self) -> u16 {
        self.inner().version
    }

    #[wasm_bindgen(setter, js_name = version)]
    pub fn set_version(&self, v: u16) {
        self.inner().version = v;
    }

    #[wasm_bindgen(getter, js_name = lock_time)]
    pub fn get_lock_time(&self) -> u64 {
        self.inner().lock_time
    }

    #[wasm_bindgen(setter, js_name = lock_time)]
    pub fn set_lock_time(&self, v: u64) {
        self.inner().lock_time = v;
    }

    #[wasm_bindgen(getter, js_name = gas)]
    pub fn get_gas(&self) -> u64 {
        self.inner().gas
    }

    #[wasm_bindgen(setter, js_name = gas)]
    pub fn set_gas(&self, v: u64) {
        self.inner().gas = v;
    }

    #[wasm_bindgen(getter = subnetworkId)]
    pub fn get_subnetwork_id_as_hex(&self) -> String {
        self.inner().subnetwork_id.to_hex()
    }

    #[wasm_bindgen(setter = subnetworkId)]
    pub fn set_subnetwork_id_from_js_value(&mut self, js_value: JsValue) {
        let subnetwork_id = js_value.try_as_vec_u8().unwrap_or_else(|err| panic!("subnetwork id error: {err}"));
        self.inner().subnetwork_id = subnetwork_id.as_slice().try_into().unwrap_or_else(|err| panic!("subnetwork id error: {err}"));
    }

    #[wasm_bindgen(getter = payload)]
    pub fn get_payload_as_hex_string(&self) -> String {
        self.inner().payload.to_hex()
    }

    #[wasm_bindgen(setter = payload)]
    pub fn set_payload_from_js_value(&mut self, js_value: JsValue) {
        self.inner.lock().unwrap().payload = js_value.try_as_vec_u8().unwrap_or_else(|err| panic!("payload value error: {err}"));
    }
}

impl TryCastFromJs for Transaction {
    type Error = Error;
    fn try_cast_from(value: impl AsRef<JsValue>) -> std::result::Result<Cast<Self>, Self::Error> {
        Self::resolve_cast(&value, || {
            if let Some(object) = Object::try_from(value.as_ref()) {
                if let Some(tx) = object.try_get_value("tx")? {
                    Transaction::try_cast_from(&tx)
                } else {
                    let id = object.try_get_cast::<TransactionId>("id")?.map(|id| id.into_owned());
                    let version = object.get_u16("version")?;
                    let lock_time = object.get_u64("lockTime")?;
                    let gas = object.get_u64("gas")?;
                    let payload = object.get_vec_u8("payload")?;
                    let subnetwork_id = object.get_vec_u8("subnetworkId")?;
                    if subnetwork_id.len() != subnets::SUBNETWORK_ID_SIZE {
                        return Err(Error::Custom("subnetworkId must be 20 bytes long".into()));
                    }
                    let subnetwork_id: SubnetworkId = subnetwork_id
                        .as_slice()
                        .try_into()
                        .map_err(|err| Error::Custom(format!("`subnetworkId` property error: `{err}`")))?;
                    let inputs = object
                        .get_vec("inputs")?
                        .iter()
                        .map(TryCastFromJs::try_owned_from)
                        .collect::<std::result::Result<Vec<TransactionInput>, Error>>()?;
                    let outputs: Vec<TransactionOutput> = object
                        .get_vec("outputs")?
                        .iter()
                        .map(|jsv| jsv.try_into())
                        .collect::<std::result::Result<Vec<TransactionOutput>, Error>>()?;
                    Transaction::new(id, version, inputs, outputs, lock_time, subnetwork_id, gas, payload).map(Into::into)
                }
            } else {
                Err("Transaction must be an object".into())
            }
        })
        // Transaction::try_from(value)
    }
}

impl From<cctx::Transaction> for Transaction {
    fn from(tx: cctx::Transaction) -> Self {
        let id = tx.id();
        let inputs: Vec<TransactionInput> = tx.inputs.into_iter().map(|input| input.into()).collect::<Vec<TransactionInput>>();
        let outputs: Vec<TransactionOutput> = tx.outputs.into_iter().map(|output| output.into()).collect::<Vec<TransactionOutput>>();
        Self::new_with_inner(TransactionInner {
            version: tx.version,
            inputs,
            outputs,
            lock_time: tx.lock_time,
            gas: tx.gas,
            payload: tx.payload,
            subnetwork_id: tx.subnetwork_id,
            id,
        })
    }
}

impl From<&Transaction> for cctx::Transaction {
    fn from(tx: &Transaction) -> Self {
        let inner = tx.inner();
        let inputs: Vec<cctx::TransactionInput> =
            inner.inputs.clone().into_iter().map(|input| input.as_ref().into()).collect::<Vec<cctx::TransactionInput>>();
        let outputs: Vec<cctx::TransactionOutput> =
            inner.outputs.clone().into_iter().map(|output| output.as_ref().into()).collect::<Vec<cctx::TransactionOutput>>();
        cctx::Transaction::new(
            inner.version,
            inputs,
            outputs,
            inner.lock_time,
            inner.subnetwork_id.clone(),
            inner.gas,
            inner.payload.clone(),
        )
    }
}

impl Transaction {
    pub fn from_cctx_transaction(tx: &cctx::Transaction, utxos: &AHashMap<UtxoEntryId, UtxoEntryReference>) -> Self {
        let inputs: Vec<TransactionInput> = tx
            .inputs
            .iter()
            .map(|input| {
                let previous_outpoint: TransactionOutpoint = input.previous_outpoint.into();
                let utxo = utxos.get(previous_outpoint.id()).cloned();
                TransactionInput::new(previous_outpoint, input.signature_script.clone(), input.sequence, input.sig_op_count, utxo)
            })
            .collect::<Vec<TransactionInput>>();
        let outputs: Vec<TransactionOutput> = tx.outputs.iter().map(|output| output.into()).collect::<Vec<TransactionOutput>>();

        Self::new_with_inner(TransactionInner {
            id: tx.id(),
            version: tx.version,
            inputs,
            outputs,
            lock_time: tx.lock_time,
            gas: tx.gas,
            payload: tx.payload.clone(),
            subnetwork_id: tx.subnetwork_id.clone(),
        })
    }

    pub fn tx_and_utxos(&self) -> (cctx::Transaction, Vec<UtxoEntry>) {
        let mut utxos = vec![];
        let inner = self.inner();
        let inputs: Vec<cctx::TransactionInput> = inner
            .inputs
            .clone()
            .into_iter()
            .map(|input| {
                utxos.push((&input.get_utxo().unwrap().entry()).into());
                input.as_ref().into()
            })
            .collect::<Vec<cctx::TransactionInput>>();
        let outputs: Vec<cctx::TransactionOutput> =
            inner.outputs.clone().into_iter().map(|output| output.as_ref().into()).collect::<Vec<cctx::TransactionOutput>>();
        let tx = cctx::Transaction::new(
            inner.version,
            inputs,
            outputs,
            inner.lock_time,
            inner.subnetwork_id.clone(),
            inner.gas,
            inner.payload.clone(),
        );

        (tx, utxos)
    }

    pub fn set_signature_script(&self, input_index: usize, signature_script: Vec<u8>) -> Result<()> {
        if self.inner().inputs.len() <= input_index {
            return Err(Error::Custom("Input index is invalid".to_string()));
        }
        self.inner().inputs[input_index].set_signature_script(signature_script);
        Ok(())
    }
}

#[wasm_bindgen]
impl Transaction {
    /// Serializes the transaction to a pure JavaScript Object.
    /// The schema of the JavaScript object is defined by {@link ISerializableTransaction}.
    /// @see {@link ISerializableTransaction}
    #[wasm_bindgen(js_name = "serializeToObject")]
    pub fn serialize_to_object(&self) -> Result<ITransaction> {
        Ok(numeric::SerializableTransaction::from_client_transaction(self)?.serialize_to_object()?.into())
    }

    /// Serializes the transaction to a JSON string.
    /// The schema of the JSON is defined by {@link ISerializableTransaction}.
    #[wasm_bindgen(js_name = "serializeToJSON")]
    pub fn serialize_to_json(&self) -> Result<String> {
        numeric::SerializableTransaction::from_client_transaction(self)?.serialize_to_json()
    }

    /// Serializes the transaction to a "Safe" JSON schema where it converts all `bigint` values to `string` to avoid potential client-side precision loss.
    #[wasm_bindgen(js_name = "serializeToSafeJSON")]
    pub fn serialize_to_json_safe(&self) -> Result<String> {
        string::SerializableTransaction::from_client_transaction(self)?.serialize_to_json()
    }

    /// Deserialize the {@link Transaction} Object from a pure JavaScript Object.
    #[wasm_bindgen(js_name = "deserializeFromObject")]
    pub fn deserialize_from_object(js_value: &JsValue) -> Result<Transaction> {
        numeric::SerializableTransaction::deserialize_from_object(js_value.clone())?.try_into()
    }

    /// Deserialize the {@link Transaction} Object from a JSON string.
    #[wasm_bindgen(js_name = "deserializeFromJSON")]
    pub fn deserialize_from_json(json: &str) -> Result<Transaction> {
        numeric::SerializableTransaction::deserialize_from_json(json)?.try_into()
    }

    /// Deserialize the {@link Transaction} Object from a "Safe" JSON schema where all `bigint` values are represented as `string`.
    #[wasm_bindgen(js_name = "deserializeFromSafeJSON")]
    pub fn deserialize_from_safe_json(json: &str) -> Result<Transaction> {
        string::SerializableTransaction::deserialize_from_json(json)?.try_into()
    }
}
