use std::net::IpAddr;

pub use kaspa_utils::networking::{AddressKind, NetAddress};

pub(super) mod address_store;
pub(super) mod banned_address_store;

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub struct AddressKey {
    kind: AddressKind,
    port: u16,
}

impl AddressKey {
    pub fn new(kind: AddressKind, port: u16) -> Self {
        Self { kind, port }
    }

    pub fn is_ip(&self, ip: IpAddr) -> bool {
        match self.kind {
            AddressKind::Ip(stored) => IpAddr::from(stored) == ip,
            AddressKind::Onion(_) => false,
        }
    }

    pub fn kind(&self) -> AddressKind {
        self.kind
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl From<NetAddress> for AddressKey {
    fn from(value: NetAddress) -> Self {
        AddressKey::new(value.ip, value.port)
    }
}
