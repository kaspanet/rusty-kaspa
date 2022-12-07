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

    // async fn get_current_network(
    //     &self
    // ) -> RpcResult<NetworkType>;

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

    // async fn get_peer_addresses(
    //     &self
    // ) -> RpcResult<GetPeerAddressesResponse>;

    // async fn get_selected_tip_hash(
    //     &self
    // ) -> RpcResult<GetSelectedTipHashResponse>;

    // async fn get_mempool_entry(
    //     &self,
    //     request:  GetMempoolEntryRequest
    // ) -> RpcResult<GetMempoolEntriesResponse>;

    // async fn get_mempool_entries(
    //     &self,
    //     include_orphan_pool: bool,
    //     filter_transaction_pool: bool
    // ) -> RpcResult<GetMempoolEntriesResponse>;

    // async fn get_connected_peer_info(
    //     &self
    // ) -> RpcResult<GetConnectedPeerInfoResponse>;

    // async fn add_peer(
    //     &self,
    //     request: AddPeerRequest
    // ) -> RpcResult<AddPeerResponse>;

    // async fn submit_transaction(
    //     &self,
    //     transaction: RpcTransaction,
    //     allow_orphan: bool,
    // ) -> RpcResult<SubmitTransactionResponse>;

    /// Requests information about a specific block.
    async fn get_block(&self, hash: RpcHash, include_transactions: bool) -> RpcResult<GetBlockResponse> {
        self.get_block_call(GetBlockRequest::new(hash, include_transactions)).await
    }
    async fn get_block_call(&self, request: GetBlockRequest) -> RpcResult<GetBlockResponse>;

    // async fn get_subnetwork(
    //     &self,
    //     request: GetSubnetworkRequest
    // ) -> RpcResult<GetSubnetworkResponse>;

    // async fn get_virtual_selected_parent_chain_from_block(
    //     &self,
    //     request: GetVirtualSelectedParentChainFromBlockRequest
    // ) -> RpcResult<GetVirtualSelectedParentChainFromBlockResponse>;

    // async fn get_blocks(
    //     &self,
    //     request: GetBlocksRequest
    // ) -> RpcResult<GetBlocksResponse>;

    // async fn get_block_count(
    //     &self,
    //     request: GetBlockCountRequest
    // ) -> RpcResult<GetBlockCountResponse>;

    // async fn get_block_dag_info(
    //     &self,
    //     request: GetBlockDagInfoRequest
    // ) -> RpcResult<GetBlockDagInfoResponse>;

    // async fn resolve_finality_conflict(
    //     &self,
    //     request: ResolveFinalityConflictRequest
    // ) -> RpcResult<ResolveFinalityConflictResponse>;

    // async fn shutdown(
    //     &self
    // ) -> RpcResult<()>;

    // async fn get_headers(
    //     &self,
    //     request: GetHeadersRequest
    // ) -> RpcResult<GetHeadersResponse>;

    // async fn get_utxos_by_address(
    //     &self,
    //     addresses : Vec<Address>
    // ) -> RpcResult<GetUtxosByAddressesResponse>;

    // async fn get_balance_by_address(
    //     &self,
    //     address : Address
    // ) -> RpcResult<u64>;

    // async fn get_balances_by_addresses(
    //     &self,
    //     addresses : Vec<Address>
    // ) -> RpcResult<Vec<(Address,u64)>>;

    // async fn get_virtual_selected_parent_blue_score(
    //     &self
    // ) -> RpcResult<u64>;

    // async fn ban(
    //     &self,
    //     request: BanRequest
    // ) -> RpcResult<BanResponse>;

    // async fn unban(
    //     &self,
    //     request: UnbanRequest
    // ) -> RpcResult<UnbanResponse>;

    async fn get_info_call(&self, request: GetInfoRequest) -> RpcResult<GetInfoResponse>;
    async fn get_info(&self) -> RpcResult<GetInfoResponse> {
        self.get_info_call(GetInfoRequest {}).await
    }

    // async fn estimate_network_hashes_per_second(
    //     &self,
    //     request: EstimateNetworkHashesPerSecondRequest
    // ) -> RpcResult<u64>;

    // async fn get_mempool_entries_by_addresses(
    //     &self,
    //     request: GetMempoolEntriesByAddressesRequest
    // ) -> RpcResult<GetMempoolEntriesByAddressesResponse>;

    // async fn get_coin_supply(
    //     &self
    // ) -> RpcResult<GetCoinSupplyResponse>;

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
