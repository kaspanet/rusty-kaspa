pub mod pb {
    // this one includes messages.proto + p2p.proto + rcp.proto
    tonic::include_proto!("protowire");
}

pub mod echo;

mod common;
mod core;
mod handshake;

pub use crate::core::adaptor::{Adaptor, ConnectionError, ConnectionInitializer};
pub use crate::core::payload_type::KaspadMessagePayloadType;
pub use crate::core::router::Router;
pub use handshake::KaspadHandshake;
