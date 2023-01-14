use super::route::route;
use crate::result::Result;
use async_trait::async_trait;
use regex::Regex;
use rpc_core::api::ops::RpcApiOps;
use rpc_core::api::rpc::RpcApi;
use rpc_core::errors::RpcResult;
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
    route!(submit_block_call, SubmitBlock);
    route!(get_block_template_call, GetBlockTemplate);
    route!(get_block_call, GetBlock);
    route!(get_info_call, GetInfo);

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
