// use crate::iterator::*;
use crate::result::Result;
// use crate::runtime::iterators::*;
use crate::runtime::{Account, AccountId, AccountMap};
use crate::secret::Secret;
use crate::storage::interface::{AccessContext, CreateArgs};
use crate::storage::local::interface::LocalStore;
use crate::storage::{self, AccessContextT, Interface, PrvKeyData, PrvKeyDataId, PrvKeyDataInfo};
use crate::utxo::UtxoEntryReference;
#[allow(unused_imports)]
use crate::{accounts::gen0, accounts::gen0::import::*, accounts::gen1, accounts::gen1::import::*};
use crate::{imports::*, DynRpcApi};
use futures::future::join_all;
use futures::stream::StreamExt;
use futures::{select, FutureExt, Stream};
use kaspa_addresses::Prefix as AddressPrefix;
use kaspa_bip32::Mnemonic;
use kaspa_consensus_core::networktype::NetworkType;
use kaspa_notify::{
    listener::ListenerId,
    scope::{Scope, VirtualDaaScoreChangedScope},
};
use kaspa_rpc_core::{
    notify::{connection::ChannelConnection, mode::NotificationMode},
    Notification,
};
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use storage::PubKeyData;
use workflow_core::channel::{Channel, DuplexChannel, Multiplexer, Receiver};
use workflow_core::task::spawn;
use workflow_log::log_error;
use workflow_rpc::client::Ctl;

pub struct WalletCreateArgs {
    pub name: Option<String>,
    pub user_hint: Option<String>,
    pub overwrite_wallet: bool,
}

impl WalletCreateArgs {
    pub fn new(name: Option<String>, user_hint: Option<String>, overwrite_wallet: bool) -> Self {
        Self { name, user_hint, overwrite_wallet }
    }
}

impl From<(Option<String>, &WalletCreateArgs)> for CreateArgs {
    fn from((name, args): (Option<String>, &WalletCreateArgs)) -> Self {
        CreateArgs::new(name, args.user_hint.clone(), args.overwrite_wallet)
    }
}

#[derive(Clone)]
pub struct AccountCreateArgs {
    pub name: String,
    pub title: String,
    pub account_kind: storage::AccountKind,
    pub wallet_password: Secret,
    pub payment_password: Option<Secret>,
}

impl AccountCreateArgs {
    pub fn new(
        name: String,
        title: String,
        account_kind: storage::AccountKind,
        wallet_password: Secret,
        payment_password: Option<Secret>,
    ) -> Self {
        Self { name, title, account_kind, wallet_password, payment_password }
    }
}

#[derive(Debug)]
pub struct BalanceUpdate {
    pub balance: u64,
    pub account_id: AccountId,
}

#[derive(Clone, Debug)]
pub enum Events {
    Connect,
    Disconnect,
    DAAScoreChange(u64),
    Balance(Arc<BalanceUpdate>),
}

pub struct Inner {
    active_accounts: AccountMap,
    listener_id: Mutex<Option<ListenerId>>,

    #[allow(dead_code)] //TODO: remove me
    ctl_receiver: Receiver<Ctl>,
    pub task_ctl: DuplexChannel,
    pub selected_account: Mutex<Option<Arc<Account>>>,
    pub is_connected: AtomicBool,

    pub notification_channel: Channel<Notification>,
    // ---
    pub address_to_account_map: Arc<Mutex<HashMap<Address, Arc<Account>>>>,
    // ---
    pub store: Arc<dyn Interface>,
}

/// `Wallet` data structure
#[derive(Clone)]
// #[wasm_bindgen]
pub struct Wallet {
    // #[wasm_bindgen(skip)]
    pub rpc: Arc<DynRpcApi>,
    // #[wasm_bindgen(skip)]
    pub multiplexer: Multiplexer<Events>,
    // #[wasm_bindgen(skip)]
    // pub rpc_client: Arc<KaspaRpcClient>,
    inner: Arc<Inner>,
    // #[wasm_bindgen(skip)]
    pub virtual_daa_score: Arc<AtomicU64>,
}

impl Wallet {
    pub async fn try_new() -> Result<Wallet> {
        Wallet::try_with_rpc(None).await
    }

    pub async fn try_with_rpc(rpc: Option<Arc<KaspaRpcClient>>) -> Result<Wallet> {
        // let _master_xprv =
        //     "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ";

        let rpc = if let Some(rpc) = rpc {
            rpc
        } else {
            // Arc::new(KaspaRpcClient::new_with_args(WrpcEncoding::Borsh, NotificationMode::Direct, "wrpc://localhost:17110")?)
            Arc::new(KaspaRpcClient::new_with_args(WrpcEncoding::Borsh, NotificationMode::MultiListeners, "wrpc://127.0.0.1:17110")?)
        };

        // let (listener_id, notification_receiver) = match rpc.notification_mode() {
        //     NotificationMode::MultiListeners => {
        //         let notification_channel = Channel::unbounded();
        //         let connection = ChannelConnection::new(notification_channel.sender);
        //         (rpc.register_new_listener(connection), notification_channel.receiver)
        //     }
        //     NotificationMode::Direct => (ListenerId::default(), rpc.notification_channel_receiver()),
        // };

        let ctl_receiver = rpc.ctl_channel_receiver();

        let multiplexer = Multiplexer::new();

        // let store = Arc::new(LocalStore::try_new(None, storage::local::DEFAULT_WALLET_FILE)?);
        let store = Arc::new(LocalStore::try_new()?);

        let wallet = Wallet {
            // rpc_client : rpc.clone(),
            rpc,
            multiplexer,
            virtual_daa_score: Arc::new(AtomicU64::new(0)),
            inner: Arc::new(Inner {
                store,
                active_accounts: AccountMap::default(),
                // account_list: Mutex::new(AccountList::default()),
                // notification_receiver,
                listener_id: Mutex::new(None),
                ctl_receiver,
                task_ctl: DuplexChannel::oneshot(),
                selected_account: Mutex::new(None),
                is_connected: AtomicBool::new(false),
                notification_channel: Channel::<Notification>::unbounded(),
                address_to_account_map: Arc::new(Mutex::new(HashMap::new())),
            }),
        };

        Ok(wallet)
    }

    pub fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    pub fn store(&self) -> &Arc<dyn Interface> {
        &self.inner.store
    }

    pub fn active_accounts(&self) -> &AccountMap {
        &self.inner.active_accounts
    }

    pub fn address_to_account_map(&self) -> &Arc<Mutex<HashMap<Address, Arc<Account>>>> {
        &self.inner.address_to_account_map
    }

    pub async fn reset(&self) -> Result<()> {
        let accounts = self.inner.active_accounts.cloned_flat_list();
        let futures = accounts.iter().map(|account| account.stop());
        join_all(futures).await.into_iter().collect::<Result<Vec<_>>>()?;
        self.inner.address_to_account_map.lock().unwrap().clear();

        Ok(())
    }

    // pub fn load_accounts(&self, stored_accounts: Vec<storage::Account>) => Result<()> {
    pub async fn load(self: &Arc<Wallet>, secret: Secret, _prefix: AddressPrefix) -> Result<()> {
        // - TODO - RESET?
        self.reset().await?;

        use storage::interface::*;
        use storage::local::interface::*;

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(Some(secret)));
        // let ctx: Arc<dyn AccessContextT> = ctx;
        // let local_store = Arc::new(LocalStore::try_new(None, storage::local::DEFAULT_WALLET_FILE)?);
        let local_store = Arc::new(LocalStore::try_new()?);
        local_store.open(&ctx, OpenArgs::new(None)).await?;
        // let iface : Arc<dyn Interface> = local_store;
        let store_accounts = local_store.as_account_store()?;
        let mut iter = store_accounts.iter(None).await?;
        // pin_mut!(iter);
        // let mut iter = Box::pin(iter);
        // let mut iter = iter;
        // pin!(iter);
        // let v = iter;
        // while let Some(ids) = iter.next().await {

        // iter.for_each()

        while let Some(_accounts) = iter.try_next().await? {
            // let accounts = store_accounts.load(&ctx, &ids).await?;

            // let account = accounts?;

            // let accounts = accounts.iter().map(|stored| Account::try_new_arc_from_storage(self, stored, prefix)).collect::<Vec<_>>();
            // let _accounts = join_all(accounts).await.into_iter().collect::<Result<Vec<_>>>()?;
            // let accounts = accounts.into_iter().map(Arc::new).collect::<Vec<_>>();

            todo!();
            // self.inner.account_map.extend(accounts)?;
        }

        // let store = storage::local::Store::default();
        // let wallet = storage::local::Wallet::try_load(&store).await?;
        // let payload = wallet.payload.decrypt::<storage::Payload>(secret)?;

        // let accounts =
        //     payload.as_ref().accounts.iter().map(|stored| Account::try_new_from_storage(self, stored, prefix)).collect::<Vec<_>>();
        // let accounts = join_all(accounts).await.into_iter().collect::<Result<Vec<_>>>()?;
        // let accounts = accounts.into_iter().map(Arc::new).collect::<Vec<_>>();

        // self.inner.account_map.extend(accounts)?;

        Ok(())
    }

    pub async fn get_prv_key_data(&self, wallet_secret: Secret, id: &PrvKeyDataId) -> Result<Option<PrvKeyData>> {
        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(Some(wallet_secret)));
        self.inner.store.as_prv_key_data_store()?.load_key_data(&ctx, id).await
    }

    pub async fn get_prv_key_info(&self, account: &Account) -> Result<Option<Arc<PrvKeyDataInfo>>> {
        self.inner.store.as_prv_key_data_store()?.load_key_info(&account.prv_key_data_id).await
    }

    pub async fn is_account_key_encrypted(&self, account: &Account) -> Result<Option<bool>> {
        Ok(self.get_prv_key_info(account).await?.map(|info| info.is_encrypted))
    }

    pub fn rpc_client(&self) -> Arc<KaspaRpcClient> {
        self.rpc.clone().downcast_arc::<KaspaRpcClient>().expect("unable to downcast DynRpcApi to KaspaRpcClient")
    }

    pub fn rpc(&self) -> &Arc<DynRpcApi> {
        &self.rpc
    }

    pub fn multiplexer(&self) -> &Multiplexer<Events> {
        &self.multiplexer
    }

    // intended for starting async management tasks
    pub async fn start(&self) -> Result<()> {
        // internal event loop
        self.start_task().await?;
        // rpc services (notifier)
        self.rpc_client().start().await?;
        // start async RPC connection

        // TODO handle reconnect flag
        // self.rpc.connect_as_task()?;
        Ok(())
    }

    // intended for stopping async management task
    pub async fn stop(&self) -> Result<()> {
        self.rpc_client().stop().await?;

        // self.rpc.stop().await?;
        self.stop_task().await?;
        Ok(())
    }

    pub fn listener_id(&self) -> ListenerId {
        self.inner.listener_id.lock().unwrap().expect("missing wallet.inner.listener_id in Wallet::listener_id()")
    }

    // pub fn notification_channel_receiver(&self) -> Receiver<Notification> {
    //     self.inner.notification_receiver.clone()
    // }

    // ~~~

    pub async fn get_info(&self) -> Result<String> {
        let v = self.rpc.get_info().await?;
        Ok(format!("{v:#?}").replace('\n', "\r\n"))
    }

    pub async fn subscribe_daa_score(&self) -> Result<()> {
        self.rpc.start_notify(self.listener_id(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    pub async fn unsubscribe_daa_score(&self) -> Result<()> {
        self.rpc.stop_notify(self.listener_id(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    pub async fn ping(&self) -> Result<()> {
        Ok(self.rpc.ping().await?)
    }

    pub async fn broadcast(&self) -> Result<()> {
        Ok(())
    }

    pub fn network(&self) -> NetworkType {
        NetworkType::Mainnet
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
        wallet_secret: Option<Secret>,
        payment_secret: Option<Secret>,
        prv_key_data_id: PrvKeyDataId,
        args: &AccountCreateArgs,
    ) -> Result<Arc<Account>> {
        let prefix: AddressPrefix = self.network().into();

        let account_storage = self.inner.store.clone().as_account_store()?;
        let account_index = account_storage.clone().len(Some(prv_key_data_id)).await? as u64;

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
        let prv_key_data = self
            .inner
            .store
            .as_prv_key_data_store()?
            .load_key_data(&ctx, &prv_key_data_id)
            .await?
            .ok_or(Error::PrivateKeyNotFound(prv_key_data_id.to_hex()))?;

        let xpub_key = prv_key_data.create_xpub(payment_secret, args.account_kind, account_index).await?;
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

        let account = Account::try_new_arc_from_storage(self, &stored_account, prefix).await?;
        // self.inner.connected_accounts.insert(account.clone())?;

        // - TODO autoload ???

        account.start().await?;

        Ok(account)
    }

    pub async fn create_wallet(
        self: &Arc<Wallet>,
        wallet_args: &WalletCreateArgs,
        account_args: &AccountCreateArgs,
    ) -> Result<(Mnemonic, Option<String>)> {
        log_info!("running create_wallet A");
        self.reset().await?;

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(Some(account_args.wallet_password.clone())));

        // self.inner.store.create(&ctx, CreateArgs::new(None, wallet_args.override_wallet)).await?;
        self.inner.store.create(&ctx, (None, wallet_args).into()).await?;
        let descriptor = self.inner.store.descriptor().await?;

        let prefix: AddressPrefix = self.network().into();
        let xpub_prefix = kaspa_bip32::Prefix::XPUB;
        let payment_secret = account_args.payment_password.clone();
        let mnemonic = Mnemonic::create_random()?;
        let account_index = 0;
        let prv_key_data = PrvKeyData::try_from((mnemonic.clone(), payment_secret.clone()))?;
        let xpub_key = prv_key_data.create_xpub(payment_secret, account_args.account_kind, account_index).await?;
        let pub_key_data = PubKeyData::new(vec![xpub_key.to_string(Some(xpub_prefix))], None, None);
        log_info!("running create_wallet B");

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
        log_info!("running create_wallet - store 1");
        prv_key_data_store.store(&ctx, prv_key_data).await?;
        let account_store = self.inner.store.as_account_store()?;
        log_info!("running create_wallet - store 2");
        account_store.store(&[&stored_account]).await?;
        self.inner.store.commit(&ctx).await?;

        log_info!("running create_wallet - C");
        let account = Account::try_new_arc_from_storage(self, &stored_account, prefix).await?;
        self.select(Some(account.clone())).await?;

        log_info!("running create_wallet - D");
        // - TODO autoload ???
        account.start().await?;

        Ok((mnemonic, descriptor))
    }

    pub async fn dump_unencrypted(&self) -> Result<()> {
        Ok(())
    }

    pub async fn select(&self, account: Option<Arc<Account>>) -> Result<()> {
        *self.inner.selected_account.lock().unwrap() = account.clone();
        if let Some(account) = account {
            log_info!("selecting account: {}", account.name());
            account.start().await?;
        } else {
            log_info!("selecting account");
        }
        Ok(())
    }

    pub fn account(&self) -> Result<Arc<Account>> {
        self.inner.selected_account.lock().unwrap().clone().ok_or_else(|| Error::AccountSelection)
    }

    pub async fn import_gen0_keydata(self: &Arc<Wallet>, import_secret: Secret, wallet_secret: Secret) -> Result<Arc<Account>> {
        let keydata = load_v0_keydata(&import_secret).await?;

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(Some(wallet_secret)));

        let prv_key_data = PrvKeyData::new_from_mnemonic(&keydata.mnemonic);
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

        let prefix = AddressPrefix::Mainnet;

        let account = Account::try_new_arc_from_storage(self, &stored_account, prefix).await?;

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

    /// handle connection event
    pub async fn handle_connect(&self) -> Result<()> {
        self.inner.is_connected.store(true, Ordering::SeqCst);
        // - TODO - register for daa change
        self.register_notification_listener().await?;
        Ok(())
    }

    /// handle disconnection event
    pub async fn handle_disconnect(&self) -> Result<()> {
        self.inner.is_connected.store(false, Ordering::SeqCst);
        self.unregister_notification_listener().await?;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.inner.is_connected.load(Ordering::SeqCst)
    }

    async fn register_notification_listener(&self) -> Result<()> {
        let listener_id = self.rpc.register_new_listener(ChannelConnection::new(self.inner.notification_channel.sender.clone()));
        *self.inner.listener_id.lock().unwrap() = Some(listener_id);

        self.rpc.start_notify(listener_id, Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;

        Ok(())
    }

    async fn unregister_notification_listener(&self) -> Result<()> {
        let listener_id = self.inner.listener_id.lock().unwrap().take();
        if let Some(id) = listener_id {
            // we do not need this as we are unregister the entire listener here...
            // self.rpc.stop_notify(id, Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
            self.rpc.unregister_listener(id).await?;
        }
        Ok(())
    }

    fn address_to_account(&self, address: &Address) -> Option<Arc<Account>> {
        self.inner.address_to_account_map.lock().unwrap().get(address).cloned()
    }

    async fn handle_notification(&self, notification: Notification) -> Result<()> {
        //log_info!("handling notification: {:?}", notification);

        match &notification {
            Notification::VirtualDaaScoreChanged(data) => {
                self.handle_daa_score_change(data.virtual_daa_score).await?;
            }

            Notification::UtxosChanged(utxos) => {
                for entry in utxos.added.iter() {
                    if let Some(address) = entry.address.as_ref() {
                        if let Some(account) = self.address_to_account(address) {
                            account.handle_utxo_added(entry.clone().into()).await?;
                        } else {
                            log_error!("receiving UTXO Changed notification for an unknown address: {}", address);
                        }
                    } else {
                        log_error!("receiving UTXO Changed 'added' notification without an address is not supported");
                    }
                }

                for entry in utxos.removed.iter() {
                    // self.utxos.remove(UtxoEntryReference::from(entry.clone()).id());
                    if let Some(address) = entry.address.as_ref() {
                        if let Some(account) = self.address_to_account(address) {
                            let removed = account.handle_utxo_removed(UtxoEntryReference::from(entry.clone()).id()).await?;
                            log_info!("utxo removed: {removed}, {}", entry.outpoint.transaction_id);
                        } else {
                            log_error!("receiving UTXO Changed notification for an unknown address: {}", address);
                        }
                    } else {
                        log_error!("receiving UTXO Changed 'remove' notification without an address is not supported");
                    }
                }
            }

            _ => {
                log_warning!("unknown notification: {:?}", notification);
            }
        }

        Ok(())
    }

    async fn handle_daa_score_change(&self, virtual_daa_score: u64) -> Result<()> {
        self.virtual_daa_score.store(virtual_daa_score, Ordering::SeqCst);

        self.multiplexer.broadcast(Events::DAAScoreChange(virtual_daa_score)).await.map_err(|err| format!("{err}"))?;
        Ok(())
    }

    pub async fn start_task(&self) -> Result<()> {
        let self_ = self.clone();
        let ctl_receiver = self.rpc_client().ctl_channel_receiver();

        let task_ctl_receiver = self.inner.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.inner.task_ctl.response.sender.clone();
        let multiplexer = self.multiplexer.clone();
        let notification_receiver = self.inner.notification_channel.receiver.clone();

        spawn(async move {
            loop {
                select! {
                    _ = task_ctl_receiver.recv().fuse() => {
                        break;
                    },
                    notification = notification_receiver.recv().fuse() => {
                        if let Ok(notification) = notification {
                            self_.handle_notification(notification).await.unwrap_or_else(|err| {
                                log_error!("error while handling notification: {err}");
                            });
                        }
                    },
                    msg = ctl_receiver.recv().fuse() => {
                        if let Ok(msg) = msg {
                            match msg {
                                Ctl::Open => {
                                    //println!("Ctl::Open:::::::");
                                    self_.handle_connect().await.unwrap_or_else(|err| log_error!("{err}"));
                                    multiplexer.broadcast(Events::Connect).await.unwrap_or_else(|err| log_error!("{err}"));
                                    // self_.connect().await?;
                                },
                                Ctl::Close => {
                                    self_.handle_disconnect().await.unwrap_or_else(|err| log_error!("{err}"));
                                    multiplexer.broadcast(Events::Disconnect).await.unwrap_or_else(|err| log_error!("{err}"));
                                    // self_.disconnect().await?;
                                }
                            }
                        }
                    }
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

    pub async fn is_open(&self) -> Result<bool> {
        self.inner.store.is_open().await
    }

    pub async fn exists(&self, name: Option<&str>) -> Result<bool> {
        self.inner.store.exists(name).await
    }

    pub async fn keys(&self) -> Result<impl Stream<Item = Result<Arc<PrvKeyDataInfo>>>> {
        self.inner.store.as_prv_key_data_store().unwrap().iter().await
    }

    pub async fn accounts(self: &Arc<Self>, filter: Option<PrvKeyDataId>) -> Result<impl Stream<Item = Result<Arc<Account>>>> {
        let iter = self.inner.store.as_account_store().unwrap().iter(filter).await.unwrap();
        let wallet = self.clone();

        let stream = iter.then(move |stored| {
            let wallet = wallet.clone();
            async move {
                // TODO - set prefix in the Wallet
                let prefix: AddressPrefix = wallet.network().into();

                let stored = stored.unwrap();
                if let Some(account) = wallet.active_accounts().get(&stored.id) {
                    Ok(account)
                } else {
                    Account::try_new_arc_from_storage(&wallet, &stored, prefix).await
                }
            }
        });

        Ok(Box::pin(stream))
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod test {
    use std::{str::FromStr, thread::sleep, time};

    use super::*;
    use crate::{
        signer::sign_mutable_transaction,
        tx::MutableTransaction,
        utxo::{UtxoOrdering, UtxoSet},
    };

    // TODO - re-export subnets
    use crate::tx::Transaction;
    use crate::tx::TransactionInput;
    use crate::tx::TransactionOutput;
    use kaspa_consensus_core::subnets::SubnetworkId;
    //use kaspa_consensus_core::tx::ScriptPublicKey;
    //use kaspa_consensus_core::tx::MutableTransaction;
    use kaspa_addresses::{Address, Prefix, Version};
    use kaspa_bip32::{ChildNumber, ExtendedPrivateKey, SecretKey};
    use kaspa_txscript::pay_to_address_script;

    // async fn get_utxos_set_by_addresses(rpc: Arc<KaspaRpcClient>, addresses: Vec<Address>) -> Result<UtxoSet> {
    async fn get_utxos_set_by_addresses(rpc: Arc<DynRpcApi>, addresses: Vec<Address>) -> Result<UtxoSet> {
        let utxos = rpc.get_utxos_by_addresses(addresses).await?;
        let utxo_set = UtxoSet::new();
        for utxo in utxos {
            utxo_set.insert(utxo.into());
        }
        Ok(utxo_set)
    }

    #[allow(dead_code)]
    // #[tokio::test]
    async fn wallet_test() -> Result<()> {
        println!("Creating wallet...");
        let wallet = Arc::new(Wallet::try_new().await?);
        // let stored_accounts = vec![StoredWalletAccount{
        //     private_key_index: 0,
        //     account_kind: crate::storage::AccountKind::Bip32,
        //     name: "Default Account".to_string(),
        //     title: "Default Account".to_string(),
        // }];

        // wallet.load_accounts(stored_accounts);

        let rpc = wallet.rpc();
        let rpc_client = wallet.rpc_client();

        let _connect_result = rpc_client.connect(true).await;
        //println!("connect_result: {_connect_result:?}");

        let _result = wallet.start().await;
        //println!("wallet.task(): {_result:?}");
        let result = wallet.get_info().await;
        println!("wallet.get_info(): {result:#?}");

        let address = Address::try_from("kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd")?;

        let utxo_set = self::get_utxos_set_by_addresses(rpc.clone(), vec![address.clone()]).await?;

        let utxo_set_balance = utxo_set.calculate_balance().await?;
        println!("get_utxos_by_addresses: {utxo_set_balance:?}");

        let utxo_selection = utxo_set.select(100000, UtxoOrdering::AscendingAmount, true).await?;

        //let payload = vec![];
        let to_address = Address::try_from("kaspatest:qpakxqlesqywgkq7rg4wyhjd93kmw7trkl3gpa3vd5flyt59a43yyn8vu0w8c")?;
        //let outputs = Outputs { outputs: vec![Output::new(to_address, 100000, None)] };
        //let vtx = VirtualTransaction::new(utxo_selection, &outputs, payload);

        //vtx.sign();
        let utxo = (*utxo_selection.selected_entries[0].utxo).clone();
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

        let utxo_set = self::get_utxos_set_by_addresses(rpc.clone(), vec![to_address.clone()]).await?;
        let to_balance = utxo_set.calculate_balance().await?;
        println!("to address balance before tx submit: {to_balance:?}");

        let result = rpc.submit_transaction(mtx.try_into()?, false).await?;

        println!("tx submit result, {:?}", result);
        println!("sleep for 5s...");
        sleep(time::Duration::from_millis(5000));
        let utxo_set = self::get_utxos_set_by_addresses(rpc.clone(), vec![to_address.clone()]).await?;
        let to_balance = utxo_set.calculate_balance().await?;
        println!("to address balance after tx submit: {to_balance:?}");

        Ok(())
    }
}
