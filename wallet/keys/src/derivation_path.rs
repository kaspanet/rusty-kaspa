use crate::imports::*;
use workflow_wasm::prelude::*;

/// Key derivation path
/// @category Wallet SDK
#[derive(Clone)]
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

impl TryFrom<JsValue> for DerivationPath {
    type Error = Error;
    fn try_from(value: JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(path) = value.as_string() {
            return Self::new(&path);
        }

        Ok(ref_from_abi!(DerivationPath, &value)?)
    }
}

impl From<DerivationPath> for kaspa_bip32::DerivationPath {
    fn from(value: DerivationPath) -> Self {
        value.inner
    }
}
