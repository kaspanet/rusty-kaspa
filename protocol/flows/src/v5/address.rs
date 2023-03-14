use crate::{flow_context::FlowContext, flow_trait::Flow};
use addressmanager::NetAddress;

use itertools::Itertools;

use p2p_lib::{
    common::ProtocolError,
    dequeue, dequeue_with_timeout, make_message,
    pb::{kaspad_message::Payload, AddressesMessage, RequestAddressesMessage},
    IncomingRoute, Router,
};

use std::{net::IpAddr, sync::Arc};

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

        let msg = dequeue_with_timeout!(self.incoming_route, Payload::Addresses)?;
        let address_list: Vec<(IpAddr, u16)> = msg.try_into()?;
        let mut amgr_lock = self.ctx.amgr.lock();
        for (ip, port) in address_list {
            amgr_lock.add_address(NetAddress::new(ip, port))
        }

        Ok(())
    }
}

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
            dequeue!(self.incoming_route, Payload::RequestAddresses)?;
            let addresses = self.ctx.amgr.lock().get_random_addresses(Default::default());
            self.router
                .enqueue(make_message!(
                    Payload::Addresses,
                    AddressesMessage { address_list: addresses.into_iter().map(|addr| (addr.ip, addr.port).into()).collect_vec() }
                ))
                .await?;
        }
    }
}
