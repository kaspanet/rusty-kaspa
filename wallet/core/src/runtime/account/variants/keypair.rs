use crate::imports::*;
use crate::result::Result;
use crate::runtime::account::{Account, AccountId, AccountKind, Inner};
use crate::runtime::Wallet;
use crate::storage::{self, Metadata, PrvKeyDataId, Settings};
use kaspa_addresses::Version;
use secp256k1::PublicKey;

pub struct Keypair {
    inner: Arc<Inner>,
    prv_key_data_id: PrvKeyDataId,
    public_key: PublicKey,
    ecdsa: bool,
}

impl Keypair {
    pub async fn try_new(
        wallet: &Arc<Wallet>,
        prv_key_data_id: PrvKeyDataId,
        settings: Settings,
        data: storage::account::Keypair,
        _meta: Option<Arc<Metadata>>,
    ) -> Result<Self> {
        let id = AccountId::from_keypair(&prv_key_data_id, &data);
        let inner = Arc::new(Inner::new(wallet, id, Some(settings)));

        let storage::account::Keypair { public_key, ecdsa, .. } = data;
        Ok(Self { inner, prv_key_data_id, public_key, ecdsa })
    }
}

#[async_trait]
impl Account for Keypair {
    fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    fn account_kind(&self) -> AccountKind {
        AccountKind::Keypair
    }

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        Ok(&self.prv_key_data_id)
    }

    fn as_dyn_arc(self: Arc<Self>) -> Arc<dyn Account> {
        self
    }

    fn receive_address(&self) -> Result<Address> {
        let (xonly_public_key, _) = self.public_key.x_only_public_key();
        Ok(Address::new(self.inner().wallet.network_id()?.into(), Version::PubKey, &xonly_public_key.serialize()))
    }

    fn change_address(&self) -> Result<Address> {
        let (xonly_public_key, _) = self.public_key.x_only_public_key();
        Ok(Address::new(self.inner().wallet.network_id()?.into(), Version::PubKey, &xonly_public_key.serialize()))
    }

    fn as_storable(&self) -> Result<storage::account::Account> {
        let settings = self.context().settings.clone().unwrap_or_default();
        let keypair = storage::Keypair::new(self.public_key, self.ecdsa);
        let account = storage::Account::new(*self.id(), Some(self.prv_key_data_id), settings, storage::AccountData::Keypair(keypair));
        Ok(account)
    }

    fn metadata(&self) -> Result<Option<Metadata>> {
        Ok(None)
    }
}
