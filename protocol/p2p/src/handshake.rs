use std::time::Duration;

use crate::pb::{kaspad_message::Payload, ReadyMessage, VerackMessage, VersionMessage};
use crate::{common::ProtocolError, dequeue_with_timeout, make_message};
use crate::{IncomingRoute, KaspadMessagePayloadType, Router};
use kaspa_core::debug;

/// Implements the Kaspa peer-to-peer handshake protocol
pub struct KaspadHandshake<'a> {
    router: &'a Router,
    version_receiver: IncomingRoute,
    verack_receiver: IncomingRoute,
    ready_receiver: IncomingRoute,
}

impl<'a> KaspadHandshake<'a> {
    /// Builds the handshake object and subscribes to handshake messages
    pub fn new(router: &'a Router) -> Self {
        Self {
            router,
            version_receiver: router.subscribe(vec![KaspadMessagePayloadType::Version]),
            verack_receiver: router.subscribe(vec![KaspadMessagePayloadType::Verack]),
            ready_receiver: router.subscribe(vec![KaspadMessagePayloadType::Ready]),
        }
    }

    async fn receive_version_flow(router: &Router, version_receiver: &mut IncomingRoute) -> Result<VersionMessage, ProtocolError> {
        debug!("starting receive version flow");

        let version_message = dequeue_with_timeout!(version_receiver, Payload::Version, Duration::from_secs(4))?;
        debug!("accepted version message: {version_message:?}");

        let verack_message = make_message!(Payload::Verack, VerackMessage {});
        router.enqueue(verack_message).await?;

        Ok(version_message)
    }

    async fn send_version_flow(
        router: &Router,
        verack_receiver: &mut IncomingRoute,
        version_message: VersionMessage,
    ) -> Result<(), ProtocolError> {
        debug!("starting send version flow");

        debug!("sending version message: {version_message:?}");
        let version_message = make_message!(Payload::Version, version_message);
        router.enqueue(version_message).await?;

        let verack_message = dequeue_with_timeout!(verack_receiver, Payload::Verack, Duration::from_secs(4))?;
        debug!("accepted verack_message: {verack_message:?}");

        Ok(())
    }

    /// Exchange `Ready` messages with the peer. This is the final step of the handshake protocol and should
    /// only be called after all flows corresponding to the version exchange info are registered.
    pub async fn exchange_ready_messages(&mut self) -> Result<(), ProtocolError> {
        debug!("starting ready flow");

        let sent_ready_message = make_message!(Payload::Ready, ReadyMessage {});
        self.router.enqueue(sent_ready_message).await?;

        let recv_ready_message = dequeue_with_timeout!(self.ready_receiver, Payload::Ready, Duration::from_secs(8))?;
        debug!("accepted ready message: {recv_ready_message:?}");

        Ok(())
    }

    /// Performs the handshake with the peer, essentially exchanging version messages
    pub async fn handshake(&mut self, self_version_message: VersionMessage) -> Result<VersionMessage, ProtocolError> {
        // Run both send and receive flows concurrently -- this is critical in order to avoid a handshake deadlock
        let (send_res, recv_res) = tokio::join!(
            Self::send_version_flow(self.router, &mut self.verack_receiver, self_version_message),
            Self::receive_version_flow(self.router, &mut self.version_receiver)
        );
        send_res?;
        recv_res
    }
}
