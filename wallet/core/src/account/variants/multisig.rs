use crate::account::Inner;
use crate::derivation::{AddressDerivationManager, AddressDerivationManagerTrait};
use crate::imports::*;

pub const MULTISIG_ACCOUNT_VERSION: u32 = 0;
pub const MULTISIG_ACCOUNT_KIND: &str = "kaspa-multisig-standard";

pub struct Ctor {}

#[async_trait]
impl Factory for Ctor {
    async fn try_load(
        &self,
        wallet: &Arc<Wallet>,
        storage: &AccountStorage,
        meta: Option<Arc<AccountMetadata>>,
    ) -> Result<Arc<dyn Account>> {
        Ok(Arc::new(MultiSig::try_load(wallet, storage, meta).await?))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "lowercase")]
pub struct Storable {
    pub version: u32,
    pub xpub_keys: Arc<Vec<String>>,
    pub prv_key_data_ids: Option<Arc<Vec<PrvKeyDataId>>>,
    pub cosigner_index: Option<u8>,
    pub minimum_signatures: u16,
    pub ecdsa: bool,
}

impl Storable {
    pub fn new(
        xpub_keys: Arc<Vec<String>>,
        prv_key_data_ids: Option<Arc<Vec<PrvKeyDataId>>>,
        cosigner_index: Option<u8>,
        minimum_signatures: u16,
        ecdsa: bool,
    ) -> Self {
        Self { version: MULTISIG_ACCOUNT_VERSION, xpub_keys, prv_key_data_ids, cosigner_index, minimum_signatures, ecdsa }
    }

    pub fn try_load(storage: &AccountStorage) -> Result<Self> {
        let storable = serde_json::from_str::<Storable>(std::str::from_utf8(&storage.serialized)?)?;
        Ok(storable)
    }
}

pub struct MultiSig {
    inner: Arc<Inner>,
    xpub_keys: Arc<Vec<String>>,
    prv_key_data_ids: Option<Arc<Vec<PrvKeyDataId>>>,
    cosigner_index: Option<u8>,
    minimum_signatures: u16,
    ecdsa: bool,
    derivation: Arc<AddressDerivationManager>,
}

impl MultiSig {
    pub async fn try_new(
        wallet: &Arc<Wallet>,
        name: Option<String>,
        xpub_keys: Arc<Vec<String>>,
        prv_key_data_ids: Option<Arc<Vec<PrvKeyDataId>>>,
        cosigner_index: Option<u8>,
        minimum_signatures: u16,
        ecdsa: bool,
    ) -> Result<Self> {
        let storable = Storable::new(xpub_keys.clone(), prv_key_data_ids.clone(), cosigner_index, minimum_signatures, ecdsa);
        let settings = AccountSettings { name, ..Default::default() };
        let (id, storage_key) = make_account_hashes(from_multisig(&storable));
        let inner = Arc::new(Inner::new(wallet, id, storage_key, settings));

        // let address_derivation_indexes = meta.and_then(|meta| meta.address_derivation_indexes()).unwrap_or_default();

        let derivation = AddressDerivationManager::new(
            wallet,
            MULTISIG_ACCOUNT_KIND.into(),
            &xpub_keys,
            ecdsa,
            0,
            cosigner_index.map(|v| v as u32),
            minimum_signatures,
            Default::default(),
        )
        .await?;

        Ok(Self { inner, xpub_keys, cosigner_index, minimum_signatures, ecdsa, derivation, prv_key_data_ids })
    }

    pub async fn try_load(wallet: &Arc<Wallet>, storage: &AccountStorage, meta: Option<Arc<AccountMetadata>>) -> Result<Self> {
        let storable = Storable::try_load(storage)?;
        let inner = Arc::new(Inner::from_storage(wallet, storage));

        let Storable { xpub_keys, prv_key_data_ids, cosigner_index, minimum_signatures, ecdsa, .. } = storable;

        let address_derivation_indexes = meta.and_then(|meta| meta.address_derivation_indexes()).unwrap_or_default();

        let derivation = AddressDerivationManager::new(
            wallet,
            MULTISIG_ACCOUNT_KIND.into(),
            &xpub_keys,
            ecdsa,
            0,
            cosigner_index.map(|v| v as u32),
            minimum_signatures,
            address_derivation_indexes,
        )
        .await?;

        Ok(Self { inner, xpub_keys, cosigner_index, minimum_signatures, ecdsa, derivation, prv_key_data_ids })
    }

    pub fn prv_key_data_ids(&self) -> &Option<Arc<Vec<PrvKeyDataId>>> {
        &self.prv_key_data_ids
    }

    pub fn minimum_signatures(&self) -> u16 {
        self.minimum_signatures
    }

    pub fn xpub_keys(&self) -> &Arc<Vec<String>> {
        &self.xpub_keys
    }
}

#[async_trait]
impl Account for MultiSig {
    fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    fn account_kind(&self) -> AccountKind {
        MULTISIG_ACCOUNT_KIND.into()
    }

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        Err(Error::AccountKindFeature)
    }

    fn as_dyn_arc(self: Arc<Self>) -> Arc<dyn Account> {
        self
    }

    fn sig_op_count(&self) -> u8 {
        // TODO
        1
    }

    fn minimum_signatures(&self) -> u16 {
        self.minimum_signatures
    }

    fn receive_address(&self) -> Result<Address> {
        self.derivation.receive_address_manager().current_address()
    }

    fn change_address(&self) -> Result<Address> {
        self.derivation.change_address_manager().current_address()
    }

    fn to_storage(&self) -> Result<AccountStorage> {
        let settings = self.context().settings.clone();

        let storable = Storable::new(
            self.xpub_keys.clone(),
            self.prv_key_data_ids.clone(),
            self.cosigner_index,
            self.minimum_signatures,
            self.ecdsa,
        );

        let serialized = serde_json::to_string(&storable)?;

        let account_storage = AccountStorage::new(
            MULTISIG_ACCOUNT_KIND.into(),
            MULTISIG_ACCOUNT_VERSION,
            self.id(),
            self.storage_key(),
            self.prv_key_data_ids.clone().try_into()?,
            settings,
            serialized.as_bytes(),
        );

        Ok(account_storage)
    }

    fn metadata(&self) -> Result<Option<AccountMetadata>> {
        let metadata = AccountMetadata::new(self.inner.id, self.derivation.address_derivation_meta());
        Ok(Some(metadata))
    }

    fn descriptor(&self) -> Result<AccountDescriptor> {
        let descriptor = AccountDescriptor::new(
            MULTISIG_ACCOUNT_KIND.into(),
            *self.id(),
            self.name(),
            self.prv_key_data_ids.clone().try_into()?,
            self.receive_address().ok(),
            self.change_address().ok(),
        )
        .with_property(AccountDescriptorProperty::XpubKeys, self.xpub_keys.clone().into())
        .with_property(AccountDescriptorProperty::Ecdsa, self.ecdsa.into())
        .with_property(AccountDescriptorProperty::DerivationMeta, self.derivation.address_derivation_meta().into());

        Ok(descriptor)
    }

    fn as_derivation_capable(self: Arc<Self>) -> Result<Arc<dyn DerivationCapableAccount>> {
        Ok(self.clone())
    }
}

impl DerivationCapableAccount for MultiSig {
    fn derivation(&self) -> Arc<dyn AddressDerivationManagerTrait> {
        self.derivation.clone()
    }

    fn account_index(&self) -> u64 {
        0
    }
}
