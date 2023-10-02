use std::sync::Arc;

pub use kaspa_rpc_core::api::ctl::RpcCtl;
pub type DynRpcApi = dyn kaspa_rpc_core::api::rpc::RpcApi;
pub type NotificationChannel = kaspa_utils::channel::Channel<kaspa_rpc_core::Notification>;
pub use kaspa_rpc_core::notify::mode::NotificationMode;
pub use kaspa_wrpc_client::WrpcEncoding;

/// RPC adaptor class that holds the [`RpcApi`](crate::api::RpcApi)
/// and [`RpcCtl`](crate::api::RpcCtl) instances.
#[derive(Clone)]
pub struct Rpc {
    pub rpc_api: Arc<DynRpcApi>,
    pub rpc_ctl: RpcCtl,
}

impl Rpc {
    pub fn new(rpc_api: Arc<DynRpcApi>, rpc_ctl: RpcCtl) -> Self {
        Rpc { rpc_api, rpc_ctl }
    }

    pub fn rpc_api(&self) -> &Arc<DynRpcApi> {
        &self.rpc_api
    }

    pub fn rpc_ctl(&self) -> &RpcCtl {
        &self.rpc_ctl
    }
}
