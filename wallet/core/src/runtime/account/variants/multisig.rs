#![allow(dead_code)]

use crate::result::Result;
use crate::runtime::account::Inner;
use crate::runtime::account::{Account, AccountId, AccountKind, DerivationCapableAccount};
use crate::runtime::Wallet;
use crate::storage::{self, Metadata, PrvKeyDataId, Settings};
use crate::AddressDerivationManager;
use crate::{imports::*, AddressDerivationManagerTrait};

pub struct MultiSig {
    inner: Arc<Inner>,
    pub xpub_keys: Arc<Vec<String>>,
    cosigner_index: Option<u8>,
    pub minimum_signatures: u16,
    ecdsa: bool,
    derivation: Arc<AddressDerivationManager>,
    pub prv_key_data_ids: Option<Arc<Vec<PrvKeyDataId>>>,
}

impl MultiSig {
    pub async fn try_new(
        wallet: &Arc<Wallet>,
        settings: Settings,
        data: storage::account::MultiSig,
        meta: Option<Arc<Metadata>>,
    ) -> Result<Self> {
        let id = AccountId::from_multisig(&data);
        let inner = Arc::new(Inner::new(wallet, id, Some(settings)));

        let storage::account::MultiSig { xpub_keys, prv_key_data_ids, cosigner_index, minimum_signatures, ecdsa, .. } = data;

        let address_derivation_indexes = meta.and_then(|meta| meta.address_derivation_indexes()).unwrap_or_default();

        let derivation = AddressDerivationManager::new(
            wallet,
            AccountKind::MultiSig,
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
        Err(Error::AccountKindFeature)
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

        let multisig = storage::MultiSig::new(
            self.xpub_keys.clone(),
            self.prv_key_data_ids.clone(),
            self.cosigner_index,
            self.minimum_signatures,
            self.ecdsa,
        );

        let account = storage::Account::new(*self.id(), None, settings, storage::AccountData::MultiSig(multisig));

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

impl DerivationCapableAccount for MultiSig {
    fn derivation(&self) -> Arc<dyn AddressDerivationManagerTrait> {
        self.derivation.clone()
    }
}
