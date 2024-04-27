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
use secp256k1::{Secp256k1, XOnlyPublicKey};
use serde_wasm_bindgen::to_value;

/// Data structure that contains a secret and public keys.
/// @category Wallet SDK
#[derive(Debug, Clone, CastFromJs)]
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
    pub fn get_public_key(&self) -> String {
        PublicKey::from(&self.public_key).to_string()
    }

    /// Get the [`PrivateKey`] of this [`Keypair`].
    #[wasm_bindgen(getter = privateKey)]
    pub fn get_private_key(&self) -> String {
        PrivateKey::from(&self.secret_key).to_hex()
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
    pub fn to_address(&self, network: &NetworkTypeT) -> Result<Address> {
        let payload = &self.xonly_public_key.serialize();
        let address = Address::new(network.try_into()?, AddressVersion::PubKey, payload);
        Ok(address)
    }

    /// Get `ECDSA` [`Address`] of this Keypair's [`PublicKey`].
    /// Receives a [`NetworkType`] to determine the prefix of the address.
    /// JavaScript: `let address = keypair.toAddress(NetworkType.MAINNET);`.
    #[wasm_bindgen(js_name = toAddressECDSA)]
    pub fn to_address_ecdsa(&self, network: &NetworkTypeT) -> Result<Address> {
        let payload = &self.public_key.serialize();
        let address = Address::new(network.try_into()?, AddressVersion::PubKeyECDSA, payload);
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

impl TryCastFromJs for Keypair {
    type Error = Error;
    fn try_cast_from(value: impl AsRef<JsValue>) -> Result<Cast<Self>, Self::Error> {
        Ok(Self::try_ref_from_js_value_as_cast(value)?)
    }
}
