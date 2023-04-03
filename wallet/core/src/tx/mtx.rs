use crate::imports::*;
use crate::tx::{script_hashes, Transaction, TransactionInput, TransactionOutput};
use crate::utxo::UtxoEntries;
use kaspa_consensus_core::tx;
use kaspa_rpc_core::{RpcTransaction, RpcTransactionInput, RpcTransactionOutput};
use serde_wasm_bindgen::to_value;
use std::str::FromStr;
use workflow_wasm::jsvalue::JsValueTrait;

/// Represents a generic mutable transaction
#[derive(Clone, Debug, Serialize, Deserialize)]
#[wasm_bindgen]
pub struct MutableTransaction {
    tx: Arc<Mutex<Transaction>>,
    /// UTXO entry data
    #[wasm_bindgen(getter_with_clone)]
    pub entries: UtxoEntries,
}

#[wasm_bindgen]
impl MutableTransaction {
    #[wasm_bindgen(constructor)]
    pub fn new(tx: &Transaction, entries: &UtxoEntries) -> Self {
        Self { tx: Arc::new(Mutex::new(tx.clone())), entries: entries.clone() }
    }

    #[wasm_bindgen(js_name=toJSON)]
    pub fn to_json(&self) -> Result<String, JsError> {
        Ok(self.serialize(serde_json::value::Serializer)?.to_string())
    }

    #[wasm_bindgen(js_name=fromJSON)]
    pub fn from_json(json: &str) -> Result<MutableTransaction, JsError> {
        let mtx: Self = serde_json::from_value(serde_json::Value::from_str(json)?)?;
        Ok(mtx)
    }

    #[wasm_bindgen(js_name=getScriptHashes)]
    pub fn script_hashes(&self) -> Result<JsValue, JsError> {
        let hashes = script_hashes(self.clone().try_into()?)?;
        Ok(to_value(&hashes)?)
    }

    #[wasm_bindgen(js_name=setSignatures)]
    pub fn set_signatures(&self, signatures: js_sys::Array) -> Result<JsValue, JsError> {
        let signatures =
            signatures.iter().map(|s| s.try_as_vec_u8()).collect::<Result<Vec<Vec<u8>>, workflow_wasm::error::Error>>()?;

        {
            let mut locked = self.tx.lock();
            let tx = locked.as_mut().unwrap();

            if signatures.len() != tx.inner().inputs.len() {
                return Err(Error::Custom("Signature counts dont match input counts".to_string()).into());
            }
            let len = tx.inner().inputs.len();
            for (i, signature) in signatures.into_iter().enumerate().take(len) {
                tx.inner().inputs[i].inner().sig_op_count = 1;
                tx.inner().inputs[i].inner().signature_script = signature;
            }
        }

        let tx: RpcTransaction = (*self).clone().try_into()?;
        Ok(to_value(&tx)?)
    }

    #[wasm_bindgen(js_name=toRpcTransaction)]
    pub fn rpc_tx_request(&self) -> Result<JsValue, JsError> {
        let tx: RpcTransaction = (*self).clone().try_into()?;
        Ok(to_value(&tx)?)
    }

    #[wasm_bindgen(getter=inputs)]
    pub fn get_inputs(&self) -> Result<js_sys::Array, JsError> {
        let inputs = self.tx.lock()?.get_inputs_as_js_array();
        Ok(inputs)
    }

    #[wasm_bindgen(getter=outputs)]
    pub fn get_outputs(&self) -> Result<js_sys::Array, JsError> {
        let outputs = self.tx.lock()?.get_outputs_as_js_array();
        Ok(outputs)
    }
}

impl MutableTransaction {
    pub fn tx(&self) -> MutexGuard<'_, Transaction> {
        self.tx.lock().unwrap()
    }
    pub fn inputs(&self) -> Result<Vec<TransactionInput>, crate::error::Error> {
        Ok(self.tx.lock()?.inner().inputs.clone())
    }

    pub fn outputs(&self) -> Result<Vec<TransactionOutput>, crate::error::Error> {
        Ok(self.tx.lock()?.inner().outputs.clone())
    }

    pub fn total_input_amount(&self) -> Result<u64, crate::error::Error> {
        let amount = self.entries.items().iter().map(|entry| entry.amount()).sum();
        Ok(amount)
    }

    pub fn total_output_amount(&self) -> Result<u64, crate::error::Error> {
        let amount = self.outputs()?.iter().map(|output| output.get_value()).sum();
        Ok(amount)
    }
}

impl TryFrom<MutableTransaction> for tx::MutableTransaction<tx::Transaction> {
    type Error = Error;
    fn try_from(mtx: MutableTransaction) -> Result<Self, Self::Error> {
        let tx = &mtx.tx.lock()?.clone();
        Ok(Self { tx: tx.try_into()?, entries: mtx.entries.into(), calculated_fee: None, calculated_mass: None })
    }
}

impl TryFrom<(tx::MutableTransaction<tx::Transaction>, UtxoEntries)> for MutableTransaction {
    type Error = Error;
    fn try_from(value: (tx::MutableTransaction<tx::Transaction>, UtxoEntries)) -> Result<Self, Self::Error> {
        Ok(Self { tx: Arc::new(Mutex::new(value.0.tx.try_into()?)), entries: value.1 })
    }
}

impl TryFrom<MutableTransaction> for RpcTransaction {
    type Error = Error;
    fn try_from(mtx: MutableTransaction) -> Result<Self, Self::Error> {
        let tx = tx::MutableTransaction::try_from(mtx)?.tx;

        let rpc_tx = RpcTransaction {
            version: tx.version,
            inputs: RpcTransactionInput::from_transaction_inputs(tx.inputs),
            outputs: RpcTransactionOutput::from_transaction_outputs(tx.outputs),
            lock_time: tx.lock_time,
            subnetwork_id: tx.subnetwork_id,
            gas: tx.gas,
            payload: tx.payload,
            verbose_data: None,
        };

        Ok(rpc_tx)
    }
}
