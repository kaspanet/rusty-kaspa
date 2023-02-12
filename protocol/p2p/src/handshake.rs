use crate::common::FlowError;
use crate::pb::{self, kaspad_message::Payload, KaspadMessage, VersionMessage};
use crate::recv_payload;
use crate::Router;
use kaspa_core::debug;
use std::sync::Arc;
use tokio::sync::mpsc::Receiver as MpscReceiver;

#[derive(Default)]
pub struct KaspadHandshake {}

impl KaspadHandshake {
    pub fn new() -> Self {
        Self {}
    }

    async fn receive_version_flow(
        &self,
        router: &Arc<Router>,
        mut receiver: MpscReceiver<KaspadMessage>,
    ) -> Result<VersionMessage, FlowError> {
        debug!("starting receive version flow");

        let version_message = recv_payload!(receiver, Payload::Version)?;
        debug!("accepted version massage: {version_message:?}");

        let verack_message = pb::KaspadMessage { payload: Some(pb::kaspad_message::Payload::Verack(pb::VerackMessage {})) };
        router.route_to_network(verack_message).await;

        Ok(version_message)
    }

    async fn send_version_flow(
        &self,
        router: &Arc<Router>,
        mut receiver: MpscReceiver<KaspadMessage>,
        self_version_message: pb::VersionMessage,
    ) -> Result<(), FlowError> {
        debug!("starting send version flow");

        let version_message = pb::KaspadMessage { payload: Some(pb::kaspad_message::Payload::Version(self_version_message)) };
        router.route_to_network(version_message).await;

        let verack_message = recv_payload!(receiver, Payload::Verack)?;
        debug!("accepted verack_message: {verack_message:?}");

        Ok(())
    }

    pub async fn ready_flow(&self, router: &Arc<Router>, mut receiver: MpscReceiver<KaspadMessage>) -> Result<(), FlowError> {
        debug!("starting ready flow");

        let sent_ready_message = pb::KaspadMessage { payload: Some(pb::kaspad_message::Payload::Ready(pb::ReadyMessage {})) };
        router.route_to_network(sent_ready_message).await;

        let recv_ready_message = recv_payload!(receiver, Payload::Ready)?;
        debug!("accepted ready message: {recv_ready_message:?}");

        Ok(())
    }

    /// Performs the handshake with the peer, essentially exchanging version messages
    pub async fn handshake(
        &self,
        router: &Arc<Router>,
        version_receiver: MpscReceiver<KaspadMessage>,
        verack_receiver: MpscReceiver<KaspadMessage>,
        self_version_message: pb::VersionMessage,
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
