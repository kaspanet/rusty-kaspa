use crate::{flow_context::FlowContext, flow_trait::Flow};
use kaspa_core::debug;
use kaspa_p2p_lib::{
    common::ProtocolError,
    dequeue, dequeue_with_timeout, make_message,
    pb::{kaspad_message::Payload, PingMessage, PongMessage},
    IncomingRoute, Router,
};
use rand::Rng;
use std::{
    sync::{Arc, Weak},
    time::Duration,
};

/// Flow for managing a loop receiving pings and responding with pongs
pub struct ReceivePingsFlow {
    _ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for ReceivePingsFlow {
    fn name(&self) -> &'static str {
        "Receive pings"
    }

    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl ReceivePingsFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { _ctx: ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            // We dequeue without a timeout in this case, responding to pings whenever they arrive
            let ping = dequeue!(self.incoming_route, Payload::Ping)?;
            debug!("P2P Flows, got ping request with nonce {}", ping.nonce);
            let pong = make_message!(Payload::Pong, PongMessage { nonce: ping.nonce });
            self.router.enqueue(pong).await?;
        }
    }
}

pub const PING_INTERVAL: Duration = Duration::from_secs(120); // 2 minutes

/// Flow for managing a loop sending pings and waiting for pongs
pub struct SendPingsFlow {
    _ctx: FlowContext,

    // We use a weak reference to avoid this flow from holding the router during timer waiting if the connection was closed
    router: Weak<Router>,
    peer: String,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for SendPingsFlow {
    fn name(&self) -> &'static str {
        "Send pings"
    }

    fn router(&self) -> Option<Arc<Router>> {
        self.router.upgrade()
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl SendPingsFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        let peer = router.to_string();
        Self { _ctx: ctx, router: Arc::downgrade(&router), peer, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            // TODO: handle application shutdown signal
            // TODO: set peer ping state to pending/idle

            // Wait `PING_INTERVAL` between pings
            tokio::time::sleep(PING_INTERVAL).await;

            // Create a fresh random nonce for each ping
            let nonce = rand::thread_rng().gen::<u64>();
            let ping = make_message!(Payload::Ping, PingMessage { nonce });
            if let Some(router) = self.router.upgrade() {
                router.enqueue(ping).await?;
            } else {
                return Err(ProtocolError::ConnectionClosed);
            }
            let pong = dequeue_with_timeout!(self.incoming_route, Payload::Pong)?;
            if pong.nonce != nonce {
                return Err(ProtocolError::Other("nonce mismatch between ping and pong"));
            } else {
                debug!("Successful ping with peer {} (nonce: {})", self.peer, pong.nonce);
            }
        }
    }
}
