use kaspa_core::warn;
use kaspa_p2p_lib::{common::ProtocolError, Router};
use std::sync::Arc;

#[async_trait::async_trait]
pub trait Flow
where
    Self: 'static + Send + Sync,
{
    fn name(&self) -> &'static str;
    fn router(&self) -> Option<Arc<Router>>;

    async fn start(&mut self) -> Result<(), ProtocolError>;
    fn launch(mut self: Box<Self>) {
        tokio::spawn(async move {
            let res = self.start().await;
            if let Err(err) = res {
                // TODO: imp complete error handler (what happens in go?)
                if let Some(router) = self.router() {
                    if router.close().await || !err.is_connection_closed_error() {
                        // TODO: send and receive an explicit reject message for easier tracing of bugs causing disconnections
                        warn!("{} flow error: {}, disconnecting from peer {}.", self.name(), err, router);
                    }
                }
            }
        });
    }
}
