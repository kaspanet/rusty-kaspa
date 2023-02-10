pub mod pb {
    // this one includes messages.proto + p2p.proto + rcp.proto
    tonic::include_proto!("protowire");
}
pub mod adaptor;
pub mod infra;
pub mod registry;
