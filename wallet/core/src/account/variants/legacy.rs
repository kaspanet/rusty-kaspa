//!
//! Legacy (KDX, kaspanet.io Web Wallet) account implementation
//!

use crate::account::{AsLegacyAccount, Inner};
use crate::derivation::{AddressDerivationManager, AddressDerivationManagerTrait};
use crate::imports::*;
use kaspa_bip32::{ExtendedPrivateKey, Prefix, SecretKey};

const CACHE_ADDRESS_OFFSET: u32 = 2048;

pub const LEGACY_ACCOUNT_KIND: &str = "kaspa-legacy-standard";

pub struct Ctor {}

#[async_trait]
impl Factory for Ctor {
    fn name(&self) -> String {
        "bip32/legacy".to_string()
    }

    fn description(&self) -> String {
        "Kaspa Legacy Account (KDX, kaspanet.io Web Wallet)".to_string()
    }

    async fn try_load(
        &self,
        wallet: &Arc<Wallet>,
        storage: &AccountStorage,
        meta: Option<Arc<AccountMetadata>>,
    ) -> Result<Arc<dyn Account>> {
        Ok(Arc::new(Legacy::try_load(wallet, storage, meta).await?))
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Payload;

impl Payload {
    pub fn try_load(storage: &AccountStorage) -> Result<Self> {
        Ok(Self::try_from_slice(storage.serialized.as_slice())?)
    }
}

impl Storable for Payload {
    const STORAGE_MAGIC: u32 = 0x5943474c;
    const STORAGE_VERSION: u32 = 0;
}

impl AccountStorable for Payload {}

impl BorshSerialize for Payload {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        StorageHeader::new(Self::STORAGE_MAGIC, Self::STORAGE_VERSION).serialize(writer)?;

        Ok(())
    }
}

impl BorshDeserialize for Payload {
    fn deserialize(buf: &mut &[u8]) -> IoResult<Self> {
        let StorageHeader { version: _, .. } =
            StorageHeader::deserialize(buf)?.try_magic(Self::STORAGE_MAGIC)?.try_version(Self::STORAGE_VERSION)?;

        Ok(Self {})
    }
}

pub struct Legacy {
    inner: Arc<Inner>,
    prv_key_data_id: PrvKeyDataId,
    derivation: Arc<AddressDerivationManager>,
}

impl Legacy {
    pub async fn try_new(wallet: &Arc<Wallet>, name: Option<String>, prv_key_data_id: PrvKeyDataId) -> Result<Self> {
        let storable = Payload;
        let settings = AccountSettings { name, ..Default::default() };

        let (id, storage_key) = make_account_hashes(from_legacy(&prv_key_data_id, &storable));
        let inner = Arc::new(Inner::new(wallet, id, storage_key, settings));

        let account_index = 0;
        let derivation = AddressDerivationManager::create_legacy_pubkey_managers(wallet, account_index, Default::default())?;

        Ok(Self { inner, prv_key_data_id, derivation })
    }

    pub async fn try_load(wallet: &Arc<Wallet>, storage: &AccountStorage, meta: Option<Arc<AccountMetadata>>) -> Result<Self> {
        let prv_key_data_id: PrvKeyDataId = storage.prv_key_data_ids.clone().try_into()?;

        let inner = Arc::new(Inner::from_storage(wallet, storage));

        let address_derivation_indexes = meta.and_then(|meta| meta.address_derivation_indexes()).unwrap_or_default();
        let account_index = 0;
        let derivation =
            AddressDerivationManager::create_legacy_pubkey_managers(wallet, account_index, address_derivation_indexes.clone())?;

        Ok(Self { inner, prv_key_data_id, derivation })
    }

    pub async fn initialize_derivation(
        &self,
        wallet_secret: &Secret,
        payment_secret: Option<&Secret>,
        index: Option<u32>,
    ) -> Result<()> {
        let prv_key_data = self
            .inner
            .wallet
            .get_prv_key_data(wallet_secret, &self.prv_key_data_id)
            .await?
            .ok_or(Error::Custom(format!("Prv key data is missing for {}", self.prv_key_data_id.to_hex())))?;
        let mnemonic = prv_key_data
            .as_mnemonic(payment_secret)?
            .ok_or(Error::Custom(format!("Could not convert Prv key data into mnemonic for {}", self.prv_key_data_id.to_hex())))?;

        let seed = mnemonic.to_seed("");
        let xprv = ExtendedPrivateKey::<SecretKey>::new(seed).unwrap();
        let xprv = xprv.to_string(Prefix::XPRV).to_string();

        for derivator in &self.derivation.derivators {
            derivator.initialize(xprv.clone(), index)?;
        }

        let m = self.derivation.receive_address_manager();
        m.get_range(0..(m.index() + CACHE_ADDRESS_OFFSET))?;
        let m = self.derivation.change_address_manager();
        m.get_range(0..(m.index() + CACHE_ADDRESS_OFFSET))?;

        Ok(())
    }
}

#[async_trait]
impl Account for Legacy {
    fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    fn account_kind(&self) -> AccountKind {
        LEGACY_ACCOUNT_KIND.into()
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
        self.derivation.receive_address_manager().current_address()
    }

    fn change_address(&self) -> Result<Address> {
        self.derivation.change_address_manager().current_address()
    }

    fn to_storage(&self) -> Result<AccountStorage> {
        let settings = self.context().settings.clone();
        let storable = Payload;
        let account_storage = AccountStorage::try_new(
            LEGACY_ACCOUNT_KIND.into(),
            self.id(),
            self.storage_key(),
            self.prv_key_data_id.into(),
            settings,
            storable,
        )?;

        Ok(account_storage)
    }

    fn metadata(&self) -> Result<Option<AccountMetadata>> {
        let metadata = AccountMetadata::new(self.inner.id, self.derivation.address_derivation_meta());
        Ok(Some(metadata))
    }

    fn descriptor(&self) -> Result<AccountDescriptor> {
        let descriptor = AccountDescriptor::new(
            LEGACY_ACCOUNT_KIND.into(),
            *self.id(),
            self.name(),
            self.prv_key_data_id.into(),
            self.receive_address().ok(),
            self.change_address().ok(),
        )
        .with_property(AccountDescriptorProperty::DerivationMeta, self.derivation.address_derivation_meta().into());

        Ok(descriptor)
    }

    fn as_derivation_capable(self: Arc<Self>) -> Result<Arc<dyn DerivationCapableAccount>> {
        Ok(self.clone())
    }

    fn as_legacy_account(self: Arc<Self>) -> Result<Arc<dyn AsLegacyAccount>> {
        Ok(self.clone())
    }
}

#[async_trait]
impl AsLegacyAccount for Legacy {
    async fn create_private_context(&self, wallet_secret: &Secret, payment_secret: Option<&Secret>, index: Option<u32>) -> Result<()> {
        self.initialize_derivation(wallet_secret, payment_secret, index).await?;
        Ok(())
    }

    async fn clear_private_context(&self) -> Result<()> {
        for derivator in &self.derivation.derivators {
            derivator.uninitialize()?;
        }
        Ok(())
    }
}

impl DerivationCapableAccount for Legacy {
    fn derivation(&self) -> Arc<dyn AddressDerivationManagerTrait> {
        self.derivation.clone()
    }

    // legacy accounts do not support bip44
    fn account_index(&self) -> u64 {
        0
    }
}
