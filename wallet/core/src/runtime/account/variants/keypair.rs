use crate::imports::*;
use crate::result::Result;
use crate::runtime::account::{Account, AccountId, AccountKind, Inner};
use crate::runtime::Wallet;
use crate::storage::{self, PrvKeyDataId};
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
        prv_key_data_id: &PrvKeyDataId,
        settings: &storage::account::Settings,
        data: &storage::account::Keypair,
    ) -> Result<Self> {
        let id = AccountId::from_keypair(prv_key_data_id, data);
        let inner = Arc::new(Inner::new(wallet, id, Some(settings)));

        let storage::account::Keypair {
            public_key,
            ecdsa,
        } = data;

        Ok(Self { inner, prv_key_data_id: prv_key_data_id.clone(), public_key: public_key.clone(), ecdsa: *ecdsa })
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

    async fn receive_address(&self) -> Result<Address> {
        Ok(Address::new(self.inner().wallet.network_id()?.into(), Version::PubKey, &self.public_key.serialize()[1..]))
    }

    async fn change_address(&self) -> Result<Address> {
        Ok(Address::new(self.inner().wallet.network_id()?.into(), Version::PubKey, &self.public_key.serialize()[1..]))
    }

    fn as_storable(&self) -> Result<storage::account::Account> {
        let settings = self.context().settings.clone().unwrap_or_default();

        let keypair = storage::Keypair { public_key: self.public_key.clone(), ecdsa: self.ecdsa };

        let account =
            storage::Account::new(self.id_ref().clone(), self.prv_key_data_id, settings, storage::AccountData::Keypair(keypair));

        Ok(account)
    }
}
