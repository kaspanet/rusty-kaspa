use crate::imports::*;
use crate::python::tx::generator::{Generator, PendingTransaction};
use crate::tx::payment::PaymentOutput;
use kaspa_consensus_client::*;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;

#[pyfunction]
#[pyo3(name = "create_transaction")]
pub fn create_transaction_py(
    utxo_entry_source: Vec<&PyDict>,
    outputs: Vec<&PyDict>,
    priority_fee: u64,
    payload: Option<PyBinary>,
    sig_op_count: Option<u8>,
) -> PyResult<Transaction> {
    let utxo_entries: Vec<UtxoEntryReference> =
        utxo_entry_source.iter().map(|utxo| UtxoEntryReference::try_from(*utxo)).collect::<Result<Vec<_>, _>>()?;

    let outputs: Vec<PaymentOutput> = outputs.iter().map(|utxo| PaymentOutput::try_from(*utxo)).collect::<Result<Vec<_>, _>>()?;

    let payload: Vec<u8> = payload.map(Into::into).unwrap_or_default();
    let sig_op_count = sig_op_count.unwrap_or(1);

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
        return Err(PyException::new_err(format!("priority fee({priority_fee}) > amount({total_input_amount})")));
    }

    let outputs = outputs.into_iter().map(|output| output.into()).collect::<Vec<TransactionOutput>>();
    let transaction = Transaction::new(None, 0, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, payload, 0)?;

    Ok(transaction)
}

#[pyfunction]
#[pyo3(name = "create_transactions")]
pub fn create_transactions_py<'a>(
    py: Python<'a>,
    network_id: String,
    entries: Vec<&PyDict>,
    outputs: Vec<&PyDict>,
    change_address: Address,
    payload: Option<PyBinary>,
    priority_fee: Option<u64>,
    priority_entries: Option<Vec<&PyDict>>,
    sig_op_count: Option<u8>,
    minimum_signatures: Option<u16>,
) -> PyResult<Bound<'a, PyDict>> {
    let generator = Generator::ctor(
        network_id,
        entries,
        outputs,
        change_address,
        payload.map(Into::into),
        priority_fee,
        priority_entries,
        sig_op_count,
        minimum_signatures,
    )?;

    let transactions =
        generator.iter().map(|r| r.map(PendingTransaction::from).map(|tx| tx.into_py(py))).collect::<Result<Vec<_>>>()?;
    let summary = generator.summary().into_py(py);
    let dict = PyDict::new_bound(py);
    dict.set_item("transactions", &transactions)?;
    dict.set_item("summary", &summary)?;
    Ok(dict)
}
