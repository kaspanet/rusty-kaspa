#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;
pub mod pb {
    // this one includes messages.proto + p2p.proto + rcp.proto
    tonic::include_proto!("protowire");
}

pub mod common;
pub mod convert;
pub mod echo;

mod core;
mod handshake;

pub use crate::core::adaptor::{Adaptor, ConnectionInitializer};
pub use crate::core::connection_handler::ConnectionError;
pub use crate::core::hub::Hub;
pub use crate::core::payload_type::KaspadMessagePayloadType;
pub use crate::core::peer::{Peer, PeerKey, PeerOutboundType, PeerProperties};
pub use crate::core::router::{BLANK_ROUTE_ID, IncomingRoute, Router, SharedIncomingRoute};
pub use handshake::KaspadHandshake;
