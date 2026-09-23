use crate::imports::*;
use crate::result::Result;
use crate::tx::{IPaymentOutputArray, PaymentOutputs};
use crate::wasm::tx::generator::*;
use kaspa_consensus_client::*;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::ComputeCommit as CoreComputeCommit;
use kaspa_wallet_macros::declare_typescript_wasm_interface as declare;
use kaspa_wasm_core::types::BinaryT;
use workflow_core::runtime::is_web;

/// Create a transaction manually without any mass limit checks.
/// Optionally, you can pass compute commit and transaction version (useful for covenants)
///
/// @category Wallet SDK
#[wasm_bindgen(js_name=createTransaction)]
pub fn create_transaction_js(
    utxo_entry_source: IUtxoEntryArray,
    outputs: IPaymentOutputArray,
    priority_fee: BigInt,
    payload: Option<BinaryT>,
    compute_commit: Option<ComputeCommitT>,
    version: Option<u16>,
) -> crate::result::Result<Transaction> {
    let utxo_entries = if let Some(utxo_entries) = utxo_entry_source.dyn_ref::<js_sys::Array>() {
        utxo_entries.to_vec().iter().map(UtxoEntryReference::try_owned_from).collect::<Result<Vec<_>, _>>()?
    } else {
        return Err(Error::custom("utxo_entries must be an array"));
    };
    let priority_fee: u64 = priority_fee.try_into().map_err(|err| Error::custom(format!("invalid fee value: {err}")))?;
    let payload = payload.and_then(|payload| payload.try_as_vec_u8().ok()).unwrap_or_default();
    let outputs = PaymentOutputs::try_owned_from(outputs)?;
    let version = version.unwrap_or_default();

    // one of: user provided, ensure required when txv1 or defaults to sigopcount
    let compute_commit = if let Some(compute_commit) = compute_commit {
        ComputeCommit::try_owned_from(&compute_commit)?.inner()
    } else if CoreComputeCommit::version_expects_compute_budget_field(version) {
        return Err(Error::custom(format!("transaction version {version} requires computeBudget commit")));
    } else {
        CoreComputeCommit::SigopCount(1.into())
    };

    if CoreComputeCommit::version_expects_compute_budget_field(version) && compute_commit.compute_budget().is_none() {
        return Err(Error::custom(format!("transaction version {version} requires computeBudget commit")));
    }

    if CoreComputeCommit::version_expects_sig_op_count_field(version) && compute_commit.sig_op_count().is_none() {
        return Err(Error::custom(format!("transaction version {version} requires sigOpCount commit")));
    }

    // rejects covenant output is txv0
    if CoreComputeCommit::version_expects_sig_op_count_field(version) && outputs.iter().any(|output| output.covenant.is_some()) {
        return Err(Error::custom(format!("transaction version {version} does not support covenant outputs")));
    }

    let (sig_op_count, compute_budget) = match compute_commit {
        CoreComputeCommit::SigopCount(sig_op_count) => (u8::from(sig_op_count), 0),
        CoreComputeCommit::ComputeBudget(compute_budget) => (0, u16::from(compute_budget)),
    };

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
            TransactionInput::new(utxo.outpoint.clone(), None, sequence as u64, sig_op_count, compute_budget, Some(reference))
        })
        .collect::<Vec<TransactionInput>>();

    if priority_fee > total_input_amount {
        return Err(format!("priority fee({priority_fee}) > amount({total_input_amount})").into());
    }

    let outputs: Vec<TransactionOutput> = outputs.into();
    let transaction = Transaction::new(None, version, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, payload, 0)?;

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

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;
    use kaspa_addresses::Address;
    use kaspa_hashes::Hash;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn test_create_transaction_js_supports_v1_compute_commit_and_covenant_output() {
        let address = Address::try_from("kaspatest:qqz22l98sf8jun72rwh5rqe2tm8lhwtdxdmynrz4ypwak427qed5juktjt7ju").unwrap();
        let utxo = UtxoEntryReference::simulated_with_address(2_000, &address);
        let utxos = Array::new();
        utxos.push(&JsValue::from(utxo));

        let covenant_id = Hash::from_bytes([0xab; 32]);
        let covenant = Object::new();
        covenant.set("authorizingInput", &JsValue::from(0)).unwrap();
        covenant.set("covenantId", &JsValue::from_str(&covenant_id.to_string())).unwrap();

        let output = Object::new();
        output.set("address", &JsValue::from_str(&address.to_string())).unwrap();
        output.set("amount", &JsValue::from(1_000)).unwrap();
        output.set("covenant", &covenant.into()).unwrap();
        let outputs = Array::new();
        outputs.push(&output.into());

        let compute_commit = Object::new();
        compute_commit.set("type", &JsValue::from_str("computeBudget")).unwrap();
        compute_commit.set("value", &JsValue::from(7)).unwrap();

        let tx = create_transaction_js(
            utxos.unchecked_into(),
            outputs.unchecked_into(),
            BigInt::from(0),
            None,
            Some(compute_commit.unchecked_into()),
            Some(1),
        )
        .expect("createTransaction should support v1 covenant outputs");

        let inner = tx.inner();
        assert_eq!(inner.version, 1);
        assert_eq!(inner.inputs.len(), 1);
        assert_eq!(inner.inputs[0].inner().compute_budget, 7);
        assert_eq!(inner.inputs[0].inner().sig_op_count, 0);
        assert_eq!(inner.outputs.len(), 1);
        assert_eq!(inner.outputs[0].get_covenant().expect("covenant").get_covenant_id(), covenant_id);
    }
}
