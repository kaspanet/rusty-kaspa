pub mod pb {
    // this one includes messages.proto + p2p.proto + rcp.proto
    tonic::include_proto!("protowire");
}

pub mod common;
pub mod convert;
pub mod echo;
pub mod flags;

mod core;
mod handshake;

pub use crate::core::adaptor::{Adaptor, ConnectionInitializer};
pub use crate::core::connection_handler::{ConnectionError, SocksAuth, SocksProxyConfig, SocksProxyParams};
pub use crate::core::hub::Hub;
pub use crate::core::payload_type::KaspadMessagePayloadType;
pub use crate::core::peer::{Peer, PeerKey, PeerProperties};
pub use crate::core::router::{IncomingRoute, Router, SharedIncomingRoute, BLANK_ROUTE_ID};
pub use flags::service as service_flags;
pub use handshake::KaspadHandshake;
