use async_trait::async_trait;
use std::sync::Arc;

use self::resolver::Resolver;
use self::result::Result;
use rpc_core::{
    api::ops::RpcApiOps,
    api::rpc::RpcApi,
    notify::{
        channel::NotificationChannel,
        collector::RpcCoreCollector,
        listener::{ListenerID, ListenerReceiverSide, ListenerUtxoNotificationFilterSetting},
        notifier::Notifier,
        subscriber::Subscriber,
    },
    GetBlockRequest, GetBlockResponse, GetBlockTemplateRequest, GetBlockTemplateResponse, GetInfoRequest, GetInfoResponse,
    NotificationType, RpcError, RpcResult, SubmitBlockRequest, SubmitBlockResponse,
};

mod errors;
mod resolver;
mod result;

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
    async fn submit_block_call(&self, request: SubmitBlockRequest) -> RpcResult<SubmitBlockResponse> {
        self.inner.clone().call(RpcApiOps::SubmitBlock, request).await?.as_ref().try_into()
    }

    async fn get_block_template_call(&self, request: GetBlockTemplateRequest) -> RpcResult<GetBlockTemplateResponse> {
        self.inner.clone().call(RpcApiOps::GetBlockTemplate, request).await?.as_ref().try_into()
    }

    async fn get_block_call(&self, request: GetBlockRequest) -> RpcResult<GetBlockResponse> {
        self.inner.clone().call(RpcApiOps::GetBlock, request).await?.as_ref().try_into()
    }

    async fn get_info_call(&self, request: GetInfoRequest) -> RpcResult<GetInfoResponse> {
        self.inner.clone().call(RpcApiOps::GetInfo, request).await?.as_ref().try_into()
    }

    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // Notification API

    /// Register a new listener and returns an id and a channel receiver.
    fn register_new_listener(&self, channel: Option<NotificationChannel>) -> ListenerReceiverSide {
        self.notifier.register_new_listener(channel)
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
