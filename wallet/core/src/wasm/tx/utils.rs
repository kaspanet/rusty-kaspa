use crate::imports::*;
use crate::result::Result;
use crate::tx::{IPaymentOutputArray, PaymentOutputs};
use crate::wasm::tx::generator::*;
use kaspa_consensus_client::*;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_wallet_macros::declare_typescript_wasm_interface as declare;
use kaspa_wasm_core::types::BinaryT;
use workflow_core::runtime::is_web;

/// Create a basic transaction without any mass limit checks.
/// @category Wallet SDK
#[wasm_bindgen(js_name=createTransaction)]
pub fn create_transaction_js(
    utxo_entry_source: IUtxoEntryArray,
    outputs: IPaymentOutputArray,
    priority_fee: BigInt,
    payload: Option<BinaryT>,
    sig_op_count: Option<u8>,
) -> crate::result::Result<Transaction> {
    let utxo_entries = if let Some(utxo_entries) = utxo_entry_source.dyn_ref::<js_sys::Array>() {
        utxo_entries.to_vec().iter().map(UtxoEntryReference::try_owned_from).collect::<Result<Vec<_>, _>>()?
    } else {
        return Err(Error::custom("utxo_entries must be an array"));
    };
    let priority_fee: u64 = priority_fee.try_into().map_err(|err| Error::custom(format!("invalid fee value: {err}")))?;
    let payload = payload.and_then(|payload| payload.try_as_vec_u8().ok()).unwrap_or_default();
    let outputs = PaymentOutputs::try_owned_from(outputs)?;
    let sig_op_count = sig_op_count.unwrap_or(1);

    // ---

    let mut total_input_amount = 0;
    let mut entries = vec![];

    let inputs = utxo_entries
        .into_iter()
        .enumerate()
        .map(|(sequence, reference)| {
            let UtxoEntryReference { utxo } = &reference;
            total_input_amount += utxo.amount();
            entries.push(reference.clone());
            TransactionInput::new(utxo.outpoint.clone(), None, sequence as u64, sig_op_count, Some(reference))
        })
        .collect::<Vec<TransactionInput>>();

    if priority_fee > total_input_amount {
        return Err(format!("priority fee({priority_fee}) > amount({total_input_amount})").into());
    }

    let outputs: Vec<TransactionOutput> = outputs.into();
    let transaction = Transaction::new(None, 0, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, payload, 0)?;

    Ok(transaction)
}

declare! {
    ICreateTransactions,
    r#"
    /**
     * Interface defining response from the {@link createTransactions} function.
     * 
     * @category Wallet SDK
     */
    export interface ICreateTransactions {
        /**
         * Array of pending unsigned transactions.
         */
        transactions : PendingTransaction[];
        /**
         * Summary of the transaction generation process.
         */
        summary : GeneratorSummary;
    }
    "#,
}

#[wasm_bindgen(typescript_custom_section)]
const TS_CREATE_TRANSACTIONS: &'static str = r#"
"#;

/// Helper function that creates a set of transactions using the transaction {@link Generator}.
/// @see {@link IGeneratorSettingsObject}, {@link Generator}, {@link estimateTransactions}
/// @category Wallet SDK
#[wasm_bindgen(js_name=createTransactions)]
pub async fn create_transactions_js(settings: IGeneratorSettingsObject) -> Result<ICreateTransactions> {
    let generator = Generator::ctor(settings)?;
    if is_web() {
        // yield after each generated transaction if operating in the browser
        let mut stream = generator.stream();
        let mut transactions = vec![];
        while let Some(transaction) = stream.try_next().await? {
            transactions.push(PendingTransaction::from(transaction));
            yield_executor().await;
        }
        let transactions = Array::from_iter(transactions.into_iter().map(JsValue::from)); //.collect::<Array>();
        let summary = JsValue::from(generator.summary());
        let object = ICreateTransactions::default();
        object.set("transactions", &transactions)?;
        object.set("summary", &summary)?;
        Ok(object)
    } else {
        let transactions = generator.iter().map(|r| r.map(PendingTransaction::from)).collect::<Result<Vec<_>>>()?;
        let transactions = Array::from_iter(transactions.into_iter().map(JsValue::from)); //.collect::<Array>();
        let summary = JsValue::from(generator.summary());
        let object = ICreateTransactions::default();
        object.set("transactions", &transactions)?;
        object.set("summary", &summary)?;
        Ok(object)
    }
}

/// Helper function that creates an estimate using the transaction {@link Generator}
/// by producing only the {@link GeneratorSummary} containing the estimate.
/// @see {@link IGeneratorSettingsObject}, {@link Generator}, {@link createTransactions}
/// @category Wallet SDK
#[wasm_bindgen(js_name=estimateTransactions)]
pub async fn estimate_transactions_js(settings: IGeneratorSettingsObject) -> Result<GeneratorSummary> {
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
