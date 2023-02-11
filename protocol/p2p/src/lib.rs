pub mod pb {
    // this one includes messages.proto + p2p.proto + rcp.proto
    tonic::include_proto!("protowire");
}

pub mod echo;

mod adaptor;
mod connection;
mod hub;
mod payload_type;
mod router;

pub use adaptor::{Adaptor, ConnectionError, ConnectionInitializer};
pub use payload_type::KaspadMessagePayloadType;
pub use router::Router;
