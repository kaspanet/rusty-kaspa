//!
//! [`keypair`](mod@keypair) module encapsulates [`Keypair`] and [`PrivateKey`].
//! The [`Keypair`] provides access to the secret and public keys.
//!
//! ```javascript
//!
//! let keypair = Keypair.random();
//! let privateKey = keypair.privateKey;
//! let publicKey = keypair.publicKey;
//!
//! // to obtain an address from a keypair
//! let address = keypair.toAddress(NetworkType.Mainnnet);
//!
//! // to obtain a keypair from a private key
//! let keypair = privateKey.toKeypair();
//!
//! ```
//!

use crate::error::Error;
use crate::result::Result;
use js_sys::{Array, Uint8Array};
use kaspa_addresses::{Address, Version as AddressVersion};
use kaspa_consensus_core::network::wasm::Network;
use secp256k1::{Secp256k1, XOnlyPublicKey};
use serde_wasm_bindgen::to_value;
use std::str::FromStr;
use wasm_bindgen::prelude::*;
use workflow_wasm::abi::*;

/// Data structure that contains a secret and public keys.
#[derive(Debug, Clone)]
#[wasm_bindgen(inspectable)]
pub struct Keypair {
    secret_key: secp256k1::SecretKey,
    public_key: secp256k1::PublicKey,
    xonly_public_key: XOnlyPublicKey,
}

#[wasm_bindgen]
impl Keypair {
    fn new(secret_key: secp256k1::SecretKey, public_key: secp256k1::PublicKey, xonly_public_key: XOnlyPublicKey) -> Self {
        Self { secret_key, public_key, xonly_public_key }
    }

    /// Get the [`PublicKey`] of this [`Keypair`].
    #[wasm_bindgen(getter = publicKey)]
    pub fn get_public_key(&self) -> JsValue {
        to_value(&self.public_key).unwrap()
    }

    /// Get the [`PrivateKey`] of this [`Keypair`].
    #[wasm_bindgen(getter = privateKey)]
    pub fn get_private_key(&self) -> PrivateKey {
        (&self.secret_key).into()
    }

    /// Get the `XOnlyPublicKey` of this [`Keypair`].
    #[wasm_bindgen(getter = xOnlyPublicKey)]
    pub fn get_xonly_public_key(&self) -> JsValue {
        to_value(&self.xonly_public_key).unwrap()
    }

    /// Get the [`Address`] of this Keypair's [`PublicKey`].
    /// Receives a [`NetworkType`] to determine the prefix of the address.
    /// JavaScript: `let address = keypair.toAddress(NetworkType.MAINNET);`.
    #[wasm_bindgen(js_name = toAddress)]
    // pub fn to_address(&self, network_type: NetworkType) -> Result<Address> {
    pub fn to_address(&self, network: Network) -> Result<Address> {
        let pk = PublicKey { xonly_public_key: self.xonly_public_key, source: self.public_key.to_string() };
        let address = pk.to_address(network)?;
        Ok(address)
    }

    /// Get `ECDSA` [`Address`] of this Keypair's [`PublicKey`].
    /// Receives a [`NetworkType`] to determine the prefix of the address.
    /// JavaScript: `let address = keypair.toAddress(NetworkType.MAINNET);`.
    #[wasm_bindgen(js_name = toAddressECDSA)]
    pub fn to_address_ecdsa(&self, network: Network) -> Result<Address> {
        let pk = PublicKey { xonly_public_key: self.xonly_public_key, source: self.public_key.to_string() };
        let address = pk.to_address_ecdsa(network)?;
        Ok(address)
    }

    /// Create a new random [`Keypair`].
    /// JavaScript: `let keypair = Keypair::random();`.
    #[wasm_bindgen]
    pub fn random() -> Result<Keypair, JsError> {
        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let (xonly_public_key, _) = public_key.x_only_public_key();
        Ok(Keypair::new(secret_key, public_key, xonly_public_key))
    }

    /// Create a new [`Keypair`] from a [`PrivateKey`].
    /// JavaScript: `let privkey = new PrivateKey(hexString); let keypair = privkey.toKeypair();`.
    #[wasm_bindgen(js_name = "fromPrivateKey")]
    pub fn from_private_key(secret_key: &PrivateKey) -> Result<Keypair, JsError> {
        let secp = Secp256k1::new();
        let secret_key = secp256k1::SecretKey::from_slice(&secret_key.secret_bytes())?;
        let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
        let (xonly_public_key, _) = public_key.x_only_public_key();
        Ok(Keypair::new(secret_key, public_key, xonly_public_key))
    }
}

impl TryFrom<JsValue> for Keypair {
    type Error = Error;
    fn try_from(value: JsValue) -> std::result::Result<Self, Self::Error> {
        Ok(ref_from_abi!(Keypair, &value)?)
    }
}

/// Data structure that envelops a Private Key
#[derive(Clone, Debug)]
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

#[wasm_bindgen]
impl PrivateKey {
    /// Returns the [`PrivateKey`] key encoded as a hex string.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_hex(&self) -> String {
        use kaspa_utils::hex::ToHex;
        self.secret_bytes().to_vec().to_hex()
    }

    /// Generate a [`Keypair`] from this [`PrivateKey`].
    #[wasm_bindgen(js_name = toKeypair)]
    pub fn to_keypair(&self) -> Result<Keypair, JsError> {
        Keypair::from_private_key(self)
    }
}

impl TryFrom<JsValue> for PrivateKey {
    type Error = Error;
    fn try_from(js_value: JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(hex_str) = js_value.as_string() {
            Self::try_new(hex_str.as_str())
        } else if Array::is_array(&js_value) {
            let array = Uint8Array::new(&js_value);
            Self::try_from_slice(array.to_vec().as_slice())
        } else {
            Ok(ref_from_abi!(PrivateKey, &js_value)?)
        }
    }
}

// Data structure that envelopes a PublicKey
// Only supports Schnorr-based addresses
#[derive(Clone, Debug)]
#[wasm_bindgen(js_name = PublicKey)]
pub struct PublicKey {
    xonly_public_key: XOnlyPublicKey,
    source: String,
}

#[wasm_bindgen(js_class = PublicKey)]
impl PublicKey {
    /// Create a new [`PublicKey`] from a hex-encoded string.
    #[wasm_bindgen(constructor)]
    pub fn try_new(key: &str) -> Result<PublicKey> {
        match secp256k1::PublicKey::from_str(key) {
            Ok(public_key) => {
                let (xonly_public_key, _) = public_key.x_only_public_key();
                Ok(Self { xonly_public_key, source: (*key).to_string() })
            }
            Err(_e) => Ok(Self { xonly_public_key: XOnlyPublicKey::from_str(key)?, source: (*key).to_string() }),
        }
    }

    #[wasm_bindgen(js_name = "toString")]
    pub fn js_to_string(&self) -> String {
        self.source.clone()
    }

    /// Get the [`Address`] of this PublicKey.
    /// Receives a [`NetworkType`] to determine the prefix of the address.
    /// JavaScript: `let address = keypair.toAddress(NetworkType.MAINNET);`.
    #[wasm_bindgen(js_name = toAddress)]
    pub fn to_address(&self, network: Network) -> Result<Address> {
        let payload = &self.xonly_public_key.serialize();
        let address = Address::new(network.try_into()?, AddressVersion::PubKey, payload);
        Ok(address)
    }

    /// Get `ECDSA` [`Address`] of this PublicKey.
    /// Receives a [`NetworkType`] to determine the prefix of the address.
    /// JavaScript: `let address = keypair.toAddress(NetworkType.MAINNET);`.
    #[wasm_bindgen(js_name = toAddressECDSA)]
    pub fn to_address_ecdsa(&self, network: Network) -> Result<Address> {
        let payload = &self.xonly_public_key.serialize();
        let address = Address::new(network.try_into()?, AddressVersion::PubKeyECDSA, payload);
        Ok(address)
    }
}

impl std::fmt::Display for PublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.source)
    }
}

impl TryFrom<JsValue> for PublicKey {
    type Error = Error;
    fn try_from(js_value: JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(hex_str) = js_value.as_string() {
            Self::try_new(hex_str.as_str())
        } else {
            Ok(ref_from_abi!(PublicKey, &js_value)?)
        }
    }
}

impl From<PublicKey> for XOnlyPublicKey {
    fn from(value: PublicKey) -> Self {
        value.xonly_public_key
    }
}
