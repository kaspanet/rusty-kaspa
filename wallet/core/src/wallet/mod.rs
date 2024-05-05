//!
//! Kaspa wallet runtime implementation.
//!
pub mod api;
pub mod args;
pub mod maps;
pub use args::*;

use crate::account::ScanNotifier;
use crate::compat::gen1::decrypt_mnemonic;
use crate::error::Error::Custom;
use crate::factory::try_load_account;
use crate::imports::*;
use crate::settings::{SettingsStore, WalletSettings};
use crate::storage::interface::{OpenArgs, StorageDescriptor};
use crate::storage::local::interface::LocalStore;
use crate::storage::local::Storage;
use crate::wallet::maps::ActiveAccountMap;
use kaspa_bip32::{ExtendedKey, Language, Mnemonic, Prefix as KeyPrefix, WordCount};
use kaspa_notify::{
    listener::ListenerId,
    scope::{Scope, VirtualDaaScoreChangedScope},
};
use kaspa_wrpc_client::{KaspaRpcClient, Resolver, WrpcEncoding};
use workflow_core::task::spawn;

#[derive(Debug)]
pub struct EncryptedMnemonic<T: AsRef<[u8]>> {
    pub cipher: T, // raw
    pub salt: T,   // raw
}

#[derive(Debug)]
pub struct SingleWalletFileV0<'a, T: AsRef<[u8]>> {
    pub num_threads: u32,
    pub encrypted_mnemonic: EncryptedMnemonic<T>,
    pub xpublic_key: &'a str,
    pub ecdsa: bool,
}

#[derive(Debug)]
pub struct SingleWalletFileV1<'a, T: AsRef<[u8]>> {
    pub encrypted_mnemonic: EncryptedMnemonic<T>,
    pub xpublic_key: &'a str,
    pub ecdsa: bool,
}

impl<'a, T: AsRef<[u8]>> SingleWalletFileV1<'a, T> {
    const NUM_THREADS: u32 = 8;
}

#[derive(Debug)]
pub struct MultisigWalletFileV0<'a, T: AsRef<[u8]>> {
    pub num_threads: u32,
    pub encrypted_mnemonics: Vec<EncryptedMnemonic<T>>,
    pub xpublic_keys: Vec<&'a str>, // includes pub keys from encrypted
    pub required_signatures: u16,
    pub cosigner_index: u8,
    pub ecdsa: bool,
}

#[derive(Debug)]
pub struct MultisigWalletFileV1<'a, T: AsRef<[u8]>> {
    pub encrypted_mnemonics: Vec<EncryptedMnemonic<T>>,
    pub xpublic_keys: Vec<&'a str>, // includes pub keys from encrypted
    pub required_signatures: u16,
    pub cosigner_index: u8,
    pub ecdsa: bool,
}

impl<'a, T: AsRef<[u8]>> MultisigWalletFileV1<'a, T> {
    const NUM_THREADS: u32 = 8;
}

#[derive(Clone)]
pub enum WalletBusMessage {
    Discovery { record: TransactionRecord },
}

pub struct Inner {
    active_accounts: ActiveAccountMap,
    legacy_accounts: ActiveAccountMap,
    listener_id: Mutex<Option<ListenerId>>,
    task_ctl: DuplexChannel,
    selected_account: Mutex<Option<Arc<dyn Account>>>,
    store: Arc<dyn Interface>,
    settings: SettingsStore<WalletSettings>,
    utxo_processor: Arc<UtxoProcessor>,
    multiplexer: Multiplexer<Box<Events>>,
    wallet_bus: Channel<WalletBusMessage>,
    estimation_abortables: Mutex<HashMap<AccountId, Abortable>>,
    retained_contexts: Mutex<HashMap<String, Arc<Vec<u8>>>>,
}

///
/// `Wallet` represents a single wallet instance.
/// It is the main data structure responsible for
/// managing a runtime wallet.
///
/// @category Wallet API
///
#[derive(Clone)]
pub struct Wallet {
    inner: Arc<Inner>,
}

impl Wallet {
    pub fn local_store() -> Result<Arc<dyn Interface>> {
        Ok(Arc::new(LocalStore::try_new(false)?))
    }

    pub fn resident_store() -> Result<Arc<dyn Interface>> {
        Ok(Arc::new(LocalStore::try_new(true)?))
    }

    pub fn try_new(storage: Arc<dyn Interface>, resolver: Option<Resolver>, network_id: Option<NetworkId>) -> Result<Wallet> {
        Wallet::try_with_wrpc(storage, resolver, network_id)
    }

    pub fn try_with_wrpc(store: Arc<dyn Interface>, resolver: Option<Resolver>, network_id: Option<NetworkId>) -> Result<Wallet> {
        let rpc_client =
            Arc::new(KaspaRpcClient::new_with_args(WrpcEncoding::Borsh, Some("wrpc://127.0.0.1:17110"), resolver, network_id, None)?);

        // pub fn try_with_wrpc(store: Arc<dyn Interface>, network_id: Option<NetworkId>) -> Result<Wallet> {
        //     let rpc_client = Arc::new(KaspaRpcClient::new_with_args(
        //         WrpcEncoding::Borsh,
        //         NotificationMode::MultiListeners,
        //         "wrpc://127.0.0.1:17110",
        //         None,
        //     )?);

        let rpc_ctl = rpc_client.ctl().clone();
        let rpc_api: Arc<DynRpcApi> = rpc_client;
        let rpc = Rpc::new(rpc_api, rpc_ctl);
        Self::try_with_rpc(Some(rpc), store, network_id)
    }

    pub fn try_with_rpc(rpc: Option<Rpc>, store: Arc<dyn Interface>, network_id: Option<NetworkId>) -> Result<Wallet> {
        let multiplexer = Multiplexer::<Box<Events>>::new();
        let wallet_bus = Channel::unbounded();
        let utxo_processor =
            Arc::new(UtxoProcessor::new(rpc.clone(), network_id, Some(multiplexer.clone()), Some(wallet_bus.clone())));

        let wallet = Wallet {
            inner: Arc::new(Inner {
                multiplexer,
                store,
                active_accounts: ActiveAccountMap::default(),
                legacy_accounts: ActiveAccountMap::default(),
                listener_id: Mutex::new(None),
                task_ctl: DuplexChannel::oneshot(),
                selected_account: Mutex::new(None),
                settings: SettingsStore::new_with_storage(Storage::default_settings_store()),
                utxo_processor: utxo_processor.clone(),
                wallet_bus,
                estimation_abortables: Mutex::new(HashMap::new()),
                retained_contexts: Mutex::new(HashMap::new()),
            }),
        };

        Ok(wallet)
    }

    pub fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    pub fn is_resident(&self) -> Result<bool> {
        Ok(self.store().location()? == StorageDescriptor::Resident)
    }

    pub fn utxo_processor(&self) -> &Arc<UtxoProcessor> {
        &self.inner.utxo_processor
    }

    pub fn descriptor(&self) -> Option<WalletDescriptor> {
        self.store().descriptor()
    }

    pub fn store(&self) -> &Arc<dyn Interface> {
        &self.inner.store
    }

    pub fn active_accounts(&self) -> &ActiveAccountMap {
        &self.inner.active_accounts
    }
    pub fn legacy_accounts(&self) -> &ActiveAccountMap {
        &self.inner.legacy_accounts
    }

    pub async fn reset(self: &Arc<Self>, clear_legacy_cache: bool) -> Result<()> {
        self.utxo_processor().cleanup().await?;

        self.select(None).await?;

        let accounts = self.active_accounts().collect();
        let futures = accounts.into_iter().map(|account| account.stop());
        join_all(futures).await.into_iter().collect::<Result<Vec<_>>>()?;

        if clear_legacy_cache {
            self.legacy_accounts().clear();
        }

        Ok(())
    }

    pub async fn reload(self: &Arc<Self>, reactivate: bool) -> Result<()> {
        if self.is_open() {
            // similar to reset(), but effectively reboots the wallet

            let accounts = self.active_accounts().collect();
            let account_descriptors = Some(accounts.iter().map(|account| account.descriptor()).collect::<Result<Vec<_>>>()?);
            let wallet_descriptor = self.store().descriptor();

            // shutdown all accounts
            let futures = accounts.iter().map(|account| account.clone().stop());
            join_all(futures).await.into_iter().collect::<Result<Vec<_>>>()?;

            // reset utxo processor
            self.utxo_processor().cleanup().await?;

            // notify reload event
            self.notify(Events::WalletReload { wallet_descriptor, account_descriptors }).await?;

            // if `reactivate` is false, it is the responsibility of the client
            // to re-activate accounts. just like with WalletOpen, the client
            // should fetch transaction history and only then re-activate the accounts.

            if reactivate {
                // restarting accounts will post discovery and balance events
                let futures = accounts.into_iter().map(|account| account.start());
                join_all(futures).await.into_iter().collect::<Result<Vec<_>>>()?;
            }
        }

        Ok(())
    }

    pub async fn close(self: &Arc<Wallet>) -> Result<()> {
        if self.is_open() {
            self.reset(true).await?;
            self.store().close().await?;
            self.notify(Events::WalletClose).await?;
        }

        Ok(())
    }

    cfg_if! {
        if #[cfg(not(feature = "multi-user"))] {

            fn default_active_account(&self) -> Option<Arc<dyn Account>> {
                self.active_accounts().first()
            }

            /// For end-user wallets only - selects an account only if there
            /// is only a single account currently active in the wallet.
            /// Can be used to automatically select the default account.
            pub async fn autoselect_default_account_if_single(self: &Arc<Wallet>) -> Result<()> {
                if self.active_accounts().len() == 1 {
                    self.select(self.default_active_account().as_ref()).await?;
                }
                Ok(())
            }

            /// Select an account as 'active'. Supply `None` to remove active selection.
            pub async fn select(self: &Arc<Self>, account: Option<&Arc<dyn Account>>) -> Result<()> {
                *self.inner.selected_account.lock().unwrap() = account.cloned();
                if let Some(account) = account {
                    // log_info!("selecting account: {}", account.name_or_id());
                    account.clone().start().await?;
                    self.notify(Events::AccountSelection{ id : Some(*account.id()) }).await?;
                } else {
                    self.notify(Events::AccountSelection{ id : None }).await?;
                }
                Ok(())
            }

            /// Get currently selected account
            pub fn account(&self) -> Result<Arc<dyn Account>> {
                self.inner.selected_account.lock().unwrap().clone().ok_or_else(|| Error::AccountSelection)
            }



        }
    }

    /// Loads a wallet from storage. Accounts are not activated by this call.
    async fn open_impl(
        self: &Arc<Wallet>,
        wallet_secret: &Secret,
        filename: Option<String>,
        args: WalletOpenArgs,
    ) -> Result<Option<Vec<AccountDescriptor>>> {
        let filename = filename.or_else(|| self.settings().get(WalletSettings::Wallet));
        // let name = Some(make_filename(&name, &None));

        let was_open = self.is_open();

        self.store().open(wallet_secret, OpenArgs::new(filename)).await?;
        let wallet_name = self.store().descriptor();

        if was_open {
            self.notify(Events::WalletClose).await?;
        }

        // reset current state only after we have successfully opened another wallet
        self.reset(true).await?;

        let accounts: Option<Vec<Arc<dyn Account>>> = if args.load_account_descriptors() {
            let stored_accounts = self.inner.store.as_account_store().unwrap().iter(None).await?.try_collect::<Vec<_>>().await?;
            let stored_accounts = if !args.is_legacy_only() {
                stored_accounts
            } else {
                stored_accounts
                    .into_iter()
                    .filter(|(account_storage, _)| account_storage.kind.as_ref() == LEGACY_ACCOUNT_KIND)
                    .collect::<Vec<_>>()
            };
            Some(
                futures::stream::iter(stored_accounts.into_iter())
                    .then(|(account, meta)| try_load_account(self, account, meta))
                    .try_collect::<Vec<_>>()
                    // .try_collect::<Result<Vec<_>>>()
                    .await?,
            )
        } else {
            None
        };

        let account_descriptors = accounts
            .as_ref()
            .map(|accounts| accounts.iter().map(|account| account.descriptor()).collect::<Result<Vec<_>>>())
            .transpose()?;

        if let Some(accounts) = accounts {
            for account in accounts.into_iter() {
                if let Ok(legacy_account) = account.clone().as_legacy_account() {
                    self.legacy_accounts().insert(account);
                    legacy_account.create_private_context(wallet_secret, None, None).await?;
                }
            }
        }

        self.notify(Events::WalletOpen { wallet_descriptor: wallet_name, account_descriptors: account_descriptors.clone() }).await?;

        let hint = self.store().get_user_hint().await?;
        self.notify(Events::WalletHint { hint }).await?;

        Ok(account_descriptors)
    }

    /// Loads a wallet from storage. Accounts are not activated by this call.
    pub async fn open(
        self: &Arc<Wallet>,
        wallet_secret: &Secret,
        filename: Option<String>,
        args: WalletOpenArgs,
    ) -> Result<Option<Vec<AccountDescriptor>>> {
        // This is a wrapper of open_impl() that catches errors and notifies the UI
        match self.open_impl(wallet_secret, filename, args).await {
            Ok(account_descriptors) => Ok(account_descriptors),
            Err(err) => {
                self.notify(Events::WalletError { message: err.to_string() }).await?;
                Err(err)
            }
        }
    }

    async fn activate_accounts_impl(self: &Arc<Wallet>, account_ids: Option<&[AccountId]>) -> Result<Vec<AccountId>> {
        let stored_accounts = if let Some(ids) = account_ids {
            self.inner.store.as_account_store().unwrap().load_multiple(ids).await?
        } else {
            self.inner.store.as_account_store().unwrap().iter(None).await?.try_collect::<Vec<_>>().await?
        };

        let ids = stored_accounts.iter().map(|(account, _)| *account.id()).collect::<Vec<_>>();

        for (account_storage, meta) in stored_accounts.into_iter() {
            if account_storage.kind.as_ref() == LEGACY_ACCOUNT_KIND {
                let legacy_account = self
                    .legacy_accounts()
                    .get(account_storage.id())
                    .ok_or_else(|| Error::LegacyAccountNotInitialized)?
                    .clone()
                    .as_legacy_account()?;
                legacy_account.clone().start().await?;
                legacy_account.clear_private_context().await?;
            } else {
                let account = try_load_account(self, account_storage, meta).await?;
                account.clone().start().await?;
            }
        }

        self.notify(Events::AccountActivation { ids: ids.clone() }).await?;

        Ok(ids)
    }

    /// Activates accounts (performs account address space counts, initializes balance tracking, etc.)
    pub async fn activate_accounts(self: &Arc<Wallet>, account_ids: Option<&[AccountId]>) -> Result<()> {
        // This is a wrapper of activate_accounts_impl() that catches errors and notifies the UI
        if let Err(err) = self.activate_accounts_impl(account_ids).await {
            self.notify(Events::WalletError { message: err.to_string() }).await?;
            Err(err)
        } else {
            Ok(())
        }
    }

    pub async fn deactivate_accounts(self: &Arc<Wallet>, ids: Option<&[AccountId]>) -> Result<()> {
        let (ids, futures) = if let Some(ids) = ids {
            let accounts =
                ids.iter().map(|id| self.active_accounts().get(id).ok_or(Error::AccountNotFound(*id))).collect::<Result<Vec<_>>>()?;
            (ids.to_vec(), accounts.into_iter().map(|account| account.stop()).collect::<Vec<_>>())
        } else {
            self.active_accounts().collect().iter().map(|account| (account.id(), account.clone().stop())).unzip()
        };

        join_all(futures).await.into_iter().collect::<Result<Vec<_>>>()?;
        self.notify(Events::AccountDeactivation { ids }).await?;

        Ok(())
    }

    pub async fn account_descriptors(self: Arc<Self>) -> Result<Vec<AccountDescriptor>> {
        let iter = self.inner.store.as_account_store().unwrap().iter(None).await.unwrap();
        let wallet = self.clone();

        let stream = iter.then(move |stored| {
            let wallet = wallet.clone();

            async move {
                let (stored_account, stored_metadata) = stored.unwrap();
                if let Some(account) = wallet.legacy_accounts().get(&stored_account.id) {
                    account.descriptor()
                } else if let Some(account) = wallet.active_accounts().get(&stored_account.id) {
                    account.descriptor()
                } else {
                    try_load_account(&wallet, stored_account, stored_metadata).await?.descriptor()
                }
            }
        });

        stream.try_collect::<Vec<_>>().await
    }

    pub async fn get_prv_key_data(&self, wallet_secret: &Secret, id: &PrvKeyDataId) -> Result<Option<PrvKeyData>> {
        self.inner.store.as_prv_key_data_store()?.load_key_data(wallet_secret, id).await
    }

    pub async fn get_prv_key_info(&self, account: &Arc<dyn Account>) -> Result<Option<Arc<PrvKeyDataInfo>>> {
        self.inner.store.as_prv_key_data_store()?.load_key_info(account.prv_key_data_id()?).await
    }

    pub async fn is_account_key_encrypted(&self, account: &Arc<dyn Account>) -> Result<Option<bool>> {
        Ok(self.get_prv_key_info(account).await?.map(|info| info.is_encrypted()))
    }

    pub fn try_wrpc_client(&self) -> Option<Arc<KaspaRpcClient>> {
        self.try_rpc_api().and_then(|api| api.clone().downcast_arc::<KaspaRpcClient>().ok())
    }

    pub fn rpc_api(&self) -> Arc<DynRpcApi> {
        self.utxo_processor().rpc_api()
    }

    pub fn try_rpc_api(&self) -> Option<Arc<DynRpcApi>> {
        self.utxo_processor().try_rpc_api()
    }

    pub fn rpc_ctl(&self) -> RpcCtl {
        self.utxo_processor().rpc_ctl()
    }

    pub fn try_rpc_ctl(&self) -> Option<RpcCtl> {
        self.utxo_processor().try_rpc_ctl()
    }

    pub fn has_rpc(&self) -> bool {
        self.utxo_processor().has_rpc()
    }

    pub async fn bind_rpc(self: &Arc<Self>, rpc: Option<Rpc>) -> Result<()> {
        self.utxo_processor().bind_rpc(rpc).await?;
        Ok(())
    }

    pub fn multiplexer(&self) -> &Multiplexer<Box<Events>> {
        &self.inner.multiplexer
    }

    pub(crate) fn wallet_bus(&self) -> &Channel<WalletBusMessage> {
        &self.inner.wallet_bus
    }

    pub fn settings(&self) -> &SettingsStore<WalletSettings> {
        &self.inner.settings
    }

    pub fn current_daa_score(&self) -> Option<u64> {
        self.utxo_processor().current_daa_score()
    }

    pub async fn load_settings(&self) -> Result<()> {
        self.settings().try_load().await?;

        let settings = self.settings();

        if let Some(network_id) = settings.get(WalletSettings::Network) {
            self.set_network_id(&network_id).unwrap_or_else(|_| log_error!("Unable to select network type: `{}`", network_id));
        }

        if let Some(url) = settings.get::<String>(WalletSettings::Server) {
            if let Some(wrpc_client) = self.try_wrpc_client() {
                wrpc_client.set_url(Some(url.as_str())).unwrap_or_else(|_| log_error!("Unable to set rpc url: `{}`", url));
            }
        }

        Ok(())
    }

    // intended for starting async management tasks
    pub async fn start(self: &Arc<Self>) -> Result<()> {
        // self.load_settings().await.unwrap_or_else(|_| log_error!("Unable to load settings, discarding..."));

        // internal event loop
        self.start_task().await?;
        self.utxo_processor().start().await?;
        // rpc services (notifier)
        if let Some(rpc_client) = self.try_wrpc_client() {
            rpc_client.start().await?;
        }

        Ok(())
    }

    // intended for stopping async management task
    pub async fn stop(&self) -> Result<()> {
        self.utxo_processor().stop().await?;
        self.stop_task().await?;
        Ok(())
    }

    pub fn listener_id(&self) -> Result<ListenerId> {
        self.inner.listener_id.lock().unwrap().ok_or(Error::ListenerId)
    }

    pub async fn get_info(&self) -> Result<String> {
        let v = self.rpc_api().get_info().await?;
        Ok(format!("{v:#?}").replace('\n', "\r\n"))
    }

    pub async fn subscribe_daa_score(&self) -> Result<()> {
        self.rpc_api().start_notify(self.listener_id()?, Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    pub async fn unsubscribe_daa_score(&self) -> Result<()> {
        self.rpc_api().stop_notify(self.listener_id()?, Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    pub async fn broadcast(&self) -> Result<()> {
        Ok(())
    }

    pub fn set_network_id(&self, network_id: &NetworkId) -> Result<()> {
        if self.is_connected() {
            return Err(Error::NetworkTypeConnected);
        }
        self.utxo_processor().set_network_id(network_id);

        if let Some(wrpc_client) = self.try_wrpc_client() {
            wrpc_client.set_network_id(network_id)?;
        }
        Ok(())
    }

    pub fn network_id(&self) -> Result<NetworkId> {
        self.utxo_processor().network_id()
    }

    pub fn address_prefix(&self) -> Result<kaspa_addresses::Prefix> {
        Ok(self.network_id()?.into())
    }

    pub fn default_port(&self) -> Result<Option<u16>> {
        let network_type = self.network_id()?;
        if let Some(wrpc_client) = self.try_wrpc_client() {
            let port = match wrpc_client.encoding() {
                WrpcEncoding::Borsh => network_type.default_borsh_rpc_port(),
                WrpcEncoding::SerdeJson => network_type.default_json_rpc_port(),
            };
            Ok(Some(port))
        } else {
            Ok(None)
        }
    }

    pub async fn create_account(
        self: &Arc<Wallet>,
        wallet_secret: &Secret,
        account_create_args: AccountCreateArgs,
        notify: bool,
    ) -> Result<Arc<dyn Account>> {
        let account = match account_create_args {
            AccountCreateArgs::Bip32 { prv_key_data_args, account_args } => {
                let PrvKeyDataArgs { prv_key_data_id, payment_secret } = prv_key_data_args;
                self.create_account_bip32(wallet_secret, prv_key_data_id, payment_secret.as_ref(), account_args).await?
            }
            AccountCreateArgs::Legacy { prv_key_data_id, account_name } => {
                self.create_account_legacy(wallet_secret, prv_key_data_id, account_name).await?
            }
            AccountCreateArgs::Multisig { prv_key_data_args, additional_xpub_keys, name, minimum_signatures } => {
                self.create_account_multisig(wallet_secret, prv_key_data_args, additional_xpub_keys, name, minimum_signatures).await?
            }
        };

        if notify {
            let account_descriptor = account.descriptor()?;
            self.notify(Events::AccountCreate { account_descriptor }).await?;
        }

        Ok(account)
    }

    pub async fn create_account_multisig(
        self: &Arc<Wallet>,
        wallet_secret: &Secret,
        prv_key_data_args: Vec<PrvKeyDataArgs>,
        mut xpub_keys: Vec<String>,
        account_name: Option<String>,
        minimum_signatures: u16,
    ) -> Result<Arc<dyn Account>> {
        let account_store = self.inner.store.clone().as_account_store()?;

        let account: Arc<dyn Account> = if prv_key_data_args.is_not_empty() {
            let mut generated_xpubs = Vec::with_capacity(prv_key_data_args.len());
            let mut prv_key_data_ids = Vec::with_capacity(prv_key_data_args.len());
            for prv_key_data_arg in prv_key_data_args.into_iter() {
                let PrvKeyDataArgs { prv_key_data_id, payment_secret } = prv_key_data_arg;
                let prv_key_data = self
                    .inner
                    .store
                    .as_prv_key_data_store()?
                    .load_key_data(wallet_secret, &prv_key_data_id)
                    .await?
                    .ok_or_else(|| Error::PrivateKeyNotFound(prv_key_data_id))?;
                let xpub_key = prv_key_data.create_xpub(payment_secret.as_ref(), MULTISIG_ACCOUNT_KIND.into(), 0).await?; // todo it can be done concurrently
                generated_xpubs.push(xpub_key.to_string(Some(KeyPrefix::XPUB)));
                prv_key_data_ids.push(prv_key_data_id);
            }

            generated_xpubs.sort_unstable();
            xpub_keys.extend_from_slice(generated_xpubs.as_slice());
            xpub_keys.sort_unstable();

            let min_cosigner_index =
                generated_xpubs.first().and_then(|first_generated| xpub_keys.binary_search(first_generated).ok()).map(|v| v as u8);

            let xpub_keys = xpub_keys
                .into_iter()
                .map(|xpub_key| {
                    ExtendedPublicKeySecp256k1::from_str(&xpub_key).map_err(|err| Error::InvalidExtendedPublicKey(xpub_key, err))
                })
                .collect::<Result<Vec<_>>>()?;

            Arc::new(
                multisig::MultiSig::try_new(
                    self,
                    account_name,
                    Arc::new(xpub_keys),
                    Some(Arc::new(prv_key_data_ids)),
                    min_cosigner_index,
                    minimum_signatures,
                    false,
                )
                .await?,
            )
        } else {
            let xpub_keys = xpub_keys
                .into_iter()
                .map(|xpub_key| {
                    ExtendedPublicKeySecp256k1::from_str(&xpub_key).map_err(|err| Error::InvalidExtendedPublicKey(xpub_key, err))
                })
                .collect::<Result<Vec<_>>>()?;

            Arc::new(
                multisig::MultiSig::try_new(self, account_name, Arc::new(xpub_keys), None, None, minimum_signatures, false).await?,
            )
        };

        if account_store.load_single(account.id()).await?.is_some() {
            return Err(Error::AccountAlreadyExists(*account.id()));
        }

        self.inner.store.clone().as_account_store()?.store_single(&account.to_storage()?, None).await?;
        self.inner.store.commit(wallet_secret).await?;

        Ok(account)
    }

    pub async fn create_account_bip32(
        self: &Arc<Wallet>,
        wallet_secret: &Secret,
        prv_key_data_id: PrvKeyDataId,
        payment_secret: Option<&Secret>,
        account_args: AccountCreateArgsBip32,
    ) -> Result<Arc<dyn Account>> {
        let account_store = self.inner.store.clone().as_account_store()?;

        let prv_key_data = self
            .inner
            .store
            .as_prv_key_data_store()?
            .load_key_data(wallet_secret, &prv_key_data_id)
            .await?
            .ok_or_else(|| Error::PrivateKeyNotFound(prv_key_data_id))?;

        let AccountCreateArgsBip32 { account_name, account_index } = account_args;

        let account_index = if let Some(account_index) = account_index {
            account_index
        } else {
            account_store.clone().len(Some(prv_key_data_id)).await? as u64
        };

        let xpub_key = prv_key_data.create_xpub(payment_secret, BIP32_ACCOUNT_KIND.into(), account_index).await?;
        let xpub_keys = Arc::new(vec![xpub_key]);

        let account: Arc<dyn Account> =
            Arc::new(bip32::Bip32::try_new(self, account_name, prv_key_data.id, account_index, xpub_keys, false).await?);

        if account_store.load_single(account.id()).await?.is_some() {
            return Err(Error::AccountAlreadyExists(*account.id()));
        }

        self.inner.store.clone().as_account_store()?.store_single(&account.to_storage()?, None).await?;
        self.inner.store.commit(wallet_secret).await?;

        Ok(account)
    }

    async fn create_account_legacy(
        self: &Arc<Wallet>,
        wallet_secret: &Secret,
        prv_key_data_id: PrvKeyDataId,
        account_name: Option<String>,
    ) -> Result<Arc<dyn Account>> {
        let account_store = self.inner.store.clone().as_account_store()?;

        let prv_key_data = self
            .inner
            .store
            .as_prv_key_data_store()?
            .load_key_data(wallet_secret, &prv_key_data_id)
            .await?
            .ok_or_else(|| Error::PrivateKeyNotFound(prv_key_data_id))?;

        let account: Arc<dyn Account> = Arc::new(legacy::Legacy::try_new(self, account_name, prv_key_data.id).await?);

        if account_store.load_single(account.id()).await?.is_some() {
            return Err(Error::AccountAlreadyExists(*account.id()));
        }

        self.inner.store.clone().as_account_store()?.store_single(&account.to_storage()?, None).await?;
        self.inner.store.commit(wallet_secret).await?;

        Ok(account)
    }

    pub async fn create_wallet(
        self: &Arc<Wallet>,
        wallet_secret: &Secret,
        args: WalletCreateArgs,
    ) -> Result<(WalletDescriptor, StorageDescriptor)> {
        self.close().await?;

        let wallet_descriptor = self.inner.store.create(wallet_secret, args.into()).await?;
        let storage_descriptor = self.inner.store.location()?;
        self.inner.store.commit(wallet_secret).await?;

        self.notify(Events::WalletCreate {
            wallet_descriptor: wallet_descriptor.clone(),
            storage_descriptor: storage_descriptor.clone(),
        })
        .await?;

        Ok((wallet_descriptor, storage_descriptor))
    }

    pub async fn create_prv_key_data(
        self: &Arc<Wallet>,
        wallet_secret: &Secret,
        prv_key_data_create_args: PrvKeyDataCreateArgs,
    ) -> Result<PrvKeyDataId> {
        let mnemonic = Mnemonic::new(prv_key_data_create_args.mnemonic.as_str()?, Language::default())?;
        let prv_key_data = PrvKeyData::try_from_mnemonic(
            mnemonic.clone(),
            prv_key_data_create_args.payment_secret.as_ref(),
            self.store().encryption_kind()?,
        )?;
        let prv_key_data_info = PrvKeyDataInfo::from(prv_key_data.as_ref());
        let prv_key_data_id = prv_key_data.id;
        let prv_key_data_store = self.inner.store.as_prv_key_data_store()?;
        prv_key_data_store.store(wallet_secret, prv_key_data).await?;
        self.inner.store.commit(wallet_secret).await?;

        self.notify(Events::PrvKeyDataCreate { prv_key_data_info }).await?;

        Ok(prv_key_data_id)
    }

    pub async fn create_wallet_with_accounts(
        self: &Arc<Wallet>,
        wallet_secret: &Secret,
        wallet_args: WalletCreateArgs,
        account_name: Option<String>,
        account_kind: Option<AccountKind>,
        mnemonic_phrase_word_count: WordCount,
        payment_secret: Option<Secret>,
    ) -> Result<(WalletDescriptor, StorageDescriptor, Mnemonic, Arc<dyn Account>)> {
        self.close().await?;

        let encryption_kind = wallet_args.encryption_kind;
        let wallet_descriptor = self.inner.store.create(wallet_secret, wallet_args.into()).await?;
        let storage_descriptor = self.inner.store.location()?;
        let mnemonic = Mnemonic::random(mnemonic_phrase_word_count, Default::default())?;
        let account_index = 0;
        let prv_key_data = PrvKeyData::try_from_mnemonic(mnemonic.clone(), payment_secret.as_ref(), encryption_kind)?;
        let xpub_key = prv_key_data
            .create_xpub(payment_secret.as_ref(), account_kind.unwrap_or(BIP32_ACCOUNT_KIND.into()), account_index)
            .await?;
        let xpub_keys = Arc::new(vec![xpub_key]);

        let account: Arc<dyn Account> =
            Arc::new(bip32::Bip32::try_new(self, account_name, prv_key_data.id, account_index, xpub_keys, false).await?);

        let prv_key_data_store = self.inner.store.as_prv_key_data_store()?;
        prv_key_data_store.store(wallet_secret, prv_key_data).await?;
        self.inner.store.clone().as_account_store()?.store_single(&account.to_storage()?, None).await?;
        self.inner.store.commit(wallet_secret).await?;

        self.select(Some(&account)).await?;
        Ok((wallet_descriptor, storage_descriptor, mnemonic, account))
    }

    pub async fn get_account_by_id(self: &Arc<Self>, account_id: &AccountId) -> Result<Option<Arc<dyn Account>>> {
        if let Some(account) = self.active_accounts().get(account_id) {
            Ok(Some(account.clone()))
        } else {
            let account_storage = self.inner.store.as_account_store()?;
            let stored = account_storage.load_single(account_id).await?;
            if let Some((stored_account, stored_metadata)) = stored {
                let account = try_load_account(self, stored_account, stored_metadata).await?;
                Ok(Some(account))
            } else {
                Ok(None)
            }
        }
    }

    pub async fn notify(&self, event: Events) -> Result<()> {
        self.multiplexer()
            .try_broadcast(Box::new(event))
            .map_err(|_| Error::Custom("multiplexer channel error during update_balance".to_string()))?;
        Ok(())
    }

    pub fn is_synced(&self) -> bool {
        self.utxo_processor().is_synced()
    }

    pub fn is_connected(&self) -> bool {
        self.utxo_processor().is_connected()
    }

    pub(crate) async fn handle_discovery(&self, record: TransactionRecord) -> Result<()> {
        let transaction_store = self.store().as_transaction_record_store()?;

        if let Err(_err) = transaction_store.load_single(record.binding(), &self.network_id()?, record.id()).await {
            let transaction_daa_score = record.block_daa_score();
            match self.rpc_api().get_daa_score_timestamp_estimate(vec![transaction_daa_score]).await {
                Ok(timestamps) => {
                    if let Some(timestamp) = timestamps.first() {
                        let mut record = record.clone();
                        record.set_unixtime(*timestamp);

                        transaction_store.store(&[&record]).await?;

                        self.notify(Events::Discovery { record }).await?;
                    } else {
                        self.notify(Events::Error {
                            message: format!(
                                "Unable to obtain DAA to unixtime for DAA {transaction_daa_score}, timestamp data is empty"
                            ),
                        })
                        .await?;
                    }
                }
                Err(err) => {
                    self.notify(Events::Error { message: format!("Unable to resolve DAA to unixtime: {err}") }).await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_wallet_bus(self: &Arc<Self>, message: WalletBusMessage) -> Result<()> {
        match message {
            WalletBusMessage::Discovery { record } => {
                self.handle_discovery(record).await?;
            }
        }
        Ok(())
    }

    async fn handle_event(self: &Arc<Self>, event: Box<Events>) -> Result<()> {
        match &*event {
            Events::Pending { record } | Events::Maturity { record } | Events::Reorg { record } => {
                if !record.is_change() {
                    self.store().as_transaction_record_store()?.store(&[record]).await?;
                }
            }

            _ => {}
        }

        Ok(())
    }

    async fn start_task(self: &Arc<Self>) -> Result<()> {
        let this = self.clone();
        let task_ctl_receiver = self.inner.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.inner.task_ctl.response.sender.clone();
        let events = self.multiplexer().channel();
        let wallet_bus_receiver = self.wallet_bus().receiver.clone();

        // let this_clone = self.clone();
        // spawn(async move {
        //     loop {
        //         log_info!("Wallet broadcasting ping...");
        //         this_clone.notify(Events::WalletPing).await.expect("Wallet::start_task() `notify` error");
        //         sleep(Duration::from_secs(5)).await;
        //     }
        // });

        spawn(async move {
            loop {
                select! {
                    _ = task_ctl_receiver.recv().fuse() => {
                        break;
                    },

                    msg = events.receiver.recv().fuse() => {
                        match msg {
                            Ok(event) => {
                                this.handle_event(event).await.unwrap_or_else(|e| log_error!("Wallet::handle_event() error: {}", e));
                            },
                            Err(err) => {
                                log_error!("Wallet: error while receiving multiplexer message: {err}");
                                log_error!("Suspending Wallet processing...");

                                break;
                            }
                        }
                    },

                    msg = wallet_bus_receiver.recv().fuse() => {
                        match msg {
                            Ok(message) => {
                                this.handle_wallet_bus(message).await.unwrap_or_else(|e| log_error!("Wallet::handle_wallet_bus() error: {}", e));
                            },
                            Err(err) => {
                                log_error!("Wallet: error while receiving wallet bus message: {err}");
                                log_error!("Suspending Wallet processing...");

                                break;
                            }
                        }
                    }
                }
            }

            task_ctl_sender.send(()).await.unwrap();
        });
        Ok(())
    }

    async fn stop_task(&self) -> Result<()> {
        self.inner.task_ctl.signal(()).await.expect("Wallet::stop_task() `signal` error");
        Ok(())
    }

    pub fn enable_metrics_kinds(&self, kinds: &[MetricsUpdateKind]) {
        self.utxo_processor().enable_metrics_kinds(kinds);
    }

    pub async fn start_metrics(&self) -> Result<()> {
        self.utxo_processor().start_metrics().await?;
        Ok(())
    }

    pub async fn stop_metrics(&self) -> Result<()> {
        self.utxo_processor().stop_metrics().await?;
        Ok(())
    }

    pub fn is_open(&self) -> bool {
        self.inner.store.is_open()
    }

    pub fn location(&self) -> Result<StorageDescriptor> {
        self.inner.store.location()
    }

    pub async fn exists(&self, name: Option<&str>) -> Result<bool> {
        self.inner.store.exists(name).await
    }

    pub async fn keys(&self) -> Result<impl Stream<Item = Result<Arc<PrvKeyDataInfo>>>> {
        self.inner.store.as_prv_key_data_store()?.iter().await
    }

    pub async fn find_accounts_by_name_or_id(&self, pat: &str) -> Result<Vec<Arc<dyn Account>>> {
        let active_accounts = self.active_accounts().inner().values().cloned().collect::<Vec<_>>();
        let matches = active_accounts
            .into_iter()
            .filter(|account| {
                account.name().map(|name| name.starts_with(pat)).unwrap_or(false) || account.id().to_hex().starts_with(pat)
            })
            .collect::<Vec<_>>();
        Ok(matches)
    }

    pub async fn accounts(self: &Arc<Self>, filter: Option<PrvKeyDataId>) -> Result<impl Stream<Item = Result<Arc<dyn Account>>>> {
        let iter = self.inner.store.as_account_store().unwrap().iter(filter).await.unwrap();
        let wallet = self.clone();

        let stream = iter.then(move |stored| {
            let wallet = wallet.clone();

            async move {
                let (stored_account, stored_metadata) = stored.unwrap();
                if let Some(account) = wallet.legacy_accounts().get(&stored_account.id) {
                    if !wallet.active_accounts().contains(account.id()) {
                        account.clone().start().await?;
                    }
                    Ok(account)
                } else if let Some(account) = wallet.active_accounts().get(&stored_account.id) {
                    Ok(account)
                } else {
                    let account = try_load_account(&wallet, stored_account, stored_metadata).await?;
                    account.clone().start().await?;
                    Ok(account)
                }
            }
        });

        Ok(Box::pin(stream))
    }

    // TODO - remove these comments (these functions are a part of
    // a major refactoring and are temporarily kept here for reference)

    // pub async fn initialize_legacy_accounts(
    //     self: &Arc<Self>,
    //     filter: Option<PrvKeyDataId>,
    //     secret: Secret,
    // ) -> Result<()> {
    //     let mut iter = self.inner.store.as_account_store().unwrap().iter(filter).await.unwrap();
    //     let wallet = self.clone();

    //     while let Some((stored_account, stored_metadata)) = iter.try_next().await? {
    //         if matches!(stored_account.data, AccountData::Legacy { .. }) {

    //             let account = try_from_storage(&wallet, stored_account, stored_metadata).await?;

    //                 account.clone().initialize_private_data(secret.clone(), None, None).await?;
    //                 wallet.legacy_accounts().insert(account.clone());
    //                 // account.clone().start().await?;

    //             // if is_legacy {
    //                 // let derivation = account.clone().as_derivation_capable()?.derivation();
    //                 // let m = derivation.receive_address_manager();
    //                 // m.get_range(0..(m.index() + CACHE_ADDRESS_OFFSET))?;
    //                 // let m = derivation.change_address_manager();
    //                 // m.get_range(0..(m.index() + CACHE_ADDRESS_OFFSET))?;

    //                 // - TODO - consider two-phase approach
    //                 // account.clone().clear_private_data().await?;
    //             // }
    //         }
    //     }

    //     Ok(())

    // // let stream = iter.then(move |stored| {
    //     let wallet = wallet.clone();
    //     let secret = secret.clone();

    //     // async move {
    //         let (stored_account, stored_metadata) = stored.unwrap();
    //         // if let Some(account) = wallet.active_accounts().get(&stored_account.id) {
    //             // Ok(account)
    //         // } else {
    //             if matches!(stored_account.data, AccountData::Legacy { .. }) {

    //                 let account = try_from_storage(&wallet, stored_account, stored_metadata).await?;

    //                 // if is_legacy {
    //                     account.clone().initialize_private_data(secret, None, None).await?;
    //                     wallet.legacy_accounts().insert(account.clone());
    //                 // }

    //                 // account.clone().start().await?;

    //                 // if is_legacy {
    //                     let derivation = account.clone().as_derivation_capable()?.derivation();
    //                     let m = derivation.receive_address_manager();
    //                     m.get_range(0..(m.index() + CACHE_ADDRESS_OFFSET))?;
    //                     let m = derivation.change_address_manager();
    //                     m.get_range(0..(m.index() + CACHE_ADDRESS_OFFSET))?;
    //                     account.clone().clear_private_data().await?;
    //                 // }
    //             }

    // Ok(account)
    // }
    // }
    // });
    // Ok(Box::pin(stream))
    // }

    // pub async fn initialize_accounts(
    //     self: &Arc<Self>,
    //     filter: Option<PrvKeyDataId>,
    //     secret: Secret,
    // ) -> Result<impl Stream<Item = Result<Arc<dyn Account>>>> {
    //     let iter = self.inner.store.as_account_store().unwrap().iter(filter).await.unwrap();
    //     let wallet = self.clone();

    //     let stream = iter.then(move |stored| {
    //         let wallet = wallet.clone();
    //         let secret = secret.clone();

    //         async move {
    //             let (stored_account, stored_metadata) = stored.unwrap();
    //             if let Some(account) = wallet.active_accounts().get(&stored_account.id) {
    //                 Ok(account)
    //             } else {
    //                 let is_legacy = matches!(stored_account.data, AccountData::Legacy { .. });
    //                 let account = try_from_storage(&wallet, stored_account, stored_metadata).await?;

    //                 if is_legacy {
    //                     account.clone().initialize_private_data(secret, None, None).await?;
    //                     wallet.legacy_accounts().insert(account.clone());
    //                 }

    //                 // account.clone().start().await?;

    //                 if is_legacy {
    //                     let derivation = account.clone().as_derivation_capable()?.derivation();
    //                     let m = derivation.receive_address_manager();
    //                     m.get_range(0..(m.index() + CACHE_ADDRESS_OFFSET))?;
    //                     let m = derivation.change_address_manager();
    //                     m.get_range(0..(m.index() + CACHE_ADDRESS_OFFSET))?;
    //                     account.clone().clear_private_data().await?;
    //                 }

    //                 Ok(account)
    //             }
    //         }
    //     });

    //     Ok(Box::pin(stream))
    // }

    pub async fn import_kaspawallet_golang_single_v1<T: AsRef<[u8]>>(
        self: &Arc<Wallet>,
        import_secret: &Secret,
        wallet_secret: &Secret,
        file: SingleWalletFileV1<'_, T>,
    ) -> Result<Arc<dyn Account>> {
        if file.ecdsa {
            return Err(Error::Custom("ecdsa currently not suppoerted".to_owned()));
            // todo import_with_mnemonic should accept both
        }
        let mnemonic = decrypt_mnemonic(SingleWalletFileV1::<T>::NUM_THREADS, file.encrypted_mnemonic, import_secret.as_ref())?;
        let mnemonic = Mnemonic::new(mnemonic.trim(), Language::English)?;
        let prv_key_data = storage::PrvKeyData::try_new_from_mnemonic(mnemonic.clone(), None, self.store().encryption_kind()?)?;
        let prefix = file.xpublic_key.split_at(kaspa_bip32::Prefix::LENGTH).0;
        let prefix = kaspa_bip32::Prefix::try_from(prefix)?;

        if prv_key_data.create_xpub(None, BIP32_ACCOUNT_KIND.into(), 0).await?.to_string(Some(prefix)) != file.xpublic_key {
            return Err(Custom("imported xpub does not equal derived one".to_owned()));
        }
        self.import_with_mnemonic(wallet_secret, None, mnemonic, BIP32_ACCOUNT_KIND.into()).await
    }

    pub async fn import_kaspawallet_golang_single_v0<T: AsRef<[u8]>>(
        self: &Arc<Wallet>,
        import_secret: &Secret,
        wallet_secret: &Secret,
        file: SingleWalletFileV0<'_, T>,
    ) -> Result<Arc<dyn Account>> {
        if file.ecdsa {
            return Err(Error::Custom("ecdsa currently not suppoerted".to_owned()));
            // todo import_with_mnemonic should accept both
        }
        let mnemonic = decrypt_mnemonic(file.num_threads, file.encrypted_mnemonic, import_secret.as_ref())?;
        let mnemonic = Mnemonic::new(mnemonic.trim(), Language::English)?;
        let prv_key_data = storage::PrvKeyData::try_new_from_mnemonic(mnemonic.clone(), None, self.store().encryption_kind()?)?;
        let prefix = file.xpublic_key.split_at(kaspa_bip32::Prefix::LENGTH).0;
        let prefix = kaspa_bip32::Prefix::try_from(prefix)?;
        if prv_key_data.create_xpub(None, BIP32_ACCOUNT_KIND.into(), 0).await.unwrap().to_string(Some(prefix)) != file.xpublic_key {
            return Err(Custom("imported xpub does not equal derived one".to_owned()));
        }
        self.import_with_mnemonic(wallet_secret, None, mnemonic, BIP32_ACCOUNT_KIND.into()).await
    }

    pub async fn import_kaspawallet_golang_multisig_v0<T: AsRef<[u8]>>(
        self: &Arc<Wallet>,
        import_secret: &Secret,
        wallet_secret: &Secret,
        file: MultisigWalletFileV0<'_, T>,
    ) -> Result<Arc<dyn Account>> {
        if file.ecdsa {
            return Err(Error::Custom("ecdsa currently not suppoerted".to_owned()));
            // todo import_with_mnemonic should accept both
        }
        let Some(first_pub_key) = file.xpublic_keys.first() else {
            return Err(Error::Custom("no public keys".to_owned()));
        };
        let prefix = first_pub_key.split_at(kaspa_bip32::Prefix::LENGTH).0;
        let prefix = kaspa_bip32::Prefix::try_from(prefix)?;

        let mnemonics_and_secrets: Vec<(Mnemonic, Option<Secret>)> = file
            .encrypted_mnemonics
            .into_iter()
            .map(|mnemonic| {
                decrypt_mnemonic(file.num_threads, mnemonic, import_secret.as_ref())
                    .map_err(Error::from)
                    .and_then(|decrypted| Mnemonic::new(decrypted.trim(), Language::English).map_err(Error::from))
            })
            .map(|r| r.map(|m| (m, <Option<Secret>>::None)))
            .collect::<Result<Vec<(Mnemonic, Option<Secret>)>>>()?;

        let mut all_pub_keys = file.xpublic_keys;
        all_pub_keys.sort_unstable();

        let mut pubkeys_from_mnemonics = Vec::with_capacity(mnemonics_and_secrets.len());
        for (mnemonic, _) in mnemonics_and_secrets.iter() {
            let priv_key = storage::PrvKeyData::try_new_from_mnemonic(mnemonic.clone(), None, self.store().encryption_kind()?)?;
            let xpub_key = priv_key.create_xpub(None, BIP32_ACCOUNT_KIND.into(), 0).await.unwrap().to_string(Some(prefix));
            pubkeys_from_mnemonics.push(xpub_key);
        }
        pubkeys_from_mnemonics.sort_unstable();
        all_pub_keys.retain(|v| pubkeys_from_mnemonics.binary_search_by_key(v, |xpub| xpub.as_str()).is_err());
        let additional_pub_keys = all_pub_keys.into_iter().map(String::from).collect();
        self.import_multisig_with_mnemonic(wallet_secret, mnemonics_and_secrets, file.required_signatures, additional_pub_keys).await
    }

    pub async fn import_kaspawallet_golang_multisig_v1<T: AsRef<[u8]>>(
        self: &Arc<Wallet>,
        import_secret: &Secret,
        wallet_secret: &Secret,
        file: MultisigWalletFileV1<'_, T>,
    ) -> Result<Arc<dyn Account>> {
        if file.ecdsa {
            return Err(Error::Custom("ecdsa currently not suppoerted".to_owned()));
            // todo import_with_mnemonic should accept both
        }
        let Some(first_pub_key) = file.xpublic_keys.first() else {
            return Err(Error::Custom("no public keys".to_owned()));
        };
        let prefix = first_pub_key.split_at(kaspa_bip32::Prefix::LENGTH).0;
        let prefix = kaspa_bip32::Prefix::try_from(prefix)?;

        let mnemonics_and_secrets: Vec<(Mnemonic, Option<Secret>)> = file
            .encrypted_mnemonics
            .into_iter()
            .map(|mnemonic| {
                decrypt_mnemonic(MultisigWalletFileV1::<T>::NUM_THREADS, mnemonic, import_secret.as_ref())
                    .map_err(Error::from)
                    .and_then(|decrypted| Mnemonic::new(decrypted.trim(), Language::English).map_err(Error::from))
            })
            .map(|r| r.map(|m| (m, <Option<Secret>>::None)))
            .collect::<Result<Vec<(Mnemonic, Option<Secret>)>>>()?;

        let mut all_pub_keys = file.xpublic_keys;
        all_pub_keys.sort_unstable_by(|left, right| {
            left.split_at(kaspa_bip32::Prefix::LENGTH).1.cmp(right.split_at(kaspa_bip32::Prefix::LENGTH).1)
        });

        let mut pubkeys_from_mnemonics = Vec::with_capacity(mnemonics_and_secrets.len());
        for (mnemonic, _) in mnemonics_and_secrets.iter() {
            let priv_key = storage::PrvKeyData::try_new_from_mnemonic(mnemonic.clone(), None, self.store().encryption_kind()?)?;
            let xpub_key = priv_key.create_xpub(None, MULTISIG_ACCOUNT_KIND.into(), 0).await.unwrap().to_string(Some(prefix));
            pubkeys_from_mnemonics.push(xpub_key);
        }
        pubkeys_from_mnemonics.sort_unstable_by(|left, right| {
            left.split_at(kaspa_bip32::Prefix::LENGTH).1.cmp(right.split_at(kaspa_bip32::Prefix::LENGTH).1)
        });
        all_pub_keys.retain(|v| {
            let found = pubkeys_from_mnemonics.binary_search_by_key(v, |xpub| xpub.as_str());
            found.is_err()
        });
        let additional_pub_keys = all_pub_keys.into_iter().map(String::from).collect();
        let acc = self
            .import_multisig_with_mnemonic(wallet_secret, mnemonics_and_secrets, file.required_signatures, additional_pub_keys)
            .await?;
        Ok(acc)
    }

    pub async fn import_legacy_keydata(
        self: &Arc<Wallet>,
        import_secret: &Secret,
        wallet_secret: &Secret,
        payment_secret: Option<&Secret>,
        notifier: Option<ScanNotifier>,
    ) -> Result<Arc<dyn Account>> {
        use crate::compat::gen0::load_v0_keydata;

        let notifier = notifier.as_ref();
        let keydata = load_v0_keydata(import_secret).await?;

        let mnemonic = Mnemonic::new(keydata.mnemonic.trim(), Language::English)?;
        let prv_key_data = PrvKeyData::try_new_from_mnemonic(mnemonic, payment_secret, self.store().encryption_kind()?)?;
        let prv_key_data_store = self.inner.store.as_prv_key_data_store()?;
        if prv_key_data_store.load_key_data(wallet_secret, &prv_key_data.id).await?.is_some() {
            return Err(Error::PrivateKeyAlreadyExists(prv_key_data.id));
        }

        let account: Arc<dyn Account> = Arc::new(legacy::Legacy::try_new(self, None, prv_key_data.id).await?);

        // activate account (add it to wallet active account list)
        self.active_accounts().insert(account.clone().as_dyn_arc());
        self.legacy_accounts().insert(account.clone().as_dyn_arc());

        // store private key and account
        self.inner.store.batch().await?;
        prv_key_data_store.store(wallet_secret, prv_key_data).await?;
        self.inner.store.clone().as_account_store()?.store_single(&account.to_storage()?, None).await?;
        self.inner.store.flush(wallet_secret).await?;

        let legacy_account = account.clone().as_legacy_account()?;
        legacy_account.create_private_context(wallet_secret, payment_secret, None).await?;
        // account.clone().initialize_private_data(wallet_secret, payment_secret, None).await?;

        if self.is_connected() {
            if let Some(notifier) = notifier {
                notifier(0, 0, 0, None);
            }
            account.clone().scan(Some(100), Some(5000)).await?;
        }

        // let derivation = account.clone().as_derivation_capable()?.derivation();
        // let m = derivation.receive_address_manager();
        // m.get_range(0..(m.index() + CACHE_ADDRESS_OFFSET))?;
        // let m = derivation.change_address_manager();
        // m.get_range(0..(m.index() + CACHE_ADDRESS_OFFSET))?;
        // account.clone().clear_private_data().await?;

        legacy_account.clear_private_context().await?;

        Ok(account)
    }

    pub async fn import_gen1_keydata(self: &Arc<Wallet>, _secret: Secret) -> Result<()> {
        // use crate::derivation::gen1::import::load_v1_keydata;

        // let _keydata = load_v1_keydata(&secret).await?;
        todo!();
        // Ok(())
    }

    pub async fn import_with_mnemonic(
        self: &Arc<Wallet>,
        wallet_secret: &Secret,
        payment_secret: Option<&Secret>,
        mnemonic: Mnemonic,
        account_kind: AccountKind,
    ) -> Result<Arc<dyn Account>> {
        let prv_key_data = storage::PrvKeyData::try_new_from_mnemonic(mnemonic, payment_secret, self.store().encryption_kind()?)?;
        let prv_key_data_store = self.store().as_prv_key_data_store()?;
        if prv_key_data_store.load_key_data(wallet_secret, &prv_key_data.id).await?.is_some() {
            return Err(Error::PrivateKeyAlreadyExists(prv_key_data.id));
        }
        // let mut is_legacy = false;
        let account: Arc<dyn Account> = match account_kind.as_ref() {
            BIP32_ACCOUNT_KIND => {
                let account_index = 0;
                let xpub_key = prv_key_data.create_xpub(payment_secret, account_kind, account_index).await?;
                let xpub_keys = Arc::new(vec![xpub_key]);
                let ecdsa = false;
                // ---
                Arc::new(bip32::Bip32::try_new(self, None, prv_key_data.id, account_index, xpub_keys, ecdsa).await?)
            }
            LEGACY_ACCOUNT_KIND => Arc::new(legacy::Legacy::try_new(self, None, prv_key_data.id).await?),
            _ => {
                return Err(Error::AccountKindFeature);
            }
        };

        let account_store = self.inner.store.as_account_store()?;
        self.inner.store.batch().await?;
        account_store.store_single(&account.to_storage()?, None).await?;
        self.inner.store.flush(wallet_secret).await?;

        if let Ok(legacy_account) = account.clone().as_legacy_account() {
            self.legacy_accounts().insert(account.clone());
            legacy_account.create_private_context(wallet_secret, None, None).await?;
            legacy_account.clone().start().await?;
            legacy_account.clear_private_context().await?;
        } else {
            account.clone().start().await?;
        }

        // if is_legacy {
        //     account.clone().initialize_private_data(wallet_secret, None, None).await?;
        //     self.legacy_accounts().insert(account.clone());
        // }
        // account.clone().start().await?;
        // if is_legacy {
        //     let derivation = account.clone().as_derivation_capable()?.derivation();
        //     let m = derivation.receive_address_manager();
        //     m.get_range(0..(m.index() + CACHE_ADDRESS_OFFSET))?;
        //     let m = derivation.change_address_manager();
        //     m.get_range(0..(m.index() + CACHE_ADDRESS_OFFSET))?;
        //     account.clone().clear_private_data().await?;
        // }

        Ok(account)
    }

    /// Perform a "2d" scan of account derivations while scanning addresses
    /// in each account (UTXOs up to `address_scan_extent` address derivation).
    /// Report back the last account index that has UTXOs. The scan is performed
    /// until we have encountered at least `account_scan_extent` of empty
    /// accounts.
    pub async fn scan_bip44_accounts(
        self: &Arc<Self>,
        bip39_mnemonic: Secret,
        bip39_passphrase: Option<Secret>,
        address_scan_extent: u32,
        account_scan_extent: u32,
    ) -> Result<u32> {
        let bip39_mnemonic = std::str::from_utf8(bip39_mnemonic.as_ref()).map_err(|_| Error::InvalidMnemonicPhrase)?;
        let mnemonic = Mnemonic::new(bip39_mnemonic, Language::English)?;

        // TODO @aspect - this is not efficient, we need to scan without encrypting prv_key_data
        let prv_key_data =
            storage::PrvKeyData::try_new_from_mnemonic(mnemonic, bip39_passphrase.as_ref(), EncryptionKind::XChaCha20Poly1305)?;

        let mut last_account_index = 0;
        let mut account_index = 0;

        while account_index < last_account_index + account_scan_extent {
            let xpub_key =
                prv_key_data.create_xpub(bip39_passphrase.as_ref(), BIP32_ACCOUNT_KIND.into(), account_index as u64).await?;
            let xpub_keys = Arc::new(vec![xpub_key]);
            let ecdsa = false;
            // ---

            let addresses = bip32::Bip32::try_new(self, None, prv_key_data.id, account_index as u64, xpub_keys, ecdsa)
                .await?
                .get_address_range_for_scan(0..address_scan_extent)?;
            if self.rpc_api().get_utxos_by_addresses(addresses).await?.is_not_empty() {
                last_account_index = account_index;
            }
            account_index += 1;
        }

        Ok(last_account_index)
    }

    pub async fn import_multisig_with_mnemonic(
        self: &Arc<Wallet>,
        wallet_secret: &Secret,
        mnemonics_secrets: Vec<(Mnemonic, Option<Secret>)>,
        minimum_signatures: u16,
        additional_xpub_keys: Vec<String>,
    ) -> Result<Arc<dyn Account>> {
        let mut additional_xpub_keys = additional_xpub_keys
            .into_iter()
            .map(|xpub| {
                ExtendedKey::from_str(&xpub).map(|mut xpub| {
                    xpub.prefix = KeyPrefix::XPUB;
                    xpub.to_string()
                })
            })
            .collect::<Result<Vec<_>, kaspa_bip32::Error>>()?;

        let mut generated_xpubs = Vec::with_capacity(mnemonics_secrets.len());
        let mut prv_key_data_ids = Vec::with_capacity(mnemonics_secrets.len());
        let prv_key_data_store = self.store().as_prv_key_data_store()?;

        for (mnemonic, payment_secret) in mnemonics_secrets {
            let prv_key_data =
                storage::PrvKeyData::try_new_from_mnemonic(mnemonic, payment_secret.as_ref(), self.store().encryption_kind()?)?;
            if prv_key_data_store.load_key_data(wallet_secret, &prv_key_data.id).await?.is_some() {
                return Err(Error::PrivateKeyAlreadyExists(prv_key_data.id));
            }
            let xpub_key = prv_key_data.create_xpub(payment_secret.as_ref(), MULTISIG_ACCOUNT_KIND.into(), 0).await?; // todo it can be done concurrently
            generated_xpubs.push(xpub_key.to_string(Some(KeyPrefix::XPUB)));
            prv_key_data_ids.push(prv_key_data.id);
            prv_key_data_store.store(wallet_secret, prv_key_data).await?;
        }

        generated_xpubs.sort_unstable();
        additional_xpub_keys.extend_from_slice(generated_xpubs.as_slice());
        let mut xpub_keys = additional_xpub_keys;
        xpub_keys.sort_unstable();

        let min_cosigner_index =
            generated_xpubs.first().and_then(|first_generated| xpub_keys.binary_search(first_generated).ok()).map(|v| v as u8);

        let xpub_keys = xpub_keys
            .into_iter()
            .map(|xpub_key| {
                ExtendedPublicKeySecp256k1::from_str(&xpub_key).map_err(|err| Error::InvalidExtendedPublicKey(xpub_key, err))
            })
            .collect::<Result<Vec<_>>>()?;

        let account: Arc<dyn Account> = Arc::new(
            multisig::MultiSig::try_new(
                self,
                None,
                Arc::new(xpub_keys),
                Some(Arc::new(prv_key_data_ids)),
                min_cosigner_index,
                minimum_signatures,
                false,
            )
            .await?,
        );

        self.inner.store.clone().as_account_store()?.store_single(&account.to_storage()?, None).await?;
        account.clone().start().await?;

        Ok(account)
    }

    async fn rename(&self, title: Option<String>, filename: Option<String>, wallet_secret: &Secret) -> Result<()> {
        let store = self.store();
        store.rename(wallet_secret, title.as_deref(), filename.as_deref()).await?;
        Ok(())
    }

    async fn ensure_default_account_impl(
        self: Arc<Self>,
        wallet_secret: &Secret,
        payment_secret: Option<&Secret>,
        kind: AccountKind,
        mnemonic_phrase: Option<&Secret>,
    ) -> Result<AccountDescriptor> {
        if kind != BIP32_ACCOUNT_KIND {
            return Err(Error::custom("Account kind is not supported"));
        }

        let account = self.store().as_account_store()?.iter(None).await?.next().await;

        if let Some(Ok((stored_account, stored_metadata))) = account {
            let account_descriptor = try_load_account(&self, stored_account, stored_metadata).await?.descriptor()?;
            Ok(account_descriptor)
        } else {
            let mnemonic_phrase_string = if let Some(phrase) = mnemonic_phrase.cloned() {
                phrase
            } else {
                let mnemonic = Mnemonic::random(WordCount::Words24, Language::English)?;
                Secret::from(mnemonic.phrase_string())
            };

            let prv_key_data_args = PrvKeyDataCreateArgs::new(None, payment_secret.cloned(), mnemonic_phrase_string);

            self.store().batch().await?;
            let prv_key_data_id = self.clone().create_prv_key_data(wallet_secret, prv_key_data_args).await?;

            let account_create_args = AccountCreateArgs::new_bip32(prv_key_data_id, payment_secret.cloned(), None, None);

            let account = self.clone().create_account(wallet_secret, account_create_args, false).await?;

            self.store().flush(wallet_secret).await?;

            Ok(account.descriptor()?)
        }
    }
}

// fn decrypt_mnemonic<T: AsRef<[u8]>>(
//     num_threads: u32,
//     EncryptedMnemonic { cipher, salt }: EncryptedMnemonic<T>,
//     pass: &[u8],
// ) -> Result<String> {
//     let params = argon2::ParamsBuilder::new().t_cost(1).m_cost(64 * 1024).p_cost(num_threads).output_len(32).build().unwrap();
//     let mut key = [0u8; 32];
//     argon2::Argon2::new(argon2::Algorithm::Argon2id, Default::default(), params)
//         .hash_password_into(pass, salt.as_ref(), &mut key[..])
//         .unwrap();
//     let mut aead = chacha20poly1305::XChaCha20Poly1305::new(Key::from_slice(&key));
//     let (nonce, ciphertext) = cipher.as_ref().split_at(24);

//     let decrypted = aead.decrypt(nonce.into(), ciphertext).unwrap();
//     Ok(unsafe { String::from_utf8_unchecked(decrypted) })
// }

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod test {
    // use hex_literal::hex;

    // use super::*;
    // use kaspa_addresses::Address;

    /*
    use workflow_rpc::client::ConnectOptions;
    use std::{str::FromStr, thread::sleep, time};
    use crate::derivation::gen1;
    use crate::utxo::{UtxoContext, UtxoContextBinding, UtxoIterator};
    use kaspa_addresses::{Prefix, Version};
    use kaspa_bip32::{ChildNumber, ExtendedPrivateKey, SecretKey};
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
    use kaspa_consensus_wasm::{sign_transaction, SignableTransaction, Transaction, TransactionInput, TransactionOutput};
    use kaspa_txscript::pay_to_address_script;

    async fn create_utxos_context_with_addresses(
        rpc: Arc<DynRpcApi>,
        addresses: Vec<Address>,
        current_daa_score: u64,
        core: &UtxoProcessor,
    ) -> Result<UtxoContext> {
        let utxos = rpc.get_utxos_by_addresses(addresses).await?;
        let utxo_context = UtxoContext::new(core, UtxoContextBinding::default());
        let entries = utxos.into_iter().map(|entry| entry.into()).collect::<Vec<_>>();
        for entry in entries.into_iter() {
            utxo_context.insert(entry, current_daa_score, false).await?;
        }
        Ok(utxo_context)
    }

    #[allow(dead_code)]
    // #[tokio::test]
    async fn wallet_test() -> Result<()> {
        println!("Creating wallet...");
        let resident_store = Wallet::resident_store()?;
        let wallet = Arc::new(Wallet::try_new(resident_store, None)?);

        let rpc_api = wallet.rpc_api();
        let utxo_processor = wallet.utxo_processor();

        let wrpc_client = wallet.wrpc_client().expect("Unable to obtain wRPC client");

        let info = rpc_api.get_block_dag_info().await?;
        let current_daa_score = info.virtual_daa_score;

        let _connect_result = wrpc_client.connect(ConnectOptions::fallback()).await;
        //println!("connect_result: {_connect_result:?}");

        let _result = wallet.start().await;
        //println!("wallet.task(): {_result:?}");
        let result = wallet.get_info().await;
        println!("wallet.get_info(): {result:#?}");

        let address = Address::try_from("kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd")?;

        let utxo_context =
            self::create_utxos_context_with_addresses(rpc_api.clone(), vec![address.clone()], current_daa_score, utxo_processor)
                .await?;

        let utxo_set_balance = utxo_context.calculate_balance().await;
        println!("get_utxos_by_addresses: {utxo_set_balance:?}");

        let to_address = Address::try_from("kaspatest:qpakxqlesqywgkq7rg4wyhjd93kmw7trkl3gpa3vd5flyt59a43yyn8vu0w8c")?;
        let mut iter = UtxoIterator::new(&utxo_context);
        let utxo = iter.next().unwrap();
        let utxo = (*utxo.utxo).clone();
        let selected_entries = vec![utxo];

        let entries = &selected_entries;

        let inputs = selected_entries
            .iter()
            .enumerate()
            .map(|(sequence, utxo)| TransactionInput::new(utxo.outpoint.clone(), vec![], sequence as u64, 0))
            .collect::<Vec<TransactionInput>>();

        let tx = Transaction::new(
            0,
            inputs,
            vec![TransactionOutput::new(1000, &pay_to_address_script(&to_address))],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        )?;

        let mtx = SignableTransaction::new(tx, (*entries).clone().into());

        let derivation_path =
            gen1::WalletDerivationManager::build_derivate_path(false, 0, None, Some(kaspa_bip32::AddressType::Receive))?;

        let xprv = "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ";

        let xkey = ExtendedPrivateKey::<SecretKey>::from_str(xprv)?.derive_path(derivation_path)?;

        let xkey = xkey.derive_child(ChildNumber::new(0, false)?)?;

        // address test
        let address_test = Address::new(Prefix::Testnet, Version::PubKey, &xkey.public_key().to_bytes()[1..]);
        let address_str: String = address_test.clone().into();
        assert_eq!(address, address_test, "Addresses don't match");
        println!("address: {address_str}");

        let private_keys = vec![xkey.to_bytes()];

        println!("mtx: {mtx:?}");

        let mtx = sign_transaction(mtx, private_keys, true)?;

        let utxo_context =
            self::create_utxos_context_with_addresses(rpc_api.clone(), vec![to_address.clone()], current_daa_score, utxo_processor)
                .await?;
        let to_balance = utxo_context.calculate_balance().await;
        println!("to address balance before tx submit: {to_balance:?}");

        let result = rpc_api.submit_transaction(mtx.into(), false).await?;

        println!("tx submit result, {:?}", result);
        println!("sleep for 5s...");
        sleep(time::Duration::from_millis(5000));
        let utxo_context =
            self::create_utxos_context_with_addresses(rpc_api.clone(), vec![to_address.clone()], current_daa_score, utxo_processor)
                .await?;
        let to_balance = utxo_context.calculate_balance().await;
        println!("to address balance after tx submit: {to_balance:?}");

        Ok(())
    }
    */
}
