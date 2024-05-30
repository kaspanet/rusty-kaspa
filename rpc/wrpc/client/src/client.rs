use crate::imports::*;
use crate::parse::parse_host;
use crate::{error::Error, node::NodeDescriptor};
use kaspa_consensus_core::network::NetworkType;
use kaspa_notify::{
    listener::ListenerLifespan,
    subscription::{context::SubscriptionContext, MutationPolicies, UtxosChangedMutationPolicy},
};
use kaspa_rpc_core::{
    api::ctl::RpcCtl,
    notify::collector::{RpcCoreCollector, RpcCoreConverter},
};
pub use kaspa_rpc_macros::build_wrpc_client_interface;
use std::fmt::Debug;
use workflow_core::{channel::Multiplexer, runtime as application_runtime};
use workflow_dom::utils::window;
use workflow_rpc::client::Ctl as WrpcCtl;
pub use workflow_rpc::client::{
    ConnectOptions, ConnectResult, ConnectStrategy, Resolver as RpcResolver, ResolverResult, WebSocketConfig, WebSocketError,
};

type RpcClientNotifier = Arc<Notifier<Notification, ChannelConnection>>;

struct Inner {
    rpc_client: Arc<RpcClient<RpcApiOps>>,
    notification_relay_channel: Channel<Notification>,
    notification_intake_channel: Mutex<Channel<Notification>>,
    notifier: Arc<Mutex<Option<RpcClientNotifier>>>,
    encoding: Encoding,
    wrpc_ctl_multiplexer: Multiplexer<WrpcCtl>,
    rpc_ctl: RpcCtl,
    background_services_running: Arc<AtomicBool>,
    service_ctl: DuplexChannel<()>,
    connect_guard: AsyncMutex<()>,
    disconnect_guard: AsyncMutex<()>,
    // ---
    default_url: Mutex<Option<String>>,
    current_url: Mutex<Option<String>>,
    resolver: Mutex<Option<Resolver>>,
    network_id: Mutex<Option<NetworkId>>,
    node_descriptor: Mutex<Option<Arc<NodeDescriptor>>>,
}

impl Inner {
    pub fn new(encoding: Encoding, url: Option<&str>, resolver: Option<Resolver>, network_id: Option<NetworkId>) -> Result<Inner> {
        // log_trace!("Kaspa wRPC::{encoding} connecting to: {url}");
        let rpc_ctl = RpcCtl::with_descriptor(url);
        let wrpc_ctl_multiplexer = Multiplexer::<WrpcCtl>::new();

        let options = RpcClientOptions::new().with_ctl_multiplexer(wrpc_ctl_multiplexer.clone());

        let notification_relay_channel = Channel::unbounded();
        let notification_intake_channel = Mutex::new(Channel::unbounded());

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
            let notification_sender_ = notification_relay_channel.sender.clone();
            interface.notification(
                notification_op,
                workflow_rpc::client::Notification::new(move |notification: kaspa_rpc_core::Notification| {
                    let notification_sender = notification_sender_.clone();
                    Box::pin(async move {
                        // log_info!("notification receivers: {}", notification_sender.receiver_count());
                        // log_trace!("notification {:?}", notification);
                        if notification_sender.receiver_count() > 1 {
                            // log_info!("notification: posting to channel: {notification:?}");
                            notification_sender.send(notification).await?;
                        } else {
                            log_warn!("WARNING: Kaspa RPC notification is not consumed by user: {:?}", notification);
                        }
                        Ok(())
                    })
                }),
            );
        });

        let rpc = Arc::new(RpcClient::new_with_encoding(encoding, interface.into(), options, None)?);
        let client = Self {
            rpc_client: rpc,
            notification_relay_channel,
            notification_intake_channel,
            notifier: Default::default(),
            encoding,
            wrpc_ctl_multiplexer,
            rpc_ctl,
            service_ctl: DuplexChannel::unbounded(),
            background_services_running: Arc::new(AtomicBool::new(false)),
            connect_guard: async_std::sync::Mutex::new(()),
            disconnect_guard: async_std::sync::Mutex::new(()),
            // ---
            default_url: Mutex::new(url.map(|s| s.to_string())),
            current_url: Mutex::new(None),
            resolver: Mutex::new(resolver),
            network_id: Mutex::new(network_id),
            node_descriptor: Mutex::new(None),
        };
        Ok(client)
    }

    pub fn reset_notification_intake_channel(&self) {
        let mut intake = self.notification_intake_channel.lock().unwrap();
        intake.sender.close();
        *intake = Channel::unbounded();
    }

    /// Start sending notifications of some type to the client.
    async fn start_notify_to_client(&self, scope: Scope) -> RpcResult<()> {
        let _response: SubscribeResponse = self.rpc_client.call(RpcApiOps::Subscribe, scope).await.map_err(|err| err.to_string())?;
        Ok(())
    }

    /// Stop sending notifications of some type to the client.
    async fn stop_notify_to_client(&self, scope: Scope) -> RpcResult<()> {
        let _response: UnsubscribeResponse =
            self.rpc_client.call(RpcApiOps::Unsubscribe, scope).await.map_err(|err| err.to_string())?;
        Ok(())
    }

    fn default_url(&self) -> Option<String> {
        self.default_url.lock().unwrap().clone()
    }

    fn set_default_url(&self, url: Option<&str>) {
        *self.default_url.lock().unwrap() = url.map(String::from);
    }

    fn current_url(&self) -> Option<String> {
        self.current_url.lock().unwrap().clone()
    }

    fn set_current_url(&self, url: Option<&str>) {
        *self.current_url.lock().unwrap() = url.map(String::from);
    }

    fn resolver(&self) -> Option<Resolver> {
        self.resolver.lock().unwrap().clone()
    }

    fn network_id(&self) -> Option<NetworkId> {
        *self.network_id.lock().unwrap()
    }

    fn build_notifier(self: &Arc<Self>, subscription_context: Option<SubscriptionContext>) -> Result<RpcClientNotifier> {
        let receiver = self.notification_intake_channel.lock().unwrap().receiver.clone();

        let enabled_events = EVENT_TYPE_ARRAY[..].into();
        let converter = Arc::new(RpcCoreConverter::new());
        let collector = Arc::new(RpcCoreCollector::new(WRPC_CLIENT, receiver, converter));
        let subscriber = Arc::new(Subscriber::new(WRPC_CLIENT, enabled_events, self.clone(), 0));
        let policies = MutationPolicies::new(UtxosChangedMutationPolicy::AddressSet);
        let notifier = Arc::new(Notifier::new(
            WRPC_CLIENT,
            enabled_events,
            vec![collector],
            vec![subscriber],
            subscription_context.unwrap_or_default(),
            3,
            policies,
        ));

        // let receiver = self.notification_intake_channel.lock().unwrap().receiver.clone();
        // let enabled_events = EVENT_TYPE_ARRAY[..].into();
        // let converter = Arc::new(RpcCoreConverter::new());
        // let collector = Arc::new(RpcCoreCollector::new(WRPC_CLIENT, receiver, converter));
        // let subscriber = Arc::new(Subscriber::new(WRPC_CLIENT, enabled_events, self.clone(), 0));
        // let notifier = Arc::new(Notifier::new(WRPC_CLIENT, enabled_events, vec![collector], vec![subscriber], 3));
        *self.notifier.lock().unwrap() = Some(notifier.clone());
        Ok(notifier)
    }
}

impl Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KaspaRpcClient")
            .field("rpc", &"rpc")
            // .field("notification_channel", &self.notification_channel)
            .field("encoding", &self.encoding)
            .finish()
    }
}

#[async_trait]
impl SubscriptionManager for Inner {
    async fn start_notify(&self, _: ListenerId, scope: Scope) -> NotifyResult<()> {
        // log_trace!("[WrpcClient] start_notify: {:?}", scope);
        self.start_notify_to_client(scope).await.map_err(|err| NotifyError::General(err.to_string()))?;
        Ok(())
    }

    async fn stop_notify(&self, _: ListenerId, scope: Scope) -> NotifyResult<()> {
        // log_trace!("[WrpcClient] stop_notify: {:?}", scope);
        self.stop_notify_to_client(scope).await.map_err(|err| NotifyError::General(err.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl RpcResolver for Inner {
    async fn resolve_url(&self) -> ResolverResult {
        let url = if let Some(url) = self.default_url() {
            url
        } else if let Some(resolver) = self.resolver().as_ref() {
            let network_id = self.network_id().expect("Resolver requires network id in RPC client configuration");
            let node = resolver.get_node(self.encoding, network_id).await.map_err(WebSocketError::custom)?;
            let url = node.url.clone();
            self.node_descriptor.lock().unwrap().replace(Arc::new(node));
            url
        } else {
            panic!("RpcClient resolver configuration error (expecting Some(Resolver))")
        };

        self.rpc_ctl.set_descriptor(Some(url.clone()));
        self.set_current_url(Some(&url));
        Ok(url)
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
}

impl Debug for KaspaRpcClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KaspaRpcClient").field("url", &self.url()).field("connected", &self.is_connected()).finish()
    }
}

impl KaspaRpcClient {
    /// Create a new `KaspaRpcClient` with the given Encoding and URL
    pub fn new(
        encoding: Encoding,
        url: Option<&str>,
        resolver: Option<Resolver>,
        network_id: Option<NetworkId>,
        subscription_context: Option<SubscriptionContext>,
    ) -> Result<KaspaRpcClient> {
        Self::new_with_args(encoding, url, resolver, network_id, subscription_context)
        // FIXME
        // pub fn new(encoding: Encoding, url: &str, ) -> Result<KaspaRpcClient> {
        //     Self::new_with_args(encoding, NotificationMode::Direct, url, subscription_context)
    }

    /// Extended constructor that accepts [`NotificationMode`] argument.
    pub fn new_with_args(
        encoding: Encoding,
        url: Option<&str>,
        resolver: Option<Resolver>,
        network_id: Option<NetworkId>,
        subscription_context: Option<SubscriptionContext>,
    ) -> Result<KaspaRpcClient> {
        let inner = Arc::new(Inner::new(encoding, url, resolver, network_id)?);
        inner.build_notifier(subscription_context)?;
        let client = KaspaRpcClient { inner };
        //     notification_mode: NotificationMode,
        //     url: &str,
        //     subscription_context: Option<SubscriptionContext>,
        // ) -> Result<KaspaRpcClient> {
        //     let inner = Arc::new(Inner::new(encoding, url)?);
        //     let notifier = if matches!(notification_mode, NotificationMode::MultiListeners) {
        //         let enabled_events = EVENT_TYPE_ARRAY[..].into();
        //         let converter = Arc::new(RpcCoreConverter::new());
        //         let collector = Arc::new(RpcCoreCollector::new(WRPC_CLIENT, inner.notification_channel_receiver(), converter));
        //         let subscriber = Arc::new(Subscriber::new(WRPC_CLIENT, enabled_events, inner.clone(), 0));
        //         let policies = MutationPolicies::new(UtxosChangedMutationPolicy::AddressSet);
        //         Some(Arc::new(Notifier::new(
        //             WRPC_CLIENT,
        //             enabled_events,
        //             vec![collector],
        //             vec![subscriber],
        //             subscription_context.unwrap_or_default(),
        //             3,
        //             policies,
        //         )))
        //     } else {
        //         None
        //     };

        // let client = KaspaRpcClient { inner, notifier, notification_mode };

        Ok(client)
    }

    async fn start_notifier(&self) -> Result<()> {
        let notifier = self.inner.build_notifier(None)?;
        notifier.start();
        Ok(())
    }

    async fn stop_notifier(&self) -> Result<()> {
        self.inner.reset_notification_intake_channel();
        self.notifier().join().await?;
        Ok(())
    }

    fn notifier(&self) -> RpcClientNotifier {
        self.inner.notifier.lock().unwrap().clone().expect("Rpc client is not correctly initialized")
    }

    pub fn url(&self) -> Option<String> {
        self.inner.current_url()
    }

    pub fn set_url(&self, url: Option<&str>) -> Result<()> {
        self.inner.set_default_url(url);
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.inner.rpc_client.is_connected()
    }

    pub fn encoding(&self) -> Encoding {
        self.inner.encoding
    }

    pub fn resolver(&self) -> Option<Resolver> {
        self.inner.resolver()
    }

    pub fn set_resolver(&self, resolver: Resolver) -> Result<()> {
        self.inner.resolver.lock().unwrap().replace(resolver);
        Ok(())
    }

    pub fn set_network_id(&self, network_id: &NetworkId) -> Result<()> {
        self.inner.network_id.lock().unwrap().replace(*network_id);
        Ok(())
    }

    pub fn node_descriptor(&self) -> Option<Arc<NodeDescriptor>> {
        self.inner.node_descriptor.lock().unwrap().clone()
    }

    pub fn rpc_client(&self) -> &Arc<RpcClient<RpcApiOps>> {
        &self.inner.rpc_client
    }

    pub fn rpc_api(self: &Arc<Self>) -> Arc<dyn RpcApi> {
        self.clone()
    }

    pub fn rpc_ctl(&self) -> &RpcCtl {
        &self.inner.rpc_ctl
    }

    /// Start background RPC services.
    pub async fn start(&self) -> Result<()> {
        if !self.inner.background_services_running.load(Ordering::SeqCst) {
            self.inner.background_services_running.store(true, Ordering::SeqCst);
            self.start_notifier().await?;
            self.start_rpc_ctl_service().await?;
        }

        Ok(())
    }

    /// Stop background RPC services.
    pub async fn stop(&self) -> Result<()> {
        if self.inner.background_services_running.load(Ordering::SeqCst) {
            self.stop_rpc_ctl_service().await?;
            self.stop_notifier().await?;
            self.inner.background_services_running.store(false, Ordering::SeqCst);
        }

        Ok(())
    }

    /// Starts a background async connection task connecting
    /// to the wRPC server.  If the supplied `block` call is `true`
    /// this function will block until the first successful
    /// connection.
    ///
    /// This method starts background RPC services if they are not running and
    /// attempts to connect to the RPC endpoint.
    pub async fn connect(&self, options: Option<ConnectOptions>) -> ConnectResult<Error> {
        let _guard = self.inner.connect_guard.lock().await;

        let mut options = options.unwrap_or_default();
        let strategy = options.strategy;

        if let Some(url) = options.url.take() {
            self.set_url(Some(&url))?;
        }

        // 1Gb message and frame size limits (on native and NodeJs platforms)
        let ws_config = WebSocketConfig {
            max_message_size: Some(1024 * 1024 * 1024),
            max_frame_size: Some(1024 * 1024 * 1024),
            accept_unmasked_frames: false,
            resolver: Some(self.inner.clone()),
            ..Default::default()
        };

        self.start().await?;
        self.inner.rpc_client.configure(ws_config);
        match self.inner.rpc_client.connect(options).await {
            Ok(v) => Ok(v),
            Err(err) => {
                if strategy == ConnectStrategy::Fallback {
                    let _guard = self.inner.disconnect_guard.lock().await;
                    self.inner.rpc_client.shutdown().await?;
                    self.stop().await?;
                }
                Err(err.into())
            }
        }
    }

    /// This method stops background RPC services and disconnects
    /// from the RPC endpoint.
    pub async fn disconnect(&self) -> Result<()> {
        let _guard = self.inner.disconnect_guard.lock().await;

        self.inner.rpc_client.shutdown().await?;
        self.stop().await?;
        Ok(())
    }

    // Stop and shutdown RPC disconnecting existing connections
    // and stopping reconnection process.
    // pub async fn shutdown(&self) -> Result<()> {
    //     Ok(self.inner.rpc_client.shutdown().await?)
    // }

    /// A helper function that is not `async`, allowing connection
    /// process to be initiated from non-async contexts.
    pub fn connect_as_task(&self) -> Result<()> {
        let self_ = self.clone();
        workflow_core::task::spawn(async move {
            self_.inner.rpc_client.connect(ConnectOptions::default()).await.ok();
        });
        Ok(())
    }

    pub fn notification_channel_receiver(&self) -> Receiver<Notification> {
        self.inner.notification_intake_channel.lock().unwrap().receiver.clone()
    }

    pub fn ctl(&self) -> &RpcCtl {
        &self.inner.rpc_ctl
    }

    pub fn parse_url_with_network_type(&self, url: String, network_type: NetworkType) -> Result<String> {
        Self::parse_url(url, self.inner.encoding, network_type)
    }

    pub fn parse_url(url: String, encoding: Encoding, network_type: NetworkType) -> Result<String> {
        let parse_output = parse_host(&url).map_err(|err| Error::Custom(err.to_string()))?;
        let scheme = parse_output
            .scheme
            .map(Ok)
            .unwrap_or_else(|| {
                if !application_runtime::is_web() {
                    return Ok("ws");
                }
                let location = window().location();
                let protocol =
                    location.protocol().map_err(|_| Error::UrlError("Unable to obtain window location protocol".to_string()))?;
                if protocol == "http:" || protocol == "chrome-extension:" {
                    Ok("ws")
                } else if protocol == "https:" {
                    Ok("wss")
                } else {
                    Err(Error::Custom(format!("Unsupported protocol: {}", protocol)))
                }
            })?
            .to_lowercase();
        let port = parse_output.port.unwrap_or_else(|| match encoding {
            WrpcEncoding::Borsh => network_type.default_borsh_rpc_port(),
            WrpcEncoding::SerdeJson => network_type.default_json_rpc_port(),
        });
        let path_str = parse_output.path;

        // Do not automatically include port if:
        //  1) the URL contains a scheme
        //  2) the URL contains a path
        //  3) explicitly specified in the URL,
        //
        //  This means wss://host.com or host.com/path will remain as-is
        //  while host.com or 1.2.3.4 will be converted to host.com:port
        //  or 1.2.3.4:port where port is based on the network type.
        //
        if (parse_output.scheme.is_some() || !path_str.is_empty()) && parse_output.port.is_none() {
            Ok(format!("{}://{}{}", scheme, parse_output.host, path_str))
        } else {
            Ok(format!("{}://{}:{}{}", scheme, parse_output.host, port, path_str))
        }
    }

    async fn start_rpc_ctl_service(&self) -> Result<()> {
        let inner = self.inner.clone();
        let wrpc_ctl_channel = inner.wrpc_ctl_multiplexer.channel();
        let notification_relay_channel = inner.notification_relay_channel.clone();
        spawn(async move {
            loop {
                select! {
                    _ = inner.service_ctl.request.receiver.recv().fuse() => {
                        break;
                    },
                    msg = notification_relay_channel.receiver.recv().fuse() => {
                        if let Ok(msg) = msg {
                            // inner.rpc_ctl.notify(msg).await.expect("(KaspaRpcClient) rpc_ctl.notify() error");
                            if let Err(err) = inner.notification_intake_channel.lock().unwrap().sender.try_send(msg) {
                                log_error!("notification_intake_channel.sender.try_send() error: {err}");
                            }
                        } else {
                            log_error!("notification_relay_channel receiver error");
                        }
                    }
                    msg = wrpc_ctl_channel.receiver.recv().fuse() => {
                        if let Ok(msg) = msg {
                            match msg {
                                WrpcCtl::Connect => {
                                    inner.rpc_ctl.signal_open().await.expect("(KaspaRpcClient) rpc_ctl.signal_open() error");
                                }
                                WrpcCtl::Disconnect => {
                                    inner.rpc_ctl.signal_close().await.expect("(KaspaRpcClient) rpc_ctl.signal_close() error");
                                }
                            }
                        } else {
                            log_error!("wrpc_ctl_channel.receiver.recv() error");
                        }
                    }
                }
            }
            inner.service_ctl.response.send(()).await.unwrap();
        });

        Ok(())
    }

    async fn stop_rpc_ctl_service(&self) -> Result<()> {
        self.inner.service_ctl.signal(()).await?;
        Ok(())
    }

    /// Triggers a disconnection on the underlying WebSocket.
    /// This is intended for debug purposes only.
    /// Can be used to test application reconnection logic.
    pub fn trigger_abort(&self) -> Result<()> {
        Ok(self.inner.rpc_client.trigger_abort()?)
    }
}

#[async_trait]
impl RpcApi for KaspaRpcClient {
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
            GetDaaScoreTimestampEstimate,
            GetServerInfo,
            GetCurrentNetwork,
            GetHeaders,
            GetInfo,
            GetMempoolEntries,
            GetMempoolEntriesByAddresses,
            GetMempoolEntry,
            GetPeerAddresses,
            GetMetrics,
            GetSink,
            GetSyncStatus,
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
        self.notifier().register_new_listener(connection, ListenerLifespan::Dynamic)
        // match self.notification_mode {
        //     NotificationMode::MultiListeners => {
        //         self.notifier.as_ref().unwrap().register_new_listener(connection, ListenerLifespan::Dynamic)
        //     }
        //     NotificationMode::Direct => ListenerId::default(),
        // }
    }

    /// Unregister an existing listener.
    ///
    /// Stop all notifications for this listener and drop its channel.
    async fn unregister_listener(&self, id: ListenerId) -> RpcResult<()> {
        self.notifier().unregister_listener(id)?;
        Ok(())
    }

    /// Start sending notifications of some type to a listener.
    async fn start_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        self.notifier().try_start_notify(id, scope)?;
        Ok(())
    }

    /// Stop sending notifications of some type to a listener.
    async fn stop_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        self.notifier().try_stop_notify(id, scope)?;
        Ok(())
    }
}
