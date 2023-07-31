use futures::{select, FutureExt};
use kaspa_notify::{
    listener::ListenerId,
    scope::{Scope, UtxosChangedScope, VirtualDaaScoreChangedScope},
};
use kaspa_rpc_core::message::UtxosChangedNotification;
use kaspa_wrpc_client::KaspaRpcClient;
use workflow_core::channel::{Channel, DuplexChannel};
use workflow_core::task::spawn;
use workflow_rpc::client::Ctl;

use crate::imports::*;
use crate::result::Result;
use crate::utxo::{PendingUtxoEntryReference, UtxoContext, UtxoContextId, UtxoEntryId, UtxoEntryReference};
use crate::{events::Events, runtime::SyncMonitor};
use kaspa_rpc_core::{
    notify::connection::{ChannelConnection, ChannelType},
    Notification,
};
use std::collections::HashMap;

pub struct Inner {
    pending: DashMap<UtxoEntryId, PendingUtxoEntryReference>,
    address_to_utxo_context_map: DashMap<Arc<Address>, UtxoContext>,
    // event_consumer: Mutex<Option<Arc<dyn EventConsumer>>>,
    current_daa_score: Arc<AtomicU64>,
    network_id: Arc<Mutex<Option<NetworkId>>>,

    rpc: Arc<DynRpcApi>,
    is_connected: AtomicBool,
    // is_synced: AtomicBool,
    listener_id: Mutex<Option<ListenerId>>,
    task_ctl: DuplexChannel,
    notification_channel: Channel<Notification>,
    multiplexer: Multiplexer<Events>,
    sync_proc: SyncMonitor,
}

impl Inner {
    pub fn new(rpc: &Arc<DynRpcApi>, network_id: Option<NetworkId>, multiplexer: &Multiplexer<Events>) -> Self {
        Self {
            pending: DashMap::new(),
            address_to_utxo_context_map: DashMap::new(),
            // event_consumer: Mutex::new(None),
            current_daa_score: Arc::new(AtomicU64::new(0)),
            network_id: Arc::new(Mutex::new(network_id)),

            rpc: rpc.clone(),
            is_connected: AtomicBool::new(false),
            // is_synced: AtomicBool::new(false),
            listener_id: Mutex::new(None),
            task_ctl: DuplexChannel::oneshot(),
            notification_channel: Channel::<Notification>::unbounded(),
            multiplexer: multiplexer.clone(),
            sync_proc: SyncMonitor::new(rpc, multiplexer),
        }
    }
}

#[derive(Clone)]
#[wasm_bindgen]
pub struct UtxoProcessor {
    inner: Arc<Inner>,
}

impl UtxoProcessor {
    pub fn new(rpc: &Arc<DynRpcApi>, network_id: Option<NetworkId>, multiplexer: &Multiplexer<Events>) -> Self {
        UtxoProcessor { inner: Arc::new(Inner::new(rpc, network_id, multiplexer)) }
    }

    pub fn rpc(&self) -> &Arc<DynRpcApi> {
        &self.inner.rpc
    }

    pub fn rpc_client(&self) -> Arc<KaspaRpcClient> {
        self.rpc().clone().downcast_arc::<KaspaRpcClient>().expect("unable to downcast DynRpcApi to KaspaRpcClient")
    }

    pub fn multiplexer(&self) -> &Multiplexer<Events> {
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

    // pub fn current_daa_score(&self) -> u64 {
    //     self.inner.virtual_daa_score.load(Ordering::SeqCst)
    // }

    pub fn current_daa_score(&self) -> Option<u64> {
        self.is_connected().then_some(self.inner.current_daa_score.load(Ordering::SeqCst))
    }

    pub async fn clear(&self) -> Result<()> {
        self.inner.address_to_utxo_context_map.clear();
        // TODO - clear processors?
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
                // let listener_id = self.listener_id();
                // log_info!("registering addresses {:?}", addresses);

                let utxos_changed_scope = UtxosChangedScope { addresses };
                self.rpc().start_notify(self.listener_id(), Scope::UtxosChanged(utxos_changed_scope)).await?;
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
                // log_info!("unregistering addresses {:?}", addresses);
                let utxos_changed_scope = UtxosChangedScope { addresses };
                self.rpc().stop_notify(self.listener_id(), Scope::UtxosChanged(utxos_changed_scope)).await?;
            } else {
                log_info!("unregistering empty address list!");
            }
        }
        Ok(())
    }

    // pub fn register_event_consumer(&self, event_consumer : Arc<dyn EventConsumer>) {
    //     self.inner.event_consumer.lock().unwrap().replace(event_consumer);
    // }

    // pub fn event_consumer(&self) -> Option<Arc<dyn EventConsumer>> {
    //     self.inner.event_consumer.lock().unwrap().clone()
    // }

    pub async fn notify(&self, event: Events) -> Result<()> {
        self.multiplexer()
            .broadcast(event)
            .await
            .map_err(|_| Error::Custom("multiplexer channel error during update_balance".to_string()))?;
        Ok(())
    }

    pub async fn handle_daa_score_change(&self, current_daa_score: u64) -> Result<()> {
        self.inner.current_daa_score.store(current_daa_score, Ordering::SeqCst);
        self.notify(Events::DAAScoreChange(current_daa_score)).await?;
        self.handle_pending(current_daa_score).await?;
        Ok(())
    }

    // pub async fn handle_pending(&self, current_daa_score: u64) -> Result<Vec<Arc<Account>>> {
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

        let mut contexts = HashMap::<UtxoContextId, UtxoContext>::default();
        for mature in mature_entries.into_iter() {
            let utxo_context = &mature.utxo_context;
            let entry = mature.entry;
            utxo_context.promote(entry);

            contexts.insert(utxo_context.id(), utxo_context.clone());
        }

        let contexts = contexts.values().cloned().collect::<Vec<_>>();

        for context in contexts.iter() {
            context.update_balance().await?;
        }

        Ok(())
    }

    pub async fn handle_utxo_changed(&self, utxos: UtxosChangedNotification) -> Result<()> {
        // log_info!("utxo changed: {:?}", utxos);
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

            pub async fn init_state_from_server(self: &Arc<Self>) -> Result<()> {

                let kaspa_rpc_core::GetInfoResponse { is_synced, is_utxo_indexed: has_utxo_index, server_version, .. } = self.rpc().get_info().await?;

                if !has_utxo_index {
                    self.notify(Events::UtxoIndexNotEnabled).await?;
                    return Err(Error::MissingUtxoIndex);
                }

                let kaspa_rpc_core::GetBlockDagInfoResponse { virtual_daa_score, network: server_network_id, .. } = self.rpc().get_block_dag_info().await?;

                let server_network_id = NetworkId::from(server_network_id);
                let network_id = self.network_id()?;
                if network_id != server_network_id {
                    return Err(Error::InvalidNetworkType(network_id.to_string(), server_network_id.to_string()));
                }

                self.inner.current_daa_score.store(virtual_daa_score, Ordering::SeqCst);

                log_info!("Connected to kaspad: '{server_version}' on '{server_network_id}';  SYNC: {is_synced}  DAA: {virtual_daa_score}");

                self.sync_proc().track(is_synced).await?;
                self.notify(Events::ServerStatus { server_version, is_synced, network_id, url: self.rpc_client().url().to_string() }).await?;

                Ok(())
            }

        } else {

            pub async fn init_state_from_server(self: &Arc<Self>) -> Result<()> {

                let GetConnectionInfoResponse { server_version, network_id: server_network_id, has_utxo_index, is_synced, virtual_daa_score } =
                self.rpc().get_connection_info().await?;

                if !has_utxo_index {
                    self.notify(Events::UtxoIndexNotEnabled).await?;
                    return Err(Error::MissingUtxoIndex);
                }

                let network_id = self.network_id()?;
                let server_network_id = NetworkId::from(server_network_id);
                if network_id != server_network_id {
                    return Err(Error::InvalidNetworkType(network_id.to_string(), server_network_id.to_string()));
                }

                self.inner.current_daa_score.store(virtual_daa_score, Ordering::SeqCst);

                log_info!("Connected to kaspad: '{server_version}' on '{server_network_id}';  SYNC: {is_synced}  DAA: {virtual_daa_score}");
                self.sync_proc().track(is_synced).await?;
                self.notify(Events::ServerStatus { server_version, is_synced, network_id, url: self.rpc_client().url().to_string() }).await?;

                Ok(())
            }
        }
    }

    pub async fn handle_connect_impl(self: &Arc<Self>) -> Result<()> {
        self.init_state_from_server().await?;

        self.inner.is_connected.store(true, Ordering::SeqCst);
        self.register_notification_listener().await?;
        // self.start_task().await?;
        self.notify(Events::UtxoProcStart).await?;
        Ok(())
    }

    pub async fn handle_connect(self: &Arc<Self>) -> Result<()> {
        if let Err(err) = self.handle_connect_impl().await {
            self.notify(Events::UtxoProcError(err.to_string())).await?;
            self.rpc_client().disconnect().await?;
        }
        Ok(())
    }

    pub async fn handle_disconnect(&self) -> Result<()> {
        self.inner.is_connected.store(false, Ordering::SeqCst);
        self.notify(Events::UtxoProcStop).await?;
        self.unregister_notification_listener().await?;
        // self.stop_task().await?;
        Ok(())
    }

    async fn register_notification_listener(&self) -> Result<()> {
        let listener_id = self
            .rpc()
            .register_new_listener(ChannelConnection::new(self.inner.notification_channel.sender.clone(), ChannelType::Persistent));
        *self.inner.listener_id.lock().unwrap() = Some(listener_id);

        self.rpc().start_notify(listener_id, Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;

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

    async fn handle_notification(&self, notification: Notification) -> Result<()> {
        // log_info!("handling notification: {:?}", notification);

        match notification {
            Notification::VirtualDaaScoreChanged(virtual_daa_score_changed_notification) => {
                self.handle_daa_score_change(virtual_daa_score_changed_notification.virtual_daa_score).await?;
            }

            Notification::UtxosChanged(utxos_changed_notification) => {
                if !self.is_synced() {
                    self.sync_proc().track(true).await?;
                    // self.inner.is_synced.store(true, Ordering::SeqCst);
                    // self.notify(Events::NodeSync { is_synced: true }).await?;
                }

                self.handle_utxo_changed(utxos_changed_notification).await?;
            }

            _ => {
                log_warning!("unknown notification: {:?}", notification);
            }
        }

        Ok(())
    }

    pub async fn start(self: &Arc<Self>) -> Result<()> {
        // *self.inner.event_consumer.lock().unwrap() = event_consumer;

        let this = self.clone();
        let rpc_ctl_channel = this
            .rpc()
            .clone()
            .downcast_arc::<KaspaRpcClient>()
            .expect("unable to downcast DynRpcApi to KaspaRpcClient")
            .ctl_multiplexer()
            .create_channel();

        let task_ctl_receiver = self.inner.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.inner.task_ctl.response.sender.clone();
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
                                        this.inner.multiplexer.broadcast(Events::Connect {
                                            network_id : this.network_id().expect("network id expected during connection"),
                                            url : this.rpc_client().url().to_string()
                                        }).await.unwrap_or_else(|err| log_error!("{err}"));
                                        this.handle_connect().await.unwrap_or_else(|err| log_error!("{err}"));
                                    },
                                    Ctl::Close => {
                                        this.inner.multiplexer.broadcast(Events::Disconnect {
                                            network_id : this.network_id().expect("network id expected during connection"),
                                            url : this.rpc_client().url().to_string()
                                        }).await.unwrap_or_else(|err| log_error!("{err}"));
                                        this.handle_disconnect().await.unwrap_or_else(|err| log_error!("{err}"));
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

                }
            }
            // this.inner.event_consumer.lock().unwrap().take();
            task_ctl_sender.send(()).await.unwrap();
        });
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.inner.sync_proc.stop().await?;
        self.inner.task_ctl.signal(()).await.expect("UtxoProcessor::stop_task() `signal` error");
        Ok(())
    }
}

#[wasm_bindgen]
impl UtxoProcessor {}
