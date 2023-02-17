use kaspa_core::warn;
use p2p_lib::{common::FlowError, Router};
use std::sync::Arc;

#[async_trait::async_trait]
pub trait Flow
where
    Self: 'static + Send + Sync,
{
    fn name(&self) -> &'static str;
    fn router(&self) -> Option<Arc<Router>>;

    async fn start(&mut self) -> Result<(), FlowError>;
    fn launch(mut self: Box<Self>) {
        tokio::spawn(async move {
            let res = self.start().await;
            if let Err(err) = res {
                warn!("{} flow error: {}, disconnecting from peer.", self.name(), err); // TODO: imp complete error handler with net-connection peer info etc
                if let Some(router) = self.router() {
                    router.close().await;
                }
            }
        });
    }
}
