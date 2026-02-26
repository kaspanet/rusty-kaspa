use crate::error::Error;
use crate::result::Result;
use js_sys::Array;
use kaspa_consensus_client::{TransactionOutpoint, TransactionOutpointT, TransactionOutput};
use kaspa_consensus_core::{hashing::covenant_id, tx as cctx};
use kaspa_hashes::Hash;
use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::*;

/// Computes the covenant ID from the genesis outpoint and its authorized outputs.
///
/// `genesis_outpoint` may be a [`TransactionOutpoint`] instance or a
/// compatible plain object: `{ transactionId: HexString, index: number }`.
///
/// `auth_outputs` is a JS array of objects, each with:
/// - `index: number` — position of this output in the transaction's output array
/// - `output: TransactionOutput | ITransactionOutput` — the authorized output
///
/// @category Consensus
#[wasm_bindgen(js_name = covenantId)]
pub fn js_covenant_id(genesis_outpoint: &TransactionOutpointT, auth_outputs: JsValue) -> Result<Hash> {
    let outpoint_client = TransactionOutpoint::try_from(genesis_outpoint.as_ref())?;
    let outpoint = cctx::TransactionOutpoint::from(&outpoint_client);
    let outputs: Vec<(u32, cctx::TransactionOutput)> = Array::from(&auth_outputs)
        .iter()
        .map(|item| {
            let obj = js_sys::Object::try_from(&item).ok_or_else(|| Error::custom("each auth_output must be an object"))?;
            let index = obj.get_u32("index")?;
            let output = TransactionOutput::try_owned_from(obj.get_value("output")?)?;
            Ok((index, cctx::TransactionOutput::from(&output)))
        })
        .collect::<Result<_>>()?;

    Ok(covenant_id::covenant_id(outpoint, outputs.iter().map(|(i, o)| (*i, o))))
}
