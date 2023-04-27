#[allow(unused_imports)]
use crate::accounts::{gen0::*, gen1::*, PubkeyDerivationManagerTrait, WalletDerivationManagerTrait};
use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
use crate::storage::{self, PrvKeyDataId, PubKeyData};
use crate::utxo::{UtxoEntryReference, UtxoOrdering, UtxoSet};
use crate::wallet::Events;
use crate::AddressDerivationManager;
use crate::DynRpcApi;
use async_trait::async_trait;
//use kaspa_bip32::ExtendedPublicKey;
use kaspa_notify::listener::ListenerId;
use kaspa_notify::scope::{Scope, UtxosChangedScope};
use kaspa_rpc_core::api::notifications::Notification;
use kaspa_rpc_core::notify::connection::ChannelConnection;
//use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64};
use workflow_core::channel::{oneshot, Channel, DuplexChannel, Multiplexer};
// use workflow_core::channel::{Channel, DuplexChannel, Multiplexer, Receiver};
use crate::address::AddressManager;
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
    pub listener_id: ListenerId,
    pub stored: storage::Account,
}

/// Wallet `Account` data structure. An account is typically a single
/// HD-key derivation (derived from a wallet or from an an external secret)
#[wasm_bindgen(inspectable)]
pub struct Account {
    // TODO bind with accounts/ primitives
    // _generator: Arc<dyn WalletAccountTrait>,
    inner: Arc<Mutex<Inner>>,
    utxos: UtxoSet,
    balance: AtomicU64,
    rpc: Arc<DynRpcApi>,
    multiplexer: Multiplexer<Events>,
    is_connected: AtomicBool,
    #[wasm_bindgen(js_name = "accountKind")]
    pub account_kind: AccountKind,
    pub account_index: u32,
    #[wasm_bindgen(skip)]
    pub prv_key_data_id: Option<PrvKeyDataId>,
    pub ecdsa: bool,
    // ~
    // pub derivation_path : DerivationPath,
    #[wasm_bindgen(skip)]
    pub derivation: Arc<AddressDerivationManager>,

    #[wasm_bindgen(skip)]
    pub task_ctl: DuplexChannel,
}

impl Account {
    pub async fn try_new_with_args(
        rpc_api: Arc<DynRpcApi>,
        multiplexer: Multiplexer<Events>,
        name: &str,
        title: &str,
        account_kind: AccountKind,
        account_index: u32,
        prv_key_data_id: Option<PrvKeyDataId>,
        pub_key_data: PubKeyData,
        ecdsa: bool,
        address_prefix: AddressPrefix,
    ) -> Result<Self> {
        let channel = Channel::<Notification>::unbounded();
        let listener_id = rpc_api.register_new_listener(ChannelConnection::new(channel.sender));

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

        let inner = Inner { listener_id, stored };

        Ok(Account {
            utxos: UtxoSet::default(),
            balance: AtomicU64::new(0),
            // _generator: Arc::new(config.clone()),
            rpc: rpc_api.clone(),
            multiplexer,
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
        })
    }

    pub async fn try_new_from_storage(
        rpc: Arc<DynRpcApi>,
        multiplexer: Multiplexer<Events>,
        stored: &storage::Account,
        address_prefix: AddressPrefix,
    ) -> Result<Self> {
        let channel = Channel::<Notification>::unbounded();
        let listener_id = rpc.register_new_listener(ChannelConnection::new(channel.sender));

        // rpc_api.register_new_listener();
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

        let inner = Inner { listener_id, stored: stored.clone() };

        Ok(Account {
            utxos: UtxoSet::default(),
            balance: AtomicU64::new(0),
            // _generator: Arc::new(config.clone()),
            rpc, //: rpc.clone(),
            multiplexer,
            is_connected: AtomicBool::new(false),
            inner: Arc::new(Mutex::new(inner)),
            account_kind: stored.account_kind,
            account_index: stored.account_index,
            prv_key_data_id: stored.prv_key_data_id,
            ecdsa: stored.ecdsa,
            // -
            derivation,
            task_ctl: DuplexChannel::oneshot(),
        })
    }

    pub async fn subscribe(&self) {
        // TODO query account interface
        let addresses = vec![];
        let utxos_changed_scope = UtxosChangedScope { addresses };
        let id = self.inner.lock().unwrap().listener_id;
        let _ = self.rpc.start_notify(id, Scope::UtxosChanged(utxos_changed_scope)).await;
    }

    pub async fn update_balance(&mut self) -> Result<u64> {
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

    pub async fn scan_utxos(&self, scan_depth: u32, window_size: u32) -> Result<u64> {
        self.utxos.clear();

        // let scan_depth: usize = 1024;
        // let window_size: usize = 128;

        let receive = self.derivation.receive_address_manager();
        let change = self.derivation.change_address_manager();

        // let mut scan = Scan::new(receive,change,window_size,ScanExtent::EmptyRange(window_size));
        // let next = scan.next().await;

        let receive_index = receive.index()?;
        let change_index = change.index()?;

        let _last_receive = receive_index + window_size;
        let _last_change = change_index + window_size;

        let mut balance = 0u64;
        let mut cursor = 0;
        while cursor < scan_depth {
            let first = cursor;
            let last = cursor + window_size;
            cursor = last;

            log_info!("first: {}, last: {}", first, last);

            let addresses = receive.get_range(cursor..(cursor + window_size)).await?;

            // - TODO - populate address range from derivators/generators
            // let _addresses = Vec::<Address>::new();

            let resp = self.rpc.get_utxos_by_addresses(addresses.clone()).await?;

            let refs: Vec<UtxoEntryReference> = resp.into_iter().map(UtxoEntryReference::from).collect();

            balance += refs.iter().map(|r| r.as_ref().amount()).sum::<u64>();

            self.utxos.extend(&refs);
        }

        // - TODO - post balance updates to the wallet

        self.utxos.order(UtxoOrdering::AscendingAmount)?;

        Ok(balance)
    }

    pub async fn estimate(&self, _address: &Address, _amount_sompi: u64, _priority_fee_sompi: u64) -> Result<Estimate> {
        todo!()
        // Ok(())
    }

    pub async fn send(
        &self,
        _address: &Address,
        _amount_sompi: u64,
        _priority_fee_sompi: u64,
        _payment_secret: Option<Secret>,
    ) -> Result<()> {
        todo!()
        // Ok(())
    }

    pub async fn address(&self) -> Result<Address> {
        todo!()
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
        self.subscribe_notififcations().await?;

        Ok(())
    }

    /// handle disconnection event
    pub async fn disconnect(&self) -> Result<()> {
        Ok(())
    }

    async fn subscribe_notififcations(&self) -> Result<()> {
        // - TODO - subscribe to notifications from the wallet notifier
        Ok(())
    }

    async fn start_task(self: &Arc<Self>) -> Result<()> {
        let _self = self.clone();

        let multiplexer = self.multiplexer.clone();
        let (mux_id, _mux_sender, mux_receiver) = multiplexer.register_event_channel();
        let task_ctl_receiver = self.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.task_ctl.response.sender.clone();
        let (task_start_sender, task_start_receiver) = oneshot::<()>();

        spawn(async move {
            task_start_sender.send(()).await.unwrap();
            loop {
                select! {
                    _ = task_ctl_receiver.recv().fuse() => {
                        break;
                    },
                    msg = mux_receiver.recv().fuse() => {
                        if let Ok(msg) = msg {
                            match msg {
                                Events::Connect => {
                                    // - TODO -
                                    _self.connect().await.unwrap_or_else(|err| {
                                        log_error!("{err}");
                                    });
                                },
                                Events::Disconnect => {

                                    _self.disconnect().await.unwrap_or_else(|err| {
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

// ----------------------------------------------------------------------------
// TODO - relocate to scan.rs
pub struct Cursor {
    pub done: bool,
    index: u32,
    derivation: Arc<AddressManager>,
}

impl Cursor {
    pub fn new(derivation: Arc<AddressManager>) -> Self {
        Self { index: 0, done: false, derivation }
    }

    pub async fn next(&mut self, n: u32) -> Result<Vec<Address>> {
        let list = self.derivation.get_range(self.index..self.index + n).await?;
        self.index += n;
        Ok(list)
    }
}

pub enum ScanExtent {
    /// Scan until an empty range is found
    EmptyRange(u32),
    /// Scan until a specific depth (a particular derivation index)
    Depth(u32),
}

pub struct Scan {
    pub derivations: Vec<Cursor>,
    pub window_size: u32,
    pub extent: ScanExtent,
    pub pos: usize,
}

impl Scan {
    pub fn new(receive: Arc<AddressManager>, change: Arc<AddressManager>, window_size: u32, extent: ScanExtent) -> Self {
        let derivations = vec![Cursor::new(receive), Cursor::new(change)];
        Scan { derivations, window_size, extent, pos: 0 }
    }

    pub async fn next(&mut self) -> Result<Option<Vec<Address>>> {
        let len = self.derivations.len();
        if let Some(cursor) = self.derivations.get_mut(self.pos) {
            self.pos += 1;
            if self.pos >= len {
                self.pos = 0;
            }

            let list = cursor.next(self.window_size).await?;
            Ok(Some(list))
        } else {
            Ok(None)
        }
    }
}
