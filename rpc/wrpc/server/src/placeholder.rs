use async_trait::async_trait;
// use workflow_log::*;
use rpc_core::api::rpc::RpcApi;
use rpc_core::error::RpcError;
use rpc_core::error::RpcResult;
use rpc_core::notify::channel::*;
use rpc_core::notify::listener::*;
use rpc_core::prelude::*;

macro_rules! placeholder {
    ($fn:ident, $name:tt) => {
        paste::paste! {
            fn $fn<'life0, 'async_trait>(
                &'life0 self,
                _request: [<$name Request>],
            ) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = RpcResult<[<$name Response>]>> + ::core::marker::Send + 'async_trait>>
            where
                'life0: 'async_trait,
                Self: 'async_trait,
            {
                Box::pin(async move {
                    if let ::core::option::Option::Some(__ret) = ::core::option::Option::None::<RpcResult<[<$name Response>]>> {
                        return __ret;
                    }
                    let __self = self;
                    // let request = request;
                    let __ret: RpcResult<[<$name Response>]> = {
                        Err(RpcError::NotImplemented)
                    };
                    #[allow(unreachable_code)]
                    __ret
                })
            }
        }
    };
}

pub struct KaspaInterfacePlaceholder {}

#[async_trait]
impl RpcApi for KaspaInterfacePlaceholder {
    // placeholder!(submit_block_call,SubmitBlock);
    placeholder!(ping_call, Ping);
    placeholder!(get_process_metrics_call, GetProcessMetrics);
    placeholder!(get_current_network_call, GetCurrentNetwork);
    placeholder!(submit_block_call, SubmitBlock);
    placeholder!(get_block_template_call, GetBlockTemplate);
    placeholder!(get_peer_addresses_call, GetPeerAddresses);
    placeholder!(get_selected_tip_hash_call, GetSelectedTipHash);
    placeholder!(get_mempool_entry_call, GetMempoolEntry);
    placeholder!(get_mempool_entries_call, GetMempoolEntries);
    placeholder!(get_connected_peer_info_call, GetConnectedPeerInfo);
    placeholder!(add_peer_call, AddPeer);
    placeholder!(submit_transaction_call, SubmitTransaction);
    placeholder!(get_block_call, GetBlock);
    placeholder!(get_subnetwork_call, GetSubnetwork);
    placeholder!(get_virtual_selected_parent_chain_from_block_call, GetVirtualSelectedParentChainFromBlock);
    placeholder!(get_blocks_call, GetBlocks);
    placeholder!(get_block_count_call, GetBlockCount);
    placeholder!(get_block_dag_info_call, GetBlockDagInfo);
    placeholder!(resolve_finality_conflict_call, ResolveFinalityConflict);
    placeholder!(shutdown_call, Shutdown);
    placeholder!(get_headers_call, GetHeaders);
    placeholder!(get_utxos_by_addresses_call, GetUtxosByAddresses);
    placeholder!(get_balance_by_address_call, GetBalanceByAddress);
    placeholder!(get_balances_by_addresses_call, GetBalancesByAddresses);
    placeholder!(get_virtual_selected_parent_blue_score_call, GetVirtualSelectedParentBlueScore);
    placeholder!(ban_call, Ban);
    placeholder!(unban_call, Unban);
    placeholder!(get_info_call, GetInfo);
    placeholder!(estimate_network_hashes_per_second_call, EstimateNetworkHashesPerSecond);
    placeholder!(get_mempool_entries_by_addresses_call, GetMempoolEntriesByAddresses);
    placeholder!(get_coin_supply_call, GetCoinSupply);

    // async fn submit_block_call(&self, _request: SubmitBlockRequest) -> RpcResult<SubmitBlockResponse> {
    //     Err(RpcError::NotImplemented)
    // }

    // async fn get_block_template_call(&self, _request: GetBlockTemplateRequest) -> RpcResult<GetBlockTemplateResponse> {
    //     Err(RpcError::NotImplemented)
    // }

    // async fn get_block_call(&self, _request: GetBlockRequest) -> RpcResult<GetBlockResponse> {
    //     Err(RpcError::NotImplemented)
    // }

    // async fn get_info_call(&self, _request: GetInfoRequest) -> RpcResult<GetInfoResponse> {
    //     Err(RpcError::NotImplemented)
    // }

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
