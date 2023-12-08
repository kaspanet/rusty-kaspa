use crate::result::Result;
use crate::runtime::account::Inner;
use crate::runtime::account::{Account, AccountId, AccountKind, DerivationCapableAccount};
use crate::runtime::Wallet;
use crate::storage::account::HtlcRole;
use crate::storage::{self, Metadata, PrvKeyDataId, Settings};
use crate::{imports::*, AddressDerivationManagerTrait};
use std::marker::PhantomData;

pub struct Sender;
pub struct Receiver;

pub struct HTLC<T> {
    inner: Arc<Inner>,
    prv_key_data_id: PrvKeyDataId,
    account_index: u64,
    xpub_key: Arc<String>,
    second_party_xpub_key: Arc<String>,
    ecdsa: bool,
    role: PhantomData<T>,
}

impl HTLC<Sender> {
    pub async fn try_new(
        prv_key_data_id: PrvKeyDataId,
        settings: Settings,
        wallet: &Arc<Wallet>,
        data: storage::account::HTLC,
    ) -> Result<Self> {
        let id = AccountId::from_htlc(&prv_key_data_id, &data);
        let inner = Arc::new(Inner::new(wallet, id, Some(settings)));

        let storage::account::HTLC { xpub_key, second_party_xpub_key, account_index, ecdsa, role, .. } = data;

        if let HtlcRole::Receiver = role {
            return Err(Error::Custom("unexpected role".to_string()));
        };
        Ok(Self { inner, prv_key_data_id, account_index, xpub_key, second_party_xpub_key, ecdsa, role: PhantomData::default() })
    }
}

impl HTLC<Receiver> {
    pub async fn try_new(
        prv_key_data_id: PrvKeyDataId,
        settings: Settings,
        wallet: &Arc<Wallet>,
        data: storage::account::HTLC,
    ) -> Result<Self> {
        let id = AccountId::from_htlc(&prv_key_data_id, &data);
        let inner = Arc::new(Inner::new(wallet, id, Some(settings)));

        let storage::account::HTLC { xpub_key, second_party_xpub_key, account_index, ecdsa, role, .. } = data;

        if let HtlcRole::Sender = role {
            return Err(Error::Custom("unexpected role".to_string()));
        };
        Ok(Self { inner, prv_key_data_id, account_index, xpub_key, second_party_xpub_key, ecdsa, role: PhantomData::default() })
    }
}

#[async_trait]
impl<T> Account for HTLC<T> {
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
        todo!()
    }

    fn change_address(&self) -> Result<Address> {
        Err(Error::AccountKindFeature)
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
        Ok(None)
    }

    fn as_derivation_capable(self: Arc<Self>) -> Result<Arc<dyn DerivationCapableAccount>> {
        Err(Error::AccountKindFeature)
    }
}
