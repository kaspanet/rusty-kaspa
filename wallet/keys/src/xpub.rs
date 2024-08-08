use crate::imports::*;

///
/// Extended public key (XPub).
///
/// This class allows accepts another XPub and and provides
/// functions for derivation of dependent child public keys.
///
/// Please note that Kaspa extended public keys use `kpub` prefix.
///
/// @see {@link PrivateKeyGenerator}, {@link PublicKeyGenerator}, {@link XPrv}, {@link Mnemonic}
/// @category Wallet SDK
///
#[derive(Clone, CastFromJs)]
#[cfg_attr(feature = "py-sdk", pyclass)]
#[wasm_bindgen]
pub struct XPub {
    inner: ExtendedPublicKey<secp256k1::PublicKey>,
}

impl XPub {
    pub fn inner(&self) -> &ExtendedPublicKey<secp256k1::PublicKey> {
        &self.inner
    }
}

#[wasm_bindgen]
impl XPub {
    #[wasm_bindgen(constructor)]
    pub fn try_new(xpub: &str) -> Result<XPub> {
        let inner = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(xpub)?;
        Ok(Self { inner })
    }

    #[wasm_bindgen(js_name=deriveChild)]
    pub fn derive_child(&self, chile_number: u32, hardened: Option<bool>) -> Result<XPub> {
        let chile_number = ChildNumber::new(chile_number, hardened.unwrap_or(false))?;
        let inner = self.inner.derive_child(chile_number)?;
        Ok(Self { inner })
    }

    #[wasm_bindgen(js_name=derivePath)]
    pub fn derive_path(&self, path: &JsValue) -> Result<XPub> {
        let path = DerivationPath::try_cast_from(path)?;
        let inner = self.inner.clone().derive_path(path.as_ref().into())?;
        Ok(Self { inner })
    }

    //#[wasm_bindgen(js_name = toString)]
    #[wasm_bindgen(js_name = intoString)]
    pub fn to_str(&self, prefix: &str) -> Result<String> {
        Ok(self.inner.to_string(Some(prefix.try_into()?)))
    }

    #[wasm_bindgen(js_name = toPublicKey)]
    pub fn public_key(&self) -> PublicKey {
        self.inner.public_key().into()
    }
}

#[cfg(feature = "py-sdk")]
#[pymethods]
impl XPub {
    #[new]
    pub fn try_new_py(xpub: String) -> PyResult<XPub> {
        let inner = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(&xpub)?;
        Ok(Self { inner })
    }

    #[pyo3(name = "derive_child")]
    pub fn derive_child_py(&self, chile_number: u32, hardened: Option<bool>) -> PyResult<XPub> {
        let chile_number = ChildNumber::new(chile_number, hardened.unwrap_or(false))?;
        let inner = self.inner.derive_child(chile_number)?;
        Ok(Self { inner })
    }

    #[pyo3(name = "derive_path")]
    pub fn derive_path_py(&self, path: String) -> PyResult<XPub> {
        let path = DerivationPath::new(path.as_str())?;
        let inner = self.inner.clone().derive_path((&path).into())?;
        Ok(Self { inner })
    }

    #[pyo3(name = "to_str")]
    pub fn to_str_py(&self, prefix: &str) -> Result<String> {
        Ok(self.inner.to_string(Some(prefix.try_into()?)))
    }

    #[pyo3(name = "public_key")]
    pub fn public_key_py(&self) -> PublicKey {
        self.inner.public_key().into()
    }
}

impl From<ExtendedPublicKey<secp256k1::PublicKey>> for XPub {
    fn from(inner: ExtendedPublicKey<secp256k1::PublicKey>) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "XPub | string")]
    pub type XPubT;
}

impl TryCastFromJs for XPub {
    type Error = Error;
    fn try_cast_from(value: impl AsRef<JsValue>) -> Result<Cast<Self>, Self::Error> {
        Self::resolve(&value, || {
            if let Some(xpub) = value.as_ref().as_string() {
                Ok(XPub::try_new(xpub.as_str())?)
            } else {
                Err(Error::InvalidXPub)
            }
        })
    }
}
