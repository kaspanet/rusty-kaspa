pub mod pb {
    // this one includes messages.proto + p2p.proto + rcp.proto
    tonic::include_proto!("protowire");
}
pub mod core;
pub mod echo;

mod payloadtype;

pub use payloadtype::KaspadMessagePayloadType;
