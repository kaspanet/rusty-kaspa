use crate::{from, protowire, try_from};
use kaspa_rpc_core::{RpcError, RpcVerbosityTiers};

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcOptionalizedHeaderVerbosity, protowire::RpcOptionalizedBlockHeaderVerbosity, {
    Self {
            verbosity: item.verbosity.to_string(),
            include_parents_by_level: item.include_parents_by_level,
        }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcOptionalizedBlockHeaderVerbosity, kaspa_rpc_core::RpcOptionalizedHeaderVerbosity, {
Self {
        verbosity: RpcVerbosityTiers::try_from(item.verbosity.clone())?,
        include_parents_by_level: item.include_parents_by_level,
    }
});
