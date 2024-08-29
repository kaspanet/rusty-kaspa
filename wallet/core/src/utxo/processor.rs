//!
//! Implements [`UtxoProcessor`], which is the main component
//! of the UTXO subsystem. It is responsible for managing and
//! coordinating multiple [`UtxoContext`] instances acting as
//! a hub for UTXO event dispersal and related processing.
//!

use crate::imports::*;
// use futures::pin_mut;
use kaspa_notify::{
    listener::ListenerId,
    scope::{Scope, UtxosChangedScope, VirtualDaaScoreChangedScope},
};
use kaspa_rpc_core::{
    api::{
        ctl::{RpcCtl, RpcState},
        ops::{RPC_API_REVISION, RPC_API_VERSION},
    },
    message::UtxosChangedNotification,
    GetServerInfoResponse,
};
use kaspa_wrpc_client::KaspaRpcClient;
use workflow_core::channel::{Channel, DuplexChannel, Sender};
use workflow_core::task::spawn;

use crate::events::Events;
use crate::result::Result;
use crate::utxo::{
    Maturity, OutgoingTransaction, PendingUtxoEntryReference, SyncMonitor, UtxoContext, UtxoEntryId, UtxoEntryReference,
};
use crate::wallet::WalletBusMessage;
use kaspa_rpc_core::{
    notify::connection::{ChannelConnection, ChannelType},
    Notification,
};
// use workflow_core::task;
// use kaspa_metrics_core::{Metrics,Metric};

pub struct Inner {
    /// Coinbase UTXOs in stasis
    stasis: DashMap<UtxoEntryId, PendingUtxoEntryReference>,
    /// UTXOs pending maturity
    pending: DashMap<UtxoEntryId, PendingUtxoEntryReference>,
    /// Outgoing Transactions
    outgoing: DashMap<TransactionId, OutgoingTransaction>,
    /// Address to UtxoContext map (maps all addresses used by
    /// all UtxoContexts to their respective UtxoContexts)
    address_to_utxo_context_map: DashMap<Arc<Address>, UtxoContext>,
    // ---
    current_daa_score: Arc<AtomicU64>,
    network_id: Arc<Mutex<Option<NetworkId>>>,
    rpc: Mutex<Option<Rpc>>,
    is_connected: AtomicBool,
    listener_id: Mutex<Option<ListenerId>>,
    task_ctl: DuplexChannel,
    task_is_running: AtomicBool,
    notification_channel: Channel<Notification>,
    sync_proc: SyncMonitor,
    multiplexer: Multiplexer<Box<Events>>,
    wallet_bus: Option<Channel<WalletBusMessage>>,
    notification_guard: AsyncMutex<()>,
    connect_disconnect_guard: AsyncMutex<()>,
    metrics: Arc<Metrics>,
    metrics_kinds: Mutex<Vec<MetricsUpdateKind>>,
    connection_signaler: Mutex<Option<Sender<std::result::Result<(), String>>>>,
}

impl Inner {
    pub fn new(
        rpc: Option<Rpc>,
        network_id: Option<NetworkId>,
        multiplexer: Multiplexer<Box<Events>>,
        wallet_bus: Option<Channel<WalletBusMessage>>,
    ) -> Self {
        Self {
            stasis: DashMap::new(),
            pending: DashMap::new(),
            outgoing: DashMap::new(),
            address_to_utxo_context_map: DashMap::new(),
            current_daa_score: Arc::new(AtomicU64::new(0)),
            network_id: Arc::new(Mutex::new(network_id)),
            rpc: Mutex::new(rpc.clone()),
            is_connected: AtomicBool::new(false),
            listener_id: Mutex::new(None),
            task_ctl: DuplexChannel::oneshot(),
            task_is_running: AtomicBool::new(false),
            notification_channel: Channel::<Notification>::unbounded(),
            sync_proc: SyncMonitor::new(rpc.clone(), &multiplexer),
            multiplexer,
            wallet_bus,
            notification_guard: Default::default(),
            connect_disconnect_guard: Default::default(),
            metrics: Arc::new(Metrics::default()),
            metrics_kinds: Mutex::new(vec![]),
            connection_signaler: Mutex::new(None),
        }
    }
}

#[derive(Clone)]
pub struct UtxoProcessor {
    inner: Arc<Inner>,
}

impl UtxoProcessor {
    pub fn new(
        rpc: Option<Rpc>,
        network_id: Option<NetworkId>,
        multiplexer: Option<Multiplexer<Box<Events>>>,
        wallet_bus: Option<Channel<WalletBusMessage>>,
    ) -> Self {
        let multiplexer = multiplexer.unwrap_or_default();
        UtxoProcessor { inner: Arc::new(Inner::new(rpc, network_id, multiplexer, wallet_bus)) }
    }

    pub fn rpc_api(&self) -> Arc<DynRpcApi> {
        self.inner.rpc.lock().unwrap().as_ref().expect("UtxoProcessor RPC not initialized").rpc_api().clone()
    }

    pub fn try_rpc_api(&self) -> Option<Arc<DynRpcApi>> {
        self.inner.rpc.lock().unwrap().as_ref().map(|rpc| rpc.rpc_api()).cloned()
    }

    pub fn rpc_ctl(&self) -> RpcCtl {
        self.inner.rpc.lock().unwrap().as_ref().expect("UtxoProcessor RPC not initialized").rpc_ctl().clone()
    }

    pub fn try_rpc_ctl(&self) -> Option<RpcCtl> {
        self.inner.rpc.lock().unwrap().as_ref().map(|rpc| rpc.rpc_ctl()).cloned()
    }

    pub fn rpc_url(&self) -> Option<String> {
        self.rpc_ctl().descriptor()
    }

    pub fn rpc_client(&self) -> Option<Arc<KaspaRpcClient>> {
        self.rpc_api().clone().downcast_arc::<KaspaRpcClient>().ok()
    }

    pub async fn bind_rpc(&self, rpc: Option<Rpc>) -> Result<()> {
        self.inner.rpc.lock().unwrap().clone_from(&rpc);
        let rpc_api = rpc.as_ref().map(|rpc| rpc.rpc_api().clone());
        self.metrics().bind_rpc(rpc_api);
        self.sync_proc().bind_rpc(rpc).await?;
        Ok(())
    }

    pub fn metrics(&self) -> &Arc<Metrics> {
        &self.inner.metrics
    }

    pub fn wallet_bus(&self) -> &Option<Channel<WalletBusMessage>> {
        &self.inner.wallet_bus
    }

    pub fn has_rpc(&self) -> bool {
        self.inner.rpc.lock().unwrap().is_some()
    }

    pub fn multiplexer(&self) -> &Multiplexer<Box<Events>> {
        &self.inner.multiplexer
    }

    pub async fn notification_lock(&self) -> AsyncMutexGuard<()> {
        self.inner.notification_guard.lock().await
    }

    pub fn sync_proc(&self) -> &SyncMonitor {
        &self.inner.sync_proc
    }

    pub fn listener_id(&self) -> Result<ListenerId> {
        self.inner.listener_id.lock().unwrap().ok_or(Error::ListenerId)
    }

    pub fn set_network_id(&self, network_id: &NetworkId) {
        self.inner.network_id.lock().unwrap().replace(*network_id);
    }

    pub fn network_id(&self) -> Result<NetworkId> {
        (*self.inner.network_id.lock().unwrap()).ok_or(Error::MissingNetworkId)
    }

    pub fn network_params(&self) -> Result<&'static NetworkParams> {
        // pub fn network_params(&self) -> Result<NetworkParams> {
        let network_id = (*self.inner.network_id.lock().unwrap()).ok_or(Error::MissingNetworkId)?;
        Ok(NetworkParams::from(network_id))
        // Ok(network_id.into())
    }

    pub fn pending(&self) -> &DashMap<UtxoEntryId, PendingUtxoEntryReference> {
        &self.inner.pending
    }

    pub fn outgoing(&self) -> &DashMap<TransactionId, OutgoingTransaction> {
        &self.inner.outgoing
    }

    pub fn stasis(&self) -> &DashMap<UtxoEntryId, PendingUtxoEntryReference> {
        &self.inner.stasis
    }

    pub fn current_daa_score(&self) -> Option<u64> {
        self.is_connected().then_some(self.inner.current_daa_score.load(Ordering::SeqCst))
    }

    pub fn address_to_utxo_context_map(&self) -> &DashMap<Arc<Address>, UtxoContext> {
        &self.inner.address_to_utxo_context_map
    }

    pub fn address_to_utxo_context(&self, address: &Address) -> Option<UtxoContext> {
        self.inner.address_to_utxo_context_map.get(address).map(|v| v.clone())
    }

    pub async fn register_addresses(&self, addresses: Vec<Arc<Address>>, utxo_context: &UtxoContext) -> Result<()> {
        addresses.iter().for_each(|address| {
            self.inner.address_to_utxo_context_map.insert(address.clone(), utxo_context.clone());
        });

        if self.is_connected() {
            if !addresses.is_empty() {
                let addresses = addresses.into_iter().map(|address| (*address).clone()).collect::<Vec<_>>();
                let utxos_changed_scope = UtxosChangedScope::new(addresses);
                self.rpc_api().start_notify(self.listener_id()?, utxos_changed_scope.into()).await?;
            } else {
                log_error!("registering an empty address list!");
            }
        }
        Ok(())
    }

    pub async fn unregister_addresses(&self, addresses: Vec<Arc<Address>>) -> Result<()> {
        addresses.iter().for_each(|address| {
            self.inner.address_to_utxo_context_map.remove(address);
        });

        if self.is_connected() {
            if !addresses.is_empty() {
                let addresses = addresses.into_iter().map(|address| (*address).clone()).collect::<Vec<_>>();
                let utxos_changed_scope = UtxosChangedScope::new(addresses);
                self.rpc_api().stop_notify(self.listener_id()?, utxos_changed_scope.into()).await?;
            } else {
                log_error!("unregistering empty address list!");
            }
        }
        Ok(())
    }

    pub async fn notify(&self, event: Events) -> Result<()> {
        self.multiplexer()
            .try_broadcast(Box::new(event))
            .map_err(|_| Error::Custom("multiplexer channel error during notify".to_string()))?;
        Ok(())
    }

    pub fn try_notify(&self, event: Events) -> Result<()> {
        self.multiplexer()
            .try_broadcast(Box::new(event))
            .map_err(|_| Error::Custom("multiplexer channel error during try_notify".to_string()))?;
        Ok(())
    }

    pub async fn handle_daa_score_change(&self, current_daa_score: u64) -> Result<()> {
        self.inner.current_daa_score.store(current_daa_score, Ordering::SeqCst);
        self.notify(Events::DaaScoreChange { current_daa_score }).await?;
        self.handle_pending(current_daa_score).await?;
        self.handle_outgoing(current_daa_score).await?;
        Ok(())
    }

    #[allow(clippy::mutable_key_type)]
    pub async fn handle_pending(&self, current_daa_score: u64) -> Result<()> {
        let params = self.network_params()?;

        let (mature_entries, revived_entries) = {
            // scan and remove any pending entries that gained maturity
            let mut mature_entries = vec![];
            let pending_entries = &self.inner.pending;
            pending_entries.retain(|_, pending_entry| match pending_entry.maturity(params, current_daa_score) {
                Maturity::Confirmed => {
                    mature_entries.push(pending_entry.clone());
                    false
                }
                _ => true,
            });

            // scan and remove any stasis entries that can now become pending
            // or gained maturity
            let mut revived_entries = vec![];
            let stasis_entries = &self.inner.stasis;
            stasis_entries.retain(|_, stasis_entry| {
                match stasis_entry.maturity(params, current_daa_score) {
                    Maturity::Confirmed => {
                        mature_entries.push(stasis_entry.clone());
                        false
                    }
                    Maturity::Pending => {
                        revived_entries.push(stasis_entry.clone());
                        // relocate from stasis to pending ...
                        pending_entries.insert(stasis_entry.id(), stasis_entry.clone());
                        false
                    }
                    Maturity::Stasis => true,
                }
            });
            (mature_entries, revived_entries)
        };

        // ------

        let promotions =
            HashMap::group_from(mature_entries.into_iter().map(|utxo| (utxo.inner.utxo_context.clone(), utxo.inner.entry.clone())));
        let mut updated_contexts: HashSet<UtxoContext> = HashSet::from_iter(promotions.keys().cloned());

        for (context, utxos) in promotions.into_iter() {
            context.promote(utxos).await?;
        }

        // ------

        let revivals =
            HashMap::group_from(revived_entries.into_iter().map(|utxo| (utxo.inner.utxo_context.clone(), utxo.inner.entry.clone())));
        updated_contexts.extend(revivals.keys().cloned());

        for (context, utxos) in revivals.into_iter() {
            context.revive(utxos).await?;
        }

        for context in updated_contexts.into_iter() {
            context.update_balance().await?;
        }

        Ok(())
    }

    async fn handle_outgoing(&self, current_daa_score: u64) -> Result<()> {
        let longevity = self.network_params()?.user_transaction_maturity_period_daa();

        self.inner.outgoing.retain(|_, outgoing| {
            if outgoing.acceptance_daa_score() != 0 && (outgoing.acceptance_daa_score() + longevity) < current_daa_score {
                outgoing.originating_context().remove_outgoing_transaction(&outgoing.id());
                false
            } else {
                true
            }
        });

        Ok(())
    }

    pub fn register_outgoing_transaction(&self, outgoing_transaction: OutgoingTransaction) {
        self.inner.outgoing.insert(outgoing_transaction.id(), outgoing_transaction);
    }

    pub fn cancel_outgoing_transaction(&self, transaction_id: TransactionId) {
        self.inner.outgoing.remove(&transaction_id);
    }

    pub async fn handle_discovery(&self, record: TransactionRecord) -> Result<()> {
        if let Some(wallet_bus) = self.wallet_bus() {
            // if UtxoProcessor has an associated wallet_bus installed
            // by the wallet, cascade the discovery to the wallet so that
            // it can check if the record exists in its storage and handle
            // it in accordance to its policies.
            wallet_bus.sender.send(WalletBusMessage::Discovery { record }).await?;
        } else {
            // otherwise we fetch the unixtime and broadcast the discovery event
            let transaction_daa_score = record.block_daa_score();
            match self.rpc_api().get_daa_score_timestamp_estimate(vec![transaction_daa_score]).await {
                Ok(timestamps) => {
                    if let Some(timestamp) = timestamps.first() {
                        let mut record = record.clone();
                        record.set_unixtime(*timestamp);
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

    pub async fn handle_utxo_changed(&self, utxos: UtxosChangedNotification) -> Result<()> {
        let current_daa_score = self.current_daa_score().expect("DAA score expected when handling UTXO Changed notifications");

        #[allow(clippy::mutable_key_type)]
        let mut updated_contexts: HashSet<UtxoContext> = HashSet::default();

        let removed = (*utxos.removed).clone().into_iter().filter_map(|entry| entry.address.clone().map(|address| (address, entry)));
        let removed = HashMap::group_from(removed);
        for (address, entries) in removed.into_iter() {
            if let Some(utxo_context) = self.address_to_utxo_context(&address) {
                updated_contexts.insert(utxo_context.clone());
                let entries = entries.into_iter().map(|entry| entry.into()).collect::<Vec<_>>();
                utxo_context.handle_utxo_removed(entries, current_daa_score).await?;
            } else {
                log_error!("receiving UTXO Changed 'removed' notification for an unknown address: {}", address);
            }
        }

        let added = (*utxos.added).clone().into_iter().filter_map(|entry| entry.address.clone().map(|address| (address, entry)));
        let added = HashMap::group_from(added);
        for (address, entries) in added.into_iter() {
            if let Some(utxo_context) = self.address_to_utxo_context(&address) {
                updated_contexts.insert(utxo_context.clone());
                let entries = entries.into_iter().map(|entry| entry.into()).collect::<Vec<UtxoEntryReference>>();
                utxo_context.handle_utxo_added(entries, current_daa_score).await?;
            } else {
                log_error!("receiving UTXO Changed 'added' notification for an unknown address: {}", address);
            }
        }

        // iterate over all affected utxo contexts and
        // update as well as notify their balances.
        for context in updated_contexts.iter() {
            context.update_balance().await?;
        }

        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.inner.is_connected.load(Ordering::SeqCst)
    }

    pub fn is_synced(&self) -> bool {
        self.sync_proc().is_synced()
    }

    pub fn is_running(&self) -> bool {
        self.inner.task_is_running.load(Ordering::SeqCst)
    }

    pub async fn init_state_from_server(&self) -> Result<bool> {
        let GetServerInfoResponse {
            rpc_api_version,
            rpc_api_revision,
            server_version,
            network_id: server_network_id,
            has_utxo_index,
            is_synced,
            virtual_daa_score,
        } = self.rpc_api().get_server_info().await?;

        if rpc_api_version > RPC_API_VERSION {
            let current = format!("{RPC_API_VERSION}.{RPC_API_REVISION}");
            let connected = format!("{rpc_api_version}.{rpc_api_revision}");
            return Err(Error::RpcApiVersion(current, connected));
        }

        if !has_utxo_index {
            self.notify(Events::UtxoIndexNotEnabled { url: self.rpc_url() }).await?;
            return Err(Error::MissingUtxoIndex);
        }

        let network_id = self.network_id()?;
        if network_id != server_network_id {
            return Err(Error::InvalidNetworkType(network_id.to_string(), server_network_id.to_string()));
        }

        self.inner.current_daa_score.store(virtual_daa_score, Ordering::SeqCst);

        log_trace!("Connected to kaspad: '{server_version}' on '{server_network_id}';  SYNC: {is_synced}  DAA: {virtual_daa_score}");
        self.notify(Events::ServerStatus { server_version, is_synced, network_id, url: self.rpc_url() }).await?;

        Ok(is_synced)
    }

    pub async fn handle_connect_impl(&self) -> Result<()> {
        let is_synced = self.init_state_from_server().await?;
        self.inner.is_connected.store(true, Ordering::SeqCst);
        self.register_notification_listener().await?;
        self.notify(Events::UtxoProcStart).await?;
        self.sync_proc().track(is_synced).await?;

        let this = self.clone();
        self.inner.metrics.register_sink(Arc::new(Box::new(move |snapshot: MetricsSnapshot| {
            if let Err(err) = this.deliver_metrics_snapshot(Box::new(snapshot)) {
                println!("Error ingesting metrics snapshot: {}", err);
            }
            None
        })));

        Ok(())
    }

    /// Allows use to supply a channel Sender that will
    /// receive the result of the wRPC connection attempt.
    pub fn set_connection_signaler(&self, signal: Sender<std::result::Result<(), String>>) {
        *self.inner.connection_signaler.lock().unwrap() = Some(signal);
    }

    fn signal_connection(&self, result: std::result::Result<(), String>) -> bool {
        let signal = self.inner.connection_signaler.lock().unwrap().take();
        if let Some(signal) = signal.as_ref() {
            let _ = signal.try_send(result);
            true
        } else {
            false
        }
    }

    pub async fn handle_connect(&self) -> Result<()> {
        let _ = self.inner.connect_disconnect_guard.lock().await;

        match self.handle_connect_impl().await {
            Err(err) => {
                if !self.signal_connection(Err(err.to_string())) {
                    log_error!("UtxoProcessor: error while connecting to node: {err}");
                }
                self.notify(Events::UtxoProcError { message: err.to_string() }).await?;
                if let Some(client) = self.rpc_client() {
                    // try force disconnect the client if we have failed
                    // to negotiate the connection to the node.
                    client.disconnect().await?;
                }
                Err(err)
            }
            Ok(_) => {
                self.signal_connection(Ok(()));
                Ok(())
            }
        }
    }

    pub async fn handle_disconnect(&self) -> Result<()> {
        let _ = self.inner.connect_disconnect_guard.lock().await;

        self.inner.is_connected.store(false, Ordering::SeqCst);
        // self.stop_metrics();

        self.inner.metrics.unregister_sink();

        self.unregister_notification_listener().await?;
        self.notify(Events::UtxoProcStop).await?;
        self.cleanup().await?;

        Ok(())
    }

    pub async fn cleanup(&self) -> Result<()> {
        self.inner.pending.clear();
        self.inner.stasis.clear();
        self.inner.outgoing.clear();
        self.inner.address_to_utxo_context_map.clear();
        Ok(())
    }

    async fn register_notification_listener(&self) -> Result<()> {
        let listener_id = self.rpc_api().register_new_listener(ChannelConnection::new(
            "utxo processor",
            self.inner.notification_channel.sender.clone(),
            ChannelType::Persistent,
        ));
        *self.inner.listener_id.lock().unwrap() = Some(listener_id);
        self.rpc_api().start_notify(listener_id, Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    async fn unregister_notification_listener(&self) -> Result<()> {
        let listener_id = self.inner.listener_id.lock().unwrap().take();
        if let Some(id) = listener_id {
            // we do not need this as we are unregister the entire listener here...
            self.rpc_api().unregister_listener(id).await?;
        }
        Ok(())
    }

    async fn handle_notification(&self, notification: Notification) -> Result<()> {
        let _lock = self.notification_lock().await;

        match notification {
            Notification::VirtualDaaScoreChanged(virtual_daa_score_changed_notification) => {
                self.handle_daa_score_change(virtual_daa_score_changed_notification.virtual_daa_score).await?;
            }

            Notification::UtxosChanged(utxos_changed_notification) => {
                if !self.is_synced() {
                    self.sync_proc().track(true).await?;
                }

                self.handle_utxo_changed(utxos_changed_notification).await?;
            }

            _ => {
                log_warn!("unknown notification: {:?}", notification);
            }
        }

        Ok(())
    }

    fn deliver_metrics_snapshot(&self, snapshot: Box<MetricsSnapshot>) -> Result<()> {
        let metrics_kinds = self.inner.metrics_kinds.lock().unwrap().clone();
        for kind in metrics_kinds.into_iter() {
            match kind {
                MetricsUpdateKind::WalletMetrics => {
                    let mempool_size = snapshot.get(&Metric::NetworkMempoolSize) as u64;
                    let node_peers = snapshot.get(&Metric::NodeActivePeers) as u32;
                    let network_tps = snapshot.get(&Metric::NetworkTransactionsPerSecond);
                    let metrics = MetricsUpdate::WalletMetrics { mempool_size, node_peers, network_tps };
                    self.try_notify(Events::Metrics { network_id: self.network_id()?, metrics })?;
                }
            }
        }

        Ok(())
    }

    pub async fn start_metrics(&self) -> Result<()> {
        self.inner.metrics.start_task().await?;
        self.inner.metrics.bind_rpc(Some(self.rpc_api().clone()));

        Ok(())
    }

    pub async fn stop_metrics(&self) -> Result<()> {
        self.inner.metrics.stop_task().await?;
        self.inner.metrics.bind_rpc(None);

        Ok(())
    }

    pub async fn start(&self) -> Result<()> {
        let this = self.clone();
        if this.inner.task_is_running.load(Ordering::SeqCst) {
            return Err(Error::custom("UtxoProcessor::start() called while task is already running"));
        }
        this.inner.task_is_running.store(true, Ordering::SeqCst);
        let rpc_ctl_channel = this.rpc_ctl().multiplexer().channel();
        let task_ctl_receiver = self.inner.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.inner.task_ctl.response.sender.clone();
        let notification_receiver = self.inner.notification_channel.receiver.clone();

        // handle power up on an already connected rpc channel
        // clients relying on UtxoProcessor state should monitor
        // for and handle `UtxoProcStart` and `UtxoProcStop` events.
        if this.rpc_ctl().is_connected() {
            this.handle_connect().await.unwrap_or_else(|err| log_error!("{err}"));
        }

        spawn(async move {
            loop {
                select_biased! {
                    msg = rpc_ctl_channel.receiver.recv().fuse() => {
                        match msg {
                            Ok(msg) => {

                                // handle RPC channel connection and disconnection events
                                match msg {
                                    RpcState::Connected => {
                                        if !this.is_connected() && this.handle_connect().await.is_ok() {
                                            this.inner.multiplexer.try_broadcast(Box::new(Events::Connect {
                                                network_id : this.network_id().expect("network id expected during connection"),
                                                url : this.rpc_url()
                                            })).unwrap_or_else(|err| log_error!("{err}"));
                                        }
                                    },
                                    RpcState::Disconnected => {
                                        if this.is_connected() {
                                            this.inner.multiplexer.try_broadcast(Box::new(Events::Disconnect {
                                                network_id : this.network_id().expect("network id expected during connection"),
                                                url : this.rpc_url()
                                            })).unwrap_or_else(|err| log_error!("{err}"));
                                            this.handle_disconnect().await.unwrap_or_else(|err| log_error!("{err}"));
                                        }
                                    }
                                }
                            }
                            Err(err) => {
                                log_error!("UtxoProcessor: error while receiving rpc_ctl_channel message: {err}");
                                log_error!("Suspending UTXO processor...");
                                break;
                            }
                        }
                    }
                    notification = notification_receiver.recv().fuse() => {
                        match notification {
                            Ok(notification) => {
                                if let Err(err) = this.handle_notification(notification).await {
                                    this.notify(Events::UtxoProcError { message: err.to_string() }).await.ok();
                                    log_error!("error while handling notification: {err}");
                                }
                            }
                            Err(err) => {
                                log_error!("RPC notification channel error: {err}");
                                log_error!("Suspending UTXO processor...");
                                break;
                            }
                        }
                    },

                    // we use select_biased to drain rpc_ctl
                    // and notifications before shutting down
                    // as such task_ctl is last in the poll order
                    _ = task_ctl_receiver.recv().fuse() => {
                        break;
                    },

                }
            }

            // handle power down on rpc channel that remains connected
            if this.is_connected() {
                this.handle_disconnect().await.unwrap_or_else(|err| log_error!("{err}"));
            }

            this.inner.task_is_running.store(false, Ordering::SeqCst);
            task_ctl_sender.send(()).await.unwrap();
        });
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        if self.inner.task_is_running.load(Ordering::SeqCst) {
            self.inner.sync_proc.stop().await?;
            self.inner.task_ctl.signal(()).await.expect("UtxoProcessor::stop_task() `signal` error");
        }
        Ok(())
    }

    pub fn enable_metrics_kinds(&self, metrics_kinds: &[MetricsUpdateKind]) {
        *self.inner.metrics_kinds.lock().unwrap() = metrics_kinds.to_vec();
    }
}

#[cfg(test)]
pub(crate) mod mock {
    use super::*;

    impl UtxoProcessor {
        pub fn mock_set_connected(&self, connected: bool) {
            self.inner.is_connected.store(connected, Ordering::SeqCst);
        }

        // pub fn mock_set_daa_score(&self, connected : bool) {
        //     self.inner.is_connected.store(connected, Ordering::SeqCst);
        // }
    }
}
