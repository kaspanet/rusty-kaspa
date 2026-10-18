//! Kaspa wRPC client implementation.

use crate::imports::*;
use crate::parse::parse_host;
use crate::{error::Error, node::NodeDescriptor};
use kaspa_consensus_core::network::NetworkType;
use kaspa_notify::{
    listener::ListenerLifespan,
    subscription::{MutationPolicies, UtxosChangedMutationPolicy, context::SubscriptionContext},
};
use kaspa_rpc_core::{
    api::ctl::RpcCtl,
    notify::collector::{RpcCoreCollector, RpcCoreConverter},
};
pub use kaspa_rpc_macros::build_wrpc_client_interface;
use std::fmt::Debug;
use std::sync::Weak;
use workflow_core::{channel::Multiplexer, runtime as application_runtime};
use workflow_dom::utils::window;
use workflow_rpc::client::Ctl as WrpcCtl;
pub use workflow_rpc::client::{
    ConnectOptions, ConnectResult, ConnectStrategy, Resolver as RpcResolver, ResolverResult, WebSocketConfig, WebSocketError,
};
use workflow_serializer::prelude::*;
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
    // The permanent url passed in the constructor
    // (dominant, overrides Resolver if supplied).
    ctor_url: Mutex<Option<String>>,
    // The url passed in the connect() method
    // (overrides default URL and the Resolver).
    default_url: Mutex<Option<String>>,
    // The current url wRPC is connected to
    // (possibly acquired via the Resolver).
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
                workflow_rpc::client::Notification::new(move |notification: Serializable<kaspa_rpc_core::Notification>| {
                    let notification_sender = notification_sender_.clone();
                    Box::pin(async move {
                        // log_info!("notification receivers: {}", notification_sender.receiver_count());
                        // log_trace!("notification {:?}", notification);
                        if notification_sender.receiver_count() > 1 {
                            // log_info!("notification: posting to channel: {notification:?}");
                            notification_sender.send(notification.into_inner()).await?;
                        } else {
                            log_warn!("WARNING: Kaspa RPC notification is not consumed by user: {:?}", notification.into_inner());
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
            connect_guard: async_lock::Mutex::new(()),
            disconnect_guard: async_lock::Mutex::new(()),
            // ---
            ctor_url: Mutex::new(url.map(|s| s.to_string())),
            default_url: Mutex::new(None),
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
        let _response: Serializable<SubscribeResponse> =
            self.rpc_client.call(RpcApiOps::Subscribe, Serializable(scope)).await.map_err(|err| err.to_string())?;
        Ok(())
    }

    /// Stop sending notifications of some type to the client.
    async fn stop_notify_to_client(&self, scope: Scope) -> RpcResult<()> {
        let _response: Serializable<UnsubscribeResponse> =
            self.rpc_client.call(RpcApiOps::Unsubscribe, Serializable(scope)).await.map_err(|err| err.to_string())?;
        Ok(())
    }

    fn ctor_url(&self) -> Option<String> {
        self.ctor_url.lock().unwrap().clone()
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
        let url = if let Some(url) = self.default_url().or(self.ctor_url()) {
            url
        } else if let Some(resolver) = self.resolver().as_ref() {
            let network_id = self.network_id().expect("Resolver requires network id in RPC client configuration");
            let node = resolver.get_node(self.encoding, network_id).await.map_err(WebSocketError::custom)?;
            let url = node.url.clone();
            self.node_descriptor.lock().unwrap().replace(Arc::new(node));
            url
        } else {
            panic!("RpcClient resolver configuration error (expecting `url` or `resolver` as `Some(Resolver))`")
        };

        self.rpc_ctl.set_descriptor(Some(url.clone()));
        self.set_current_url(Some(&url));
        Ok(url)
    }
}

/// Resolves URLs through a *weak* reference to [`Inner`], so that handing the resolver to the
/// underlying WebSocket client does not create an `Inner -> rpc_client -> resolver -> Inner`
/// reference cycle that would keep the client state alive after the last handle is dropped.
struct ResolverProxy(Weak<Inner>);

#[async_trait]
impl RpcResolver for ResolverProxy {
    async fn resolve_url(&self) -> ResolverResult {
        match self.0.upgrade() {
            Some(inner) => inner.resolve_url().await,
            None => Err(WebSocketError::custom("KaspaRpcClient was dropped before its URL could be resolved")),
        }
    }
}

const WRPC_CLIENT: &str = "wrpc-client";

/// # [`KaspaRpcClient`] connects to Kaspa wRPC endpoint via binary Borsh or JSON protocols.
///
/// ## Resource Management
///
/// `KaspaRpcClient` manages background tasks and network connections.
/// For a clean shutdown, call [`disconnect()`](Self::disconnect) before dropping:
///
/// ```ignore
/// let client = KaspaRpcClient::new(...)?;
/// client.connect(None).await?;
/// // ... use client ...
/// client.disconnect().await?;
/// ```
///
/// If the last clone of a client is dropped while still connected, a warning is logged and a
/// best-effort background shutdown is performed; the internal state is released once that
/// completes. This cleanup is not guaranteed to finish if the async runtime exits immediately.
///
/// ## Architecture
///
/// RpcClient has two ways to interface with the underlying RPC subsystem:
/// [`Interface`] that has a [`notification()`](Interface::notification)
/// method to register closures that will be invoked on server-side
/// notifications and the [`RpcClient::call`] method that allows server-side
/// async method invocation.
///
/// The node address can be supplied via a URL or a [`Resolver`] that
/// can be used to resolve a public node address dynamically. [`Resolver`] can also
/// be configured to operate against custom node clusters.
///
/// Guard dropped exactly once, when the last user-held [`KaspaRpcClient`] clone is dropped.
///
/// Background tasks hold `Arc<Inner>` but never this guard, so its `Drop` is a reliable
/// "no more user handles" signal (unlike `Arc::strong_count`, which also counts internal
/// holders and is racy to act on). On drop it breaks the internal reference cycles and, if
/// the client is still connected, performs a best-effort asynchronous shutdown so that
/// `Inner` can actually be freed.
struct ClientGuard {
    inner: Arc<Inner>,
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        // Break the `Inner -> Notifier -> Subscriber -> Arc<Inner>` cycle by taking the notifier
        // out of `Inner`. It is kept so it can be joined below if it is still running.
        let notifier = self.inner.notifier.lock().ok().and_then(|mut notifier| notifier.take());

        if self.inner.background_services_running.load(Ordering::SeqCst) {
            log_warn!("KaspaRpcClient dropped while still connected. Call disconnect() before dropping for clean shutdown.");

            // Best-effort asynchronous shutdown, in the same order as `disconnect()` + `stop()`.
            // The task holds `Inner` only until it completes; once it returns nothing references
            // `Inner` anymore and it is freed.
            //
            // The notification channels must not be closed up front: the rpc ctl service task
            // keeps polling `notification_relay_channel.receiver` and only exits on the
            // `service_ctl` signal, so a closed channel would turn its loop into a busy loop
            // that never yields.
            let inner = self.inner.clone();
            spawn(async move {
                let _ = inner.rpc_client.shutdown().await;
                // Stops the rpc ctl service task, which releases its own `Arc<Inner>`.
                let _ = inner.service_ctl.signal(()).await;
                // Closes the old intake channel so the notifier's collector can exit, then joins
                // the notifier tasks (a no-op if it was never started).
                inner.reset_notification_intake_channel();
                if let Some(notifier) = notifier {
                    let _ = notifier.join().await;
                }
                inner.background_services_running.store(false, Ordering::SeqCst);
            });
        }
        // Otherwise the client was never connected or was already disconnected (services stopped
        // and the notifier already joined), so dropping the taken notifier is sufficient.
    }
}

/// A weak handle to a [`KaspaRpcClient`]'s internal state, for tests that need to verify the
/// state is actually freed once the last client handle is dropped.
#[cfg(feature = "test-util")]
pub struct WeakKaspaRpcClient(Weak<Inner>);

#[cfg(feature = "test-util")]
impl WeakKaspaRpcClient {
    /// Number of strong references to the internal client state (0 once it has been freed).
    pub fn strong_count(&self) -> usize {
        self.0.strong_count()
    }
}

#[cfg(feature = "test-util")]
impl KaspaRpcClient {
    /// Returns a weak handle to the internal client state. Test utility only.
    pub fn weak_clone(&self) -> WeakKaspaRpcClient {
        WeakKaspaRpcClient(Arc::downgrade(&self.inner))
    }
}

#[derive(Clone)]
pub struct KaspaRpcClient {
    inner: Arc<Inner>,
    // Dropped only when the last clone is dropped; see [`ClientGuard`].
    _guard: Arc<ClientGuard>,
}

impl Debug for KaspaRpcClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KaspaRpcClient").field("url", &self.url()).field("connected", &self.is_connected()).finish()
    }
}

impl KaspaRpcClient {
    /// Create a new `KaspaRpcClient` with the given Encoding, and an optional url or a Resolver.
    /// Please note that if you pass the url to the constructor, it will force the KaspaRpcClient
    /// to always use this url.  If you want to have the ability to switch between urls,
    /// you must pass [`Option::None`] as the `url` argument and then supply your own url to the `connect()`
    /// function each time you connect.
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
        let _guard = Arc::new(ClientGuard { inner: inner.clone() });
        let client = KaspaRpcClient { inner, _guard };
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

    pub fn network_id(&self) -> Option<NetworkId> {
        *self.inner.network_id.lock().unwrap()
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

    pub fn ctl_multiplexer(&self) -> Multiplexer<WrpcCtl> {
        self.inner.wrpc_ctl_multiplexer.clone()
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
        // this has no effect if not currently connected
        self.disconnect().await?;

        let _guard = self.inner.connect_guard.lock().await;

        let options = options.unwrap_or_default();
        let strategy = options.strategy;

        self.inner.set_default_url(options.url.as_deref());
        self.inner.rpc_ctl.set_descriptor(options.url.clone());

        // current url is populated by a an implemented trait when not forced in options
        if let Some(ref url) = options.url {
            self.inner.set_current_url(Some(url));
        }

        // 1Gb message and frame size limits (on native and NodeJs platforms)
        let ws_config = WebSocketConfig {
            max_message_size: Some(1024 * 1024 * 1024),
            max_frame_size: Some(1024 * 1024 * 1024),
            accept_unmasked_frames: false,
            resolver: Some(Arc::new(ResolverProxy(Arc::downgrade(&self.inner)))),
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
            Ping,
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
            GetCurrentBlockColor,
            GetBlockRewardInfo,
            GetCoinSupply,
            GetConnectedPeerInfo,
            GetConnections,
            GetCurrentNetwork,
            GetDaaScoreTimestampEstimate,
            GetFeeEstimate,
            GetFeeEstimateExperimental,
            GetHeaders,
            GetInfo,
            GetMempoolEntries,
            GetMempoolEntriesByAddresses,
            GetMempoolEntry,
            GetMetrics,
            GetPeerAddresses,
            GetServerInfo,
            GetSink,
            GetSinkBlueScore,
            GetSubnetwork,
            GetSyncStatus,
            GetSystemInfo,
            GetUtxoReturnAddress,
            GetUtxosByAddresses,
            GetUtxosByAddressesV2,
            GetVirtualChainFromBlock,
            GetVirtualChainFromBlockV2,
            ResolveFinalityConflict,
            Shutdown,
            SubmitBlock,
            SubmitTransaction,
            SubmitTransactionReplacement,
            Unban,
            GetSeqCommitLaneProof,
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

#[cfg(test)]
mod drop_tests {
    use super::*;

    #[test]
    fn inner_state_is_freed_when_last_handle_is_dropped_without_connecting() {
        // A freshly constructed client already holds the `Inner -> Notifier -> Subscriber -> Inner`
        // cycle; dropping the last handle must break it so `Inner` is actually freed.
        let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some("ws://127.0.0.1:1"), None, None, None).unwrap();
        let weak = Arc::downgrade(&client.inner);
        let clone = client.clone();

        // Dropping a non-last clone must not tear anything down.
        drop(client);
        assert!(weak.strong_count() > 0, "a remaining clone must keep the client state alive");

        drop(clone);
        assert_eq!(weak.strong_count(), 0, "client state must be freed once the last handle is dropped");
    }
}
