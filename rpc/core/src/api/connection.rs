use std::sync::Arc;

pub trait RpcConnection: Send + Sync {
    fn id(&self) -> u64;
}

pub type DynRpcConnection = Arc<dyn RpcConnection>;
