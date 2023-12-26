use crate::imports::*;
use crate::result::Result;
use crate::tx::PaymentOutputs;
use crate::wasm::tx::consensus::get_consensus_params_by_address;
use crate::wasm::tx::generator::*;
use crate::wasm::tx::mass::MassCalculator;
use kaspa_addresses::Address;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_wasm::*;
use workflow_core::runtime::is_web;

/// Create a basic transaction without any mass limit checks.
#[wasm_bindgen(js_name=createTransaction)]
pub fn create_transaction_js(
    utxo_entry_source: JsValue,
    outputs: JsValue,
    change_address: JsValue,
    priority_fee: BigInt,
    payload: JsValue,
    sig_op_count: JsValue,
    minimum_signatures: JsValue,
) -> crate::result::Result<SignableTransaction> {
    let change_address = Address::try_from(change_address)?;
    let params = get_consensus_params_by_address(&change_address);
    let mc = MassCalculator::new(params);

    let utxo_entries = if let Some(utxo_entries) = utxo_entry_source.dyn_ref::<js_sys::Array>() {
        utxo_entries.to_vec().iter().map(UtxoEntryReference::try_from).collect::<Result<Vec<_>, _>>()?
    } else {
        return Err(Error::custom("utxo_entries must be an array"));
    };
    let priority_fee: u64 = priority_fee.try_into().map_err(|err| Error::custom(format!("invalid fee value: {err}")))?;
    let payload = payload.try_as_vec_u8().ok().unwrap_or_default();
    let outputs: PaymentOutputs = outputs.try_into()?;
    let sig_op_count =
        if !sig_op_count.is_undefined() { sig_op_count.as_f64().expect("sigOpCount should be a number") as u8 } else { 1 };

    let minimum_signatures = if !minimum_signatures.is_undefined() {
        minimum_signatures.as_f64().expect("minimumSignatures should be a number") as u16
    } else {
        1
    };

    // ---

    let mut total_input_amount = 0;
    let mut entries = vec![];

    let inputs = utxo_entries
        .iter()
        .enumerate()
        .map(|(sequence, reference)| {
            let UtxoEntryReference { utxo } = reference;
            total_input_amount += utxo.amount();
            entries.push(reference.clone());
            TransactionInput::new(utxo.outpoint.clone(), vec![], sequence as u64, sig_op_count)
        })
        .collect::<Vec<TransactionInput>>();

    if priority_fee > total_input_amount {
        return Err(format!("priority fee({priority_fee}) > amount({total_input_amount})").into());
    }

    // TODO - Calculate mass and fees

    let outputs: Vec<TransactionOutput> = outputs.into();
    let transaction = Transaction::new(0, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, payload)?;
    let _fee = mc.calc_minimum_transaction_relay_fee(&transaction, minimum_signatures);
    let mtx = SignableTransaction::new(transaction, entries.into());

    Ok(mtx)
}

/// Creates a set of transactions using transaction [`Generator`].
#[wasm_bindgen(js_name=createTransactions)]
pub async fn create_transactions_js(settings: GeneratorSettingsObject) -> Result<Object> {
    let generator = Generator::ctor(settings)?;
    if is_web() {
        // yield after each generated transaction if operating in the browser
        let mut stream = generator.stream();
        let mut transactions = vec![];
        while let Some(transaction) = stream.try_next().await? {
            transactions.push(PendingTransaction::from(transaction));
            yield_executor().await;
        }
        let transactions = transactions.into_iter().map(JsValue::from).collect::<Array>();
        let summary = JsValue::from(generator.summary());
        let object = Object::new();
        object.set("transactions", &transactions)?;
        object.set("summary", &summary)?;
        Ok(object)
    } else {
        // use iterator to aggregate all transactions
        let transactions = generator.iter().map(|r| r.map(PendingTransaction::from)).collect::<Result<Vec<_>>>()?;
        let transactions = transactions.into_iter().map(JsValue::from).collect::<Array>();
        let summary = JsValue::from(generator.summary());
        let object = Object::new();
        object.set("transactions", &transactions)?;
        object.set("summary", &summary)?;
        Ok(object)
    }
}

/// Creates a set of transactions using transaction [`Generator`].
#[wasm_bindgen(js_name=estimateTransactions)]
pub async fn estimate_js(settings: GeneratorSettingsObject) -> Result<GeneratorSummary> {
    let generator = Generator::ctor(settings)?;
    if is_web() {
        // yield after each generated transaction if operating in the browser
        let mut stream = generator.stream();
        while stream.try_next().await?.is_some() {
            yield_executor().await;
        }
        Ok(generator.summary())
    } else {
        // use iterator to aggregate all transactions
        generator.iter().collect::<Result<Vec<_>>>()?;
        Ok(generator.summary())
    }
}
