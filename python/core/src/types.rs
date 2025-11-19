use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};

pub struct PyBinary {
    pub data: Vec<u8>,
}

impl<'py> FromPyObject<'_, 'py> for PyBinary {
    type Error = PyErr;

    fn extract(value: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        if let Ok(str) = value.extract::<String>() {
            // Python `str` (of valid hex)
            let mut data = vec![0u8; str.len() / 2];
            match faster_hex::hex_decode(str.as_bytes(), &mut data) {
                Ok(()) => Ok(PyBinary { data }),
                Err(_) => Err(PyException::new_err("Invalid hex string")),
            }
        } else if let Ok(py_bytes) = value.cast::<PyBytes>() {
            // Python `bytes` type
            Ok(PyBinary { data: py_bytes.as_bytes().to_vec() })
        } else if let Ok(op_list) = value.cast::<PyList>() {
            // Python `[int]` (list of bytes)
            let data = op_list.iter().map(|item| item.extract::<u8>()).collect::<PyResult<Vec<u8>>>()?;
            Ok(PyBinary { data })
        } else {
            Err(PyException::new_err("Expected `str` (of valid hex), `bytes`, or `[int]`"))
        }
    }
}

impl TryFrom<&Bound<'_, PyAny>> for PyBinary {
    type Error = PyErr;
    fn try_from(value: &Bound<PyAny>) -> Result<Self, Self::Error> {
        if let Ok(str) = value.extract::<String>() {
            // Python `str` (of valid hex)
            let mut data = vec![0u8; str.len() / 2];
            match faster_hex::hex_decode(str.as_bytes(), &mut data) {
                Ok(()) => Ok(PyBinary { data }), // Hex string
                Err(_) => Err(PyException::new_err("Invalid hex string")),
            }
        } else if let Ok(py_bytes) = value.cast::<PyBytes>() {
            // Python `bytes` type
            Ok(PyBinary { data: py_bytes.as_bytes().to_vec() })
        } else if let Ok(op_list) = value.cast::<PyList>() {
            // Python `[int]` (list of bytes)
            let data = op_list.iter().map(|item| item.extract::<u8>().unwrap()).collect();
            Ok(PyBinary { data })
        } else {
            Err(PyException::new_err("Expected `str` (of valid hex), `bytes`, or `[int]`"))
        }
    }
}

impl Into<Vec<u8>> for PyBinary {
    fn into(self) -> Vec<u8> {
        self.data
    }
}

impl AsRef<[u8]> for PyBinary {
    fn as_ref(&self) -> &[u8] {
        self.data.as_slice()
    }
}
