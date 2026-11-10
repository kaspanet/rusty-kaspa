use crate::protowire;
use crate::{from, try_from};
use aero_rpc_core::RpcError;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &aero_rpc_core::RpcBalancesByAddressesEntry, protowire::RpcBalancesByAddressesEntry, {
    Self { address: (&item.address).into(), balance: item.balance.unwrap_or_default(), error: None }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcBalancesByAddressesEntry, aero_rpc_core::RpcBalancesByAddressesEntry, {
    let balance = if item.error.is_some() { None } else { Some(item.balance) };
    Self { address: item.address.as_str().try_into()?, balance }
});
