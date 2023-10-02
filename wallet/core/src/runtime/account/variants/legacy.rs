use crate::imports::*;
use crate::result::Result;
use crate::runtime::account::{Account, AccountId, AccountKind, DerivationCapableAccount, Inner};
use crate::runtime::Wallet;
use crate::storage::{self, Metadata, PrvKeyDataId, Settings};
use crate::AddressDerivationManager;
use crate::AddressDerivationManagerTrait;

pub struct Legacy {
    inner: Arc<Inner>,
    prv_key_data_id: PrvKeyDataId,
    xpub_keys: Arc<Vec<String>>,
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

        let storage::account::Legacy { xpub_keys, .. } = data;

        let address_derivation_indexes = meta.and_then(|meta| meta.address_derivation_indexes()).unwrap_or_default();

        let derivation =
            AddressDerivationManager::new(wallet, AccountKind::Legacy, &xpub_keys, false, 0, None, 1, address_derivation_indexes)
                .await?;

        Ok(Self { inner, prv_key_data_id, xpub_keys, derivation })
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
        self.derivation.receive_address_manager().current_address()
    }

    fn change_address(&self) -> Result<Address> {
        self.derivation.change_address_manager().current_address()
    }

    fn as_storable(&self) -> Result<storage::account::Account> {
        let settings = self.context().settings.clone().unwrap_or_default();

        let legacy = storage::Legacy::new(self.xpub_keys.clone());

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
}

impl DerivationCapableAccount for Legacy {
    fn derivation(&self) -> Arc<dyn AddressDerivationManagerTrait> {
        self.derivation.clone()
    }
}
