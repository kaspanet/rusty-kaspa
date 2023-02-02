use crate::protowire;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

impl From<rpc_core::RpcError> for protowire::RpcError {
    fn from(item: rpc_core::RpcError) -> Self {
        Self { message: item.to_string() }
    }
}

impl From<&rpc_core::RpcError> for protowire::RpcError {
    fn from(item: &rpc_core::RpcError) -> Self {
        Self { message: item.to_string() }
    }
}

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

impl From<&protowire::RpcError> for rpc_core::RpcError {
    fn from(item: &protowire::RpcError) -> Self {
        rpc_core::RpcError::from(item.message.to_string())
    }
}
