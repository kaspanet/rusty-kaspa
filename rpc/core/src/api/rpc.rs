//! The client API
//!
//! Rpc = External RPC Service
//! All data provided by the RCP server can be trusted by the client
//! No data submitted by the client to the server can be trusted

use crate::{
    model::*,
    notify::{
        channel::NotificationChannel,
        listener::{ListenerID, ListenerReceiverSide},
    },
    NotificationType, RpcResult,
};
use async_trait::async_trait;

/// Client RPC Api
///
/// The [`RpcApi`] trait defines RPC calls taking a request message as unique parameter.
///
/// For each RPC call a matching readily implemented function taking detailed parameters is also provided.
#[async_trait]
pub trait RpcApi: Sync + Send {
    // async fn ping(
    //     &self,
    //     msg : String
    // ) -> RpcResult<String>;

    ///
    async fn ping(&self) -> RpcResult<PingResponse> {
        Ok(self.ping_call(PingRequest {}).await?)
    }
    async fn ping_call(&self, request: PingRequest) -> RpcResult<PingResponse>;

    ///
    async fn get_process_metrics(&self) -> RpcResult<GetProcessMetricsResponse> {
        Ok(self.get_process_metrics_call(GetProcessMetricsRequest {}).await?)
    }
    async fn get_process_metrics_call(&self, request: GetProcessMetricsRequest) -> RpcResult<GetProcessMetricsResponse>;

    ///
    async fn get_current_network(&self) -> RpcResult<RpcNetworkType> {
        Ok(self.get_current_network_call(GetCurrentNetworkRequest {}).await?.network)
    }
    async fn get_current_network_call(&self, request: GetCurrentNetworkRequest) -> RpcResult<GetCurrentNetworkResponse>;

    /// Submit a block into the DAG.
    /// Blocks are generally expected to have been generated using the get_block_template call.
    async fn submit_block(&self, block: RpcBlock, allow_non_daa_blocks: bool) -> RpcResult<SubmitBlockResponse> {
        self.submit_block_call(SubmitBlockRequest::new(block, allow_non_daa_blocks)).await
    }
    async fn submit_block_call(&self, request: SubmitBlockRequest) -> RpcResult<SubmitBlockResponse>;

    /// Request a current block template.
    /// Callers are expected to solve the block template and submit it using the submit_block call.
    async fn get_block_template(&self, pay_address: RpcAddress, extra_data: RpcExtraData) -> RpcResult<GetBlockTemplateResponse> {
        self.get_block_template_call(GetBlockTemplateRequest::new(pay_address, extra_data)).await
    }
    async fn get_block_template_call(&self, request: GetBlockTemplateRequest) -> RpcResult<GetBlockTemplateResponse>;

    ///
    async fn get_peer_addresses(&self) -> RpcResult<GetPeerAddressesResponse> {
        self.get_peer_addresses_call(GetPeerAddressesRequest {}).await
    }
    async fn get_peer_addresses_call(&self, request: GetPeerAddressesRequest) -> RpcResult<GetPeerAddressesResponse>;

    ///
    async fn get_selected_tip_hash(&self) -> RpcResult<GetSelectedTipHashResponse> {
        self.get_selected_tip_hash_call(GetSelectedTipHashRequest {}).await
    }
    async fn get_selected_tip_hash_call(&self, request: GetSelectedTipHashRequest) -> RpcResult<GetSelectedTipHashResponse>;

    ///
    async fn get_mempool_entry_call(&self, request: GetMempoolEntryRequest) -> RpcResult<GetMempoolEntryResponse>;

    ///
    async fn get_mempool_entries_call(&self, request: GetMempoolEntriesRequest) -> RpcResult<GetMempoolEntriesResponse>;

    ///
    async fn get_connected_peer_info(&self) -> RpcResult<GetConnectedPeerInfoResponse> {
        self.get_connected_peer_info_call(GetConnectedPeerInfoRequest {}).await
    }
    async fn get_connected_peer_info_call(&self, request: GetConnectedPeerInfoRequest) -> RpcResult<GetConnectedPeerInfoResponse>;

    ///
    async fn add_peer_call(&self, request: AddPeerRequest) -> RpcResult<AddPeerResponse>;

    ///
    async fn submit_transaction(&self, transaction: RpcTransaction, allow_orphan: bool) -> RpcResult<SubmitTransactionResponse> {
        self.submit_transaction_call(SubmitTransactionRequest { transaction, allow_orphan }).await
    }
    async fn submit_transaction_call(&self, request: SubmitTransactionRequest) -> RpcResult<SubmitTransactionResponse>;

    /// Requests information about a specific block.
    async fn get_block(&self, hash: RpcHash, include_transactions: bool) -> RpcResult<GetBlockResponse> {
        self.get_block_call(GetBlockRequest::new(hash, include_transactions)).await
    }
    async fn get_block_call(&self, request: GetBlockRequest) -> RpcResult<GetBlockResponse>;

    ///
    async fn get_subnetwork_call(&self, request: GetSubnetworkRequest) -> RpcResult<GetSubnetworkResponse>;

    ///
    async fn get_virtual_selected_parent_chain_from_block_call(
        &self,
        request: GetVirtualSelectedParentChainFromBlockRequest,
    ) -> RpcResult<GetVirtualSelectedParentChainFromBlockResponse>;

    ///
    async fn get_blocks_call(&self, request: GetBlocksRequest) -> RpcResult<GetBlocksResponse>;

    ///
    async fn get_block_count(&self) -> RpcResult<GetBlockCountResponse> {
        self.get_block_count_call(GetBlockCountRequest {}).await
    }
    async fn get_block_count_call(&self, request: GetBlockCountRequest) -> RpcResult<GetBlockCountResponse>;

    ///
    async fn get_block_dag_info(&self) -> RpcResult<GetBlockDagInfoResponse> {
        self.get_block_dag_info_call(GetBlockDagInfoRequest {}).await
    }
    async fn get_block_dag_info_call(&self, request: GetBlockDagInfoRequest) -> RpcResult<GetBlockDagInfoResponse>;

    ///
    async fn resolve_finality_conflict_call(
        &self,
        request: ResolveFinalityConflictRequest,
    ) -> RpcResult<ResolveFinalityConflictResponse>;

    ///
    async fn shutdown(&self) -> RpcResult<ShutdownResponse> {
        self.shutdown_call(ShutdownRequest {}).await
    }
    async fn shutdown_call(&self, request: ShutdownRequest) -> RpcResult<ShutdownResponse>;

    ///
    async fn get_headers_call(&self, request: GetHeadersRequest) -> RpcResult<GetHeadersResponse>;

    ///
    async fn get_balance_by_address_call(&self, request: GetBalanceByAddressRequest) -> RpcResult<GetBalanceByAddressResponse>;

    ///
    async fn get_balances_by_addresses_call(
        &self,
        request: GetBalancesByAddressesRequest,
    ) -> RpcResult<GetBalancesByAddressesResponse>;

    ///
    async fn get_utxos_by_addresses_call(&self, request: GetUtxosByAddressesRequest) -> RpcResult<GetUtxosByAddressesResponse>;

    ///
    async fn get_virtual_selected_parent_blue_score(&self) -> RpcResult<GetVirtualSelectedParentBlueScoreResponse> {
        self.get_virtual_selected_parent_blue_score_call(GetVirtualSelectedParentBlueScoreRequest {}).await
    }
    async fn get_virtual_selected_parent_blue_score_call(
        &self,
        request: GetVirtualSelectedParentBlueScoreRequest,
    ) -> RpcResult<GetVirtualSelectedParentBlueScoreResponse>;

    ///
    async fn ban_call(&self, request: BanRequest) -> RpcResult<BanResponse>;

    ///
    async fn unban_call(&self, request: UnbanRequest) -> RpcResult<UnbanResponse>;

    ///
    async fn get_info_call(&self, request: GetInfoRequest) -> RpcResult<GetInfoResponse>;
    async fn get_info(&self) -> RpcResult<GetInfoResponse> {
        self.get_info_call(GetInfoRequest {}).await
    }

    ///
    async fn estimate_network_hashes_per_second_call(
        &self,
        request: EstimateNetworkHashesPerSecondRequest,
    ) -> RpcResult<EstimateNetworkHashesPerSecondResponse>;

    ///
    async fn get_mempool_entries_by_addresses_call(
        &self,
        request: GetMempoolEntriesByAddressesRequest,
    ) -> RpcResult<GetMempoolEntriesByAddressesResponse>;

    ///
    async fn get_coin_supply(&self) -> RpcResult<GetCoinSupplyResponse> {
        self.get_coin_supply_call(GetCoinSupplyRequest {}).await
    }
    async fn get_coin_supply_call(&self, request: GetCoinSupplyRequest) -> RpcResult<GetCoinSupplyResponse>;

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    /// Register a new listener and returns an id and a channel receiver.
    fn register_new_listener(&self, channel: Option<NotificationChannel>) -> ListenerReceiverSide;

    /// Unregister an existing listener.
    ///
    /// Stop all notifications for this listener and drop its channel.
    async fn unregister_listener(&self, id: ListenerID) -> RpcResult<()>;

    /// Start sending notifications of some type to a listener.
    async fn start_notify(&self, id: ListenerID, notification_type: NotificationType) -> RpcResult<()>;

    /// Stop sending notifications of some type to a listener.
    async fn stop_notify(&self, id: ListenerID, notification_type: NotificationType) -> RpcResult<()>;
}
