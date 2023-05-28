use crate::result::Result;
use crate::runtime::{Account, AccountId, AccountList, AccountMap};
use crate::secret::Secret;
use crate::storage::{self, AccountKind, PrvKeyData, PrvKeyDataId};
use crate::utxo::UtxoEntryReference;
#[allow(unused_imports)]
use crate::{accounts::gen0, accounts::gen0::import::*, accounts::gen1, accounts::gen1::import::*};
use crate::{imports::*, DynRpcApi};
use futures::future::join_all;
use futures::{select, FutureExt};
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
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use storage::{local::Store, Payload, PubKeyData};
use workflow_core::channel::{Channel, DuplexChannel, Multiplexer, Receiver};
use workflow_core::task::spawn;
use workflow_log::log_error;
use workflow_rpc::client::Ctl;

#[derive(Clone)]
pub struct AccountCreateArgs {
    pub title: String,
    pub account_kind: storage::AccountKind,
    pub wallet_password: Secret,
    pub payment_password: Option<Secret>,
    pub override_wallet: bool,
}

impl AccountCreateArgs {
    pub fn new(title: String, account_kind: storage::AccountKind, wallet_password: Secret, payment_password: Option<Secret>) -> Self {
        Self { title, account_kind, wallet_password, payment_password, override_wallet: false }
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

//#[derive(Clone)]
pub struct Inner {
    // accounts: Vec<Arc<dyn WalletAccountTrait>>,
    // accounts: Mutex<Vec<Arc<Account>>>,
    // accounts: Mutex<HashMap<PrvKeyDataId,Vec<Arc<Account>>>>,
    account_map: AccountMap,
    //account_list: Mutex<AccountList>,
    listener_id: Mutex<Option<ListenerId>>,
    // notification_receiver: Receiver<Notification>,
    #[allow(dead_code)] //TODO: remove me
    ctl_receiver: Receiver<Ctl>,
    pub task_ctl: DuplexChannel,
    pub selected_account: Mutex<Option<Arc<Account>>>,
    pub is_connected: AtomicBool,

    // #[wasm_bindgen(skip)]
    pub notification_channel: Channel<Notification>,
    // ---
    pub address_to_account_map: Arc<Mutex<HashMap<Address, Arc<Account>>>>,
    // ---
}

/// `Wallet` data structure
#[derive(Clone)]
#[wasm_bindgen]
pub struct Wallet {
    #[wasm_bindgen(skip)]
    pub rpc: Arc<DynRpcApi>,
    #[wasm_bindgen(skip)]
    pub multiplexer: Multiplexer<Events>,
    // #[wasm_bindgen(skip)]
    // pub rpc_client: Arc<KaspaRpcClient>,
    inner: Arc<Inner>,
    #[wasm_bindgen(skip)]
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

        let wallet = Wallet {
            // rpc_client : rpc.clone(),
            rpc,
            multiplexer,
            virtual_daa_score: Arc::new(AtomicU64::new(0)),
            inner: Arc::new(Inner {
                account_map: AccountMap::default(), //Mutex::new(HashMap::new()),
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

    pub fn address_to_account_map(&self) -> &Arc<Mutex<HashMap<Address, Arc<Account>>>> {
        &self.inner.address_to_account_map
    }

    pub async fn reset(&self) -> Result<()> {
        let accounts = self.account_list()?;

        for account in accounts {
            account.stop().await?;
        }
        self.inner.account_map.clear();
        self.inner.address_to_account_map.lock().unwrap().clear();

        Ok(())
    }

    // pub fn load_accounts(&self, stored_accounts: Vec<storage::Account>) => Result<()> {
    pub async fn load(self: &Arc<Wallet>, _secret: Secret, prefix: AddressPrefix) -> Result<()> {
        // - TODO - RESET?
        self.reset().await?;

        use crate::iterator::*;
        use storage::interface::*;
        use storage::local::interface::*;

        let ctx = Arc::new(AccessContext::default());
        let ctx: Arc<dyn AccessContextT> = ctx;
        let local_store = Arc::new(LocalStore::try_new(None, storage::local::DEFAULT_WALLET_FILE)?);
        // let iface : Arc<dyn Interface> = local_store;
        let store_accounts = local_store.account().await;
        let mut iter = store_accounts.clone().iter(IteratorOptions::default()).await;
        while let Some(ids) = iter.next().await {
            let accounts = store_accounts.load(&ctx, &ids).await?;

            let accounts = accounts.iter().map(|stored| Account::try_new_from_storage(self, stored, prefix)).collect::<Vec<_>>();
            let accounts = join_all(accounts).await.into_iter().collect::<Result<Vec<_>>>()?;
            let accounts = accounts.into_iter().map(Arc::new).collect::<Vec<_>>();

            self.inner.account_map.extend(accounts)?;
        }

        // let store = storage::local::Store::default();
        // let wallet = storage::Wallet::try_load(&store).await?;
        // let payload = wallet.payload.decrypt::<storage::Payload>(secret)?;

        // let accounts =
        //     payload.as_ref().accounts.iter().map(|stored| Account::try_new_from_storage(self, stored, prefix)).collect::<Vec<_>>();
        // let accounts = join_all(accounts).await.into_iter().collect::<Result<Vec<_>>>()?;
        // let accounts = accounts.into_iter().map(Arc::new).collect::<Vec<_>>();

        // self.inner.account_map.extend(accounts)?;

        Ok(())
    }

    pub async fn get_account_keydata(&self, id: PrvKeyDataId, secret: Secret) -> Result<Option<PrvKeyData>> {
        // let id = if let Some(id) = id { id } else { return Ok(None) };

        let store = storage::local::Store::default();
        let wallet = storage::Wallet::try_load(&store).await?;
        let payload = wallet.payload.decrypt::<storage::Payload>(secret)?;
        let key = payload.as_ref().prv_key_data.iter().find(|k| k.id == id);

        Ok(key.cloned())
    }

    pub async fn is_account_key_encrypted(&self, account: &Account, secret: Secret) -> Result<bool> {
        let _id = account.prv_key_data_id;

        // let id = if let Some(id) = account.prv_key_data_id { id } else { return Ok(false) };
        let id = account.prv_key_data_id;
        let store = storage::local::Store::default();
        let wallet = storage::Wallet::try_load(&store).await?;
        let payload = wallet.payload.decrypt::<storage::Payload>(secret)?;
        let key = payload.as_ref().prv_key_data.iter().find(|k| k.id == id);

        if let Some(key) = key {
            Ok(key.payload.is_encrypted())
        } else {
            Ok(false)
        }
    }

    pub fn rpc_client(&self) -> Arc<KaspaRpcClient> {
        self.rpc.clone().downcast_arc::<KaspaRpcClient>().expect("unable to downcast DynRpcApi to KaspaRpcClient")
    }

    pub fn rpc(&self) -> &Arc<DynRpcApi> {
        &self.rpc //.clone()
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
    //     let wallet = storage::Wallet::try_load(&store).await?;
    //     let mut payload = wallet.payload.decrypt::<Payload>(wallet_secret).unwrap();
    //     payload.as_mut().add_prv_key_data(mnemonic.clone(), payment_secret)?;

    //     Ok(mnemonic)
    // }

    pub async fn create_private_key(self: &Arc<Wallet>, wallet_secret: Secret, payment_secret: Option<Secret>) -> Result<Mnemonic> {
        let store = Store::default();

        let mnemonic = Mnemonic::create_random()?;
        let wallet = storage::Wallet::try_load(&store).await?;
        let mut payload = wallet.payload.decrypt::<Payload>(wallet_secret).unwrap();
        payload.as_mut().add_prv_key_data(mnemonic.clone(), payment_secret)?;

        Ok(mnemonic)
    }

    pub async fn create_bip32_account(
        self: &Arc<Wallet>,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        prv_key_data_id: PrvKeyDataId,
        // account_index: u64,
        args: &AccountCreateArgs,
    ) -> Result<Arc<Account>> {
        let prefix: AddressPrefix = self.network().into();
        let store = Store::default();

        let wallet = storage::Wallet::try_load(&store).await?;
        let mut payload = wallet.payload.decrypt::<Payload>(wallet_secret.clone()).unwrap();
        // let payload = payload.as_mut();

        let prv_key_data = payload.find_prv_key_data(&prv_key_data_id).ok_or(Error::PrivateKeyNotFound(prv_key_data_id.to_hex()))?;

        // determine account index based on the total count of created accounts
        // TODO - how should we determine account index?
        let account_index = payload
            .accounts
            .iter()
            .filter(|account| matches!(account.account_kind, AccountKind::Bip32) && account.prv_key_data_id == prv_key_data_id)
            .collect::<Vec<_>>()
            .len() as u64;

        let xpub_key = prv_key_data.create_xpub(payment_secret, args.account_kind, account_index).await?;
        let xpub_prefix = kaspa_bip32::Prefix::XPUB;
        let pub_key_data = PubKeyData::new(vec![xpub_key.to_string(Some(xpub_prefix))], None, None);

        let stored_account = storage::Account::new(
            args.title.replace(' ', "-").to_lowercase(),
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

        payload.as_mut().accounts.push(stored_account.clone());
        storage::Wallet::try_store(&store, wallet_secret, payload.0).await?;

        // -

        let account = Arc::new(Account::try_new_from_storage(self, &stored_account, prefix).await?);
        self.inner.account_map.insert(account.clone())?;

        account.start().await?;

        Ok(account)
    }

    pub async fn create_wallet(self: &Arc<Wallet>, args: &AccountCreateArgs) -> Result<(PathBuf, Mnemonic)> {
        let store = Store::new(storage::local::DEFAULT_WALLET_FOLDER, storage::local::DEFAULT_WALLET_NAME)?;
        // let store = Store::new(storage::DEFAULT_WALLET_FILE)?;
        if !args.override_wallet && store.exists().await? {
            return Err(Error::WalletAlreadyExists);
        }

        let prefix: AddressPrefix = self.network().into();

        let xpub_prefix = kaspa_bip32::Prefix::XPUB;

        let payment_secret = args.payment_password.clone();

        let mnemonic = Mnemonic::create_random()?;
        // let mnemonic_phrase = Secret::new(mnemonic.phrase().as_bytes().to_vec());
        let mut payload = Payload::default();
        let account_index = 0;
        let prv_key_data = payload.add_prv_key_data(mnemonic.clone(), None)?;
        let xpub_key = prv_key_data.create_xpub(payment_secret, args.account_kind, account_index).await?;
        let pub_key_data = PubKeyData::new(vec![xpub_key.to_string(Some(xpub_prefix))], None, None);

        let stored_account = storage::Account::new(
            args.title.replace(' ', "-").to_lowercase(),
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

        payload.accounts.push(stored_account.clone());
        storage::Wallet::try_store(&store, args.wallet_password.clone(), payload).await?;

        // -
        self.reset().await?;

        let account = Arc::new(Account::try_new_from_storage(self, &stored_account, prefix).await?);
        self.inner.account_map.insert(account.clone())?;

        self.select(Some(account.clone())).await?;

        account.start().await?;

        Ok((store.filename().clone(), mnemonic))
    }

    pub async fn dump_unencrypted(&self) -> Result<()> {
        Ok(())
    }

    pub async fn select(&self, account: Option<Arc<Account>>) -> Result<()> {
        // log_info!(target: "term","selecting account");

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

    pub fn account_map(&self) -> &AccountMap {
        &self.inner.account_map
    }

    pub fn account_list(&self) -> Result<AccountList> {
        self.inner.account_map.cloned_flat_list()
    }
    // pub fn accounts(&self) -> Vec<Arc<Account>> {
    //     self.accounts.flat_list()
    //     self.inner.accounts.lock().unwrap().values().collect_vec().into_iter().cloned().flatten().collect_vec()
    // }

    // pub async fn import_gen0_keydata(self: &Arc<Wallet>, import_secret: Secret, wallet_secret: Secret) -> Result<()> {
    pub async fn import_gen0_keydata(self: &Arc<Wallet>, import_secret: Secret, wallet_secret: Secret) -> Result<()> {
        let keydata = load_v0_keydata(&import_secret).await?;

        let store = storage::local::Store::new(storage::local::DEFAULT_WALLET_FOLDER, storage::local::DEFAULT_WALLET_NAME)?;
        let wallet = storage::Wallet::try_load(&store).await?;
        let mut payload = wallet.payload.decrypt::<Payload>(wallet_secret).unwrap();
        let payload = payload.as_mut();

        let prv_key_data = PrvKeyData::new_from_mnemonic(&keydata.mnemonic);

        // TODO: integrate address generation
        // let derivation_path = gen1::WalletAccount::build_derivate_path(false, 0, Some(kaspa_bip32::AddressType::Receive))?;
        // let xkey = ExtendedPrivateKey::<SecretKey>::from_str(xprv)?.derive_path(derivation_path)?;

        let stored_account = storage::Account::new(
            "imported-wallet".to_string(),       // name
            "Imported Wallet".to_string(),       // title
            storage::AccountKind::V0,            // kind
            0,                                   // account index
            false,                               // public visibility
            PubKeyData::new(vec![], None, None), // TODO - pub keydata
            prv_key_data.id,                     // privkey id
            false,                               // ecdsa
            1,                                   // min signatures
            0,                                   // cosigner_index
        );

        let prefix = AddressPrefix::Mainnet;

        let runtime_account = Account::try_new_from_storage(self, &stored_account, prefix).await?;

        payload.prv_key_data.push(prv_key_data);
        // TODO - prevent multiple addition of the same private key
        payload.accounts.push(stored_account);

        self.inner.account_map.insert(Arc::new(runtime_account))?;

        Ok(())
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

        // let ctl_receiver = self.rpc.ctl_channel_receiver();
        // let task_ctl = self.inner.lock().unwrap().task_ctl.clone();
        // let multiplexer = self.multiplexer.clone();
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
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod test {
    use std::{str::FromStr, thread::sleep, time};

    use super::*;
    use crate::{
        signer::sign_mutable_transaction,
        // Signer,
        tx::MutableTransaction,
        utxo::{
            //SelectionContext,
            UtxoOrdering,
            UtxoSet,
        },
    };
    //use kaspa_bip32::{ExtendedPrivateKey, SecretKey};

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

        let utxo_selection = utxo_set.select(100000, UtxoOrdering::AscendingAmount).await?;

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
