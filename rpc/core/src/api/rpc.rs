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

#[async_trait]
pub trait RpcApi: Sync + Send {
    // async fn ping(
    //     &self,
    //     msg : String
    // ) -> RpcResult<String>;

    // async fn get_current_network(
    //     &self
    // ) -> RpcResult<NetworkType>;

    // async fn submit_block(
    //     &self,
    //     block: RpcBlock,
    //     allow_non_daa_blocks : bool
    // ) -> RpcResult<SubmitBlockResponse>;

    // async fn get_block_template(
    //     &self,
    //     req: GetBlockTemplateRequest
    // ) -> RpcResult<GetBlockTemplateResponse>;

    // async fn get_peer_addresses(
    //     &self
    // ) -> RpcResult<GetPeerAddressesResponse>;

    // async fn get_selected_tip_hash(
    //     &self
    // ) -> RpcResult<GetSelectedTipHashResponse>;

    // async fn get_mempool_entry(
    //     &self,
    //     req:  GetMempoolEntryRequest
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
    //     req: AddPeerRequest
    // ) -> RpcResult<AddPeerResponse>;

    // async fn submit_transaction(
    //     &self,
    //     transaction: RpcTransaction,
    //     allow_orphan: bool,
    // ) -> RpcResult<SubmitTransactionResponse>;

    async fn get_block(&self, req: GetBlockRequest) -> RpcResult<GetBlockResponse>;

    // async fn get_subnetwork(
    //     &self,
    //     req: GetSubnetworkRequest
    // ) -> RpcResult<GetSubnetworkResponse>;

    // async fn get_virtual_selected_parent_chain_from_block(
    //     &self,
    //     req: GetVirtualSelectedParentChainFromBlockRequest
    // ) -> RpcResult<GetVirtualSelectedParentChainFromBlockResponse>;

    // async fn get_blocks(
    //     &self,
    //     req: GetBlocksRequest
    // ) -> RpcResult<GetBlocksResponse>;

    // async fn get_block_count(
    //     &self,
    //     req: GetBlockCountRequest
    // ) -> RpcResult<GetBlockCountResponse>;

    // async fn get_block_dag_info(
    //     &self,
    //     req: GetBlockDagInfoRequest
    // ) -> RpcResult<GetBlockDagInfoResponse>;

    // async fn resolve_finality_conflict(
    //     &self,
    //     req: ResolveFinalityConflictRequest
    // ) -> RpcResult<ResolveFinalityConflictResponse>;

    // async fn shutdown(
    //     &self
    // ) -> RpcResult<()>;

    // async fn get_headers(
    //     &self,
    //     req: GetHeadersRequest
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
    //     req: BanRequest
    // ) -> RpcResult<BanResponse>;

    // async fn unban(
    //     &self,
    //     req: UnbanRequest
    // ) -> RpcResult<UnbanResponse>;

    async fn get_info(&self, req: GetInfoRequest) -> RpcResult<GetInfoResponse>;

    // async fn estimate_network_hashes_per_second(
    //     &self,
    //     req: EstimateNetworkHashesPerSecondRequest
    // ) -> RpcResult<u64>;

    // async fn get_mempool_entries_by_addresses(
    //     &self,
    //     req: GetMempoolEntriesByAddressesRequest
    // ) -> RpcResult<GetMempoolEntriesByAddressesResponse>;

    // async fn get_coin_supply(
    //     &self
    // ) -> RpcResult<GetCoinSupplyResponse>;

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    /// Register a new listenera and return an id and channel receiver.
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
