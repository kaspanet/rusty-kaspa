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

pub struct Legacy {
    inner: Arc<Inner>,
    prv_key_data_id: PrvKeyDataId,
    derivation: Arc<AddressDerivationManager>,
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

        let address_derivation_indexes =
            meta.and_then(|meta| meta.address_derivation_indexes()).unwrap_or(AddressDerivationMeta::new(0, 0));
        let account_index = 0;
        let derivation =
            AddressDerivationManager::create_legacy_pubkey_managers(wallet, account_index, address_derivation_indexes.clone(), data)?;

        Ok(Self { inner, prv_key_data_id, derivation })
    }

    pub async fn initialize_derivation(
        &self,
        wallet_secret: Secret,
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
        // for manager in self.derivation.receive_address_manager().pubkey_managers.iter() {
        //     manager.initialize(xprv.clone())?;
        // }
        // for manager in self.derivation.change_address_manager().pubkey_managers.iter() {
        //     manager.initialize(xprv.clone())?;
        // }
        for derivator in &self.derivation.derivators {
            derivator.initialize(xprv.clone(), index)?;
        }
        //let keys = vec![xprv];

        // let meta = { self.info.lock()?.meta.clone() };

        // let address_derivation_indexes = address_derivation_indexes.unwrap_or(meta);
        // let derivation = AddressDerivationManager::new(
        //     &self.inner.wallet,
        //     AccountKind::Legacy,
        //     &keys,
        //     false,
        //     0,
        //     None,
        //     1,
        //     address_derivation_indexes,
        // )
        // .await?;

        // let meta = derivation.address_derivation_meta();
        // let receive_address = derivation.receive_address_manager.current_address()?;
        // let change_address = derivation.change_address_manager.current_address()?;

        // let mut info = self.info.lock()?;
        // info.derivation = Some(derivation.clone());
        // info.receive_address = Some(receive_address);
        // info.change_address = Some(change_address);
        // info.meta = meta;

        Ok(())
    }

    // fn info(&self) -> Result<MutexGuard<Info>> {
    //     Ok(self.info.lock()?)
    // }
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
        //self.info()?.receive_address.clone().ok_or(Error::Custom("Account initialization is pending.".into()))
        Ok(self.derivation.receive_address_manager().current_address()?)
        //Ok(self.receive_address.clone())
    }

    fn change_address(&self) -> Result<Address> {
        //self.info()?.change_address.clone().ok_or(Error::Custom("Account initialization is pending.".into()))
        Ok(self.derivation.change_address_manager().current_address()?)
        //Ok(self.change_address.clone())
    }

    fn as_storable(&self) -> Result<storage::account::Account> {
        let settings = self.context().settings.clone().unwrap_or_default();
        // let mut receive_pubkeys = HashMap::new();
        // let mut change_pubkeys = HashMap::new();
        // for derivator in &self.derivation.derivators {
        //     receive_pubkeys.extend(derivator.receive_pubkey_manager().get_cache()?);
        //     change_pubkeys.extend(derivator.change_pubkey_manager().get_cache()?);
        // }
        let legacy = storage::Legacy {};
        // receive_pubkeys: Arc::new(receive_pubkeys),
        // change_pubkeys: Arc::new(change_pubkeys)

        let account = storage::Account::new(*self.id(), Some(self.prv_key_data_id), settings, storage::AccountData::Legacy(legacy));

        Ok(account)
    }

    fn metadata(&self) -> Result<Option<Metadata>> {
        let metadata = Metadata::new(self.inner.id, self.derivation.address_derivation_meta());
        Ok(Some(metadata))
    }

    fn as_derivation_capable(self: Arc<Self>) -> Result<Arc<dyn DerivationCapableAccount>> {
        Ok(self.clone())
    }

    async fn initialize(self: Arc<Self>, secret: Secret, payment_secret: Option<&Secret>, index: Option<u32>) -> Result<()> {
        log_info!("initialize_derivation");
        self.initialize_derivation(secret, payment_secret, index).await?;
        Ok(())
    }

    async fn uninitialize(self: Arc<Self>) -> Result<()> {
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
}
