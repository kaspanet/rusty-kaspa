//!
//! Secp256k1 keypair account implementation
//!

use crate::account::Inner;
use crate::imports::*;
use kaspa_addresses::Version;
use secp256k1::PublicKey;

pub const KEYPAIR_ACCOUNT_KIND: &str = "kaspa-keypair-standard";

pub struct Ctor {}

#[async_trait]
impl Factory for Ctor {
    fn name(&self) -> String {
        "Keypair".to_string()
    }

    fn description(&self) -> String {
        "Secp265k1 Keypair Account".to_string()
    }

    async fn try_load(
        &self,
        wallet: &Arc<Wallet>,
        storage: &AccountStorage,
        meta: Option<Arc<AccountMetadata>>,
    ) -> Result<Arc<dyn Account>> {
        Ok(Arc::new(Keypair::try_load(wallet, storage, meta).await?))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Payload {
    pub public_key: secp256k1::PublicKey,
    pub ecdsa: bool,
}

impl Payload {
    pub fn new(public_key: secp256k1::PublicKey, ecdsa: bool) -> Self {
        Self { public_key, ecdsa }
    }

    pub fn try_load(storage: &AccountStorage) -> Result<Self> {
        Ok(Self::try_from_slice(storage.serialized.as_slice())?)
    }
}

impl Storable for Payload {
    const STORAGE_MAGIC: u32 = 0x52494150;
    const STORAGE_VERSION: u32 = 0;
}

impl AccountStorable for Payload {}

impl BorshSerialize for Payload {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let public_key = self.public_key.serialize();

        StorageHeader::new(Self::STORAGE_MAGIC, Self::STORAGE_VERSION).serialize(writer)?;

        BorshSerialize::serialize(public_key.as_slice(), writer)?;
        BorshSerialize::serialize(&self.ecdsa, writer)?;

        Ok(())
    }
}

impl BorshDeserialize for Payload {
    fn deserialize(buf: &mut &[u8]) -> IoResult<Self> {
        use secp256k1::constants::PUBLIC_KEY_SIZE;

        let StorageHeader { version: _, .. } =
            StorageHeader::deserialize(buf)?.try_magic(Self::STORAGE_MAGIC)?.try_version(Self::STORAGE_VERSION)?;

        let public_key_bytes: [u8; PUBLIC_KEY_SIZE] = buf[..PUBLIC_KEY_SIZE]
            .try_into()
            .map_err(|_| IoError::new(IoErrorKind::Other, "Unable to deserialize keypair account (public_key buffer try_into)"))?;
        let public_key = secp256k1::PublicKey::from_slice(&public_key_bytes)
            .map_err(|_| IoError::new(IoErrorKind::Other, "Unable to deserialize keypair account (invalid public key)"))?;
        *buf = &buf[PUBLIC_KEY_SIZE..];
        let ecdsa = BorshDeserialize::deserialize(buf)?;

        Ok(Self { public_key, ecdsa })
    }
}

pub struct Keypair {
    inner: Arc<Inner>,
    prv_key_data_id: PrvKeyDataId,
    public_key: PublicKey,
    ecdsa: bool,
}

impl Keypair {
    pub async fn try_new(
        wallet: &Arc<Wallet>,
        name: Option<String>,
        public_key: secp256k1::PublicKey,
        prv_key_data_id: PrvKeyDataId,
        ecdsa: bool,
    ) -> Result<Self> {
        let storable = Payload::new(public_key, ecdsa);
        let settings = AccountSettings { name, ..Default::default() };

        let (id, storage_key) = make_account_hashes(from_keypair(&prv_key_data_id, &storable));
        let inner = Arc::new(Inner::new(wallet, id, storage_key, settings));

        let Payload { public_key, ecdsa, .. } = storable;
        Ok(Self { inner, prv_key_data_id, public_key, ecdsa })
    }

    pub async fn try_load(wallet: &Arc<Wallet>, storage: &AccountStorage, _meta: Option<Arc<AccountMetadata>>) -> Result<Self> {
        let storable = Payload::try_load(storage)?;
        let inner = Arc::new(Inner::from_storage(wallet, storage));

        let Payload { public_key, ecdsa, .. } = storable;
        Ok(Self { inner, prv_key_data_id: storage.prv_key_data_ids.clone().try_into()?, public_key, ecdsa })
    }
}

#[async_trait]
impl Account for Keypair {
    fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    fn account_kind(&self) -> AccountKind {
        KEYPAIR_ACCOUNT_KIND.into()
    }

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        Ok(&self.prv_key_data_id)
    }

    fn as_dyn_arc(self: Arc<Self>) -> Arc<dyn Account> {
        self
    }

    fn sig_op_count(&self) -> u8 {
        1
    }

    fn minimum_signatures(&self) -> u16 {
        1
    }

    fn receive_address(&self) -> Result<Address> {
        let (xonly_public_key, _) = self.public_key.x_only_public_key();
        Ok(Address::new(self.inner().wallet.network_id()?.into(), Version::PubKey, &xonly_public_key.serialize()))
    }

    fn change_address(&self) -> Result<Address> {
        let (xonly_public_key, _) = self.public_key.x_only_public_key();
        Ok(Address::new(self.inner().wallet.network_id()?.into(), Version::PubKey, &xonly_public_key.serialize()))
    }

    fn to_storage(&self) -> Result<AccountStorage> {
        let settings = self.context().settings.clone();
        let storable = Payload::new(self.public_key, self.ecdsa);
        let account_storage = AccountStorage::try_new(
            KEYPAIR_ACCOUNT_KIND.into(),
            self.id(),
            self.storage_key(),
            self.prv_key_data_id.into(),
            settings,
            storable,
        )?;

        Ok(account_storage)
    }

    fn metadata(&self) -> Result<Option<AccountMetadata>> {
        Ok(None)
    }

    fn descriptor(&self) -> Result<AccountDescriptor> {
        let descriptor = AccountDescriptor::new(
            KEYPAIR_ACCOUNT_KIND.into(),
            *self.id(),
            self.name(),
            self.prv_key_data_id.into(),
            self.receive_address().ok(),
            self.change_address().ok(),
        )
        .with_property(AccountDescriptorProperty::Ecdsa, self.ecdsa.into());

        Ok(descriptor)
    }
}
