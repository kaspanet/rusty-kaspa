// #![allow(dead_code)]
use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use borsh::{BorshDeserialize, BorshSerialize};
use core::{
    fmt::Display,
    net::{AddrParseError, IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    ops::Deref,
    str::FromStr,
    time::Duration,
};
use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use thiserror::Error;
#[cfg(feature = "peer-id")]
use uuid::Uuid;
use wasm_bindgen::prelude::*;

// A network address serialization of [`ContextualNetAddress`].
#[wasm_bindgen(typescript_custom_section)]
const TS_IP_ADDRESS: &'static str = r#"
    /**
     * Generic network address representation.
     * 
     * @category General
     */
    export interface INetworkAddress {
        /**
         * IPv4 or IPv6 address.
         */
        ip: string;
        /**
         * Optional port number.
         */
        port?: number;
    }
"#;

/// A bucket based on an ip's prefix bytes.
/// for ipv4 it consists of 6 leading zero bytes, and the first two octets,
/// for ipv6 it consists of the first 8 octets,
/// encoded into a big endian u64.
#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug)]
pub struct PrefixBucket(u64);

impl PrefixBucket {
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<&IpAddress> for PrefixBucket {
    fn from(ip_address: &IpAddress) -> Self {
        match ip_address.0 {
            IpAddr::V4(ipv4) => {
                let prefix_bytes = ipv4.octets();
                Self(u64::from_be_bytes([0u8, 0u8, 0u8, 0u8, 0u8, 0u8, prefix_bytes[0], prefix_bytes[1]]))
            }
            IpAddr::V6(ipv6) => {
                if let Some(ipv4) = ipv6.to_ipv4() {
                    let prefix_bytes = ipv4.octets();
                    Self(u64::from_be_bytes([0u8, 0u8, 0u8, 0u8, 0u8, 0u8, prefix_bytes[0], prefix_bytes[1]]))
                } else {
                    // Else use first 8 bytes (routing prefix + subnetwork id) of ipv6
                    Self(u64::from_be_bytes(ipv6.octets().as_slice()[..8].try_into().expect("Slice with incorrect length")))
                }
            }
        }
    }
}

impl From<&NetAddress> for PrefixBucket {
    fn from(net_address: &NetAddress) -> Self {
        Self::from(&net_address.ip)
    }
}

/// An IP address, newtype of [IpAddr].
#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize, Debug)]
#[repr(transparent)]
pub struct IpAddress(pub IpAddr);

impl IpAddress {
    pub fn new(ip: IpAddr) -> Self {
        Self(ip)
    }

    pub fn is_publicly_routable(&self) -> bool {
        if self.is_loopback() || self.is_unspecified() {
            return false;
        }

        match self.0 {
            IpAddr::V4(ip) => {
                // RFC 1918 is covered by is_private
                // RFC 5737 is covered by is_documentation
                // RFC 3927 is covered by is_link_local
                // RFC  919 is covered by is_broadcast (wasn't originally covered in go code)
                if ip.is_broadcast() || ip.is_private() || ip.is_documentation() || ip.is_link_local() {
                    return false;
                }
            }
            IpAddr::V6(_ip) => {
                // All of the is_ helper functions for ipv6 are currently marked unstable
            }
        }

        // Based on values from network.go
        let unroutable_nets = [
            "198.18.0.0/15",   // RFC 2544
            "2001:DB8::/32",   // RFC 3849
            "2002::/16",       // RFC 3964
            "FC00::/7",        // RFC 4193
            "2001::/32",       // RFC 4380
            "2001:10::/28",    // RFC 4843
            "FE80::/64",       // RFC 4862
            "64:FF9B::/96",    // RFC 6052
            "::FFFF:0:0:0/96", // RFC 6145
            "100.64.0.0/10",   // RFC 6598
            "0.0.0.0/8",       // Zero Net
            "2001:470::/32",   // Hurricane Electric IPv6 address block.
        ];

        for curr_net in unroutable_nets {
            if IpNet::from_str(curr_net).unwrap().contains(&self.0) {
                return false;
            }
        }

        true
    }

    pub fn prefix_bucket(&self) -> PrefixBucket {
        PrefixBucket::from(self)
    }
}

impl From<IpAddr> for IpAddress {
    fn from(ip: IpAddr) -> Self {
        Self(ip)
    }
}
impl From<Ipv4Addr> for IpAddress {
    fn from(value: Ipv4Addr) -> Self {
        Self(value.into())
    }
}
impl From<Ipv6Addr> for IpAddress {
    fn from(value: Ipv6Addr) -> Self {
        Self(value.into())
    }
}
impl From<IpAddress> for IpAddr {
    fn from(value: IpAddress) -> Self {
        value.0
    }
}

impl FromStr for IpAddress {
    type Err = AddrParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        IpAddr::from_str(s).map(IpAddress::from)
    }
}

impl Display for IpAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for IpAddress {
    type Target = IpAddr;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

//
// Borsh serializers need to be manually implemented for `NetAddress` since
// IpAddr does not currently support Borsh
//

impl BorshSerialize for IpAddress {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> ::core::result::Result<(), borsh::io::Error> {
        let variant_idx: u8 = match self.0 {
            IpAddr::V4(..) => 0u8,
            IpAddr::V6(..) => 1u8,
        };
        writer.write_all(&variant_idx.to_le_bytes())?;
        match self.0 {
            IpAddr::V4(id0) => {
                borsh::BorshSerialize::serialize(&id0.octets(), writer)?;
            }
            IpAddr::V6(id0) => {
                borsh::BorshSerialize::serialize(&id0.octets(), writer)?;
            }
        }
        Ok(())
    }
}

impl BorshDeserialize for IpAddress {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> ::core::result::Result<Self, borsh::io::Error> {
        let variant_idx: u8 = BorshDeserialize::deserialize_reader(reader)?;
        let ip = match variant_idx {
            0u8 => {
                let octets: [u8; 4] = BorshDeserialize::deserialize_reader(reader)?;
                IpAddr::V4(Ipv4Addr::from(octets))
            }
            1u8 => {
                let octets: [u8; 16] = BorshDeserialize::deserialize_reader(reader)?;
                IpAddr::V6(Ipv6Addr::from(octets))
            }
            _ => {
                let msg = format!("Unexpected variant index: {:?}", variant_idx);
                return Err(borsh::io::Error::new(borsh::io::ErrorKind::InvalidInput, msg));
            }
        };
        Ok(Self(ip))
    }
}

/// A network address, equivalent of a [SocketAddr].
#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize, Debug, BorshSerialize, BorshDeserialize)]
pub struct NetAddress {
    pub ip: IpAddress,
    pub port: u16,
}

impl NetAddress {
    pub fn new(ip: IpAddress, port: u16) -> Self {
        Self { ip, port }
    }

    pub fn prefix_bucket(&self) -> PrefixBucket {
        PrefixBucket::from(self)
    }
}

impl From<SocketAddr> for NetAddress {
    fn from(value: SocketAddr) -> Self {
        Self::new(value.ip().into(), value.port())
    }
}

impl From<NetAddress> for SocketAddr {
    fn from(value: NetAddress) -> Self {
        Self::new(value.ip.0, value.port)
    }
}

impl FromStr for NetAddress {
    type Err = AddrParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SocketAddr::from_str(s).map(NetAddress::from)
    }
}

impl Display for NetAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        SocketAddr::from(self.to_owned()).fmt(f)
    }
}

/// A network address possibly without explicit port.
///
/// Use `normalize` to get a fully determined address.
#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize, Debug, BorshSerialize, BorshDeserialize)]
pub struct ContextualNetAddress {
    ip: IpAddress,
    port: Option<u16>,
}

impl ContextualNetAddress {
    pub fn new(ip: IpAddress, port: Option<u16>) -> Self {
        Self { ip, port }
    }

    pub fn has_port(&self) -> bool {
        self.port.is_some()
    }

    pub fn normalize(&self, default_port: u16) -> NetAddress {
        NetAddress::new(self.ip, self.port.unwrap_or(default_port))
    }

    pub fn unspecified() -> Self {
        Self { ip: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)).into(), port: None }
    }

    pub fn loopback() -> Self {
        Self { ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)).into(), port: None }
    }

    pub fn port_not_specified(&self) -> bool {
        self.port.is_none()
    }

    pub fn with_port(&self, port: u16) -> Self {
        Self { ip: self.ip, port: Some(port) }
    }
}

impl From<NetAddress> for ContextualNetAddress {
    fn from(value: NetAddress) -> Self {
        Self::new(value.ip, Some(value.port))
    }
}

impl FromStr for ContextualNetAddress {
    type Err = AddrParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match SocketAddr::from_str(s) {
            Ok(socket) => Ok(Self::new(socket.ip().into(), Some(socket.port()))),
            Err(_) => Ok(Self::new(IpAddress::from_str(s)?, None)),
        }
    }
}

impl TryFrom<&str> for ContextualNetAddress {
    type Error = AddrParseError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        ContextualNetAddress::from_str(s)
    }
}

impl TryFrom<String> for ContextualNetAddress {
    type Error = AddrParseError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        ContextualNetAddress::from_str(&s)
    }
}

impl Display for ContextualNetAddress {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.port {
            Some(port) => SocketAddr::new(self.ip.into(), port).fmt(f),
            None => self.ip.fmt(f),
        }
    }
}

/// Maximum wall-clock time for one DNS lookup attempt by
/// [`PeerEndpoint::resolve`]. Bounded so a slow or unreachable
/// resolver cannot stall the dial loop or the RPC handler.
pub const PEER_ENDPOINT_RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

/// Errors from textual parsing of a [`PeerEndpoint`].
#[derive(Error, Debug, PartialEq, Eq, Clone)]
pub enum PeerEndpointParseError {
    #[error("empty endpoint string")]
    Empty,
    #[error("invalid port `{0}`")]
    InvalidPort(String),
    #[error("invalid hostname: {reason}")]
    InvalidHostname { reason: &'static str },
}

/// Errors from async DNS resolution of a [`PeerEndpoint`].
#[derive(Error, Debug)]
pub enum PeerEndpointResolveError {
    #[error("DNS lookup of `{host}` timed out after {timeout:?}")]
    Timeout { host: String, timeout: Duration },
    #[error("DNS lookup of `{host}` failed: {source}")]
    Lookup { host: String, source: std::io::Error },
}

/// Wrap a hostname-resolution future in the canonical
/// [`PEER_ENDPOINT_RESOLVE_TIMEOUT`]. Returns the resolver's raw
/// `Vec<SocketAddr>` payload so callers that need a different
/// projection (e.g. [`PeerEndpoint::resolve`] returning
/// `Vec<NetAddress>`, or `kaspa_connectionmanager::TokioHostnameResolver`
/// returning `Vec<SocketAddr>` directly) apply their own conversion
/// without redundant timeout/error-mapping bodies.
///
/// Public so the hostname-resolver implementation in
/// `kaspa-connectionmanager` shares a single source-of-truth for the
/// timeout bound and the `PeerEndpointResolveError` mapping; the
/// previous in-tree duplicate had drift risk if the timeout
/// constant or the error variant set ever changed.
#[cfg(not(target_arch = "wasm32"))]
pub async fn resolve_with_timeout<F>(host: &str, lookup: F) -> Result<Vec<SocketAddr>, PeerEndpointResolveError>
where
    F: std::future::Future<Output = std::io::Result<Vec<SocketAddr>>>,
{
    tokio::time::timeout(PEER_ENDPOINT_RESOLVE_TIMEOUT, lookup)
        .await
        .map_err(|_| PeerEndpointResolveError::Timeout { host: host.to_owned(), timeout: PEER_ENDPOINT_RESOLVE_TIMEOUT })?
        .map_err(|source| PeerEndpointResolveError::Lookup { host: host.to_owned(), source })
}

/// A peer endpoint as accepted from operator-facing inputs (`kaspad
/// --addpeer/--connect`, the kaspa-cli interactive `addpeer`, the RPC
/// `AddPeer` method).
///
/// Either a numeric IP literal (already resolved) or a textual
/// hostname (resolved at dial time, never at parse time). The
/// distinction is preserved as first-class state so the connection
/// manager can periodically re-resolve hostname-origin entries and
/// reconcile the resulting socket-address sets.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum PeerEndpoint {
    /// Numeric IP literal -- no DNS required at any point in its lifecycle.
    Address(ContextualNetAddress),
    /// Textual hostname -- resolved by the connection manager.
    Hostname { host: String, port: Option<u16> },
}

impl PeerEndpoint {
    /// Textual parse only -- never performs DNS.
    ///
    /// Order of operations: try [`ContextualNetAddress::from_str`]
    /// first (catches every numeric form including bracketed IPv6);
    /// if that fails, treat the input as a hostname and validate it
    /// under a strict RFC 1123 policy: total length 1..=253; labels
    /// separated by `.`; each label 1..=63 ASCII letters, digits,
    /// hyphens; no leading/trailing hyphen per label; no underscores;
    /// no empty labels.
    ///
    /// Hostnames are canonicalised before storage so different textual
    /// forms of the same conceptual host map to a single registry
    /// entry (and hash to a single key, and round-trip through
    /// [`Display`] to a single canonical string):
    ///
    /// - **Case folding:** ASCII-lowercased (RFC 1123 Sec.2.1:
    ///   hostnames are case-insensitive).
    /// - **Trailing-dot stripping:** RFC 1034 Sec.3.1 admits a single
    ///   trailing `.` as the rooted/absolute FQDN form; the relative
    ///   form (no trailing dot) is semantically equivalent for peer
    ///   dialing, so the canonical stored form has the dot removed.
    ///   `parse("example.com.")` and `parse("example.com")` produce
    ///   equal [`PeerEndpoint::Hostname`] values; the OS resolver is
    ///   handed the same target string in either case, so the
    ///   trailing dot's resolver-side suffix-search-suppression
    ///   behaviour is no longer surface-divergent on the rusty-kaspa
    ///   side.
    pub fn parse(s: &str) -> Result<Self, PeerEndpointParseError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(PeerEndpointParseError::Empty);
        }
        if let Ok(addr) = ContextualNetAddress::from_str(s) {
            return Ok(Self::Address(addr));
        }
        let (host, port) = split_host_port(s)?;
        validate_hostname_rfc1123(host)?;
        // Canonical store form: lowercase + trailing-dot stripped.
        // The validator already accepts both forms (it strips a single
        // trailing `.` for the syntactic check); stripping here too
        // collapses them at the type level so registry keying, hashing,
        // equality, and Display round-trip are single-canonical-form.
        let host_canonical = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
        Ok(Self::Hostname { host: host_canonical, port })
    }

    /// Async resolution to one or more [`NetAddress`] records.
    ///
    /// `Address` variants resolve trivially (no DNS). `Hostname`
    /// variants call [`tokio::net::lookup_host`] wrapped in
    /// [`tokio::time::timeout`] with [`PEER_ENDPOINT_RESOLVE_TIMEOUT`].
    /// Multi-record results yield one `NetAddress` per resolved
    /// socket address, in the order returned by the OS resolver.
    pub async fn resolve(&self, default_port: u16) -> Result<Vec<NetAddress>, PeerEndpointResolveError> {
        match self {
            Self::Address(addr) => Ok(vec![addr.normalize(default_port)]),
            Self::Hostname { host, port } => {
                let port = port.unwrap_or(default_port);
                let target = format!("{host}:{port}");
                let lookup = async move {
                    let iter = tokio::net::lookup_host(target).await?;
                    Ok::<Vec<SocketAddr>, std::io::Error>(iter.collect())
                };
                let sockets = resolve_with_timeout(host, lookup).await?;
                Ok(sockets.into_iter().map(NetAddress::from).collect())
            }
        }
    }

    /// Hostname for diagnostics; `None` for the `Address` variant.
    pub fn hostname(&self) -> Option<&str> {
        match self {
            Self::Address(_) => None,
            Self::Hostname { host, .. } => Some(host.as_str()),
        }
    }
}

impl Display for PeerEndpoint {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Address(addr) => addr.fmt(f),
            Self::Hostname { host, port: None } => f.write_str(host),
            Self::Hostname { host, port: Some(p) } => write!(f, "{host}:{p}"),
        }
    }
}

impl FromStr for PeerEndpoint {
    type Err = PeerEndpointParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for PeerEndpoint {
    type Error = PeerEndpointParseError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::parse(s)
    }
}

impl TryFrom<String> for PeerEndpoint {
    type Error = PeerEndpointParseError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::parse(&s)
    }
}

/// Split a hostname-form input into `(host, optional port)`. IPv6
/// numeric literals are caught earlier by the numeric parse path, so
/// `s` here is guaranteed not to contain `[`/`]` brackets -- the only
/// `:` that may appear is the port separator.
fn split_host_port(s: &str) -> Result<(&str, Option<u16>), PeerEndpointParseError> {
    match s.rfind(':') {
        None => Ok((s, None)),
        Some(idx) => {
            let (host, rest) = s.split_at(idx);
            let port_str = &rest[1..];
            if port_str.is_empty() {
                return Err(PeerEndpointParseError::InvalidPort(rest.to_owned()));
            }
            let port = port_str.parse::<u16>().map_err(|_| PeerEndpointParseError::InvalidPort(port_str.to_owned()))?;
            Ok((host, Some(port)))
        }
    }
}

/// Validate a hostname under RFC 1123 with the strict policy used by
/// `PeerEndpoint::parse`: total length 1..=253; labels separated by
/// `.`; each label 1..=63 ASCII letters, digits, hyphens; no
/// leading/trailing hyphen per label; no underscores; no empty
/// labels; rightmost label MUST contain at least one ASCII letter
/// (RFC 1123 Sec.2.1: "at least the highest-level component label
/// will be alphabetic"). Strict so typos surface at parse time, not
/// during DNS.
///
/// A single trailing `.` (the rooted/absolute FQDN form admitted by
/// RFC 1034 Sec.3.1) is stripped before validation: operators copying
/// hostnames from `dig +short` output or BIND zone files commonly
/// carry the trailing dot, and it is semantically equivalent to the
/// relative form for peer dialing.
fn validate_hostname_rfc1123(host: &str) -> Result<(), PeerEndpointParseError> {
    const MAX_HOSTNAME: usize = 253;
    const MAX_LABEL: usize = 63;
    let host = host.strip_suffix('.').unwrap_or(host);
    if host.is_empty() {
        return Err(PeerEndpointParseError::InvalidHostname { reason: "empty hostname" });
    }
    if host.len() > MAX_HOSTNAME {
        return Err(PeerEndpointParseError::InvalidHostname { reason: "hostname exceeds 253 characters" });
    }
    for label in host.split('.') {
        if label.is_empty() {
            return Err(PeerEndpointParseError::InvalidHostname { reason: "empty label (consecutive '.' or leading/trailing '.')" });
        }
        if label.len() > MAX_LABEL {
            return Err(PeerEndpointParseError::InvalidHostname { reason: "label exceeds 63 characters" });
        }
        let bytes = label.as_bytes();
        if bytes[0] == b'-' || bytes[bytes.len() - 1] == b'-' {
            return Err(PeerEndpointParseError::InvalidHostname { reason: "label starts or ends with hyphen" });
        }
        for &b in bytes {
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(PeerEndpointParseError::InvalidHostname {
                    reason: "label contains non-LDH (non-letter/digit/hyphen) character",
                });
            }
        }
    }
    // RFC 1123 Sec.2.1: the rightmost label of a hostname must contain
    // at least one ASCII letter so it cannot be confused with a
    // dotted-decimal IP literal. Numeric-only labels in non-rightmost
    // positions remain accepted (e.g. `host.123.example`). The empty-
    // host and empty-label cases are caught above, so `rsplit('.')`
    // always yields at least one non-empty label here.
    let rightmost = host.rsplit('.').next().expect("host is non-empty and labels are non-empty (checked above)");
    if !rightmost.bytes().any(|b| b.is_ascii_alphabetic()) {
        return Err(PeerEndpointParseError::InvalidHostname {
            reason: "rightmost label must contain at least one ASCII letter (RFC 1123 Sec.2.1)",
        });
    }
    Ok(())
}

#[cfg(feature = "peer-id")]
#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize, Debug, Default)]
#[repr(transparent)]
pub struct PeerId(pub Uuid);

#[cfg(feature = "peer-id")]
impl PeerId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, uuid::Error> {
        Ok(Uuid::from_slice(bytes)?.into())
    }
}

#[cfg(feature = "peer-id")]
impl From<Uuid> for PeerId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

#[cfg(feature = "peer-id")]
impl From<PeerId> for Uuid {
    fn from(value: PeerId) -> Self {
        value.0
    }
}

#[cfg(feature = "peer-id")]
impl FromStr for PeerId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::from_str(s).map(PeerId::from)
    }
}

#[cfg(feature = "peer-id")]
impl Display for PeerId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(feature = "peer-id")]
impl Deref for PeerId {
    type Target = Uuid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

//
// Borsh serializers need to be manually implemented for `PeerId` since
// Uuid does not currently support Borsh
//
#[cfg(feature = "peer-id")]
impl BorshSerialize for PeerId {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> ::core::result::Result<(), borsh::io::Error> {
        borsh::BorshSerialize::serialize(&self.0.as_bytes(), writer)?;
        Ok(())
    }
}

#[cfg(feature = "peer-id")]
impl BorshDeserialize for PeerId {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> ::core::result::Result<Self, borsh::io::Error> {
        let bytes: uuid::Bytes = BorshDeserialize::deserialize_reader(reader)?;
        Ok(Self::new(Uuid::from_bytes(bytes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use std::str::FromStr;

    #[test]
    fn test_ip_address_borsh() {
        // Tests for IpAddress Borsh ser/deser since we manually implemented them
        let ip: IpAddress = Ipv4Addr::from([44u8; 4]).into();
        let bin = borsh::to_vec(&ip).unwrap();
        let ip2: IpAddress = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(ip, ip2);

        let ip: IpAddress = Ipv6Addr::from([66u8; 16]).into();
        let bin = borsh::to_vec(&ip).unwrap();
        let ip2: IpAddress = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(ip, ip2);
    }

    #[test]
    fn test_peer_id_borsh() {
        // Tests for PeerId Borsh ser/deser since we manually implemented them
        let id: PeerId = Uuid::new_v4().into();
        let bin = borsh::to_vec(&id).unwrap();
        let id2: PeerId = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(id, id2);

        let id: PeerId = Uuid::from_bytes([123u8; 16]).into();
        let bin = borsh::to_vec(&id).unwrap();
        let id2: PeerId = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(id, id2);
    }

    #[test]
    fn test_net_address_from_str() {
        let addr_v4 = NetAddress::from_str("1.2.3.4:5678");
        assert!(addr_v4.is_ok());
        let addr_v6 = NetAddress::from_str("[2a01:4f8:191:1143::2]:5678");
        assert!(addr_v6.is_ok());
    }

    #[test]
    fn test_prefix_bucket() {
        let prefix_bytes: [u8; 2] = [42u8, 43u8];
        let addr = NetAddress::from_str(format!("{0}.{1}.3.4:5678", prefix_bytes[0], prefix_bytes[1]).as_str()).unwrap();
        assert!(addr.prefix_bucket() == PrefixBucket(u16::from_be_bytes(prefix_bytes) as u64));
    }

    #[test]
    fn test_contextual_address_ser() {
        let addr = IpAddress::from_str("127.0.0.1").unwrap();
        let port = Some(1234);
        let net_addr = ContextualNetAddress::new(addr, port);
        let s = serde_json::to_string(&net_addr).unwrap();
        assert_eq!(s, r#"{"ip":"127.0.0.1","port":1234}"#);
    }

    #[test]
    fn test_is_publicly_routable() {
        // RFC 2544 tests
        assert!(!IpAddress::from_str("198.18.0.0").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("198.19.255.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("198.17.255.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("198.20.0.0").unwrap().is_publicly_routable());

        // Zero net tests
        assert!(!IpAddress::from_str("0.0.0.0").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("0.0.0.1").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("0.0.1.0").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("0.1.0.0").unwrap().is_publicly_routable());

        // RFC 3849
        assert!(!IpAddress::from_str("2001:db8::").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("2001:db8:ffff:ffff:ffff:ffff:ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("2001:db7:ffff:ffff:ffff:ffff:ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("2001:db9::").unwrap().is_publicly_routable());

        // Localhost
        assert!(!IpAddress::from_str("127.0.0.1").unwrap().is_publicly_routable());

        // Some random routable IP
        assert!(IpAddress::from_str("123.45.67.89").unwrap().is_publicly_routable());

        // RFC 1918
        assert!(!IpAddress::from_str("10.0.0.0").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("10.255.255.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("9.255.255.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("11.0.0.0").unwrap().is_publicly_routable());

        assert!(!IpAddress::from_str("172.16.0.0").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("172.31.255.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("172.15.255.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("172.32.0.0").unwrap().is_publicly_routable());

        assert!(!IpAddress::from_str("192.168.0.0").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("192.168.255.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("192.167.255.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("192.169.0.0").unwrap().is_publicly_routable());

        // RFC 3927
        assert!(!IpAddress::from_str("169.254.0.0").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("169.254.255.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("169.253.255.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("169.255.0.0").unwrap().is_publicly_routable());

        // RFC 3964
        assert!(!IpAddress::from_str("2002::").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("2002:ffff:ffff:ffff:ffff:ffff:ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("2001:ffff:ffff:ffff:ffff:ffff:ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("2003::").unwrap().is_publicly_routable());

        // RFC 4193
        assert!(!IpAddress::from_str("fc00::").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("fdff:ffff:ffff:ffff:ffff:ffff:ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("fb00:ffff:ffff:ffff:ffff:ffff:ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("fe00::").unwrap().is_publicly_routable());

        // RFC 4380
        assert!(!IpAddress::from_str("2001::").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("2001:0:ffff:ffff:ffff:ffff:ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("2000:0:ffff:ffff:ffff:ffff:ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("2001:1::").unwrap().is_publicly_routable());

        // RFC 4843
        assert!(!IpAddress::from_str("2001:10::").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("2001:1f:ffff:ffff:ffff:ffff:ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("2001:f:ffff:ffff:ffff:ffff:ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("2001:20::").unwrap().is_publicly_routable());

        // RFC 4862
        assert!(!IpAddress::from_str("fe80::").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("fe80::ffff:ffff:ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("fe7f::ffff:ffff:ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("fe81::").unwrap().is_publicly_routable());

        // RFC 5737
        assert!(!IpAddress::from_str("192.0.2.0").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("192.0.2.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("192.0.1.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("192.0.3.0").unwrap().is_publicly_routable());

        assert!(!IpAddress::from_str("198.51.100.0").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("198.51.100.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("198.51.99.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("198.51.101.0").unwrap().is_publicly_routable());

        assert!(!IpAddress::from_str("203.0.113.0").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("203.0.113.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("203.0.112.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("203.0.114.0").unwrap().is_publicly_routable());

        // RFC 6052
        assert!(!IpAddress::from_str("64:ff9b::").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("64:ff9b::ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("64:ff9a::ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("64:ff9b:1::").unwrap().is_publicly_routable());

        // RFC 6145
        assert!(!IpAddress::from_str("::ffff:0:0:0").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("::ffff:0:ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("::fffe:0:ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("::ffff:1:0:0").unwrap().is_publicly_routable());

        // RFC 6598
        assert!(!IpAddress::from_str("100.64.0.0").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("100.127.255.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("100.63.255.255").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("100.128.0.0").unwrap().is_publicly_routable());

        // Hurricane Electric IPv6 address block.
        assert!(!IpAddress::from_str("2001:470::").unwrap().is_publicly_routable());
        assert!(!IpAddress::from_str("2001:470:ffff:ffff:ffff:ffff:ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("2001:46f:ffff:ffff:ffff:ffff:ffff:ffff").unwrap().is_publicly_routable());
        assert!(IpAddress::from_str("2001:471::").unwrap().is_publicly_routable());

        // Broadcast ip
        assert!(!IpAddress::from_str("255.255.255.255").unwrap().is_publicly_routable());
    }

    // ---------------------------------------------------------------
    // PeerEndpoint tests
    // ---------------------------------------------------------------

    fn assert_address(ep: &PeerEndpoint, ip: &str, port: Option<u16>) {
        match ep {
            PeerEndpoint::Address(a) => {
                assert_eq!(a, &ContextualNetAddress::new(IpAddress::from_str(ip).unwrap(), port), "address mismatch");
            }
            other => panic!("expected Address variant, got {other:?}"),
        }
    }

    fn assert_hostname(ep: &PeerEndpoint, host: &str, port: Option<u16>) {
        match ep {
            PeerEndpoint::Hostname { host: h, port: p } => {
                assert_eq!(h, host);
                assert_eq!(*p, port);
            }
            other => panic!("expected Hostname variant, got {other:?}"),
        }
    }

    #[test]
    fn peer_endpoint_parses_ipv4() {
        assert_address(&PeerEndpoint::parse("1.2.3.4").unwrap(), "1.2.3.4", None);
        assert_address(&PeerEndpoint::parse("1.2.3.4:16111").unwrap(), "1.2.3.4", Some(16111));
    }

    #[test]
    fn peer_endpoint_parses_ipv6_bracketed() {
        assert_address(&PeerEndpoint::parse("::1").unwrap(), "::1", None);
        assert_address(&PeerEndpoint::parse("[::1]:16111").unwrap(), "::1", Some(16111));
    }

    #[test]
    fn peer_endpoint_parses_hostname_no_port() {
        assert_hostname(&PeerEndpoint::parse("node.example.com").unwrap(), "node.example.com", None);
    }

    #[test]
    fn peer_endpoint_parses_hostname_with_port() {
        assert_hostname(&PeerEndpoint::parse("node.example.com:16111").unwrap(), "node.example.com", Some(16111));
    }

    #[test]
    fn peer_endpoint_parses_hostname_subdomain() {
        assert_hostname(&PeerEndpoint::parse("pod-1.svc.cluster.local").unwrap(), "pod-1.svc.cluster.local", None);
    }

    #[test]
    fn peer_endpoint_parses_hostname_lowercases_input() {
        // RFC 1123 hostnames are case-insensitive; the parser canonicalises to
        // ASCII-lowercase so case-different inputs map to the same registry entry.
        assert_hostname(&PeerEndpoint::parse("Foo.Example.COM").unwrap(), "foo.example.com", None);
        assert_hostname(&PeerEndpoint::parse("NODE.example.com:16111").unwrap(), "node.example.com", Some(16111));
        assert_eq!(
            PeerEndpoint::parse("Foo.Example.com").unwrap(),
            PeerEndpoint::parse("foo.example.COM").unwrap(),
            "case-different inputs must compare equal after parse",
        );
    }

    #[test]
    fn peer_endpoint_parses_trailing_dot_canonical() {
        // RFC 1034 Sec.3.1 admits a single trailing `.` as the rooted/absolute
        // FQDN form; semantically equivalent to the relative form for peer
        // dialing. Canonical store form strips the trailing dot so the same
        // conceptual host produces a single registry entry regardless of
        // which textual form the operator typed.
        assert_hostname(&PeerEndpoint::parse("example.com.").unwrap(), "example.com", None);
        assert_hostname(&PeerEndpoint::parse("node.example.com.:16111").unwrap(), "node.example.com", Some(16111));
        assert_eq!(
            PeerEndpoint::parse("example.com.").unwrap(),
            PeerEndpoint::parse("example.com").unwrap(),
            "rooted-FQDN and relative-form inputs must compare equal after parse",
        );
        assert_eq!(
            PeerEndpoint::parse("Example.COM.").unwrap(),
            PeerEndpoint::parse("example.com").unwrap(),
            "case-different rooted-FQDN inputs must also compare equal after parse",
        );
        // Display round-trip emits the canonical (no-trailing-dot) form.
        assert_eq!(PeerEndpoint::parse("example.com.").unwrap().to_string(), "example.com");
    }

    #[test]
    fn peer_endpoint_rejects_underscore() {
        let err = PeerEndpoint::parse("host_with_underscore.example.com").unwrap_err();
        assert!(matches!(err, PeerEndpointParseError::InvalidHostname { .. }), "got {err:?}");
    }

    #[test]
    fn peer_endpoint_rejects_double_dot() {
        let err = PeerEndpoint::parse("host..example.com").unwrap_err();
        assert!(matches!(err, PeerEndpointParseError::InvalidHostname { .. }), "got {err:?}");
    }

    #[test]
    fn peer_endpoint_rejects_label_too_long() {
        let label = "a".repeat(64);
        let input = format!("{label}.example.com");
        let err = PeerEndpoint::parse(&input).unwrap_err();
        assert!(matches!(err, PeerEndpointParseError::InvalidHostname { .. }), "got {err:?}");
    }

    #[test]
    fn peer_endpoint_rejects_total_too_long() {
        // 63 + 1 + 63 + 1 + 63 + 1 + 63 = 255 chars total, all-LDH labels.
        let label = "a".repeat(63);
        let input = format!("{label}.{label}.{label}.{label}");
        assert_eq!(input.len(), 255);
        assert!(input.len() > 253);
        let err = PeerEndpoint::parse(&input).unwrap_err();
        assert!(matches!(err, PeerEndpointParseError::InvalidHostname { .. }), "got {err:?}");
    }

    #[test]
    fn peer_endpoint_rejects_all_numeric_rightmost_label() {
        // RFC 1123 Sec.2.1: rightmost label MUST contain at least one ASCII
        // letter. Numeric-only TLD-position labels are rejected even when
        // the IP-literal parser would not accept them (e.g. 3-octet `1.2.3`,
        // single-label `123`). Numeric labels in non-rightmost positions
        // remain accepted.
        for input in ["123", "node.42", "1.2.3", "host.456"] {
            let err = PeerEndpoint::parse(input).unwrap_err();
            assert!(
                matches!(err, PeerEndpointParseError::InvalidHostname { .. }),
                "expected InvalidHostname for {input:?}, got {err:?}",
            );
        }
        // Counter-examples: numeric label in non-rightmost position is fine
        // because the rightmost label is alphabetic.
        for input in ["host.123.example", "42.com", "9-leading-digit.example.com"] {
            PeerEndpoint::parse(input).unwrap_or_else(|e| panic!("expected `{input}` to parse, got {e:?}"));
        }
    }

    #[test]
    fn peer_endpoint_rejects_garbage() {
        for input in
            ["", "   ", "not a host", ":::", ":80", "host:", "host:abc", "-leadinghyphen.example.com", "trailinghyphen-.example.com"]
        {
            assert!(PeerEndpoint::parse(input).is_err(), "expected error parsing {input:?}");
        }
    }

    #[test]
    fn peer_endpoint_borsh_roundtrip_address() {
        let ep = PeerEndpoint::parse("1.2.3.4:16111").unwrap();
        let bytes = borsh::to_vec(&ep).unwrap();
        let back: PeerEndpoint = BorshDeserialize::try_from_slice(&bytes).unwrap();
        assert_eq!(ep, back);
    }

    #[test]
    fn peer_endpoint_borsh_roundtrip_hostname() {
        let ep = PeerEndpoint::parse("node.example.com:16111").unwrap();
        let bytes = borsh::to_vec(&ep).unwrap();
        let back: PeerEndpoint = BorshDeserialize::try_from_slice(&bytes).unwrap();
        assert_eq!(ep, back);
        let ep_no_port = PeerEndpoint::parse("node.example.com").unwrap();
        let bytes = borsh::to_vec(&ep_no_port).unwrap();
        let back: PeerEndpoint = BorshDeserialize::try_from_slice(&bytes).unwrap();
        assert_eq!(ep_no_port, back);
    }

    #[test]
    fn peer_endpoint_display_canonical() {
        // For every supported textual form, Display::fmt round-trips through from_str.
        for input in
            ["1.2.3.4", "1.2.3.4:16111", "[::1]:16111", "node.example.com", "node.example.com:16111", "pod-1.svc.cluster.local"]
        {
            let parsed = PeerEndpoint::parse(input).unwrap();
            let displayed = parsed.to_string();
            let reparsed = PeerEndpoint::parse(&displayed).unwrap();
            assert_eq!(parsed, reparsed, "Display round-trip failed for {input:?} -> {displayed:?}");
        }
    }

    #[tokio::test]
    async fn peer_endpoint_resolve_address_variant() {
        let ep = PeerEndpoint::parse("1.2.3.4:16111").unwrap();
        let addrs = ep.resolve(0).await.unwrap();
        assert_eq!(addrs, vec![NetAddress::new(IpAddress::from_str("1.2.3.4").unwrap(), 16111)]);
        // Address variant with no port falls back to the default_port argument.
        let ep_no_port = PeerEndpoint::parse("1.2.3.4").unwrap();
        let addrs = ep_no_port.resolve(16211).await.unwrap();
        assert_eq!(addrs, vec![NetAddress::new(IpAddress::from_str("1.2.3.4").unwrap(), 16211)]);
    }

    /// End-to-end OS-resolver smoke test: `localhost` must resolve via
    /// the host system's resolver to the loopback address. This is the
    /// only test that depends on the production
    /// [`tokio::net::lookup_host`] path -- the resolve-Address-variant,
    /// timeout-wrapper, and io-error-propagation tests use synthetic
    /// futures or the `Address` short-circuit. The OS-resolver dependency
    /// is intentional: it is the smallest end-to-end probe that asserts
    /// the resolver-feature-gate (`tokio = { ..., features = ["net"] }`)
    /// is wired correctly without paying the wall-clock cost of a real
    /// DNS round-trip. CI environments are required to provide working
    /// `localhost` resolution -- which every nss-files-equipped libc
    /// (glibc, musl with `nsswitch.conf`, BSD libc) does out of the box;
    /// minimal containers without `/etc/hosts` are not a supported test
    /// target. The synthetic-future variants alongside this test cover
    /// the wrapper plumbing under hermetic conditions.
    #[tokio::test]
    async fn peer_endpoint_resolve_localhost() {
        let ep = PeerEndpoint::parse("localhost").unwrap();
        let addrs = ep.resolve(0).await.expect("localhost must resolve via OS resolver");
        assert!(!addrs.is_empty(), "expected at least one resolved address for localhost");
        assert!(
            addrs.iter().any(|a| a.ip == IpAddress::from_str("127.0.0.1").unwrap() || a.ip == IpAddress::from_str("::1").unwrap()),
            "expected 127.0.0.1 or ::1 in {addrs:?}",
        );
    }

    /// Drive the timeout path with a synthetic lookup future that pends
    /// past [`PEER_ENDPOINT_RESOLVE_TIMEOUT`] under paused tokio time.
    /// The wrapper must yield [`PeerEndpointResolveError::Timeout`] in
    /// virtual time without blocking the runtime on real wall-clock
    /// duration.
    #[tokio::test(start_paused = true)]
    async fn peer_endpoint_resolve_timeout_wrapper_fires() {
        let host = "fake.kas947.example";
        let lookup = async {
            // Sleep one full second past the wrapper's deadline so the
            // timeout arm wins the race deterministically. Under
            // `start_paused`, tokio auto-advances virtual time when the
            // runtime is otherwise idle.
            tokio::time::sleep(PEER_ENDPOINT_RESOLVE_TIMEOUT + Duration::from_secs(1)).await;
            Ok(Vec::new())
        };
        let start = tokio::time::Instant::now();
        let result = resolve_with_timeout(host, lookup).await;
        let elapsed = start.elapsed();
        match result {
            Err(PeerEndpointResolveError::Timeout { host: h, timeout }) => {
                assert_eq!(h, host);
                assert_eq!(timeout, PEER_ENDPOINT_RESOLVE_TIMEOUT);
            }
            other => panic!("expected Timeout error, got {other:?}"),
        }
        assert!(
            elapsed >= PEER_ENDPOINT_RESOLVE_TIMEOUT,
            "wrapper must wait at least the full timeout in virtual time before firing; elapsed = {elapsed:?}",
        );
        assert!(
            elapsed < PEER_ENDPOINT_RESOLVE_TIMEOUT + Duration::from_secs(1),
            "wrapper must fire at the timeout, not after the inner sleep finishes; elapsed = {elapsed:?}",
        );
    }

    /// Counterpart to the timeout test: a lookup that resolves before
    /// the deadline propagates its `Vec<SocketAddr>` payload through the
    /// wrapper without spurious timeout errors.
    #[tokio::test(start_paused = true)]
    async fn peer_endpoint_resolve_timeout_wrapper_returns_payload() {
        let host = "fake.kas947.example";
        let payload: Vec<SocketAddr> = vec!["127.0.0.1:42101".parse().unwrap()];
        let payload_clone = payload.clone();
        let lookup = async move {
            // Brief virtual-time delay well under the timeout.
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(payload_clone)
        };
        let result = resolve_with_timeout(host, lookup).await.expect("wrapper must propagate the inner Ok payload");
        assert_eq!(result, payload);
    }

    /// An inner `io::Error` propagates as [`PeerEndpointResolveError::Lookup`]
    /// with the host and source preserved -- the wrapper does NOT swallow
    /// it as a spurious timeout.
    #[tokio::test(start_paused = true)]
    async fn peer_endpoint_resolve_timeout_wrapper_propagates_io_error() {
        let host = "fake.kas947.example";
        let lookup = async {
            Err::<Vec<SocketAddr>, _>(std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "synthetic lookup failure"))
        };
        match resolve_with_timeout(host, lookup).await {
            Err(PeerEndpointResolveError::Lookup { host: h, source }) => {
                assert_eq!(h, host);
                assert_eq!(source.kind(), std::io::ErrorKind::ConnectionRefused);
            }
            other => panic!("expected Lookup error, got {other:?}"),
        }
    }

    #[test]
    fn hostname_validator_accepts_rfc1123_examples() {
        // Battery of RFC 1123 Sec.2.1 / RFC 952 examples that MUST pass the strict validator.
        for host in [
            "a",
            "example",
            "example.com",
            "node1.example.com",
            "pod-1.svc.cluster.local",
            "kaspa-mainnet-01.eu-west-1.compute.internal",
            "h.h.h.h.h.h.h.h.h.h",
            "9-leading-digit.example.com",
            "all-numeric-label.123.example",
            // Maximum valid label (63 chars).
            &format!("{}.example.com", "a".repeat(63)),
            // RFC 1034 Sec.3.1 rooted/absolute FQDN form (single trailing dot).
            "example.com.",
            "node1.example.com.",
        ] {
            validate_hostname_rfc1123(host).unwrap_or_else(|e| panic!("expected `{host}` to validate, got {e:?}"));
        }
    }

    #[test]
    fn hostname_validator_rejects_only_dot_and_double_trailing_dots() {
        // The rooted-form strip removes a single trailing `.`, not more.
        // `"."` strips to empty and is rejected as empty hostname.
        // `"example.com.."` strips to `"example.com."` which still has
        // an empty trailing label and is rejected with the consecutive-
        // dots reason.
        for (input, expected_reason) in
            [(".", "empty hostname"), ("example.com..", "empty label (consecutive '.' or leading/trailing '.')")]
        {
            match validate_hostname_rfc1123(input) {
                Err(PeerEndpointParseError::InvalidHostname { reason }) => {
                    assert_eq!(reason, expected_reason, "input `{input}` produced wrong reason");
                }
                other => panic!("expected `{input}` to be rejected as InvalidHostname, got {other:?}"),
            }
        }
    }
}
