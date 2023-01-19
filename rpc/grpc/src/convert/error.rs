use crate::from;
use crate::protowire;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: rpc_core::RpcError, protowire::RpcError, { Self { message: item.to_string() } });
from!(item: &rpc_core::RpcError, protowire::RpcError, { Self { message: item.to_string() } });

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

from!(item: &protowire::RpcError, rpc_core::RpcError, { rpc_core::RpcError::from(item.message.to_string()) });
