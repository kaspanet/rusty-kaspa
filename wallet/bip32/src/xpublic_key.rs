//! Extended public keys
//!
use crate::{
    types::*, ChildNumber, DerivationPath, Error, ExtendedKey, ExtendedKeyAttrs, ExtendedPrivateKey, KeyFingerprint, Prefix,
    PrivateKey, PublicKey, PublicKeyBytes, Result, KEY_SIZE,
};
use borsh::{BorshDeserialize, BorshSerialize};
use core::str::FromStr;
use hmac::Mac;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

///// Extended public secp256k1 ECDSA verification key.
//#[cfg(feature = "secp256k1")]
//#[cfg_attr(docsrs, doc(cfg(feature = "secp256k1")))]
//pub type XPub = ExtendedPublicKey<k256::ecdsa::VerifyingKey>;

/// Extended public keys derived using BIP32.
///
/// Generic around a [`PublicKey`] type.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ExtendedPublicKey<K: PublicKey> {
    /// Derived public key
    pub public_key: K,

    /// Extended key attributes.
    pub attrs: ExtendedKeyAttrs,
}

#[allow(dead_code)]
impl<K> ExtendedPublicKey<K>
where
    K: PublicKey,
{
    /// Obtain the non-extended public key value `K`.
    pub fn public_key(&self) -> &K {
        &self.public_key
    }

    /// Get attributes for this key such as depth, parent fingerprint,
    /// child number, and chain code.
    pub fn attrs(&self) -> &ExtendedKeyAttrs {
        &self.attrs
    }

    /// Compute a 4-byte key fingerprint for this extended public key.
    pub fn fingerprint(&self) -> KeyFingerprint {
        self.public_key().fingerprint()
    }

    /// Derive a child key for a particular [`ChildNumber`].
    pub fn derive_child(&self, child_number: ChildNumber) -> Result<Self> {
        if child_number.is_hardened() {
            // Cannot derive child public keys for hardened `ChildNumber`s
            return Err(Error::ChildNumber);
        }

        let depth = self.attrs.depth.checked_add(1).ok_or(Error::Depth)?;

        let mut hmac = HmacSha512::new_from_slice(&self.attrs.chain_code).map_err(Error::Hmac)?;

        hmac.update(&self.public_key.to_bytes());
        hmac.update(&child_number.to_bytes());

        let result = hmac.finalize().into_bytes();
        let (child_key, chain_code) = result.split_at(KEY_SIZE);
        let public_key = self.public_key.derive_child(child_key.try_into()?)?;

        let attrs = ExtendedKeyAttrs {
            parent_fingerprint: self.public_key.fingerprint(),
            child_number,
            chain_code: chain_code.try_into()?,
            depth,
        };

        Ok(ExtendedPublicKey { public_key, attrs })
    }

    pub fn derive_path(self, path: &DerivationPath) -> Result<Self> {
        path.iter().try_fold(self, |key, child_num| key.derive_child(child_num))
    }

    /// Serialize the raw public key as a byte array (e.g. SEC1-encoded).
    pub fn to_bytes(&self) -> PublicKeyBytes {
        self.public_key.to_bytes()
    }

    /// Serialize this key as an [`ExtendedKey`].
    pub fn to_extended_key(&self, prefix: Prefix) -> ExtendedKey {
        ExtendedKey { prefix, attrs: self.attrs.clone(), key_bytes: self.to_bytes() }
    }

    pub fn to_string(&self, prefix: Option<Prefix>) -> String {
        let prefix = prefix.unwrap_or(Prefix::XPUB);
        self.to_extended_key(prefix).to_string()
    }

    pub fn from_public_key(public_key: K, attrs: &ExtendedKeyAttrs) -> Self {
        ExtendedPublicKey { public_key, attrs: attrs.clone() }
    }
}

impl<K> From<&ExtendedPrivateKey<K>> for ExtendedPublicKey<K::PublicKey>
where
    K: PrivateKey,
{
    fn from(xprv: &ExtendedPrivateKey<K>) -> ExtendedPublicKey<K::PublicKey> {
        ExtendedPublicKey { public_key: xprv.private_key().public_key(), attrs: xprv.attrs().clone() }
    }
}

impl<K> FromStr for ExtendedPublicKey<K>
where
    K: PublicKey,
{
    type Err = Error;

    fn from_str(xpub: &str) -> Result<Self> {
        ExtendedKey::from_str(xpub)?.try_into()
    }
}

impl<K> TryFrom<ExtendedKey> for ExtendedPublicKey<K>
where
    K: PublicKey,
{
    type Error = Error;

    fn try_from(extended_key: ExtendedKey) -> Result<ExtendedPublicKey<K>> {
        if extended_key.prefix.is_public() {
            Ok(ExtendedPublicKey { public_key: PublicKey::from_bytes(extended_key.key_bytes)?, attrs: extended_key.attrs.clone() })
        } else {
            Err(Error::Crypto(secp256k1::Error::InvalidPublicKey))
        }
    }
}

impl fmt::Display for ExtendedPublicKey<secp256k1::PublicKey> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_string(None).fmt(f)
    }
}

impl<K> ExtendedPublicKey<K>
where
    K: PublicKey,
{
    // a unique number used for binary
    // serialization data alignment check
    const STORAGE_MAGIC: u16 = 0x4b58;
    // binary serialization version
    const STORAGE_VERSION: u16 = 0;
}

#[derive(BorshSerialize, BorshDeserialize)]
struct Header {
    magic: u16,
    version: u16,
}

impl<K> BorshSerialize for ExtendedPublicKey<K>
where
    K: PublicKey,
{
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        Header { version: Self::STORAGE_VERSION, magic: Self::STORAGE_MAGIC }.serialize(writer)?;
        writer.write_all(self.public_key.to_bytes().as_slice())?;
        BorshSerialize::serialize(&self.attrs, writer)?;
        Ok(())
    }
}

impl<K> BorshDeserialize for ExtendedPublicKey<K>
where
    K: PublicKey,
{
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let Header { version, magic } = Header::deserialize_reader(reader)?;
        if magic != Self::STORAGE_MAGIC {
            return Err(std::io::Error::other("Invalid extended public key magic value"));
        }
        if version != Self::STORAGE_VERSION {
            return Err(std::io::Error::other("Invalid extended public key version"));
        }

        let mut public_key_bytes: [u8; KEY_SIZE + 1] = [0; KEY_SIZE + 1];
        reader.read_exact(&mut public_key_bytes)?;
        let public_key = K::from_bytes(public_key_bytes).map_err(|_| std::io::Error::other("Invalid extended public key"))?;
        let attrs = ExtendedKeyAttrs::deserialize_reader(reader)?;
        Ok(Self { public_key, attrs })
    }
}

impl<K> Serialize for ExtendedPublicKey<K>
where
    K: Serialize + PublicKey,
{
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string(None))
    }
}

struct ExtendedPublicKeyVisitor<'de, K>
where
    K: Deserialize<'de> + PublicKey,
{
    phantom: std::marker::PhantomData<&'de K>,
}

impl<'de, K> de::Visitor<'de> for ExtendedPublicKeyVisitor<'de, K>
where
    K: Deserialize<'de> + PublicKey,
{
    type Value = ExtendedPublicKey<K>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string containing network_type and optional suffix separated by a '-'")
    }

    fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        ExtendedPublicKey::<K>::from_str(value).map_err(|err| de::Error::custom(err.to_string()))
    }
}

impl<'de, K> Deserialize<'de> for ExtendedPublicKey<K>
where
    K: Deserialize<'de> + PublicKey + 'de,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<ExtendedPublicKey<K>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(ExtendedPublicKeyVisitor::<'de, K> { phantom: std::marker::PhantomData::<&'de K> })
    }
}
