use async_trait::async_trait;
use borsh::*;
use rpc_core::api::ops::RpcApiOps;
use rpc_core::api::rpc::RpcApi;
#[allow(unused_imports)]
use rpc_core::errors::RpcResult;
#[allow(unused_imports)]
use rpc_core::notify::channel::*;
#[allow(unused_imports)]
use rpc_core::notify::listener::*;
use rpc_core::prelude::*;
use std::sync::Arc;
use workflow_log::*;
use workflow_rpc::asynchronous::error::RpcResponseError as ResponseError;
use workflow_rpc::asynchronous::result::RpcResult as Response;
use workflow_rpc::asynchronous::server::*;

use crate::placeholder::KaspaInterfacePlaceholder;
use crate::result::Result;

pub struct Options {
    is_public: bool,
}

pub struct Server {
    /// disables Shutdown command and possibly other
    /// commands in the future that should not be
    /// on the public interface
    _is_public: bool,
    interface: Arc<dyn RpcApi>,
}

impl Server {
    // #[allow(dead_code)]
    pub fn try_new(options: Options, interface: Arc<dyn RpcApi>) -> Result<Server> {
        let server = Server { _is_public: options.is_public, interface };

        Ok(server)
    }

    pub async fn init(&self) -> Result<()> {
        Ok(())
    }
}

macro_rules! route {
    ($self: ident, $data: ident, $fn:ident, $name: ident) => {
        paste::paste! {
            {
                let req = [<$name Request>]::try_from_slice($data)?;
                let resp : [<$name Response>] = $self
                    .interface
                    .$fn(req)
                    .await
                    .map_err(|e|ResponseError::Text(e.to_string()))?;
                Ok(resp.try_to_vec()?)
            }
        }
    };
}

#[async_trait]
// impl RpcHandlerBorsh<RpcApiOps> for Server
impl RpcHandler<RpcApiOps> for Server {
    async fn handle_request(self: Arc<Self>, op: RpcApiOps, data: &[u8]) -> Response {
        match op {
            RpcApiOps::SubmitBlock => {
                route!(self, data, submit_block_call, SubmitBlock)
            }
            RpcApiOps::GetBlockTemplate => {
                route!(self, data, get_block_template_call, GetBlockTemplate)
            }
            RpcApiOps::GetBlock => {
                route!(self, data, get_block_call, GetBlock)
            }
            RpcApiOps::GetInfo => {
                route!(self, data, get_info_call, GetInfo)
            }

            // Ping = 0,

            // GetCurrentNetwork => {
            //     route!(self, data, get_current_network_call, GetCurrentNetwork)
            // },
            // GetPeerAddresses => {
            //     route!(self, data, get_peer_addresses_call, GetPeerAddresses)
            // },
            // GetSelectedTipHash => {
            //     route!(self, data, get_selected_tip_hash_call, GetSelectedTipHash)
            // },
            // GetMempoolEntry => {
            //     route!(self, data, get_mempool_entry_call, GetMempoolEntry)
            // },
            // GetMempoolEntries => {
            //     route!(self, data, get_mempool_entries_call, GetMempoolEntries)
            // },
            // GetConnectedPeerInfo => {
            //     route!(self, data, get_connected_peer_info_call, GetConnectedPeer)
            // },
            // AddPeer => {
            //     route!(self, data, add_peer_call, AddPeer)
            // },
            // SubmitTransaction => {
            //     route!(self, data, submit_transaction_call, SubmitTransaction)
            // },
            // GetBlock => {
            //     route!(self, data, get_block_call, GetBlock)
            // },
            // GetSubnetwork => {
            //     route!(self, data, get_subnetwork_call, GetSubnetwork)
            // },
            // GetVirtualSelectedParentChainFromBlock => {
            //     route!(self, data, get_virtual_selected_parent_chain_from_block_call, GetVirtualSelectedParentChainFromBlock)
            // },
            // GetBlocks => {
            //     route!(self, data, get_blocks_call, GetBlocks)
            // },
            // GetBlockCount => {
            //     route!(self, data, get_block_count_call, GetBlockCount)
            // },
            // GetBlockDagInfo => {
            //     route!(self, data, get_block_dag_info_call, GetBlockDagInfo)
            // },
            // ResolveFinalityConflict => {
            //     route!(self, data, resolve_finality_conflict_call, ResolveFinalityConflict)
            // },
            // Shutdown => {
            //     route!(self, data, shutdown_call, Shutdown)
            // },
            // GetHeaders => {
            //     route!(self, data, get_headers_call, GetHeaders)
            // },
            // GetUtxosByAddresses => {
            //     route!(self, data, get_utxos_by_addresses_call, GetUtxosByAddresses)
            // },
            // GetBalanceByAddress => {
            //     route!(self, data, get_balance_by_address_call, GetBalanceByAddress)
            // },
            // GetBalancesByAddresses => {
            //     route!(self, data, get_balances_by_addresses_call, GetBalancesByAddresses)
            // },
            // GetVirtualSelectedParentBlueScore => {
            //     route!(self, data, get_virtual_selected_parent_blue_score_call, GetVirtualSelectedParentBlueScore)
            // },
            // Ban => {
            //     route!(self, data, ban_call, Ban)
            // },
            // Unban => {
            //     route!(self, data, unban_call, Unban)
            // },
            // GetInfo => {
            //     route!(self, data, get_info_call, GetInfo)
            // },
            // EstimateNetworkHashesPerSecond => {
            //     route!(self, data, estimate_network_hashes_per_second_call, EstimateNetworkHashesPerSecond)
            // },
            // GetMempoolEntriesByAddresses => {
            //     route!(self, data, get_mempool_entries_by_addresses_call, GetMempoolEntriesByAddresses)
            // },
            // GetCoinSupply => {
            //     route!(self, data, get_coin_supply_call, GetCoinSupply)
            // },

            // // Subscription commands for starting/stopping notifications
            // NotifyBlockAdded => {
            //     unimplemented!()
            // },
            // NotifyNewBlockTemplate => {
            //     unimplemented!()
            // },

            // // Server to client notification
            // Notification => {
            //     unimplemented!()
            // },
            _ => {
                unimplemented!()
            }
        }

        // Ok(().try_to_vec()?)
    }
}

pub async fn rpc_server_task(addr: &str) -> Result<()> {
    let options = Options { is_public: false };

    let interface: Arc<dyn RpcApi> = Arc::new(KaspaInterfacePlaceholder {});

    let server = Arc::new(Server::try_new(options, interface)?);
    server.init().await?;
    // let rpc_handler = Arc::new(server);
    // let adaptor = Arc::new(RpcHandlerBorshAdaptor::new(server));
    // let adaptor = Arc::new(RpcHandler::new(server));
    let rpc = RpcServer::new(server);
    // let rpc = RpcServer::new(adaptor);

    log_info!("Kaspa workflow RPC server is listening on {}", addr);
    rpc.listen(addr).await?;

    Ok(())
}
