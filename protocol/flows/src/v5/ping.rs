use crate::ctx::FlowContext;
use p2p_lib::{
    common::FlowError,
    dequeue, dequeue_with_timeout, make_message,
    pb::{kaspad_message::Payload, PingMessage, PongMessage},
    ConnectionError, IncomingRoute, Router,
};
use rand::Rng;
use std::{
    sync::{Arc, Weak},
    time::Duration,
};

/// Flow for managing a loop receiving pings and responding with pongs
pub struct ReceivePingsFlow {
    _ctx: FlowContext,
    pub router: Arc<Router>, // TODO: remove pub
    incoming_route: IncomingRoute,
}

impl ReceivePingsFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { _ctx: ctx, router, incoming_route }
    }

    pub async fn start(&mut self) -> Result<(), FlowError> {
        loop {
            // We dequeue without a timeout in this case, responding to pings whenever they arrive
            let ping = dequeue!(self.incoming_route, Payload::Ping)?;
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
    pub router: Weak<Router>, // TODO: remove pub

    incoming_route: IncomingRoute,
}

impl SendPingsFlow {
    pub fn new(ctx: FlowContext, router: Weak<Router>, incoming_route: IncomingRoute) -> Self {
        Self { _ctx: ctx, router, incoming_route }
    }

    pub async fn start(&mut self) -> Result<(), FlowError> {
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
                return Err(FlowError::P2pConnectionError(ConnectionError::ChannelClosed));
            }
            let pong = dequeue_with_timeout!(self.incoming_route, Payload::Pong)?;
            if pong.nonce != nonce {
                return Err(FlowError::ProtocolError("nonce mismatch between ping and pong"));
            }
        }
    }
}
