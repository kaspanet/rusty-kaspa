use std::sync::Arc;

use super::handler_trait::Handler;
use crate::{
    connection::{Connection, GrpcNotifier, IncomingRoute},
    error::{GrpcServerError, GrpcServerResult},
};
use kaspa_core::debug;
use kaspa_grpc_core::protowire::{kaspad_request::Payload, *};
use kaspa_notify::{
    listener::ListenerId,
    scope::{
        BlockAddedScope, FinalityConflictResolvedScope, FinalityConflictScope, NewBlockTemplateScope,
        PruningPointUtxoSetOverrideScope, Scope, SinkBlueScoreChangedScope, UtxosChangedScope, VirtualChainChangedScope,
        VirtualDaaScoreChangedScope,
    },
    subscriber::SubscriptionManager,
};
use kaspa_rpc_core::api::{ops::RpcApiOps, rpc::DynRpcService};

// TODO: consider a macro generating RpcOpsApi-dedicated handler structs

pub struct RequestHandler {
    rpc_op: RpcApiOps,
    connection: Connection,
    core_service: DynRpcService,
    incoming_route: IncomingRoute,
}

impl RequestHandler {
    pub fn new(rpc_op: RpcApiOps, connection: Connection, core_service: DynRpcService, incoming_route: IncomingRoute) -> Self {
        Self { rpc_op, connection, core_service, incoming_route }
    }

    pub async fn handle_request(&self, request: KaspadRequest) -> GrpcServerResult<KaspadResponse> {
        let mut response: KaspadResponse = if let Some(payload) = request.payload {
            match payload {
                Payload::GetMetricsRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_metrics_call(request).await.into(),
                    Err(err) => GetMetricsResponseMessage::from(err).into(),
                },
                Payload::PingRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.ping_call(request).await.into(),
                    Err(err) => PingResponseMessage::from(err).into(),
                },
                Payload::GetCoinSupplyRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_coin_supply_call(request).await.into(),
                    Err(err) => GetCoinSupplyResponseMessage::from(err).into(),
                },
                Payload::GetMempoolEntriesByAddressesRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_mempool_entries_by_addresses_call(request).await.into(),
                    Err(err) => GetMempoolEntriesByAddressesResponseMessage::from(err).into(),
                },
                Payload::GetBalancesByAddressesRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_balances_by_addresses_call(request).await.into(),
                    Err(err) => GetBalancesByAddressesResponseMessage::from(err).into(),
                },
                Payload::GetBalanceByAddressRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_balance_by_address_call(request).await.into(),
                    Err(err) => GetBalanceByAddressResponseMessage::from(err).into(),
                },
                Payload::EstimateNetworkHashesPerSecondRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.estimate_network_hashes_per_second_call(request).await.into(),
                    Err(err) => EstimateNetworkHashesPerSecondResponseMessage::from(err).into(),
                },
                Payload::UnbanRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.unban_call(request).await.into(),
                    Err(err) => UnbanResponseMessage::from(err).into(),
                },
                Payload::BanRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.ban_call(request).await.into(),
                    Err(err) => BanResponseMessage::from(err).into(),
                },
                Payload::GetSinkBlueScoreRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_sink_blue_score_call(request).await.into(),
                    Err(err) => GetSinkBlueScoreResponseMessage::from(err).into(),
                },
                Payload::GetUtxosByAddressesRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_utxos_by_addresses_call(request).await.into(),
                    Err(err) => GetUtxosByAddressesResponseMessage::from(err).into(),
                },
                Payload::GetHeadersRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_headers_call(request).await.into(),
                    Err(err) => ShutdownResponseMessage::from(err).into(),
                },
                Payload::ShutdownRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.shutdown_call(request).await.into(),
                    Err(err) => ShutdownResponseMessage::from(err).into(),
                },
                Payload::GetMempoolEntriesRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_mempool_entries_call(request).await.into(),
                    Err(err) => GetMempoolEntriesResponseMessage::from(err).into(),
                },
                Payload::ResolveFinalityConflictRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.resolve_finality_conflict_call(request).await.into(),
                    Err(err) => ResolveFinalityConflictResponseMessage::from(err).into(),
                },
                Payload::GetBlockDagInfoRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_block_dag_info_call(request).await.into(),
                    Err(err) => GetBlockDagInfoResponseMessage::from(err).into(),
                },
                Payload::GetBlockCountRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_block_count_call(request).await.into(),
                    Err(err) => GetBlockCountResponseMessage::from(err).into(),
                },
                Payload::GetBlocksRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_blocks_call(request).await.into(),
                    Err(err) => GetBlocksResponseMessage::from(err).into(),
                },
                Payload::GetVirtualChainFromBlockRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_virtual_chain_from_block_call(request).await.into(),
                    Err(err) => GetVirtualChainFromBlockResponseMessage::from(err).into(),
                },
                Payload::GetSubnetworkRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_subnetwork_call(request).await.into(),
                    Err(err) => GetSubnetworkResponseMessage::from(err).into(),
                },
                Payload::SubmitTransactionRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.submit_transaction_call(request).await.into(),
                    Err(err) => SubmitTransactionResponseMessage::from(err).into(),
                },
                Payload::AddPeerRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.add_peer_call(request).await.into(),
                    Err(err) => AddPeerResponseMessage::from(err).into(),
                },
                Payload::GetConnectedPeerInfoRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_connected_peer_info_call(request).await.into(),
                    Err(err) => GetConnectedPeerInfoResponseMessage::from(err).into(),
                },
                Payload::GetMempoolEntryRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_mempool_entry_call(request).await.into(),
                    Err(err) => GetMempoolEntryResponseMessage::from(err).into(),
                },
                Payload::GetSelectedTipHashRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_selected_tip_hash_call(request).await.into(),
                    Err(err) => GetSelectedTipHashResponseMessage::from(err).into(),
                },
                Payload::GetPeerAddressesRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_peer_addresses_call(request).await.into(),
                    Err(err) => GetPeerAddressesResponseMessage::from(err).into(),
                },
                Payload::GetCurrentNetworkRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_current_network_call(request).await.into(),
                    Err(err) => GetCurrentNetworkResponseMessage::from(err).into(),
                },
                Payload::SubmitBlockRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.submit_block_call(request).await.into(),
                    Err(err) => SubmitBlockResponseMessage::from(err).into(),
                },
                Payload::GetBlockTemplateRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_block_template_call(request).await.into(),
                    Err(err) => GetBlockTemplateResponseMessage::from(err).into(),
                },

                Payload::GetBlockRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_block_call(request).await.into(),
                    Err(err) => GetBlockResponseMessage::from(err).into(),
                },

                Payload::GetInfoRequest(ref request) => match request.try_into() {
                    Ok(request) => self.core_service.get_info_call(request).await.into(),
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
}

#[async_trait::async_trait]
impl Handler for RequestHandler {
    async fn start(&mut self) {
        while let Some(request) = self.incoming_route.recv().await {
            let response = self.handle_request(request).await;
            match response {
                Ok(response) => {
                    if !self.connection.enqueue(response).await {
                        break;
                    }
                }
                Err(e) => {
                    debug!("GRPC: Request handling error {} for client {}", e, self.connection);
                }
            }
        }
        debug!("GRPC: exiting request handler {:?} for client {}", self.rpc_op, self.connection);
    }
}

pub struct SubscriptionHandler {
    rpc_op: RpcApiOps,
    connection: Connection,
    notifier: Arc<GrpcNotifier>,
    listener_id: ListenerId,
    incoming_route: IncomingRoute,
}

impl SubscriptionHandler {
    pub fn new(
        rpc_op: RpcApiOps,
        connection: Connection,
        notifier: Arc<GrpcNotifier>,
        listener_id: ListenerId,
        incoming_route: IncomingRoute,
    ) -> Self {
        Self { rpc_op, connection, notifier, listener_id, incoming_route }
    }

    pub async fn handle_subscription(&self, request: KaspadRequest) -> GrpcServerResult<KaspadResponse> {
        let mut response: KaspadResponse = if let Some(payload) = request.payload {
            match payload {
                Payload::NotifyBlockAddedRequest(ref request) => match kaspa_rpc_core::NotifyBlockAddedRequest::try_from(request) {
                    Ok(request) => {
                        let result = self
                            .notifier
                            .clone()
                            .execute_subscribe_command(
                                self.listener_id,
                                Scope::BlockAdded(BlockAddedScope::default()),
                                request.command,
                            )
                            .await;
                        NotifyBlockAddedResponseMessage::from(result).into()
                    }
                    Err(err) => NotifyBlockAddedResponseMessage::from(err).into(),
                },

                Payload::NotifyVirtualChainChangedRequest(ref request) => {
                    match kaspa_rpc_core::NotifyVirtualChainChangedRequest::try_from(request) {
                        Ok(request) => {
                            let result = self
                                .notifier
                                .clone()
                                .execute_subscribe_command(
                                    self.listener_id,
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
                            let result = self
                                .notifier
                                .clone()
                                .execute_subscribe_command(
                                    self.listener_id,
                                    Scope::FinalityConflict(FinalityConflictScope::default()),
                                    request.command,
                                )
                                .await
                                .and(
                                    self.notifier
                                        .clone()
                                        .execute_subscribe_command(
                                            self.listener_id,
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
                            let result = self
                                .notifier
                                .clone()
                                .execute_subscribe_command(
                                    self.listener_id,
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
                            let result = self
                                .notifier
                                .clone()
                                .execute_subscribe_command(
                                    self.listener_id,
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
                            let result = self
                                .notifier
                                .clone()
                                .execute_subscribe_command(
                                    self.listener_id,
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
                            let result = self
                                .notifier
                                .clone()
                                .execute_subscribe_command(
                                    self.listener_id,
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
                            let result = self
                                .notifier
                                .clone()
                                .execute_subscribe_command(
                                    self.listener_id,
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
                                let result = self
                                    .notifier
                                    .clone()
                                    .execute_subscribe_command(
                                        self.listener_id,
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
                                let result = self
                                    .notifier
                                    .clone()
                                    .execute_subscribe_command(
                                        self.listener_id,
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

#[async_trait::async_trait]
impl Handler for SubscriptionHandler {
    async fn start(&mut self) {
        while let Some(request) = self.incoming_route.recv().await {
            let response = self.handle_subscription(request).await;
            match response {
                Ok(response) => {
                    if !self.connection.enqueue(response).await {
                        break;
                    }
                }
                Err(e) => {
                    debug!("GRPC: Request handling error {} for client {}", e, self.connection);
                }
            }
        }
        debug!("GRPC: exiting subscription handler {:?} for client {}", self.rpc_op, self.connection);
    }
}
