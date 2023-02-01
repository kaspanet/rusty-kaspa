use self::{
    errors::Error,
    resolver::{id::IdResolver, queue::QueueResolver, DynResolver},
    result::Result,
};
use crate::protowire::{kaspad_request, rpc_client::RpcClient, GetInfoRequestMessage, KaspadRequest, KaspadResponse};
use async_trait::async_trait;
use futures::{
    future::FutureExt, // for `.fuse()`
    pin_mut,
    select,
};
use kaspa_core::trace;
use kaspa_rpc_core::{
    api::ops::RpcApiOps,
    api::{ops::SubscribeCommand, rpc::RpcApi},
    error::RpcError,
    error::RpcResult,
    model::message::*,
    notify::{
        channel::NotificationChannel,
        collector::RpcCoreCollector,
        events::EventType,
        listener::{ListenerID, ListenerReceiverSide, ListenerUtxoNotificationFilterSetting},
        notifier::Notifier,
        subscriber::{Subscriber, SubscriptionManager},
    },
    Notification, NotificationSender, NotificationType,
};
use kaspa_utils::triggers::DuplexTrigger;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tonic::Streaming;
use tonic::{codec::CompressionEncoding, transport::Endpoint};

mod errors;
mod resolver;
mod result;
#[macro_use]
mod route;

pub struct GrpcClient {
    inner: Arc<Inner>,
    notifier: Arc<Notifier>,
}

impl GrpcClient {
    pub async fn connect(address: String, reconnect: bool) -> Result<GrpcClient> {
        let notify_channel = NotificationChannel::default();
        let inner = Inner::connect(address, reconnect, notify_channel.sender()).await?;
        let collector = Arc::new(RpcCoreCollector::new(notify_channel.receiver()));
        let subscriber = Subscriber::new(inner.clone(), 0);

        let notifier =
            Arc::new(Notifier::new(Some(collector), Some(subscriber), ListenerUtxoNotificationFilterSetting::FilteredByAddress));

        Ok(Self { inner, notifier })
    }

    #[inline(always)]
    fn notifier(&self) -> Arc<Notifier> {
        self.notifier.clone()
    }

    pub async fn start(&self) {
        self.notifier.start();
    }

    pub async fn stop(&self) -> Result<()> {
        self.notifier.stop().await?;
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

    pub async fn shutdown(&mut self) -> Result<()> {
        self.inner.shutdown().await?;
        Ok(())
    }
}

#[async_trait]
impl RpcApi for GrpcClient {
    // this example illustrates the body of the function created by the route!() macro
    // async fn submit_block_call(&self, request: SubmitBlockRequest) -> RpcResult<SubmitBlockResponse> {
    //     self.inner.call(RpcApiOps::SubmitBlock, request).await?.as_ref().try_into()
    // }

    route!(ping_call, Ping);
    route!(get_process_metrics_call, GetProcessMetrics);
    route!(submit_block_call, SubmitBlock);
    route!(get_block_template_call, GetBlockTemplate);
    route!(get_block_call, GetBlock);
    route!(get_info_call, GetInfo);
    route!(get_current_network_call, GetCurrentNetwork);
    route!(get_peer_addresses_call, GetPeerAddresses);
    route!(get_selected_tip_hash_call, GetSelectedTipHash);
    route!(get_mempool_entry_call, GetMempoolEntry);
    route!(get_mempool_entries_call, GetMempoolEntries);
    route!(get_connected_peer_info_call, GetConnectedPeerInfo);
    route!(add_peer_call, AddPeer);
    route!(submit_transaction_call, SubmitTransaction);
    route!(get_subnetwork_call, GetSubnetwork);
    route!(get_virtual_selected_parent_chain_from_block_call, GetVirtualSelectedParentChainFromBlock);
    route!(get_blocks_call, GetBlocks);
    route!(get_block_count_call, GetBlockCount);
    route!(get_block_dag_info_call, GetBlockDagInfo);
    route!(resolve_finality_conflict_call, ResolveFinalityConflict);
    route!(shutdown_call, Shutdown);
    route!(get_headers_call, GetHeaders);
    route!(get_utxos_by_addresses_call, GetUtxosByAddresses);
    route!(get_balance_by_address_call, GetBalanceByAddress);
    route!(get_balances_by_addresses_call, GetBalancesByAddresses);
    route!(get_virtual_selected_parent_blue_score_call, GetVirtualSelectedParentBlueScore);
    route!(ban_call, Ban);
    route!(unban_call, Unban);
    route!(estimate_network_hashes_per_second_call, EstimateNetworkHashesPerSecond);
    route!(get_mempool_entries_by_addresses_call, GetMempoolEntriesByAddresses);
    route!(get_coin_supply_call, GetCoinSupply);

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    /// Register a new listener and returns an id and a channel receiver.
    fn register_new_listener(&self, channel: Option<NotificationChannel>) -> ListenerReceiverSide {
        self.notifier.register_new_listener(channel)
    }

    /// Unregister an existing listener.
    ///
    /// Stop all notifications for this listener and drop its channel.
    async fn unregister_listener(&self, id: ListenerID) -> RpcResult<()> {
        self.notifier.unregister_listener(id)?;
        Ok(())
    }

    /// Start sending notifications of some type to a listener.
    async fn start_notify(&self, id: ListenerID, notification_type: NotificationType) -> RpcResult<()> {
        self.notifier().start_notify(id, notification_type).await?;
        Ok(())
    }

    /// Stop sending notifications of some type to a listener.
    async fn stop_notify(&self, id: ListenerID, notification_type: NotificationType) -> RpcResult<()> {
        if self.handle_stop_notify() {
            self.notifier().stop_notify(id, notification_type).await?;
            Ok(())
        } else {
            Err(RpcError::UnsupportedFeature)
        }
    }
}

pub const CONNECT_TIMEOUT_DURATION: u64 = 20_000;
pub const KEEP_ALIVE_DURATION: u64 = 5_000;
pub const REQUEST_TIMEOUT_DURATION: u64 = 5_000;
pub const TIMEOUT_MONITORING_INTERVAL: u64 = 1_000;
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
/// Carry any subscribe call result up to the initial RpcApiGrpc::start_notify execution.
/// For now, RpcApiGrpc::start_notify only gets a result reflecting the call to
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
    address: String,

    server_features: ServerFeatures,

    // Pushing incoming notifications forward
    notify_sender: NotificationSender,

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
}

impl Inner {
    fn new(
        address: String,
        server_features: ServerFeatures,
        notify_sender: NotificationSender,
        request_sender: KaspadRequestSender,
        request_receiver: KaspadRequestReceiver,
    ) -> Self {
        let resolver: DynResolver = match server_features.handle_message_id {
            true => Arc::new(IdResolver::new()),
            false => Arc::new(QueueResolver::new()),
        };
        Self {
            address,
            server_features,
            notify_sender,
            request_sender,
            request_receiver,
            resolver,
            receiver_is_running: AtomicBool::new(false),
            receiver_shutdown: DuplexTrigger::new(),
            timeout_is_running: AtomicBool::new(false),
            timeout_shutdown: DuplexTrigger::new(),
            timeout_duration: REQUEST_TIMEOUT_DURATION,
            timeout_timer_interval: TIMEOUT_MONITORING_INTERVAL,
            connector_is_running: AtomicBool::new(false),
            connector_shutdown: DuplexTrigger::new(),
            connector_timer_interval: RECONNECT_INTERVAL,
        }
    }

    async fn connect(address: String, reconnect: bool, notify_sender: NotificationSender) -> Result<Arc<Self>> {
        // Request channel
        let (request_sender, request_receiver) = async_channel::unbounded();

        // Try to connect to the server
        let (stream, server_features) = Inner::try_connect(address.clone(), request_sender.clone(), request_receiver.clone()).await?;

        // create the inner object
        let inner = Arc::new(Inner::new(address, server_features, notify_sender, request_sender, request_receiver));

        // Start the request timeout cleaner
        inner.clone().spawn_request_timeout_monitor();

        // Start the response receiving task
        inner.clone().spawn_response_receiver_task(stream);

        if reconnect {
            // Start the connection monitor
            inner.clone().spawn_connection_monitor();
        }

        Ok(inner)
    }

    async fn try_connect(
        address: String,
        request_sender: KaspadRequestSender,
        request_receiver: KaspadRequestReceiver,
    ) -> Result<(Streaming<KaspadResponse>, ServerFeatures)> {
        // gRPC endpoint
        let channel = Endpoint::from_shared(address.clone())?
            .timeout(tokio::time::Duration::from_millis(REQUEST_TIMEOUT_DURATION))
            .connect_timeout(tokio::time::Duration::from_millis(CONNECT_TIMEOUT_DURATION))
            .tcp_keepalive(Some(tokio::time::Duration::from_millis(KEEP_ALIVE_DURATION)))
            .connect()
            .await?;

        let mut client =
            RpcClient::new(channel).send_compressed(CompressionEncoding::Gzip).accept_compressed(CompressionEncoding::Gzip);

        // Force the opening of the stream when connected to a go kaspad server.
        // This is also needed for querying server capabilities.
        request_sender.send(GetInfoRequestMessage {}.into()).await?;

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
        match stream.message().await? {
            Some(ref msg) => {
                trace!("GetInfo got response {:?}", msg);
                let response: RpcResult<GetInfoResponse> = msg.try_into();
                if let Ok(response) = response {
                    server_features.handle_stop_notify = response.has_notify_command;
                    server_features.handle_message_id = response.has_message_id;
                }
            }
            None => {
                return Err(Error::String("gRPC stream was closed by the server".to_string()));
            }
        }

        Ok((stream, server_features))
    }

    async fn reconnect(self: Arc<Self>) -> Result<()> {
        // TODO: verify if server feature have changed since first connection
        // TODO: re-register to notifications

        // Try to connect to the server
        let (stream, _) = Inner::try_connect(self.address.clone(), self.request_sender.clone(), self.request_receiver.clone()).await?;

        // Start the response receiving task
        self.spawn_response_receiver_task(stream);

        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.receiver_is_running.load(Ordering::SeqCst)
    }

    #[inline(always)]
    fn handle_message_id(&self) -> bool {
        self.server_features.handle_message_id
    }

    #[inline(always)]
    fn handle_stop_notify(&self) -> bool {
        self.server_features.handle_stop_notify
    }

    #[inline(always)]
    fn resolver(&self) -> DynResolver {
        self.resolver.clone()
    }

    async fn call(&self, op: RpcApiOps, request: impl Into<KaspadRequest>) -> Result<KaspadResponse> {
        // Calls are only allowed if the client is connected to the server
        if self.is_connected() {
            let id = u64::from_le_bytes(rand::random::<[u8; 8]>());
            let mut request: KaspadRequest = request.into();
            request.id = id;

            trace!("resolver call: {:?}", request);
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
            trace!("[GrpcClient] spawn request timeout monitor ignored since already spawned");
            return;
        }

        tokio::spawn(async move {
            let shutdown = self.timeout_shutdown.request.listener.clone().fuse();
            pin_mut!(shutdown);

            loop {
                let timeout_timer_interval = Duration::from_millis(self.timeout_timer_interval);
                let delay = tokio::time::sleep(timeout_timer_interval).fuse();
                pin_mut!(delay);

                select! {
                    _ = shutdown => { break; },
                    _ = delay => {
                        trace!("[GrpcClient] running timeout task");
                        let timeout = Duration::from_millis(self.timeout_duration);
                        self.resolver().remove_expired_requests(timeout);
                    },
                }
            }

            trace!("[GrpcClient] terminating timeout task");
            self.timeout_is_running.store(false, Ordering::SeqCst);
            self.timeout_shutdown.response.trigger.trigger();
        });
    }

    /// Launch a task receiving and handling response messages sent by the server.
    fn spawn_response_receiver_task(self: Arc<Self>, mut stream: Streaming<KaspadResponse>) {
        // Note: self is a cloned Arc here so that it can be used in the spawned task.

        // The task can only be spawned once
        if self.receiver_is_running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            trace!("[GrpcClient] spawn response receiver task ignored since already spawned");
            return;
        }

        tokio::spawn(async move {
            loop {
                trace!("[GrpcClient] response receiver loop");

                let shutdown = self.receiver_shutdown.request.listener.clone();
                pin_mut!(shutdown);

                tokio::select! {
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
                                        trace!("[GrpcClient] the connection to the server is closed");
                                        // A reconnection is needed
                                        break;
                                    }
                                }
                            },
                            Err(err) => {
                                trace!("[GrpcClient] the response receiver gets an error from the server: {:?}", err);
                            }
                        }
                    }
                }
            }
            trace!("[GrpcClient] terminating response receiver");

            // Mark as not connected
            self.receiver_is_running.store(false, Ordering::SeqCst);

            if self.receiver_shutdown.request.listener.is_triggered() {
                self.receiver_shutdown.response.trigger.trigger();
            }
        });
    }

    /// Launch a task that periodically checks if the connection to the server is alive
    /// and if not that tries to reconnect to the server.
    fn spawn_connection_monitor(self: Arc<Self>) {
        // Note: self is a cloned Arc here so that it can be used in the spawned task.

        // The task can only be spawned once
        if self.connector_is_running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            trace!("[GrpcClient] spawn connection monitor ignored since already spawned");
            return;
        }

        tokio::spawn(async move {
            let shutdown = self.connector_shutdown.request.listener.clone().fuse();
            pin_mut!(shutdown);
            loop {
                let connector_timer_interval = Duration::from_millis(self.connector_timer_interval);
                let delay = tokio::time::sleep(connector_timer_interval).fuse();
                pin_mut!(delay);
                select! {
                    _ = shutdown => { break; },
                    _ = delay => {
                        trace!("[GrpcClient] running connection monitor task");
                        if !self.is_connected() {
                            match self.clone().reconnect().await {
                                Ok(_) => {
                                    trace!("[GrpcClient] reconnection to server succeeded");
                                },
                                Err(err) => {
                                    trace!("[GrpcClient] reconnection to server failed with error {err:?}");
                                }
                            }
                        }
                    },
                }
            }
            trace!("[GrpcClient] terminating connection monitor");
            self.connector_is_running.store(false, Ordering::SeqCst);
            self.connector_shutdown.response.trigger.trigger();
        });
    }

    fn handle_response(&self, response: KaspadResponse) {
        if response.is_notification() {
            trace!("[GrpcClient] handle_response received a notification");
            match Notification::try_from(&response) {
                Ok(notification) => {
                    let event: EventType = (&notification).into();
                    trace!("[GrpcClient] handle_response received notification: {:?}", event);

                    // Here we ignore any returned error
                    match self.notify_sender.try_send(Arc::new(notification)) {
                        Ok(_) => {}
                        Err(err) => {
                            trace!("[GrpcClient] error while trying to send a notification to the notifier: {:?}", err);
                        }
                    }
                }
                Err(err) => {
                    trace!("[GrpcClient] handle_response error converting response into notification: {:?}", err);
                }
            }
        } else if response.payload.is_some() {
            self.resolver().handle_response(response);
        }
    }

    async fn shutdown(&self) -> Result<()> {
        self.stop_timeout_monitor().await?;
        self.stop_response_receiver_task().await?;
        self.stop_connector_monitor().await?;
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
}

#[async_trait]
impl SubscriptionManager for Inner {
    async fn start_notify(self: Arc<Self>, _: ListenerID, notification_type: NotificationType) -> RpcResult<()> {
        trace!("[GrpcClient] start_notify: {:?}", notification_type);
        let request = kaspad_request::Payload::from_notification_type(&notification_type, SubscribeCommand::Start);
        self.clone().call((&request).into(), request).await?;
        Ok(())
    }

    async fn stop_notify(self: Arc<Self>, _: ListenerID, notification_type: NotificationType) -> RpcResult<()> {
        if self.handle_stop_notify() {
            trace!("[GrpcClient] stop_notify: {:?}", notification_type);
            let request = kaspad_request::Payload::from_notification_type(&notification_type, SubscribeCommand::Stop);
            self.clone().call((&request).into(), request).await?;
        } else {
            trace!("[GrpcClient] stop_notify ignored because not supported by the server: {:?}", notification_type);
        }
        Ok(())
    }
}
