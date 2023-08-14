use crate::imports::*;
use crate::result::Result;
use crate::runtime::account::{Account, AccountId, AccountKind, Inner};
use crate::runtime::Wallet;
use crate::storage::{self, PrvKeyDataId};
use kaspa_addresses::Version;
use secp256k1::{PublicKey, SecretKey};

pub struct Resident {
    inner: Arc<Inner>,
    public_key: PublicKey,

    #[allow(dead_code)]
    secret_key: Option<SecretKey>,
}

impl Resident {
    pub async fn try_new(wallet: &Arc<Wallet>, public_key: PublicKey, secret_key: Option<SecretKey>) -> Result<Self> {
        let id = AccountId::from_public_key(AccountKind::Resident, &public_key);
        let inner = Arc::new(Inner::new(wallet, id, None));

        Ok(Self { inner, public_key, secret_key })
    }
}

#[async_trait]
impl Account for Resident {
    fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    fn account_kind(&self) -> AccountKind {
        AccountKind::Resident
    }

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        Err(Error::ResidentAccount)
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
        Err(Error::ResidentAccount)
    }
}
