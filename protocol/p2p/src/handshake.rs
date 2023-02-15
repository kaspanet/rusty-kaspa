use crate::common::FlowError;
use crate::pb::{kaspad_message::Payload, ReadyMessage, VerackMessage, VersionMessage};
use crate::{dequeue_with_timeout, make_message};
use crate::{IncomingRoute, Router};
use kaspa_core::debug;
use std::sync::Arc;

#[derive(Default)]
pub struct KaspadHandshake {}

impl KaspadHandshake {
    pub fn new() -> Self {
        Self {}
    }

    async fn receive_version_flow(&self, router: &Arc<Router>, mut receiver: IncomingRoute) -> Result<VersionMessage, FlowError> {
        debug!("starting receive version flow");

        let version_message = dequeue_with_timeout!(receiver, Payload::Version)?;
        debug!("accepted version massage: {version_message:?}");

        let verack_message = make_message!(Payload::Verack, VerackMessage {});
        router.enqueue(verack_message).await?;

        Ok(version_message)
    }

    async fn send_version_flow(
        &self,
        router: &Arc<Router>,
        mut receiver: IncomingRoute,
        version_message: VersionMessage,
    ) -> Result<(), FlowError> {
        debug!("starting send version flow");

        debug!("sending version massage: {version_message:?}");
        let version_message = make_message!(Payload::Version, version_message);
        router.enqueue(version_message).await?;

        let verack_message = dequeue_with_timeout!(receiver, Payload::Verack)?;
        debug!("accepted verack_message: {verack_message:?}");

        Ok(())
    }

    pub async fn ready_flow(&self, router: &Arc<Router>, mut receiver: IncomingRoute) -> Result<(), FlowError> {
        debug!("starting ready flow");

        let sent_ready_message = make_message!(Payload::Ready, ReadyMessage {});
        router.enqueue(sent_ready_message).await?;

        let recv_ready_message = dequeue_with_timeout!(receiver, Payload::Ready)?;
        debug!("accepted ready message: {recv_ready_message:?}");

        Ok(())
    }

    /// Performs the handshake with the peer, essentially exchanging version messages
    pub async fn handshake(
        &self,
        router: &Arc<Router>,
        version_receiver: IncomingRoute,
        verack_receiver: IncomingRoute,
        self_version_message: VersionMessage,
    ) -> Result<VersionMessage, FlowError> {
        // Run both send and receive flows concurrently -- this is critical in order to avoid a handshake deadlock
        let (send_res, recv_res) = tokio::join!(
            self.send_version_flow(router, verack_receiver, self_version_message),
            self.receive_version_flow(router, version_receiver)
        );
        send_res?;
        recv_res
    }
}
