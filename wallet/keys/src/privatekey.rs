//!
//! Private Key
//!

use crate::imports::*;
use crate::keypair::Keypair;
use js_sys::{Array, Uint8Array};

/// Data structure that envelops a Private Key.
/// @category Wallet SDK
#[derive(Clone, Debug, CastFromJs)]
#[cfg_attr(feature = "py-sdk", pyclass)]
#[wasm_bindgen]
pub struct PrivateKey {
    inner: secp256k1::SecretKey,
}

impl PrivateKey {
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.inner.secret_bytes()
    }
}

impl From<&secp256k1::SecretKey> for PrivateKey {
    fn from(value: &secp256k1::SecretKey) -> Self {
        Self { inner: *value }
    }
}

impl From<&PrivateKey> for [u8; 32] {
    fn from(key: &PrivateKey) -> Self {
        key.secret_bytes()
    }
}

#[wasm_bindgen]
impl PrivateKey {
    /// Create a new [`PrivateKey`] from a hex-encoded string.
    #[wasm_bindgen(constructor)]
    pub fn try_new(key: &str) -> Result<PrivateKey> {
        Ok(Self { inner: secp256k1::SecretKey::from_str(key)? })
    }
}

impl PrivateKey {
    pub fn try_from_slice(data: &[u8]) -> Result<PrivateKey> {
        Ok(Self { inner: secp256k1::SecretKey::from_slice(data)? })
    }
}

// PY-NOTE: WASM specific fn implementations
#[wasm_bindgen]
impl PrivateKey {
    #[wasm_bindgen(js_name = toPublicKey)]
    pub fn to_public_key(&self) -> Result<PublicKey, JsError> {
        Ok(PublicKey::from(secp256k1::PublicKey::from_secret_key_global(&self.inner)))
    }

    /// Get the [`Address`] of the PublicKey generated from this PrivateKey.
    /// Receives a [`NetworkType`] to determine the prefix of the address.
    /// JavaScript: `let address = privateKey.toAddress(NetworkType.MAINNET);`.
    #[wasm_bindgen(js_name = toAddress)]
    pub fn to_address(&self, network: &NetworkTypeT) -> Result<Address> {
        let public_key = secp256k1::PublicKey::from_secret_key_global(&self.inner);
        let (x_only_public_key, _) = public_key.x_only_public_key();
        let payload = x_only_public_key.serialize();
        let address = Address::new(network.try_into()?, AddressVersion::PubKey, &payload);
        Ok(address)
    }

    /// Get `ECDSA` [`Address`] of the PublicKey generated from this PrivateKey.
    /// Receives a [`NetworkType`] to determine the prefix of the address.
    /// JavaScript: `let address = privateKey.toAddress(NetworkType.MAINNET);`.
    #[wasm_bindgen(js_name = toAddressECDSA)]
    pub fn to_address_ecdsa(&self, network: &NetworkTypeT) -> Result<Address> {
        let public_key = secp256k1::PublicKey::from_secret_key_global(&self.inner);
        let payload = public_key.serialize();
        let address = Address::new(network.try_into()?, AddressVersion::PubKeyECDSA, &payload);
        Ok(address)
    }

    /// Generate a [`Keypair`] from this [`PrivateKey`].
    #[wasm_bindgen(js_name = toKeypair)]
    pub fn to_keypair(&self) -> Result<Keypair, JsError> {
        Keypair::from_private_key(self)
    }
}

// PY-NOTE: fns exposed to both WASM and Python
#[cfg_attr(feature = "py-sdk", pymethods)]
#[wasm_bindgen]
impl PrivateKey {
    // PY-NOTE: want to use `#[pyo3(name = "to_string")]` for this fn, but cannot use #[pyo3()] in block where pymethods is applied via cfg_attr
    /// Returns the [`PrivateKey`] key encoded as a hex string.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_hex(&self) -> String {
        use kaspa_utils::hex::ToHex;
        self.secret_bytes().to_vec().to_hex()
    }
}

// PY-NOTE: Python specific fn implementations
#[cfg(feature = "py-sdk")]
#[pymethods]
impl PrivateKey {
    #[new]
    pub fn try_new_py(key: &str) -> Result<PrivateKey> {
        Ok(Self { inner: secp256k1::SecretKey::from_str(key)? })
    }

    // PY-NOTE: #[pyo3()] can only be used in block that has #[pymethods] applied directly. applying via #[cfg_attr()] does not work (PyO3 limitation).
    #[pyo3(name = "to_public_key")]
    pub fn to_public_key_py(&self) -> PyResult<PublicKey> {
        Ok(PublicKey::from(secp256k1::PublicKey::from_secret_key_global(&self.inner)))
    }

    // PY-NOTE: #[pyo3()] can only be used in block that has #[pymethods] applied directly. applying via #[cfg_attr()] does not work (PyO3 limitation).
    #[pyo3(name = "to_address")]
    pub fn to_address_py(&self, network: &str) -> PyResult<Address> {
        // PY-NOTE: arg type of `network: &str` instead of `network: NetworkTypeT`
        let public_key = secp256k1::PublicKey::from_secret_key_global(&self.inner);
        let (x_only_public_key, _) = public_key.x_only_public_key();
        let payload = x_only_public_key.serialize();
        let address = Address::new(network.try_into()?, AddressVersion::PubKey, &payload);
        Ok(address)
    }

    // PY-NOTE: #[pyo3()] can only be used in block that has #[pymethods] applied directly. applying via #[cfg_attr()] does not work (PyO3 limitation).
    #[pyo3(name = "to_address_ecdsa")]
    pub fn to_address_ecdsa_py(&self, network: &str) -> PyResult<Address> {
        // PY-NOTE: arg type of `network: &str` instead of `network: NetworkTypeT`
        let public_key = secp256k1::PublicKey::from_secret_key_global(&self.inner);
        let payload = public_key.serialize();
        let address = Address::new(network.try_into()?, AddressVersion::PubKeyECDSA, &payload);
        Ok(address)
    }

    // TODO
    // #[wasm_bindgen(js_name = toKeypair)]
    // pub fn to_keypair(&self) -> Result<Keypair, JsError> {
    //     Keypair::from_private_key(self)
    // }
}

impl TryCastFromJs for PrivateKey {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            if let Some(hex_str) = value.as_ref().as_string() {
                Self::try_new(hex_str.as_str())
            } else if Array::is_array(value.as_ref()) {
                let array = Uint8Array::new(value.as_ref());
                Self::try_from_slice(array.to_vec().as_slice())
            } else {
                Err(Error::InvalidPrivateKey)
            }
        })
    }
}
