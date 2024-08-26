use self::{
    error::{Error, Result},
    resolver::{id::IdResolver, queue::QueueResolver, DynResolver},
};
use async_channel::{Receiver, Sender};
use async_trait::async_trait;
pub use client_pool::ClientPool;
use connection_event::ConnectionEvent;
use futures::{future::FutureExt, pin_mut, select};
use kaspa_core::{debug, error, trace};
use kaspa_grpc_core::{
    channel::NotificationChannel,
    ops::KaspadPayloadOps,
    protowire::{kaspad_request, rpc_client::RpcClient, GetInfoRequestMessage, KaspadRequest, KaspadResponse},
    RPC_MAX_MESSAGE_SIZE,
};
use kaspa_notify::{
    collector::{Collector, CollectorFrom},
    error::{Error as NotifyError, Result as NotifyResult},
    events::{EventArray, EventType, EVENT_TYPE_ARRAY},
    listener::{ListenerId, ListenerLifespan},
    notifier::{DynNotify, Notifier},
    scope::Scope,
    subscriber::{Subscriber, SubscriptionManager},
    subscription::{
        array::ArrayBuilder, context::SubscriptionContext, Command, DynSubscription, MutateSingle, Mutation, MutationPolicies,
        UtxosChangedMutationPolicy,
    },
};
use kaspa_rpc_core::{
    api::rpc::RpcApi,
    error::RpcError,
    error::RpcResult,
    model::message::*,
    notify::{collector::RpcCoreConverter, connection::ChannelConnection, mode::NotificationMode},
    Notification,
};
use kaspa_utils::{channel::Channel, triggers::DuplexTrigger};
use kaspa_utils_tower::{
    counters::TowerConnectionCounters,
    middleware::{measure_request_body_size_layer, CountBytesBody, MapResponseBodyLayer, ServiceBuilder},
};
use regex::Regex;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::Mutex;
use tonic::codec::CompressionEncoding;
use tonic::codegen::Body;
use tonic::Streaming;

mod connection_event;
pub mod error;
mod resolver;
#[macro_use]
mod route;

mod client_pool;

pub type GrpcClientCollector = CollectorFrom<RpcCoreConverter>;
pub type GrpcClientNotify = DynNotify<Notification>;
pub type GrpcClientNotifier = Notifier<Notification, ChannelConnection>;

type DirectSubscriptions = Mutex<EventArray<DynSubscription>>;

#[derive(Debug, Clone)]
pub struct GrpcClient {
    inner: Arc<Inner>,
    /// In multi listener mode, a full-featured Notifier
    notifier: Option<Arc<GrpcClientNotifier>>,
    /// In direct mode, a Collector relaying incoming notifications via a channel (see `self.notification_channel_receiver()`)
    collector: Option<Arc<GrpcClientCollector>>,
    subscriptions: Option<Arc<DirectSubscriptions>>,
    subscription_context: SubscriptionContext,
    policies: MutationPolicies,
    notification_mode: NotificationMode,
}

const GRPC_CLIENT: &str = "grpc-client";

impl GrpcClient {
    pub const DIRECT_MODE_LISTENER_ID: ListenerId = 0;

    pub async fn connect(url: String) -> Result<GrpcClient> {
        Self::connect_with_args(NotificationMode::Direct, url, None, false, None, false, None, Default::default()).await
    }

    /// Connects to a gRPC server.
    ///
    /// `notification_mode` determines how notifications are handled:
    ///
    /// - `MultiListeners` => Multiple listeners are supported via the [`RpcApi`] implementation.
    ///                       Registering listeners is needed before subscribing to notifications.
    /// - `Direct` => A single listener receives the notification via a channel (see  `self.notification_channel_receiver()`).
    ///               Registering a listener is pointless and ignored.
    ///               Subscribing to notifications ignores the listener ID.
    ///
    /// `url`: the server to connect to
    ///
    /// `subscription_context`: it is advised to provide a clone of the same instance if multiple clients dealing with
    /// [`UtxosChangedNotifications`] are connected concurrently in order to optimize the memory footprint.
    ///
    /// `reconnect`: features an automatic reconnection to the server, reactivating all subscriptions on success.
    ///
    /// `connection_event_sender`: when provided will notify of connection and disconnection events via the channel.
    ///
    /// `override_handle_stop_notify`: legacy, should be removed in near future, always set to `false`.
    ///
    /// `timeout_duration`: request timeout duration
    ///
    /// `counters`: collects some bandwidth metrics
    pub async fn connect_with_args(
        notification_mode: NotificationMode,
        url: String,
        subscription_context: Option<SubscriptionContext>,
        reconnect: bool,
        connection_event_sender: Option<Sender<ConnectionEvent>>,
        override_handle_stop_notify: bool,
        timeout_duration: Option<u64>,
        counters: Arc<TowerConnectionCounters>,
    ) -> Result<GrpcClient> {
        let schema = Regex::new(r"^grpc://").unwrap();
        if !schema.is_match(&url) {
            return Err(Error::GrpcAddressSchema(url));
        }
        let inner = Inner::connect(
            url,
            connection_event_sender,
            override_handle_stop_notify,
            timeout_duration.unwrap_or(REQUEST_TIMEOUT_DURATION),
            counters,
        )
        .await?;
        let converter = Arc::new(RpcCoreConverter::new());
        let policies = MutationPolicies::new(UtxosChangedMutationPolicy::AddressSet);
        let subscription_context = subscription_context.unwrap_or_default();
        let (notifier, collector, subscriptions) = match notification_mode {
            NotificationMode::MultiListeners => {
                let enabled_events = EVENT_TYPE_ARRAY[..].into();
                let collector = Arc::new(GrpcClientCollector::new(GRPC_CLIENT, inner.notification_channel_receiver(), converter));
                let subscriber = Arc::new(Subscriber::new(GRPC_CLIENT, enabled_events, inner.clone(), 0));
                let notifier: GrpcClientNotifier = Notifier::new(
                    GRPC_CLIENT,
                    enabled_events,
                    vec![collector],
                    vec![subscriber],
                    subscription_context.clone(),
                    3,
                    policies,
                );
                (Some(Arc::new(notifier)), None, None)
            }
            NotificationMode::Direct => {
                let collector = GrpcClientCollector::new(GRPC_CLIENT, inner.notification_channel_receiver(), converter);
                let subscriptions = ArrayBuilder::single(Self::DIRECT_MODE_LISTENER_ID, None);
                (None, Some(Arc::new(collector)), Some(Arc::new(Mutex::new(subscriptions))))
            }
        };

        if reconnect {
            // Start the connection monitor
            inner.clone().spawn_connection_monitor(notifier.clone(), subscriptions.clone(), subscription_context.clone());
        }

        Ok(Self { inner, notifier, collector, subscriptions, subscription_context, policies, notification_mode })
    }

    #[inline(always)]
    pub fn notifier(&self) -> Option<Arc<GrpcClientNotifier>> {
        self.notifier.clone()
    }

    /// Starts RPC services.
    pub async fn start(&self, notify: Option<GrpcClientNotify>) {
        match &self.notification_mode {
            NotificationMode::MultiListeners => {
                assert!(notify.is_none(), "client is on multi-listeners mode");
                self.notifier.clone().unwrap().start();
            }
            NotificationMode::Direct => {
                if let Some(notify) = notify {
                    self.collector.as_ref().unwrap().clone().start(notify);
                }
            }
        }
    }

    /// Joins on RPC services.
    pub async fn join(&self) -> Result<()> {
        match &self.notification_mode {
            NotificationMode::MultiListeners => {
                self.notifier.as_ref().unwrap().join().await?;
            }
            NotificationMode::Direct => {
                if self.collector.as_ref().unwrap().is_started() {
                    self.collector.as_ref().unwrap().clone().join().await?;
                }
            }
        }
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    pub fn handle_message_id(&self) -> bool {
        self.inner.handle_message_id()
    }

    pub fn handle_stop_notify(&self) -> bool {
        self.inner.handle_stop_notify()
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.inner.disconnect().await?;
        Ok(())
    }

    pub fn notification_channel_receiver(&self) -> Receiver<Notification> {
        self.inner.notification_channel.receiver()
    }

    pub fn notification_mode(&self) -> NotificationMode {
        self.notification_mode
    }
}

#[async_trait]
impl RpcApi for GrpcClient {
    // this example illustrates the body of the function created by the route!() macro
    // async fn submit_block_call(&self, request: SubmitBlockRequest) -> RpcResult<SubmitBlockResponse> {
    //     self.inner.call(KaspadPayloadOps::SubmitBlock, request).await?.as_ref().try_into()
    // }

    route!(ping_call, Ping);
    route!(get_sync_status_call, GetSyncStatus);
    route!(get_server_info_call, GetServerInfo);
    route!(get_metrics_call, GetMetrics);
    route!(get_connections_call, GetConnections);
    route!(get_system_info_call, GetSystemInfo);
    route!(submit_block_call, SubmitBlock);
    route!(get_block_template_call, GetBlockTemplate);
    route!(get_block_call, GetBlock);
    route!(get_info_call, GetInfo);
    route!(get_current_network_call, GetCurrentNetwork);
    route!(get_peer_addresses_call, GetPeerAddresses);
    route!(get_sink_call, GetSink);
    route!(get_mempool_entry_call, GetMempoolEntry);
    route!(get_mempool_entries_call, GetMempoolEntries);
    route!(get_connected_peer_info_call, GetConnectedPeerInfo);
    route!(add_peer_call, AddPeer);
    route!(submit_transaction_call, SubmitTransaction);
    route!(submit_transaction_replacement_call, SubmitTransactionReplacement);
    route!(get_subnetwork_call, GetSubnetwork);
    route!(get_virtual_chain_from_block_call, GetVirtualChainFromBlock);
    route!(get_blocks_call, GetBlocks);
    route!(get_block_count_call, GetBlockCount);
    route!(get_block_dag_info_call, GetBlockDagInfo);
    route!(resolve_finality_conflict_call, ResolveFinalityConflict);
    route!(shutdown_call, Shutdown);
    route!(get_headers_call, GetHeaders);
    route!(get_utxos_by_addresses_call, GetUtxosByAddresses);
    route!(get_balance_by_address_call, GetBalanceByAddress);
    route!(get_balances_by_addresses_call, GetBalancesByAddresses);
    route!(get_sink_blue_score_call, GetSinkBlueScore);
    route!(ban_call, Ban);
    route!(unban_call, Unban);
    route!(estimate_network_hashes_per_second_call, EstimateNetworkHashesPerSecond);
    route!(get_mempool_entries_by_addresses_call, GetMempoolEntriesByAddresses);
    route!(get_coin_supply_call, GetCoinSupply);
    route!(get_daa_score_timestamp_estimate_call, GetDaaScoreTimestampEstimate);
    route!(get_fee_estimate_call, GetFeeEstimate);
    route!(get_fee_estimate_experimental_call, GetFeeEstimateExperimental);
    route!(get_current_block_color_call, GetCurrentBlockColor);

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    /// Register a new listener and returns an id identifying it.
    fn register_new_listener(&self, connection: ChannelConnection) -> ListenerId {
        match self.notification_mode {
            NotificationMode::MultiListeners => {
                self.notifier.as_ref().unwrap().register_new_listener(connection, ListenerLifespan::Dynamic)
            }
            // In direct mode, listener registration/unregistration is ignored
            NotificationMode::Direct => Self::DIRECT_MODE_LISTENER_ID,
        }
    }

    /// Unregister an existing listener.
    ///
    /// Stop all notifications for this listener, unregister the id and its associated connection.
    async fn unregister_listener(&self, id: ListenerId) -> RpcResult<()> {
        match self.notification_mode {
            NotificationMode::MultiListeners => {
                self.notifier.as_ref().unwrap().unregister_listener(id)?;
            }
            // In direct mode, listener registration/unregistration is ignored
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
                if self.inner.will_reconnect() {
                    let event = scope.event_type();
                    self.subscriptions.as_ref().unwrap().lock().await[event].mutate(
                        Mutation::new(Command::Start, scope.clone()),
                        self.policies,
                        &self.subscription_context,
                    )?;
                }
                self.inner.start_notify_to_client(scope).await?;
            }
        }
        Ok(())
    }

    /// Stop sending notifications of some type to a listener.
    async fn stop_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        if self.handle_stop_notify() {
            match self.notification_mode {
                NotificationMode::MultiListeners => {
                    self.notifier.clone().unwrap().try_stop_notify(id, scope)?;
                }
                NotificationMode::Direct => {
                    if self.inner.will_reconnect() {
                        let event = scope.event_type();
                        self.subscriptions.as_ref().unwrap().lock().await[event].mutate(
                            Mutation::new(Command::Stop, scope.clone()),
                            self.policies,
                            &self.subscription_context,
                        )?;
                    }
                    self.inner.stop_notify_to_client(scope).await?;
                }
            }
            Ok(())
        } else {
            Err(RpcError::UnsupportedFeature)
        }
    }
}

pub const CONNECT_TIMEOUT_DURATION: u64 = 20_000;
pub const REQUEST_TIMEOUT_DURATION: u64 = 5_000;
pub const TIMEOUT_MONITORING_INTERVAL: u64 = 10_000;
pub const RECONNECT_INTERVAL: u64 = 2_000;

type KaspadRequestSender = async_channel::Sender<KaspadRequest>;
type KaspadRequestReceiver = async_channel::Receiver<KaspadRequest>;

#[derive(Debug, Default)]
struct ServerFeatures {
    pub handle_stop_notify: bool,
    pub handle_message_id: bool,
}

/// A struct to handle messages flowing to (requests) and from (responses) a protowire server.
/// Incoming responses are associated to pending requests based on their matching operation
/// type and, for some operations like [`ClientApiOps::GetBlock`], on their properties.
///
/// Data flow:
/// ```
/// //   KaspadRequest -> request_send -> stream -> KaspadResponse
/// ```
///
/// Execution flow:
/// ```
/// // | call ---------------------------------------------------->|
/// //                                  | response_receiver_task ->|
/// ```
///
///
/// #### Further development
///
/// TODO:
///
/// Carry any subscribe call result up to the initial GrpcClient::start_notify execution.
/// For now, GrpcClient::start_notify only gets a result reflecting the call to
/// Notifier::try_send_dispatch. This is not complete.
///
/// Investigate a possible bottleneck in handle_response with the processing of pendings.
/// If this is the case, some concurrent alternative should be considered.
///
/// Design/flow:
///
/// Currently call is blocking until response_receiver_task or timeout_task do solve the pending.
/// So actual concurrency must happen higher in the code.
/// Is there a better way to handle the flow?
///
#[derive(Debug)]
struct Inner {
    url: String,

    server_features: ServerFeatures,

    // Pushing incoming notifications forward
    notification_channel: NotificationChannel,

    // Sending to server
    request_sender: KaspadRequestSender,
    request_receiver: KaspadRequestReceiver,

    // Receiving from server
    receiver_is_running: AtomicBool,
    receiver_shutdown: DuplexTrigger,

    /// Matching responses with pending requests
    resolver: DynResolver,

    // Pending timeout cleaning task
    timeout_is_running: AtomicBool,
    timeout_shutdown: DuplexTrigger,
    timeout_timer_interval: u64,
    timeout_duration: u64,

    // Connection monitor allowing to reconnect automatically to the server
    connector_is_running: AtomicBool,
    connector_shutdown: DuplexTrigger,
    connector_timer_interval: u64,

    // Connection event channel
    connection_event_sender: Option<Sender<ConnectionEvent>>,

    // temporary hack to override the handle_stop_notify flag
    override_handle_stop_notify: bool,

    // bandwidth counters
    counters: Arc<TowerConnectionCounters>,
}

impl Inner {
    fn new(
        url: String,
        server_features: ServerFeatures,
        request_sender: KaspadRequestSender,
        request_receiver: KaspadRequestReceiver,
        connection_event_sender: Option<Sender<ConnectionEvent>>,
        override_handle_stop_notify: bool,
        timeout_duration: u64,
        counters: Arc<TowerConnectionCounters>,
    ) -> Self {
        let resolver: DynResolver = match server_features.handle_message_id {
            true => Arc::new(IdResolver::new()),
            false => Arc::new(QueueResolver::new()),
        };
        let notification_channel = Channel::default();
        Self {
            url,
            server_features,
            notification_channel,
            request_sender,
            request_receiver,
            resolver,
            receiver_is_running: AtomicBool::new(false),
            receiver_shutdown: DuplexTrigger::new(),
            timeout_is_running: AtomicBool::new(false),
            timeout_shutdown: DuplexTrigger::new(),
            timeout_duration,
            timeout_timer_interval: TIMEOUT_MONITORING_INTERVAL,
            connector_is_running: AtomicBool::new(false),
            connector_shutdown: DuplexTrigger::new(),
            connector_timer_interval: RECONNECT_INTERVAL,
            connection_event_sender,
            override_handle_stop_notify,
            counters,
        }
    }

    // TODO - remove the override (discuss how to handle this in relation to the golang client)
    async fn connect(
        url: String,
        connection_event_sender: Option<Sender<ConnectionEvent>>,
        override_handle_stop_notify: bool,
        timeout_duration: u64,
        counters: Arc<TowerConnectionCounters>,
    ) -> Result<Arc<Self>> {
        // Request channel
        let (request_sender, request_receiver) = async_channel::unbounded();

        // Try to connect to the server
        let (stream, server_features) =
            Inner::try_connect(url.clone(), request_sender.clone(), request_receiver.clone(), timeout_duration, counters.clone())
                .await?;

        // create the inner object
        let inner = Arc::new(Inner::new(
            url,
            server_features,
            request_sender,
            request_receiver,
            connection_event_sender,
            override_handle_stop_notify,
            timeout_duration,
            counters,
        ));

        // Start the request timeout cleaner
        inner.clone().spawn_request_timeout_monitor();

        // Start the response receiving task
        inner.clone().spawn_response_receiver_task(stream);

        trace!("GRPC client: connected");
        Ok(inner)
    }

    #[allow(unused_variables)]
    async fn try_connect(
        url: String,
        request_sender: KaspadRequestSender,
        request_receiver: KaspadRequestReceiver,
        request_timeout: u64,
        counters: Arc<TowerConnectionCounters>,
    ) -> Result<(Streaming<KaspadResponse>, ServerFeatures)> {
        // gRPC endpoint
        #[cfg(not(feature = "heap"))]
        let channel =
            tonic::transport::Channel::builder(url.parse::<tonic::transport::Uri>().map_err(|e| Error::String(e.to_string()))?)
                .timeout(tokio::time::Duration::from_millis(request_timeout))
                .connect_timeout(tokio::time::Duration::from_millis(CONNECT_TIMEOUT_DURATION))
                .connect()
                .await?;

        #[cfg(feature = "heap")]
        let channel =
            tonic::transport::Channel::builder(url.parse::<tonic::transport::Uri>().map_err(|e| Error::String(e.to_string()))?)
                .connect()
                .await?;

        let bytes_rx = &counters.bytes_rx;
        let bytes_tx = &counters.bytes_tx;
        let channel = ServiceBuilder::new()
            .layer(MapResponseBodyLayer::new(move |body| CountBytesBody::new(body, bytes_rx.clone())))
            .layer(measure_request_body_size_layer(bytes_tx.clone(), |body| {
                body.map_err(|e| tonic::Status::from_error(Box::new(e))).boxed_unsync()
            }))
            .service(channel);

        // Build the gRPC client with an interceptor setting the request timeout
        #[cfg(not(feature = "heap"))]
        let request_timeout = tokio::time::Duration::from_millis(request_timeout);
        #[cfg(not(feature = "heap"))]
        let mut client = RpcClient::with_interceptor(channel, move |mut req: tonic::Request<()>| {
            req.set_timeout(request_timeout);
            Ok(req)
        });

        #[cfg(feature = "heap")]
        let mut client = RpcClient::new(channel);

        client = client
            .send_compressed(CompressionEncoding::Gzip)
            .accept_compressed(CompressionEncoding::Gzip)
            .max_decoding_message_size(RPC_MAX_MESSAGE_SIZE);

        // Prepare a request receiver stream
        let stream_receiver = request_receiver.clone();
        let request_stream = async_stream::stream! {
            while let Ok(item) = stream_receiver.recv().await {
                yield item;
            }
        };

        // Actual KaspadRequest to KaspadResponse stream
        let mut stream: Streaming<KaspadResponse> = client.message_stream(request_stream).await?.into_inner();

        // Collect server capabilities as stated in GetInfoResponse
        let mut server_features = ServerFeatures::default();
        request_sender.send(GetInfoRequestMessage {}.into()).await?;
        match stream.message().await? {
            Some(ref msg) => {
                trace!("GRPC client: try_connect - GetInfo got a response");
                let response: RpcResult<GetInfoResponse> = msg.try_into();
                if let Ok(response) = response {
                    server_features.handle_stop_notify = response.has_notify_command;
                    server_features.handle_message_id = response.has_message_id;
                }
            }
            None => {
                debug!("GRPC client: try_connect - stream closed by the server");
                return Err(Error::String("GRPC stream was closed by the server".to_string()));
            }
        }

        Ok((stream, server_features))
    }

    async fn reconnect(
        self: Arc<Self>,
        notifier: Option<Arc<GrpcClientNotifier>>,
        subscriptions: Option<Arc<DirectSubscriptions>>,
        subscription_context: &SubscriptionContext,
    ) -> RpcResult<()> {
        assert_ne!(
            notifier.is_some(),
            subscriptions.is_some(),
            "exclusively either a notifier in MultiListener mode or subscriptions in Direct mode"
        );
        // TODO: verify if server feature have changed since first connection

        // Try to connect to the server
        let (stream, _) = Inner::try_connect(
            self.url.clone(),
            self.request_sender.clone(),
            self.request_receiver.clone(),
            self.timeout_duration,
            self.counters.clone(),
        )
        .await?;

        // Start the response receiving task
        self.clone().spawn_response_receiver_task(stream);

        // Re-register the compounded subscription state of the notifier in MultiListener mode
        if let Some(notifier) = notifier.as_ref() {
            notifier.try_renew_subscriptions()?;
        }

        // Re-register the subscriptions state in Direct mode
        if let Some(subscriptions) = subscriptions.as_ref() {
            let subscriptions = subscriptions.lock().await;
            for event in EVENT_TYPE_ARRAY {
                if subscriptions[event].active() {
                    self.clone().start_notify_to_client(subscriptions[event].scope(subscription_context)).await?;
                }
            }
        }

        debug!("GRPC client: reconnected");
        Ok(())
    }

    pub fn notification_channel_receiver(&self) -> Receiver<Notification> {
        self.notification_channel.receiver()
    }

    fn send_connection_event(&self, event: ConnectionEvent) {
        if let Some(ref connection_event_sender) = self.connection_event_sender {
            if let Err(err) = connection_event_sender.try_send(event) {
                debug!("Send connection event error: {err}");
            }
        }
    }

    fn is_connected(&self) -> bool {
        self.receiver_is_running.load(Ordering::SeqCst)
    }

    fn will_reconnect(&self) -> bool {
        self.connector_is_running.load(Ordering::SeqCst)
    }

    #[inline(always)]
    fn handle_message_id(&self) -> bool {
        self.server_features.handle_message_id
    }

    #[inline(always)]
    fn handle_stop_notify(&self) -> bool {
        // TODO - remove this
        if self.override_handle_stop_notify {
            true
        } else {
            self.server_features.handle_stop_notify
        }
    }

    #[inline(always)]
    fn resolver(&self) -> DynResolver {
        self.resolver.clone()
    }

    async fn call(&self, op: KaspadPayloadOps, request: impl Into<KaspadRequest>) -> Result<KaspadResponse> {
        // Calls are only allowed if the client is connected to the server
        if self.is_connected() {
            let id = u64::from_le_bytes(rand::random::<[u8; 8]>());
            let mut request: KaspadRequest = request.into();
            request.id = id;

            trace!("GRPC client: resolver call: {:?}", request);
            if request.payload.is_some() {
                let receiver = self.resolver().register_request(op, &request);
                self.request_sender.send(request).await.map_err(|_| Error::ChannelRecvError)?;
                receiver.await?
            } else {
                Err(Error::MissingRequestPayload)
            }
        } else {
            Err(Error::NotConnected)
        }
    }

    /// Launch a task that periodically checks pending requests and deletes those that have
    /// waited longer than a predefined delay.
    fn spawn_request_timeout_monitor(self: Arc<Self>) {
        // Note: self is a cloned Arc here so that it can be used in the spawned task.

        // The task can only be spawned once
        if self.timeout_is_running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            trace!("GRPC client: timeout task - spawn request ignored since already spawned");
            return;
        }

        tokio::spawn(async move {
            trace!("GRPC client: timeout task - started");
            let shutdown = self.timeout_shutdown.request.listener.clone().fuse();
            pin_mut!(shutdown);

            loop {
                let timeout_timer_interval = Duration::from_millis(self.timeout_timer_interval);
                let delay = tokio::time::sleep(timeout_timer_interval).fuse();
                pin_mut!(delay);

                select! {
                    _ = shutdown => { break; },
                    _ = delay => {
                        trace!("GRPC client: timeout task - running");
                        let timeout = Duration::from_millis(self.timeout_duration);
                        self.resolver().remove_expired_requests(timeout);
                    },
                }
            }
            self.timeout_is_running.store(false, Ordering::SeqCst);
            self.timeout_shutdown.response.trigger.trigger();

            trace!("GRPC client: timeout task - terminated");
        });
    }

    /// Launch a task receiving and handling response messages sent by the server.
    fn spawn_response_receiver_task(self: Arc<Self>, mut stream: Streaming<KaspadResponse>) {
        // Note: self is a cloned Arc here so that it can be used in the spawned task.

        // The task can only be spawned once
        if self.receiver_is_running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            trace!("GRPC client: response receiver task - spawn ignored since already spawned");
            return;
        }

        // Send connection event
        self.send_connection_event(ConnectionEvent::Connected);

        tokio::spawn(async move {
            trace!("GRPC client: response receiver task - started");
            loop {
                let shutdown = self.receiver_shutdown.request.listener.clone();
                pin_mut!(shutdown);

                tokio::select! {
                    biased;

                    _ = shutdown => {
                        break;
                    }

                    message = stream.message() => {
                        match message {
                            Ok(msg) => {
                                match msg {
                                    Some(response) => {
                                        self.handle_response(response);
                                    },
                                    None =>{
                                        debug!("GRPC client: response receiver task - the connection to the server is closed");

                                        // A reconnection is needed
                                        break;
                                    }
                                }
                            },
                            Err(err) => {
                                debug!("GRPC client: response receiver task - the response receiver gets an error from the server: {:?}", err);

                                // TODO: ignore cases not requiring a reconnection

                                // A reconnection is needed
                                break;
                            }
                        }
                    }
                }
            }
            // Mark as not connected
            self.receiver_is_running.store(false, Ordering::SeqCst);
            self.send_connection_event(ConnectionEvent::Disconnected);

            // Close the notification channel so that notifiers/collectors/subscribers can be joined on
            if !self.will_reconnect() {
                self.notification_channel.close();
            }

            if self.receiver_shutdown.request.listener.is_triggered() {
                self.receiver_shutdown.response.trigger.trigger();
            }

            trace!("GRPC client: response receiver task - terminated");
        });
    }

    /// Launch a task that periodically checks if the connection to the server is alive
    /// and if not that tries to reconnect to the server.
    fn spawn_connection_monitor(
        self: Arc<Self>,
        notifier: Option<Arc<GrpcClientNotifier>>,
        subscriptions: Option<Arc<DirectSubscriptions>>,
        subscription_context: SubscriptionContext,
    ) {
        // Note: self is a cloned Arc here so that it can be used in the spawned task.

        // The task can only be spawned once
        if self.connector_is_running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            trace!("GRPC client: connection monitor task - spawn ignored since already spawned");
            return;
        }

        tokio::spawn(async move {
            trace!("GRPC client: connection monitor task - started");
            let shutdown = self.connector_shutdown.request.listener.clone().fuse();
            pin_mut!(shutdown);
            loop {
                let connector_timer_interval = Duration::from_millis(self.connector_timer_interval);
                let delay = tokio::time::sleep(connector_timer_interval).fuse();
                pin_mut!(delay);
                select! {
                    _ = shutdown => { break; },
                    _ = delay => {
                        trace!("GRPC client: connection monitor task - running");
                        if !self.is_connected() {
                            match self.clone().reconnect(notifier.clone(), subscriptions.clone(), &subscription_context).await {
                                Ok(_) => {
                                    trace!("GRPC client: reconnection to server succeeded");
                                },
                                Err(err) => {
                                    trace!("GRPC client: reconnection to server failed with error {err:?}");
                                }
                            }
                        }
                    },
                }
            }
            self.connector_is_running.store(false, Ordering::SeqCst);
            self.connector_shutdown.response.trigger.trigger();
            trace!("GRPC client: connection monitor task - terminating");
        });
    }

    fn handle_response(&self, response: KaspadResponse) {
        if response.is_notification() {
            trace!("GRPC client: handle_response received a notification");
            match Notification::try_from(&response) {
                Ok(notification) => {
                    let event: EventType = (&notification).into();
                    trace!("GRPC client: handle_response received notification: {:?}", event);

                    // Here we ignore any returned error
                    match self.notification_channel.try_send(notification) {
                        Ok(_) => {}
                        Err(err) => {
                            error!("GRPC client: error while trying to send a notification to the notifier: {:?}", err);
                        }
                    }
                }
                Err(err) => {
                    error!("GRPC client: handle_response error converting response into notification: {:?}", err);
                }
            }
        } else if response.payload.is_some() {
            self.resolver().handle_response(response);
        }
    }

    async fn disconnect(&self) -> Result<()> {
        self.stop_connector_monitor().await?;
        self.stop_timeout_monitor().await?;
        self.stop_response_receiver_task().await?;
        self.request_receiver.close();
        trace!("GRPC client: disconnected");
        Ok(())
    }

    async fn stop_response_receiver_task(&self) -> Result<()> {
        if self.receiver_is_running.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            self.receiver_shutdown.request.trigger.trigger();
            self.receiver_shutdown.response.listener.clone().await;
        }
        Ok(())
    }

    async fn stop_timeout_monitor(&self) -> Result<()> {
        if self.timeout_is_running.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            self.timeout_shutdown.request.trigger.trigger();
            self.timeout_shutdown.response.listener.clone().await;
        }
        Ok(())
    }

    async fn stop_connector_monitor(&self) -> Result<()> {
        if self.connector_is_running.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            self.connector_shutdown.request.trigger.trigger();
            self.connector_shutdown.response.listener.clone().await;
        }
        Ok(())
    }

    /// Start sending notifications of some type to the client.
    async fn start_notify_to_client(&self, scope: Scope) -> RpcResult<()> {
        let request = kaspad_request::Payload::from_notification_type(&scope, Command::Start);
        self.call((&request).into(), request).await?;
        Ok(())
    }

    /// Stop sending notifications of some type to the client.
    async fn stop_notify_to_client(&self, scope: Scope) -> RpcResult<()> {
        if self.handle_stop_notify() {
            let request = kaspad_request::Payload::from_notification_type(&scope, Command::Stop);
            self.call((&request).into(), request).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl SubscriptionManager for Inner {
    async fn start_notify(&self, _: ListenerId, scope: Scope) -> NotifyResult<()> {
        trace!("GRPC client: start_notify: {:?}", scope);
        self.start_notify_to_client(scope).await.map_err(|err| NotifyError::General(err.to_string()))?;
        Ok(())
    }

    async fn stop_notify(&self, _: ListenerId, scope: Scope) -> NotifyResult<()> {
        if self.handle_stop_notify() {
            trace!("GRPC client: stop_notify: {:?}", scope);
            self.stop_notify_to_client(scope).await.map_err(|err| NotifyError::General(err.to_string()))?;
        } else {
            trace!("GRPC client: stop_notify ignored because not supported by the server: {:?}", scope);
        }
        Ok(())
    }
}
