use crate::result::Result;
use crate::wasm::opcodes::Opcodes;
use crate::{script_builder as native, standard};
use faster_hex::hex_decode;
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_utils::hex::ToHex;
use pyo3::prelude::*;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone)]
#[pyclass]
pub struct ScriptBuilder {
    script_builder: Arc<Mutex<native::ScriptBuilder>>,
}

impl ScriptBuilder {
    #[inline]
    pub fn inner(&self) -> MutexGuard<native::ScriptBuilder> {
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
    pub fn from_script(script: Bound<PyAny>) -> PyResult<ScriptBuilder> {
        let builder = ScriptBuilder::default();
        let script = PyBinary::try_from(script)?;
        builder.inner().extend(&script.data);

        Ok(builder)
    }

    pub fn add_op(&self, op: Bound<PyAny>) -> PyResult<ScriptBuilder> {
        let op = extract_ops(op)?;
        let mut inner = self.inner();
        inner.add_op(op[0]).map_err(|err| pyo3::exceptions::PyException::new_err(format!("{}", err)))?;

        Ok(self.clone())
    }

    pub fn add_ops(&self, opcodes: Bound<PyAny>) -> PyResult<ScriptBuilder> {
        let ops = extract_ops(opcodes)?;
        self.inner().add_ops(&ops.as_slice()).map_err(|err| pyo3::exceptions::PyException::new_err(format!("{}", err)))?;

        Ok(self.clone())
    }

    pub fn add_data(&self, data: Bound<PyAny>) -> PyResult<ScriptBuilder> {
        let data = PyBinary::try_from(data)?;

        let mut inner = self.inner();
        inner.add_data(&data.data).map_err(|err| pyo3::exceptions::PyException::new_err(format!("{}", err)))?;

        Ok(self.clone())
    }

    pub fn add_i64(&self, value: i64) -> PyResult<ScriptBuilder> {
        let mut inner = self.inner();
        inner.add_i64(value).map_err(|err| pyo3::exceptions::PyException::new_err(format!("{}", err)))?;

        Ok(self.clone())
    }

    pub fn add_lock_time(&self, lock_time: u64) -> PyResult<ScriptBuilder> {
        let mut inner = self.inner();
        inner.add_lock_time(lock_time).map_err(|err| pyo3::exceptions::PyException::new_err(format!("{}", err)))?;

        Ok(self.clone())
    }

    pub fn add_sequence(&self, sequence: u64) -> PyResult<ScriptBuilder> {
        let mut inner = self.inner();
        inner.add_sequence(sequence).map_err(|err| pyo3::exceptions::PyException::new_err(format!("{}", err)))?;

        Ok(self.clone())
    }

    #[staticmethod]
    pub fn canonical_data_size(data: Bound<PyAny>) -> PyResult<u32> {
        let data = PyBinary::try_from(data)?;
        let size = native::ScriptBuilder::canonical_data_size(&data.data.as_slice()) as u32;

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
    pub fn pay_to_script_hash_signature_script(&self, signature: String) -> Result<String> {
        let inner = self.inner();
        let script = inner.script();
        let signature = signature.as_bytes();
        let generated_script = standard::pay_to_script_hash_signature_script(script.into(), signature.to_vec())?;

        Ok(generated_script.to_hex().into())
    }

    // pub fn hex_view(&self, args: Option<HexViewConfigT>) -> Result<String> {
    //     let inner = self.inner();
    //     let script = inner.script();

    //     let config = args.map(HexViewConfig::try_from).transpose()?.unwrap_or_default();
    //     Ok(config.build(script).to_string())
    // }
}

fn extract_ops(input: Bound<PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(opcode) = extract_op(&input) {
        // Single u8 or Opcodes variant
        Ok(vec![opcode])
    } else if let Ok(list) = input.downcast::<pyo3::types::PyList>() {
        // List of u8 or Opcodes variants
        list.iter().map(|item| extract_op(&item)).collect::<PyResult<Vec<u8>>>()
    } else {
        Err(pyo3::exceptions::PyTypeError::new_err("Expected an Opcodes enum or an integer."))
    }
}

fn extract_op(item: &Bound<PyAny>) -> PyResult<u8> {
    if let Ok(op) = item.extract::<u8>() {
        Ok(op)
    } else if let Ok(op) = item.extract::<Opcodes>() {
        Ok(op.value())
    } else {
        Err(pyo3::exceptions::PyTypeError::new_err("Expected Opcodes variant or u8"))
    }
}

struct PyBinary {
    pub data: Vec<u8>,
}

impl TryFrom<Bound<'_, PyAny>> for PyBinary {
    type Error = PyErr;
    fn try_from(value: Bound<PyAny>) -> Result<Self, Self::Error> {
        if let Ok(str) = value.extract::<String>() {
            // Python `str` (of valid hex)
            let mut data = vec![0u8; str.len() / 2];
            match hex_decode(str.as_bytes(), &mut data) {
                Ok(()) => Ok(PyBinary { data }), // Hex string
                Err(_) => Err(pyo3::exceptions::PyValueError::new_err("Invalid hex string")),
            }
        } else if let Ok(py_bytes) = value.downcast::<pyo3::types::PyBytes>() {
            // Python `bytes` type
            Ok(PyBinary { data: py_bytes.as_bytes().to_vec() })
        } else if let Ok(op_list) = value.downcast::<pyo3::types::PyList>() {
            // Python `[int]` (list of bytes)
            let data = op_list.iter().map(|item| item.extract::<u8>().unwrap()).collect();
            Ok(PyBinary { data })
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err("Expected `str` (of valid hex), `bytes`, `int` (u8), or `[int]` (u8)"))
        }
    }
}
