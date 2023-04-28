#[allow(unused_imports)]
use crate::accounts::{gen0::*, gen1::*, PubkeyDerivationManagerTrait, WalletDerivationManagerTrait};
use crate::result::Result;
use crate::secret::Secret;
use crate::signer::sign_mutable_transaction;
use crate::storage::{self, PrvKeyData, PrvKeyDataId, PubKeyData};
use crate::tx::{create_transaction, PaymentOutput, PaymentOutputs};
use crate::utxo::{UtxoEntryReference, UtxoOrdering, UtxoSet};
use crate::wallet::Events;
use crate::AddressDerivationManager;
use crate::DynRpcApi;
use crate::{imports::*, Wallet};
use async_trait::async_trait;
use futures::future::join_all;
use kaspa_bip32::{ChildNumber, ExtendedPrivateKey, Language, Mnemonic, PrivateKey, SecretKey};
use kaspa_hashes::Hash;
use kaspa_notify::listener::ListenerId;
use kaspa_notify::scope::{Scope, UtxosChangedScope};
//use kaspa_notify::scope::{Scope, UtxosChangedScope};
use kaspa_rpc_core::api::notifications::Notification;
use kaspa_rpc_core::notify::connection::ChannelConnection;
use std::collections::HashMap;
//use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use workflow_core::channel::{oneshot, Channel, DuplexChannel, Multiplexer};
// use workflow_core::channel::{Channel, DuplexChannel, Multiplexer, Receiver};
use crate::address::{build_derivate_paths, AddressManager};
use kaspa_addresses::Prefix as AddressPrefix;
use workflow_core::task::spawn;
// use futures::future::join_all;
use futures::{select, FutureExt};

#[derive(Default, Clone)]
pub struct Estimate {
    pub total_sompi: u64,
    pub fees_sompi: u64,
    pub utxos: usize,
    pub transactions: usize,
}

// pub enum ScanExtent {
//     /// Scan until an empty range is found
//     EmptyRange(u32),
//     /// Scan until a specific depth (a particular derivation index)
//     Depth(u32),
// }

const DEFAULT_WINDOW_SIZE: u32 = 128;

#[derive(Default, Clone, Copy)]
pub enum ScanExtent {
    /// Scan until an empty range is found
    #[default]
    EmptyWindow,
    /// Scan until a specific depth (a particular derivation index)
    Depth(u32),
}

pub struct Scan {
    pub address_manager: Arc<AddressManager>,
    pub window_size: u32,
    pub extent: ScanExtent,
}

impl Scan {
    pub fn new(address_manager: Arc<AddressManager>) -> Scan {
        Scan { address_manager, window_size: DEFAULT_WINDOW_SIZE, extent: ScanExtent::EmptyWindow }
    }
    pub fn new_with_args(address_manager: Arc<AddressManager>, window_size: u32, extent: ScanExtent) -> Scan {
        Scan { address_manager, window_size, extent }
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "kebab-case")]
#[wasm_bindgen]
pub enum AccountKind {
    V0,
    #[default]
    Bip32,
    MultiSig,
}

#[async_trait]
pub trait AccountT {
    // async fn connect(&self);
    // async fn disconnect(&self);
    // async fn reset();
}

pub struct Inner {
    pub listener_id: Option<ListenerId>,
    pub stored: storage::Account,
    pub address_to_index_map: HashMap<Address, u32>,
}

/// Wallet `Account` data structure. An account is typically a single
/// HD-key derivation (derived from a wallet or from an an external secret)
#[wasm_bindgen(inspectable)]
pub struct Account {
    inner: Arc<Mutex<Inner>>,
    // interfaces: Interfaces,
    #[wasm_bindgen(skip)]
    pub rpc: Arc<DynRpcApi>,
    #[wasm_bindgen(skip)]
    pub multiplexer: Multiplexer<Events>,

    utxos: UtxoSet,
    balance: Arc<AtomicU64>,
    is_connected: AtomicBool,
    #[wasm_bindgen(js_name = "accountKind")]
    pub account_kind: AccountKind,
    pub account_index: u64,
    #[wasm_bindgen(skip)]
    pub prv_key_data_id: Option<PrvKeyDataId>,
    pub ecdsa: bool,
    // ~
    // pub derivation_path : DerivationPath,
    #[wasm_bindgen(skip)]
    pub derivation: Arc<AddressDerivationManager>,

    #[wasm_bindgen(skip)]
    pub task_ctl: DuplexChannel,
    #[wasm_bindgen(skip)]
    pub notification_channel: Channel<Notification>,
}

impl Account {
    pub async fn try_new_with_args(
        // rpc_api: Arc<DynRpcApi>,
        // multiplexer: Multiplexer<Events>,
        // interfaces: Interfaces,
        wallet: &Wallet,
        name: &str,
        title: &str,
        account_kind: AccountKind,
        account_index: u64,
        prv_key_data_id: Option<PrvKeyDataId>,
        pub_key_data: PubKeyData,
        ecdsa: bool,
        address_prefix: AddressPrefix,
    ) -> Result<Self> {
        // rpc_api.register_new_listener();
        let minimum_signatures = pub_key_data.minimum_signatures.unwrap_or(1) as usize;
        let derivation =
            AddressDerivationManager::new(address_prefix, account_kind, &pub_key_data, ecdsa, minimum_signatures, None, None).await?;

        let stored = storage::Account::new(
            name.to_string(),
            title.to_string(),
            account_kind,
            account_index,
            false,
            pub_key_data.clone(),
            prv_key_data_id,
            ecdsa,
            pub_key_data.minimum_signatures.unwrap_or(1),
            pub_key_data.cosigner_index.unwrap_or(0),
        );

        let inner = Inner { listener_id: None, stored, address_to_index_map: HashMap::default() };

        Ok(Account {
            utxos: UtxoSet::default(),
            balance: Arc::new(AtomicU64::new(0)),
            // _generator: Arc::new(config.clone()),
            rpc: wallet.rpc().clone(),
            multiplexer: wallet.multiplexer().clone(),
            // interfaces,
            is_connected: AtomicBool::new(false),
            // -
            inner: Arc::new(Mutex::new(inner)),
            // -
            account_kind,
            account_index,
            prv_key_data_id,
            ecdsa: false,
            // -
            derivation,
            task_ctl: DuplexChannel::oneshot(),
            notification_channel: Channel::<Notification>::unbounded(),
        })
    }

    pub async fn try_new_from_storage(wallet: &Wallet, stored: &storage::Account, address_prefix: AddressPrefix) -> Result<Self> {
        let minimum_signatures = stored.pub_key_data.minimum_signatures.unwrap_or(1) as usize;
        let derivation = AddressDerivationManager::new(
            address_prefix,
            stored.account_kind,
            &stored.pub_key_data,
            stored.ecdsa,
            minimum_signatures,
            None,
            None,
        )
        .await?;

        let inner = Inner { listener_id: None, stored: stored.clone(), address_to_index_map: HashMap::default() };

        Ok(Account {
            utxos: UtxoSet::default(),
            balance: Arc::new(AtomicU64::new(0)),
            rpc: wallet.rpc().clone(), //: rpc.clone(),
            multiplexer: wallet.multiplexer().clone(),
            is_connected: AtomicBool::new(false),
            inner: Arc::new(Mutex::new(inner)),
            account_kind: stored.account_kind,
            account_index: stored.account_index,
            prv_key_data_id: stored.prv_key_data_id,
            ecdsa: stored.ecdsa,
            // -
            derivation,
            task_ctl: DuplexChannel::oneshot(),
            notification_channel: Channel::<Notification>::unbounded(),
        })
    }

    // pub async fn subscribe(&self) {
    //     // TODO query account interface
    //     let addresses = vec![];
    //     let utxos_changed_scope = UtxosChangedScope { addresses };
    //     let id = self.inner.lock().unwrap().listener_id;
    //     let _ = self.interfaces.rpc.start_notify(id, Scope::UtxosChanged(utxos_changed_scope)).await;
    // }

    pub async fn update_balance(&self) -> Result<u64> {
        let balance = self.utxos.calculate_balance().await?;
        self.balance.store(self.utxos.calculate_balance().await?, std::sync::atomic::Ordering::SeqCst);
        Ok(balance)
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn name(&self) -> String {
        self.inner.lock().unwrap().stored.name.clone()
    }

    pub fn get_ls_string(&self) -> String {
        let name = self.name();
        let balance = self.balance.load(std::sync::atomic::Ordering::SeqCst);
        format!("{balance} - {name}")
    }

    pub fn inner(&self) -> MutexGuard<Inner> {
        self.inner.lock().unwrap()
    }

    pub async fn scan_address_manager(&self, scan: Scan) -> Result<()> {
        let mut cursor = 0;

        'scan: loop {
            let first = cursor;
            let last = cursor + scan.window_size;
            cursor = last;

            log_info!("first: {}, last: {}\r\n", first, last);

            let addresses = scan.address_manager.get_range(first..last).await?;

            self.subscribe_utxos_changed(&addresses).await?;
            let resp = self.rpc.get_utxos_by_addresses(addresses).await?;
            let refs: Vec<UtxoEntryReference> = resp.into_iter().map(UtxoEntryReference::from).collect();
            //println!("{}", format!("addresses:{:#?}", address_str).replace('\n', "\r\n"));
            //println!("{}", format!("resp:{:#?}", resp.get(0).and_then(|a|a.address.clone())).replace('\n', "\r\n"));

            self.utxos.extend(&refs);
            let balance = refs.iter().map(|r| r.as_ref().amount()).sum::<u64>();

            if balance != 0 {
                println!("scan_address_managet() balance increment: {balance}");
                self.balance.fetch_add(balance, Ordering::SeqCst);

                // - TODO - post balance update to the multiplexer?
            } else {
                match &scan.extent {
                    ScanExtent::EmptyWindow => {
                        break 'scan;
                    }
                    ScanExtent::Depth(depth) => {
                        if &cursor > depth {
                            break 'scan;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn scan_utxos(&self, window_size: Option<u32>, extent: Option<u32>) -> Result<()> {
        self.utxos.clear();
        self.balance.store(0, Ordering::SeqCst);

        let window_size = window_size.unwrap_or(DEFAULT_WINDOW_SIZE);
        let extent = match extent {
            Some(depth) => ScanExtent::Depth(depth),
            None => ScanExtent::EmptyWindow,
        };

        let scans = vec![
            self.scan_address_manager(Scan::new_with_args(self.derivation.receive_address_manager(), window_size, extent)),
            self.scan_address_manager(Scan::new_with_args(self.derivation.change_address_manager(), window_size, extent)),
        ];

        join_all(scans).await.into_iter().collect::<Result<Vec<_>>>()?;

        // let mut scan = Scan::new(receive,change,window_size,ScanExtent::EmptyRange(window_size));
        // let next = scan.next().await;

        // let receive_index = receive.index()?;
        // let change_index = change.index()?;

        // let _last_receive = receive_index + window_size;
        // let _last_change = change_index + window_size;

        // let mut futures = vec![];

        // let mut balance = 0u64;
        // let mut cursor = 0;
        // while cursor < scan_depth {
        //     let first = cursor;
        //     let last = cursor + window_size;
        //     cursor = last;

        //     log_info!("first: {}, last: {}\r\n", first, last);

        //     let addresses = receive.get_range(first..last).await?;
        //     //let address_str = addresses.iter().map(String::from).collect::<Vec<_>>();
        //     futures.push(self.scan_block(addresses.clone()));
        //     self.subscribe_utxos_changed(&addresses).await?;
        //     let resp = self.rpc.get_utxos_by_addresses(addresses).await?;

        //     //println!("{}", format!("addresses:{:#?}", address_str).replace('\n', "\r\n"));
        //     //println!("{}", format!("resp:{:#?}", resp.get(0).and_then(|a|a.address.clone())).replace('\n', "\r\n"));

        //     let refs: Vec<UtxoEntryReference> = resp.into_iter().map(UtxoEntryReference::from).collect();

        //     balance += refs.iter().map(|r| r.as_ref().amount()).sum::<u64>();

        //     //println!("balance: {balance}");
        //     self.utxos.extend(&refs);

        //     let change_addresses = change.get_range(first..last).await?;
        //     //let change_address_str = change_addresses.iter().map(String::from).collect::<Vec<_>>();
        //     self.subscribe_utxos_changed(&change_addresses).await?;
        //     let resp = self.rpc.get_utxos_by_addresses(change_addresses).await?;

        //     //println!("{}", format!("addresses:{:#?}", address_str).replace('\n', "\r\n"));
        //     //println!("{}", format!("resp:{:#?}", resp.get(0).and_then(|a|a.address.clone())).replace('\n', "\r\n"));

        //     let refs: Vec<UtxoEntryReference> = resp.into_iter().map(UtxoEntryReference::from).collect();

        //     balance += refs.iter().map(|r| r.as_ref().amount()).sum::<u64>();

        //     //println!("balance: {balance}");
        //     self.utxos.extend(&refs);
        // }

        // - TODO - post balance updates to the wallet

        self.utxos.order(UtxoOrdering::AscendingAmount)?;

        self.update_balance().await?;

        Ok(())
    }

    // - TODO
    pub async fn scan_block(&self, addresses: Vec<Address>) -> Result<Vec<UtxoEntryReference>> {
        self.subscribe_utxos_changed(&addresses).await?;
        let resp = self.rpc.get_utxos_by_addresses(addresses).await?;
        let refs: Vec<UtxoEntryReference> = resp.into_iter().map(UtxoEntryReference::from).collect();
        Ok(refs)
    }

    pub async fn estimate(&self, _address: &Address, _amount_sompi: u64, _priority_fee_sompi: u64) -> Result<Estimate> {
        todo!()
        // Ok(())
    }

    pub async fn send(
        &self,
        address: &Address,
        amount_sompi: u64,
        priority_fee_sompi: u64,
        keydata: PrvKeyData,
        payment_secret: Option<Secret>,
    ) -> Result<Hash> {
        let fee_margin = 1000; //TODO update select_utxos to remove this fee_margin
        let transaction_amount = amount_sompi + priority_fee_sompi + fee_margin;
        let utxo_selection = self.utxos.select_utxos(transaction_amount, UtxoOrdering::AscendingAmount).await?;

        let change_address = self.change_address().await?;
        let outputs = PaymentOutputs { outputs: vec![PaymentOutput::new(address.clone(), amount_sompi, None)] };

        let priority_fee = Some(priority_fee_sompi);
        let payload = None;
        let mtx = create_transaction(utxo_selection, outputs, change_address, priority_fee, payload)?;

        // TODO find path indexes by UTXOs/addresses;
        let receive_indexes = vec![0];
        let change_indexes = vec![0];

        let private_keys = self.create_private_keys(keydata, payment_secret, receive_indexes, change_indexes)?;
        let private_keys = &private_keys.iter().map(|k| k.to_bytes()).collect::<Vec<_>>();
        let mtx = sign_mutable_transaction(mtx, private_keys, true)?;
        let result = self.rpc.submit_transaction(mtx.try_into()?, false).await?;
        Ok(result)
    }

    fn create_private_keys(
        &self,
        keydata: PrvKeyData,
        payment_secret: Option<Secret>,
        receive_indexes: Vec<u32>,
        change_indexes: Vec<u32>,
    ) -> Result<Vec<secp256k1::SecretKey>> {
        let payload = keydata.payload.decrypt(payment_secret)?;

        let mnemonic = Mnemonic::new(&payload.as_ref().mnemonic, Language::English)?;
        let xkey = ExtendedPrivateKey::<SecretKey>::new(mnemonic.to_seed(""))?;

        let cosigner_index = self.inner().stored.pub_key_data.cosigner_index.unwrap_or(0);
        let paths = build_derivate_paths(self.account_kind, self.account_index, cosigner_index)?;
        let receive_xkey = xkey.clone().derive_path(paths.0)?;
        let change_xkey = xkey.derive_path(paths.1)?;

        let mut private_keys = vec![];
        for index in receive_indexes {
            private_keys.push(*receive_xkey.derive_child(ChildNumber::new(index, false)?)?.private_key());
        }
        for index in change_indexes {
            private_keys.push(*change_xkey.derive_child(ChildNumber::new(index, false)?)?.private_key());
        }

        Ok(private_keys)
    }

    pub async fn address(&self) -> Result<Address> {
        self.receive_address_manager()?.current_address().await
    }

    pub async fn change_address(&self) -> Result<Address> {
        self.change_address_manager()?.current_address().await
    }

    #[allow(dead_code)]
    fn receive_address_manager(&self) -> Result<Arc<AddressManager>> {
        Ok(self.derivation.receive_address_manager())
    }

    fn change_address_manager(&self) -> Result<Arc<AddressManager>> {
        Ok(self.derivation.change_address_manager())
    }

    pub async fn new_receive_address(&self) -> Result<String> {
        let address = self.receive_address_manager()?.new_address().await?;
        Ok(address.into())
    }

    pub async fn new_change_address(self: &Arc<Self>) -> Result<String> {
        let address = self.change_address_manager()?.new_address().await?;
        Ok(address.into())
    }

    pub async fn sign(&self) -> Result<()> {
        Ok(())
    }

    pub async fn sweep(&self) -> Result<()> {
        Ok(())
    }

    pub async fn create_unsigned_transaction(&self) -> Result<()> {
        Ok(())
    }

    // -

    /// Start Account service task
    pub async fn start(self: &Arc<Self>) -> Result<()> {
        self.start_task().await?;

        Ok(())
    }

    /// Stop Account service task
    pub async fn stop(&self) -> Result<()> {
        self.stop_task().await?;

        Ok(())
    }

    /// handle connection event
    pub async fn connect(&self) -> Result<()> {
        self.is_connected.store(true, Ordering::SeqCst);
        self.register_notification_listener().await?;
        self.scan_utxos(Some(128), None).await?;
        Ok(())
    }

    /// handle disconnection event
    pub async fn disconnect(&self) -> Result<()> {
        self.is_connected.store(false, Ordering::SeqCst);
        self.unregister_notification_listener().await?;
        Ok(())
    }

    fn listener_id(&self) -> Option<ListenerId> {
        self.inner.lock().unwrap().listener_id
    }

    async fn subscribe_utxos_changed(&self, addresses: &[Address]) -> Result<()> {
        let listener_id = self
            .listener_id()
            .expect("subscribe_utxos_changed() requires `listener_id` (must call `register_notification_listener()` before use)");
        let utxos_changed_scope = UtxosChangedScope { addresses: addresses.to_vec() };
        self.rpc.start_notify(listener_id, Scope::UtxosChanged(utxos_changed_scope)).await?;

        Ok(())
    }

    async fn register_notification_listener(&self) -> Result<()> {
        let listener_id = self.rpc.register_new_listener(ChannelConnection::new(self.notification_channel.sender.clone()));
        self.inner().listener_id = Some(listener_id);

        Ok(())
    }

    async fn unregister_notification_listener(&self) -> Result<()> {
        let listener_id = self.inner.lock().unwrap().listener_id.take();
        if let Some(id) = listener_id {
            self.rpc.unregister_listener(id).await?;
        }
        Ok(())
    }

    async fn handle_notification(&self, notification: Notification) -> Result<()> {
        log_info!("handling notification: {:?}", notification);

        match &notification {
            Notification::UtxosChanged(utxos) => {
                for _entry in utxos.added.iter() {}

                for _entry in utxos.removed.iter() {}
            }
            _ => {
                log_warning!("unknown notification: {:?}", notification);
            }
        }
        Ok(())
    }

    async fn start_task(self: &Arc<Self>) -> Result<()> {
        let self_ = self.clone();

        let multiplexer = self.multiplexer.clone();
        let (mux_id, _mux_sender, mux_receiver) = multiplexer.register_event_channel();
        let task_ctl_receiver = self.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.task_ctl.response.sender.clone();
        let (task_start_sender, task_start_receiver) = oneshot::<()>();
        let notification_receiver = self.notification_channel.receiver.clone();

        spawn(async move {
            task_start_sender.send(()).await.unwrap();
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
                    msg = mux_receiver.recv().fuse() => {
                        if let Ok(msg) = msg {
                            match msg {
                                Events::Connect => {
                                    self_.connect().await.unwrap_or_else(|err| {
                                        log_error!("{err}");
                                    });
                                },
                                Events::Disconnect => {

                                    self_.disconnect().await.unwrap_or_else(|err| {
                                        log_error!("{err}");
                                    });
                                }
                            }
                        }
                    }
                }
            }
            multiplexer.unregister_event_channel(mux_id);
            task_ctl_sender.send(()).await.unwrap();
        });

        task_start_receiver.recv().await.unwrap();

        Ok(())
    }

    async fn stop_task(&self) -> Result<()> {
        self.task_ctl.signal(()).await.expect("Account::stop_task() `signal` error");
        Ok(())
    }
}

#[wasm_bindgen]
impl Account {
    #[wasm_bindgen(getter)]
    pub fn balance(&self) -> u64 {
        self.balance.load(std::sync::atomic::Ordering::SeqCst)
    }
}
