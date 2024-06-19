//! The client API
//!
//! Rpc = External RPC Service
//! All data provided by the RCP server can be trusted by the client
//! No data submitted by the client to the server can be trusted

use crate::api::connection::{RpcConnection};
use crate::{model::*, notify::connection::ChannelConnection, RpcResult};
use async_trait::async_trait;
use downcast::{downcast_sync, AnySync};
use enum_dispatch::enum_dispatch;
use kaspa_notify::{listener::ListenerId, scope::Scope, subscription::Command};
use std::future::Future;
use std::ops::Deref;
use std::sync::Arc;

pub const MAX_SAFE_WINDOW_SIZE: u32 = 10_000;

#[derive(Clone, Default, Debug)]
pub struct DummyRpcConnection;

impl RpcConnection for DummyRpcConnection {
    fn id(&self) -> u64 {
        panic!("don't call me");
    }
}

impl<T: RpcApi> RpcApi for Arc<T> {
    type RpcConnection = T::RpcConnection;

    async fn ping(&self) -> RpcResult<()> {
        self.deref().ping().await
    }

    async fn ping_call(&self, connection: Option<Self::RpcConnection>, request: PingRequest) -> RpcResult<PingResponse> {
        self.deref().ping_call(connection, request).await
    }

    async fn get_metrics(
        &self,
        process_metrics: bool,
        connection_metrics: bool,
        bandwidth_metrics: bool,
        consensus_metrics: bool,
    ) -> RpcResult<GetMetricsResponse> {
        self.deref().get_metrics(process_metrics, connection_metrics, bandwidth_metrics, consensus_metrics).await
    }

    async fn get_metrics_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetMetricsRequest,
    ) -> RpcResult<GetMetricsResponse> {
        self.deref().get_metrics_call(connection, request).await
    }

    async fn get_server_info(&self) -> RpcResult<GetServerInfoResponse> {
        self.deref().get_server_info().await
    }

    async fn get_server_info_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetServerInfoRequest,
    ) -> RpcResult<GetServerInfoResponse> {
        self.deref().get_server_info_call(connection, request).await
    }

    async fn get_sync_status(&self) -> RpcResult<bool> {
        self.deref().get_sync_status().await
    }

    async fn get_sync_status_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetSyncStatusRequest,
    ) -> RpcResult<GetSyncStatusResponse> {
        self.deref().get_sync_status_call(connection, request).await
    }

    async fn get_current_network(&self) -> RpcResult<RpcNetworkType> {
        self.deref().get_current_network().await
    }

    async fn get_current_network_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetCurrentNetworkRequest,
    ) -> RpcResult<GetCurrentNetworkResponse> {
        self.deref().get_current_network_call(connection, request).await
    }

    async fn submit_block(&self, block: RpcBlock, allow_non_daa_blocks: bool) -> RpcResult<SubmitBlockResponse> {
        self.deref().submit_block(block, allow_non_daa_blocks).await
    }

    async fn submit_block_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: SubmitBlockRequest,
    ) -> RpcResult<SubmitBlockResponse> {
        self.deref().submit_block_call(connection, request).await
    }

    async fn get_block_template(&self, pay_address: RpcAddress, extra_data: RpcExtraData) -> RpcResult<GetBlockTemplateResponse> {
        self.deref().get_block_template(pay_address, extra_data).await
    }

    async fn get_block_template_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetBlockTemplateRequest,
    ) -> RpcResult<GetBlockTemplateResponse> {
        self.deref().get_block_template_call(connection, request).await
    }

    async fn get_peer_addresses(&self) -> RpcResult<GetPeerAddressesResponse> {
        self.deref().get_peer_addresses().await
    }

    async fn get_peer_addresses_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetPeerAddressesRequest,
    ) -> RpcResult<GetPeerAddressesResponse> {
        self.deref().get_peer_addresses_call(connection, request).await
    }

    async fn get_sink(&self) -> RpcResult<GetSinkResponse> {
        self.deref().get_sink().await
    }

    async fn get_sink_call(&self, connection: Option<Self::RpcConnection>, request: GetSinkRequest) -> RpcResult<GetSinkResponse> {
        self.deref().get_sink_call(connection, request).await
    }

    async fn get_mempool_entry(
        &self,
        transaction_id: RpcTransactionId,
        include_orphan_pool: bool,
        filter_transaction_pool: bool,
    ) -> RpcResult<RpcMempoolEntry> {
        self.deref().get_mempool_entry(transaction_id, include_orphan_pool, filter_transaction_pool).await
    }

    async fn get_mempool_entry_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetMempoolEntryRequest,
    ) -> RpcResult<GetMempoolEntryResponse> {
        self.deref().get_mempool_entry_call(connection, request).await
    }

    async fn get_mempool_entries(&self, include_orphan_pool: bool, filter_transaction_pool: bool) -> RpcResult<Vec<RpcMempoolEntry>> {
        self.deref().get_mempool_entries(include_orphan_pool, filter_transaction_pool).await
    }

    async fn get_mempool_entries_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetMempoolEntriesRequest,
    ) -> RpcResult<GetMempoolEntriesResponse> {
        self.deref().get_mempool_entries_call(connection, request).await
    }

    async fn get_connected_peer_info(&self) -> RpcResult<GetConnectedPeerInfoResponse> {
        self.deref().get_connected_peer_info().await
    }

    async fn get_connected_peer_info_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetConnectedPeerInfoRequest,
    ) -> RpcResult<GetConnectedPeerInfoResponse> {
        self.deref().get_connected_peer_info_call(connection, request).await
    }

    async fn add_peer(&self, peer_address: RpcContextualPeerAddress, is_permanent: bool) -> RpcResult<()> {
        self.deref().add_peer(peer_address, is_permanent).await
    }

    async fn add_peer_call(&self, connection: Option<Self::RpcConnection>, request: AddPeerRequest) -> RpcResult<AddPeerResponse> {
        self.deref().add_peer_call(connection, request).await
    }

    async fn submit_transaction(&self, transaction: RpcTransaction, allow_orphan: bool) -> RpcResult<RpcTransactionId> {
        self.deref().submit_transaction(transaction, allow_orphan).await
    }

    async fn submit_transaction_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: SubmitTransactionRequest,
    ) -> RpcResult<SubmitTransactionResponse> {
        self.deref().submit_transaction_call(connection, request).await
    }

    async fn get_block(&self, hash: RpcHash, include_transactions: bool) -> RpcResult<RpcBlock> {
        self.deref().get_block(hash, include_transactions).await
    }

    async fn get_block_call(&self, connection: Option<Self::RpcConnection>, request: GetBlockRequest) -> RpcResult<GetBlockResponse> {
        self.deref().get_block_call(connection, request).await
    }

    async fn get_subnetwork(&self, subnetwork_id: RpcSubnetworkId) -> RpcResult<GetSubnetworkResponse> {
        self.deref().get_subnetwork(subnetwork_id).await
    }

    async fn get_subnetwork_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetSubnetworkRequest,
    ) -> RpcResult<GetSubnetworkResponse> {
        self.deref().get_subnetwork_call(connection, request).await
    }

    async fn get_virtual_chain_from_block(
        &self,
        start_hash: RpcHash,
        include_accepted_transaction_ids: bool,
    ) -> RpcResult<GetVirtualChainFromBlockResponse> {
        self.deref().get_virtual_chain_from_block(start_hash, include_accepted_transaction_ids).await
    }

    async fn get_virtual_chain_from_block_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetVirtualChainFromBlockRequest,
    ) -> RpcResult<GetVirtualChainFromBlockResponse> {
        self.deref().get_virtual_chain_from_block_call(connection, request).await
    }

    async fn get_blocks(
        &self,
        low_hash: Option<RpcHash>,
        include_blocks: bool,
        include_transactions: bool,
    ) -> RpcResult<GetBlocksResponse> {
        self.deref().get_blocks(low_hash, include_blocks, include_transactions).await
    }

    async fn get_blocks_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetBlocksRequest,
    ) -> RpcResult<GetBlocksResponse> {
        self.deref().get_blocks_call(connection, request).await
    }

    async fn get_block_count(&self) -> RpcResult<GetBlockCountResponse> {
        self.deref().get_block_count().await
    }

    async fn get_block_count_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetBlockCountRequest,
    ) -> RpcResult<GetBlockCountResponse> {
        self.deref().get_block_count_call(connection, request).await
    }

    async fn get_block_dag_info(&self) -> RpcResult<GetBlockDagInfoResponse> {
        self.deref().get_block_dag_info().await
    }

    async fn get_block_dag_info_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetBlockDagInfoRequest,
    ) -> RpcResult<GetBlockDagInfoResponse> {
        self.deref().get_block_dag_info_call(connection, request).await
    }

    async fn resolve_finality_conflict(&self, finality_block_hash: RpcHash) -> RpcResult<()> {
        self.deref().resolve_finality_conflict(finality_block_hash).await
    }

    async fn resolve_finality_conflict_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: ResolveFinalityConflictRequest,
    ) -> RpcResult<ResolveFinalityConflictResponse> {
        self.deref().resolve_finality_conflict_call(connection, request).await
    }

    async fn shutdown(&self) -> RpcResult<()> {
        self.deref().shutdown().await
    }

    async fn shutdown_call(&self, connection: Option<Self::RpcConnection>, request: ShutdownRequest) -> RpcResult<ShutdownResponse> {
        self.deref().shutdown_call(connection, request).await
    }

    async fn get_headers(&self, start_hash: RpcHash, limit: u64, is_ascending: bool) -> RpcResult<Vec<RpcHeader>> {
        self.deref().get_headers(start_hash, limit, is_ascending).await
    }

    async fn get_headers_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetHeadersRequest,
    ) -> RpcResult<GetHeadersResponse> {
        self.deref().get_headers_call(connection, request).await
    }

    async fn get_balance_by_address(&self, address: RpcAddress) -> RpcResult<u64> {
        self.deref().get_balance_by_address(address).await
    }

    async fn get_balance_by_address_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetBalanceByAddressRequest,
    ) -> RpcResult<GetBalanceByAddressResponse> {
        self.deref().get_balance_by_address_call(connection, request).await
    }

    async fn get_balances_by_addresses(&self, addresses: Vec<RpcAddress>) -> RpcResult<Vec<RpcBalancesByAddressesEntry>> {
        self.deref().get_balances_by_addresses(addresses).await
    }

    async fn get_balances_by_addresses_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetBalancesByAddressesRequest,
    ) -> RpcResult<GetBalancesByAddressesResponse> {
        self.deref().get_balances_by_addresses_call(connection, request).await
    }

    async fn get_utxos_by_addresses(&self, addresses: Vec<RpcAddress>) -> RpcResult<Vec<RpcUtxosByAddressesEntry>> {
        self.deref().get_utxos_by_addresses(addresses).await
    }

    async fn get_utxos_by_addresses_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetUtxosByAddressesRequest,
    ) -> RpcResult<GetUtxosByAddressesResponse> {
        self.deref().get_utxos_by_addresses_call(connection, request).await
    }

    async fn get_sink_blue_score(&self) -> RpcResult<u64> {
        self.deref().get_sink_blue_score().await
    }

    async fn get_sink_blue_score_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetSinkBlueScoreRequest,
    ) -> RpcResult<GetSinkBlueScoreResponse> {
        self.deref().get_sink_blue_score_call(connection, request).await
    }

    async fn ban(&self, ip: RpcIpAddress) -> RpcResult<()> {
        self.deref().ban(ip).await
    }

    async fn ban_call(&self, connection: Option<Self::RpcConnection>, request: BanRequest) -> RpcResult<BanResponse> {
        self.deref().ban_call(connection, request).await
    }

    async fn unban(&self, ip: RpcIpAddress) -> RpcResult<()> {
        self.deref().unban(ip).await
    }

    async fn unban_call(&self, connection: Option<Self::RpcConnection>, request: UnbanRequest) -> RpcResult<UnbanResponse> {
        self.deref().unban_call(connection, request).await
    }

    async fn get_info(&self) -> RpcResult<GetInfoResponse> {
        self.deref().get_info().await
    }

    async fn get_info_call(&self, connection: Option<Self::RpcConnection>, request: GetInfoRequest) -> RpcResult<GetInfoResponse> {
        self.deref().get_info_call(connection, request).await
    }

    async fn estimate_network_hashes_per_second(&self, window_size: u32, start_hash: Option<RpcHash>) -> RpcResult<u64> {
        self.deref().estimate_network_hashes_per_second(window_size, start_hash).await
    }

    async fn estimate_network_hashes_per_second_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: EstimateNetworkHashesPerSecondRequest,
    ) -> RpcResult<EstimateNetworkHashesPerSecondResponse> {
        self.deref().estimate_network_hashes_per_second_call(connection, request).await
    }

    async fn get_mempool_entries_by_addresses(
        &self,
        addresses: Vec<RpcAddress>,
        include_orphan_pool: bool,
        filter_transaction_pool: bool,
    ) -> RpcResult<Vec<RpcMempoolEntryByAddress>> {
        self.deref().get_mempool_entries_by_addresses(addresses, include_orphan_pool, filter_transaction_pool).await
    }

    async fn get_mempool_entries_by_addresses_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetMempoolEntriesByAddressesRequest,
    ) -> RpcResult<GetMempoolEntriesByAddressesResponse> {
        self.deref().get_mempool_entries_by_addresses_call(connection, request).await
    }

    async fn get_coin_supply(&self) -> RpcResult<GetCoinSupplyResponse> {
        self.deref().get_coin_supply().await
    }

    async fn get_coin_supply_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetCoinSupplyRequest,
    ) -> RpcResult<GetCoinSupplyResponse> {
        self.deref().get_coin_supply_call(connection, request).await
    }

    async fn get_daa_score_timestamp_estimate(&self, daa_scores: Vec<u64>) -> RpcResult<Vec<u64>> {
        self.deref().get_daa_score_timestamp_estimate(daa_scores).await
    }

    async fn get_daa_score_timestamp_estimate_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetDaaScoreTimestampEstimateRequest,
    ) -> RpcResult<GetDaaScoreTimestampEstimateResponse> {
        self.deref().get_daa_score_timestamp_estimate_call(connection, request).await
    }

    fn register_new_listener(&self, connection: ChannelConnection) -> ListenerId {
        self.deref().register_new_listener(connection)
    }

    async fn unregister_listener(&self, id: ListenerId) -> RpcResult<()> {
        self.deref().unregister_listener(id).await
    }

    async fn start_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        self.deref().start_notify(id, scope).await
    }

    async fn stop_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()> {
        self.deref().stop_notify(id, scope).await
    }

    async fn execute_subscribe_command(&self, id: ListenerId, scope: Scope, command: Command) -> RpcResult<()> {
        self.deref().execute_subscribe_command(id, scope, command).await
    }
}

/// Client RPC Api
///
/// The [`RpcApi`] trait defines RPC calls taking a request message as unique parameter.
///
/// For each RPC call a matching readily implemented function taking detailed parameters is also provided.
#[enum_dispatch]
pub trait RpcApi: Sync + Send + AnySync {
    type RpcConnection: RpcConnection;

    fn ping(&self) -> impl Future<Output = RpcResult<()>> + Send {
        async move {
            self.ping_call(Default::default(), PingRequest {}).await?;
            Ok(())
        }
    }
    fn ping_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: PingRequest,
    ) -> impl Future<Output = RpcResult<PingResponse>> + Send;

    fn get_metrics(
        &self,
        process_metrics: bool,
        connection_metrics: bool,
        bandwidth_metrics: bool,
        consensus_metrics: bool,
    ) -> impl Future<Output = RpcResult<GetMetricsResponse>> + Send {
        async move {
            self.get_metrics_call(
                Default::default(),
                GetMetricsRequest { process_metrics, connection_metrics, bandwidth_metrics, consensus_metrics },
            )
            .await
        }
    }
    fn get_metrics_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetMetricsRequest,
    ) -> impl Future<Output = RpcResult<GetMetricsResponse>> + Send;

    fn get_server_info(&self) -> impl Future<Output = RpcResult<GetServerInfoResponse>> + Send {
        async move { self.get_server_info_call(Default::default(), GetServerInfoRequest {}).await }
    }
    fn get_server_info_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetServerInfoRequest,
    ) -> impl Future<Output = RpcResult<GetServerInfoResponse>> + Send;

    fn get_sync_status(&self) -> impl Future<Output = RpcResult<bool>> + Send {
        async move { Ok(self.get_sync_status_call(Default::default(), GetSyncStatusRequest {}).await?.is_synced) }
    }
    fn get_sync_status_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetSyncStatusRequest,
    ) -> impl Future<Output = RpcResult<GetSyncStatusResponse>> + Send;

    fn get_current_network(&self) -> impl Future<Output = RpcResult<RpcNetworkType>> + Send {
        async move { Ok(self.get_current_network_call(Default::default(), GetCurrentNetworkRequest {}).await?.network) }
    }
    fn get_current_network_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetCurrentNetworkRequest,
    ) -> impl Future<Output = RpcResult<GetCurrentNetworkResponse>> + Send;

    fn submit_block(
        &self,
        block: RpcBlock,
        allow_non_daa_blocks: bool,
    ) -> impl Future<Output = RpcResult<SubmitBlockResponse>> + Send {
        async move { self.submit_block_call(Default::default(), SubmitBlockRequest::new(block, allow_non_daa_blocks)).await }
    }
    fn submit_block_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: SubmitBlockRequest,
    ) -> impl Future<Output = RpcResult<SubmitBlockResponse>> + Send;

    fn get_block_template(
        &self,
        pay_address: RpcAddress,
        extra_data: RpcExtraData,
    ) -> impl Future<Output = RpcResult<GetBlockTemplateResponse>> + Send {
        async move { self.get_block_template_call(Default::default(), GetBlockTemplateRequest::new(pay_address, extra_data)).await }
    }
    fn get_block_template_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetBlockTemplateRequest,
    ) -> impl Future<Output = RpcResult<GetBlockTemplateResponse>> + Send;

    fn get_peer_addresses(&self) -> impl Future<Output = RpcResult<GetPeerAddressesResponse>> + Send {
        async move { self.get_peer_addresses_call(Default::default(), GetPeerAddressesRequest {}).await }
    }
    fn get_peer_addresses_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetPeerAddressesRequest,
    ) -> impl Future<Output = RpcResult<GetPeerAddressesResponse>> + Send;

    fn get_sink(&self) -> impl Future<Output = RpcResult<GetSinkResponse>> + Send {
        async move { self.get_sink_call(Default::default(), GetSinkRequest {}).await }
    }
    fn get_sink_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetSinkRequest,
    ) -> impl Future<Output = RpcResult<GetSinkResponse>> + Send;

    fn get_mempool_entry(
        &self,
        transaction_id: RpcTransactionId,
        include_orphan_pool: bool,
        filter_transaction_pool: bool,
    ) -> impl Future<Output = RpcResult<RpcMempoolEntry>> + Send {
        async move {
            Ok(self
                .get_mempool_entry_call(
                    Default::default(),
                    GetMempoolEntryRequest::new(transaction_id, include_orphan_pool, filter_transaction_pool),
                )
                .await?
                .mempool_entry)
        }
    }
    fn get_mempool_entry_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetMempoolEntryRequest,
    ) -> impl Future<Output = RpcResult<GetMempoolEntryResponse>> + Send;

    fn get_mempool_entries(
        &self,
        include_orphan_pool: bool,
        filter_transaction_pool: bool,
    ) -> impl Future<Output = RpcResult<Vec<RpcMempoolEntry>>> + Send {
        async move {
            Ok(self
                .get_mempool_entries_call(
                    Default::default(),
                    GetMempoolEntriesRequest::new(include_orphan_pool, filter_transaction_pool),
                )
                .await?
                .mempool_entries)
        }
    }
    fn get_mempool_entries_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetMempoolEntriesRequest,
    ) -> impl Future<Output = RpcResult<GetMempoolEntriesResponse>> + Send;

    fn get_connected_peer_info(&self) -> impl Future<Output = RpcResult<GetConnectedPeerInfoResponse>> + Send {
        async move { self.get_connected_peer_info_call(Default::default(), GetConnectedPeerInfoRequest {}).await }
    }
    fn get_connected_peer_info_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetConnectedPeerInfoRequest,
    ) -> impl Future<Output = RpcResult<GetConnectedPeerInfoResponse>> + Send;

    fn add_peer(&self, peer_address: RpcContextualPeerAddress, is_permanent: bool) -> impl Future<Output = RpcResult<()>> + Send {
        async move {
            self.add_peer_call(Default::default(), AddPeerRequest::new(peer_address, is_permanent)).await?;
            Ok(())
        }
    }
    fn add_peer_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: AddPeerRequest,
    ) -> impl Future<Output = RpcResult<AddPeerResponse>> + Send;

    fn submit_transaction(
        &self,
        transaction: RpcTransaction,
        allow_orphan: bool,
    ) -> impl Future<Output = RpcResult<RpcTransactionId>> + Send {
        async move {
            Ok(self
                .submit_transaction_call(Default::default(), SubmitTransactionRequest { transaction, allow_orphan })
                .await?
                .transaction_id)
        }
    }
    fn submit_transaction_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: SubmitTransactionRequest,
    ) -> impl Future<Output = RpcResult<SubmitTransactionResponse>> + Send;

    fn get_block(&self, hash: RpcHash, include_transactions: bool) -> impl Future<Output = RpcResult<RpcBlock>> + Send {
        async move { Ok(self.get_block_call(Default::default(), GetBlockRequest::new(hash, include_transactions)).await?.block) }
    }
    fn get_block_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetBlockRequest,
    ) -> impl Future<Output = RpcResult<GetBlockResponse>> + Send;

    fn get_subnetwork(&self, subnetwork_id: RpcSubnetworkId) -> impl Future<Output = RpcResult<GetSubnetworkResponse>> + Send {
        async move { self.get_subnetwork_call(Default::default(), GetSubnetworkRequest::new(subnetwork_id)).await }
    }
    fn get_subnetwork_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetSubnetworkRequest,
    ) -> impl Future<Output = RpcResult<GetSubnetworkResponse>> + Send;

    fn get_virtual_chain_from_block(
        &self,
        start_hash: RpcHash,
        include_accepted_transaction_ids: bool,
    ) -> impl Future<Output = RpcResult<GetVirtualChainFromBlockResponse>> + Send {
        async move {
            self.get_virtual_chain_from_block_call(
                Default::default(),
                GetVirtualChainFromBlockRequest::new(start_hash, include_accepted_transaction_ids),
            )
            .await
        }
    }
    fn get_virtual_chain_from_block_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetVirtualChainFromBlockRequest,
    ) -> impl Future<Output = RpcResult<GetVirtualChainFromBlockResponse>> + Send;

    fn get_blocks(
        &self,
        low_hash: Option<RpcHash>,
        include_blocks: bool,
        include_transactions: bool,
    ) -> impl Future<Output = RpcResult<GetBlocksResponse>> + Send {
        async move { self.get_blocks_call(Default::default(), GetBlocksRequest::new(low_hash, include_blocks, include_transactions)).await }
    }
    fn get_blocks_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetBlocksRequest,
    ) -> impl Future<Output = RpcResult<GetBlocksResponse>> + Send;

    fn get_block_count(&self) -> impl Future<Output = RpcResult<GetBlockCountResponse>> + Send {
        async move { self.get_block_count_call(Default::default(), GetBlockCountRequest {}).await }
    }
    fn get_block_count_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetBlockCountRequest,
    ) -> impl Future<Output = RpcResult<GetBlockCountResponse>> + Send;

    fn get_block_dag_info(&self) -> impl Future<Output = RpcResult<GetBlockDagInfoResponse>> + Send {
        async move { self.get_block_dag_info_call(Default::default(), GetBlockDagInfoRequest {}).await }
    }
    fn get_block_dag_info_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetBlockDagInfoRequest,
    ) -> impl Future<Output = RpcResult<GetBlockDagInfoResponse>> + Send;

    fn resolve_finality_conflict(&self, finality_block_hash: RpcHash) -> impl Future<Output = RpcResult<()>> + Send {
        async move {
            self.resolve_finality_conflict_call(Default::default(), ResolveFinalityConflictRequest::new(finality_block_hash)).await?;
            Ok(())
        }
    }
    fn resolve_finality_conflict_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: ResolveFinalityConflictRequest,
    ) -> impl Future<Output = RpcResult<ResolveFinalityConflictResponse>> + Send;

    fn shutdown(&self) -> impl Future<Output = RpcResult<()>> + Send {
        async move {
            self.shutdown_call(Default::default(), ShutdownRequest {}).await?;
            Ok(())
        }
    }
    fn shutdown_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: ShutdownRequest,
    ) -> impl Future<Output = RpcResult<ShutdownResponse>> + Send;

    fn get_headers(
        &self,
        start_hash: RpcHash,
        limit: u64,
        is_ascending: bool,
    ) -> impl Future<Output = RpcResult<Vec<RpcHeader>>> + Send {
        async move { Ok(self.get_headers_call(Default::default(), GetHeadersRequest::new(start_hash, limit, is_ascending)).await?.headers) }
    }
    fn get_headers_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetHeadersRequest,
    ) -> impl Future<Output = RpcResult<GetHeadersResponse>> + Send;

    fn get_balance_by_address(&self, address: RpcAddress) -> impl Future<Output = RpcResult<u64>> + Send {
        async move { Ok(self.get_balance_by_address_call(Default::default(), GetBalanceByAddressRequest::new(address)).await?.balance) }
    }
    fn get_balance_by_address_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetBalanceByAddressRequest,
    ) -> impl Future<Output = RpcResult<GetBalanceByAddressResponse>> + Send;

    fn get_balances_by_addresses(
        &self,
        addresses: Vec<RpcAddress>,
    ) -> impl Future<Output = RpcResult<Vec<RpcBalancesByAddressesEntry>>> + Send {
        async move {
            Ok(self.get_balances_by_addresses_call(Default::default(), GetBalancesByAddressesRequest::new(addresses)).await?.entries)
        }
    }
    fn get_balances_by_addresses_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetBalancesByAddressesRequest,
    ) -> impl Future<Output = RpcResult<GetBalancesByAddressesResponse>> + Send;

    fn get_utxos_by_addresses(
        &self,
        addresses: Vec<RpcAddress>,
    ) -> impl Future<Output = RpcResult<Vec<RpcUtxosByAddressesEntry>>> + Send {
        async move { Ok(self.get_utxos_by_addresses_call(Default::default(), GetUtxosByAddressesRequest::new(addresses)).await?.entries) }
    }
    fn get_utxos_by_addresses_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetUtxosByAddressesRequest,
    ) -> impl Future<Output = RpcResult<GetUtxosByAddressesResponse>> + Send;

    fn get_sink_blue_score(&self) -> impl Future<Output = RpcResult<u64>> + Send {
        async move { Ok(self.get_sink_blue_score_call(Default::default(), GetSinkBlueScoreRequest {}).await?.blue_score) }
    }
    fn get_sink_blue_score_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetSinkBlueScoreRequest,
    ) -> impl Future<Output = RpcResult<GetSinkBlueScoreResponse>> + Send;

    fn ban(&self, ip: RpcIpAddress) -> impl Future<Output = RpcResult<()>> + Send {
        async move {
            self.ban_call(Default::default(), BanRequest::new(ip)).await?;
            Ok(())
        }
    }
    fn ban_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: BanRequest,
    ) -> impl Future<Output = RpcResult<BanResponse>> + Send;

    fn unban(&self, ip: RpcIpAddress) -> impl Future<Output = RpcResult<()>> + Send {
        async move {
            self.unban_call(Default::default(), UnbanRequest::new(ip)).await?;
            Ok(())
        }
    }
    fn unban_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: UnbanRequest,
    ) -> impl Future<Output = RpcResult<UnbanResponse>> + Send;

    fn get_info(&self) -> impl Future<Output = RpcResult<GetInfoResponse>> + Send {
        async move { self.get_info_call(Default::default(), GetInfoRequest {}).await }
    }
    fn get_info_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetInfoRequest,
    ) -> impl Future<Output = RpcResult<GetInfoResponse>> + Send;

    fn estimate_network_hashes_per_second(
        &self,
        window_size: u32,
        start_hash: Option<RpcHash>,
    ) -> impl Future<Output = RpcResult<u64>> + Send {
        async move {
            Ok(self
                .estimate_network_hashes_per_second_call(
                    Default::default(),
                    EstimateNetworkHashesPerSecondRequest::new(window_size, start_hash),
                )
                .await?
                .network_hashes_per_second)
        }
    }
    fn estimate_network_hashes_per_second_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: EstimateNetworkHashesPerSecondRequest,
    ) -> impl Future<Output = RpcResult<EstimateNetworkHashesPerSecondResponse>> + Send;

    fn get_mempool_entries_by_addresses(
        &self,
        addresses: Vec<RpcAddress>,
        include_orphan_pool: bool,
        filter_transaction_pool: bool,
    ) -> impl Future<Output = RpcResult<Vec<RpcMempoolEntryByAddress>>> + Send {
        async move {
            Ok(self
                .get_mempool_entries_by_addresses_call(
                    Default::default(),
                    GetMempoolEntriesByAddressesRequest::new(addresses, include_orphan_pool, filter_transaction_pool),
                )
                .await?
                .entries)
        }
    }
    fn get_mempool_entries_by_addresses_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetMempoolEntriesByAddressesRequest,
    ) -> impl Future<Output = RpcResult<GetMempoolEntriesByAddressesResponse>> + Send;

    fn get_coin_supply(&self) -> impl Future<Output = RpcResult<GetCoinSupplyResponse>> + Send {
        async move { self.get_coin_supply_call(Default::default(), GetCoinSupplyRequest {}).await }
    }
    fn get_coin_supply_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetCoinSupplyRequest,
    ) -> impl Future<Output = RpcResult<GetCoinSupplyResponse>> + Send;

    fn get_daa_score_timestamp_estimate(&self, daa_scores: Vec<u64>) -> impl Future<Output = RpcResult<Vec<u64>>> + Send {
        async move {
            Ok(self
                .get_daa_score_timestamp_estimate_call(Default::default(), GetDaaScoreTimestampEstimateRequest { daa_scores })
                .await?
                .timestamps)
        }
    }
    fn get_daa_score_timestamp_estimate_call(
        &self,
        connection: Option<Self::RpcConnection>,
        request: GetDaaScoreTimestampEstimateRequest,
    ) -> impl Future<Output = RpcResult<GetDaaScoreTimestampEstimateResponse>> + Send;

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    /// Register a new listener and returns an id identifying it.
    fn register_new_listener(&self, connection: ChannelConnection) -> ListenerId;

    /// Unregister an existing listener.
    ///
    /// Stop all notifications for this listener, unregister the id and its associated connection.

    fn unregister_listener(&self, id: ListenerId) -> impl Future<Output = RpcResult<()>> + Send;

    /// Start sending notifications of some type to a listener.
    fn start_notify(&self, id: ListenerId, scope: Scope) -> impl Future<Output = RpcResult<()>> + Send;

    /// Stop sending notifications of some type to a listener.
    fn stop_notify(&self, id: ListenerId, scope: Scope) -> impl Future<Output = RpcResult<()>> + Send;

    /// Execute a subscription command leading to either start or stop sending notifications
    /// of some type to a listener.
    fn execute_subscribe_command(&self, id: ListenerId, scope: Scope, command: Command) -> impl Future<Output = RpcResult<()>> + Send {
        async move {
            match command {
                Command::Start => self.start_notify(id, scope).await,
                Command::Stop => self.stop_notify(id, scope).await,
            }
        }
    }
}
pub type DynRpcService<T: RpcApi> = Arc<T>;

// downcast_sync!(dyn RpcApi);
