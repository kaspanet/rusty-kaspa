use std::sync::atomic::AtomicU64;

#[derive(Debug, Default)]
pub struct ServerCounters {
    pub live_connections : AtomicU64,
    pub connection_attempts : AtomicU64,
    pub handshake_failures : AtomicU64,
}
