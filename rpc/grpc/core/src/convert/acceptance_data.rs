use crate::protowire::{self};
use crate::{from, try_from};
use kaspa_rpc_core::RpcError;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcDataVerbosityLevel, protowire::RpcDataVerbosityLevel, {
    match item {
        kaspa_rpc_core::RpcDataVerbosityLevel::None => protowire::RpcDataVerbosityLevel::None,
        kaspa_rpc_core::RpcDataVerbosityLevel::Low => protowire::RpcDataVerbosityLevel::Low,
        kaspa_rpc_core::RpcDataVerbosityLevel::High => protowire::RpcDataVerbosityLevel::High,
        kaspa_rpc_core::RpcDataVerbosityLevel::Full => protowire::RpcDataVerbosityLevel::Full,
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcDataVerbosityLevel, kaspa_rpc_core::RpcDataVerbosityLevel,  {
    match item {
        protowire::RpcDataVerbosityLevel::None => kaspa_rpc_core::RpcDataVerbosityLevel::None,
        protowire::RpcDataVerbosityLevel::Low => kaspa_rpc_core::RpcDataVerbosityLevel::Low,
        protowire::RpcDataVerbosityLevel::High => kaspa_rpc_core::RpcDataVerbosityLevel::High,
        protowire::RpcDataVerbosityLevel::Full => kaspa_rpc_core::RpcDataVerbosityLevel::Full
    }
});
