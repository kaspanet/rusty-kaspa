pub mod codec;
pub mod error;
/// Fast Trusted Relay (FTR) for low-latency block propagation.
///
/// This crate provides the transport-layer block reassembly:
/// - UdpTransport: P2P-level UDP transport for FEC fragments
/// - TrustedRelayFlow: Fragment pooling and reassembly (FEC decoding)
/// - FragmentPool: In-flight reassembly state management
/// - FecConfig/FecEncoder/FecDecoder: Reed-Solomon FEC codec
/// - TokenAuthenticator/AuthToken: HMAC authentication
///
/// High-level consensus validation happens in separate flow layer
pub mod fast_trusted_relay;
pub mod model;
pub mod servers;

// Centralized runtime params for trusted relay.
pub mod params;

pub use fast_trusted_relay::FastTrustedRelay;
