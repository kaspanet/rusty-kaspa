pub mod channel;
pub mod convert;
pub mod ext;
pub mod macros;
pub mod ops;

/// Maximum decoded gRPC message size to send and receive
pub const RPC_MAX_MESSAGE_SIZE: usize = 1024 * 1024 * 1024; // 1GB

pub mod protowire {
    tonic::include_proto!("protowire");
}
