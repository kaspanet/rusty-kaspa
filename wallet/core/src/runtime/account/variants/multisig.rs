#![allow(dead_code)]

use crate::imports::*;
use crate::result::Result;
use crate::runtime::account::Inner;
use crate::runtime::account::{Account, DerivationCapableAccount, AccountId, AccountKind};
use crate::runtime::Wallet;
use crate::storage::{self, PrvKeyDataId};
use crate::AddressDerivationManager;

pub struct MultiSig {
    inner: Arc<Inner>,

    prv_key_data_id: PrvKeyDataId,
    account_index: u64,
    xpub_keys: Arc<Vec<String>>,
    cosigner_index: u8,
    minimum_signatures: u16,
    ecdsa: bool,
    derivation: Arc<AddressDerivationManager>,
}

impl MultiSig {
    pub async fn try_new(
        wallet: &Arc<Wallet>,
        prv_key_data_id: &PrvKeyDataId,
        settings: &storage::account::Settings,
        data: &storage::account::MultiSig,
    ) -> Result<Self> {
        let id = AccountId::from_multisig(prv_key_data_id, data);
        let inner = Arc::new(Inner::new(wallet, id, Some(settings)));

        let storage::account::MultiSig {
            account_index,
            xpub_keys,
            cosigner_index,
            minimum_signatures,
            ecdsa,
        } = data;

        let derivation = AddressDerivationManager::new(
            wallet,
            AccountKind::Legacy,
            xpub_keys,
            false,
            Some(*cosigner_index as u32),
            Some(*minimum_signatures as u32),
            None,
            None,
        )
        .await?;

        Ok(Self {
            inner,
            prv_key_data_id: prv_key_data_id.clone(),
            account_index: *account_index,
            xpub_keys: xpub_keys.clone(),
            cosigner_index: *cosigner_index,
            minimum_signatures: *minimum_signatures,
            ecdsa: *ecdsa,
            derivation,
        })
    }
}


#[async_trait]
impl Account for MultiSig {
    fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    fn account_kind(&self) -> AccountKind {
        AccountKind::MultiSig
    }

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        Ok(&self.prv_key_data_id)
    }

    fn as_dyn_arc(self: Arc<Self>) -> Arc<dyn Account> {
        self
    }

    async fn receive_address(&self) -> Result<Address> {
        self.derivation.receive_address_manager().current_address().await
    }
    async fn change_address(&self) -> Result<Address> {
        self.derivation.change_address_manager().current_address().await
    }

    fn as_storable(&self) -> Result<storage::account::Account> {
        let settings = self.context().settings.clone().unwrap_or_default();

        let multisig = storage::MultiSig {
            account_index: self.account_index,
            xpub_keys: self.xpub_keys.clone(),
            ecdsa: self.ecdsa,
            cosigner_index: self.cosigner_index,
            minimum_signatures: self.minimum_signatures,
        };

        let account = storage::Account::new(self.id_ref().clone(), self.prv_key_data_id, settings, storage::AccountData::MultiSig(multisig));

        Ok(account)
    }

    fn as_derivation_capable(self: Arc<Self>) -> Result<Arc<dyn DerivationCapableAccount>> {
        Ok(self.clone())
    }
}

impl DerivationCapableAccount for MultiSig {
    fn derivation(&self) -> &Arc<AddressDerivationManager> {
        &self.derivation
    }
}
