use crate::imports::*;
use crate::result::Result;
use crate::runtime::account::descriptor::*;
use crate::runtime::account::{Account, AccountKind, Inner};
use crate::runtime::Wallet;
use crate::storage::{AccountMetadata, PrvKeyDataId};
use kaspa_addresses::Version;
use secp256k1::PublicKey;

pub const KEYPAIR_ACCOUNT_VERSION: u32 = 0;
pub const KEYPAIR_ACCOUNT_KIND: &str = "kaspa-keypair-standard";

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "lowercase")]
pub struct Storable {
    pub version: u32,
    pub xpub_key: Arc<String>,
    pub ecdsa: bool,
}

impl Storable {
    pub fn new(public_key: PublicKey, ecdsa: bool) -> Self {
        Self { version: KEYPAIR_ACCOUNT_VERSION, xpub_key: Arc::new(public_key.to_string()), ecdsa }
    }

    pub fn try_load(storage: &AccountStorage) -> Result<Self> {
        let storable = serde_json::from_str::<Storable>(std::str::from_utf8(&storage.serialized)?)?;
        Ok(storable)
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
        public_key: PublicKey,
        prv_key_data_id: PrvKeyDataId,
        ecdsa: bool,
    ) -> Result<Self> {
        let storable = Storable::new(public_key, ecdsa);
        let settings = AccountSettings { name, ..Default::default() };

        let (id, storage_key) = make_account_hashes(from_keypair(&prv_key_data_id, &storable));
        let inner = Arc::new(Inner::new(wallet, id, storage_key, settings));

        let Storable { xpub_key, ecdsa, .. } = storable;
        Ok(Self { inner, prv_key_data_id, public_key: PublicKey::from_str(xpub_key.as_str())?, ecdsa })

        // let serialized = serde_json::to_string(&storable)?;
        // Ok(Self::try_load(wallet, KEYPAIR_ACCOUNT_VERSION, prv_key_data_id, settings, serialized.as_bytes(), None).await?)
    }

    pub async fn try_load(
        wallet: &Arc<Wallet>,
        storage: &AccountStorage,
        _meta: Option<Arc<AccountMetadata>>,
        // wallet: &Arc<Wallet>,
        // version : u32,
        // prv_key_data_id: PrvKeyDataId,
        // settings: AccountSettings,
        // serialized: &[u8],
        // _meta: Option<Arc<AccountMetadata>>,
    ) -> Result<Self> {
        let storable = Storable::try_load(storage)?;

        // let (id, storage_key) = make_account_hashes(from_keypair(&prv_key_data_id, &storable));
        // let inner = Arc::new(Inner::new(wallet, id, storage_key, settings));
        let inner = Arc::new(Inner::from_storage(wallet, storage));

        let Storable { xpub_key, ecdsa, .. } = storable;
        Ok(Self {
            inner,
            prv_key_data_id: storage.prv_key_data_ids.clone().try_into()?,
            public_key: PublicKey::from_str(xpub_key.as_str())?,
            ecdsa,
        })
    }
}

#[async_trait]
impl Account for Keypair {
    fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    fn account_kind(&self) -> AccountKind {
        AccountKind::Keypair
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
        let storable = Storable::new(self.public_key, self.ecdsa);
        let serialized = serde_json::to_string(&storable)?;
        let account_storage = AccountStorage::new(
            KEYPAIR_ACCOUNT_KIND,
            KEYPAIR_ACCOUNT_VERSION,
            self.id(),
            self.storage_key(),
            self.prv_key_data_id.into(),
            settings,
            serialized.as_bytes(),
        );

        Ok(account_storage)
    }

    // fn as_storable(&self) -> Result<storage::account::Account> {
    //     let settings = self.context().settings.clone().unwrap_or_default();
    //     let keypair = storage::Keypair::new(self.public_key, self.ecdsa);
    //     let account = AccountStorage::new(*self.id(), Some(self.prv_key_data_id), settings, storage::AccountData::Keypair(keypair));
    //     Ok(account)
    // }

    fn metadata(&self) -> Result<Option<AccountMetadata>> {
        Ok(None)
    }

    fn descriptor(&self) -> Result<AccountDescriptor> {
        let descriptor = AccountDescriptor::new(
            KEYPAIR_ACCOUNT_KIND,
            *self.id(),
            self.name(),
            self.prv_key_data_id.into(),
            self.receive_address().ok(),
            self.change_address().ok(),
        )
        // .with_property(AccountDescriptorProperty::AccountIndex, self.account_index.into())
        // .with_property(AccountDescriptorProperty::XpubKeys, self.xpub_keys.into())
        .with_property(AccountDescriptorProperty::Ecdsa, self.ecdsa.into())
        // .with_property(AccountDescriptorProperty::DerivationMeta, self.derivation.address_derivation_meta().into())
        ;

        Ok(descriptor)

        // let descriptor = AccountDescriptor {
        //     account_id: *self.id(),
        //     account_name: self.name(),
        //     prv_key_data_id: self.prv_key_data_id.into(),
        //     xpub_keys: Arc::new(vec![self.public_key.to_string()]),
        //     ecdsa: Some(self.ecdsa),
        //     receive_address: self.receive_address().ok(),
        //     change_address: self.receive_address().ok(),
        // };

        // Ok(descriptor.into())
    }
}
