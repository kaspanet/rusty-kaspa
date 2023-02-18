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
pub use crate::core::payload_type::KaspadMessagePayloadType;
pub use crate::core::router::{IncomingRoute, Router};
pub use handshake::KaspadHandshake;
