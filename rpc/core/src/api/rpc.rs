//! The client API
//!
//! Rpc = External RPC Service
//! All data provided by the RCP server can be trusted by the client
//! No data submitted by the client to the server can be trusted

use crate::{model::*, notify::connection::ChannelConnection, RpcResult};
use async_trait::async_trait;
use downcast::{downcast_sync, AnySync};
use kaspa_notify::{listener::ListenerId, scope::Scope, subscription::Command};
use std::sync::Arc;

pub const MAX_SAFE_WINDOW_SIZE: u32 = 10_000;

/// Client RPC Api
///
/// The [`RpcApi`] trait defines RPC calls taking a request message as unique parameter.
///
/// For each RPC call a matching readily implemented function taking detailed parameters is also provided.
#[async_trait]
pub trait RpcApi: Sync + Send + AnySync {
    ///
    async fn ping(&self) -> RpcResult<()> {
        self.ping_call(PingRequest {}).await?;
        Ok(())
    }
    async fn ping_call(&self, request: PingRequest) -> RpcResult<PingResponse>;

    // ---

    async fn get_metrics(
        &self,
        process_metrics: bool,
        connection_metrics: bool,
        bandwidth_metrics: bool,
        consensus_metrics: bool,
    ) -> RpcResult<GetMetricsResponse> {
        self.get_metrics_call(GetMetricsRequest { process_metrics, connection_metrics, bandwidth_metrics, consensus_metrics }).await
    }
    async fn get_metrics_call(&self, request: GetMetricsRequest) -> RpcResult<GetMetricsResponse>;

    // get_info alternative that carries only version, network_id (full), is_synced, virtual_daa_score
    // these are the only variables needed to negotiate a wRPC connection (besides the wRPC handshake)
    async fn get_server_info(&self) -> RpcResult<GetServerInfoResponse> {
        self.get_server_info_call(GetServerInfoRequest {}).await
    }
    async fn get_server_info_call(&self, request: GetServerInfoRequest) -> RpcResult<GetServerInfoResponse>;

    // Get current sync status of the node (should be converted to a notification subscription)
    async fn get_sync_status(&self) -> RpcResult<bool> {
        Ok(self.get_sync_status_call(GetSyncStatusRequest {}).await?.is_synced)
    }
    async fn get_sync_status_call(&self, request: GetSyncStatusRequest) -> RpcResult<GetSyncStatusResponse>;

    // ---

    /// Requests the network the node is currently running against.
    async fn get_current_network(&self) -> RpcResult<RpcNetworkType> {
        Ok(self.get_current_network_call(GetCurrentNetworkRequest {}).await?.network)
    }
    async fn get_current_network_call(&self, request: GetCurrentNetworkRequest) -> RpcResult<GetCurrentNetworkResponse>;

    /// Submit a block into the DAG.
    ///
    /// Blocks are generally expected to have been generated using the get_block_template call.
    async fn submit_block(&self, block: RpcBlock, allow_non_daa_blocks: bool) -> RpcResult<SubmitBlockResponse> {
        self.submit_block_call(SubmitBlockRequest::new(block, allow_non_daa_blocks)).await
    }
    async fn submit_block_call(&self, request: SubmitBlockRequest) -> RpcResult<SubmitBlockResponse>;

    /// Request a current block template.
    ///
    /// Callers are expected to solve the block template and submit it using the submit_block call.
    async fn get_block_template(&self, pay_address: RpcAddress, extra_data: RpcExtraData) -> RpcResult<GetBlockTemplateResponse> {
        self.get_block_template_call(GetBlockTemplateRequest::new(pay_address, extra_data)).await
    }
    async fn get_block_template_call(&self, request: GetBlockTemplateRequest) -> RpcResult<GetBlockTemplateResponse>;

    /// Requests the list of known kaspad addresses in the current network (mainnet, testnet, etc.)
    async fn get_peer_addresses(&self) -> RpcResult<GetPeerAddressesResponse> {
        self.get_peer_addresses_call(GetPeerAddressesRequest {}).await
    }
    async fn get_peer_addresses_call(&self, request: GetPeerAddressesRequest) -> RpcResult<GetPeerAddressesResponse>;

    /// requests the hash of the current virtual's selected parent.
    async fn get_sink(&self) -> RpcResult<GetSinkResponse> {
        self.get_sink_call(GetSinkRequest {}).await
    }
    async fn get_sink_call(&self, request: GetSinkRequest) -> RpcResult<GetSinkResponse>;

    /// Requests information about a specific transaction in the mempool.
    async fn get_mempool_entry(
        &self,
        transaction_id: RpcTransactionId,
        include_orphan_pool: bool,
        filter_transaction_pool: bool,
    ) -> RpcResult<RpcMempoolEntry> {
        Ok(self
            .get_mempool_entry_call(GetMempoolEntryRequest::new(transaction_id, include_orphan_pool, filter_transaction_pool))
            .await?
            .mempool_entry)
    }
    async fn get_mempool_entry_call(&self, request: GetMempoolEntryRequest) -> RpcResult<GetMempoolEntryResponse>;

    /// Requests information about all the transactions currently in the mempool.
    async fn get_mempool_entries(&self, include_orphan_pool: bool, filter_transaction_pool: bool) -> RpcResult<Vec<RpcMempoolEntry>> {
        Ok(self
            .get_mempool_entries_call(GetMempoolEntriesRequest::new(include_orphan_pool, filter_transaction_pool))
            .await?
            .mempool_entries)
    }
    async fn get_mempool_entries_call(&self, request: GetMempoolEntriesRequest) -> RpcResult<GetMempoolEntriesResponse>;

    /// requests information about all the p2p peers currently connected to this node.
    async fn get_connected_peer_info(&self) -> RpcResult<GetConnectedPeerInfoResponse> {
        self.get_connected_peer_info_call(GetConnectedPeerInfoRequest {}).await
    }
    async fn get_connected_peer_info_call(&self, request: GetConnectedPeerInfoRequest) -> RpcResult<GetConnectedPeerInfoResponse>;

    /// Adds a peer to the node's outgoing connection list.
    ///
    /// This will, in most cases, result in the node connecting to said peer.
    async fn add_peer(&self, peer_address: RpcContextualPeerAddress, is_permanent: bool) -> RpcResult<()> {
        self.add_peer_call(AddPeerRequest::new(peer_address, is_permanent)).await?;
        Ok(())
    }
    async fn add_peer_call(&self, request: AddPeerRequest) -> RpcResult<AddPeerResponse>;

    /// Submits a transaction to the mempool.
    async fn submit_transaction(&self, transaction: RpcTransaction, allow_orphan: bool) -> RpcResult<RpcTransactionId> {
        Ok(self.submit_transaction_call(SubmitTransactionRequest { transaction, allow_orphan }).await?.transaction_id)
    }
    async fn submit_transaction_call(&self, request: SubmitTransactionRequest) -> RpcResult<SubmitTransactionResponse>;

    /// Requests information about a specific block.
    async fn get_block(&self, hash: RpcHash, include_transactions: bool) -> RpcResult<RpcBlock> {
        Ok(self.get_block_call(GetBlockRequest::new(hash, include_transactions)).await?.block)
    }
    async fn get_block_call(&self, request: GetBlockRequest) -> RpcResult<GetBlockResponse>;

    /// Requests information about a specific subnetwork.
    async fn get_subnetwork(&self, subnetwork_id: RpcSubnetworkId) -> RpcResult<GetSubnetworkResponse> {
        self.get_subnetwork_call(GetSubnetworkRequest::new(subnetwork_id)).await
    }
    async fn get_subnetwork_call(&self, request: GetSubnetworkRequest) -> RpcResult<GetSubnetworkResponse>;

    /// Requests the virtual selected parent chain from some `start_hash` to this node's current virtual.
    async fn get_virtual_chain_from_block(
        &self,
        start_hash: RpcHash,
        include_accepted_transaction_ids: bool,
    ) -> RpcResult<GetVirtualChainFromBlockResponse> {
        self.get_virtual_chain_from_block_call(GetVirtualChainFromBlockRequest::new(start_hash, include_accepted_transaction_ids))
            .await
    }
    async fn get_virtual_chain_from_block_call(
        &self,
        request: GetVirtualChainFromBlockRequest,
    ) -> RpcResult<GetVirtualChainFromBlockResponse>;

    /// Requests blocks between a certain block `low_hash` up to this node's current virtual.
    async fn get_blocks(
        &self,
        low_hash: Option<RpcHash>,
        include_blocks: bool,
        include_transactions: bool,
    ) -> RpcResult<GetBlocksResponse> {
        self.get_blocks_call(GetBlocksRequest::new(low_hash, include_blocks, include_transactions)).await
    }
    async fn get_blocks_call(&self, request: GetBlocksRequest) -> RpcResult<GetBlocksResponse>;

    /// Requests the current number of blocks in this node.
    ///
    /// Note that this number may decrease as pruning occurs.
    async fn get_block_count(&self) -> RpcResult<GetBlockCountResponse> {
        self.get_block_count_call(GetBlockCountRequest {}).await
    }
    async fn get_block_count_call(&self, request: GetBlockCountRequest) -> RpcResult<GetBlockCountResponse>;

    /// Requests general information about the current state of this node's DAG.
    async fn get_block_dag_info(&self) -> RpcResult<GetBlockDagInfoResponse> {
        self.get_block_dag_info_call(GetBlockDagInfoRequest {}).await
    }
    async fn get_block_dag_info_call(&self, request: GetBlockDagInfoRequest) -> RpcResult<GetBlockDagInfoResponse>;

    ///
    async fn resolve_finality_conflict(&self, finality_block_hash: RpcHash) -> RpcResult<()> {
        self.resolve_finality_conflict_call(ResolveFinalityConflictRequest::new(finality_block_hash)).await?;
        Ok(())
    }
    async fn resolve_finality_conflict_call(
        &self,
        request: ResolveFinalityConflictRequest,
    ) -> RpcResult<ResolveFinalityConflictResponse>;

    /// Shuts down this node.
    async fn shutdown(&self) -> RpcResult<()> {
        self.shutdown_call(ShutdownRequest {}).await?;
        Ok(())
    }
    async fn shutdown_call(&self, request: ShutdownRequest) -> RpcResult<ShutdownResponse>;

    /// Requests headers between the given `start_hash` and the current virtual, up to the given limit.
    async fn get_headers(&self, start_hash: RpcHash, limit: u64, is_ascending: bool) -> RpcResult<Vec<RpcHeader>> {
        Ok(self.get_headers_call(GetHeadersRequest::new(start_hash, limit, is_ascending)).await?.headers)
    }
    async fn get_headers_call(&self, request: GetHeadersRequest) -> RpcResult<GetHeadersResponse>;

    /// Returns the total balance in unspent transactions towards a given address.
    ///
    /// This call is only available when this node was started with `--utxoindex`.
    async fn get_balance_by_address(&self, address: RpcAddress) -> RpcResult<u64> {
        Ok(self.get_balance_by_address_call(GetBalanceByAddressRequest::new(address)).await?.balance)
    }
    async fn get_balance_by_address_call(&self, request: GetBalanceByAddressRequest) -> RpcResult<GetBalanceByAddressResponse>;

    ///
    async fn get_balances_by_addresses(&self, addresses: Vec<RpcAddress>) -> RpcResult<Vec<RpcBalancesByAddressesEntry>> {
        Ok(self.get_balances_by_addresses_call(GetBalancesByAddressesRequest::new(addresses)).await?.entries)
    }
    async fn get_balances_by_addresses_call(
        &self,
        request: GetBalancesByAddressesRequest,
    ) -> RpcResult<GetBalancesByAddressesResponse>;

    /// Requests all current UTXOs for the given node addresses.
    ///
    /// This call is only available when this node was started with `--utxoindex`.
    async fn get_utxos_by_addresses(&self, addresses: Vec<RpcAddress>) -> RpcResult<Vec<RpcUtxosByAddressesEntry>> {
        Ok(self.get_utxos_by_addresses_call(GetUtxosByAddressesRequest::new(addresses)).await?.entries)
    }
    async fn get_utxos_by_addresses_call(&self, request: GetUtxosByAddressesRequest) -> RpcResult<GetUtxosByAddressesResponse>;

    /// Requests the blue score of the current selected parent of the virtual block.
    async fn get_sink_blue_score(&self) -> RpcResult<u64> {
        Ok(self.get_sink_blue_score_call(GetSinkBlueScoreRequest {}).await?.blue_score)
    }
    async fn get_sink_blue_score_call(&self, request: GetSinkBlueScoreRequest) -> RpcResult<GetSinkBlueScoreResponse>;

    /// Bans the given ip.
    async fn ban(&self, ip: RpcIpAddress) -> RpcResult<()> {
        self.ban_call(BanRequest::new(ip)).await?;
        Ok(())
    }
    async fn ban_call(&self, request: BanRequest) -> RpcResult<BanResponse>;

    /// Unbans the given ip.
    async fn unban(&self, ip: RpcIpAddress) -> RpcResult<()> {
        self.unban_call(UnbanRequest::new(ip)).await?;
        Ok(())
    }
    async fn unban_call(&self, request: UnbanRequest) -> RpcResult<UnbanResponse>;

    /// Returns info about the node.
    async fn get_info_call(&self, request: GetInfoRequest) -> RpcResult<GetInfoResponse>;
    async fn get_info(&self) -> RpcResult<GetInfoResponse> {
        self.get_info_call(GetInfoRequest {}).await
    }

    ///
    async fn estimate_network_hashes_per_second(&self, window_size: u32, start_hash: Option<RpcHash>) -> RpcResult<u64> {
        Ok(self
            .estimate_network_hashes_per_second_call(EstimateNetworkHashesPerSecondRequest::new(window_size, start_hash))
            .await?
            .network_hashes_per_second)
    }
    async fn estimate_network_hashes_per_second_call(
        &self,
        request: EstimateNetworkHashesPerSecondRequest,
    ) -> RpcResult<EstimateNetworkHashesPerSecondResponse>;

    ///
    async fn get_mempool_entries_by_addresses(
        &self,
        addresses: Vec<RpcAddress>,
        include_orphan_pool: bool,
        filter_transaction_pool: bool,
    ) -> RpcResult<Vec<RpcMempoolEntryByAddress>> {
        Ok(self
            .get_mempool_entries_by_addresses_call(GetMempoolEntriesByAddressesRequest::new(
                addresses,
                include_orphan_pool,
                filter_transaction_pool,
            ))
            .await?
            .entries)
    }
    async fn get_mempool_entries_by_addresses_call(
        &self,
        request: GetMempoolEntriesByAddressesRequest,
    ) -> RpcResult<GetMempoolEntriesByAddressesResponse>;

    ///
    async fn get_coin_supply(&self) -> RpcResult<GetCoinSupplyResponse> {
        self.get_coin_supply_call(GetCoinSupplyRequest {}).await
    }
    async fn get_coin_supply_call(&self, request: GetCoinSupplyRequest) -> RpcResult<GetCoinSupplyResponse>;

    async fn get_daa_score_timestamp_estimate(&self, daa_scores: Vec<u64>) -> RpcResult<Vec<u64>> {
        Ok(self.get_daa_score_timestamp_estimate_call(GetDaaScoreTimestampEstimateRequest { daa_scores }).await?.timestamps)
    }
    async fn get_daa_score_timestamp_estimate_call(
        &self,
        request: GetDaaScoreTimestampEstimateRequest,
    ) -> RpcResult<GetDaaScoreTimestampEstimateResponse>;

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    /// Register a new listener and returns an id identifying it.
    fn register_new_listener(&self, connection: ChannelConnection) -> ListenerId;

    /// Unregister an existing listener.
    ///
    /// Stop all notifications for this listener, unregister the id and its associated connection.
    async fn unregister_listener(&self, id: ListenerId) -> RpcResult<()>;

    /// Start sending notifications of some type to a listener.
    async fn start_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()>;

    /// Stop sending notifications of some type to a listener.
    async fn stop_notify(&self, id: ListenerId, scope: Scope) -> RpcResult<()>;

    /// Execute a subscription command leading to either start or stop sending notifications
    /// of some type to a listener.
    async fn execute_subscribe_command(&self, id: ListenerId, scope: Scope, command: Command) -> RpcResult<()> {
        match command {
            Command::Start => self.start_notify(id, scope).await,
            Command::Stop => self.stop_notify(id, scope).await,
        }
    }
}

pub type DynRpcService = Arc<dyn RpcApi>;

downcast_sync!(dyn RpcApi);
