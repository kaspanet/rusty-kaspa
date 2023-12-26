//!
//! Resident account implementation (for temporary use in runtime)
//!

use crate::account::Inner;
use crate::imports::*;
use kaspa_addresses::Version;
use secp256k1::{PublicKey, SecretKey};

pub const RESIDENT_ACCOUNT_KIND: &str = "kaspa-resident-standard";

pub struct Resident {
    inner: Arc<Inner>,
    public_key: PublicKey,

    #[allow(dead_code)]
    secret_key: Option<SecretKey>,
}

impl Resident {
    pub async fn try_load(wallet: &Arc<Wallet>, public_key: PublicKey, secret_key: Option<SecretKey>) -> Result<Self> {
        let (id, storage_key) = make_account_hashes(from_public_key(&RESIDENT_ACCOUNT_KIND.into(), &public_key));
        let inner = Arc::new(Inner::new(wallet, id, storage_key, Default::default()));

        Ok(Self { inner, public_key, secret_key })
    }
}

#[async_trait]
impl Account for Resident {
    fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    fn account_kind(&self) -> AccountKind {
        RESIDENT_ACCOUNT_KIND.into()
    }

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        Err(Error::ResidentAccount)
    }

    fn as_dyn_arc(self: Arc<Self>) -> Arc<dyn Account> {
        self
    }

    fn sig_op_count(&self) -> u8 {
        // TODO - discuss
        unreachable!()
    }

    fn minimum_signatures(&self) -> u16 {
        // TODO - discuss
        unreachable!()
    }

    fn receive_address(&self) -> Result<Address> {
        let (xonly_public_key, _) = self.public_key.x_only_public_key();
        Ok(Address::new(self.inner().wallet.network_id()?.into(), Version::PubKey, &xonly_public_key.serialize()))
    }

    fn change_address(&self) -> Result<Address> {
        let (xonly_public_key, _) = self.public_key.x_only_public_key();
        Ok(Address::new(self.inner().wallet.network_id()?.into(), Version::PubKey, &xonly_public_key.serialize()))
    }

    fn to_storage(&self) -> Result<AccountStorage> {
        Err(Error::ResidentAccount)
    }

    fn metadata(&self) -> Result<Option<AccountMetadata>> {
        Err(Error::ResidentAccount)
    }

    fn descriptor(&self) -> Result<AccountDescriptor> {
        let descriptor = AccountDescriptor::new(
            RESIDENT_ACCOUNT_KIND.into(),
            *self.id(),
            self.name(),
            AssocPrvKeyDataIds::None,
            self.receive_address().ok(),
            self.change_address().ok(),
        );

        Ok(descriptor)
    }
}
