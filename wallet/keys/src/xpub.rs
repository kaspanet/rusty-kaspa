//!
//! Extended public key ([`XPub`]).
//!

use kaspa_bip32::{ChainCode, KeyFingerprint, Prefix};
use std::{fmt, str::FromStr};

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
#[wasm_bindgen(inspectable)]
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
    pub fn derive_child(&self, child_number: u32, hardened: Option<bool>) -> Result<XPub> {
        let child_number = ChildNumber::new(child_number, hardened.unwrap_or(false))?;
        let inner = self.inner.derive_child(child_number)?;
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

    // ~~~~ Getters ~~~~

    #[wasm_bindgen(getter)]
    pub fn xpub(&self) -> Result<String> {
        let str = self.inner.to_extended_key("kpub".try_into()?).to_string();
        Ok(str)
    }

    #[wasm_bindgen(getter)]
    pub fn depth(&self) -> u8 {
        self.inner.attrs().depth
    }

    #[wasm_bindgen(getter, js_name = parentFingerprint)]
    pub fn parent_fingerprint_as_hex_string(&self) -> String {
        self.inner.attrs().parent_fingerprint.to_vec().to_hex()
    }

    #[wasm_bindgen(getter, js_name = childNumber)]
    pub fn child_number(&self) -> u32 {
        self.inner.attrs().child_number.into()
    }

    #[wasm_bindgen(getter, js_name = chainCode)]
    pub fn chain_code_as_hex_string(&self) -> String {
        self.inner.attrs().chain_code.to_vec().to_hex()
    }
}

impl XPub {
    pub fn parent_fingerprint(&self) -> KeyFingerprint {
        self.inner.attrs().parent_fingerprint
    }

    pub fn chain_code(&self) -> ChainCode {
        self.inner.attrs().chain_code
    }
}

#[cfg(feature = "py-sdk")]
#[pymethods]
impl XPub {
    #[new]
    pub fn try_new_py(xpub: &str) -> PyResult<XPub> {
        let inner = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(xpub)?;
        Ok(Self { inner })
    }

    #[pyo3(name = "derive_child")]
    #[pyo3(signature = (child_number, hardened=None))]
    pub fn derive_child_py(&self, child_number: u32, hardened: Option<bool>) -> PyResult<XPub> {
        let child_number = ChildNumber::new(child_number, hardened.unwrap_or(false))?;
        let inner = self.inner.derive_child(child_number)?;
        Ok(Self { inner })
    }

    #[pyo3(name = "derive_path")]
    pub fn derive_path_py(&self, path: &str) -> PyResult<XPub> {
        let path = DerivationPath::new(path)?;
        let inner = self.inner.clone().derive_path((&path).into())?;
        Ok(Self { inner })
    }

    #[pyo3(name = "into_string")]
    pub fn to_str_py(&self, prefix: &str) -> PyResult<String> {
        Ok(self.inner.to_string(Some(prefix.try_into()?)))
    }

    #[pyo3(name = "to_public_key")]
    pub fn public_key_py(&self) -> PublicKey {
        self.inner.public_key().into()
    }

    #[getter]
    #[pyo3(name = "xpub")]
    pub fn xpub_py(&self) -> PyResult<String> {
        let str = self.inner.to_extended_key("kpub".try_into()?).to_string();
        Ok(str)
    }

    #[getter]
    #[pyo3(name = "depth")]
    pub fn depth_py(&self) -> u8 {
        self.inner.attrs().depth
    }

    #[getter]
    #[pyo3(name = "parent_fingerprint")]
    pub fn parent_fingerprint_as_hex_string_py(&self) -> String {
        self.inner.attrs().parent_fingerprint.to_vec().to_hex()
    }

    #[getter]
    #[pyo3(name = "child_number")]
    pub fn child_number_py(&self) -> u32 {
        self.inner.attrs().child_number.into()
    }

    #[getter]
    #[pyo3(name = "chain_code")]
    pub fn chain_code_as_hex_string_py(&self) -> String {
        self.inner.attrs().chain_code.to_vec().to_hex()
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
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            if let Some(xpub) = value.as_ref().as_string() {
                Ok(XPub::try_new(xpub.as_str())?)
            } else {
                Err(Error::InvalidXPub)
            }
        })
    }
}

pub struct NetworkTaggedXpub {
    pub xpub: ExtendedPublicKey<secp256k1::PublicKey>,
    pub network_id: NetworkId,
}
// impl NetworkTaggedXpub {

// }

impl fmt::Display for NetworkTaggedXpub {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let obj: XPub = self.xpub.clone().into();
        write!(f, "{}", obj.inner.to_string(Some(Prefix::from(self.network_id))))
    }
}

type TaggedXpub = (ExtendedPublicKey<secp256k1::PublicKey>, NetworkId);

impl From<TaggedXpub> for NetworkTaggedXpub {
    fn from(value: TaggedXpub) -> Self {
        Self { xpub: value.0, network_id: value.1 }
    }
}
