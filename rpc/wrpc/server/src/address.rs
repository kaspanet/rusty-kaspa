use crate::service::WrpcEncoding;
use kaspa_consensus_core::network::NetworkType;
use kaspa_utils::networking::ContextualNetAddress;
use serde::Deserialize;
use std::{net::AddrParseError, str::FromStr};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename = "lowercase")]
pub enum WrpcNetAddress {
    Default,
    Public,
    Custom(ContextualNetAddress),
}

impl WrpcNetAddress {
    pub fn to_address(&self, network_type: &NetworkType, encoding: &WrpcEncoding) -> ContextualNetAddress {
        match self {
            WrpcNetAddress::Default => {
                let port = match encoding {
                    WrpcEncoding::Borsh => network_type.default_borsh_rpc_port(),
                    WrpcEncoding::SerdeJson => network_type.default_json_rpc_port(),
                };
                format!("127.0.0.1:{port}").parse().unwrap()
            }
            WrpcNetAddress::Public => {
                let port = match encoding {
                    WrpcEncoding::Borsh => network_type.default_borsh_rpc_port(),
                    WrpcEncoding::SerdeJson => network_type.default_json_rpc_port(),
                };
                format!("0.0.0.0:{port}").parse().unwrap()
            }
            WrpcNetAddress::Custom(address) => {
                if address.port_not_specified() {
                    let port = match encoding {
                        WrpcEncoding::Borsh => network_type.default_borsh_rpc_port(),
                        WrpcEncoding::SerdeJson => network_type.default_json_rpc_port(),
                    };
                    address.with_port(port)
                } else {
                    *address
                }
            }
        }
    }
}
impl FromStr for WrpcNetAddress {
    type Err = AddrParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "default" => Ok(WrpcNetAddress::Default),
            "public" => Ok(WrpcNetAddress::Public),
            _ => {
                let addr: ContextualNetAddress = s.parse()?;
                Ok(Self::Custom(addr))
            }
        }
    }
}

impl TryFrom<&str> for WrpcNetAddress {
    type Error = AddrParseError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        WrpcNetAddress::from_str(s)
    }
}

impl TryFrom<String> for WrpcNetAddress {
    type Error = AddrParseError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        WrpcNetAddress::from_str(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_utils::networking::IpAddress;

    #[test]
    fn test_wrpc_net_address_from_str() {
        // Addresses
        let port: u16 = 8080;
        let addr = format!("1.2.3.4:{port}").parse::<WrpcNetAddress>().unwrap();
        let addr_without_port = "1.2.3.4".parse::<WrpcNetAddress>().unwrap();
        let ip_addr = "1.2.3.4".parse::<IpAddress>().unwrap();
        // Test
        for schema in WrpcEncoding::iter() {
            for network in NetworkType::iter() {
                let expected_port = match schema {
                    WrpcEncoding::Borsh => Some(network.default_borsh_rpc_port()),
                    WrpcEncoding::SerdeJson => Some(network.default_json_rpc_port()),
                };
                // Custom address with port
                assert_eq!(addr.to_address(&network, schema), ContextualNetAddress::new(ip_addr, Some(port)));
                // Custom address without port
                assert_eq!(addr_without_port.to_address(&network, schema), ContextualNetAddress::new(ip_addr, expected_port))
            }
        }
    }
}
