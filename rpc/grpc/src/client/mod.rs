use self::resolver::Resolver;
use self::result::Result;
use async_trait::async_trait;
use rpc_core::{
    api::ops::RpcApiOps,
    api::rpc::RpcApi,
    error::RpcError,
    error::RpcResult,
    model::message::*,
    notify::{
        channel::NotificationChannel,
        collector::RpcCoreCollector,
        listener::{ListenerID, ListenerReceiverSide, ListenerUtxoNotificationFilterSetting},
        notifier::Notifier,
        subscriber::Subscriber,
    },
    NotificationSender, NotificationType,
};
use std::sync::Arc;

pub mod errors;
mod resolver;
mod result;
#[macro_use]
mod route;

#[derive(Debug)]
pub struct RpcApiGrpc {
    inner: Arc<Resolver>,
    notifier: Arc<Notifier>,
}

impl RpcApiGrpc {
    pub async fn connect(address: String) -> Result<RpcApiGrpc> {
        let notify_channel = NotificationChannel::default();
        let inner = Resolver::connect(address, notify_channel.sender()).await?;
        let collector = Arc::new(RpcCoreCollector::new(notify_channel.receiver()));
        let subscriber = Subscriber::new(inner.clone(), 0);

        let notifier =
            Arc::new(Notifier::new(Some(collector), Some(subscriber), ListenerUtxoNotificationFilterSetting::FilteredByAddress));

        Ok(Self { inner, notifier })
    }

    pub async fn start(&self) {
        self.notifier.clone().start();
    }

    pub async fn stop(&self) -> Result<()> {
        self.notifier.clone().stop().await?;
        Ok(())
    }

    pub fn handle_stop_notify(&self) -> bool {
        self.inner.handle_stop_notify()
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        self.inner.clone().shutdown().await?;
        Ok(())
    }
}

#[async_trait]
impl RpcApi for RpcApiGrpc {
    // this example illustrates the body of the function created by the route!() macro
    // async fn submit_block_call(&self, request: SubmitBlockRequest) -> RpcResult<SubmitBlockResponse> {
    //     self.inner.clone().call(RpcApiOps::SubmitBlock, request).await?.as_ref().try_into()
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
    fn register_new_listener(&self, sender: NotificationSender) -> ListenerID {
        self.notifier.register_new_listener(sender)
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
        self.notifier.start_notify(id, notification_type)?;
        Ok(())
    }

    /// Stop sending notifications of some type to a listener.
    async fn stop_notify(&self, id: ListenerID, notification_type: NotificationType) -> RpcResult<()> {
        if self.handle_stop_notify() {
            self.notifier.stop_notify(id, notification_type)?;
            Ok(())
        } else {
            Err(RpcError::UnsupportedFeature)
        }
    }
}
