//use super::route::route;
use crate::result::Result;
use async_trait::async_trait;
use kaspa_rpc_macros::build_wrpc_client_interface;
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
pub use workflow_rpc::client::prelude::Encoding as WrpcEncoding;
use workflow_rpc::client::prelude::*;


/// [`KaspaRpcClient`] allows connection to the Kaspa wRPC Server via 
/// binary Borsh or JSON protocols.
#[derive(Clone)]
pub struct KaspaRpcClient {
    rpc: Arc<RpcClient<RpcApiOps>>,
}

impl KaspaRpcClient {
    pub fn new(encoding: Encoding, url: &str) -> Result<KaspaRpcClient> {
        let re = Regex::new(r"^wrpc").unwrap();
        let url = re.replace(url, "ws").to_string();
        log_trace!("Kaspa wRPC::{encoding} client url: {url}");
        let options = RpcClientOptions { url: &url, ..RpcClientOptions::default() };

        // The `Interface` struct can be used to register for server-side
        // notifications. All notification methods have to be created at
        // this stage.
        let interface = Interface::<RpcApiOps>::new();
        // interface.notification(
        //     TestOps::Notify,
        //     notification!(|msg: TestNotify| async move {
        //         log_info!("received notification: {:?}", msg);
        //         Ok(())
        //     }),
        // );

        let client = KaspaRpcClient { rpc: Arc::new(RpcClient::new_with_encoding(encoding, interface.into(), options)?) };

        Ok(client)
    }

    /// Starts a background async connection task connecting
    /// to the wRPC server.  If the supplied `block` call is `true`
    /// this function will block until the first successful
    /// connection.
    pub async fn connect(&self, block: bool) -> Result<Option<Listener>> {
        Ok(self.rpc.connect(block).await?)
    }

    /// A helper function that is not `async`, allowing connection
    /// process to be initiated from non-async contexts.
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
    //
    // The following proc-macro iterates over the array of enum variants 
    // generating a function for each variant as follows:
    //
    // async fn ping_call(&self, request : PingRequest) -> RpcResult<PingResponse> {
    //     let response: ClientResult<PingResponse> = self.rpc.call(RpcApiOps::Ping, request).await;
    //     Ok(response.map_err(|e| e.to_string())?)
    // }

    build_wrpc_client_interface!(
        RpcApiOps,
        [
            AddPeer,
            Ban,
            EstimateNetworkHashesPerSecond,
            GetBalanceByAddress,
            GetBalancesByAddresses,
            GetBlock,
            GetBlockCount,
            GetBlockDagInfo,
            GetBlocks,
            GetBlockTemplate,
            GetCoinSupply,
            GetConnectedPeerInfo,
            GetCurrentNetwork,
            GetHeaders,
            GetInfo,
            GetMempoolEntries,
            GetMempoolEntriesByAddresses,
            GetMempoolEntry,
            GetPeerAddresses,
            GetProcessMetrics,
            GetSelectedTipHash,
            GetSubnetwork,
            GetUtxosByAddresses,
            GetVirtualSelectedParentBlueScore,
            GetVirtualSelectedParentChainFromBlock,
            Ping,
            ResolveFinalityConflict,
            Shutdown,
            SubmitBlock,
            SubmitTransaction,
            Unban,
        ]
    );

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
