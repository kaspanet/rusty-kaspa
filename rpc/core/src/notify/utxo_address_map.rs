extern crate derive_more;
use ahash::AHashMap;
use derive_more::Deref;

use crate::stubs::RpcUtxoAddress;

/// A newtype allowing conversion Vec<RpcUtxoAddress> to AHashMap<RpcUtxoAddress, ()>.
#[derive(Clone, Debug, Deref, Default)]
pub struct RpcUtxoAddressMap(AHashMap<RpcUtxoAddress, ()>);

impl RpcUtxoAddressMap {
    pub fn new() -> Self {
        Self(AHashMap::new())
    }
}

impl From<&Vec<RpcUtxoAddress>> for RpcUtxoAddressMap {
    fn from(item: &Vec<RpcUtxoAddress>) -> Self {
        Self(item.iter().map(|x| (x.clone(), ())).collect())
    }
}

/// Two [RpcUtxoAddressMap] are equal if there respective key set
/// are identical no matter the order of keys in sets.
impl PartialEq for RpcUtxoAddressMap {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len() && self.0.keys().all(|k| other.0.contains_key(k))
    }
}
