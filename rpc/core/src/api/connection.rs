//!
//! Generic connection trait representing a connection to a client (where available).
//!

use std::sync::Arc;

pub trait RpcConnection: Send + Sync {
    fn id(&self) -> u64;
    fn is_authenticated(&self) -> bool {
        false
    }
}

pub type DynRpcConnection = Arc<dyn RpcConnection>;
