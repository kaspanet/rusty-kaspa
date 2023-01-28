pub mod pb {
    // this one includes messages.proto + p2p.proto + rcp.proto
    tonic::include_proto!("protowire");
}
pub mod kaspa_flows;
pub mod kaspa_grpc;
pub mod kaspa_p2p;
