//!
//! Private key storage and encryption.
//!

use crate::derivation::create_xpub_from_xprv;
use crate::imports::*;
use kaspa_bip32::{ExtendedPrivateKey, ExtendedPublicKey, Language, Mnemonic};
use kaspa_utils::hex::ToHex;
use secp256k1::SecretKey;
use xxhash_rust::xxh3::xxh3_64;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum PrvKeyDataVariantKind {
    Mnemonic,
    Bip39Seed,
    ExtendedPrivateKey,
    SecretKey,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "key-variant", content = "key-data")]
pub enum PrvKeyDataVariant {
    // 12 or 24 word bip39 mnemonic
    Mnemonic(String),
    // Bip39 seed (generated from mnemonic)
    Bip39Seed(String),
    // Extended Private Key (XPrv)
    ExtendedPrivateKey(String),
    // secp256k1::SecretKey
    SecretKey(String),
}

impl BorshSerialize for PrvKeyDataVariant {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        StorageHeader::new(Self::MAGIC, Self::VERSION).serialize(writer)?;
        let kind = self.kind();
        let string = self.get_string();
        BorshSerialize::serialize(&kind, writer)?;
        BorshSerialize::serialize(string.as_str(), writer)?;

        Ok(())
    }
}

impl BorshDeserialize for PrvKeyDataVariant {
    fn deserialize(buf: &mut &[u8]) -> IoResult<Self> {
        let StorageHeader { version: _, .. } = StorageHeader::deserialize(buf)?.try_magic(Self::MAGIC)?.try_version(Self::VERSION)?;

        let kind: PrvKeyDataVariantKind = BorshDeserialize::deserialize(buf)?;
        let string: String = BorshDeserialize::deserialize(buf)?;

        match kind {
            PrvKeyDataVariantKind::Mnemonic => Ok(Self::Mnemonic(string)),
            PrvKeyDataVariantKind::Bip39Seed => Ok(Self::Bip39Seed(string)),
            PrvKeyDataVariantKind::ExtendedPrivateKey => Ok(Self::ExtendedPrivateKey(string)),
            PrvKeyDataVariantKind::SecretKey => Ok(Self::SecretKey(string)),
        }
    }
}

impl PrvKeyDataVariant {
    const MAGIC: u32 = 0x5652504b;
    const VERSION: u32 = 0;

    pub fn kind(&self) -> PrvKeyDataVariantKind {
        match self {
            PrvKeyDataVariant::Mnemonic(_) => PrvKeyDataVariantKind::Mnemonic,
            PrvKeyDataVariant::Bip39Seed(_) => PrvKeyDataVariantKind::Bip39Seed,
            PrvKeyDataVariant::ExtendedPrivateKey(_) => PrvKeyDataVariantKind::ExtendedPrivateKey,
            PrvKeyDataVariant::SecretKey(_) => PrvKeyDataVariantKind::SecretKey,
        }
    }

    pub fn from_mnemonic(mnemonic: Mnemonic) -> Self {
        PrvKeyDataVariant::Mnemonic(mnemonic.phrase_string())
    }

    pub fn from_secret_key(secret_key: SecretKey) -> Self {
        PrvKeyDataVariant::SecretKey(secret_key.secret_bytes().to_vec().to_hex())
    }

    pub fn get_string(&self) -> Zeroizing<String> {
        match self {
            PrvKeyDataVariant::Mnemonic(s) => Zeroizing::new(s.clone()),
            PrvKeyDataVariant::Bip39Seed(s) => Zeroizing::new(s.clone()),
            PrvKeyDataVariant::ExtendedPrivateKey(s) => Zeroizing::new(s.clone()),
            PrvKeyDataVariant::SecretKey(s) => Zeroizing::new(s.clone()),
        }
    }

    pub fn id(&self) -> PrvKeyDataId {
        let s = PrvKeyDataVariant::get_string(self); //self.get_string();
        PrvKeyDataId::new(xxh3_64(s.as_bytes()))
    }
}

impl Zeroize for PrvKeyDataVariant {
    fn zeroize(&mut self) {
        match self {
            PrvKeyDataVariant::Mnemonic(s) => s.zeroize(),
            PrvKeyDataVariant::Bip39Seed(s) => s.zeroize(),
            PrvKeyDataVariant::ExtendedPrivateKey(s) => s.zeroize(),
            PrvKeyDataVariant::SecretKey(s) => s.zeroize(),
        }
    }
}
impl Drop for PrvKeyDataVariant {
    fn drop(&mut self) {
        self.zeroize()
    }
}

impl ZeroizeOnDrop for PrvKeyDataVariant {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyDataPayload {
    prv_key_variant: PrvKeyDataVariant,
}

impl PrvKeyDataPayload {
    pub fn try_new_with_mnemonic(mnemonic: Mnemonic) -> Result<Self> {
        Ok(Self { prv_key_variant: PrvKeyDataVariant::from_mnemonic(mnemonic) })
    }

    pub fn try_new_with_secret_key(secret_key: SecretKey) -> Result<Self> {
        Ok(Self { prv_key_variant: PrvKeyDataVariant::from_secret_key(secret_key) })
    }

    pub fn get_xprv(&self, payment_secret: Option<&Secret>) -> Result<ExtendedPrivateKey<SecretKey>> {
        let payment_secret = payment_secret.map(|s| std::str::from_utf8(s.as_ref())).transpose()?;

        match &self.prv_key_variant {
            PrvKeyDataVariant::Mnemonic(mnemonic) => {
                let mnemonic = Mnemonic::new(mnemonic, Language::English)?;
                let xkey = ExtendedPrivateKey::<SecretKey>::new(mnemonic.to_seed(payment_secret.unwrap_or_default()))?;
                Ok(xkey)
            }
            PrvKeyDataVariant::Bip39Seed(seed) => {
                let seed = Zeroizing::new(Vec::from_hex(seed.as_ref())?);
                let xkey = ExtendedPrivateKey::<SecretKey>::new(seed)?;
                Ok(xkey)
            }
            PrvKeyDataVariant::ExtendedPrivateKey(extended_private_key) => {
                let xkey: ExtendedPrivateKey<SecretKey> = extended_private_key.parse()?;
                Ok(xkey)
            }
            PrvKeyDataVariant::SecretKey(_) => Err(Error::XPrvSupport),
        }
    }

    pub fn as_mnemonic(&self) -> Result<Option<Mnemonic>> {
        match &self.prv_key_variant {
            PrvKeyDataVariant::Mnemonic(mnemonic) => Ok(Some(Mnemonic::new(mnemonic.clone(), Language::English)?)),
            _ => Ok(None),
        }
    }

    pub fn as_variant(&self) -> Zeroizing<PrvKeyDataVariant> {
        Zeroizing::new(self.prv_key_variant.clone())
    }

    pub fn as_secret_key(&self) -> Result<Option<SecretKey>> {
        match &self.prv_key_variant {
            PrvKeyDataVariant::SecretKey(private_key) => Ok(Some(SecretKey::from_str(private_key)?)),
            _ => Ok(None),
        }
    }

    pub fn id(&self) -> PrvKeyDataId {
        self.prv_key_variant.id()
    }
}

impl Zeroize for PrvKeyDataPayload {
    fn zeroize(&mut self) {
        self.prv_key_variant.zeroize();
    }
}

impl Drop for PrvKeyDataPayload {
    fn drop(&mut self) {
        self.zeroize()
    }
}

impl ZeroizeOnDrop for PrvKeyDataPayload {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyData {
    pub id: PrvKeyDataId,
    pub name: Option<String>,
    pub payload: Encryptable<PrvKeyDataPayload>,
}

impl PrvKeyData {
    pub async fn create_xpub(
        &self,
        payment_secret: Option<&Secret>,
        account_kind: AccountKind,
        account_index: u64,
    ) -> Result<ExtendedPublicKey<secp256k1::PublicKey>> {
        let payload = self.payload.decrypt(payment_secret)?;
        let xprv = payload.get_xprv(payment_secret)?;
        create_xpub_from_xprv(xprv, account_kind, account_index).await
    }

    pub fn get_xprv(&self, payment_secret: Option<&Secret>) -> Result<ExtendedPrivateKey<secp256k1::SecretKey>> {
        let payload = self.payload.decrypt(payment_secret)?;
        payload.get_xprv(payment_secret)
    }

    pub fn as_mnemonic(&self, payment_secret: Option<&Secret>) -> Result<Option<Mnemonic>> {
        let payload = self.payload.decrypt(payment_secret)?;
        payload.as_mnemonic()
    }

    pub fn as_variant(&self, payment_secret: Option<&Secret>) -> Result<Zeroizing<PrvKeyDataVariant>> {
        let payload = self.payload.decrypt(payment_secret)?;
        Ok(payload.as_variant())
    }

    pub fn try_from_mnemonic(mnemonic: Mnemonic, payment_secret: Option<&Secret>, encryption_kind: EncryptionKind) -> Result<Self> {
        let key_data_payload = PrvKeyDataPayload::try_new_with_mnemonic(mnemonic)?;
        let key_data_payload_id = key_data_payload.id();
        let key_data_payload = Encryptable::Plain(key_data_payload);

        let mut prv_key_data = PrvKeyData::new(key_data_payload_id, None, key_data_payload);
        if let Some(payment_secret) = payment_secret {
            prv_key_data.encrypt(payment_secret, encryption_kind)?;
        }

        Ok(prv_key_data)
    }
}

impl AsRef<PrvKeyData> for PrvKeyData {
    fn as_ref(&self) -> &PrvKeyData {
        self
    }
}

impl Zeroize for PrvKeyData {
    fn zeroize(&mut self) {
        self.id.zeroize();
        self.name.zeroize();
        self.payload.zeroize();
    }
}

impl Drop for PrvKeyData {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl PrvKeyData {
    pub fn new(id: PrvKeyDataId, name: Option<String>, payload: Encryptable<PrvKeyDataPayload>) -> Self {
        Self { id, payload, name }
    }

    pub fn try_new_from_mnemonic(
        mnemonic: Mnemonic,
        payment_secret: Option<&Secret>,
        encryption_kind: EncryptionKind,
    ) -> Result<Self> {
        let payload = PrvKeyDataPayload::try_new_with_mnemonic(mnemonic)?;
        let mut prv_key_data = Self { id: payload.id(), payload: Encryptable::Plain(payload), name: None };
        if let Some(payment_secret) = payment_secret {
            prv_key_data.encrypt(payment_secret, encryption_kind)?;
        }

        Ok(prv_key_data)
    }

    pub fn try_new_from_secret_key(
        secret_key: SecretKey,
        payment_secret: Option<&Secret>,
        encryption_kind: EncryptionKind,
    ) -> Result<Self> {
        let payload = PrvKeyDataPayload::try_new_with_secret_key(secret_key)?;
        let mut prv_key_data = Self { id: payload.id(), payload: Encryptable::Plain(payload), name: None };
        if let Some(payment_secret) = payment_secret {
            prv_key_data.encrypt(payment_secret, encryption_kind)?;
        }

        Ok(prv_key_data)
    }

    pub fn encrypt(&mut self, secret: &Secret, encryption_kind: EncryptionKind) -> Result<()> {
        self.payload = self.payload.into_encrypted(secret, encryption_kind)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test_storage_prv_key_data() -> Result<()> {
        let storable_in = PrvKeyDataVariant::Bip39Seed("lorem ipsum".to_string());
        let guard = StorageGuard::new(&storable_in);
        let storable_out = guard.validate()?;

        match &storable_out {
            PrvKeyDataVariant::Bip39Seed(s) => assert_eq!(s, "lorem ipsum"),
            _ => unreachable!("invalid prv key variant storage data"),
        }

        Ok(())
    }
}
