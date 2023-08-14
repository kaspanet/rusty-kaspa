use crate::derivation::AddressDerivationManager;
use crate::imports::*;
use crate::result::Result;
use crate::runtime::account::Inner;
use crate::runtime::account::{Account, AccountId, AccountKind, DerivationCapableAccount};
use crate::runtime::Wallet;
use crate::storage::{self, PrvKeyDataId};
// use kaspa_addresses::Version as AddressVersion;
// use secp256k1::{PublicKey, SecretKey};

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
        prv_key_data_id: &PrvKeyDataId,
        settings: &storage::account::Settings,
        data: &storage::account::Bip32,
    ) -> Result<Self> {
        let id = AccountId::from_bip32(prv_key_data_id, data);
        let inner = Arc::new(Inner::new(wallet, id, Some(settings)));

        let storage::account::Bip32 {
            // prv_key_data_id,
            account_index,
            xpub_keys,
            ecdsa,
        } = data;

        let derivation = AddressDerivationManager::new(wallet, AccountKind::Bip32, xpub_keys, *ecdsa, None, None, None, None).await?;

        Ok(Self {
            inner,
            prv_key_data_id: prv_key_data_id.clone(), //*prv_key_data_id,
            account_index: *account_index,
            xpub_keys: data.xpub_keys.clone(),
            ecdsa: *ecdsa,
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

    // fn test(self: &Arc<Self>) -> Arc<dyn Account> {
    //     self.clone()
    // }

    async fn receive_address(&self) -> Result<Address> {
        self.derivation.receive_address_manager().current_address().await
    }
    async fn change_address(&self) -> Result<Address> {
        self.derivation.change_address_manager().current_address().await
    }

    // async fn new_receive_address(self: Arc<Self>) -> Result<String> {
    //     todo!()
    // }
    // async fn new_change_address(self: Arc<Self>) -> Result<String> {
    //     todo!()
    // }

    fn as_storable(&self) -> Result<storage::account::Account> {
        let settings = self.context().settings.clone().unwrap_or_default();

        let bip32 = storage::Bip32 {
            // prv_key_data_id: self.prv_key_data_id,
            account_index: self.account_index,
            xpub_keys: self.xpub_keys.clone(),
            ecdsa: self.ecdsa,
        };

        let account = storage::Account::new(self.id_ref().clone(), self.prv_key_data_id, settings, storage::AccountData::Bip32(bip32));

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

impl DerivationCapableAccount for Bip32 {
    fn derivation(&self) -> &Arc<AddressDerivationManager> {
        &self.derivation
    }
}
