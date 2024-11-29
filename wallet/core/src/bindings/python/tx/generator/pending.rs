use crate::imports::*;
use crate::tx::generator as native;
use kaspa_consensus_client::Transaction;
use kaspa_consensus_core::hashing::wasm::SighashType;
use kaspa_wallet_keys::privatekey::PrivateKey;
use kaspa_wrpc_python::client::RpcClient;

#[pyclass]
pub struct PendingTransaction {
    inner: native::PendingTransaction,
}

#[pymethods]
impl PendingTransaction {
    #[getter]
    fn id(&self) -> String {
        self.inner.id().to_string()
    }

    #[getter]
    #[pyo3(name = "payment_amount")]
    fn payment_value(&self) -> Option<u64> {
        self.inner.payment_value()
    }

    #[getter]
    #[pyo3(name = "change_amount")]
    fn change_value(&self) -> u64 {
        self.inner.change_value()
    }

    #[getter]
    #[pyo3(name = "fee_amount")]
    fn fees(&self) -> u64 {
        self.inner.fees()
    }

    #[getter]
    fn mass(&self) -> u64 {
        self.inner.mass()
    }

    #[getter]
    fn minimum_signatures(&self) -> u16 {
        self.inner.minimum_signatures()
    }

    #[getter]
    #[pyo3(name = "aggregate_input_amount")]
    fn aggregate_input_value(&self) -> u64 {
        self.inner.aggregate_input_value()
    }

    #[getter]
    #[pyo3(name = "aggregate_output_amount")]
    fn aggregate_output_value(&self) -> u64 {
        self.inner.aggregate_output_value()
    }

    #[getter]
    #[pyo3(name = "transaction_type")]
    fn kind(&self) -> String {
        if self.inner.is_batch() {
            "batch".to_string()
        } else {
            "final".to_string()
        }
    }

    fn addresses(&self) -> Vec<Address> {
        self.inner.addresses().clone()
    }

    fn get_utxo_entries(&self) -> Vec<UtxoEntryReference> {
        self.inner.utxo_entries().values().map(|utxo_entry| UtxoEntryReference::from(utxo_entry.clone())).collect()
    }

    #[pyo3(signature = (input_index, private_key, sighash_type=None))]
    fn create_input_signature(
        &self,
        input_index: u8,
        private_key: &PrivateKey,
        sighash_type: Option<&SighashType>,
    ) -> PyResult<String> {
        let signature = self.inner.create_input_signature(
            input_index.into(),
            &private_key.secret_bytes(),
            sighash_type.cloned().unwrap_or(SighashType::All).into(),
        )?;

        Ok(signature.to_hex())
    }

    fn fill_input(&self, input_index: u8, signature_script: PyBinary) -> PyResult<()> {
        self.inner.fill_input(input_index.into(), signature_script.into())?;

        Ok(())
    }

    #[pyo3(signature = (input_index, private_key, sighash_type=None))]
    fn sign_input(&self, input_index: u8, private_key: &PrivateKey, sighash_type: Option<&SighashType>) -> PyResult<()> {
        self.inner.sign_input(
            input_index.into(),
            &private_key.secret_bytes(),
            sighash_type.cloned().unwrap_or(SighashType::All).into(),
        )?;

        Ok(())
    }

    #[pyo3(signature = (private_keys, check_fully_signed=None))]
    fn sign(&self, private_keys: Vec<PrivateKey>, check_fully_signed: Option<bool>) -> PyResult<()> {
        let mut keys = private_keys.iter().map(|key| key.secret_bytes()).collect::<Vec<_>>();
        self.inner.try_sign_with_keys(&keys, check_fully_signed)?;
        keys.zeroize();
        Ok(())
    }

    fn submit<'py>(&self, py: Python<'py>, rpc_client: &RpcClient) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        let rpc: Arc<DynRpcApi> = rpc_client.client().clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let txid = inner.try_submit(&rpc).await?;
            Ok(txid.to_string())
        })
    }

    #[getter]
    fn transaction(&self) -> PyResult<Transaction> {
        Ok(Transaction::from_cctx_transaction(&self.inner.transaction(), self.inner.utxo_entries()))
    }
}

impl From<native::PendingTransaction> for PendingTransaction {
    fn from(pending_transaction: native::PendingTransaction) -> Self {
        Self { inner: pending_transaction }
    }
}
