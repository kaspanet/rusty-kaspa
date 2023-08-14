pub mod data;
pub mod id;
pub mod kind;
pub mod variants;

pub use data::*;
pub use id::*;
use kaspa_bip32::ChildNumber;
pub use kind::*;
use secp256k1::ONE_KEY;
pub use variants::*;

#[allow(unused_imports)]
use crate::accounts::{gen0::*, gen1::*, PubkeyDerivationManagerTrait, WalletDerivationManagerTrait};
use crate::derivation::build_derivate_paths;
use crate::derivation::AddressDerivationManager;
use crate::imports::*;
use crate::result::Result;
use crate::runtime::{Balance, BalanceStrings, Wallet};
use crate::secret::Secret;
use crate::storage::interface::AccessContext;
use crate::storage::{self, AccountData, AccessContextT, PrvKeyData, PrvKeyDataId};
use crate::tx::{Fees, Generator, GeneratorSettings, GeneratorSummary, KeydataSigner, PaymentDestination, PendingTransaction, Signer};
use crate::utxo::{UtxoContext, UtxoContextBinding, UtxoEntryReference};
use kaspa_notify::listener::ListenerId;
use separator::Separatable;
use workflow_core::abortable::Abortable;

use super::AtomicBalance;

pub const DEFAULT_AMOUNT_PADDING: usize = 19;

pub type GenerationNotifier = Arc<dyn Fn(&PendingTransaction) + Send + Sync>;
pub type DeepScanNotifier = Arc<dyn Fn(usize, u64, Option<TransactionId>) + Send + Sync>;

pub struct Context {
    pub settings: Option<storage::account::Settings>,
    pub listener_id: Option<ListenerId>,
}

pub struct Inner {
    context: Mutex<Context>,
    id: AccountId,
    wallet: Arc<Wallet>,
    utxo_context: UtxoContext,
}

impl Inner {
    pub fn new(wallet: &Arc<Wallet>, id: AccountId, settings: Option<&storage::account::Settings>) -> Self {
        let utxo_context = UtxoContext::new(wallet.utxo_processor(), UtxoContextBinding::AccountId(id));

        let context = Context { listener_id: None, settings: settings.cloned() };
        Inner { context: Mutex::new(context), id, wallet: wallet.clone(), utxo_context: utxo_context.clone() }
    }
}

pub async fn try_from_storage(wallet: &Arc<Wallet>, stored_account: &Arc<storage::Account>) -> Result<Arc<dyn Account>> {
    match &stored_account.data {
        AccountData::Bip32(bip32) => {
            Ok(Arc::new(Bip32::try_new(wallet, &stored_account.prv_key_data_id, &stored_account.settings, bip32).await?))
        }
        AccountData::Legacy(legacy) => {
            Ok(Arc::new(Legacy::try_new(wallet, &stored_account.prv_key_data_id, &stored_account.settings, legacy).await?))
        },
        AccountData::MultiSig(multisig) => {
            Ok(Arc::new(MultiSig::try_new(wallet, &stored_account.prv_key_data_id, &stored_account.settings, multisig).await?))
        },
        AccountData::Keypair(keypair) => {
            Ok(Arc::new(Keypair::try_new(wallet, &stored_account.prv_key_data_id, &stored_account.settings, keypair).await?))
        }
    }
}

#[async_trait]
pub trait Account: AnySync + Send + Sync + 'static {
    fn inner(&self) -> &Arc<Inner>;

    fn context(&self) -> MutexGuard<Context> {
        self.inner().context.lock().unwrap()
    }

    fn id_ref(&self) -> &AccountId {
        &self.inner().id
    }

    fn account_kind(&self) -> AccountKind;

    fn wallet(&self) -> &Arc<Wallet> {
        &self.inner().wallet
    }

    fn utxo_context(&self) -> &UtxoContext {
        &self.inner().utxo_context
    }

    fn balance(&self) -> Option<Balance> {
        self.utxo_context().balance()
    }

    fn balance_as_strings(&self, padding: Option<usize>) -> Result<BalanceStrings> {
        Ok(BalanceStrings::from((&self.balance(), &self.wallet().network_id()?.into(), padding)))
    }

    fn name(&self) -> Option<String> {
        self.context().settings.as_ref().and_then(|settings| settings.name.clone())
    }

    fn title(&self) -> Option<String> {
        self.context().settings.as_ref().and_then(|settings| settings.title.clone())
    }

    fn name_or_id(&self) -> String {
        if let Some(name) = self.name() {
            // compensate for an empty name
            if name.is_empty() {
                self.id_ref().short()
            } else {
                name
            }
        } else {
            self.id_ref().short()
        }
    }

    fn name_with_id(&self) -> String {
        if let Some(name) = self.name() {
            // compensate for an empty name
            if name.is_empty() {
                self.id_ref().short()
            } else {
                format!("{name} {}", self.id_ref().short())
            }
        } else {
            self.id_ref().short()
        }
    }

    async fn rename(&self, secret: Secret, name: Option<&str>) -> Result<()> {
        {
            let mut context = self.context();
            if let Some(settings) = &mut context.settings {
                settings.name = name.map(String::from);
            } else {
                context.settings = Some(storage::Settings { name: name.map(String::from), title: None, ..Default::default() });
            }
        }

        let stored_account = self.as_storable()?;
        self.wallet().store().as_account_store()?.store(&[&stored_account]).await?;

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(secret));
        self.wallet().store().commit(&ctx).await?;
        Ok(())
    }

    fn get_list_string(&self) -> Result<String> {
        let name = style(self.name_with_id()).blue();
        let balance = self.balance_as_strings(None)?;
        let mature_utxo_size = self.utxo_context().mature_utxo_size();
        let pending_utxo_size = self.utxo_context().pending_utxo_size();
        let info = match (mature_utxo_size, pending_utxo_size) {
            (0, 0) => "".to_string(),
            (_, 0) => {
                format!("{} UTXOs", mature_utxo_size.separated_string())
            }
            (0, _) => {
                format!("{} UTXOs pending", pending_utxo_size.separated_string())
            }
            _ => {
                format!("{} UTXOs, {} UTXOs pending", mature_utxo_size.separated_string(), pending_utxo_size.separated_string())
            }
        };
        Ok(format!("{name}: {balance}   {}", style(info).dim()))
    }

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        Err(Error::ResidentAccount)
        // panic!("account type does not have a private key in storage")
    }

    async fn prv_key_data(&self, wallet_secret: Secret) -> Result<PrvKeyData> {
        let prv_key_data_id = self.prv_key_data_id()?;

        let access_ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
        let keydata = self
            .wallet()
            .store()
            .as_prv_key_data_store()?
            .load_key_data(&access_ctx, prv_key_data_id)
            .await?
            .ok_or(Error::PrivateKeyNotFound(prv_key_data_id.to_hex()))?;
        Ok(keydata)
    }

    fn as_storable(&self) -> Result<storage::Account>;

    async fn scan(self: Arc<Self>, window_size: Option<usize>, extent: Option<u32>) -> Result<()> {
        self.utxo_context().clear().await?;

        match self.clone().as_derivation_capable() {
            Ok(account) => {
                let derivation = account.derivation();

                let current_daa_score = self.wallet().current_daa_score().ok_or(Error::NotConnected)?;
                let balance = Arc::new(AtomicBalance::default());

                let extent = match extent {
                    Some(depth) => ScanExtent::Depth(depth),
                    None => ScanExtent::EmptyWindow,
                };

                let scans = vec![
                    Scan::new_with_args(derivation.receive_address_manager(), window_size, extent, &balance, current_daa_score),
                    Scan::new_with_args(derivation.change_address_manager(), window_size, extent, &balance, current_daa_score),
                ];

                let futures = scans.iter().map(|scan| scan.scan(self.utxo_context())).collect::<Vec<_>>();

                join_all(futures).await.into_iter().collect::<Result<Vec<_>>>()?;
            }
            Err(_) => {

                // - TODO - Handle Keypair & Resident accounts
                // - TODO - Handle Keypair & Resident accounts
                // - TODO - Handle Keypair & Resident accounts
            }
        }

        // match self.data() {
        //     AccountData::Legacy { derivation, .. }
        //     | AccountData::Bip32 { derivation, .. }
        //     | AccountData::MultiSig { derivation, .. } => {

        //     }
        //     AccountData::ResidentSecp256k1Keypair { public_key, .. } => {
        //         let payload = &public_key.serialize()[1..];
        //         let address = Address::new(self.wallet().network_id()?.network_type.into(), AddressVersion::PubKey, payload);

        //     },

        //     AccountData::Secp256k1Keypair { public_key, .. } => {

        //     }
        // }

        self.utxo_context().update_balance().await?;

        Ok(())
    }

    fn sig_op_count(&self) -> u8 {
        1
    }

    fn minimum_signatures(&self) -> u16 {
        1
    }

    async fn receive_address(&self) -> Result<Address>;

    async fn change_address(&self) -> Result<Address>;

    /// Start Account service task
    async fn start(self: Arc<Self>) -> Result<()> {
        self.connect().await?;
        Ok(())
    }

    /// Stop Account service task
    async fn stop(self: Arc<Self>) -> Result<()> {
        self.utxo_context().clear().await?;
        self.disconnect().await?;
        Ok(())
    }

    /// handle connection event
    async fn connect(self: Arc<Self>) -> Result<()> {
        let vacated = self.wallet().active_accounts().insert(self.clone().as_dyn_arc());
        if vacated.is_none() && self.wallet().is_connected() {
            self.scan(None, None).await?;
        }
        Ok(())
    }

    /// handle disconnection event
    async fn disconnect(&self) -> Result<()> {
        self.wallet().active_accounts().remove(self.id_ref());
        Ok(())
    }

    fn as_dyn_arc(self: Arc<Self>) -> Arc<dyn Account>;

    async fn sweep(
        self: Arc<Self>,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        abortable: &Abortable,
        notifier: Option<GenerationNotifier>,
    ) -> Result<(GeneratorSummary, Vec<kaspa_hashes::Hash>)> {
        let keydata = self.prv_key_data(wallet_secret).await?;
        let signer = Arc::new(Signer::new(self.clone().as_dyn_arc(), keydata, payment_secret));
        let settings =
            GeneratorSettings::try_new_with_account(self.clone().as_dyn_arc(), PaymentDestination::Change, Fees::None, None).await?;
        let generator = Generator::new(settings, Some(signer), abortable);

        let mut stream = generator.stream();
        let mut ids = vec![];
        while let Some(transaction) = stream.try_next().await? {
            if let Some(notifier) = notifier.as_ref() {
                notifier(&transaction);
            }

            transaction.try_sign()?;
            transaction.log().await?;
            let id = transaction.try_submit(self.wallet().rpc()).await?;
            ids.push(id);
            yield_executor().await;
        }

        Ok((generator.summary(), ids))
    }

    async fn send(
        self: Arc<Self>,
        destination: PaymentDestination,
        priority_fee_sompi: Fees,
        payload: Option<Vec<u8>>,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        abortable: &Abortable,
        notifier: Option<GenerationNotifier>,
    ) -> Result<(GeneratorSummary, Vec<kaspa_hashes::Hash>)> {
        let keydata = self.prv_key_data(wallet_secret).await?;
        let signer = Arc::new(Signer::new(self.clone().as_dyn_arc(), keydata, payment_secret));

        let settings =
            GeneratorSettings::try_new_with_account(self.clone().as_dyn_arc(), destination, priority_fee_sompi, payload).await?;

        let generator = Generator::new(settings, Some(signer), abortable);

        let mut stream = generator.stream();
        let mut ids = vec![];
        while let Some(transaction) = stream.try_next().await? {
            if let Some(notifier) = notifier.as_ref() {
                notifier(&transaction);
            }

            transaction.try_sign()?;
            transaction.log().await?;
            let id = transaction.try_submit(self.wallet().rpc()).await?;
            ids.push(id);
            yield_executor().await;
        }

        Ok((generator.summary(), ids))
    }

    async fn estimate(
        self: Arc<Self>,
        destination: PaymentDestination,
        priority_fee_sompi: Fees,
        payload: Option<Vec<u8>>,
        abortable: &Abortable,
    ) -> Result<GeneratorSummary> {
        let settings = GeneratorSettings::try_new_with_account(self.as_dyn_arc(), destination, priority_fee_sompi, payload).await?;

        let generator = Generator::new(settings, None, abortable);

        let mut stream = generator.stream();
        while let Some(_transaction) = stream.try_next().await? {
            _transaction.log().await?;
            yield_executor().await;
        }

        Ok(generator.summary())
    }

    fn as_derivation_capable(self: Arc<Self>) -> Result<Arc<dyn DerivationCapableAccount>> {
        Err(Error::AccountAddressDerivationCaps)
    }
}

downcast_sync!(dyn Account);

// pub struct DerivationInfo {
//     pub account_kind: AccountKind,
//     pub account_index: u64,
//     pub cosigner_index: u8,
//     pub minimum_signatures: u16,
//     pub ecdsa: bool,
// }

#[async_trait]
pub trait DerivationCapableAccount: Account {
    fn derivation(&self) -> &Arc<AddressDerivationManager>;

    fn account_index(&self) -> u64 {
        0
    }

    async fn derivation_scan(
        self: Arc<Self>,
        wallet_secret: Secret,
        _payment_secret: Option<Secret>,
        extent: usize,
        window: usize,
        sweep: bool,
        abortable: &Abortable,
        notifier: Option<DeepScanNotifier>,
    ) -> Result<()> {
        let derivation = self.derivation();

        let _prv_key_data = self.prv_key_data(wallet_secret).await?;
        let change_address = derivation.change_address_manager().current_address().await?;

        let mut index: usize = 0;
        let mut last_notification = 0;
        let mut aggregate_balance = 0;

        while index < extent {
            let first = index as u32;
            let last = (index + window) as u32;
            index = last as usize;

            // ----
            // - _keydata is initialized above ^
            // - TODO - generate pairs of private keys and addresses as a (Address, secp256k1::Secret) tuple without updating address indexes
            let mut keypairs = derivation.receive_address_manager().get_range(first..last).await?;
            let change_keypairs = derivation.change_address_manager().get_range(first..last).await?;
            keypairs.extend(change_keypairs);
            let keypairs: Vec<(Address, secp256k1::SecretKey)> =
                keypairs.into_iter().map(|address| (address.clone(), ONE_KEY)).collect();

            // ----

            let addresses = keypairs.iter().map(|(address, _)| address.clone()).collect::<Vec<_>>();
            let utxos = self.wallet().rpc().get_utxos_by_addresses(addresses).await?;
            let balance = utxos.iter().map(|utxo| utxo.utxo_entry.amount).sum::<u64>();
            if balance > 0 {
                aggregate_balance += balance;

                if sweep {
                    // TODO - populate with keypairs ^^^
                    let keydata: Vec<(Address, secp256k1::SecretKey)> = vec![];
                    let signer = Arc::new(KeydataSigner::new(keydata));

                    let utxos = utxos.into_iter().map(UtxoEntryReference::from).collect::<Vec<_>>();
                    let settings = GeneratorSettings::try_new_with_iterator(
                        Box::new(utxos.into_iter()),
                        None,
                        1,
                        1,
                        change_address.clone(),
                        PaymentDestination::Change,
                        Fees::None,
                        None,
                    )?;

                    let generator = Generator::new(settings, Some(signer), abortable);

                    let mut stream = generator.stream();
                    while let Some(transaction) = stream.try_next().await? {
                        transaction.try_sign()?;
                        let id = transaction.try_submit(self.wallet().rpc()).await?;
                        if let Some(notifier) = notifier.as_ref() {
                            notifier(index, balance, Some(id));
                        }
                        yield_executor().await;
                    }
                } else {
                    if let Some(notifier) = notifier.as_ref() {
                        notifier(index, aggregate_balance, None);
                    }
                    yield_executor().await;
                }
            }

            if index > last_notification + 1_000 {
                last_notification = index;
                if let Some(notifier) = notifier.as_ref() {
                    notifier(index, aggregate_balance, None);
                }
                yield_executor().await;
            }
        }

        Ok(())
    }

    async fn new_receive_address(self: Arc<Self>) -> Result<Address> {
        let address = self.derivation().receive_address_manager().new_address().await?;
        self.utxo_context().register_addresses(&[address.clone()]).await?;
        Ok(address)
    }

    async fn new_change_address(self: Arc<Self>) -> Result<Address> {
        let address = self.derivation().change_address_manager().new_address().await?;
        self.utxo_context().register_addresses(&[address.clone()]).await?;
        Ok(address)
    }

    fn cosigner_index(&self) -> u32 {
        0
    }

    fn create_private_keys<'l>(
        &self,
        keydata: &PrvKeyData,
        payment_secret: &Option<Secret>,
        receive: &[(&'l Address, u32)],
        change: &[(&'l Address, u32)],
    ) -> Result<Vec<(&'l Address, secp256k1::SecretKey)>> {
        let account_index = self.account_index();
        let cosigner_index = self.cosigner_index();

        let payload = keydata.payload.decrypt(payment_secret.as_ref())?;
        let xkey = payload.get_xprv(payment_secret.as_ref())?;

        let paths = build_derivate_paths(self.account_kind(), account_index, cosigner_index)?;
        let receive_xkey = xkey.clone().derive_path(paths.0)?;
        let change_xkey = xkey.derive_path(paths.1)?;

        let mut private_keys = vec![];
        for (address, index) in receive.iter() {
            private_keys.push((*address, *receive_xkey.derive_child(ChildNumber::new(*index, false)?)?.private_key()));
        }
        for (address, index) in change.iter() {
            private_keys.push((*address, *change_xkey.derive_child(ChildNumber::new(*index, false)?)?.private_key()));
        }

        create_private_keys(self.account_kind(), self.cosigner_index(), self.account_index(), keydata, payment_secret, receive, change)
    }
}

downcast_sync!(dyn DerivationCapableAccount);

pub fn create_private_keys<'l>(
    account_kind: AccountKind,
    cosigner_index: u32,
    account_index: u64,
    keydata: &PrvKeyData,
    payment_secret: &Option<Secret>,
    receive: &[(&'l Address, u32)],
    change: &[(&'l Address, u32)],
) -> Result<Vec<(&'l Address, secp256k1::SecretKey)>> {
    let payload = keydata.payload.decrypt(payment_secret.as_ref())?;
    let xkey = payload.get_xprv(payment_secret.as_ref())?;

    let paths = build_derivate_paths(account_kind, account_index, cosigner_index)?;
    let receive_xkey = xkey.clone().derive_path(paths.0)?;
    let change_xkey = xkey.derive_path(paths.1)?;

    let mut private_keys = vec![];
    for (address, index) in receive.iter() {
        private_keys.push((*address, *receive_xkey.derive_child(ChildNumber::new(*index, false)?)?.private_key()));
    }
    for (address, index) in change.iter() {
        private_keys.push((*address, *change_xkey.derive_child(ChildNumber::new(*index, false)?)?.private_key()));
    }

    Ok(private_keys)
}
