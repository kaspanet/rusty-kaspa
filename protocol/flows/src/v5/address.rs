use crate::{flow_context::FlowContext, flow_trait::Flow};
use addressmanager::NetAddress;
use consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use itertools::Itertools;
use kaspa_core::debug;
use p2p_lib::{
    common::ProtocolError,
    dequeue, dequeue_with_timeout, make_message,
    pb::{kaspad_message::Payload, AddressesMessage, PingMessage, PongMessage, RequestAddressesMessage},
    IncomingRoute, Router,
};
use rand::Rng;
use std::{
    net::IpAddr,
    sync::{Arc, Weak},
    time::Duration,
};

/// Flow for managing a loop receiving pings and responding with pongs
pub struct ReceiveAddressesFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for ReceiveAddressesFlow {
    fn name(&self) -> &'static str {
        "Receive addresses"
    }

    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl ReceiveAddressesFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        self.router
            .enqueue(make_message!(
                Payload::RequestAddresses,
                RequestAddressesMessage { include_all_subnetworks: false, subnetwork_id: None }
            ))
            .await?;
        // We dequeue without a timeout in this case, responding to pings whenever they arrive
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::Addresses)?;
        let address_list: Vec<(IpAddr, u16)> = msg.try_into()?;
        for (ip, port) in address_list {
            self.ctx.amgr.lock().add_address(NetAddress::new(ip, port))
        }

        Ok(())
    }
}

/// Flow for managing a loop sending pings and waiting for pongs
pub struct SendAddressesFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for SendAddressesFlow {
    fn name(&self) -> &'static str {
        "Send addresses"
    }

    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl SendAddressesFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            dequeue!(self.incoming_route, Payload::Ping)?;
            let addresses = self.ctx.amgr.lock().get_random_addresses(Default::default());
            self.router
                .enqueue(make_message!(
                    Payload::Addresses,
                    AddressesMessage {
                        address_list: addresses.into_iter().map(|addr| (addr.ip.into(), addr.port).into()).collect_vec()
                    }
                ))
                .await?;
        }
    }
}
