use crate::bindings::python::tx::generator::pending::PendingTransaction;
use crate::bindings::python::tx::generator::summary::GeneratorSummary;
use crate::imports::*;
use crate::tx::{generator as native, Fees, PaymentDestination, PaymentOutput, PaymentOutputs};

pub struct PyUtxoEntries {
    pub entries: Vec<UtxoEntryReference>,
}

impl FromPyObject<'_> for PyUtxoEntries {
    fn extract_bound(ob: &Bound<'_, PyAny>) -> PyResult<Self> {
        // Must be list
        let list = ob.downcast::<PyList>()?;

        let entries = list
            .iter()
            .map(|item| {
                if let Ok(entry) = item.extract::<UtxoEntryReference>() {
                    Ok(entry)
                } else if let Ok(entry) = item.downcast::<PyDict>() {
                    UtxoEntryReference::try_from(entry)
                } else {
                    Err(PyException::new_err("All entries must be UtxoEntryReference instance or compatible dict"))
                }
            })
            .collect::<PyResult<Vec<UtxoEntryReference>>>()?;

        Ok(PyUtxoEntries { entries })
    }
}

pub struct PyOutputs {
    pub outputs: Vec<PaymentOutput>,
}

impl FromPyObject<'_> for PyOutputs {
    fn extract_bound(ob: &Bound<'_, PyAny>) -> PyResult<Self> {
        // Must be list
        let list = ob.downcast::<PyList>()?;

        let outputs = list
            .iter()
            .map(|item| {
                if let Ok(output) = item.extract::<PaymentOutput>() {
                    Ok(output)
                } else if let Ok(output) = item.downcast::<PyDict>() {
                    PaymentOutput::try_from(output)
                } else {
                    Err(PyException::new_err("All outputs must be PaymentOutput instance or compatible dict"))
                }
            })
            .collect::<PyResult<Vec<PaymentOutput>>>()?;

        Ok(PyOutputs { outputs })
    }
}

#[pyclass]
pub struct Generator {
    inner: Arc<native::Generator>,
}

#[pymethods]
impl Generator {
    #[new]
    #[pyo3(signature = (network_id, entries, change_address, outputs=None, payload=None, fee_rate=None, priority_fee=None, priority_entries=None, sig_op_count=None, minimum_signatures=None))]
    pub fn ctor(
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
    ) -> PyResult<Generator> {
        let settings = GeneratorSettings::new(
            outputs,
            change_address,
            fee_rate,
            priority_fee,
            entries.entries,
            priority_entries.map(|p| p.entries),
            sig_op_count,
            minimum_signatures,
            payload.map(Into::into),
            network_id,
        );

        let settings = match settings.source {
            GeneratorSource::UtxoEntries(utxo_entries) => {
                let change_address = settings
                    .change_address
                    .ok_or_else(|| PyException::new_err("changeAddress is required for Generator constructor with UTXO entries"))?;

                let network_id = settings
                    .network_id
                    .ok_or_else(|| PyException::new_err("networkId is required for Generator constructor with UTXO entries"))?;

                native::GeneratorSettings::try_new_with_iterator(
                    network_id,
                    Box::new(utxo_entries.into_iter()),
                    settings.priority_utxo_entries,
                    change_address,
                    settings.sig_op_count,
                    settings.minimum_signatures,
                    settings.final_transaction_destination,
                    None,
                    settings.final_priority_fee,
                    settings.payload,
                    settings.multiplexer,
                )?
            }
            GeneratorSource::UtxoContext(_) => unimplemented!(),
        };

        let abortable = Abortable::default();
        let generator = native::Generator::try_new(settings, None, Some(&abortable))?;

        Ok(Self { inner: Arc::new(generator) })
    }

    pub fn estimate(&self) -> PyResult<GeneratorSummary> {
        self.inner.iter().collect::<Result<Vec<_>>>()?;
        Ok(self.inner.summary().into())
    }

    pub fn summary(&self) -> GeneratorSummary {
        self.inner.summary().into()
    }
}

impl Generator {
    pub fn iter(&self) -> impl Iterator<Item = Result<native::PendingTransaction>> {
        self.inner.iter()
    }

    pub fn stream(&self) -> impl Stream<Item = Result<native::PendingTransaction>> {
        self.inner.stream()
    }
}

#[pymethods]
impl Generator {
    fn __iter__(slf: PyRefMut<Self>) -> PyResult<Py<Generator>> {
        Ok(slf.into())
    }

    fn __next__(slf: PyRefMut<Self>) -> PyResult<Option<PendingTransaction>> {
        match slf.inner.iter().next() {
            Some(result) => match result {
                Ok(transaction) => Ok(Some(transaction.into())),
                Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(format!("{}", e))),
            },
            None => Ok(None),
        }
    }
}

#[allow(dead_code)]
enum GeneratorSource {
    UtxoEntries(Vec<UtxoEntryReference>),
    UtxoContext(UtxoContext),
    // Account(Account),
}

#[allow(dead_code)]
struct GeneratorSettings {
    pub network_id: Option<NetworkId>,
    pub source: GeneratorSource,
    pub priority_utxo_entries: Option<Vec<UtxoEntryReference>>,
    pub multiplexer: Option<Multiplexer<Box<Events>>>,
    pub final_transaction_destination: PaymentDestination,
    pub change_address: Option<Address>,
    pub fee_rate: Option<f64>,
    pub final_priority_fee: Fees,
    pub sig_op_count: u8,
    pub minimum_signatures: u16,
    pub payload: Option<Vec<u8>>,
}

impl GeneratorSettings {
    pub fn new(
        outputs: Option<PyOutputs>,
        change_address: Address,
        fee_rate: Option<f64>,
        priority_fee: Option<u64>,
        entries: Vec<UtxoEntryReference>,
        priority_entries: Option<Vec<UtxoEntryReference>>,
        sig_op_count: Option<u8>,
        minimum_signatures: Option<u16>,
        payload: Option<Vec<u8>>,
        network_id: &str,
    ) -> GeneratorSettings {
        let network_id = NetworkId::from_str(network_id).unwrap();

        let final_transaction_destination = match outputs {
            Some(py_outputs) => PaymentOutputs { outputs: py_outputs.outputs }.into(),
            None => PaymentDestination::Change,
        };

        let fee_rate = fee_rate.and_then(|v| (v.is_finite() && !v.is_nan() && v >= 1e-8).then_some(v));

        let final_priority_fee = match priority_fee {
            Some(fee) => fee.try_into().unwrap(),
            None => Fees::None,
        };

        // PY-TODO support GeneratorSource::UtxoContext when available
        let generator_source =
            GeneratorSource::UtxoEntries(entries.iter().map(|entry| UtxoEntryReference::try_from(entry.clone()).unwrap()).collect());

        let sig_op_count = sig_op_count.unwrap_or(1);

        let minimum_signatures = minimum_signatures.unwrap_or(1);

        GeneratorSettings {
            network_id: Some(network_id),
            source: generator_source,
            priority_utxo_entries: priority_entries,
            multiplexer: None,
            final_transaction_destination,
            change_address: Some(change_address),
            fee_rate,
            final_priority_fee,
            sig_op_count,
            minimum_signatures,
            payload,
        }
    }
}
