use crate::imports::*;
use crate::result::Result;
use crate::runtime::Events;
use crate::runtime::{Account, AccountId, AccountMap};
use crate::secret::Secret;
use crate::settings::{SettingsStore, WalletSettings};
use crate::storage::interface::{AccessContext, CreateArgs, OpenArgs};
use crate::storage::local::interface::LocalStore;
use crate::storage::local::Storage;
use crate::storage::{self, AccessContextT, Interface, PrvKeyData, PrvKeyDataId, PrvKeyDataInfo};
use crate::utxo::UtxoProcessor;
#[allow(unused_imports)]
use crate::{accounts::gen0, accounts::gen0::import::*, accounts::gen1, accounts::gen1::import::*};
use futures::future::join_all;
use futures::stream::StreamExt;
use futures::{select, FutureExt, Stream};
use kaspa_addresses::Prefix as AddressPrefix;
use kaspa_bip32::{Language, Mnemonic};
use kaspa_consensus_core::networktype::{NetworkId, NetworkType};
use kaspa_notify::{
    listener::ListenerId,
    scope::{Scope, VirtualDaaScoreChangedScope},
};
use kaspa_rpc_core::{
    notify::{
        connection::{ChannelConnection, ChannelType},
        mode::NotificationMode,
    },
    Notification,
};
use kaspa_rpc_core::{GetBlockDagInfoResponse, GetInfoResponse};
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use storage::PubKeyData;
use workflow_core::channel::{Channel, DuplexChannel, Multiplexer};
use workflow_core::task::spawn;
use workflow_log::log_error;
use workflow_rpc::client::Ctl;
use zeroize::Zeroize;

pub struct WalletCreateArgs {
    pub name: Option<String>,
    pub user_hint: Option<String>,
    pub wallet_secret: Secret,
    pub overwrite_wallet_storage: bool,
}

impl WalletCreateArgs {
    pub fn new(name: Option<String>, user_hint: Option<String>, secret: Secret, overwrite_wallet_storage: bool) -> Self {
        Self { name, user_hint, wallet_secret: secret, overwrite_wallet_storage }
    }
}

impl From<(Option<String>, &WalletCreateArgs)> for CreateArgs {
    fn from((name, args): (Option<String>, &WalletCreateArgs)) -> Self {
        CreateArgs::new(name, args.user_hint.clone(), args.overwrite_wallet_storage)
    }
}

pub struct PrvKeyDataCreateArgs {
    pub name: Option<String>,
    pub wallet_secret: Secret,
    pub payment_secret: Option<Secret>,
    pub mnemonic: Option<String>,
}

impl PrvKeyDataCreateArgs {
    pub fn new(name: Option<String>, wallet_secret: Secret, payment_secret: Option<Secret>) -> Self {
        Self { name, wallet_secret, payment_secret, mnemonic: None }
    }

    pub fn new_with_mnemonic(
        name: Option<String>,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        mnemonic: Option<String>,
    ) -> Self {
        Self { name, wallet_secret, payment_secret, mnemonic }
    }
}

impl Zeroize for PrvKeyDataCreateArgs {
    fn zeroize(&mut self) {
        self.mnemonic.zeroize();
    }
}

#[derive(Clone)]
pub struct AccountCreateArgs {
    pub name: String,
    pub title: String,
    pub account_kind: storage::AccountKind,
    pub wallet_secret: Secret,
    pub payment_secret: Option<Secret>,
}

impl AccountCreateArgs {
    pub fn new(
        name: String,
        title: String,
        account_kind: storage::AccountKind,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
    ) -> Self {
        Self { name, title, account_kind, wallet_secret, payment_secret }
    }
}

pub struct NetworkInfo {
    pub network_id: NetworkId,
    pub prefix: AddressPrefix,
}

impl From<NetworkId> for NetworkInfo {
    fn from(network_id: NetworkId) -> Self {
        Self { network_id, prefix: network_id.into() }
    }
}
// impl From<NetworkType> for NetworkInfo {
//     fn from(network_type: NetworkType) -> Self {
//         Self { network_id: network_type, prefix: network_type.into() }
//     }
// }

pub struct Inner {
    active_accounts: AccountMap,
    listener_id: Mutex<Option<ListenerId>>,
    task_ctl: DuplexChannel,
    notification_channel: Channel<Notification>,
    selected_account: Mutex<Option<Arc<Account>>>,
    is_connected: AtomicBool,
    is_synced: AtomicBool,
    store: Arc<dyn Interface>,
    virtual_daa_score: Arc<AtomicU64>,
    network: Arc<Mutex<Option<NetworkInfo>>>,
    settings: SettingsStore<WalletSettings>,
    utxo_processor: Arc<UtxoProcessor>,
    rpc: Arc<DynRpcApi>,
    multiplexer: Multiplexer<Events>,
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
        Wallet::try_with_rpc(None, storage, network_id)
    }

    pub fn try_with_rpc(rpc: Option<Arc<KaspaRpcClient>>, store: Arc<dyn Interface>, network_id: Option<NetworkId>) -> Result<Wallet> {
        let rpc: Arc<DynRpcApi> = if let Some(rpc) = rpc {
            rpc
        } else {
            Arc::new(KaspaRpcClient::new_with_args(WrpcEncoding::Borsh, NotificationMode::MultiListeners, "wrpc://127.0.0.1:17110")?)
        };

        let multiplexer = Multiplexer::new();
        let utxo_processor = Arc::new(UtxoProcessor::new(&rpc));

        let wallet = Wallet {
            inner: Arc::new(Inner {
                rpc,
                multiplexer,
                store,
                active_accounts: AccountMap::default(),
                listener_id: Mutex::new(None),
                task_ctl: DuplexChannel::oneshot(),
                notification_channel: Channel::<Notification>::unbounded(),
                selected_account: Mutex::new(None),
                is_connected: AtomicBool::new(false),
                is_synced: AtomicBool::new(false),
                virtual_daa_score: Arc::new(AtomicU64::new(0)),
                network: Arc::new(Mutex::new(
                    network_id.map(|t| t.into()).or_else(|| Some(NetworkId::from(NetworkType::Mainnet).into())),
                )),
                settings: SettingsStore::new_with_storage(Storage::default_settings_store()), //::default(),
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

    pub fn name(&self) -> Option<String> {
        self.store().name()
    }

    pub fn store(&self) -> &Arc<dyn Interface> {
        &self.inner.store
    }

    pub fn active_accounts(&self) -> &AccountMap {
        &self.inner.active_accounts
    }

    pub async fn reset(&self) -> Result<()> {
        // log_info!("**** WALLET RESET ****");
        let accounts = self.inner.active_accounts.cloned_flat_list();
        let futures = accounts.iter().map(|account| account.stop());
        join_all(futures).await.into_iter().collect::<Result<Vec<_>>>()?;
        self.utxo_processor().clear().await?;

        Ok(())
    }

    pub async fn reload(self: &Arc<Self>) -> Result<()> {
        let accounts = self.inner.active_accounts.cloned_flat_list();
        let futures = accounts.iter().map(|account| account.stop());
        join_all(futures).await.into_iter().collect::<Result<Vec<_>>>()?;
        self.utxo_processor().clear().await?;

        // TODO - parallelize?
        if self.is_open()? {
            // log_info!("reloading accounts...");
            // let mut accounts = self.accounts(None).await?;
            let mut accounts = self.accounts(None).await?;
            while let Some(_account) = accounts.try_next().await? {
                //     account.start().await?;
                //     // TODO - parallelize?
            }

            let hint = self.store().get_user_hint().await?;
            self.notify(Events::WalletHasLoaded { hint }).await?;
        }

        Ok(())
    }

    pub async fn close(self: &Arc<Wallet>) -> Result<()> {
        self.reset().await?;
        self.store().close().await?;
        Ok(())
    }

    pub async fn load(self: &Arc<Wallet>, secret: Secret, name: Option<String>) -> Result<()> {
        // - TODO - RESET?
        self.reset().await?;

        let name = name.or_else(|| self.settings().get(WalletSettings::Wallet));
        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(secret));
        self.store().open(&ctx, OpenArgs::new(name.clone())).await?;
        let store_accounts = self.store().as_account_store()?;
        let mut iter = store_accounts.iter(None).await?;

        while let Some(stored_account) = iter.try_next().await? {
            let account = Account::try_new_arc_from_storage(self, &stored_account).await?;
            account.start().await?;
        }

        let hint = self.store().get_user_hint().await?;
        self.notify(Events::WalletHasLoaded { hint }).await?;

        Ok(())
    }

    pub async fn get_prv_key_data(&self, wallet_secret: Secret, id: &PrvKeyDataId) -> Result<Option<PrvKeyData>> {
        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
        self.inner.store.as_prv_key_data_store()?.load_key_data(&ctx, id).await
    }

    pub async fn get_prv_key_info(&self, account: &Account) -> Result<Option<Arc<PrvKeyDataInfo>>> {
        self.inner.store.as_prv_key_data_store()?.load_key_info(&account.prv_key_data_id).await
    }

    pub async fn is_account_key_encrypted(&self, account: &Account) -> Result<Option<bool>> {
        Ok(self.get_prv_key_info(account).await?.map(|info| info.is_encrypted()))
    }

    pub fn rpc_client(&self) -> Arc<KaspaRpcClient> {
        self.rpc().clone().downcast_arc::<KaspaRpcClient>().expect("unable to downcast DynRpcApi to KaspaRpcClient")
    }

    pub fn rpc(&self) -> &Arc<DynRpcApi> {
        &self.inner.rpc
    }

    pub fn multiplexer(&self) -> &Multiplexer<Events> {
        &self.inner.multiplexer
    }

    pub fn settings(&self) -> &SettingsStore<WalletSettings> {
        &self.inner.settings
    }

    pub fn current_daa_score(&self) -> u64 {
        self.inner.virtual_daa_score.load(Ordering::SeqCst)
    }

    pub async fn load_settings(&self) -> Result<()> {
        self.settings().try_load().await?;

        let settings = self.settings();

        if let Some(network_type) = settings.get(WalletSettings::Network) {
            self.select_network(network_type).unwrap_or_else(|_| log_error!("Unable to select network type: `{}`", network_type));
        }

        if let Some(url) = settings.get::<String>(WalletSettings::Server) {
            self.rpc_client().set_url(url.as_str()).unwrap_or_else(|_| log_error!("Unable to set rpc url: `{}`", url));
        }

        Ok(())
    }

    // intended for starting async management tasks
    pub async fn start(self: &Arc<Self>) -> Result<()> {
        self.load_settings().await.unwrap_or_else(|_| log_error!("Unable to load settings, discarding..."));
        // internal event loop
        self.start_task().await?;
        self.utxo_processor().start(Some(self.clone())).await?;
        // rpc services (notifier)

        self.rpc_client().start().await?;

        // self.subscribe_daa_score().await?;
        // start async RPC connection

        // TODO handle reconnect flag
        // self.rpc.connect_as_task()?;
        Ok(())
    }

    // intended for stopping async management task
    pub async fn stop(&self) -> Result<()> {
        // self.unsubscribe_daa_score().await?;
        self.utxo_processor().stop().await?;
        self.stop_task().await?;
        self.rpc_client().stop().await?;
        self.rpc_client().disconnect().await?;
        Ok(())
    }

    pub fn listener_id(&self) -> ListenerId {
        self.inner.listener_id.lock().unwrap().expect("missing wallet.inner.listener_id in Wallet::listener_id()")
    }

    pub async fn get_info(&self) -> Result<String> {
        let v = self.rpc().get_info().await?;
        Ok(format!("{v:#?}").replace('\n', "\r\n"))
    }

    pub async fn subscribe_daa_score(&self) -> Result<()> {
        self.rpc().start_notify(self.listener_id(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    pub async fn unsubscribe_daa_score(&self) -> Result<()> {
        self.rpc().stop_notify(self.listener_id(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    pub async fn ping(&self) -> bool {
        self.rpc().ping().await.is_ok()
    }

    pub async fn broadcast(&self) -> Result<()> {
        Ok(())
    }

    pub fn select_network(&self, network_id: NetworkId) -> Result<()> {
        if self.is_connected() {
            return Err(Error::NetworkTypeConnected);
        }
        let network_info = NetworkInfo::from(network_id);
        *self.inner.network.lock().unwrap() = Some(network_info);
        Ok(())
    }

    pub fn network(&self) -> Result<NetworkId> {
        let network = self.inner.network.lock().unwrap();
        let network = network.as_ref().ok_or(Error::MissingNetworkId)?;
        Ok(network.network_id)
    }

    pub fn address_prefix(&self) -> Result<AddressPrefix> {
        Ok(self.network()?.into())
    }

    pub fn default_port(&self) -> Result<u16> {
        let network_type = self.network()?;
        let port = match self.rpc_client().encoding() {
            WrpcEncoding::Borsh => network_type.default_borsh_rpc_port(),
            WrpcEncoding::SerdeJson => network_type.default_json_rpc_port(),
        };
        Ok(port)
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

    pub async fn create_bip32_account(
        self: &Arc<Wallet>,
        // wallet_secret: Option<Secret>,
        // payment_secret: Option<Secret>,
        prv_key_data_id: PrvKeyDataId,
        args: AccountCreateArgs,
    ) -> Result<Arc<Account>> {
        let account_storage = self.inner.store.clone().as_account_store()?;
        let account_index = account_storage.clone().len(Some(prv_key_data_id)).await? as u64;
        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(args.wallet_secret));
        let prv_key_data = self
            .inner
            .store
            .as_prv_key_data_store()?
            .load_key_data(&ctx, &prv_key_data_id)
            .await?
            .ok_or(Error::PrivateKeyNotFound(prv_key_data_id.to_hex()))?;
        let xpub_key = prv_key_data.create_xpub(args.payment_secret.as_ref(), args.account_kind, account_index).await?;
        let xpub_prefix = kaspa_bip32::Prefix::XPUB;
        let pub_key_data = PubKeyData::new(vec![xpub_key.to_string(Some(xpub_prefix))], None, None);

        let stored_account = storage::Account::new(
            args.name.clone(),
            args.title.clone(),
            args.account_kind,
            account_index,
            false,
            pub_key_data,
            prv_key_data.id,
            false,
            1,
            0,
        );

        account_storage.store(&[&stored_account]).await?;
        self.inner.store.clone().commit(&ctx).await?;
        let account = Account::try_new_arc_from_storage(self, &stored_account).await?;
        account.start().await?;

        Ok(account)
    }

    pub async fn create_wallet(self: &Arc<Wallet>, args: WalletCreateArgs) -> Result<Option<String>> {
        self.reset().await?;

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(args.wallet_secret.clone()));
        self.inner.store.create(&ctx, (None, &args).into()).await?;
        let descriptor = self.inner.store.descriptor()?;
        self.inner.store.commit(&ctx).await?;
        Ok(descriptor)
    }

    pub async fn create_prv_key_data(self: &Arc<Wallet>, args: PrvKeyDataCreateArgs) -> Result<(PrvKeyDataId, Mnemonic)> {
        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(args.wallet_secret.clone()));
        let mnemonic = if let Some(mnemonic) = args.mnemonic.as_ref() {
            let mnemonic = mnemonic.to_string();
            Mnemonic::new(mnemonic, Language::English)?
        } else {
            Mnemonic::create_random()?
        };
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
    ) -> Result<(Mnemonic, Option<String>, Arc<Account>)> {
        self.reset().await?;

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(account_args.wallet_secret));

        self.inner.store.create(&ctx, (None, &wallet_args).into()).await?;
        let descriptor = self.inner.store.descriptor()?;
        // let prefix = self.address_prefix()?;
        let xpub_prefix = kaspa_bip32::Prefix::XPUB;
        let mnemonic = Mnemonic::create_random()?;
        let account_index = 0;
        let prv_key_data = PrvKeyData::try_from((mnemonic.clone(), account_args.payment_secret.as_ref()))?;
        let xpub_key =
            prv_key_data.create_xpub(account_args.payment_secret.as_ref(), account_args.account_kind, account_index).await?;
        let pub_key_data = PubKeyData::new(vec![xpub_key.to_string(Some(xpub_prefix))], None, None);

        let stored_account = storage::Account::new(
            account_args.title.replace(' ', "-").to_lowercase(),
            account_args.title.clone(),
            account_args.account_kind,
            account_index,
            false,
            pub_key_data,
            prv_key_data.id,
            false,
            1,
            0,
        );

        let prv_key_data_store = self.inner.store.as_prv_key_data_store()?;
        prv_key_data_store.store(&ctx, prv_key_data).await?;
        let account_store = self.inner.store.as_account_store()?;
        account_store.store(&[&stored_account]).await?;
        self.inner.store.commit(&ctx).await?;

        let account = Account::try_new_arc_from_storage(self, &stored_account).await?;
        self.select(Some(&account)).await?;

        Ok((mnemonic, descriptor, account))
    }

    pub async fn dump_unencrypted(&self) -> Result<()> {
        Ok(())
    }

    pub async fn select(self: &Arc<Self>, account: Option<&Arc<Account>>) -> Result<()> {
        *self.inner.selected_account.lock().unwrap() = account.cloned();
        if let Some(account) = account {
            // log_info!("selecting account: {}", account.name_or_id());
            account.start().await?;
        }
        Ok(())
    }

    pub fn account(&self) -> Result<Arc<Account>> {
        self.inner.selected_account.lock().unwrap().clone().ok_or_else(|| Error::AccountSelection)
    }

    pub fn account_with_id(&self, account_id: &AccountId) -> Result<Option<Arc<Account>>> {
        Ok(self.inner.active_accounts.get(account_id))
    }

    pub async fn import_gen0_keydata(self: &Arc<Wallet>, import_secret: Secret, wallet_secret: Secret) -> Result<Arc<Account>> {
        let keydata = load_v0_keydata(&import_secret).await?;

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));

        let mnemonic = Mnemonic::new(keydata.mnemonic.trim(), Language::English)?;
        let prv_key_data = PrvKeyData::try_new_from_mnemonic(mnemonic, None)?;
        let prv_key_data_store = self.inner.store.as_prv_key_data_store()?;
        if prv_key_data_store.load_key_data(&ctx, &prv_key_data.id).await?.is_some() {
            return Err(Error::PrivateKeyAlreadyExists(prv_key_data.id.to_hex()));
        }

        // TODO: integrate address generation
        // let derivation_path = gen1::WalletAccount::build_derivate_path(false, 0, Some(kaspa_bip32::AddressType::Receive))?;
        // let xkey = ExtendedPrivateKey::<SecretKey>::from_str(xprv)?.derive_path(derivation_path)?;

        let stored_account = storage::Account::new(
            "imported-wallet".to_string(),       // name
            "Imported Wallet".to_string(),       // title
            storage::AccountKind::Legacy,        // kind
            0,                                   // account index
            false,                               // public visibility
            PubKeyData::new(vec![], None, None), // TODO - pub keydata
            prv_key_data.id,                     // privkey id
            false,                               // ecdsa
            1,                                   // min signatures
            0,                                   // cosigner_index
        );

        // let prefix = AddressPrefix::Mainnet;

        let account = Account::try_new_arc_from_storage(self, &stored_account).await?;

        prv_key_data_store.store(&ctx, prv_key_data).await?;
        let account_store = self.inner.store.as_account_store()?;
        account_store.store(&[&stored_account]).await?;
        self.inner.store.commit(&ctx).await?;
        // payload.prv_key_data.push(prv_key_data);
        // // TODO - prevent multiple addition of the same private key
        // payload.accounts.push(stored_account);

        // self.inner.account_map.insert(Arc::new(runtime_account))?;
        // account.start().await?;

        Ok(account)
    }

    pub async fn import_gen1_keydata(self: &Arc<Wallet>, secret: Secret) -> Result<()> {
        let _keydata = load_v1_keydata(&secret).await?;

        Ok(())
    }

    pub async fn init_state_from_server(self: &Arc<Self>) -> Result<()> {
        let GetInfoResponse { is_synced, is_utxo_indexed: has_utxo_index, server_version, .. } = self.rpc().get_info().await?;
        let network = self.rpc().get_current_network().await?;

        if !has_utxo_index {
            self.notify(Events::UtxoIndexNotEnabled).await?;
            return Err(Error::MissingUtxoIndex);
        }

        if let Some(network_id) = self.inner.network.lock().unwrap().as_ref() {
            let current_network_type = network_id.network_id.as_type();
            if current_network_type != &network {
                return Err(Error::InvalidNetworkType(current_network_type.to_string(), network.to_string()));
            }
        } else {
            return Err(Error::MissingNetworkId);
            // *self.inner.network.lock().unwrap() = Some(network.into());
        }

        let GetBlockDagInfoResponse { virtual_daa_score, .. } = self.rpc().get_block_dag_info().await?;

        self.inner.virtual_daa_score.store(virtual_daa_score, Ordering::SeqCst);

        self.inner.is_synced.store(is_synced, Ordering::SeqCst);
        self.notify(Events::ServerStatus { server_version, is_synced, has_utxo_index, url: self.rpc_client().url().to_string() })
            .await?;

        Ok(())
    }

    pub async fn notify(&self, event: Events) -> Result<()> {
        self.multiplexer()
            .broadcast(event)
            .await
            .map_err(|_| Error::Custom("multiplexer channel error during update_balance".to_string()))?;
        Ok(())
    }

    pub fn is_synced(&self) -> bool {
        self.inner.is_synced.load(Ordering::SeqCst)
    }

    /// handle connection event
    pub async fn handle_connect(self: &Arc<Self>) -> Result<()> {
        self.init_state_from_server().await?;
        // self.inner.is_synced.store(false, Ordering::SeqCst);
        self.inner.is_connected.store(true, Ordering::SeqCst);
        self.register_notification_listener().await?;

        self.subscribe_daa_score().await?;

        if self.inner.is_synced.load(Ordering::SeqCst) {
            // log_info!("executing reload...");
            self.reload().await?;
        }

        Ok(())
    }

    /// handle disconnection event
    pub async fn handle_disconnect(self: &Arc<Self>) -> Result<()> {
        self.utxo_processor().handle_disconnect().await?;
        self.inner.is_connected.store(false, Ordering::SeqCst);
        self.unsubscribe_daa_score().await?;
        self.unregister_notification_listener().await?;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.inner.is_connected.load(Ordering::SeqCst)
    }

    async fn register_notification_listener(&self) -> Result<()> {
        let listener_id = self
            .rpc()
            .register_new_listener(ChannelConnection::new(self.inner.notification_channel.sender.clone(), ChannelType::Persistent));
        *self.inner.listener_id.lock().unwrap() = Some(listener_id);

        Ok(())
    }

    async fn unregister_notification_listener(&self) -> Result<()> {
        let listener_id = self.inner.listener_id.lock().unwrap().take();
        if let Some(id) = listener_id {
            // we do not need this as we are unregister the entire listener here...
            // self.rpc.stop_notify(id, Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
            self.rpc().unregister_listener(id).await?;
        }
        Ok(())
    }

    async fn handle_notification(&self, _notification: Notification) -> Result<()> {
        Ok(())
    }

    pub async fn start_task(self: &Arc<Self>) -> Result<()> {
        let this = self.clone();
        let rpc_ctl_channel = self.rpc_client().ctl_multiplexer().create_channel();

        let task_ctl_receiver = self.inner.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.inner.task_ctl.response.sender.clone();
        let multiplexer = self.multiplexer().clone();
        let notification_receiver = self.inner.notification_channel.receiver.clone();

        spawn(async move {
            loop {
                select! {
                    _ = task_ctl_receiver.recv().fuse() => {
                        break;
                    },

                    msg = rpc_ctl_channel.receiver.recv().fuse() => {
                        match msg {
                            Ok(msg) => {

                                match msg {
                                    Ctl::Open => {
                                        multiplexer.broadcast(Events::Connect(this.rpc_client().url().to_string())).await.unwrap_or_else(|err| log_error!("{err}"));
                                        this.handle_connect().await.unwrap_or_else(|err| log_error!("{err}"));
                                    },
                                    Ctl::Close => {
                                        multiplexer.broadcast(Events::Disconnect(this.rpc_client().url().to_string())).await.unwrap_or_else(|err| log_error!("{err}"));
                                        this.handle_disconnect().await.unwrap_or_else(|err| log_error!("{err}"));
                                    }
                                }

                            },
                            Err(err) => {
                                log_error!("{err}");
                                log_error!("Suspending Wallet processing...");
                            }
                        }
                    }

                    notification = notification_receiver.recv().fuse() => {
                        match notification {
                            Ok(notification) => {
                                this.handle_notification(notification).await.unwrap_or_else(|err| {
                                    log_error!("error while handling notification: {err}");
                                });
                            },
                            Err(err) => {
                                log_error!("RPC notification channel error: {err}");
                                log_error!("Suspending Wallet notification processing...");
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

    pub async fn stop_task(&self) -> Result<()> {
        self.inner.task_ctl.signal(()).await.expect("Wallet::stop_task() `signal` error");
        Ok(())
    }

    // pub async fn connect(&self) -> Result<()> {
    //     for account in self.inner.accounts.iter() {
    //         account.connect().await?;
    //     }
    //     Ok(())
    // }

    // pub async fn disconnect(&self) -> Result<()> {
    //     for account in self.inner.accounts.iter() {
    //         account.disconnect().await?;
    //     }
    //     Ok(())
    // }

    // pub fn store(&self) -> &Arc<

    pub fn is_open(&self) -> Result<bool> {
        self.inner.store.is_open()
    }

    pub fn descriptor(&self) -> Result<Option<String>> {
        self.inner.store.descriptor()
    }

    pub async fn exists(&self, name: Option<&str>) -> Result<bool> {
        self.inner.store.exists(name).await
    }

    pub async fn keys(&self) -> Result<impl Stream<Item = Result<Arc<PrvKeyDataInfo>>>> {
        self.inner.store.as_prv_key_data_store()?.iter().await
    }

    pub async fn accounts(self: &Arc<Self>, filter: Option<PrvKeyDataId>) -> Result<impl Stream<Item = Result<Arc<Account>>>> {
        let iter = self.inner.store.as_account_store().unwrap().iter(filter).await.unwrap();
        let wallet = self.clone();

        let stream = iter.then(move |stored| {
            let wallet = wallet.clone();
            async move {
                let stored = stored.unwrap();
                if let Some(account) = wallet.active_accounts().get(&stored.id) {
                    log_info!("fetching active account: {}", account.id);
                    Ok(account)
                } else {
                    let account = Account::try_new_arc_from_storage(&wallet, &stored).await?;
                    log_info!("starting new active account instance {}", account.id);
                    account.start().await?;
                    Ok(account)
                }
            }
        });

        Ok(Box::pin(stream))
    }
}

#[async_trait]
impl utxo::EventConsumer for Wallet {
    async fn notify(&self, event: utxo::Events) -> Result<()> {
        self.notify(Events::UtxoProcessor(event)).await?;
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod test {
    use std::{str::FromStr, thread::sleep, time};

    use super::*;
    use crate::{signer::sign_mutable_transaction, tx::MutableTransaction, utxo::UtxoContext};

    // TODO - re-export subnets
    use crate::tx::Transaction;
    use crate::tx::TransactionInput;
    use crate::tx::TransactionOutput;
    use kaspa_consensus_core::subnets::SubnetworkId;
    //use kaspa_consensus_core::tx::ScriptPublicKey;
    //use kaspa_consensus_core::tx::MutableTransaction;
    use kaspa_addresses::{Address, Prefix, Version};
    use kaspa_bip32::{ChildNumber, ExtendedPrivateKey, SecretKey};
    use kaspa_rpc_core::api::rpc::RpcApi;
    use kaspa_txscript::pay_to_address_script;
    use workflow_rpc::client::ConnectOptions;

    // async fn get_utxos_set_by_addresses(rpc: Arc<KaspaRpcClient>, addresses: Vec<Address>) -> Result<UtxoSet> {
    async fn get_utxos_set_by_addresses(
        rpc: Arc<DynRpcApi>,
        addresses: Vec<Address>,
        current_daa_score: u64,
        core: &UtxoProcessor,
    ) -> Result<UtxoContext> {
        let utxos = rpc.get_utxos_by_addresses(addresses).await?;
        let utxo_set = UtxoContext::new(core);
        let entries = utxos.into_iter().map(|entry| entry.into()).collect::<Vec<_>>();
        for entry in entries.into_iter() {
            utxo_set.insert(entry, current_daa_score).await?;
        }
        Ok(utxo_set)
    }

    #[allow(dead_code)]
    // #[tokio::test]
    async fn wallet_test() -> Result<()> {
        println!("Creating wallet...");
        let resident_store = Wallet::resident_store()?;
        let wallet = Arc::new(Wallet::try_new(resident_store, None)?);
        // let stored_accounts = vec![StoredWalletAccount{
        //     private_key_index: 0,
        //     account_kind: crate::storage::AccountKind::Bip32,
        //     name: "Default Account".to_string(),
        //     title: "Default Account".to_string(),
        // }];
        // let utxo_db_ctx = wallet.utxo_db_core().ctx(self)
        // wallet.load_accounts(stored_accounts);

        let rpc = wallet.rpc();
        let utxo_processor = UtxoProcessor::new(rpc);

        let rpc_client = wallet.rpc_client();

        let info = rpc_client.get_block_dag_info().await?;
        let current_daa_score = info.virtual_daa_score;

        let _connect_result = rpc_client.connect(ConnectOptions::fallback()).await;
        //println!("connect_result: {_connect_result:?}");

        let _result = wallet.start().await;
        //println!("wallet.task(): {_result:?}");
        let result = wallet.get_info().await;
        println!("wallet.get_info(): {result:#?}");

        let address = Address::try_from("kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd")?;

        let utxo_set =
            self::get_utxos_set_by_addresses(rpc.clone(), vec![address.clone()], current_daa_score, &utxo_processor).await?;

        let utxo_set_balance = utxo_set.calculate_balance().await;
        println!("get_utxos_by_addresses: {utxo_set_balance:?}");

        let mut ctx = utxo_set.create_selection_context();
        // let mut ctx = UtxoSelectionContext::new(utxo_set);
        let selected_entries = ctx.select(100_000).await?;

        // let utxo_selection = utxo_set.select(100000, UtxoOrdering::AscendingAmount, true).await?;

        //let payload = vec![];
        let to_address = Address::try_from("kaspatest:qpakxqlesqywgkq7rg4wyhjd93kmw7trkl3gpa3vd5flyt59a43yyn8vu0w8c")?;
        //let outputs = Outputs { outputs: vec![Output::new(to_address, 100000, None)] };
        //let vtx = VirtualTransaction::new(utxo_selection, &outputs, payload);

        //vtx.sign();
        let utxo = (*selected_entries[0].utxo).clone();
        //utxo.utxo_entry.is_coinbase = false;
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
            vec![
                TransactionOutput::new(1000, &pay_to_address_script(&to_address)),
                // TransactionOutput::new() { value: 1000, script_public_key: pay_to_address_script(&to_address) },
                //TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) },
            ],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        )?;

        let mtx = MutableTransaction::new(&tx, &(*entries).clone().into());

        let derivation_path =
            gen1::WalletDerivationManager::build_derivate_path(false, 0, None, Some(kaspa_bip32::AddressType::Receive))?;

        let xprv = "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ";
        //let (xkey, _attrs) = WalletAccount::create_extended_key_from_xprv(xprv, false, 0).await?;

        let xkey = ExtendedPrivateKey::<SecretKey>::from_str(xprv)?.derive_path(derivation_path)?;

        let xkey = xkey.derive_child(ChildNumber::new(0, false)?)?;

        // address test
        let address_test = Address::new(Prefix::Testnet, Version::PubKey, &xkey.public_key().to_bytes()[1..]);
        let address_str: String = address_test.clone().into();
        assert_eq!(address, address_test, "Address dont match");
        println!("address: {address_str}");

        let private_keys = vec![
            //xkey.private_key().into()
            xkey.to_bytes(),
        ];

        println!("mtx: {mtx:?}");

        //let signer = Signer::new(private_keys)?;
        let mtx = sign_mutable_transaction(mtx, &private_keys, true)?;
        //println!("mtx: {mtx:?}");

        let utxo_set =
            self::get_utxos_set_by_addresses(rpc.clone(), vec![to_address.clone()], current_daa_score, &utxo_processor).await?;
        let to_balance = utxo_set.calculate_balance().await;
        println!("to address balance before tx submit: {to_balance:?}");

        let result = rpc.submit_transaction(mtx.try_into()?, false).await?;

        println!("tx submit result, {:?}", result);
        println!("sleep for 5s...");
        sleep(time::Duration::from_millis(5000));
        let utxo_set =
            self::get_utxos_set_by_addresses(rpc.clone(), vec![to_address.clone()], current_daa_score, &utxo_processor).await?;
        let to_balance = utxo_set.calculate_balance().await;
        println!("to address balance after tx submit: {to_balance:?}");

        Ok(())
    }
}
