// use async_trait::async_trait;
// use borsh::*;
use rpc_core::api::ops::RpcApiOps;
use rpc_core::api::rpc::RpcApi;
#[allow(unused_imports)]
use rpc_core::error::RpcResult;
#[allow(unused_imports)]
use rpc_core::notify::channel::*;
#[allow(unused_imports)]
use rpc_core::notify::listener::*;
use rpc_core::prelude::*;
use std::sync::Arc;
use workflow_rpc::server::prelude::*;
// use workflow_rpc::error::ServerError;
// use workflow_rpc::result::ServerResult;

pub struct Router<ConnectionContext>
where
    ConnectionContext: Send + Sync + 'static,
{
    // iface: Arc<dyn RpcApi>,
    pub interface: Arc<Interface<Arc<dyn RpcApi>, Arc<ConnectionContext>, RpcApiOps>>,
    verbose: bool,
}

// macro_rules! route {
//     ($self: ident, $data: ident, $fn:ident, $name: ident) => {
//         paste::paste! {
//             {
//                 let req = [<$name Request>]::try_from_slice($data)?;

//                 if $self.verbose {
//                     println!("{:#?}",req);
//                 }

//                 let resp : [<$name Response>] = $self
//                     .iface
//                     .$fn(req)
//                     .await
//                     .map_err(|e|ServerError::Text(e.to_string()))?;

//                 if $self.verbose {
//                     println!("{:#?}",resp);
//                 }

//                 Ok(resp.try_to_vec()?)
//             }
//         }
//     };
// }

impl<ConnectionContext> Router<ConnectionContext>
where
    ConnectionContext: Send + Sync + 'static,
{
    pub fn new(rpc_api: Arc<dyn RpcApi>, verbose: bool) -> Self {
        let mut interface = Interface::<Arc<dyn RpcApi>, Arc<ConnectionContext>, RpcApiOps>::new(rpc_api.clone());

        interface.method(
            RpcApiOps::GetInfo,
            method!(|rpc_api : Arc<dyn RpcApi>, _connection_ctx, req: GetInfoRequest| async move { 
                let res: GetInfoResponse = rpc_api.get_info_call(req).await
                    .map_err(|e|ServerError::Text(e.to_string()))?;
                Ok(res)
            }),
        );

        Router { interface: Arc::new(interface), verbose }
    }

    // pub async fn route(&self, op: RpcApiOps, data: &[u8]) -> ServerResult {
    //     match op {
    //         RpcApiOps::Ping => {
    //             route!(self, data, ping_call, Ping)
    //         }
    //         RpcApiOps::GetProcessMetrics => {
    //             route!(self, data, get_process_metrics_call, GetProcessMetrics)
    //         }
    //         RpcApiOps::SubmitBlock => {
    //             route!(self, data, submit_block_call, SubmitBlock)
    //         }
    //         RpcApiOps::GetBlockTemplate => {
    //             route!(self, data, get_block_template_call, GetBlockTemplate)
    //         }
    //         RpcApiOps::GetBlock => {
    //             route!(self, data, get_block_call, GetBlock)
    //         }
    //         RpcApiOps::GetInfo => {
    //             route!(self, data, get_info_call, GetInfo)
    //         }
    //         RpcApiOps::GetCurrentNetwork => {
    //             route!(self, data, get_current_network_call, GetCurrentNetwork)
    //         }
    //         RpcApiOps::GetPeerAddresses => {
    //             route!(self, data, get_peer_addresses_call, GetPeerAddresses)
    //         }
    //         RpcApiOps::GetSelectedTipHash => {
    //             route!(self, data, get_selected_tip_hash_call, GetSelectedTipHash)
    //         }
    //         RpcApiOps::GetMempoolEntry => {
    //             route!(self, data, get_mempool_entry_call, GetMempoolEntry)
    //         }
    //         RpcApiOps::GetMempoolEntries => {
    //             route!(self, data, get_mempool_entries_call, GetMempoolEntries)
    //         }
    //         RpcApiOps::GetConnectedPeerInfo => {
    //             route!(self, data, get_connected_peer_info_call, GetConnectedPeerInfo)
    //         }
    //         RpcApiOps::AddPeer => {
    //             route!(self, data, add_peer_call, AddPeer)
    //         }
    //         RpcApiOps::SubmitTransaction => {
    //             route!(self, data, submit_transaction_call, SubmitTransaction)
    //         }
    //         RpcApiOps::GetSubnetwork => {
    //             route!(self, data, get_subnetwork_call, GetSubnetwork)
    //         }
    //         RpcApiOps::GetVirtualSelectedParentChainFromBlock => {
    //             route!(self, data, get_virtual_selected_parent_chain_from_block_call, GetVirtualSelectedParentChainFromBlock)
    //         }
    //         RpcApiOps::GetBlocks => {
    //             route!(self, data, get_blocks_call, GetBlocks)
    //         }
    //         RpcApiOps::GetBlockCount => {
    //             route!(self, data, get_block_count_call, GetBlockCount)
    //         }
    //         RpcApiOps::GetBlockDagInfo => {
    //             route!(self, data, get_block_dag_info_call, GetBlockDagInfo)
    //         }
    //         RpcApiOps::ResolveFinalityConflict => {
    //             route!(self, data, resolve_finality_conflict_call, ResolveFinalityConflict)
    //         }
    //         RpcApiOps::Shutdown => {
    //             route!(self, data, shutdown_call, Shutdown)
    //         }
    //         RpcApiOps::GetHeaders => {
    //             route!(self, data, get_headers_call, GetHeaders)
    //         }
    //         RpcApiOps::GetUtxosByAddresses => {
    //             route!(self, data, get_utxos_by_addresses_call, GetUtxosByAddresses)
    //         }
    //         RpcApiOps::GetBalanceByAddress => {
    //             route!(self, data, get_balance_by_address_call, GetBalanceByAddress)
    //         }
    //         RpcApiOps::GetBalancesByAddresses => {
    //             route!(self, data, get_balances_by_addresses_call, GetBalancesByAddresses)
    //         }
    //         RpcApiOps::GetVirtualSelectedParentBlueScore => {
    //             route!(self, data, get_virtual_selected_parent_blue_score_call, GetVirtualSelectedParentBlueScore)
    //         }
    //         RpcApiOps::Ban => {
    //             route!(self, data, ban_call, Ban)
    //         }
    //         RpcApiOps::Unban => {
    //             route!(self, data, unban_call, Unban)
    //         }
    //         RpcApiOps::EstimateNetworkHashesPerSecond => {
    //             route!(self, data, estimate_network_hashes_per_second_call, EstimateNetworkHashesPerSecond)
    //         }
    //         RpcApiOps::GetMempoolEntriesByAddresses => {
    //             route!(self, data, get_mempool_entries_by_addresses_call, GetMempoolEntriesByAddresses)
    //         }
    //         RpcApiOps::GetCoinSupply => {
    //             route!(self, data, get_coin_supply_call, GetCoinSupply)
    //         }

    //         // Subscription commands for starting/stopping notifications
    //         RpcApiOps::NotifyBlockAdded => {
    //             unimplemented!()
    //         }
    //         RpcApiOps::NotifyNewBlockTemplate => {
    //             unimplemented!()
    //         }

    //         // Server to client notification
    //         RpcApiOps::Notification => {
    //             unimplemented!()
    //         }

    //         _ => {
    //             unimplemented!()
    //         }
    //     }
    // }
}
