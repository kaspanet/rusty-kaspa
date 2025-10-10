use crate::bindings::opcodes::Opcodes;
use crate::{script_builder as native, standard};
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_python_core::types::PyBinary;
use kaspa_utils::hex::ToHex;
use pyo3::{exceptions::PyException, prelude::*};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone)]
#[pyclass]
pub struct ScriptBuilder {
    script_builder: Arc<Mutex<native::ScriptBuilder>>,
}

impl ScriptBuilder {
    #[inline]
    pub fn inner(&self) -> MutexGuard<'_, native::ScriptBuilder> {
        self.script_builder.lock().unwrap()
    }
}

impl Default for ScriptBuilder {
    fn default() -> Self {
        Self { script_builder: Arc::new(Mutex::new(native::ScriptBuilder::new())) }
    }
}

#[pymethods]
impl ScriptBuilder {
    #[new]
    pub fn new() -> Self {
        Self::default()
    }

    #[staticmethod]
    pub fn from_script(script: PyBinary) -> PyResult<ScriptBuilder> {
        let builder = ScriptBuilder::default();
        let script: Vec<u8> = script.into();
        builder.inner().script_mut().extend(&script);

        Ok(builder)
    }

    pub fn add_op(&self, op: &Bound<PyAny>) -> PyResult<ScriptBuilder> {
        let op = extract_ops(op)?;
        let mut inner = self.inner();
        inner.add_op(op[0]).map_err(|err| PyException::new_err(format!("{}", err)))?;

        Ok(self.clone())
    }

    pub fn add_ops(&self, opcodes: &Bound<PyAny>) -> PyResult<ScriptBuilder> {
        let ops = extract_ops(opcodes)?;
        self.inner().add_ops(&ops.as_slice()).map_err(|err| PyException::new_err(format!("{}", err)))?;

        Ok(self.clone())
    }

    pub fn add_data(&self, data: PyBinary) -> PyResult<ScriptBuilder> {
        let mut inner = self.inner();
        inner.add_data(data.as_ref()).map_err(|err| PyException::new_err(format!("{}", err)))?;

        Ok(self.clone())
    }

    pub fn add_i64(&self, value: i64) -> PyResult<ScriptBuilder> {
        let mut inner = self.inner();
        inner.add_i64(value).map_err(|err| PyException::new_err(format!("{}", err)))?;

        Ok(self.clone())
    }

    pub fn add_lock_time(&self, lock_time: u64) -> PyResult<ScriptBuilder> {
        let mut inner = self.inner();
        inner.add_lock_time(lock_time).map_err(|err| PyException::new_err(format!("{}", err)))?;

        Ok(self.clone())
    }

    pub fn add_sequence(&self, sequence: u64) -> PyResult<ScriptBuilder> {
        let mut inner = self.inner();
        inner.add_sequence(sequence).map_err(|err| PyException::new_err(format!("{}", err)))?;

        Ok(self.clone())
    }

    #[staticmethod]
    pub fn canonical_data_size(data: PyBinary) -> PyResult<u32> {
        let size = native::ScriptBuilder::canonical_data_size(data.as_ref()) as u32;

        Ok(size)
    }

    pub fn to_string(&self) -> String {
        let inner = self.inner();

        inner.script().to_vec().iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn drain(&self) -> String {
        let mut inner = self.inner();

        String::from_utf8(inner.drain()).unwrap()
    }

    #[pyo3(name = "create_pay_to_script_hash_script")]
    pub fn pay_to_script_hash_script(&self) -> ScriptPublicKey {
        let inner = self.inner();
        let script = inner.script();

        standard::pay_to_script_hash_script(script)
    }

    #[pyo3(name = "encode_pay_to_script_hash_signature_script")]
    pub fn pay_to_script_hash_signature_script(&self, signature: PyBinary) -> PyResult<String> {
        let inner = self.inner();
        let script = inner.script();
        let generated_script = standard::pay_to_script_hash_signature_script(script.into(), signature.into())
            .map_err(|err| PyException::new_err(format!("{}", err)))?;

        Ok(generated_script.to_hex().into())
    }
}

// PY-TODO change to PyOpcode struct and handle similar to PyBinary?
fn extract_ops(input: &Bound<PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(opcode) = extract_op(&input) {
        // Single u8 or Opcodes variant
        Ok(vec![opcode])
    } else if let Ok(list) = input.downcast::<pyo3::types::PyList>() {
        // List of u8 or Opcodes variants
        list.iter().map(|item| extract_op(&item)).collect::<PyResult<Vec<u8>>>()
    } else {
        Err(PyException::new_err("Expected an Opcodes enum variant or an integer."))
    }
}

fn extract_op(item: &Bound<PyAny>) -> PyResult<u8> {
    if let Ok(op) = item.extract::<u8>() {
        Ok(op)
    } else if let Ok(op) = item.extract::<Opcodes>() {
        Ok(op.value())
    } else {
        Err(PyException::new_err("Expected Opcodes enum variant or u8"))
    }
}
