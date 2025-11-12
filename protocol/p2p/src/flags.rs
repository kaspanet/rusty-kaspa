/// Service flags advertised in the P2P version handshake.
///
/// The values mirror the feature bits used in Bitcoin Core where possible in
/// order to ease interoperability and future parity work.
pub mod service {
    /// BIP155 (ADDRv2) support flag.
    ///
    /// Bitcoin Core uses bit 23 (`1 << 23`) to signal support for the ADDRv2
    /// message format and associated address gossip semantics. We keep the same
    /// value so downstream tooling can reason about Kaspa nodes in the same way.
    pub const ADDR_V2: u64 = 1 << 23;
}
