use crate::{flow_context::FlowContext, flow_trait::Flow};
use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::ProtocolError,
    dequeue, dequeue_with_timeout, make_message,
    pb::{AddressesMessage, RequestAddressesMessage, kaspad_message::Payload},
};
use kaspa_utils::connection::PeerAddress;
use rand::seq::IteratorRandom;
use std::{sync::Arc, time::Duration};

/// The maximum number of addresses that are sent in a single kaspa Addresses message.
const MAX_ADDRESSES_SEND: usize = 1000;

/// The maximum number of addresses that can be received in a single kaspa Addresses response.
/// If a peer exceeds this value we consider it a protocol error.
const MAX_ADDRESSES_RECEIVE: usize = 2500;
const ADDRESS_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

pub struct ReceiveAddressesFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for ReceiveAddressesFlow {
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
        loop {
            self.router
                .enqueue(make_message!(
                    Payload::RequestAddresses,
                    RequestAddressesMessage { include_all_subnetworks: false, subnetwork_id: None }
                ))
                .await?;
            let msg = dequeue_with_timeout!(self.incoming_route, Payload::Addresses)?;
            if msg.address_list.len() > MAX_ADDRESSES_RECEIVE {
                return Err(ProtocolError::Other("addresses message exceeds limit"));
            }
            let addresses = msg.address_list.into_iter().map(PeerAddress::try_from).collect::<Result<Vec<_>, _>>()?;
            {
                let mut manager = self.ctx.address_manager.lock();
                for address in addresses {
                    manager.add_address(address);
                }
            }
            tokio::select! {
                _ = self.router.closed() => return Ok(()),
                _ = tokio::time::sleep(ADDRESS_REFRESH_INTERVAL) => {}
            }
        }
    }
}

pub struct SendAddressesFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for SendAddressesFlow {
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
            let message = {
                let manager = self.ctx.address_manager.lock();
                let mut records = manager.iterator().choose_multiple(&mut rand::thread_rng(), MAX_ADDRESSES_SEND - 1);
                if let Some(own) = manager.local_advertisement() {
                    records.retain(|address| address.key() != own.key());
                    records.push(own);
                }
                let addresses = records.into_iter().map(Into::into).collect();
                AddressesMessage { address_list: addresses }
            };
            self.router.enqueue(make_message!(Payload::Addresses, message)).await?;
            // We wait 10 seconds to avoid a flood of requests.
            tokio::select! {
                _ = self.router.closed() => return Ok(()),
                _ = tokio::time::sleep(Duration::from_secs(10)) => {}
            }
        }
    }
}
