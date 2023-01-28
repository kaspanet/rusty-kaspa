use super::connection::{GrpcConnectionManager, GrpcSender};
use crate::protowire::NotifyNewBlockTemplateResponseMessage;
use crate::protowire::{kaspad_request::Payload, rpc_server::Rpc, *};
use crate::server::StatusResult;
use futures::Stream;
use kaspa_core::trace;
use rpc_core::notify::channel::NotificationChannel;
use rpc_core::notify::listener::{ListenerID, ListenerReceiverSide, ListenerUtxoNotificationFilterSetting};
use rpc_core::notify::subscriber::DynSubscriptionManager;
use rpc_core::notify::subscriber::Subscriber;
use rpc_core::RpcResult;
use rpc_core::{
    api::rpc::RpcApi,
    notify::{collector::RpcCoreCollector, events::EVENT_TYPE_ARRAY, notifier::Notifier},
    server::service::RpcCoreService,
};
use std::{io::ErrorKind, net::SocketAddr, pin::Pin, sync::Arc};
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response};

/// A protowire RPC service.
///
/// Relay requests to a central core service that queries the consensus.
///
/// Registers into a central core service in order to receive consensus notifications and
/// send those forward to the registered clients.
///
///
/// ### Implementation notes
///
/// The service is a listener of the provided core service. The registration happens in the constructor,
/// giving it the lifetime of the overall service.
///
/// As a corollary, the unregistration should occur just before the object is dropped by calling finalize.
///
/// #### Lifetime and usage
///
/// - new -> Self
///     - start
///         - register_connection
///         - unregister_connection
///     - stop
/// - finalize
///
/// _Object is ready for being dropped. Any further usage of it is undefined behavior._
///
/// #### Further development
///
/// TODO: implement a queue of requests and a pool of workers preparing and sending back the responses.
pub struct GrpcService {
    core_service: Arc<RpcCoreService>,
    core_channel: NotificationChannel,
    core_listener: Arc<ListenerReceiverSide>,
    connection_manager: Arc<RwLock<GrpcConnectionManager>>,
    notifier: Arc<Notifier>,
}

impl GrpcService {
    pub fn new(core_service: Arc<RpcCoreService>) -> Self {
        // Prepare core objects
        let core_channel = NotificationChannel::default();
        let core_listener = Arc::new(core_service.register_new_listener(Some(core_channel.clone())));

        // Prepare internals
        let collector = Arc::new(RpcCoreCollector::new(core_channel.receiver()));
        let subscription_manager: DynSubscriptionManager = core_service.notifier();
        let subscriber = Subscriber::new(subscription_manager, core_listener.id);
        let notifier =
            Arc::new(Notifier::new(Some(collector), Some(subscriber), ListenerUtxoNotificationFilterSetting::FilteredByAddress));
        let connection_manager = Arc::new(RwLock::new(GrpcConnectionManager::new(notifier.clone())));

        Self { core_service, core_channel, core_listener, connection_manager, notifier }
    }

    pub fn start(&self) {
        // Start the internal notifier
        self.notifier.clone().start();
    }

    pub async fn register_connection(&self, address: SocketAddr, sender: GrpcSender) -> ListenerID {
        self.connection_manager.write().await.register(address, sender).await
    }

    pub async fn unregister_connection(&self, address: SocketAddr) {
        self.connection_manager.write().await.unregister(address).await;
    }

    pub async fn stop(&self) -> RpcResult<()> {
        // Unsubscribe from all notification types
        let listener_id = self.core_listener.id;
        for event in EVENT_TYPE_ARRAY.into_iter() {
            self.core_service.stop_notify(listener_id, event.into()).await?;
        }

        // Stop the internal notifier
        self.notifier.clone().stop().await?;

        Ok(())
    }

    pub async fn finalize(&self) -> RpcResult<()> {
        self.core_service.unregister_listener(self.core_listener.id).await?;
        self.core_channel.receiver().close();
        Ok(())
    }
}

#[tonic::async_trait]
impl Rpc for Arc<GrpcService> {
    type MessageStreamStream = Pin<Box<dyn Stream<Item = Result<KaspadResponse, tonic::Status>> + Send + Sync + 'static>>;

    async fn message_stream(
        &self,
        request: Request<tonic::Streaming<KaspadRequest>>,
    ) -> Result<Response<Self::MessageStreamStream>, tonic::Status> {
        let remote_addr = request.remote_addr().ok_or_else(|| {
            tonic::Status::new(tonic::Code::InvalidArgument, "Incoming connection opening request has no remote address".to_string())
        })?;

        trace!("MessageStream from {:?}", remote_addr);

        // External sender and receiver
        let (send_channel, mut recv_channel) = mpsc::channel::<StatusResult<KaspadResponse>>(128);
        let listener_id = self.register_connection(remote_addr, send_channel.clone()).await;

        // Internal related sender and receiver
        let (stream_tx, stream_rx) = mpsc::channel::<StatusResult<KaspadResponse>>(10);

        // KaspadResponse forwarder
        let connection_manager = self.connection_manager.clone();
        tokio::spawn(async move {
            while let Some(msg) = recv_channel.recv().await {
                match stream_tx.send(msg).await {
                    Ok(_) => {}
                    Err(_) => {
                        // If sending failed, then remove the connection from connection manager
                        trace!("[Remote] stream tx sending error. Remote {:?}", &remote_addr);
                        connection_manager.write().await.unregister(remote_addr).await;
                    }
                }
            }
        });

        // Request handler
        let core_service = self.core_service.clone();
        let connection_manager = self.connection_manager.clone();
        let notifier = self.notifier.clone();
        let mut request_stream: tonic::Streaming<KaspadRequest> = request.into_inner();
        tokio::spawn(async move {
            loop {
                match request_stream.message().await {
                    Ok(Some(request)) => {
                        //trace!("Incoming {:?}", request);
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
                                Payload::GetVirtualSelectedParentBlueScoreRequest(ref request) => match request.try_into() {
                                    Ok(request) => core_service.get_virtual_selected_parent_blue_score_call(request).await.into(),
                                    Err(err) => GetVirtualSelectedParentBlueScoreResponseMessage::from(err).into(),
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
                                Payload::GetVirtualSelectedParentChainFromBlockRequest(ref request) => match request.try_into() {
                                    Ok(request) => {
                                        core_service.get_virtual_selected_parent_chain_from_block_call(request).await.into()
                                    }
                                    Err(err) => GetVirtualSelectedParentChainFromBlockResponseMessage::from(err).into(),
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

                                Payload::NotifyBlockAddedRequest(ref request) => {
                                    match rpc_core::NotifyBlockAddedRequest::try_from(request) {
                                        Ok(request) => {
                                            let result = notifier.clone().execute_subscribe_command(
                                                listener_id,
                                                rpc_core::NotificationType::BlockAdded,
                                                request.command,
                                            );
                                            NotifyBlockAddedResponseMessage::from(result).into()
                                        }
                                        Err(err) => NotifyBlockAddedResponseMessage::from(err).into(),
                                    }
                                }

                                Payload::NotifyVirtualSelectedParentChainChangedRequest(ref request) => {
                                    match rpc_core::NotifyVirtualSelectedParentChainChangedRequest::try_from(request) {
                                        Ok(request) => {
                                            let result = notifier.clone().execute_subscribe_command(
                                                listener_id,
                                                rpc_core::NotificationType::VirtualSelectedParentChainChanged,
                                                request.command,
                                            );
                                            NotifyVirtualSelectedParentChainChangedResponseMessage::from(result).into()
                                        }
                                        Err(err) => NotifyVirtualSelectedParentChainChangedResponseMessage::from(err).into(),
                                    }
                                }

                                Payload::NotifyFinalityConflictRequest(ref request) => {
                                    match rpc_core::NotifyFinalityConflictRequest::try_from(request) {
                                        Ok(request) => {
                                            let result = notifier
                                                .clone()
                                                .execute_subscribe_command(
                                                    listener_id,
                                                    rpc_core::NotificationType::FinalityConflict,
                                                    request.command,
                                                )
                                                .and(notifier.clone().execute_subscribe_command(
                                                    listener_id,
                                                    rpc_core::NotificationType::FinalityConflictResolved,
                                                    request.command,
                                                ));
                                            NotifyFinalityConflictResponseMessage::from(result).into()
                                        }
                                        Err(err) => NotifyFinalityConflictResponseMessage::from(err).into(),
                                    }
                                }

                                Payload::NotifyUtxosChangedRequest(ref request) => {
                                    match rpc_core::NotifyUtxosChangedRequest::try_from(request) {
                                        Ok(request) => {
                                            let result = notifier.clone().execute_subscribe_command(
                                                listener_id,
                                                rpc_core::NotificationType::UtxosChanged(request.addresses),
                                                request.command,
                                            );
                                            NotifyUtxosChangedResponseMessage::from(result).into()
                                        }
                                        Err(err) => NotifyUtxosChangedResponseMessage::from(err).into(),
                                    }
                                }

                                Payload::NotifyVirtualSelectedParentBlueScoreChangedRequest(ref request) => {
                                    match rpc_core::NotifyVirtualSelectedParentBlueScoreChangedRequest::try_from(request) {
                                        Ok(request) => {
                                            let result = notifier.clone().execute_subscribe_command(
                                                listener_id,
                                                rpc_core::NotificationType::VirtualSelectedParentBlueScoreChanged,
                                                request.command,
                                            );
                                            NotifyVirtualSelectedParentBlueScoreChangedResponseMessage::from(result).into()
                                        }
                                        Err(err) => NotifyVirtualSelectedParentBlueScoreChangedResponseMessage::from(err).into(),
                                    }
                                }

                                Payload::NotifyVirtualDaaScoreChangedRequest(ref request) => {
                                    match rpc_core::NotifyVirtualDaaScoreChangedRequest::try_from(request) {
                                        Ok(request) => {
                                            let result = notifier.clone().execute_subscribe_command(
                                                listener_id,
                                                rpc_core::NotificationType::VirtualDaaScoreChanged,
                                                request.command,
                                            );
                                            NotifyVirtualDaaScoreChangedResponseMessage::from(result).into()
                                        }
                                        Err(err) => NotifyVirtualDaaScoreChangedResponseMessage::from(err).into(),
                                    }
                                }

                                Payload::NotifyPruningPointUtxoSetOverrideRequest(ref request) => {
                                    match rpc_core::NotifyPruningPointUtxoSetOverrideRequest::try_from(request) {
                                        Ok(request) => {
                                            let result = notifier.clone().execute_subscribe_command(
                                                listener_id,
                                                rpc_core::NotificationType::PruningPointUtxoSetOverride,
                                                request.command,
                                            );
                                            NotifyPruningPointUtxoSetOverrideResponseMessage::from(result).into()
                                        }
                                        Err(err) => NotifyPruningPointUtxoSetOverrideResponseMessage::from(err).into(),
                                    }
                                }

                                Payload::NotifyNewBlockTemplateRequest(ref request) => {
                                    match rpc_core::NotifyNewBlockTemplateRequest::try_from(request) {
                                        Ok(request) => {
                                            let result = notifier.clone().execute_subscribe_command(
                                                listener_id,
                                                rpc_core::NotificationType::NewBlockTemplate,
                                                request.command,
                                            );
                                            NotifyNewBlockTemplateResponseMessage::from(result).into()
                                        }
                                        Err(err) => NotifyNewBlockTemplateResponseMessage::from(err).into(),
                                    }
                                }

                                Payload::StopNotifyingUtxosChangedRequest(ref request) => {
                                    let notify_request = NotifyUtxosChangedRequestMessage::from(request);
                                    let response: StopNotifyingUtxosChangedResponseMessage =
                                        match rpc_core::NotifyUtxosChangedRequest::try_from(&notify_request) {
                                            Ok(request) => {
                                                let result = notifier.clone().execute_subscribe_command(
                                                    listener_id,
                                                    rpc_core::NotificationType::UtxosChanged(request.addresses),
                                                    request.command,
                                                );
                                                NotifyUtxosChangedResponseMessage::from(result).into()
                                            }
                                            Err(err) => NotifyUtxosChangedResponseMessage::from(err).into(),
                                        };
                                    KaspadResponse {
                                        id: 0,
                                        payload: Some(kaspad_response::Payload::StopNotifyingUtxosChangedResponse(response)),
                                    }
                                }

                                Payload::StopNotifyingPruningPointUtxoSetOverrideRequest(ref request) => {
                                    let notify_request = NotifyPruningPointUtxoSetOverrideRequestMessage::from(request);
                                    let response: StopNotifyingPruningPointUtxoSetOverrideResponseMessage =
                                        match rpc_core::NotifyPruningPointUtxoSetOverrideRequest::try_from(&notify_request) {
                                            Ok(request) => {
                                                let result = notifier.clone().execute_subscribe_command(
                                                    listener_id,
                                                    rpc_core::NotificationType::PruningPointUtxoSetOverride,
                                                    request.command,
                                                );
                                                NotifyPruningPointUtxoSetOverrideResponseMessage::from(result).into()
                                            }
                                            Err(err) => NotifyPruningPointUtxoSetOverrideResponseMessage::from(err).into(),
                                        };
                                    KaspadResponse {
                                        id: 0,
                                        payload: Some(kaspad_response::Payload::StopNotifyingPruningPointUtxoSetOverrideResponse(
                                            response,
                                        )),
                                    }
                                }
                            }
                        } else {
                            GetBlockResponseMessage::from(rpc_core::RpcError::General("Missing request payload".to_string())).into()
                        };
                        response.id = request.id;
                        //trace!("Outgoing {:?}", response);

                        match send_channel.send(Ok(response)).await {
                            Ok(_) => {}
                            Err(err) => {
                                trace!("tx send error: {:?}", err);
                            }
                        }
                    }
                    Ok(None) => {
                        trace!("Request handler stream {0} got Ok(None). Connection terminated by the server", remote_addr);
                        break;
                    }

                    Err(err) => {
                        if let Some(io_err) = match_for_io_error(&err) {
                            if io_err.kind() == ErrorKind::BrokenPipe {
                                // here you can handle special case when client
                                // disconnected in unexpected way
                                trace!("\tRequest handler stream {0} error: client disconnected, broken pipe", remote_addr);
                                break;
                            }
                        }

                        match send_channel.send(Err(err)).await {
                            Ok(_) => (),
                            Err(_err) => break, // response was dropped
                        }
                    }
                }
            }
            trace!("Request handler {0} terminated", remote_addr);
            connection_manager.write().await.unregister(remote_addr).await;
        });

        // Return connection stream
        let response_stream = ReceiverStream::new(stream_rx);
        Ok(Response::new(Box::pin(response_stream)))
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
