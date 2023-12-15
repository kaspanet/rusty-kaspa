use crate::imports::*;
use crate::result::Result;
use crate::runtime::account::{Account, AccountId, AccountKind, DerivationCapableAccount};
use crate::runtime::account::{GenerationNotifier, Inner};
use crate::runtime::Wallet;
use crate::secret::Secret;
use crate::storage::account::HtlcRole;
use crate::storage::{self, Metadata, PrvKeyDataId, Settings};
use crate::tx::{Fees, Generator, GeneratorSettings, GeneratorSummary, HtlcReceiverSigner, HtlcSenderSigner, PaymentDestination};
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
    pub(crate) xpub_key: Arc<String>,
    pub second_party_address: Arc<Address>,
    ecdsa: bool,
    role: PhantomData<T>,
    pub(crate) locktime: u64,
    pub(crate) secret_hash: Hash,
    pub(crate) secret: Option<Vec<u8>>,
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

        let storage::account::HTLC { xpub_key, second_party_address, account_index, ecdsa, role, locktime, secret_hash, .. } = data;

        if let HtlcRole::Receiver = role {
            return Err(Error::Custom("unexpected role".to_string()));
        };
        Ok(Self {
            inner,
            prv_key_data_id,
            account_index,
            xpub_key,
            second_party_address,
            ecdsa,
            role: PhantomData,
            locktime,
            secret_hash,
            secret: None,
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

        let storage::account::HTLC {
            xpub_key, second_party_address, account_index, ecdsa, role, locktime, secret_hash, secret, ..
        } = data;

        if let HtlcRole::Sender = role {
            return Err(Error::Custom("unexpected role".to_string()));
        };
        Ok(Self {
            inner,
            prv_key_data_id,
            account_index,
            xpub_key,
            second_party_address,
            ecdsa,
            role: PhantomData,
            locktime,
            secret_hash,
            secret,
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

        let receiver = &self.second_party_address.payload;
        let sender = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(&self.xpub_key)?;

        let script = htlc_redeem_script(
            receiver.as_slice(),
            sender.public_key.x_only_public_key().0.serialize().as_slice(),
            &self.secret_hash.as_bytes(),
            self.locktime,
        )?;

        let script_pub_key = pay_to_script_hash_script(&script);
        let address = extract_script_pub_key_address(&script_pub_key, prefix)?;
        Ok(address)
    }

    fn change_address(&self) -> Result<Address> {
        self.receive_address()
    }

    fn as_storable(&self) -> Result<storage::account::Account> {
        let settings = self.context().settings.clone().unwrap_or_default();

        let htlc = storage::HTLC::new(
            self.xpub_key.clone(),
            self.second_party_address.clone(),
            self.account_index,
            self.ecdsa,
            HtlcRole::Sender,
            self.locktime,
            self.secret_hash,
            self.secret.clone(),
        );

        let account = storage::Account::new(*self.id(), Some(self.prv_key_data_id), settings, storage::AccountData::Htlc(htlc));

        Ok(account)
    }

    fn metadata(&self) -> Result<Option<Metadata>> {
        Ok(None)
    }

    fn as_derivation_capable(self: Arc<Self>) -> Result<Arc<dyn DerivationCapableAccount>> {
        Err(Error::AccountKindFeature)
    }

    fn sig_op_count(&self) -> u8 {
        2
    }

    async fn send(
        self: Arc<Self>,
        destination: PaymentDestination,
        priority_fee_sompi: Fees,
        payload: Option<Vec<u8>>,
        wallet_secret: Secret,
        _payment_secret: Option<Secret>,
        abortable: &Abortable,
        notifier: Option<GenerationNotifier>,
    ) -> Result<(GeneratorSummary, Vec<kaspa_hashes::Hash>)> {
        let keydata = self.prv_key_data(wallet_secret).await?;
        let signer = Arc::new(HtlcSenderSigner::new(self.clone(), keydata));

        let settings = GeneratorSettings::try_new_with_account(self.clone().as_dyn_arc(), destination, priority_fee_sompi, payload)?;

        let generator = Generator::try_new(settings, Some(signer), Some(abortable))?;

        let mut stream = generator.stream();
        let mut ids = vec![];
        while let Some(transaction) = stream.try_next().await? {
            if let Some(notifier) = notifier.as_ref() {
                notifier(&transaction);
            }

            transaction.try_sign()?;
            transaction.log().await?;
            let id = transaction.try_submit(&self.wallet().rpc_api()).await?;
            ids.push(id);
            yield_executor().await;
        }

        Ok((generator.summary(), ids))
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
        let receiver_xpub = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(&self.xpub_key)?;

        let script = htlc_redeem_script(
            receiver_xpub.public_key.x_only_public_key().0.serialize().as_slice(),
            self.second_party_address.payload.as_slice(),
            &self.secret_hash.as_bytes(),
            self.locktime,
        )?;
        let script_pub_key = pay_to_script_hash_script(&script);
        let address = extract_script_pub_key_address(&script_pub_key, prefix)?;
        Ok(address)
    }

    fn change_address(&self) -> Result<Address> {
        self.receive_address()
    }

    fn as_storable(&self) -> Result<storage::account::Account> {
        let settings = self.context().settings.clone().unwrap_or_default();

        let htlc = storage::HTLC::new(
            self.xpub_key.clone(),
            self.second_party_address.clone(),
            self.account_index,
            self.ecdsa,
            HtlcRole::Receiver,
            self.locktime,
            self.secret_hash,
            self.secret.clone(),
        );

        let account = storage::Account::new(*self.id(), Some(self.prv_key_data_id), settings, storage::AccountData::Htlc(htlc));

        Ok(account)
    }

    fn metadata(&self) -> Result<Option<Metadata>> {
        Ok(None)
    }

    fn as_derivation_capable(self: Arc<Self>) -> Result<Arc<dyn DerivationCapableAccount>> {
        Err(Error::AccountKindFeature)
    }

    async fn send(
        self: Arc<Self>,
        destination: PaymentDestination,
        priority_fee_sompi: Fees,
        payload: Option<Vec<u8>>,
        wallet_secret: Secret,
        _payment_secret: Option<Secret>,
        abortable: &Abortable,
        notifier: Option<GenerationNotifier>,
    ) -> Result<(GeneratorSummary, Vec<Hash>)> {
        let Some(_) = &self.secret else { return Err(Error::Custom("not ready, should fill secret first".to_string())) };
        let keydata = self.prv_key_data(wallet_secret).await?;
        let signer = Arc::new(HtlcReceiverSigner::new(self.clone(), keydata));

        let settings = GeneratorSettings::try_new_with_account(self.clone().as_dyn_arc(), destination, priority_fee_sompi, payload)?;

        let generator = Generator::try_new(settings, Some(signer), Some(abortable))?;

        let mut stream = generator.stream();
        let mut ids = vec![];
        while let Some(transaction) = stream.try_next().await? {
            if let Some(notifier) = notifier.as_ref() {
                notifier(&transaction);
            }

            transaction.try_sign()?;
            transaction.log().await?;
            let id = transaction.try_submit(&self.wallet().rpc_api()).await?;
            ids.push(id);
            yield_executor().await;
        }

        Ok((generator.summary(), ids))
    }
    fn sig_op_count(&self) -> u8 {
        2
    }
}
