use crate::derivation::AddressDerivationManager;
use crate::imports::*;
use crate::result::Result;
use crate::runtime::account::Inner;
use crate::runtime::account::{Account, AccountId, AccountKind, DerivationCapableAccount};
use crate::runtime::Wallet;
use crate::storage::{self, Metadata, PrvKeyDataId, Settings};
use crate::AddressDerivationManagerTrait;

pub struct Bip32 {
    inner: Arc<Inner>,
    prv_key_data_id: PrvKeyDataId,
    account_index: u64,
    xpub_keys: Arc<Vec<String>>,
    ecdsa: bool,
    derivation: Arc<AddressDerivationManager>,
}

impl Bip32 {
    pub async fn try_new(
        wallet: &Arc<Wallet>,
        prv_key_data_id: PrvKeyDataId,
        settings: Settings,
        data: storage::account::Bip32,
        meta: Option<Arc<Metadata>>,
    ) -> Result<Self> {
        let id = AccountId::from_bip32(&prv_key_data_id, &data);
        let inner = Arc::new(Inner::new(wallet, id, Some(settings)));

        let storage::account::Bip32 { account_index, xpub_keys, ecdsa, .. } = data;

        let address_derivation_indexes = meta.and_then(|meta| meta.address_derivation_indexes()).unwrap_or_default();

        let derivation =
            AddressDerivationManager::new(wallet, AccountKind::Bip32, &xpub_keys, ecdsa, 0, None, 1, address_derivation_indexes)
                .await?;

        Ok(Self {
            inner,
            prv_key_data_id, //: prv_key_data_id.clone(),
            account_index,   //: account_index,
            xpub_keys,       //: data.xpub_keys.clone(),
            ecdsa,
            derivation,
        })
    }
}

#[async_trait]
impl Account for Bip32 {
    fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    fn account_kind(&self) -> AccountKind {
        AccountKind::Bip32
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
        let bip32 = storage::Bip32::new(self.account_index, self.xpub_keys.clone(), self.ecdsa);
        let account = storage::Account::new(*self.id(), Some(self.prv_key_data_id), settings, storage::AccountData::Bip32(bip32));
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

impl DerivationCapableAccount for Bip32 {
    fn derivation(&self) -> Arc<dyn AddressDerivationManagerTrait> {
        self.derivation.clone()
    }
}
