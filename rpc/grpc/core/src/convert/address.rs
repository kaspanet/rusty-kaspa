use crate::protowire;
use crate::{from, try_from};
use kaspa_rpc_core::RpcError;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcBalancesByAddressesEntry, protowire::RpcBalancesByAddressesEntry, {
    // TODO: Add an error if the balance is None
    let error = Some(protowire::RpcError { message: "address has no balance".to_string() });
    Self { address: (&item.address).into(), balance: item.balance.unwrap_or_default(), error }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcBalancesByAddressesEntry, kaspa_rpc_core::RpcBalancesByAddressesEntry, {
    let balance = if item.error.is_some() { None } else { Some(item.balance) };
    Self { address: item.address.as_str().try_into()?, balance }
});
