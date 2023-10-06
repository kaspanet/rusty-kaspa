use futures::{select_biased, FutureExt};
use kaspa_notify::{
    listener::ListenerId,
    scope::{Scope, UtxosChangedScope, VirtualDaaScoreChangedScope},
};
use kaspa_rpc_core::{
    api::ctl::{RpcCtl, RpcState},
    message::UtxosChangedNotification,
};
use kaspa_wrpc_client::KaspaRpcClient;
use workflow_core::channel::{Channel, DuplexChannel};
use workflow_core::task::spawn;

use crate::imports::*;
use crate::result::Result;
use crate::utxo::{PendingUtxoEntryReference, UtxoContext, UtxoEntryId, UtxoEntryReference};
use crate::{events::Events, runtime::SyncMonitor};
use kaspa_rpc_core::{
    notify::connection::{ChannelConnection, ChannelType},
    Notification,
};
use std::collections::HashMap;

pub struct Inner {
    /// UTXOs pending maturity (confirmation)
    pending: DashMap<UtxoEntryId, PendingUtxoEntryReference>,
    /// Address to UtxoContext map (maps all addresses used by
    /// all UtxoContexts to their respective UtxoContexts)
    address_to_utxo_context_map: DashMap<Arc<Address>, UtxoContext>,
    /// UtxoContexts that have recoverable UTXOs (UTXOs used in
    /// outgoing transactions, that have not yet been confirmed)
    recoverable_contexts: DashSet<UtxoContext>,
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
}

impl Inner {
    pub fn new(rpc: Option<Rpc>, network_id: Option<NetworkId>, multiplexer: Multiplexer<Box<Events>>) -> Self {
        Self {
            pending: DashMap::new(),
            address_to_utxo_context_map: DashMap::new(),
            recoverable_contexts: DashSet::new(),
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
        }
    }
}

#[derive(Clone)]
pub struct UtxoProcessor {
    inner: Arc<Inner>,
}

impl UtxoProcessor {
    pub fn new(rpc: Option<Rpc>, network_id: Option<NetworkId>, multiplexer: Option<Multiplexer<Box<Events>>>) -> Self {
        let multiplexer = multiplexer.unwrap_or_default();
        UtxoProcessor { inner: Arc::new(Inner::new(rpc, network_id, multiplexer)) }
    }

    pub fn rpc_api(&self) -> Arc<DynRpcApi> {
        self.inner.rpc.lock().unwrap().as_ref().expect("UtxoProcessor RPC not initialized").rpc_api().clone()
    }

    pub fn rpc_ctl(&self) -> RpcCtl {
        self.inner.rpc.lock().unwrap().as_ref().expect("UtxoProcessor RPC not initialized").rpc_ctl().clone()
    }

    pub fn rpc_url(&self) -> Option<String> {
        self.rpc_ctl().descriptor()
    }

    pub fn rpc_client(&self) -> Option<Arc<KaspaRpcClient>> {
        self.rpc_api().clone().downcast_arc::<KaspaRpcClient>().ok()
    }

    pub async fn bind_rpc(&self, rpc: Option<Rpc>) -> Result<()> {
        *self.inner.rpc.lock().unwrap() = rpc.clone();
        self.sync_proc().bind_rpc(rpc).await?;
        Ok(())
    }

    pub fn has_rpc(&self) -> bool {
        self.inner.rpc.lock().unwrap().is_some()
    }

    pub fn multiplexer(&self) -> &Multiplexer<Box<Events>> {
        &self.inner.multiplexer
    }

    pub fn sync_proc(&self) -> &SyncMonitor {
        &self.inner.sync_proc
    }

    pub fn listener_id(&self) -> ListenerId {
        self.inner.listener_id.lock().unwrap().expect("missing listener_id in UtxoProcessor::listener_id()")
    }

    pub fn set_network_id(&self, network_id: NetworkId) {
        self.inner.network_id.lock().unwrap().replace(network_id);
    }

    pub fn network_id(&self) -> Result<NetworkId> {
        (*self.inner.network_id.lock().unwrap()).ok_or(Error::MissingNetworkId)
    }

    pub fn pending(&self) -> &DashMap<UtxoEntryId, PendingUtxoEntryReference> {
        &self.inner.pending
    }

    pub fn current_daa_score(&self) -> Option<u64> {
        self.is_connected().then_some(self.inner.current_daa_score.load(Ordering::SeqCst))
    }

    pub async fn clear(&self) -> Result<()> {
        self.inner.address_to_utxo_context_map.clear();
        Ok(())
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
                let utxos_changed_scope = UtxosChangedScope { addresses };
                self.rpc_api().start_notify(self.listener_id(), Scope::UtxosChanged(utxos_changed_scope)).await?;
            } else {
                log_info!("registering empty address list!");
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
                let utxos_changed_scope = UtxosChangedScope { addresses };
                self.rpc_api().stop_notify(self.listener_id(), Scope::UtxosChanged(utxos_changed_scope)).await?;
            } else {
                log_info!("unregistering empty address list!");
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
        self.notify(Events::DAAScoreChange { current_daa_score }).await?;
        self.handle_pending(current_daa_score).await?;
        self.handle_recoverable(current_daa_score).await?;
        Ok(())
    }

    pub async fn handle_pending(&self, current_daa_score: u64) -> Result<()> {
        let mature_entries = {
            let mut mature_entries = vec![];
            let pending_entries = &self.inner.pending;
            pending_entries.retain(|_, pending| {
                if pending.is_mature(current_daa_score) {
                    mature_entries.push(pending.clone());
                    false
                } else {
                    true
                }
            });
            mature_entries
        };

        // ------

        let promotions =
            HashMap::group_from(mature_entries.into_iter().map(|utxo| (utxo.inner.utxo_context.clone(), utxo.inner.entry.clone())));
        let contexts = promotions.keys().cloned().collect::<Vec<_>>();

        for (context, utxos) in promotions.into_iter() {
            context.promote(utxos).await?;
        }

        for context in contexts.into_iter() {
            context.update_balance().await?;
        }

        Ok(())
    }

    async fn handle_recoverable(&self, current_daa_score: u64) -> Result<()> {
        self.inner.recoverable_contexts.retain(|context| context.recover(current_daa_score, None));

        Ok(())
    }

    pub fn register_recoverable_context(&self, context: &UtxoContext) {
        self.inner.recoverable_contexts.insert(context.clone());
    }

    pub async fn handle_utxo_changed(&self, utxos: UtxosChangedNotification) -> Result<()> {
        let removed = (*utxos.removed).clone().into_iter().filter_map(|entry| entry.address.clone().map(|address| (address, entry)));
        let removed = HashMap::group_from(removed);
        for (address, entries) in removed.into_iter() {
            if let Some(utxo_context) = self.address_to_utxo_context(&address) {
                let entries = entries.into_iter().map(|entry| entry.into()).collect::<Vec<_>>();
                utxo_context.handle_utxo_removed(entries).await?;
            } else {
                log_error!("receiving UTXO Changed 'removed' notification for an unknown address: {}", address);
            }
        }

        let added = (*utxos.added).clone().into_iter().filter_map(|entry| entry.address.clone().map(|address| (address, entry)));
        let added = HashMap::group_from(added);
        for (address, entries) in added.into_iter() {
            if let Some(utxo_context) = self.address_to_utxo_context(&address) {
                let entries = entries.into_iter().map(|entry| entry.into()).collect::<Vec<UtxoEntryReference>>();
                utxo_context.handle_utxo_added(entries).await?;
            } else {
                log_error!("receiving UTXO Changed 'added' notification for an unknown address: {}", address);
            }
        }

        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.inner.is_connected.load(Ordering::SeqCst)
    }

    pub fn is_synced(&self) -> bool {
        self.sync_proc().is_synced()
    }

    cfg_if! {
        if #[cfg(feature = "legacy-rpc")] {

            pub async fn init_state_from_server(&self) -> Result<()> {

                let kaspa_rpc_core::GetInfoResponse { is_synced, is_utxo_indexed: has_utxo_index, server_version, .. } = self.rpc_api().get_info().await?;

                if !has_utxo_index {
                    self.notify(Events::UtxoIndexNotEnabled { url: self.rpc_url() }).await?;
                    return Err(Error::MissingUtxoIndex);
                }

                let kaspa_rpc_core::GetBlockDagInfoResponse { virtual_daa_score, network: server_network_id, .. } = self.rpc_api().get_block_dag_info().await?;

                let network_id = self.network_id()?;
                if network_id != server_network_id {
                    return Err(Error::InvalidNetworkType(network_id.to_string(), server_network_id.to_string()));
                }

                self.inner.current_daa_score.store(virtual_daa_score, Ordering::SeqCst);

                log_trace!("Connected to kaspad: '{server_version}' on '{server_network_id}';  SYNC: {is_synced}  DAA: {virtual_daa_score}");

                self.sync_proc().track(is_synced).await?;
                self.notify(Events::ServerStatus { server_version, is_synced, network_id, url: self.rpc_url() }).await?;

                Ok(())
            }

        } else {

            pub async fn init_state_from_server(&self) -> Result<()> {

                let GetServerInfoResponse { server_version, network_id: server_network_id, has_utxo_index, is_synced, virtual_daa_score } =
                self.rpc().get_server_info().await?;

                if !has_utxo_index {
                    self.notify(Events::UtxoIndexNotEnabled { url: self.rpc_url() }).await?;
                    return Err(Error::MissingUtxoIndex);
                }

                let network_id = self.network_id()?;
                let server_network_id = NetworkId::from(server_network_id);
                if network_id != server_network_id {
                    return Err(Error::InvalidNetworkType(network_id.to_string(), server_network_id.to_string()));
                }

                self.inner.current_daa_score.store(virtual_daa_score, Ordering::SeqCst);

                log_trace!("Connected to kaspad: '{server_version}' on '{server_network_id}';  SYNC: {is_synced}  DAA: {virtual_daa_score}");
                self.sync_proc().track(is_synced).await?;
                self.notify(Events::ServerStatus { server_version, is_synced, network_id, url: self.rpc_url() }).await?;

                Ok(())
            }
        }
    }

    pub async fn handle_connect_impl(&self) -> Result<()> {
        self.init_state_from_server().await?;

        self.inner.is_connected.store(true, Ordering::SeqCst);
        self.register_notification_listener().await?;
        self.notify(Events::UtxoProcStart).await?;
        Ok(())
    }

    pub async fn handle_connect(&self) -> Result<()> {
        if let Err(err) = self.handle_connect_impl().await {
            log_error!("UtxoProcessor: error while connecting to node: {err}");
            self.notify(Events::UtxoProcError { message: err.to_string() }).await?;
            if let Some(client) = self.rpc_client() {
                client.disconnect().await?;
            }
        }
        Ok(())
    }

    pub async fn handle_disconnect(&self) -> Result<()> {
        self.inner.is_connected.store(false, Ordering::SeqCst);
        self.notify(Events::UtxoProcStop).await?;
        self.unregister_notification_listener().await?;

        Ok(())
    }

    async fn register_notification_listener(&self) -> Result<()> {
        let listener_id = self
            .rpc_api()
            .register_new_listener(ChannelConnection::new(self.inner.notification_channel.sender.clone(), ChannelType::Persistent));
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
        // log_info!("handling notification: {:?}", notification);

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
                log_warning!("unknown notification: {:?}", notification);
            }
        }

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
                                    RpcState::Opened => {
                                        if !this.is_connected() {
                                            this.inner.multiplexer.try_broadcast(Box::new(Events::Connect {
                                                network_id : this.network_id().expect("network id expected during connection"),
                                                url : this.rpc_url()
                                            })).unwrap_or_else(|err| log_error!("{err}"));
                                            this.handle_connect().await.unwrap_or_else(|err| log_error!("{err}"));
                                        }
                                    },
                                    RpcState::Closed => {
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
                                this.handle_notification(notification).await.unwrap_or_else(|err| {
                                    log_error!("error while handling notification: {err}");
                                });
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
}
