use super::error::Error;
use consensus_core::subnets::SubnetworkId;
use consensus_core::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
// use consensus_core::wasm::{ref_from_abi, UtxoEntry };
use js_sys::Object;
use wasm_bindgen::prelude::*;
//use workflow_log::log_trace;
// use std::sync::Arc;
use workflow_wasm::jsvalue::*;
use workflow_wasm::object::*;

impl TryFrom<JsValue> for TransactionOutpoint {
    type Error = Error;
    fn try_from(value: JsValue) -> Result<Self, Self::Error> {
        if value.is_object() {
            let object = Object::from(value);
            let transaction_id = object.get("transactionId")?.try_into()?;
            let index = object.get_u32("index")?;
            Ok(TransactionOutpoint::new(transaction_id, index))
        } else {
            Err("outpoint is not an object".into())
        }
    }
}

impl TryFrom<JsValue> for TransactionInput {
    type Error = Error;
    fn try_from(js_value: JsValue) -> Result<Self, Self::Error> {
        if js_value.is_object() {
            let object = Object::from(js_value);
            let previous_outpoint: TransactionOutpoint = object.get("previousOutpoint")?.try_into()?;
            let signature_script = object.get_vec_u8("signatureScript")?;
            let sequence = object.get_u64("sequence")?;
            let sig_op_count = object.get_u8("sigOpCount")?;

            Ok(TransactionInput::new(previous_outpoint, signature_script, sequence, sig_op_count))
        } else {
            Err("TransactionInput must be an object".into())
        }
    }
}

impl TryFrom<JsValue> for ScriptPublicKey {
    type Error = Error;
    fn try_from(js_value: JsValue) -> Result<Self, Self::Error> {
        if js_value.is_object() {
            let object = Object::from(js_value);
            let version_value = object.get("version")?;
            let version = if version_value.is_string() {
                let hex_string = version_value.as_string().unwrap();
                if hex_string.len() != 4 {
                    return Err("`ScriptPublicKey::version` must be a string of length 4 (2 byte hex repr)".into());
                }
                u16::from_str_radix(&hex_string, 16).map_err(|_| Error::Custom("error parsing version hex value".into()))?
            } else if let Ok(version) = version_value.try_as_u16() {
                version
            } else {
                return Err(Error::Custom(format!(
                    "`ScriptPublicKey::version` must be a hex string or a 16-bit integer: `{version_value:?}`"
                )));
            };

            let script = object.get_vec_u8("script")?;

            Ok(ScriptPublicKey::new(version, script.into()))
        } else {
            Err("ScriptPublicKey must be an object".into())
        }
    }
}

impl TryFrom<JsValue> for TransactionOutput {
    type Error = Error;
    fn try_from(js_value: JsValue) -> Result<Self, Self::Error> {
        if js_value.is_object() {
            let object = Object::from(js_value);
            let value = object.get_u64("value")?;
            let script_public_key: ScriptPublicKey =
                object.get("scriptPublicKey").map_err(|_| Error::Custom("missing `script` property".into()))?.try_into()?;
            Ok(TransactionOutput::new(value, script_public_key))
        } else {
            Err("TransactionInput must be an object".into())
        }
    }
}

impl TryFrom<JsValue> for Transaction {
    type Error = Error;
    fn try_from(js_value: JsValue) -> Result<Self, Self::Error> {
        if js_value.is_object() {
            let object = Object::from(js_value);
            let version = object.get_u16("version")?;
            let lock_time = object.get_u64("lockTime")?;
            let gas = object.get_u64("gas")?;
            let payload = object.get_vec_u8("payload")?;
            let subnetwork_id = object.get_vec_u8("subnetworkId")?;
            if subnetwork_id.len() != crate::subnets::SUBNETWORK_ID_SIZE {
                return Err(Error::Custom("subnetworkId must be 20 bytes long".into()));
            }
            let subnetwork_id: SubnetworkId =
                subnetwork_id.as_slice().try_into().map_err(|err| Error::Custom(format!("`subnetworkId` property error: `{err}`")))?;
            let inputs =
                object.get_vec("inputs")?.into_iter().map(|jsv| jsv.try_into()).collect::<Result<Vec<TransactionInput>, Error>>()?;
            let outputs =
                object.get_vec("outputs")?.into_iter().map(|jsv| jsv.try_into()).collect::<Result<Vec<TransactionOutput>, Error>>()?;
            Ok(Transaction::new(version, inputs, outputs, lock_time, subnetwork_id, gas, payload))
        } else {
            Err("Transaction must be an object".into())
        }
    }
}
