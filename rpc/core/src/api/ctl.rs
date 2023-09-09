use crate::error::RpcResult;
use std::sync::{Arc, Mutex};
use workflow_core::channel::Multiplexer;

/// RPC channel control operations
#[derive(Debug, Clone)]
pub enum RpcCtlOp {
    /// RpcApi channel open (connected)
    Open,
    /// RpcApi channel close (disconnected)
    Close,
}

#[derive(Default)]
struct Inner {
    // MPMC channel for [`RpcCtlOp`] operations.
    multiplexer: Multiplexer<RpcCtlOp>,
    // Optional Connection descriptor such as a connection URL.
    descriptor: Mutex<Option<String>>,
}

/// RPC channel control helper. This is a companion
/// struct to [`RpcApi`](crate::api::RpcApi) that
/// provides signaling for RPC open/close events as
/// well as an optional connection descriptor (URL).
#[derive(Default, Clone)]
pub struct RpcCtl {
    inner: Arc<Inner>,
}

impl RpcCtl {
    pub fn new() -> Self {
        Self { inner: Arc::new(Inner::default()) }
    }

    pub fn with_descriptor<Str: ToString>(descriptor: Str) -> Self {
        Self { inner: Arc::new(Inner { descriptor: Mutex::new(Some(descriptor.to_string())), ..Inner::default() }) }
    }

    /// Obtain internal multiplexer (MPMC channel for [`RpcCtlOp`] operations)
    pub fn multiplexer(&self) -> &Multiplexer<RpcCtlOp> {
        &self.inner.multiplexer
    }

    /// Signal open to all listeners (async)
    pub async fn signal_open(&self) -> RpcResult<()> {
        Ok(self.inner.multiplexer.broadcast(RpcCtlOp::Open).await?)
    }

    /// Signal close to all listeners (async)
    pub async fn signal_close(&self) -> RpcResult<()> {
        Ok(self.inner.multiplexer.broadcast(RpcCtlOp::Close).await?)
    }

    /// Try signal open to all listeners (sync)
    pub fn try_signal_open(&self) -> RpcResult<()> {
        Ok(self.inner.multiplexer.try_broadcast(RpcCtlOp::Open)?)
    }

    /// Try signal close to all listeners (sync)
    pub fn try_signal_close(&self) -> RpcResult<()> {
        Ok(self.inner.multiplexer.try_broadcast(RpcCtlOp::Close)?)
    }

    /// Set the connection descriptor (URL, peer address, etc.)
    pub fn set_descriptor(&self, descriptor: Option<String>) {
        *self.inner.descriptor.lock().unwrap() = descriptor;
    }

    /// Get the connection descriptor (URL, peer address, etc.)
    pub fn descriptor(&self) -> Option<String> {
        self.inner.descriptor.lock().unwrap().clone()
    }
}
