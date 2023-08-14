use crate::imports::*;
use crate::result::Result;
use crate::runtime::account::{Account, AccountId, AccountKind, DerivationCapableAccount, Inner};
use crate::runtime::Wallet;
use crate::storage::{self, PrvKeyDataId};
use crate::AddressDerivationManager;
// use kaspa_addresses::Version as AddressVersion;
// use secp256k1::{PublicKey, SecretKey};

pub struct Legacy {
    inner: Arc<Inner>,
    prv_key_data_id: PrvKeyDataId,
    derivation: Arc<AddressDerivationManager>,
}

impl Legacy {
    pub async fn try_new(
        wallet: &Arc<Wallet>,
        prv_key_data_id: &PrvKeyDataId,
        settings: &storage::account::Settings,
        data: &storage::account::Legacy,
    ) -> Result<Self> {
        let id = AccountId::from_legacy(prv_key_data_id, data);
        let inner = Arc::new(Inner::new(wallet, id, Some(settings)));

        let derivation =
            AddressDerivationManager::new(wallet, AccountKind::Legacy, &data.xpub_keys, false, 0, None, None, None, None).await?;

        Ok(Self { inner, prv_key_data_id: prv_key_data_id.clone(), derivation })
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

    // fn test(self: &Arc<Self>) -> Arc<dyn Account> {
    //     self.clone()
    // }

    async fn receive_address(&self) -> Result<Address> {
        self.derivation.receive_address_manager().current_address().await
    }

    async fn change_address(&self) -> Result<Address> {
        self.derivation.change_address_manager().current_address().await
    }

    // async fn new_receive_address(self: Arc<Self>) -> Result<Address> {
    //     self.derivation.receive_address_manager.new_address().await
    // }
    // async fn new_change_address(self: Arc<Self>) -> Result<Address> {
    //     todo!()
    // }

    fn as_storable(&self) -> Result<storage::account::Account> {
        let settings = self.context().settings.clone().unwrap_or_default();

        let legacy = storage::Legacy { prv_key_data_id: self.prv_key_data_id };

        let account =
            storage::Account::new(self.id_ref().clone(), self.prv_key_data_id, settings, storage::AccountData::Legacy(legacy));

        Ok(account)

        // Ok(storage::account::Account::Bip32(storage::account::Bip32 {
        //     prv_key_data_id: self.prv_key_data_id,
        //     account_index: self.account_index,
        //     xpub_keys: self.xpub_keys.clone(),
        //     ecdsa: self.ecdsa,
        // }))
    }

    fn as_derivation_capable(self: Arc<Self>) -> Result<Arc<dyn DerivationCapableAccount>> {
        Ok(self.clone())
    }
}

impl DerivationCapableAccount for Legacy {
    fn derivation(&self) -> &Arc<AddressDerivationManager> {
        &self.derivation
    }
}
