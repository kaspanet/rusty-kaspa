use crate::imports::*;
use workflow_wasm::prelude::*;

/// Key derivation path
/// @category Wallet SDK
#[derive(Clone, CastFromJs)]
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

impl TryCastFromJs for DerivationPath {
    type Error = Error;
    fn try_cast_from(value: impl AsRef<JsValue>) -> Result<Cast<Self>, Self::Error> {
        Self::resolve(&value, || {
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
