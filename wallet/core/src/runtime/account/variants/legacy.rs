use crate::derivation::AddressDerivationMeta;
use crate::imports::*;
use crate::result::Result;
use crate::runtime::account::{Account, AccountId, AccountKind, DerivationCapableAccount, Inner};
use crate::runtime::Wallet;
use crate::secret::Secret;
use crate::storage::{self, Metadata, PrvKeyDataId, Settings};
use crate::AddressDerivationManager;
use crate::AddressDerivationManagerTrait;
use kaspa_bip32::{ExtendedPrivateKey, Prefix, SecretKey};

pub struct Info {
    receive_address: Option<Address>,
    change_address: Option<Address>,
    derivation: Option<Arc<AddressDerivationManager>>,
    meta: AddressDerivationMeta,
}
pub struct Legacy {
    inner: Arc<Inner>,
    info: Arc<Mutex<Info>>,
    prv_key_data_id: PrvKeyDataId,
}

impl Legacy {
    pub async fn try_new(
        wallet: &Arc<Wallet>,
        prv_key_data_id: PrvKeyDataId,
        settings: Settings,
        data: storage::account::Legacy,
        meta: Option<Arc<Metadata>>,
    ) -> Result<Self> {
        let id = AccountId::from_legacy(&prv_key_data_id, &data);
        let inner = Arc::new(Inner::new(wallet, id, Some(settings)));

        //let storage::account::Legacy { xpub_keys } = data;

        let derivation_indexes = meta.and_then(|meta| meta.address_derivation_indexes()).unwrap_or(AddressDerivationMeta::new(0, 0));

        // let derivation =
        //     Self::create_derivation(wallet, prv_key_data_id, address_derivation_indexes).await?;

        // let derivation_meta = derivation.address_derivation_meta();
        // let receive_address = derivation.receive_address_manager.current_address()?;
        // let change_address = derivation.change_address_manager.current_address()?;

        Ok(Self {
            inner,
            prv_key_data_id,
            info: Arc::new(Mutex::new(Info {
                receive_address: None,
                change_address: None,
                meta: derivation_indexes,
                derivation: None,
            })),
        })
    }

    pub async fn initialize_derivation(
        &self,
        wallet_secret: Secret,
        payment_secret: Option<&Secret>,
        address_derivation_indexes: Option<AddressDerivationMeta>,
    ) -> Result<Arc<AddressDerivationManager>> {
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

        let keys = vec![xprv.to_string(Prefix::XPRV).to_string()];

        let meta = { self.info.lock()?.meta.clone() };

        let address_derivation_indexes = address_derivation_indexes.unwrap_or(meta);
        let derivation = AddressDerivationManager::new(
            &self.inner.wallet,
            AccountKind::Legacy,
            &keys,
            false,
            0,
            None,
            1,
            address_derivation_indexes,
        )
        .await?;

        let meta = derivation.address_derivation_meta();
        let receive_address = derivation.receive_address_manager.current_address()?;
        let change_address = derivation.change_address_manager.current_address()?;

        let mut info = self.info.lock()?;
        info.derivation = Some(derivation.clone());
        info.receive_address = Some(receive_address);
        info.change_address = Some(change_address);
        info.meta = meta;

        Ok(derivation)
    }

    fn info(&self) -> Result<MutexGuard<Info>> {
        Ok(self.info.lock()?)
    }
}

#[async_trait]
impl Account for Legacy {
    fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    fn account_kind(&self) -> AccountKind {
        AccountKind::Legacy
    }

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        Ok(&self.prv_key_data_id)
    }

    fn as_dyn_arc(self: Arc<Self>) -> Arc<dyn Account> {
        self
    }

    fn receive_address(&self) -> Result<Address> {
        self.info()?.receive_address.clone().ok_or(Error::Custom("Account initialization is pending.".into()))
        //Ok(self.receive_address.clone())
    }

    fn change_address(&self) -> Result<Address> {
        self.info()?.change_address.clone().ok_or(Error::Custom("Account initialization is pending.".into()))
        //Ok(self.change_address.clone())
    }

    fn as_storable(&self) -> Result<storage::account::Account> {
        let settings = self.context().settings.clone().unwrap_or_default();

        let legacy = storage::Legacy { xpub_keys: Arc::new(vec![]) };

        let account = storage::Account::new(*self.id(), Some(self.prv_key_data_id), settings, storage::AccountData::Legacy(legacy));

        Ok(account)
    }

    fn metadata(&self) -> Result<Option<Metadata>> {
        let derivation = self.info().unwrap().derivation.clone().expect("Account initialization is pending.");
        let metadata = Metadata::new(self.inner.id, derivation.address_derivation_meta());
        Ok(Some(metadata))
    }

    fn as_derivation_capable(self: Arc<Self>) -> Result<Arc<dyn DerivationCapableAccount>> {
        Ok(self.clone())
    }

    async fn initialize(
        self: Arc<Self>,
        secret: Secret,
        payment_secret: Option<&Secret>,
        address_derivation_indexes: Option<AddressDerivationMeta>,
    ) -> Result<()> {
        self.initialize_derivation(secret, payment_secret, address_derivation_indexes).await?;
        Ok(())
    }

    async fn uninitialize(self: Arc<Self>) -> Result<()> {
        let derivation = self.derivation();
        for manager in derivation.receive_address_manager().pubkey_managers.iter() {
            manager.uninitialize()?;
        }
        for manager in derivation.change_address_manager().pubkey_managers.iter() {
            manager.uninitialize()?;
        }
        Ok(())
    }
}

impl DerivationCapableAccount for Legacy {
    fn derivation(&self) -> Arc<dyn AddressDerivationManagerTrait> {
        self.info().unwrap().derivation.clone().expect("Account initialization is pending.")
    }
}
