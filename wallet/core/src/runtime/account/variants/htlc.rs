use crate::imports::*;
use crate::result::Result;
use crate::runtime::account::Inner;
use crate::runtime::account::{Account, AccountId, AccountKind, DerivationCapableAccount};
use crate::runtime::Wallet;
use crate::storage::account::HtlcRole;
use crate::storage::{self, Metadata, PrvKeyDataId, Settings};
use kaspa_bip32::ExtendedPublicKey;
use kaspa_hashes::Hash;
use kaspa_txscript::{extract_script_pub_key_address, htlc_redeem_script, pay_to_script_hash_script};
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
    locktime: u64,
    secret_hash: Hash,
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

        let storage::account::HTLC { xpub_key, second_party_xpub_key, account_index, ecdsa, role, locktime, secret_hash, .. } = data;

        if let HtlcRole::Receiver = role {
            return Err(Error::Custom("unexpected role".to_string()));
        };
        Ok(Self {
            inner,
            prv_key_data_id,
            account_index,
            xpub_key,
            second_party_xpub_key,
            ecdsa,
            role: PhantomData::default(),
            locktime,
            secret_hash,
        })
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

        let storage::account::HTLC { xpub_key, second_party_xpub_key, account_index, ecdsa, role, locktime, secret_hash, .. } = data;

        if let HtlcRole::Sender = role {
            return Err(Error::Custom("unexpected role".to_string()));
        };
        Ok(Self {
            inner,
            prv_key_data_id,
            account_index,
            xpub_key,
            second_party_xpub_key,
            ecdsa,
            role: PhantomData::default(),
            locktime,
            secret_hash,
        })
    }
}

#[async_trait]
impl Account for HTLC<Sender> {
    fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    fn account_kind(&self) -> AccountKind {
        AccountKind::HTLC
    }

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        Ok(&self.prv_key_data_id)
    }

    fn as_dyn_arc(self: Arc<Self>) -> Arc<dyn Account> {
        self
    }

    fn receive_address(&self) -> Result<Address> {
        let prefix = self.wallet().address_prefix()?;
        let receiver_xpub = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(&self.second_party_xpub_key)?;
        let sender_xpub = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(&self.xpub_key)?;

        let script = htlc_redeem_script(
            receiver_xpub.public_key.x_only_public_key().0.serialize().as_slice(),
            sender_xpub.public_key.x_only_public_key().0.serialize().as_slice(),
            &self.secret_hash.as_bytes(),
            self.locktime,
        )?;

        let script_pub_key = pay_to_script_hash_script(&script);
        let address = extract_script_pub_key_address(&script_pub_key, prefix)?;
        Ok(address)
    }

    fn change_address(&self) -> Result<Address> {
        Err(Error::AccountKindFeature)
    }

    fn as_storable(&self) -> Result<storage::account::Account> {
        let settings = self.context().settings.clone().unwrap_or_default();

        let htlc = storage::HTLC::new(
            self.xpub_key.clone(),
            self.second_party_xpub_key.clone(),
            self.account_index,
            self.ecdsa,
            HtlcRole::Sender,
            self.locktime,
            self.secret_hash,
        );

        let account = storage::Account::new(*self.id(), None, settings, storage::AccountData::Htlc(htlc));

        Ok(account)
    }

    fn metadata(&self) -> Result<Option<Metadata>> {
        Ok(None)
    }

    fn as_derivation_capable(self: Arc<Self>) -> Result<Arc<dyn DerivationCapableAccount>> {
        Err(Error::AccountKindFeature)
    }
}

#[async_trait]
impl Account for HTLC<Receiver> {
    fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    fn account_kind(&self) -> AccountKind {
        AccountKind::HTLC
    }

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        Ok(&self.prv_key_data_id)
    }

    fn as_dyn_arc(self: Arc<Self>) -> Arc<dyn Account> {
        self
    }

    fn receive_address(&self) -> Result<Address> {
        let prefix = self.wallet().address_prefix()?;
        let sender_xpub = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(&self.second_party_xpub_key)?;
        let receiver_xpub = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(&self.xpub_key)?;

        let script = htlc_redeem_script(
            receiver_xpub.public_key.x_only_public_key().0.serialize().as_slice(),
            sender_xpub.public_key.x_only_public_key().0.serialize().as_slice(),
            &self.secret_hash.as_bytes(),
            self.locktime,
        )?;

        let script_pub_key = pay_to_script_hash_script(&script);
        let address = extract_script_pub_key_address(&script_pub_key, prefix)?;
        Ok(address)
    }

    fn change_address(&self) -> Result<Address> {
        Err(Error::AccountKindFeature)
    }

    fn as_storable(&self) -> Result<storage::account::Account> {
        let settings = self.context().settings.clone().unwrap_or_default();

        let htlc = storage::HTLC::new(
            self.xpub_key.clone(),
            self.second_party_xpub_key.clone(),
            self.account_index,
            self.ecdsa,
            HtlcRole::Receiver,
            self.locktime,
            self.secret_hash,
        );

        let account = storage::Account::new(*self.id(), None, settings, storage::AccountData::Htlc(htlc));

        Ok(account)
    }

    fn metadata(&self) -> Result<Option<Metadata>> {
        Ok(None)
    }

    fn as_derivation_capable(self: Arc<Self>) -> Result<Arc<dyn DerivationCapableAccount>> {
        Err(Error::AccountKindFeature)
    }
}
