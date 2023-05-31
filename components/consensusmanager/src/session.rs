//! Consensus and Session management structures.
//!
//! We use newtypes in order to simplify changing the underlying lock in the future

use kaspa_consensus_core::api::{ConsensusApi, DynConsensus};
use kaspa_utils::sync::rwlock::*;
use std::{ops::Deref, sync::Arc};

pub struct SessionOwnedReadGuard(RfRwLockOwnedReadGuard);

pub struct SessionReadGuard<'a>(RfRwLockReadGuard<'a>);

pub struct SessionWriteGuard<'a>(RfRwLockWriteGuard<'a>);

impl SessionWriteGuard<'_> {
    /// Releases and recaptures the write lock. Makes sure that other pending readers/writers get a
    /// chance to capture the lock before this thread does so.
    pub fn blocking_yield(&mut self) {
        self.0.blocking_yield();
    }
}

#[derive(Clone)]
pub struct SessionLock(Arc<RfRwLock>);

impl Default for SessionLock {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionLock {
    pub fn new() -> SessionLock {
        SessionLock(Arc::new(RfRwLock::new()))
    }

    pub async fn read_owned(&self) -> SessionOwnedReadGuard {
        SessionOwnedReadGuard(self.0.clone().read_owned().await)
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
        let g = self.session_lock.read().await;
        ConsensusSession::new(g, self.consensus.clone())
    }

    pub async fn session_owned(&self) -> ConsensusSessionOwned {
        let g = self.session_lock.read_owned().await;
        ConsensusSessionOwned::new(g, self.consensus.clone())
    }
}

pub struct ConsensusSession<'a> {
    _session_guard: SessionReadGuard<'a>,
    consensus: DynConsensus,
}

impl<'a> ConsensusSession<'a> {
    pub fn new(session_guard: SessionReadGuard<'a>, consensus: DynConsensus) -> Self {
        Self { _session_guard: session_guard, consensus }
    }
}

impl Deref for ConsensusSession<'_> {
    type Target = dyn ConsensusApi; // We avoid exposing the Arc itself by ref since it can be easily cloned and misused

    fn deref(&self) -> &Self::Target {
        self.consensus.as_ref()
    }
}

pub struct ConsensusSessionOwned {
    _session_guard: SessionOwnedReadGuard,
    consensus: DynConsensus,
}

impl ConsensusSessionOwned {
    pub fn new(session_guard: SessionOwnedReadGuard, consensus: DynConsensus) -> Self {
        Self { _session_guard: session_guard, consensus }
    }
}

impl Deref for ConsensusSessionOwned {
    type Target = dyn ConsensusApi; // We avoid exposing the Arc itself by ref since it can be easily cloned and misused

    fn deref(&self) -> &Self::Target {
        self.consensus.as_ref()
    }
}
