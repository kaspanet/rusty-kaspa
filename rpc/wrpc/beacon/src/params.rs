// use std::path::Display;
use crate::imports::*;
use std::fmt;

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Params {
    pub encoding: WrpcEncoding,
    pub network: NetworkId,
}

impl Params {
    pub fn new(encoding: WrpcEncoding, network: NetworkId) -> Self {
        Self { encoding, network }
    }

    pub fn iter() -> impl Iterator<Item = Params> {
        NetworkId::iter().flat_map(move |network_id| WrpcEncoding::iter().map(move |encoding| Params::new(*encoding, network_id)))
    }
}

impl fmt::Display for Params {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.encoding.to_string().to_lowercase(), self.network)
    }
}
