//!
//! [`keypair`](mod@self) module encapsulates [`Keypair`] and [`PrivateKey`].
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

use crate::imports::*;
use secp256k1::XOnlyPublicKey;
use workflow_wasm::abi::*;

/// Data structure that envelopes a PublicKey.
/// Only supports Schnorr-based addresses.
/// @category Wallet SDK
#[derive(Clone, Debug)]
#[wasm_bindgen(js_name = PublicKey)]
pub struct PublicKey {
    #[wasm_bindgen(skip)]
    pub xonly_public_key: XOnlyPublicKey,
    #[wasm_bindgen(skip)]
    pub source: String,
}

#[wasm_bindgen(js_class = PublicKey)]
impl PublicKey {
    /// Create a new [`PublicKey`] from a hex-encoded string.
    #[wasm_bindgen(constructor)]
    pub fn try_new(key: &str) -> Result<PublicKey> {
        match secp256k1::PublicKey::from_str(key) {
            Ok(public_key) => Ok((&public_key).into()),
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

impl From<&secp256k1::PublicKey> for PublicKey {
    fn from(value: &secp256k1::PublicKey) -> Self {
        let (xonly_public_key, _) = value.x_only_public_key();
        Self { xonly_public_key, source: value.to_string() }
    }
}
