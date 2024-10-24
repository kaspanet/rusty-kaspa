use crate::imports::*;
use crate::python::tx::generator::pending::PendingTransaction;
use crate::python::tx::generator::summary::GeneratorSummary;
use crate::tx::{generator as native, Fees, PaymentDestination, PaymentOutputs};

#[pyclass]
pub struct Generator {
    inner: Arc<native::Generator>,
}

#[pymethods]
impl Generator {
    #[new]
    pub fn ctor(
        network_id: String, // TODO this is wrong
        entries: Vec<&PyDict>,
        outputs: Vec<&PyDict>,
        change_address: Address,
        payload: Option<String>, // TODO Hex string for now, use PyBinary
        priority_fee: Option<u64>,
        priority_entries: Option<Vec<&PyDict>>,
        sig_op_count: Option<u8>,
        minimum_signatures: Option<u16>,
    ) -> PyResult<Generator> {
        let settings = GeneratorSettings::new(
            outputs,
            change_address,
            priority_fee,
            entries,
            priority_entries,
            sig_op_count,
            minimum_signatures,
            payload,
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

enum GeneratorSource {
    UtxoEntries(Vec<UtxoEntryReference>),
    UtxoContext(UtxoContext),
    // #[cfg(any(feature = "wasm32-sdk"), not(target_arch = "wasm32"))]
    // Account(Account),
}

struct GeneratorSettings {
    pub network_id: Option<NetworkId>,
    pub source: GeneratorSource,
    pub priority_utxo_entries: Option<Vec<UtxoEntryReference>>,
    pub multiplexer: Option<Multiplexer<Box<Events>>>,
    pub final_transaction_destination: PaymentDestination,
    pub change_address: Option<Address>,
    pub final_priority_fee: Fees,
    pub sig_op_count: u8,
    pub minimum_signatures: u16,
    pub payload: Option<Vec<u8>>,
}

impl GeneratorSettings {
    pub fn new(
        outputs: Vec<&PyDict>,
        change_address: Address,
        priority_fee: Option<u64>,
        entries: Vec<&PyDict>,
        priority_entries: Option<Vec<&PyDict>>,
        sig_op_count: Option<u8>,
        minimum_signatures: Option<u16>,
        payload: Option<String>, // TODO Hex string for now, use PyBinary
        network_id: String,      // TODO this is wrong
    ) -> GeneratorSettings {
        let network_id = NetworkId::from_str(&network_id).unwrap();

        // let final_transaction_destination: PaymentDestination =
        //     if outputs.is_empty() { PaymentDestination::Change } else { PaymentOutputs::try_from(outputs).unwrap().into() };
        let final_transaction_destination: PaymentDestination = PaymentOutputs::try_from(outputs).unwrap().into();

        let final_priority_fee = match priority_fee {
            Some(fee) => fee.try_into().unwrap(),
            None => Fees::None,
        };

        // TODO support GeneratorSource::UtxoContext and clean up below
        let generator_source =
            GeneratorSource::UtxoEntries(entries.iter().map(|entry| UtxoEntryReference::try_from(*entry).unwrap()).collect());

        let priority_utxo_entries = if let Some(entries) = priority_entries {
            Some(entries.iter().map(|entry| UtxoEntryReference::try_from(*entry).unwrap()).collect())
        } else {
            None
        };

        let sig_op_count = sig_op_count.unwrap_or(1);

        let minimum_signatures = minimum_signatures.unwrap_or(1);

        let payload = payload.map(|s| s.into_bytes());

        GeneratorSettings {
            network_id: Some(network_id),
            source: generator_source,
            priority_utxo_entries,
            multiplexer: None,
            final_transaction_destination,
            change_address: Some(change_address),
            final_priority_fee,
            sig_op_count,
            minimum_signatures,
            payload,
        }
    }
}
