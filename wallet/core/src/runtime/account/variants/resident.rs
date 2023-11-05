use crate::imports::*;
use crate::result::Result;
use crate::runtime::account::descriptor::{self, AccountDescriptor};
use crate::runtime::account::{Account, AccountId, AccountKind, Inner};
use crate::runtime::Wallet;
use crate::storage::{self, Metadata, PrvKeyDataId};
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

    fn receive_address(&self) -> Result<Address> {
        let (xonly_public_key, _) = self.public_key.x_only_public_key();
        Ok(Address::new(self.inner().wallet.network_id()?.into(), Version::PubKey, &xonly_public_key.serialize()))
    }

    fn change_address(&self) -> Result<Address> {
        let (xonly_public_key, _) = self.public_key.x_only_public_key();
        Ok(Address::new(self.inner().wallet.network_id()?.into(), Version::PubKey, &xonly_public_key.serialize()))
    }

    fn as_storable(&self) -> Result<storage::account::Account> {
        Err(Error::ResidentAccount)
    }

    fn metadata(&self) -> Result<Option<Metadata>> {
        Err(Error::ResidentAccount)
    }

    fn descriptor(&self) -> Result<AccountDescriptor> {
        let descriptor =
            descriptor::Resident { account_id: *self.id(), account_name: self.name(), public_key: self.public_key.to_string() };

        Ok(descriptor.into())
    }
}
