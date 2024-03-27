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
    fn test_wrpc_net_address_from_str_custom() {
        // Addresses
        let addr = "1.2.3.4:8080".parse::<WrpcNetAddress>().unwrap();
        let addr_without_port = "1.2.3.4".parse::<WrpcNetAddress>().unwrap();
        // Network types
        let mainnet = NetworkType::Mainnet;
        let testnet = NetworkType::Testnet;
        let simnet = NetworkType::Simnet;
        let devnet = NetworkType::Devnet;
        // Encodings
        let borsh_encoding = WrpcEncoding::Borsh;
        let json_encoding = WrpcEncoding::SerdeJson;

        // Custom address with port, borsh, mainnet
        assert_eq!(
            addr.to_address(&mainnet, &borsh_encoding),
            ContextualNetAddress::new("1.2.3.4".parse::<IpAddress>().unwrap(), Some(8080 as u16))
        );

        // Custom address without port, borsh, mainnet
        assert_eq!(
            addr_without_port.to_address(&mainnet, &borsh_encoding),
            ContextualNetAddress::new(
                "1.2.3.4".parse::<IpAddress>().unwrap(),
                Some(mainnet.default_borsh_rpc_port())
            )
        );

        // Custom address without port, borsh, testnet
        assert_eq!(
            addr_without_port.to_address(&testnet, &borsh_encoding),
            ContextualNetAddress::new(
                "1.2.3.4".parse::<IpAddress>().unwrap(),
                Some(testnet.default_borsh_rpc_port())
            )
        );

        // Custom address without port, borsh, simnet
        assert_eq!(
            addr_without_port.to_address(&simnet, &borsh_encoding),
            ContextualNetAddress::new(
                "1.2.3.4".parse::<IpAddress>().unwrap(),
                Some(simnet.default_borsh_rpc_port())
            )
        );

        // Custom address without port, borsh, devnet
        assert_eq!(
            addr_without_port.to_address(&devnet, &borsh_encoding),
            ContextualNetAddress::new(
                "1.2.3.4".parse::<IpAddress>().unwrap(),
                Some(devnet.default_borsh_rpc_port())
            )
        );

        // Custom address without port, json, mainnet
        assert_eq!(
            addr_without_port.to_address(&mainnet, &json_encoding),
            ContextualNetAddress::new(
                "1.2.3.4".parse::<IpAddress>().unwrap(),
                Some(mainnet.default_json_rpc_port())
            )
        );

        // Custom address without port, json, testnet
        assert_eq!(
            addr_without_port.to_address(&testnet, &json_encoding),
            ContextualNetAddress::new(
                "1.2.3.4".parse::<IpAddress>().unwrap(),
                Some(testnet.default_json_rpc_port())
            )
        );

        // Custom address without port, json, simnet
        assert_eq!(
            addr_without_port.to_address(&simnet, &json_encoding),
            ContextualNetAddress::new(
                "1.2.3.4".parse::<IpAddress>().unwrap(),
                Some(simnet.default_json_rpc_port())
            )
        );

        // Custom address without port, json, devnet
        assert_eq!(
            addr_without_port.to_address(&devnet, &json_encoding),
            ContextualNetAddress::new(
                "1.2.3.4".parse::<IpAddress>().unwrap(),
                Some(devnet.default_json_rpc_port())
            )
        );

    }
}
