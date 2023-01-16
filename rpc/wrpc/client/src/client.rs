use super::route::route;
use crate::result::Result;
use async_trait::async_trait;
use regex::Regex;
use rpc_core::api::ops::RpcApiOps;
use rpc_core::api::rpc::RpcApi;
use rpc_core::error::RpcResult;
use rpc_core::notify::channel::*;
use rpc_core::notify::listener::*;
use rpc_core::prelude::*;
use std::sync::Arc;
use workflow_core::trigger::Listener;
use workflow_log::*;
use workflow_rpc::asynchronous::client::result::Result as Response;
use workflow_rpc::asynchronous::client::RpcClient;

#[derive(Clone)]
pub struct KaspaRpcClient {
    rpc: Arc<RpcClient<RpcApiOps>>,
}

impl KaspaRpcClient {
    pub fn new(url: &str) -> Result<KaspaRpcClient> {
        let re = Regex::new(r"^rpc").unwrap();
        let url = re.replace(url, "ws");
        log_trace!("Kaspa RPC client url: {}", url);
        let client = KaspaRpcClient { rpc: Arc::new(RpcClient::new(&url)?) };

        Ok(client)
    }

    pub async fn connect(&self, block: bool) -> Result<Option<Listener>> {
        Ok(self.rpc.connect(block).await?)
    }

    pub fn connect_as_task(self: &Arc<Self>) -> Result<()> {
        let self_ = self.clone();
        workflow_core::task::spawn(async move {
            self_.rpc.connect(false).await.ok();
        });
        Ok(())
    }
}

#[async_trait]
impl RpcApi for KaspaRpcClient {
    route!(ping_call, Ping);
    route!(get_process_metrics_call, GetProcessMetrics);
    route!(get_current_network_call, GetCurrentNetwork);
    route!(submit_block_call, SubmitBlock);
    route!(get_block_template_call, GetBlockTemplate);
    route!(get_peer_addresses_call, GetPeerAddresses);
    route!(get_selected_tip_hash_call, GetSelectedTipHash);
    route!(get_mempool_entry_call, GetMempoolEntry);
    route!(get_mempool_entries_call, GetMempoolEntries);
    route!(get_connected_peer_info_call, GetConnectedPeerInfo);
    route!(add_peer_call, AddPeer);
    route!(submit_transaction_call, SubmitTransaction);
    route!(get_block_call, GetBlock);
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
    route!(get_info_call, GetInfo);
    route!(estimate_network_hashes_per_second_call, EstimateNetworkHashesPerSecond);
    route!(get_mempool_entries_by_addresses_call, GetMempoolEntriesByAddresses);
    route!(get_coin_supply_call, GetCoinSupply);

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    /// Register a new listener and returns an id and a channel receiver.
    fn register_new_listener(&self, _channel: Option<NotificationChannel>) -> ListenerReceiverSide {
        todo!()
        // self.notifier.register_new_listener(channel)
    }

    /// Unregister an existing listener.
    ///
    /// Stop all notifications for this listener and drop its channel.
    async fn unregister_listener(&self, _id: ListenerID) -> RpcResult<()> {
        todo!()
        // self.notifier.unregister_listener(id)?;
        // Ok(())
    }

    /// Start sending notifications of some type to a listener.
    async fn start_notify(&self, _id: ListenerID, _notification_type: NotificationType) -> RpcResult<()> {
        todo!()
        // self.notifier.start_notify(id, notification_type)?;
        // Ok(())
    }

    /// Stop sending notifications of some type to a listener.
    async fn stop_notify(&self, _id: ListenerID, _notification_type: NotificationType) -> RpcResult<()> {
        todo!()
        // if self.handle_stop_notify() {
        //     self.notifier.stop_notify(id, notification_type)?;
        //     Ok(())
        // } else {
        //     Err(RpcError::UnsupportedFeature)
        // }
    }
}
