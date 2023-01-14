use async_trait::async_trait;
// use workflow_log::*;
use rpc_core::api::rpc::RpcApi;
use rpc_core::errors::RpcError;
use rpc_core::errors::RpcResult;
use rpc_core::notify::channel::*;
use rpc_core::notify::listener::*;
use rpc_core::prelude::*;

pub struct KaspaInterfacePlaceholder {}

#[async_trait]
impl RpcApi for KaspaInterfacePlaceholder {
    async fn submit_block_call(&self, _request: SubmitBlockRequest) -> RpcResult<SubmitBlockResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_block_template_call(&self, _request: GetBlockTemplateRequest) -> RpcResult<GetBlockTemplateResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_block_call(&self, _request: GetBlockRequest) -> RpcResult<GetBlockResponse> {
        Err(RpcError::NotImplemented)
    }

    async fn get_info_call(&self, _request: GetInfoRequest) -> RpcResult<GetInfoResponse> {
        Err(RpcError::NotImplemented)
    }

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
