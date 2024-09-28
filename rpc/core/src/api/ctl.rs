//!
//! Client-side RPC helper for handling connection and disconnection events.
//!

use crate::error::RpcResult;
use std::sync::{Arc, Mutex};
use workflow_core::channel::Multiplexer;

/// RPC channel control operations
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcState {
    /// RpcApi channel open (connected)
    Connected,
    /// RpcApi channel close (disconnected)
    #[default]
    Disconnected,
}

#[derive(Default)]
struct Inner {
    // Current channel state
    state: Mutex<RpcState>,
    // MPMC channel for [`RpcCtlOp`] operations.
    multiplexer: Multiplexer<RpcState>,
    // Optional Connection descriptor such as a connection URL.
    descriptor: Mutex<Option<String>>,
}

/// RPC channel control helper. This is a companion
/// struct to [`RpcApi`](crate::api::rpc::RpcApi) that
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

    pub fn with_descriptor<Str: ToString>(descriptor: Option<Str>) -> Self {
        if let Some(descriptor) = descriptor {
            Self { inner: Arc::new(Inner { descriptor: Mutex::new(Some(descriptor.to_string())), ..Inner::default() }) }
        } else {
            Self::default()
        }
    }

    /// Obtain internal multiplexer (MPMC channel for [`RpcState`] operations)
    pub fn multiplexer(&self) -> &Multiplexer<RpcState> {
        &self.inner.multiplexer
    }

    pub fn is_connected(&self) -> bool {
        *self.inner.state.lock().unwrap() == RpcState::Connected
    }

    pub fn state(&self) -> RpcState {
        *self.inner.state.lock().unwrap()
    }

    /// Signal open to all listeners (async)
    pub async fn signal_open(&self) -> RpcResult<()> {
        *self.inner.state.lock().unwrap() = RpcState::Connected;
        Ok(self.inner.multiplexer.broadcast(RpcState::Connected).await?)
    }

    /// Signal close to all listeners (async)
    pub async fn signal_close(&self) -> RpcResult<()> {
        *self.inner.state.lock().unwrap() = RpcState::Disconnected;
        Ok(self.inner.multiplexer.broadcast(RpcState::Disconnected).await?)
    }

    /// Try signal open to all listeners (sync)
    pub fn try_signal_open(&self) -> RpcResult<()> {
        *self.inner.state.lock().unwrap() = RpcState::Connected;
        Ok(self.inner.multiplexer.try_broadcast(RpcState::Connected)?)
    }

    /// Try signal close to all listeners (sync)
    pub fn try_signal_close(&self) -> RpcResult<()> {
        *self.inner.state.lock().unwrap() = RpcState::Disconnected;
        Ok(self.inner.multiplexer.try_broadcast(RpcState::Disconnected)?)
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
