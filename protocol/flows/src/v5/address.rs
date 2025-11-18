use crate::{flow_context::FlowContext, flow_trait::Flow};
use kaspa_addressmanager::NetAddress;
use kaspa_p2p_lib::{
    common::ProtocolError,
    dequeue, dequeue_with_timeout, make_message,
    pb::{kaspad_message::Payload, AddressesMessage, RequestAddressesMessage},
    IncomingRoute, Router,
};
use rand::seq::SliceRandom;
use std::sync::Arc;

/// The maximum number of addresses that are sent in a single kaspa Addresses message.
const MAX_ADDRESSES_SEND: usize = 1000;

/// The maximum number of addresses that can be received in a single kaspa Addresses response.
/// If a peer exceeds this value we consider it a protocol error.
const MAX_ADDRESSES_RECEIVE: usize = 2500;

fn allow_onion_addresses(
    tor_proxy_configured: bool,
    tor_only_mode: bool,
    onion_service_active: bool,
    peer_supports_addrv2: bool,
) -> bool {
    (tor_proxy_configured || onion_service_active || tor_only_mode) && peer_supports_addrv2
}

fn collect_gossipable_addresses<I>(addresses: I, allow_onion: bool) -> Vec<NetAddress>
where
    I: IntoIterator<Item = NetAddress>,
{
    addresses.into_iter().filter(|addr| allow_onion || addr.as_onion().is_none()).collect()
}

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
        self.router
            .enqueue(make_message!(
                Payload::RequestAddresses,
                RequestAddressesMessage { include_all_subnetworks: false, subnetwork_id: None }
            ))
            .await?;

        let msg = dequeue_with_timeout!(self.incoming_route, Payload::Addresses)?;
        let address_list: Vec<NetAddress> = msg.try_into()?;
        if address_list.len() > MAX_ADDRESSES_RECEIVE {
            return Err(ProtocolError::OtherOwned(format!("address count {} exceeded {}", address_list.len(), MAX_ADDRESSES_RECEIVE)));
        }
        let peer_properties = self.router.properties();
        let allow_onion = allow_onion_addresses(
            self.ctx.tor_proxy().is_some(),
            self.ctx.tor_only(),
            self.ctx.onion_service_address().is_some(),
            peer_properties.supports_addrv2,
        );
        let filtered_addresses = collect_gossipable_addresses(address_list, allow_onion);

        let mut amgr_lock = self.ctx.address_manager.lock();
        for addr in filtered_addresses {
            amgr_lock.add_address(addr)
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
            let peer_properties = self.router.properties();
            let allow_onion = allow_onion_addresses(
                self.ctx.tor_proxy().is_some(),
                self.ctx.tor_only(),
                self.ctx.onion_service_address().is_some(),
                peer_properties.supports_addrv2,
            );
            let addresses = {
                let manager = self.ctx.address_manager.lock();
                collect_gossipable_addresses(manager.iterate_addresses(), allow_onion)
            };
            let address_list =
                addresses.choose_multiple(&mut rand::thread_rng(), MAX_ADDRESSES_SEND).map(|addr| (*addr).into()).collect();
            self.router.enqueue(make_message!(Payload::Addresses, AddressesMessage { address_list })).await?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_utils::networking::{IpAddress, OnionAddress};
    use std::net::Ipv4Addr;

    #[test]
    fn allow_onion_addresses_requires_tor_and_peer_support() {
        // No Tor configuration, even if the peer supports addrv2 we keep onions disabled.
        assert!(!allow_onion_addresses(false, false, false, true));
        // Tor is configured but the peer does not signal addrv2 support.
        assert!(!allow_onion_addresses(true, false, false, false));
        // Either tor proxy, tor-only mode or an onion service combined with addrv2 enables onions.
        assert!(allow_onion_addresses(true, false, false, true));
        assert!(allow_onion_addresses(false, true, false, true));
        assert!(allow_onion_addresses(false, false, true, true));
        // Onion service without addrv2 support should still reject onions.
        assert!(!allow_onion_addresses(false, false, true, false));
    }

    #[test]
    fn collect_gossipable_addresses_filters_when_not_allowed() {
        let ipv4 = NetAddress::new(IpAddress::from(Ipv4Addr::LOCALHOST), 16110);
        let onion = NetAddress::new_onion(
            OnionAddress::try_from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.onion").unwrap(),
            16110,
        );

        let filtered = collect_gossipable_addresses(vec![ipv4, onion], false);
        assert_eq!(filtered, vec![ipv4]);

        let filtered = collect_gossipable_addresses(vec![ipv4, onion], true);
        assert_eq!(filtered, vec![ipv4, onion]);
    }
}
