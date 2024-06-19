use std::sync::Arc;
use enum_dispatch::enum_dispatch;

#[enum_dispatch]
pub trait RpcConnection: Send + Sync {
    fn id(&self) -> u64;
}

// pub type DynRpcConnection = Arc<dyn RpcConnection>;
