use crate::error::{GrpcServerError, GrpcServerResult};
use kaspa_core::{debug, trace};
use kaspa_grpc_core::protowire::{kaspad_request::Payload, *};
use kaspa_notify::{
    connection::Connection,
    error::Error as NotificationError,
    listener::ListenerId,
    notifier::Notifier,
    scope::{
        BlockAddedScope, FinalityConflictResolvedScope, FinalityConflictScope, NewBlockTemplateScope,
        PruningPointUtxoSetOverrideScope, Scope, SinkBlueScoreChangedScope, UtxosChangedScope, VirtualChainChangedScope,
        VirtualDaaScoreChangedScope,
    },
    subscriber::SubscriptionManager,
};
use kaspa_rpc_core::{api::rpc::RpcApi, Notification};
use kaspa_rpc_service::service::RpcCoreService;
use once_cell::unsync::Lazy;
use parking_lot::Mutex;
use std::{collections::HashMap, fmt::Display, io::ErrorKind, net::SocketAddr, sync::Arc};
use tokio::select;
use tokio::sync::{
    mpsc::Sender as MpscSender,
    oneshot::{channel as oneshot_channel, Sender as OneshotSender},
};
use tonic::Streaming;

pub type GrpcSender = MpscSender<StatusResult<KaspadResponse>>;
pub type StatusResult<T> = Result<T, tonic::Status>;

// TODO: identify a connection by a Uuid instead of an address
#[derive(Debug)]
struct Inner {
    pub net_address: SocketAddr, // TODO: wrap into an option
    pub outgoing_route: GrpcSender,
    /// Used on connection close to signal the connection receive loop to exit
    shutdown_signal: Mutex<Option<OneshotSender<()>>>,
}

#[derive(Clone, Debug)]
pub struct GrpcConnection {
    inner: Arc<Inner>,
}

impl Display for GrpcConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner.net_address)
    }
}

impl GrpcConnection {
    pub fn new(
        net_address: SocketAddr,
        core_service: Arc<RpcCoreService>,
        connection_manager: GrpcConnectionManager,
        notifier: Arc<Notifier<Notification, GrpcConnection>>,
        mut incoming_stream: Streaming<KaspadRequest>,
        outgoing_route: GrpcSender,
    ) -> Self {
        let (shutdown_sender, mut shutdown_receiver) = oneshot_channel();
        let connection =
            Self { inner: Arc::new(Inner { net_address, outgoing_route, shutdown_signal: Mutex::new(Some(shutdown_sender)) }) };
        let connection_clone = connection.clone();
        let outgoing_route = connection.inner.outgoing_route.clone();
        // Start the connection receive loop
        tokio::spawn(async move {
            let listener_id: Lazy<ListenerId, _> = Lazy::new(|| notifier.clone().register_new_listener(connection.clone()));
            loop {
                select! {
                    biased; // We use biased polling so that the shutdown signal is always checked first

                    _ = &mut shutdown_receiver => {
                        debug!("gRPC: Connection receive loop - shutdown signal received, exiting connection receive loop, client-id: {}", connection.identity());
                        break;
                    }

                    res = incoming_stream.message() => match res {
                        Ok(Some(request)) => {
                            //trace!("gRPC: request: {:?}, client-id: {}", request, connection.identity());

                            let response = match request.is_subscription() {
                                true => {
                                    // Initialize the listener id locally to ensure thread safety
                                    let listener_id = *listener_id;
                                    Self::handle_subscription(request, listener_id, &notifier).await
                                },
                                false => Self::handle_request(request, &core_service).await,
                            };
                            match response {
                                Ok(response) => {
                                    match outgoing_route.send(Ok(response)).await {
                                        Ok(()) => {},
                                        Err(e) => {
                                            debug!("gRPC: Connection receive loop - send error {} for client: {}", e, connection);
                                            break;
                                        },
                                    }
                                }
                                Err(e) => {
                                    debug!("gRPC: Connection receive loop - request handling error {} for client: {}", e, connection);
                                    break;
                                }
                            }

                        }
                        Ok(None) => {
                            debug!("gRPC: Connection receive loop - incoming stream ended from client {}", connection);
                            break;
                        }
                        Err(err) => {
                            {
                                if let Some(io_err) = match_for_io_error(&err) {
                                    if io_err.kind() == ErrorKind::BrokenPipe {
                                        debug!("gRPC: Connection receive loop - client {} disconnected, broken pipe", connection);
                                        break;
                                    }
                                }
                                debug!("gRPC: Connection receive loop - network error: {} from client {}", err, connection);
                            }
                            break;
                        }
                    }
                }
            }
            trace!("gRPC: Connection receive loop - terminated for client {}", connection);
            if let Ok(listener_id) = Lazy::into_value(listener_id) {
                let _ = notifier.unregister_listener(listener_id);
            }
            connection_manager.unregister(net_address);
        });

        connection_clone
    }

    pub fn net_address(&self) -> SocketAddr {
        self.inner.net_address
    }

    pub fn identity(&self) -> SocketAddr {
        self.inner.net_address
    }

    async fn handle_request(request: KaspadRequest, core_service: &Arc<RpcCoreService>) -> GrpcServerResult<KaspadResponse> {
        let mut response: KaspadResponse = if let Some(payload) = request.payload {
            match payload {
                Payload::GetProcessMetricsRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_process_metrics_call(request).await.into(),
                    Err(err) => GetProcessMetricsResponseMessage::from(err).into(),
                },
                Payload::PingRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.ping_call(request).await.into(),
                    Err(err) => PingResponseMessage::from(err).into(),
                },
                Payload::GetCoinSupplyRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_coin_supply_call(request).await.into(),
                    Err(err) => GetCoinSupplyResponseMessage::from(err).into(),
                },
                Payload::GetMempoolEntriesByAddressesRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_mempool_entries_by_addresses_call(request).await.into(),
                    Err(err) => GetMempoolEntriesByAddressesResponseMessage::from(err).into(),
                },
                Payload::GetBalancesByAddressesRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_balances_by_addresses_call(request).await.into(),
                    Err(err) => GetBalancesByAddressesResponseMessage::from(err).into(),
                },
                Payload::GetBalanceByAddressRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_balance_by_address_call(request).await.into(),
                    Err(err) => GetBalanceByAddressResponseMessage::from(err).into(),
                },
                Payload::EstimateNetworkHashesPerSecondRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.estimate_network_hashes_per_second_call(request).await.into(),
                    Err(err) => EstimateNetworkHashesPerSecondResponseMessage::from(err).into(),
                },
                Payload::UnbanRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.unban_call(request).await.into(),
                    Err(err) => UnbanResponseMessage::from(err).into(),
                },
                Payload::BanRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.ban_call(request).await.into(),
                    Err(err) => BanResponseMessage::from(err).into(),
                },
                Payload::GetSinkBlueScoreRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_sink_blue_score_call(request).await.into(),
                    Err(err) => GetSinkBlueScoreResponseMessage::from(err).into(),
                },
                Payload::GetUtxosByAddressesRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_utxos_by_addresses_call(request).await.into(),
                    Err(err) => GetUtxosByAddressesResponseMessage::from(err).into(),
                },
                Payload::GetHeadersRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_headers_call(request).await.into(),
                    Err(err) => ShutdownResponseMessage::from(err).into(),
                },
                Payload::ShutdownRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.shutdown_call(request).await.into(),
                    Err(err) => ShutdownResponseMessage::from(err).into(),
                },
                Payload::GetMempoolEntriesRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_mempool_entries_call(request).await.into(),
                    Err(err) => GetMempoolEntriesResponseMessage::from(err).into(),
                },
                Payload::ResolveFinalityConflictRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.resolve_finality_conflict_call(request).await.into(),
                    Err(err) => ResolveFinalityConflictResponseMessage::from(err).into(),
                },
                Payload::GetBlockDagInfoRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_block_dag_info_call(request).await.into(),
                    Err(err) => GetBlockDagInfoResponseMessage::from(err).into(),
                },
                Payload::GetBlockCountRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_block_count_call(request).await.into(),
                    Err(err) => GetBlockCountResponseMessage::from(err).into(),
                },
                Payload::GetBlocksRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_blocks_call(request).await.into(),
                    Err(err) => GetBlocksResponseMessage::from(err).into(),
                },
                Payload::GetVirtualChainFromBlockRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_virtual_chain_from_block_call(request).await.into(),
                    Err(err) => GetVirtualChainFromBlockResponseMessage::from(err).into(),
                },
                Payload::GetSubnetworkRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_subnetwork_call(request).await.into(),
                    Err(err) => GetSubnetworkResponseMessage::from(err).into(),
                },
                Payload::SubmitTransactionRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.submit_transaction_call(request).await.into(),
                    Err(err) => SubmitTransactionResponseMessage::from(err).into(),
                },
                Payload::AddPeerRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.add_peer_call(request).await.into(),
                    Err(err) => AddPeerResponseMessage::from(err).into(),
                },
                Payload::GetConnectedPeerInfoRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_connected_peer_info_call(request).await.into(),
                    Err(err) => GetConnectedPeerInfoResponseMessage::from(err).into(),
                },
                Payload::GetMempoolEntryRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_mempool_entry_call(request).await.into(),
                    Err(err) => GetMempoolEntryResponseMessage::from(err).into(),
                },
                Payload::GetSelectedTipHashRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_selected_tip_hash_call(request).await.into(),
                    Err(err) => GetSelectedTipHashResponseMessage::from(err).into(),
                },
                Payload::GetPeerAddressesRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_peer_addresses_call(request).await.into(),
                    Err(err) => GetPeerAddressesResponseMessage::from(err).into(),
                },
                Payload::GetCurrentNetworkRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_current_network_call(request).await.into(),
                    Err(err) => GetCurrentNetworkResponseMessage::from(err).into(),
                },
                Payload::SubmitBlockRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.submit_block_call(request).await.into(),
                    Err(err) => SubmitBlockResponseMessage::from(err).into(),
                },
                Payload::GetBlockTemplateRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_block_template_call(request).await.into(),
                    Err(err) => GetBlockTemplateResponseMessage::from(err).into(),
                },

                Payload::GetBlockRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_block_call(request).await.into(),
                    Err(err) => GetBlockResponseMessage::from(err).into(),
                },

                Payload::GetInfoRequest(ref request) => match request.try_into() {
                    Ok(request) => core_service.get_info_call(request).await.into(),
                    Err(err) => GetInfoResponseMessage::from(err).into(),
                },

                _ => {
                    return Err(GrpcServerError::InvalidRequestPayload);
                }
            }
        } else {
            return Err(GrpcServerError::InvalidRequestPayload);
        };
        response.id = request.id;

        Ok(response)
    }

    async fn handle_subscription(
        request: KaspadRequest,
        listener_id: ListenerId,
        notifier: &Arc<Notifier<Notification, GrpcConnection>>,
    ) -> GrpcServerResult<KaspadResponse> {
        let mut response: KaspadResponse = if let Some(payload) = request.payload {
            match payload {
                Payload::NotifyBlockAddedRequest(ref request) => match kaspa_rpc_core::NotifyBlockAddedRequest::try_from(request) {
                    Ok(request) => {
                        let listener_id = listener_id;
                        let result = notifier
                            .clone()
                            .execute_subscribe_command(listener_id, Scope::BlockAdded(BlockAddedScope::default()), request.command)
                            .await;
                        NotifyBlockAddedResponseMessage::from(result).into()
                    }
                    Err(err) => NotifyBlockAddedResponseMessage::from(err).into(),
                },

                Payload::NotifyVirtualChainChangedRequest(ref request) => {
                    match kaspa_rpc_core::NotifyVirtualChainChangedRequest::try_from(request) {
                        Ok(request) => {
                            let listener_id = listener_id;
                            let result = notifier
                                .clone()
                                .execute_subscribe_command(
                                    listener_id,
                                    Scope::VirtualChainChanged(VirtualChainChangedScope::new(
                                        request.include_accepted_transaction_ids,
                                    )),
                                    request.command,
                                )
                                .await;
                            NotifyVirtualChainChangedResponseMessage::from(result).into()
                        }
                        Err(err) => NotifyVirtualChainChangedResponseMessage::from(err).into(),
                    }
                }

                Payload::NotifyFinalityConflictRequest(ref request) => {
                    match kaspa_rpc_core::NotifyFinalityConflictRequest::try_from(request) {
                        Ok(request) => {
                            let listener_id = listener_id;
                            let result = notifier
                                .clone()
                                .execute_subscribe_command(
                                    listener_id,
                                    Scope::FinalityConflict(FinalityConflictScope::default()),
                                    request.command,
                                )
                                .await
                                .and(
                                    notifier
                                        .clone()
                                        .execute_subscribe_command(
                                            listener_id,
                                            Scope::FinalityConflictResolved(FinalityConflictResolvedScope::default()),
                                            request.command,
                                        )
                                        .await,
                                );
                            NotifyFinalityConflictResponseMessage::from(result).into()
                        }
                        Err(err) => NotifyFinalityConflictResponseMessage::from(err).into(),
                    }
                }

                Payload::NotifyUtxosChangedRequest(ref request) => {
                    match kaspa_rpc_core::NotifyUtxosChangedRequest::try_from(request) {
                        Ok(request) => {
                            let listener_id = listener_id;
                            let result = notifier
                                .clone()
                                .execute_subscribe_command(
                                    listener_id,
                                    Scope::UtxosChanged(UtxosChangedScope::new(request.addresses)),
                                    request.command,
                                )
                                .await;
                            NotifyUtxosChangedResponseMessage::from(result).into()
                        }
                        Err(err) => NotifyUtxosChangedResponseMessage::from(err).into(),
                    }
                }

                Payload::NotifySinkBlueScoreChangedRequest(ref request) => {
                    match kaspa_rpc_core::NotifySinkBlueScoreChangedRequest::try_from(request) {
                        Ok(request) => {
                            let listener_id = listener_id;
                            let result = notifier
                                .clone()
                                .execute_subscribe_command(
                                    listener_id,
                                    Scope::SinkBlueScoreChanged(SinkBlueScoreChangedScope::default()),
                                    request.command,
                                )
                                .await;
                            NotifySinkBlueScoreChangedResponseMessage::from(result).into()
                        }
                        Err(err) => NotifySinkBlueScoreChangedResponseMessage::from(err).into(),
                    }
                }

                Payload::NotifyVirtualDaaScoreChangedRequest(ref request) => {
                    match kaspa_rpc_core::NotifyVirtualDaaScoreChangedRequest::try_from(request) {
                        Ok(request) => {
                            let listener_id = listener_id;
                            let result = notifier
                                .clone()
                                .execute_subscribe_command(
                                    listener_id,
                                    Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope::default()),
                                    request.command,
                                )
                                .await;
                            NotifyVirtualDaaScoreChangedResponseMessage::from(result).into()
                        }
                        Err(err) => NotifyVirtualDaaScoreChangedResponseMessage::from(err).into(),
                    }
                }

                Payload::NotifyPruningPointUtxoSetOverrideRequest(ref request) => {
                    match kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideRequest::try_from(request) {
                        Ok(request) => {
                            let listener_id = listener_id;
                            let result = notifier
                                .clone()
                                .execute_subscribe_command(
                                    listener_id,
                                    Scope::PruningPointUtxoSetOverride(PruningPointUtxoSetOverrideScope::default()),
                                    request.command,
                                )
                                .await;
                            NotifyPruningPointUtxoSetOverrideResponseMessage::from(result).into()
                        }
                        Err(err) => NotifyPruningPointUtxoSetOverrideResponseMessage::from(err).into(),
                    }
                }

                Payload::NotifyNewBlockTemplateRequest(ref request) => {
                    match kaspa_rpc_core::NotifyNewBlockTemplateRequest::try_from(request) {
                        Ok(request) => {
                            let listener_id = listener_id;
                            let result = notifier
                                .clone()
                                .execute_subscribe_command(
                                    listener_id,
                                    Scope::NewBlockTemplate(NewBlockTemplateScope::default()),
                                    request.command,
                                )
                                .await;
                            NotifyNewBlockTemplateResponseMessage::from(result).into()
                        }
                        Err(err) => NotifyNewBlockTemplateResponseMessage::from(err).into(),
                    }
                }

                Payload::StopNotifyingUtxosChangedRequest(ref request) => {
                    let notify_request = NotifyUtxosChangedRequestMessage::from(request);
                    let response: StopNotifyingUtxosChangedResponseMessage =
                        match kaspa_rpc_core::NotifyUtxosChangedRequest::try_from(&notify_request) {
                            Ok(request) => {
                                let listener_id = listener_id;
                                let result = notifier
                                    .clone()
                                    .execute_subscribe_command(
                                        listener_id,
                                        Scope::UtxosChanged(UtxosChangedScope::new(request.addresses)),
                                        request.command,
                                    )
                                    .await;
                                NotifyUtxosChangedResponseMessage::from(result).into()
                            }
                            Err(err) => NotifyUtxosChangedResponseMessage::from(err).into(),
                        };
                    KaspadResponse { id: 0, payload: Some(kaspad_response::Payload::StopNotifyingUtxosChangedResponse(response)) }
                }

                Payload::StopNotifyingPruningPointUtxoSetOverrideRequest(ref request) => {
                    let notify_request = NotifyPruningPointUtxoSetOverrideRequestMessage::from(request);
                    let response: StopNotifyingPruningPointUtxoSetOverrideResponseMessage =
                        match kaspa_rpc_core::NotifyPruningPointUtxoSetOverrideRequest::try_from(&notify_request) {
                            Ok(request) => {
                                let listener_id = listener_id;
                                let result = notifier
                                    .clone()
                                    .execute_subscribe_command(
                                        listener_id,
                                        Scope::PruningPointUtxoSetOverride(PruningPointUtxoSetOverrideScope::default()),
                                        request.command,
                                    )
                                    .await;
                                NotifyPruningPointUtxoSetOverrideResponseMessage::from(result).into()
                            }
                            Err(err) => NotifyPruningPointUtxoSetOverrideResponseMessage::from(err).into(),
                        };
                    KaspadResponse {
                        id: 0,
                        payload: Some(kaspad_response::Payload::StopNotifyingPruningPointUtxoSetOverrideResponse(response)),
                    }
                }

                _ => {
                    return Err(GrpcServerError::InvalidSubscriptionPayload);
                }
            }
        } else {
            return Err(GrpcServerError::InvalidSubscriptionPayload);
        };
        response.id = request.id;

        Ok(response)
    }
}

fn match_for_io_error(err_status: &tonic::Status) -> Option<&std::io::Error> {
    let mut err: &(dyn std::error::Error + 'static) = err_status;

    loop {
        if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
            return Some(io_err);
        }

        // h2::Error do not expose std::io::Error with `source()`
        // https://github.com/hyperium/h2/pull/462
        if let Some(h2_err) = err.downcast_ref::<h2::Error>() {
            if let Some(io_err) = h2_err.get_io() {
                return Some(io_err);
            }
        }

        err = match err.source() {
            Some(err) => err,
            None => return None,
        };
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Default)]
pub enum GrpcEncoding {
    #[default]
    ProtowireResponse = 0,
}

impl Connection for GrpcConnection {
    type Notification = Notification;
    type Message = Arc<StatusResult<KaspadResponse>>;
    type Encoding = GrpcEncoding;
    type Error = super::error::GrpcServerError;

    fn encoding(&self) -> Self::Encoding {
        GrpcEncoding::ProtowireResponse
    }

    fn into_message(notification: &kaspa_rpc_core::Notification, _: &Self::Encoding) -> Self::Message {
        Arc::new(Ok((notification).into()))
    }

    fn send(&self, message: Self::Message) -> Result<(), Self::Error> {
        match !self.is_closed() {
            true => Ok(self.inner.outgoing_route.try_send((*message).clone())?),
            false => Err(NotificationError::ConnectionClosed.into()),
        }
    }

    fn close(&self) -> bool {
        if let Some(signal) = self.inner.shutdown_signal.lock().take() {
            let _ = signal.send(());
            true
        } else {
            // This means the connection was already closed
            debug!("gRPC: Connection close was called more than once, client-id: {}", self.identity());
            false
        }
    }

    fn is_closed(&self) -> bool {
        self.inner.shutdown_signal.lock().is_none()
    }
}

#[derive(Clone)]
pub struct GrpcConnectionManager {
    connections: Arc<Mutex<HashMap<SocketAddr, GrpcConnection>>>,
    max_connections: usize,
}

impl GrpcConnectionManager {
    pub fn new(max_connections: usize) -> Self {
        Self { connections: Arc::new(Mutex::new(HashMap::new())), max_connections }
    }

    pub fn register(&self, connection: GrpcConnection) {
        debug!("gRPC: Register a new connection from {connection}");
        let prev_connection = self.connections.lock().insert(connection.identity(), connection);
        if let Some(prev) = prev_connection {
            if !prev.is_closed() {
                prev.close();
            }
        }
    }

    pub fn is_full(&self) -> bool {
        self.connections.lock().len() >= self.max_connections
    }

    pub fn unregister(&self, net_address: SocketAddr) {
        if let Some(connection) = self.connections.lock().remove(&net_address) {
            debug!("gRPC: Unregister the gRPC connection from {connection}");
            if !connection.is_closed() {
                connection.close();
            }
        } else {
            debug!("gRPC: Connection from {net_address} has already been unregistered");
        }
    }

    pub fn terminate_all_connections(&self) {
        let connections = self.connections.lock().drain().map(|(_, r)| r).collect::<Vec<_>>();
        for connection in connections {
            connection.close();
        }
    }
}
