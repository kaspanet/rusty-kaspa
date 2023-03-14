use crate::imports::*;
use kaspa_rpc_core::notify::collector::RpcCoreCollector;
pub use kaspa_rpc_macros::build_wrpc_client_interface;
use std::fmt::Debug;

/// [`NotificationMoe`] controls notification delivery process
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub enum NotificationMode {
    /// Local notifier is used for notification processing.
    ///
    /// Multiple listeners can register and subscribe independently.
    MultiListeners,
    /// No notifier is present, notifications are relayed
    /// directly through the internal channel to a single listener.
    Direct,
}

#[derive(Clone)]
struct Inner {
    rpc: Arc<RpcClient<RpcApiOps>>,
    notification_channel: Channel<Notification>,
    encoding: Encoding,
}

impl Inner {
    pub fn new(encoding: Encoding, url: &str) -> Result<Inner> {
        let re = Regex::new(r"^wrpc").unwrap();
        let url = re.replace(url, "ws").to_string();
        // log_trace!("Kaspa wRPC::{encoding} connecting to: {url}");
        let options = RpcClientOptions { url: &url, ..RpcClientOptions::default() };

        let notification_channel = Channel::unbounded();

        // The `Interface` struct can be used to register for server-side
        // notifications. All notification methods have to be created at
        // this stage.
        let mut interface = Interface::<RpcApiOps>::new();

        [
            RpcApiOps::BlockAddedNotification,
            RpcApiOps::VirtualChainChangedNotification,
            RpcApiOps::FinalityConflictNotification,
            RpcApiOps::FinalityConflictResolvedNotification,
            RpcApiOps::UtxosChangedNotification,
            RpcApiOps::SinkBlueScoreChangedNotification,
            RpcApiOps::VirtualDaaScoreChangedNotification,
            RpcApiOps::PruningPointUtxoSetOverrideNotification,
            RpcApiOps::NewBlockTemplateNotification,
        ]
        .into_iter()
        .for_each(|notification_op| {
            let notification_sender_ = notification_channel.sender.clone();
            interface.notification(
                notification_op,
                workflow_rpc::client::Notification::new(move |notification: kaspa_rpc_core::Notification| {
                    let notification_sender = notification_sender_.clone();
                    Box::pin(async move {
                        // log_info!("notification receivers: {}", notification_sender.receiver_count());
                        // log_trace!("notification {:?}", notification);
                        if notification_sender.receiver_count() > 1 {
                            // log_info!("notification: posting to channel");
                            notification_sender.send(notification).await?;
                        } else {
                            log_warning!("WARNING: Kaspa RPC notification is not consumed by user: {:?}", notification);
                        }
                        Ok(())
                    })
                }),
            );
        });
        let rpc = Arc::new(RpcClient::new_with_encoding(encoding, interface.into(), options)?);

        let client = Self { rpc, notification_channel, encoding };

        Ok(client)
    }

    pub fn notification_channel_receiver(&self) -> Receiver<Notification> {
        self.notification_channel.receiver.clone()
    }

    #[allow(dead_code)]
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// Start sending notifications of some type to the client.
    async fn start_notify_to_client(&self, scope: Scope) -> RpcResult<()> {
        let _response: SubscribeResponse = self.rpc.call(RpcApiOps::Subscribe, scope).await.map_err(|err| err.to_string())?;
        Ok(())
    }

    /// Stop sending notifications of some type to the client.
    async fn stop_notify_to_client(&self, scope: Scope) -> RpcResult<()> {
        let _response: UnsubscribeResponse = self.rpc.call(RpcApiOps::Unsubscribe, scope).await.map_err(|err| err.to_string())?;
        Ok(())
    }
}

impl Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KaspaRpcClient")
            .field("rpc", &"rpc")
            .field("notification_channel", &self.notification_channel)
            .field("encoding", &self.encoding)
            .finish()
    }
}

#[async_trait]
impl SubscriptionManager for Inner {
    async fn start_notify(&self, _: ListenerId, scope: Scope) -> NotifyResult<()> {
        log_trace!("[WrpcClient] start_notify: {:?}", scope);
        self.start_notify_to_client(scope).await.map_err(|err| NotifyError::General(err.to_string()))?;
        Ok(())
    }

    async fn stop_notify(&self, _: ListenerId, scope: Scope) -> NotifyResult<()> {
        log_trace!("[WrpcClient] stop_notify: {:?}", scope);
        self.stop_notify_to_client(scope).await.map_err(|err| NotifyError::General(err.to_string()))?;
        Ok(())
    }
}

const WRPC_CLIENT: &str = "wrpc-client";

/// [`KaspaRpcClient`] allows connection to the Kaspa wRPC Server via
/// binary Borsh or JSON protocols.
///
/// RpcClient has two ways to interface with the underlying RPC subsystem:
/// [`Interface`] that has a [`notification()`](Interface::notification)
/// method to register closures that will be invoked on server-side
/// notifications and the [`RpcClient::call`] method that allows async
/// method invocation server-side.
///
#[derive(Clone)]
pub struct KaspaRpcClient {
    inner: Arc<Inner>,
    notifier: Option<Arc<Notifier<Notification, ChannelConnection>>>,
    notification_mode: NotificationMode,
}

impl KaspaRpcClient {
    /// Create a new `KaspaRpcClient` with the given Encoding and URL
    pub fn new(encoding: Encoding, url: &str) -> Result<KaspaRpcClient> {
        Self::new_with_args(encoding, NotificationMode::Direct, url)
    }

    /// Extended constructor that accepts [`NotificationMode`] argument.
    pub fn new_with_args(encoding: Encoding, notification_mode: NotificationMode, url: &str) -> Result<KaspaRpcClient> {
        let inner = Arc::new(Inner::new(encoding, url)?);
        let notifier = if matches!(notification_mode, NotificationMode::MultiListeners) {
            let enabled_events = EVENT_TYPE_ARRAY[..].into();
            let collector = Arc::new(RpcCoreCollector::new(inner.notification_channel_receiver()));
            let subscriber = Arc::new(Subscriber::new(enabled_events, inner.clone(), 0));
            Some(Arc::new(Notifier::new(enabled_events, vec![collector], vec![subscriber], 3, WRPC_CLIENT)))
        } else {
            None
        };

        let client = KaspaRpcClient { inner, notifier, notification_mode };

        Ok(client)
    }

    /// Starts RPC services.
    pub async fn start(&self) -> Result<()> {
        match &self.notification_mode {
            NotificationMode::MultiListeners => {
                self.notifier.clone().unwrap().start();
            }
            NotificationMode::Direct => {}
        }
        Ok(())
    }

    /// Stops background services.
    pub async fn stop(&self) -> Result<()> {
        match &self.notification_mode {
            NotificationMode::MultiListeners => {
                log_info!("stop notifier...");
                self.notifier.as_ref().unwrap().stop().await?;
            }
            NotificationMode::Direct => {
                // log_info!("stop direct...");
                // self.notification_ctl.signal(()).await?;
            }
        }
        Ok(())
    }

    /// Starts a background async connection task connecting
    /// to the wRPC server.  If the supplied `block` call is `true`
    /// this function will block until the first successful
    /// connection.
    pub async fn connect(&self, block: bool) -> Result<Option<Listener>> {
        Ok(self.inner.rpc.connect(block).await?)
    }

    /// Stop and shutdown RPC disconnecting existing connections
    /// and stopping reconnection process.
    pub async fn shutdown(&self) -> Result<()> {
        Ok(self.inner.rpc.shutdown().await?)
    }

    /// A helper function that is not `async`, allowing connection
    /// process to be initiated from non-async contexts.
    pub fn connect_as_task(&self) -> Result<()> {
        let self_ = self.clone();
        workflow_core::task::spawn(async move {
            self_.inner.rpc.connect(false).await.ok();
        });
        Ok(())
    }

    pub fn notification_channel_receiver(&self) -> Receiver<Notification> {
        self.inner.notification_channel.receiver.clone()
    }

    pub fn encoding(&self) -> Encoding {
        self.inner.encoding
    }
}

#[async_trait]
impl RpcApi<ChannelConnection> for KaspaRpcClient {
    //
    // The following proc-macro iterates over the array of enum variants
    // generating a function for each variant as follows:
    //
    // async fn ping_call(&self, request : PingRequest) -> RpcResult<PingResponse> {
    //     let response: ClientResult<PingResponse> = self.inner.rpc.call(RpcApiOps::Ping, request).await;
    //     Ok(response.map_err(|e| e.to_string())?)
    // }

    build_wrpc_client_interface!(
        RpcApiOps,
        [
            AddPeer,
            Ban,
            EstimateNetworkHashesPerSecond,
            GetBalanceByAddress,
            GetBalancesByAddresses,
            GetBlock,
            GetBlockCount,
            GetBlockDagInfo,
            GetBlocks,
            GetBlockTemplate,
            GetCoinSupply,
            GetConnectedPeerInfo,
            GetCurrentNetwork,
            GetHeaders,
            GetInfo,
            GetMempoolEntries,
            GetMempoolEntriesByAddresses,
            GetMempoolEntry,
            GetPeerAddresses,
            GetProcessMetrics,
            GetSelectedTipHash,
            GetSubnetwork,
            GetUtxosByAddresses,
            GetSinkBlueScore,
            GetVirtualChainFromBlock,
            Ping,
            ResolveFinalityConflict,
            Shutdown,
            SubmitBlock,
            SubmitTransaction,
            Unban,
        ]
    );

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    /// Register a new listener and returns an id and a channel receiver.
    fn register_new_listener(&self, connection: ChannelConnection) -> ListenerId {
        match self.notification_mode {
            NotificationMode::MultiListeners => self.notifier.as_ref().unwrap().register_new_listener(connection),
            NotificationMode::Direct => ListenerId::default(),
        }
    }

    /// Unregister an existing listener.
    ///
    /// Stop all notifications for this listener and drop its channel.
    async fn unregister_listener(&self, id: ListenerId) -> RpcResult<()> {
        match self.notification_mode {
            NotificationMode::MultiListeners => {
                self.notifier.as_ref().unwrap().unregister_listener(id)?;
            }
            NotificationMode::Direct => {}
        }
        Ok(())
    }

    /// Start sending notifications of some type to a listener.
    async fn start_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        match self.notification_mode {
            NotificationMode::MultiListeners => {
                self.notifier.clone().unwrap().try_start_notify(id, scope)?;
            }
            NotificationMode::Direct => {
                self.inner.start_notify_to_client(scope).await?;
            }
        }
        Ok(())
    }

    /// Stop sending notifications of some type to a listener.
    async fn stop_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        match self.notification_mode {
            NotificationMode::MultiListeners => {
                self.notifier.clone().unwrap().try_stop_notify(id, scope)?;
            }
            NotificationMode::Direct => {
                self.inner.stop_notify_to_client(scope).await?;
            }
        }
        Ok(())
    }
}
