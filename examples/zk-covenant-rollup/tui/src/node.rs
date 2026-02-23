use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures::{select_biased, FutureExt};
use workflow_core::channel::{Channel, DuplexChannel, Receiver};
use workflow_core::task::spawn;

use kaspa_addresses::Address;
use kaspa_rpc_core::{api::rpc::RpcApi, GetVirtualChainFromBlockV2Response, RpcDataVerbosityLevel, RpcHash, RpcTransaction};
use kaspa_wrpc_client::prelude::*;
use kaspa_wrpc_client::result::Result;

/// Events emitted by `KaspaNode` for the TUI event loop.
#[derive(Debug)]
pub enum NodeEvent {
    Connected,
    Disconnected,
    Notification(Notification),
}

struct Inner {
    client: Arc<KaspaRpcClient>,
    notification_channel: Channel<Notification>,
    listener_id: Mutex<Option<ListenerId>>,
    task_ctl: DuplexChannel<()>,
    is_connected: AtomicBool,
    event_channel: Channel<NodeEvent>,
}

/// Wrapper around `KaspaRpcClient` that manages connection lifecycle,
/// notification subscriptions, and exposes a channel of `NodeEvent`s
/// for consumption by the TUI event loop.
#[derive(Clone)]
pub struct KaspaNode {
    inner: Arc<Inner>,
}

impl KaspaNode {
    /// Create a new node client for the given URL and network.
    pub fn try_new(url: &str, network_id: NetworkId) -> Result<Self> {
        let client = Arc::new(KaspaRpcClient::new_with_args(WrpcEncoding::Borsh, Some(url), None, Some(network_id), None)?);

        let inner = Inner {
            client,
            notification_channel: Channel::unbounded(),
            listener_id: Mutex::new(None),
            task_ctl: DuplexChannel::oneshot(),
            is_connected: AtomicBool::new(false),
            event_channel: Channel::unbounded(),
        };

        Ok(Self { inner: Arc::new(inner) })
    }

    /// The underlying RPC client.
    pub fn client(&self) -> &Arc<KaspaRpcClient> {
        &self.inner.client
    }

    /// Whether we are currently connected.
    pub fn is_connected(&self) -> bool {
        self.inner.is_connected.load(Ordering::SeqCst)
    }

    /// Receiver for `NodeEvent`s — used by the TUI event loop.
    pub fn event_receiver(&self) -> Receiver<NodeEvent> {
        self.inner.event_channel.receiver.clone()
    }

    /// Connect to the node and start the background event task.
    pub async fn connect(&self) -> Result<()> {
        self.start_event_task().await?;

        let options = ConnectOptions { block_async_connect: true, ..Default::default() };
        self.client().connect(Some(options)).await?;
        Ok(())
    }

    /// Disconnect and stop the background event task.
    pub async fn stop(&self) -> Result<()> {
        self.client().disconnect().await?;
        self.stop_event_task().await?;
        Ok(())
    }

    /// Subscribe to UTXO changes for the given addresses.
    pub async fn subscribe_utxos(&self, addresses: Vec<Address>) -> Result<()> {
        let id = { *self.inner.listener_id.lock().unwrap() };
        if let Some(id) = id {
            self.client().rpc_api().start_notify(id, Scope::UtxosChanged(UtxosChangedScope { addresses })).await?;
        }
        Ok(())
    }

    /// Get UTXO entries for the given addresses.
    pub async fn get_utxos_by_addresses(&self, addresses: Vec<Address>) -> Result<Vec<kaspa_rpc_core::RpcUtxosByAddressesEntry>> {
        Ok(self.client().get_utxos_by_addresses(addresses).await?)
    }

    /// Query the virtual chain from a starting block hash with confirmation depth.
    pub async fn get_virtual_chain_v2(
        &self,
        start_hash: RpcHash,
        min_confirmations: Option<u32>,
    ) -> Result<GetVirtualChainFromBlockV2Response> {
        Ok(self
            .client()
            .get_virtual_chain_from_block_v2(start_hash, Some(RpcDataVerbosityLevel::High), min_confirmations.map(|v| v as u64))
            .await?)
    }

    /// Submit a transaction to the network.
    pub async fn submit_transaction(&self, tx: RpcTransaction, allow_orphan: bool) -> Result<RpcHash> {
        Ok(self.client().submit_transaction(tx, allow_orphan).await?)
    }

    /// Query the virtual chain (v1) — returns block hashes and optional accepted tx IDs.
    pub async fn get_virtual_chain_from_block(
        &self,
        start_hash: RpcHash,
        include_accepted_transaction_ids: bool,
        min_confirmations: Option<u32>,
    ) -> Result<kaspa_rpc_core::GetVirtualChainFromBlockResponse> {
        Ok(self
            .client()
            .get_virtual_chain_from_block(start_hash, include_accepted_transaction_ids, min_confirmations.map(|v| v as u64))
            .await?)
    }

    /// Get a block by hash (header + optional transactions).
    pub async fn get_block(&self, hash: RpcHash, include_transactions: bool) -> Result<kaspa_rpc_core::RpcBlock> {
        Ok(self.client().get_block(hash, include_transactions).await?)
    }

    /// Get block DAG info (useful for pruning point hash).
    pub async fn get_block_dag_info(&self) -> Result<kaspa_rpc_core::GetBlockDagInfoResponse> {
        Ok(self.client().get_block_dag_info().await?)
    }

    // ── Internal ──

    async fn register_notification_listeners(&self) -> Result<()> {
        let listener_id = self.client().rpc_api().register_new_listener(ChannelConnection::new(
            "zk-covenant-rollup-tui",
            self.inner.notification_channel.sender.clone(),
            ChannelType::Persistent,
        ));
        *self.inner.listener_id.lock().unwrap() = Some(listener_id);

        // Subscribe to virtual DAA score so we can track chain tip
        self.client().rpc_api().start_notify(listener_id, Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;

        Ok(())
    }

    async fn unregister_notification_listener(&self) -> Result<()> {
        let listener_id = self.inner.listener_id.lock().unwrap().take();
        if let Some(id) = listener_id {
            self.client().rpc_api().unregister_listener(id).await?;
        }
        Ok(())
    }

    async fn handle_connect(&self) -> Result<()> {
        self.register_notification_listeners().await?;
        self.inner.is_connected.store(true, Ordering::SeqCst);
        let _ = self.inner.event_channel.sender.send(NodeEvent::Connected).await;
        Ok(())
    }

    async fn handle_disconnect(&self) -> Result<()> {
        self.unregister_notification_listener().await?;
        self.inner.is_connected.store(false, Ordering::SeqCst);
        let _ = self.inner.event_channel.sender.send(NodeEvent::Disconnected).await;
        Ok(())
    }

    async fn start_event_task(&self) -> Result<()> {
        let node = self.clone();
        let rpc_ctl_channel = self.client().rpc_ctl().multiplexer().channel();
        let task_ctl_receiver = self.inner.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.inner.task_ctl.response.sender.clone();
        let notification_receiver = self.inner.notification_channel.receiver.clone();
        let event_sender = self.inner.event_channel.sender.clone();

        spawn(async move {
            loop {
                select_biased! {
                    msg = rpc_ctl_channel.receiver.recv().fuse() => {
                        match msg {
                            Ok(msg) => match msg {
                                RpcState::Connected => {
                                    if let Err(err) = node.handle_connect().await {
                                        eprintln!("Error in connect handler: {err}");
                                    }
                                }
                                RpcState::Disconnected => {
                                    if let Err(err) = node.handle_disconnect().await {
                                        eprintln!("Error in disconnect handler: {err}");
                                    }
                                }
                            },
                            Err(err) => {
                                eprintln!("RPC CTL channel error: {err}");
                                break;
                            }
                        }
                    }
                    notification = notification_receiver.recv().fuse() => {
                        match notification {
                            Ok(notification) => {
                                let _ = event_sender.send(NodeEvent::Notification(notification)).await;
                            }
                            Err(err) => {
                                eprintln!("RPC notification channel error: {err}");
                                break;
                            }
                        }
                    }
                    _ = task_ctl_receiver.recv().fuse() => {
                        break;
                    }
                }
            }

            if node.is_connected() {
                let _ = node.handle_disconnect().await;
            }

            let _ = task_ctl_sender.send(()).await;
        });
        Ok(())
    }

    async fn stop_event_task(&self) -> Result<()> {
        self.inner.task_ctl.signal(()).await.expect("stop_event_task() signal error");
        Ok(())
    }
}
