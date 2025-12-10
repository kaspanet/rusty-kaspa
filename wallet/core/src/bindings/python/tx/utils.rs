use crate::bindings::python::tx::generator::{Generator, GeneratorSummary, PendingTransaction, PyOutputs, PyUtxoEntries};
use crate::imports::*;
use kaspa_consensus_client::*;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;

#[pyfunction]
#[pyo3(name = "create_transaction")]
#[pyo3(signature = (utxo_entry_source, outputs, priority_fee, payload=None, sig_op_count=None))]
pub fn create_transaction_py(
    utxo_entry_source: PyUtxoEntries,
    outputs: PyOutputs,
    priority_fee: u64,
    payload: Option<PyBinary>,
    sig_op_count: Option<u8>,
) -> PyResult<Transaction> {
    let payload: Vec<u8> = payload.map(Into::into).unwrap_or_default();
    let sig_op_count = sig_op_count.unwrap_or(1);

    let mut total_input_amount = 0;
    let mut entries = vec![];

    let inputs = utxo_entry_source
        .entries
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
        return Err(PyException::new_err(format!("priority fee({priority_fee}) > amount({total_input_amount})")));
    }

    let outputs = outputs.outputs.into_iter().map(|output| output.into()).collect::<Vec<TransactionOutput>>();
    let transaction = Transaction::new(None, 0, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, payload, 0)?;

    Ok(transaction)
}

#[pyfunction]
#[pyo3(name = "create_transactions")]
#[pyo3(signature = (network_id, entries, change_address, outputs=None, payload=None, fee_rate=None, priority_fee=None, priority_entries=None, sig_op_count=None, minimum_signatures=None))]
pub fn create_transactions_py<'a>(
    py: Python<'a>,
    network_id: &str,
    entries: PyUtxoEntries,
    change_address: Address,
    outputs: Option<PyOutputs>,
    payload: Option<PyBinary>,
    fee_rate: Option<f64>,
    priority_fee: Option<u64>,
    priority_entries: Option<PyUtxoEntries>,
    sig_op_count: Option<u8>,
    minimum_signatures: Option<u16>,
) -> PyResult<Bound<'a, PyDict>> {
    let generator = Generator::ctor(
        network_id,
        entries,
        change_address,
        outputs,
        payload.map(Into::into),
        fee_rate,
        priority_fee,
        priority_entries,
        sig_op_count,
        minimum_signatures,
    )?;

    let transactions = generator.iter().map(|r| r.map(PendingTransaction::from).map(|tx| tx)).collect::<Result<Vec<_>>>()?;
    let summary = generator.summary();
    let dict = PyDict::new(py);
    dict.set_item("transactions", transactions)?;
    dict.set_item("summary", summary)?;
    Ok(dict)
}

#[pyfunction]
#[pyo3(name = "estimate_transactions")]
#[pyo3(signature = (network_id, entries, change_address, outputs=None, payload=None, fee_rate=None, priority_fee=None, priority_entries=None, sig_op_count=None, minimum_signatures=None))]
pub fn estimate_transactions_py<'a>(
    network_id: &str,
    entries: PyUtxoEntries,
    change_address: Address,
    outputs: Option<PyOutputs>,
    payload: Option<PyBinary>,
    fee_rate: Option<f64>,
    priority_fee: Option<u64>,
    priority_entries: Option<PyUtxoEntries>,
    sig_op_count: Option<u8>,
    minimum_signatures: Option<u16>,
) -> PyResult<GeneratorSummary> {
    let generator = Generator::ctor(
        network_id,
        entries,
        change_address,
        outputs,
        payload.map(Into::into),
        fee_rate,
        priority_fee,
        priority_entries,
        sig_op_count,
        minimum_signatures,
    )?;

    generator.iter().collect::<Result<Vec<_>>>()?;
    Ok(generator.summary())
}
