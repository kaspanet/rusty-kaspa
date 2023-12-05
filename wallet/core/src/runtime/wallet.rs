use crate::api::{message::*, traits::WalletApi};
use crate::imports::*;
use crate::result::Result;
use crate::runtime::{account::ScanNotifier, try_from_storage, Account, AccountId, ActiveAccountMap};
use crate::secret::Secret;
use crate::settings::{SettingsStore, WalletSettings};
use crate::storage::interface::{AccessContext, CreateArgs, OpenArgs, TransactionRangeResult};
use crate::storage::local::interface::LocalStore;
use crate::storage::local::Storage;
use crate::storage::{
    self, AccessContextT, AccountKind, Binding, Hint, Interface, PrvKeyData, PrvKeyDataId, PrvKeyDataInfo, WalletDescriptor,
};
use crate::tx::Fees;
use crate::utxo::UtxoProcessor;
#[allow(unused_imports)]
use crate::{derivation::gen0, derivation::gen0::import::*, derivation::gen1, derivation::gen1::import::*};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use futures::future::join_all;
use futures::stream::StreamExt;
use futures::{select, FutureExt, Stream};
use kaspa_bip32::{Language, Mnemonic, MnemonicVariant, WordCount};
use kaspa_notify::{
    listener::ListenerId,
    scope::{Scope, VirtualDaaScoreChangedScope},
};
use kaspa_rpc_core::notify::mode::NotificationMode;
use kaspa_wallet_core::storage::MultiSig;
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use std::sync::Arc;
use workflow_core::task::spawn;
use workflow_log::log_error;
use zeroize::Zeroize;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[serde(rename_all = "camelCase")]
pub struct WalletCreateArgs {
    pub title: Option<String>,
    pub filename: Option<String>,
    pub user_hint: Option<Hint>,
    pub wallet_secret: Secret,
    pub overwrite_wallet_storage: bool,
}

impl WalletCreateArgs {
    pub fn new(
        title: Option<String>,
        filename: Option<String>,
        user_hint: Option<Hint>,
        secret: Secret,
        overwrite_wallet_storage: bool,
    ) -> Self {
        Self { title, filename, user_hint, wallet_secret: secret, overwrite_wallet_storage }
    }
}

impl From<WalletCreateArgs> for CreateArgs {
    fn from(args: WalletCreateArgs) -> Self {
        CreateArgs::new(args.title, args.filename, args.user_hint, args.overwrite_wallet_storage)
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct WalletOpenArgs {
    /// Return account descriptors
    pub account_descriptors: bool,
    /// Enable support for legacy accounts
    pub legacy_accounts: bool,
}

impl WalletOpenArgs {
    pub fn default_with_legacy_accounts() -> Self {
        Self { legacy_accounts: true, ..Default::default() }
    }

    fn load_account_descriptors(&self) -> bool {
        self.account_descriptors || self.legacy_accounts
    }

    fn is_legacy_only(&self) -> bool {
        self.legacy_accounts && !self.account_descriptors
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct PrvKeyDataCreateArgs {
    pub name: Option<String>,
    pub wallet_secret: Secret,
    pub payment_secret: Option<Secret>,
    pub mnemonic: MnemonicVariant, //Option<String>,
}

impl PrvKeyDataCreateArgs {
    pub fn new(name: Option<String>, wallet_secret: Secret, payment_secret: Option<Secret>, mnemonic: MnemonicVariant) -> Self {
        Self { name, wallet_secret, payment_secret, mnemonic }
    }
}

impl Zeroize for PrvKeyDataCreateArgs {
    fn zeroize(&mut self) {
        self.mnemonic.zeroize();
    }
}

#[derive(Clone, Debug)]
pub struct MultisigCreateArgs {
    pub prv_key_data_ids: Vec<PrvKeyDataId>,
    pub name: Option<String>,
    pub wallet_secret: Secret,
    pub additional_xpub_keys: Vec<String>,
    pub minimum_signatures: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AccountCreateArgs {
    pub account_name: Option<String>,
    pub account_kind: storage::AccountKind,
    pub wallet_secret: Secret,
    pub payment_secret: Option<Secret>,
}

impl AccountCreateArgs {
    pub fn new(
        account_name: Option<String>,
        account_kind: storage::AccountKind,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
    ) -> Self {
        Self { account_name, account_kind, wallet_secret, payment_secret }
    }
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
}

/// `Wallet` data structure
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

    pub fn try_new(storage: Arc<dyn Interface>, network_id: Option<NetworkId>) -> Result<Wallet> {
        Wallet::try_with_wrpc(storage, network_id)
    }

    pub fn try_with_wrpc(store: Arc<dyn Interface>, network_id: Option<NetworkId>) -> Result<Wallet> {
        let rpc_client =
            Arc::new(KaspaRpcClient::new_with_args(WrpcEncoding::Borsh, NotificationMode::MultiListeners, "wrpc://127.0.0.1:17110")?);
        let rpc_ctl = rpc_client.ctl().clone();
        let rpc_api: Arc<DynRpcApi> = rpc_client;
        let rpc = Rpc::new(rpc_api, rpc_ctl);
        Self::try_with_rpc(Some(rpc), store, network_id)
    }

    pub fn try_with_rpc(rpc: Option<Rpc>, store: Arc<dyn Interface>, network_id: Option<NetworkId>) -> Result<Wallet> {
        let multiplexer = Multiplexer::<Box<Events>>::new();
        let utxo_processor = Arc::new(UtxoProcessor::new(rpc.clone(), network_id, Some(multiplexer.clone())));

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
                utxo_processor,
            }),
        };

        Ok(wallet)
    }

    pub fn inner(&self) -> &Arc<Inner> {
        &self.inner
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
        self.utxo_processor().clear().await?;

        self.select(None).await?;

        let accounts = self.active_accounts().collect();
        let futures = accounts.into_iter().map(|account| account.stop());
        join_all(futures).await.into_iter().collect::<Result<Vec<_>>>()?;

        if clear_legacy_cache {
            self.legacy_accounts().clear();
        }

        Ok(())
    }

    pub async fn reload(self: &Arc<Self>) -> Result<()> {
        self.reset(false).await?;

        if self.is_open() {
            let wallet_descriptor = self.store().descriptor();
            let accounts = self.accounts(None).await?.try_collect::<Vec<_>>().await?;
            let account_descriptors = Some(accounts.iter().map(|account| account.descriptor()).collect::<Result<Vec<_>>>()?);
            self.notify(Events::WalletReload { wallet_descriptor, account_descriptors }).await?;
        }

        Ok(())
    }

    pub async fn close(self: &Arc<Wallet>) -> Result<()> {
        self.reset(true).await?;
        self.store().close().await?;
        self.notify(Events::WalletClose).await?;

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
        secret: Secret,
        filename: Option<String>,
        args: WalletOpenArgs,
    ) -> Result<Option<Vec<AccountDescriptor>>> {
        let filename = filename.or_else(|| self.settings().get(WalletSettings::Wallet));
        // let name = Some(make_filename(&name, &None));

        let was_open = self.is_open();

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(secret.clone()));
        self.store().open(&ctx, OpenArgs::new(filename)).await?;
        let wallet_name = self.store().descriptor();

        if was_open {
            self.notify(Events::WalletClose).await?;
        }

        // reset current state only after we have successfully opened another wallet
        self.reset(true).await?;

        let accounts = if args.load_account_descriptors() {
            let stored_accounts = self.inner.store.as_account_store().unwrap().iter(None).await?.try_collect::<Vec<_>>().await?;
            let stored_accounts = if !args.is_legacy_only() {
                stored_accounts
            } else {
                stored_accounts.into_iter().filter(|(stored_account, _)| stored_account.is_legacy()).collect::<Vec<_>>()
            };
            Some(
                futures::stream::iter(stored_accounts.into_iter())
                    .then(|(account, meta)| try_from_storage(self, account, meta))
                    .try_collect::<Vec<_>>()
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
                    legacy_account.create_private_context(secret.clone(), None, None).await?;
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
        secret: Secret,
        filename: Option<String>,
        args: WalletOpenArgs,
    ) -> Result<Option<Vec<AccountDescriptor>>> {
        // This is a wrapper of open_impl() that catches errors and notifies the UI
        match self.open_impl(secret, filename, args).await {
            Ok(account_descriptors) => Ok(account_descriptors),
            Err(err) => {
                self.notify(Events::WalletError { message: err.to_string() }).await?;
                Err(err)
            }
        }
    }

    async fn activate_accounts_impl(self: &Arc<Wallet>, account_ids: Option<&[AccountId]>) -> Result<()> {
        let stored_accounts = if let Some(ids) = account_ids {
            self.inner.store.as_account_store().unwrap().load_multiple(ids).await?
        } else {
            self.inner.store.as_account_store().unwrap().iter(None).await?.try_collect::<Vec<_>>().await?
        };

        let ids = stored_accounts.iter().map(|(account, _)| *account.id()).collect::<Vec<_>>();

        for (stored_account, meta) in stored_accounts.into_iter() {
            if stored_account.is_legacy() {
                let legacy_account = self
                    .legacy_accounts()
                    .get(stored_account.id())
                    .ok_or_else(|| Error::LegacyAccountNotInitialized)?
                    .clone()
                    .as_legacy_account()?;
                legacy_account.clone().start().await?;
                legacy_account.clear_private_context().await?;
            } else {
                let account = try_from_storage(self, stored_account, meta).await?;
                account.clone().start().await?;
            }
        }

        self.notify(Events::AccountActivation { ids }).await?;

        Ok(())
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

    // // /// Loads a wallet from storage. Accounts are activated by this call.
    // pub async fn open_and_activate(self: &Arc<Wallet>, secret: Secret, name: Option<String>) -> Result<()> {
    //     let name = name.or_else(|| self.settings().get(WalletSettings::Wallet));
    //     let name = Some(make_filename(&name, &None));
    //     let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(secret.clone()));
    //     self.store().open(&ctx, OpenArgs::new(name)).await?;

    //     // reset current state only after we have successfully opened another wallet
    //     self.reset(true).await?;

    //     self.initialize_all_stored_accounts(secret).await?;
    //     let hint = self.store().get_user_hint().await?;
    //     self.notify(Events::WalletHint { hint }).await?;
    //     self.notify(Events::WalletOpen).await?;
    //     Ok(())
    // }

    // async fn initialize_all_stored_accounts(self: &Arc<Wallet>, secret: Secret) -> Result<()> {
    //     self.initialize_accounts(None, secret).await?.try_collect::<Vec<_>>().await?;
    //     Ok(())
    // }

    pub async fn get_prv_key_data(&self, wallet_secret: Secret, id: &PrvKeyDataId) -> Result<Option<PrvKeyData>> {
        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
        self.inner.store.as_prv_key_data_store()?.load_key_data(&ctx, id).await
    }

    pub async fn get_prv_key_info(&self, account: &Arc<dyn Account>) -> Result<Option<Arc<PrvKeyDataInfo>>> {
        self.inner.store.as_prv_key_data_store()?.load_key_info(account.prv_key_data_id()?).await
    }

    pub async fn is_account_key_encrypted(&self, account: &Arc<dyn Account>) -> Result<Option<bool>> {
        Ok(self.get_prv_key_info(account).await?.map(|info| info.is_encrypted()))
    }

    pub fn wrpc_client(&self) -> Option<Arc<KaspaRpcClient>> {
        self.rpc_api().clone().downcast_arc::<KaspaRpcClient>().ok()
    }

    pub fn rpc_api(&self) -> Arc<DynRpcApi> {
        self.utxo_processor().rpc_api()
    }

    pub fn rpc_ctl(&self) -> RpcCtl {
        self.utxo_processor().rpc_ctl()
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

    pub fn settings(&self) -> &SettingsStore<WalletSettings> {
        &self.inner.settings
    }

    pub fn current_daa_score(&self) -> Option<u64> {
        self.utxo_processor().current_daa_score()
    }

    pub async fn load_settings(&self) -> Result<()> {
        self.settings().try_load().await?;

        let settings = self.settings();

        if let Some(network_type) = settings.get(WalletSettings::Network) {
            self.set_network_id(network_type).unwrap_or_else(|_| log_error!("Unable to select network type: `{}`", network_type));
        }

        if let Some(url) = settings.get::<String>(WalletSettings::Server) {
            if let Some(wrpc_client) = self.wrpc_client() {
                wrpc_client.set_url(url.as_str()).unwrap_or_else(|_| log_error!("Unable to set rpc url: `{}`", url));
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
        if let Some(rpc_client) = self.wrpc_client() {
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

    pub fn listener_id(&self) -> ListenerId {
        self.inner.listener_id.lock().unwrap().expect("missing wallet.inner.listener_id in Wallet::listener_id()")
    }

    pub async fn get_info(&self) -> Result<String> {
        let v = self.rpc_api().get_info().await?;
        Ok(format!("{v:#?}").replace('\n', "\r\n"))
    }

    pub async fn subscribe_daa_score(&self) -> Result<()> {
        self.rpc_api().start_notify(self.listener_id(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    pub async fn unsubscribe_daa_score(&self) -> Result<()> {
        self.rpc_api().stop_notify(self.listener_id(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    pub async fn ping(&self) -> bool {
        self.rpc_api().ping().await.is_ok()
    }

    pub async fn broadcast(&self) -> Result<()> {
        Ok(())
    }

    pub fn set_network_id(&self, network_id: NetworkId) -> Result<()> {
        if self.is_connected() {
            return Err(Error::NetworkTypeConnected);
        }
        self.utxo_processor().set_network_id(network_id);
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
        if let Some(wrpc_client) = self.wrpc_client() {
            let port = match wrpc_client.encoding() {
                WrpcEncoding::Borsh => network_type.default_borsh_rpc_port(),
                WrpcEncoding::SerdeJson => network_type.default_json_rpc_port(),
            };
            Ok(Some(port))
        } else {
            Ok(None)
        }
    }

    // pub async fn create_private_key_impl(self: &Arc<Wallet>, wallet_secret: Secret, payment_secret: Option<Secret>, save : ) -> Result<Mnemonic> {
    //     let store = Store::new(storage::DEFAULT_WALLET_FILE)?;
    //     let mnemonic = Mnemonic::create_random()?;
    //     let wallet = storage::local::Wallet::try_load(&store).await?;
    //     let mut payload = wallet.payload.decrypt::<Payload>(wallet_secret).unwrap();
    //     payload.as_mut().add_prv_key_data(mnemonic.clone(), payment_secret)?;
    //     Ok(mnemonic)
    // }

    // pub async fn create_private_key(self: &Arc<Wallet>, wallet_secret: Secret, payment_secret: Option<Secret>) -> Result<Mnemonic> {
    //     let mnemonic = Mnemonic::create_random()?;
    //     self.store.as_prv_key_data_store().store_key_data(&self.
    //     // let store = Store::default();
    //     // let mnemonic = Mnemonic::create_random()?;
    //     // let wallet = storage::local::Wallet::try_load(&store).await?;
    //     // let mut payload = wallet.payload.decrypt::<Payload>(wallet_secret).unwrap();
    //     // payload.as_mut().add_prv_key_data(mnemonic.clone(), payment_secret)?;
    //     Ok(mnemonic)
    // }

    pub async fn create_multisig_account(self: &Arc<Wallet>, args: MultisigCreateArgs) -> Result<Arc<dyn Account>> {
        let account_storage = self.inner.store.clone().as_account_store()?;
        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(args.wallet_secret));

        let settings = storage::Settings { is_visible: false, name: args.name };
        let mut xpub_keys = args.additional_xpub_keys;

        let account: Arc<dyn Account> = if args.prv_key_data_ids.is_not_empty() {
            let mut generated_xpubs = Vec::with_capacity(args.prv_key_data_ids.len());
            let mut prv_key_data_ids = Vec::with_capacity(args.prv_key_data_ids.len());
            for prv_key_data_id in args.prv_key_data_ids {
                let prv_key_data = self
                    .inner
                    .store
                    .as_prv_key_data_store()?
                    .load_key_data(&ctx, &prv_key_data_id)
                    .await?
                    .ok_or_else(|| Error::PrivateKeyNotFound(prv_key_data_id))?;
                let xpub_key = prv_key_data.create_xpub(None, AccountKind::MultiSig, 0).await?; // todo it can be done concurrently
                let xpub_prefix = kaspa_bip32::Prefix::XPUB;
                generated_xpubs.push(xpub_key.to_string(Some(xpub_prefix)));
                prv_key_data_ids.push(prv_key_data_id);
            }

            generated_xpubs.sort_unstable();
            xpub_keys.extend_from_slice(generated_xpubs.as_slice());
            xpub_keys.sort_unstable();
            let min_cosigner_index = xpub_keys.binary_search(generated_xpubs.first().unwrap()).unwrap() as u8;

            Arc::new(
                runtime::MultiSig::try_new(
                    self,
                    settings,
                    MultiSig::new(
                        Arc::new(xpub_keys),
                        Some(Arc::new(prv_key_data_ids)),
                        Some(min_cosigner_index),
                        args.minimum_signatures,
                        false,
                    ),
                    None,
                )
                .await?,
            )
        } else {
            Arc::new(
                runtime::MultiSig::try_new(
                    self,
                    settings,
                    MultiSig::new(Arc::new(xpub_keys), None, None, args.minimum_signatures, false),
                    None,
                )
                .await?,
            )
        };

        let stored_account = account.as_storable()?;

        account_storage.store_single(&stored_account, None).await?;
        self.inner.store.clone().commit(&ctx).await?;
        account.clone().start().await?;

        Ok(account)
    }

    pub async fn create_bip32_account(
        self: &Arc<Wallet>,
        prv_key_data_id: PrvKeyDataId,
        args: AccountCreateArgs,
    ) -> Result<Arc<dyn Account>> {
        let account_storage = self.inner.store.clone().as_account_store()?;
        let account_index = account_storage.clone().len(Some(prv_key_data_id)).await? as u64;

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(args.wallet_secret));
        let prv_key_data = self
            .inner
            .store
            .as_prv_key_data_store()?
            .load_key_data(&ctx, &prv_key_data_id)
            .await?
            .ok_or_else(|| Error::PrivateKeyNotFound(prv_key_data_id))?;
        let xpub_key = prv_key_data.create_xpub(args.payment_secret.as_ref(), args.account_kind, account_index).await?;
        let xpub_prefix = kaspa_bip32::Prefix::XPUB;
        let xpub_keys = Arc::new(vec![xpub_key.to_string(Some(xpub_prefix))]);

        let bip32 = storage::Bip32::new(account_index, xpub_keys, false);

        let settings = storage::Settings { is_visible: false, name: None };
        let account: Arc<dyn Account> = Arc::new(runtime::Bip32::try_new(self, prv_key_data.id, settings, bip32, None).await?);
        let stored_account = account.as_storable()?;

        account_storage.store_single(&stored_account, None).await?;
        self.inner.store.clone().commit(&ctx).await?;

        self.notify(Events::AccountCreation { descriptor: account.descriptor()? }).await?;

        Ok(account)
    }

    pub async fn create_wallet(self: &Arc<Wallet>, args: WalletCreateArgs) -> Result<Option<String>> {
        self.close().await?;

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(args.wallet_secret.clone()));
        self.inner.store.create(&ctx, args.into()).await?;
        let descriptor = self.inner.store.location()?;
        self.inner.store.commit(&ctx).await?;
        Ok(descriptor)
    }

    pub async fn create_prv_key_data(self: &Arc<Wallet>, args: PrvKeyDataCreateArgs) -> Result<(PrvKeyDataId, Mnemonic)> {
        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(args.wallet_secret.clone()));
        let mnemonic = Mnemonic::try_from(args.mnemonic)?;
        let prv_key_data = PrvKeyData::try_from((mnemonic.clone(), args.payment_secret.as_ref()))?;
        let prv_key_data_id = prv_key_data.id;
        let prv_key_data_store = self.inner.store.as_prv_key_data_store()?;
        prv_key_data_store.store(&ctx, prv_key_data).await?;
        self.inner.store.commit(&ctx).await?;
        Ok((prv_key_data_id, mnemonic))
    }

    pub async fn create_wallet_with_account(
        self: &Arc<Wallet>,
        wallet_args: WalletCreateArgs,
        account_args: AccountCreateArgs,
        mnemonic_phrase_word_count: WordCount,
    ) -> Result<(Mnemonic, Option<String>, Arc<dyn Account>)> {
        self.close().await?;

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(account_args.wallet_secret));

        self.inner.store.create(&ctx, wallet_args.into()).await?;
        let descriptor = self.inner.store.location()?;
        let xpub_prefix = kaspa_bip32::Prefix::XPUB;
        let mnemonic = Mnemonic::random(mnemonic_phrase_word_count, Default::default())?;
        let account_index = 0;
        let prv_key_data = PrvKeyData::try_from((mnemonic.clone(), account_args.payment_secret.as_ref()))?;
        let xpub_key =
            prv_key_data.create_xpub(account_args.payment_secret.as_ref(), account_args.account_kind, account_index).await?;
        let xpub_keys = Arc::new(vec![xpub_key.to_string(Some(xpub_prefix))]);

        let bip32 = storage::Bip32::new(account_index, xpub_keys, false);

        let settings = storage::Settings { is_visible: false, name: None };
        let account: Arc<dyn Account> = Arc::new(runtime::Bip32::try_new(self, prv_key_data.id, settings, bip32, None).await?);
        let stored_account = account.as_storable()?;

        let prv_key_data_store = self.inner.store.as_prv_key_data_store()?;
        prv_key_data_store.store(&ctx, prv_key_data).await?;
        let account_store = self.inner.store.as_account_store()?;
        account_store.store_single(&stored_account, None).await?;
        self.inner.store.commit(&ctx).await?;

        self.select(Some(&account)).await?;
        Ok((mnemonic, descriptor, account))
    }

    pub async fn get_account_by_id(self: &Arc<Self>, account_id: &AccountId) -> Result<Option<Arc<dyn Account>>> {
        if let Some(account) = self.active_accounts().get(account_id) {
            Ok(Some(account.clone()))
        } else {
            let account_storage = self.inner.store.as_account_store()?;
            let stored = account_storage.load_single(account_id).await?;
            if let Some((stored_account, stored_metadata)) = stored {
                let account = try_from_storage(self, stored_account, stored_metadata).await?;
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

    async fn handle_event(self: &Arc<Self>, event: Box<Events>) -> Result<()> {
        match &*event {
            Events::Change { .. } => {}
            Events::Pending { record } | Events::Maturity { record } => {
                // if transaction is outgoing, we record only its initial
                // issuance below (next match: Events::Outgoing)
                // There is no difference between pending and mature
                // outgoing transactions (the transaction is the same, but
                // the Pending event may be issued by the receipt of the
                // change UTXOs)
                if !record.is_outgoing() {
                    self.store().as_transaction_record_store()?.store(&[record]).await?;
                }
            }

            Events::Reorg { record } | Events::External { record } | Events::Outgoing { record } => {
                self.store().as_transaction_record_store()?.store(&[record]).await?;
            }
            Events::Scan { record } => {
                if let Err(err) =
                    self.store().as_transaction_record_store()?.load_single(record.binding(), &self.network_id()?, record.id()).await
                {
                    // println!("Detected unknown transaction {} {err}", record.id());

                    let transaction_daa_score = record.block_daa_score();
                    match self.rpc_api().get_daa_score_timestamp_estimate(vec![transaction_daa_score]).await {
                        Ok(timestamps) => {
                            if let Some(timestamp) = timestamps.first() {
                                let mut record = record.clone();
                                record.set_unixtime(*timestamp);
                                // this will be broadcasted to clients as well as
                                // received by this function again (above ^^)
                                // subsequently resulting in the storage of this
                                // transaction record
                                self.notify(Events::External { record }).await?;
                            } else {
                                log_error!("Wallet::handle_event() unable to obtain DAA to unixtime for DAA {transaction_daa_score}: timestamps is empty");
                            }
                            // let mut record = record.clone();
                            // record.set_timestamp(timestamps[0]);
                            // self.notify(Events::External { record : record.clone()}).await?;
                        }
                        Err(err) => {
                            log_error!("Wallet::handle_event() unable to resolve DAA to unixtime: {}", err);
                        }
                    }
                    // if transactions is not found in transaction storage, re-publish it as external
                }
            }
            Events::SyncState { sync_state } => {
                if sync_state.is_synced() && self.is_open() {
                    self.reload().await?;
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

    pub fn is_open(&self) -> bool {
        self.inner.store.is_open()
    }

    pub fn location(&self) -> Result<Option<String>> {
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
                    let account = try_from_storage(&wallet, stored_account, stored_metadata).await?;
                    account.clone().start().await?;
                    Ok(account)
                }
            }
        });

        Ok(Box::pin(stream))
    }

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

    pub async fn import_gen0_keydata(
        self: &Arc<Wallet>,
        import_secret: Secret,
        wallet_secret: Secret,
        payment_secret: Option<&Secret>,
        notifier: Option<ScanNotifier>,
    ) -> Result<Arc<dyn Account>> {
        let notifier = notifier.as_ref();
        let keydata = load_v0_keydata(&import_secret).await?;

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret.clone()));

        let mnemonic = Mnemonic::new(keydata.mnemonic.trim(), Language::English)?;
        let prv_key_data = PrvKeyData::try_new_from_mnemonic(mnemonic, payment_secret)?;
        let prv_key_data_store = self.inner.store.as_prv_key_data_store()?;
        if prv_key_data_store.load_key_data(&ctx, &prv_key_data.id).await?.is_some() {
            return Err(Error::PrivateKeyAlreadyExists(prv_key_data.id.to_hex()));
        }

        let data = storage::Legacy::new();
        let settings = storage::Settings::default();
        let account = Arc::new(runtime::account::Legacy::try_new(self, prv_key_data.id, settings, data, None).await?);

        // activate account (add it to wallet active account list)
        self.active_accounts().insert(account.clone().as_dyn_arc());
        self.legacy_accounts().insert(account.clone().as_dyn_arc());

        let account_store = self.inner.store.as_account_store()?;
        let stored_account = account.as_storable()?;
        // store private key and account
        self.inner.store.batch().await?;
        prv_key_data_store.store(&ctx, prv_key_data).await?;
        account_store.store_single(&stored_account, None).await?;
        self.inner.store.flush(&ctx).await?;

        let legacy_account = account.clone().as_legacy_account()?;
        legacy_account.create_private_context(wallet_secret, payment_secret, None).await?;
        // account.clone().initialize_private_data(wallet_secret, payment_secret, None).await?;

        if self.is_connected() {
            if let Some(notifier) = notifier {
                notifier(0, 0, None);
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

    pub async fn import_gen1_keydata(self: &Arc<Wallet>, secret: Secret) -> Result<()> {
        let _keydata = load_v1_keydata(&secret).await?;

        Ok(())
    }

    pub async fn import_with_mnemonic(
        self: &Arc<Wallet>,
        wallet_secret: Secret,
        payment_secret: Option<&Secret>,
        mnemonic: Mnemonic,
        account_kind: AccountKind,
    ) -> Result<Arc<dyn Account>> {
        let prv_key_data = storage::PrvKeyData::try_new_from_mnemonic(mnemonic, payment_secret)?;
        let prv_key_data_store = self.store().as_prv_key_data_store()?;
        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret.clone()));
        if prv_key_data_store.load_key_data(&ctx, &prv_key_data.id).await?.is_some() {
            return Err(Error::PrivateKeyAlreadyExists(prv_key_data.id.to_hex()));
        }
        // let mut is_legacy = false;
        let account: Arc<dyn Account> = match account_kind {
            AccountKind::Bip32 => {
                let account_index = 0;
                let xpub_key = prv_key_data.create_xpub(payment_secret, account_kind, account_index).await?;
                let xpub_keys = Arc::new(vec![xpub_key.to_string(Some(kaspa_bip32::Prefix::KPUB))]);
                let ecdsa = false;
                // ---

                let data = storage::Bip32::new(account_index, xpub_keys, ecdsa);
                let settings = storage::Settings::default();
                Arc::new(runtime::account::Bip32::try_new(self, prv_key_data.id, settings, data, None).await?)
                // account
            }
            AccountKind::Legacy => {
                // is_legacy = true;
                let data = storage::Legacy::new();
                let settings = storage::Settings::default();
                Arc::new(runtime::account::Legacy::try_new(self, prv_key_data.id, settings, data, None).await?)
            }
            _ => {
                return Err(Error::AccountKindFeature);
            }
        };

        let stored_account = account.as_storable()?;
        let account_store = self.inner.store.as_account_store()?;
        self.inner.store.batch().await?;
        self.store().as_prv_key_data_store()?.store(&ctx, prv_key_data).await?;
        account_store.store_single(&stored_account, None).await?;
        self.inner.store.flush(&ctx).await?;

        if let Ok(legacy_account) = account.clone().as_legacy_account() {
            self.legacy_accounts().insert(account.clone());
            legacy_account.create_private_context(wallet_secret.clone(), None, None).await?;
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

    pub async fn import_multisig_with_mnemonic(
        self: &Arc<Wallet>,
        wallet_secret: Secret,
        mnemonics_secrets: Vec<(Mnemonic, Option<Secret>)>,
        minimum_signatures: u16,
        mut additional_xpub_keys: Vec<String>,
    ) -> Result<Arc<dyn Account>> {
        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));

        let mut generated_xpubs = Vec::with_capacity(mnemonics_secrets.len());
        let mut prv_key_data_ids = Vec::with_capacity(mnemonics_secrets.len());
        let prv_key_data_store = self.store().as_prv_key_data_store()?;

        for (mnemonic, payment_secret) in mnemonics_secrets {
            let prv_key_data = storage::PrvKeyData::try_new_from_mnemonic(mnemonic, payment_secret.as_ref())?;
            if prv_key_data_store.load_key_data(&ctx, &prv_key_data.id).await?.is_some() {
                return Err(Error::PrivateKeyAlreadyExists(prv_key_data.id.to_hex()));
            }
            let xpub_key = prv_key_data.create_xpub(payment_secret.as_ref(), AccountKind::MultiSig, 0).await?; // todo it can be done concurrently
            let xpub_prefix = kaspa_bip32::Prefix::XPUB;
            generated_xpubs.push(xpub_key.to_string(Some(xpub_prefix)));
            prv_key_data_ids.push(prv_key_data.id);
            prv_key_data_store.store(&ctx, prv_key_data).await?;
        }

        generated_xpubs.sort_unstable();
        additional_xpub_keys.extend_from_slice(generated_xpubs.as_slice());
        let mut xpub_keys = additional_xpub_keys;
        xpub_keys.sort_unstable();
        let min_cosigner_index = xpub_keys.binary_search(generated_xpubs.first().unwrap()).unwrap() as u8;

        let account: Arc<dyn Account> = Arc::new(
            runtime::MultiSig::try_new(
                self,
                storage::Settings::default(),
                MultiSig::new(
                    Arc::new(xpub_keys),
                    Some(Arc::new(prv_key_data_ids)),
                    Some(min_cosigner_index),
                    minimum_signatures,
                    false,
                ),
                None,
            )
            .await?,
        );

        let stored_account = account.as_storable()?;
        self.inner.store.clone().as_account_store()?.store_single(&stored_account, None).await?;
        self.inner.store.clone().commit(&ctx).await?;
        account.clone().start().await?;

        Ok(account)
    }

    async fn rename(&self, title: Option<String>, filename: Option<String>, wallet_secret: Secret) -> Result<()> {
        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
        let store = self.store();
        store.rename(&ctx, title.as_deref(), filename.as_deref()).await?;
        Ok(())
    }
}

use workflow_core::channel::Receiver;

use super::AccountDescriptor;
#[async_trait]
impl WalletApi for Wallet {
    async fn register_notifications(self: Arc<Self>, _channel: Receiver<WalletNotification>) -> Result<u64> {
        todo!()
    }
    async fn unregister_notifications(self: Arc<Self>, _channel_id: u64) -> Result<()> {
        todo!()
    }

    async fn get_status_call(self: Arc<Self>, _request: GetStatusRequest) -> Result<GetStatusResponse> {
        let is_connected = self.is_connected();
        let is_synced = self.is_synced();
        let is_open = self.is_open();
        let network_id = self.network_id().ok();
        let (url, is_wrpc_client) =
            if let Some(wrpc_client) = self.wrpc_client() { (Some(wrpc_client.url()), true) } else { (None, false) };

        Ok(GetStatusResponse { is_connected, is_synced, is_open, network_id, url, is_wrpc_client })
    }

    // -------------------------------------------------------------------------------------

    async fn connect_call(self: Arc<Self>, request: ConnectRequest) -> Result<ConnectResponse> {
        use workflow_rpc::client::{ConnectOptions, ConnectStrategy};

        let ConnectRequest { url, network_id } = request;

        if let Some(wrpc_client) = self.wrpc_client().as_ref() {
            // let network_type = NetworkType::from(network_id);
            let url = wrpc_client.parse_url_with_network_type(url, network_id.into()).map_err(|e| e.to_string())?;
            let options = ConnectOptions {
                block_async_connect: true,
                strategy: ConnectStrategy::Fallback,
                url: Some(url),
                ..Default::default()
            };
            wrpc_client.connect(options).await.map_err(|e| e.to_string())?;
            Ok(ConnectResponse {})
        } else {
            Err(Error::NotWrpcClient)
        }
    }

    async fn disconnect_call(self: Arc<Self>, _request: DisconnectRequest) -> Result<DisconnectResponse> {
        if let Some(wrpc_client) = self.wrpc_client().as_ref() {
            wrpc_client.shutdown().await?;
            Ok(DisconnectResponse {})
        } else {
            Err(Error::NotWrpcClient)
        }
    }

    // -------------------------------------------------------------------------------------

    async fn ping_call(self: Arc<Self>, request: PingRequest) -> Result<PingResponse> {
        log_info!("Wallet received ping request '{}' (should be 1)...", request.v);
        Ok(PingResponse { v: request.v + 1 })
    }

    async fn wallet_enumerate_call(self: Arc<Self>, _request: WalletEnumerateRequest) -> Result<WalletEnumerateResponse> {
        let wallet_list = self.store().wallet_list().await?;
        Ok(WalletEnumerateResponse { wallet_list })
    }

    async fn wallet_create_call(self: Arc<Self>, request: WalletCreateRequest) -> Result<WalletCreateResponse> {
        let WalletCreateRequest { wallet_args, prv_key_data_args, account_args } = request;

        // suspend commits for multiple operations
        self.store().batch().await?;

        let wallet_secret = wallet_args.wallet_secret.clone();

        let wallet_descriptor = self.create_wallet(wallet_args).await?;
        let (prv_key_data_id, mnemonic) = self.create_prv_key_data(prv_key_data_args).await?;
        let account = self.create_bip32_account(prv_key_data_id, account_args).await?;

        // flush data to storage
        let access_ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
        self.store().flush(&access_ctx).await?;

        Ok(WalletCreateResponse { mnemonic: mnemonic.phrase_string(), wallet_descriptor, account_descriptor: account.descriptor()? })
    }

    async fn wallet_open_call(self: Arc<Self>, request: WalletOpenRequest) -> Result<WalletOpenResponse> {
        let WalletOpenRequest { wallet_secret, wallet_name, account_descriptors, legacy_accounts } = request;
        let args = WalletOpenArgs { account_descriptors, legacy_accounts: legacy_accounts.unwrap_or_default() };
        let account_descriptors = self.open(wallet_secret, wallet_name, args).await?;
        Ok(WalletOpenResponse { account_descriptors })
    }

    async fn wallet_close_call(self: Arc<Self>, _request: WalletCloseRequest) -> Result<WalletCloseResponse> {
        self.close().await?;
        Ok(WalletCloseResponse {})
    }

    async fn wallet_rename_call(self: Arc<Self>, request: WalletRenameRequest) -> Result<WalletRenameResponse> {
        let WalletRenameRequest { wallet_secret, title, filename } = request;
        self.rename(title, filename, wallet_secret).await?;
        Ok(WalletRenameResponse {})
    }

    async fn prv_key_data_enumerate_call(
        self: Arc<Self>,
        _request: PrvKeyDataEnumerateRequest,
    ) -> Result<PrvKeyDataEnumerateResponse> {
        let prv_key_data_list = self.store().as_prv_key_data_store()?.iter().await?.try_collect::<Vec<_>>().await?;
        Ok(PrvKeyDataEnumerateResponse { prv_key_data_list })
    }

    async fn prv_key_data_create_call(self: Arc<Self>, request: PrvKeyDataCreateRequest) -> Result<PrvKeyDataCreateResponse> {
        let PrvKeyDataCreateRequest { prv_key_data_args, fetch_mnemonic } = request;
        let (prv_key_data_id, mnemonic) = self.create_prv_key_data(prv_key_data_args).await?;
        Ok(PrvKeyDataCreateResponse { mnemonic: fetch_mnemonic.then_some(mnemonic.phrase_string()), prv_key_data_id })
    }

    async fn prv_key_data_remove_call(self: Arc<Self>, _request: PrvKeyDataRemoveRequest) -> Result<PrvKeyDataRemoveResponse> {
        // TODO handle key removal
        return Err(Error::NotImplemented);
    }

    async fn prv_key_data_get_call(self: Arc<Self>, request: PrvKeyDataGetRequest) -> Result<PrvKeyDataGetResponse> {
        let PrvKeyDataGetRequest { prv_key_data_id, wallet_secret } = request;

        let access_ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
        let prv_key_data = self.store().as_prv_key_data_store()?.load_key_data(&access_ctx, &prv_key_data_id).await?;

        Ok(PrvKeyDataGetResponse { prv_key_data })
    }

    async fn accounts_rename_call(self: Arc<Self>, request: AccountsRenameRequest) -> Result<AccountsRenameResponse> {
        let AccountsRenameRequest { account_id, name, wallet_secret } = request;

        let account = self.get_account_by_id(&account_id).await?.ok_or(Error::AccountNotFound(account_id))?;
        account.rename(wallet_secret, name.as_deref()).await?;

        Ok(AccountsRenameResponse {})
    }

    async fn accounts_enumerate_call(self: Arc<Self>, _request: AccountsEnumerateRequest) -> Result<AccountsEnumerateResponse> {
        let account_list = self.accounts(None).await?.try_collect::<Vec<_>>().await?;
        let descriptor_list = account_list.iter().map(|account| account.descriptor().unwrap()).collect::<Vec<_>>();

        Ok(AccountsEnumerateResponse { descriptor_list })
    }

    async fn accounts_activate_call(self: Arc<Self>, request: AccountsActivateRequest) -> Result<AccountsActivateResponse> {
        let AccountsActivateRequest { account_ids } = request;

        self.activate_accounts(account_ids.as_deref()).await?;

        Ok(AccountsActivateResponse {})
    }

    async fn accounts_create_call(self: Arc<Self>, request: AccountsCreateRequest) -> Result<AccountsCreateResponse> {
        let AccountsCreateRequest { prv_key_data_id, account_args } = request;

        if !matches!(account_args.account_kind, AccountKind::Bip32) {
            return Err(Error::NotImplemented);
        }

        let account = self.create_bip32_account(prv_key_data_id, account_args).await?;

        Ok(AccountsCreateResponse { descriptor: account.descriptor()? })
    }

    async fn accounts_import_call(self: Arc<Self>, _request: AccountsImportRequest) -> Result<AccountsImportResponse> {
        // TODO handle account imports
        return Err(Error::NotImplemented);
    }

    async fn accounts_get_call(self: Arc<Self>, request: AccountsGetRequest) -> Result<AccountsGetResponse> {
        let AccountsGetRequest { account_id } = request;
        let account = self.get_account_by_id(&account_id).await?.ok_or(Error::AccountNotFound(account_id))?;
        let descriptor = account.descriptor().unwrap();
        Ok(AccountsGetResponse { descriptor })
    }

    async fn accounts_create_new_address_call(
        self: Arc<Self>,
        request: AccountsCreateNewAddressRequest,
    ) -> Result<AccountsCreateNewAddressResponse> {
        let AccountsCreateNewAddressRequest { account_id, kind } = request;

        let account = self.get_account_by_id(&account_id).await?.ok_or(Error::AccountNotFound(account_id))?;

        let address = match kind {
            NewAddressKind::Receive => account.as_derivation_capable()?.new_receive_address().await?,
            NewAddressKind::Change => account.as_derivation_capable()?.new_change_address().await?,
        };

        Ok(AccountsCreateNewAddressResponse { address })
    }

    async fn accounts_send_call(self: Arc<Self>, request: AccountsSendRequest) -> Result<AccountsSendResponse> {
        let AccountsSendRequest { account_id, wallet_secret, payment_secret, destination, priority_fee_sompi, payload } = request;

        let account = self.get_account_by_id(&account_id).await?.ok_or(Error::AccountNotFound(account_id))?;

        let abortable = Abortable::new();
        let (generator_summary, transaction_ids) =
            account.send(destination, priority_fee_sompi, payload, wallet_secret, payment_secret, &abortable, None).await?;

        Ok(AccountsSendResponse { generator_summary, transaction_ids })
    }

    async fn accounts_transfer_call(self: Arc<Self>, request: AccountsTransferRequest) -> Result<AccountsTransferResponse> {
        let AccountsTransferRequest {
            source_account_id,
            destination_account_id,
            wallet_secret,
            payment_secret,
            priority_fee_sompi,
            transfer_amount_sompi,
        } = request;

        let source_account = self.get_account_by_id(&source_account_id).await?.ok_or(Error::AccountNotFound(source_account_id))?;

        let abortable = Abortable::new();
        let (generator_summary, transaction_ids) = source_account
            .transfer(
                destination_account_id,
                transfer_amount_sompi,
                priority_fee_sompi.unwrap_or(Fees::SenderPaysAll(0)),
                wallet_secret,
                payment_secret,
                &abortable,
                None,
            )
            .await?;

        Ok(AccountsTransferResponse { generator_summary, transaction_ids })
    }

    async fn accounts_estimate_call(self: Arc<Self>, request: AccountsEstimateRequest) -> Result<AccountsEstimateResponse> {
        let AccountsEstimateRequest { task_id: _, account_id, destination, priority_fee_sompi, payload } = request;

        let account = self.get_account_by_id(&account_id).await?.ok_or(Error::AccountNotFound(account_id))?;

        let abortable = Abortable::new();
        let generator_summary = account.estimate(destination, priority_fee_sompi, payload, &abortable).await?;

        Ok(AccountsEstimateResponse { generator_summary })
    }

    async fn transaction_data_get_call(self: Arc<Self>, request: TransactionDataGetRequest) -> Result<TransactionDataGetResponse> {
        let TransactionDataGetRequest { account_id, network_id, filter, start, end } = request;

        if start > end {
            return Err(Error::InvalidRange(start, end));
        }

        let binding = Binding::Account(account_id);
        let store = self.store().as_transaction_record_store()?;
        let TransactionRangeResult { transactions, total } =
            store.load_range(&binding, &network_id, filter, start as usize..end as usize).await?;

        Ok(TransactionDataGetResponse { transactions, total, account_id, start })
    }

    async fn address_book_enumerate_call(
        self: Arc<Self>,
        _request: AddressBookEnumerateRequest,
    ) -> Result<AddressBookEnumerateResponse> {
        return Err(Error::NotImplemented);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod test {
    use std::{str::FromStr, thread::sleep, time};

    use super::*;
    use crate::utxo::{UtxoContext, UtxoContextBinding, UtxoIterator};
    use kaspa_addresses::{Address, Prefix, Version};
    use kaspa_bip32::{ChildNumber, ExtendedPrivateKey, SecretKey};
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
    use kaspa_consensus_wasm::{sign_transaction, SignableTransaction, Transaction, TransactionInput, TransactionOutput};
    use kaspa_txscript::pay_to_address_script;
    use workflow_rpc::client::ConnectOptions;

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
}
