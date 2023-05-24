//! Consensus and Session management structures.
//!
//! We use newtypes in order to simplify changing the underlying lock to
//! a more performant one in the future

use std::{ops::Deref, sync::Arc};
use tokio::sync::RwLockWriteGuard as TokioRwLockWriteGuard;

use crate::readers_lock::{ReadersFirstRwLock, ReadersFirstRwLockReadGuard};
use kaspa_consensus_core::api::{ConsensusApi, DynConsensus};

pub struct SessionReadGuard(ReadersFirstRwLockReadGuard<()>);

pub struct SessionWriteGuard<'a>(TokioRwLockWriteGuard<'a, ()>);

#[derive(Clone)]
pub struct SessionLock(Arc<ReadersFirstRwLock<()>>);

impl Default for SessionLock {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionLock {
    pub fn new() -> SessionLock {
        SessionLock(Arc::new(ReadersFirstRwLock::new(())))
    }

    pub async fn read(&self) -> SessionReadGuard {
        SessionReadGuard(self.0.read().await)
    }

    pub fn blocking_read(&self) -> SessionReadGuard {
        SessionReadGuard(self.0.blocking_read())
    }

    pub fn blocking_write(&self) -> SessionWriteGuard<'_> {
        SessionWriteGuard(self.0.blocking_write())
    }
}

#[derive(Clone)]
pub struct ConsensusInstance {
    session_lock: SessionLock,
    consensus: DynConsensus,
}

impl ConsensusInstance {
    pub fn new(session_lock: SessionLock, consensus: DynConsensus) -> Self {
        Self { session_lock, consensus }
    }

    pub async fn session(&self) -> ConsensusSession {
        let g = self.session_lock.clone().read().await;
        ConsensusSession::new(g, self.consensus.clone())
    }
}

pub struct ConsensusSession {
    _session_guard: SessionReadGuard,
    consensus: DynConsensus,
}

impl ConsensusSession {
    pub fn new(session_guard: SessionReadGuard, consensus: DynConsensus) -> Self {
        Self { _session_guard: session_guard, consensus }
    }
}

impl Deref for ConsensusSession {
    type Target = dyn ConsensusApi; // We avoid exposing the Arc itself by ref since it can be easily cloned and misused

    fn deref(&self) -> &Self::Target {
        self.consensus.as_ref()
    }
}
