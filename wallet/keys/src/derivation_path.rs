//!
//! Implementation of the [`DerivationPath`] manager for arbitrary derivation paths.
//!

use crate::imports::*;
use workflow_wasm::prelude::*;

///
/// Key derivation path
///
/// @category Wallet SDK
#[derive(Clone, CastFromJs)]
#[cfg_attr(feature = "py-sdk", pyclass)]
#[wasm_bindgen]
pub struct DerivationPath {
    inner: kaspa_bip32::DerivationPath,
}

#[wasm_bindgen]
impl DerivationPath {
    #[wasm_bindgen(constructor)]
    pub fn new(path: &str) -> Result<DerivationPath> {
        let inner = kaspa_bip32::DerivationPath::from_str(path)?;
        Ok(Self { inner })
    }

    /// Is this derivation path empty? (i.e. the root)
    #[wasm_bindgen(js_name = isEmpty)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Get the count of [`ChildNumber`] values in this derivation path.
    #[wasm_bindgen(js_name = length)]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Get the parent [`DerivationPath`] for the current one.
    ///
    /// Returns `Undefined` if this is already the root path.
    pub fn parent(&self) -> Option<DerivationPath> {
        self.inner.parent().map(|inner| Self { inner })
    }

    /// Push a [`ChildNumber`] onto an existing derivation path.
    pub fn push(&mut self, child_number: u32, hardened: Option<bool>) -> Result<()> {
        let child = ChildNumber::new(child_number, hardened.unwrap_or(false))?;
        self.inner.push(child);
        Ok(())
    }

    #[wasm_bindgen(js_name = toString)]
    pub fn to_str(&self) -> String {
        self.inner.to_string()
    }
}

#[cfg(feature = "py-sdk")]
#[pymethods]
impl DerivationPath {
    #[new]
    pub fn new_py(path: &str) -> PyResult<DerivationPath> {
        let inner = kaspa_bip32::DerivationPath::from_str(path)?;
        Ok(Self { inner })
    }

    #[pyo3(name = "is_empty")]
    pub fn is_empty_py(&self) -> bool {
        self.inner.is_empty()
    }

    #[pyo3(name = "length")]
    pub fn len_py(&self) -> usize {
        self.inner.len()
    }

    #[pyo3(name = "parent")]
    pub fn parent_py(&self) -> Option<DerivationPath> {
        self.inner.parent().map(|inner| Self { inner })
    }

    #[pyo3(name = "push")]
    #[pyo3(signature = (child_number, hardened=None))]
    pub fn push_py(&mut self, child_number: u32, hardened: Option<bool>) -> PyResult<()> {
        let child = ChildNumber::new(child_number, hardened.unwrap_or(false))?;
        self.inner.push(child);
        Ok(())
    }

    #[pyo3(name = "to_string")]
    pub fn to_str_py(&self) -> String {
        self.inner.to_string()
    }
}

impl TryCastFromJs for DerivationPath {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            let value = value.as_ref();
            if let Some(path) = value.as_string() {
                Ok(DerivationPath::new(&path)?)
            } else {
                Err(Error::custom("Invalid derivation path"))
            }
        })
    }
}

impl<'a> From<&'a DerivationPath> for &'a kaspa_bip32::DerivationPath {
    fn from(value: &'a DerivationPath) -> Self {
        &value.inner
    }
}
