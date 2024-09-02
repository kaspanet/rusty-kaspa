use crate::imports::*;
use crate::result::Result;
use crate::tx::generator as native;
use crate::wasm::PrivateKeyArrayT;
use kaspa_consensus_client::{numeric, string};
use kaspa_consensus_client::{Transaction, TransactionT};
use kaspa_consensus_core::hashing::wasm::SighashType;
use kaspa_wallet_keys::privatekey::PrivateKey;
use kaspa_wasm_core::types::{BinaryT, HexString};
use kaspa_wrpc_wasm::RpcClient;

/// @category Wallet SDK
#[wasm_bindgen(inspectable)]
pub struct PendingTransaction {
    inner: native::PendingTransaction,
}

#[wasm_bindgen]
impl PendingTransaction {
    /// Transaction Id
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> String {
        self.inner.id().to_string()
    }

    /// Total amount transferred to the destination (aggregate output - change).
    #[wasm_bindgen(getter, js_name = paymentAmount)]
    pub fn payment_value(&self) -> JsValue {
        if let Some(payment_value) = self.inner.payment_value() {
            BigInt::from(payment_value).into()
        } else {
            JsValue::UNDEFINED
        }
    }

    /// Change amount (if any).
    #[wasm_bindgen(getter, js_name = changeAmount)]
    pub fn change_value(&self) -> BigInt {
        BigInt::from(self.inner.change_value())
    }

    /// Total transaction fees (network fees + priority fees).
    #[wasm_bindgen(getter, js_name = feeAmount)]
    pub fn fees(&self) -> BigInt {
        BigInt::from(self.inner.fees())
    }

    /// Calculated transaction mass.
    #[wasm_bindgen(getter)]
    pub fn mass(&self) -> BigInt {
        BigInt::from(self.inner.mass())
    }

    /// Minimum number of signatures required by the transaction.
    /// (as specified during the transaction creation).
    #[wasm_bindgen(getter, js_name = "minimumSignatures")]
    pub fn minimum_signatures(&self) -> u16 {
        self.inner.minimum_signatures()
    }

    /// Total aggregate input amount.
    #[wasm_bindgen(getter, js_name = aggregateInputAmount)]
    pub fn aggregate_input_value(&self) -> BigInt {
        BigInt::from(self.inner.aggregate_input_value())
    }

    /// Total aggregate output amount.
    #[wasm_bindgen(getter, js_name = aggregateOutputAmount)]
    pub fn aggregate_output_value(&self) -> BigInt {
        BigInt::from(self.inner.aggregate_output_value())
    }

    /// Transaction type ("batch" or "final").
    #[wasm_bindgen(getter, js_name = "type")]
    pub fn kind(&self) -> String {
        if self.inner.is_batch() {
            "batch".to_string()
        } else {
            "final".to_string()
        }
    }

    /// List of unique addresses used by transaction inputs.
    /// This method can be used to determine addresses used by transaction inputs
    /// in order to select private keys needed for transaction signing.
    pub fn addresses(&self) -> Array {
        self.inner.addresses().iter().map(|address| JsValue::from(address.to_string())).collect()
    }

    /// Provides a list of UTXO entries used by the transaction.
    #[wasm_bindgen(js_name = getUtxoEntries)]
    pub fn get_utxo_entries(&self) -> Array {
        self.inner.utxo_entries().values().map(|utxo_entry| JsValue::from(utxo_entry.clone())).collect()
    }

    /// Creates and returns a signature for the input at the specified index.
    #[wasm_bindgen(js_name = createInputSignature)]
    pub fn create_input_signature(
        &self,
        input_index: u8,
        private_key: &PrivateKey,
        sighash_type: Option<SighashType>,
    ) -> Result<HexString> {
        let signature = self.inner.create_input_signature(
            input_index.into(),
            &private_key.secret_bytes(),
            sighash_type.unwrap_or(SighashType::All).into(),
        )?;

        Ok(signature.to_hex().into())
    }

    /// Sets a signature to the input at the specified index.
    #[wasm_bindgen(js_name = fillInput)]
    pub fn fill_input(&self, input_index: u8, signature_script: BinaryT) -> Result<()> {
        self.inner.fill_input(input_index.into(), signature_script.try_as_vec_u8()?)
    }

    /// Signs the input at the specified index with the supplied private key
    /// and an optional SighashType.
    #[wasm_bindgen(js_name = signInput)]
    pub fn sign_input(&self, input_index: u8, private_key: &PrivateKey, sighash_type: Option<SighashType>) -> Result<()> {
        self.inner.sign_input(input_index.into(), &private_key.secret_bytes(), sighash_type.unwrap_or(SighashType::All).into())?;

        Ok(())
    }

    /// Signs transaction with supplied [`Array`] or [`PrivateKey`] or an array of
    /// raw private key bytes (encoded as `Uint8Array` or as hex strings)
    pub fn sign(&self, js_value: PrivateKeyArrayT, check_fully_signed: Option<bool>) -> Result<()> {
        if let Ok(keys) = js_value.dyn_into::<Array>() {
            let keys = keys
                .iter()
                .map(PrivateKey::try_owned_from)
                .collect::<std::result::Result<Vec<_>, kaspa_wallet_keys::error::Error>>()?;
            let mut keys = keys.iter().map(|key| key.secret_bytes()).collect::<Vec<_>>();
            self.inner.try_sign_with_keys(&keys, check_fully_signed)?;
            keys.zeroize();
            Ok(())
        } else {
            Err(Error::custom("Please supply an array of keys"))
        }
    }

    /// Submit transaction to the supplied [`RpcClient`]
    /// **IMPORTANT:** This method will remove UTXOs from the associated
    /// {@link UtxoContext} if one was used to create the transaction
    /// and will return UTXOs back to {@link UtxoContext} in case of
    /// a failed submission.
    ///
    /// # Important
    ///
    /// Make sure to consume the returned `txid` value. Always invoke this method
    /// as follows `let txid = await pendingTransaction.submit(rpc);`. If you do not
    /// consume the returned value and the rpc object is temporary, the GC will
    /// collect the `rpc` object passed to submit() potentially causing a panic.
    ///
    /// @see {@link RpcClient.submitTransaction}
    pub async fn submit(&self, wasm_rpc_client: &RpcClient) -> Result<String> {
        let rpc: Arc<DynRpcApi> = wasm_rpc_client.client().clone();
        let txid = self.inner.try_submit(&rpc).await?;
        Ok(txid.to_string())
    }

    /// Returns encapsulated network [`Transaction`]
    #[wasm_bindgen(getter)]
    pub fn transaction(&self) -> Result<Transaction> {
        Ok(Transaction::from_cctx_transaction(&self.inner.transaction(), self.inner.utxo_entries()))
    }

    /// Serializes the transaction to a pure JavaScript Object.
    /// The schema of the JavaScript object is defined by {@link ISerializableTransaction}.
    /// @see {@link ISerializableTransaction}
    /// @see {@link Transaction}, {@link ISerializableTransaction}
    #[wasm_bindgen(js_name = "serializeToObject")]
    pub fn serialize_to_object(&self) -> Result<TransactionT> {
        Ok(numeric::SerializableTransaction::from_cctx_transaction(&self.inner.transaction(), self.inner.utxo_entries())?
            .serialize_to_object()?
            .into())
    }

    /// Serializes the transaction to a JSON string.
    /// The schema of the JSON is defined by {@link ISerializableTransaction}.
    /// Once serialized, the transaction can be deserialized using {@link Transaction.deserializeFromJSON}.
    /// @see {@link Transaction}, {@link ISerializableTransaction}
    #[wasm_bindgen(js_name = "serializeToJSON")]
    pub fn serialize_to_json(&self) -> Result<String> {
        Ok(numeric::SerializableTransaction::from_cctx_transaction(&self.inner.transaction(), self.inner.utxo_entries())?
            .serialize_to_json()?)
    }

    /// Serializes the transaction to a "Safe" JSON schema where it converts all `bigint` values to `string` to avoid potential client-side precision loss.
    /// Once serialized, the transaction can be deserialized using {@link Transaction.deserializeFromSafeJSON}.
    /// @see {@link Transaction}, {@link ISerializableTransaction}
    #[wasm_bindgen(js_name = "serializeToSafeJSON")]
    pub fn serialize_to_json_safe(&self) -> Result<String> {
        Ok(string::SerializableTransaction::from_cctx_transaction(&self.inner.transaction(), self.inner.utxo_entries())?
            .serialize_to_json()?)
    }
}

impl From<native::PendingTransaction> for PendingTransaction {
    fn from(pending_transaction: native::PendingTransaction) -> Self {
        Self { inner: pending_transaction }
    }
}
