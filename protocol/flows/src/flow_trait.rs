use kaspa_core::warn;
use kaspa_p2p_lib::{common::ProtocolError, Router};
use kaspa_utils::any::type_name_short;
use std::sync::Arc;

#[async_trait::async_trait]
pub trait Flow
where
    Self: 'static + Send + Sync,
{
    fn name(&self) -> &'static str {
        type_name_short::<Self>()
    }

    fn router(&self) -> Option<Arc<Router>>;

    async fn start(&mut self) -> Result<(), ProtocolError>;

    fn launch(mut self: Box<Self>) {
        tokio::spawn(async move {
            let res = self.start().await;
            if let Err(err) = res {
                if let Some(router) = self.router() {
                    router.try_sending_reject_message(&err).await;
                    if router.close().await || !err.is_connection_closed_error() {
                        warn!("{} flow error: {}, disconnecting from peer {}.", self.name(), err, router);
                    }
                }
            }
        });
    }
}
