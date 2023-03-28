use kaspa_consensus_core::api::{ConsensusApi, DynConsensus};
use kaspa_core::{core::Core, service::Service};
use parking_lot::RwLock;
use std::{collections::VecDeque, ops::Deref, sync::Arc, thread::JoinHandle};
use tokio::sync::{RwLock as TokioRwLock, RwLockReadGuard as TokioRwLockReadGuard};

/// Consensus controller trait. Includes methods required to start/stop/control consensus, but which should not
/// be exposed to ordinary users
pub trait ConsensusCtl: Sync + Send {
    /// Initialize and start workers etc    
    fn start(&self) -> Vec<JoinHandle<()>>;

    /// Shutdown all workers and clear runtime resources
    fn stop(&self);

    /// Set as current active consensus
    fn make_active(&self);

    /// Delete this consensus instance from memory and disk permanently
    fn delete(&self);
}

pub type DynConsensusCtl = Arc<dyn ConsensusCtl>;

pub trait ConsensusFactory: Sync + Send {
    /// Load an instance of current active consensus or create one if no such exists
    fn new_active_consensus(&self) -> (ConsensusInstance, DynConsensusCtl);

    /// Create a new empty staging consensus
    fn new_staging_consensus(&self) -> (ConsensusInstance, DynConsensusCtl);
}

/// Test-only mock factory
struct MockFactory;

impl ConsensusFactory for MockFactory {
    fn new_active_consensus(&self) -> (ConsensusInstance, DynConsensusCtl) {
        unimplemented!()
    }

    fn new_staging_consensus(&self) -> (ConsensusInstance, DynConsensusCtl) {
        unimplemented!()
    }
}

/// Wraps all needed structures required for interacting and controlling a consensus instance
struct ConsensusInner {
    consensus: ConsensusInstance,
    ctl: DynConsensusCtl,
}

impl ConsensusInner {
    fn new(consensus: ConsensusInstance, ctl: DynConsensusCtl) -> Self {
        Self { consensus, ctl }
    }
}

struct ManagerInner {
    /// Current consensus
    current: ConsensusInner,

    /// Service join handles
    handles: VecDeque<JoinHandle<()>>,
}

impl ManagerInner {
    fn new(consensus: ConsensusInstance, ctl: DynConsensusCtl) -> Self {
        Self { current: ConsensusInner::new(consensus, ctl), handles: Default::default() }
    }
}

pub struct ConsensusManager {
    factory: Arc<dyn ConsensusFactory>,
    inner: RwLock<ManagerInner>,
}

impl ConsensusManager {
    pub fn new(factory: Arc<dyn ConsensusFactory>) -> Self {
        let (consensus, ctl) = factory.new_active_consensus();
        Self { factory, inner: RwLock::new(ManagerInner::new(consensus, ctl)) }
    }

    /// Creates a consensus manager with a fixed consensus. Will panic if staging API is used. To be
    /// used for test purposes only.
    pub fn from_consensus<T: ConsensusApi + ConsensusCtl + 'static>(consensus: Arc<T>) -> Self {
        let (consensus, ctl) = (consensus.clone() as DynConsensus, consensus as DynConsensusCtl);
        Self {
            factory: Arc::new(MockFactory),
            inner: RwLock::new(ManagerInner::new(ConsensusInstance::new(Arc::new(TokioRwLock::new(())), consensus), ctl)),
        }
    }

    pub fn consensus(&self) -> ConsensusInstance {
        self.inner.read().current.consensus.clone()
    }

    pub fn new_staging_consensus(&self) -> StagingConsensus<'_> {
        let (consensus, ctl) = self.factory.new_staging_consensus();
        StagingConsensus::new(self, ConsensusInner::new(consensus, ctl))
    }

    fn worker(&self) {
        let handles = self.inner.read().current.ctl.clone().start();
        self.inner.write().handles.extend(handles);
        // If current consensus is switched, this loop will join the replaced handles, and will switch to waiting for the new ones
        while let Some(handle) = self.inner.write().handles.pop_front() {
            handle.join().unwrap();
        }
    }
}

impl Service for ConsensusManager {
    fn ident(self: Arc<Self>) -> &'static str {
        "consensus manager"
    }

    fn start(self: Arc<Self>, _core: Arc<Core>) -> Vec<JoinHandle<()>> {
        vec![std::thread::spawn(move || self.worker())]
    }

    fn stop(self: Arc<Self>) {
        self.inner.read().current.ctl.clone().stop();
    }
}

pub struct StagingConsensus<'a> {
    manager: &'a ConsensusManager,
    staging: ConsensusInner,
    handles: VecDeque<JoinHandle<()>>,
}

impl<'a> StagingConsensus<'a> {
    fn new(manager: &'a ConsensusManager, staging: ConsensusInner) -> Self {
        let handles = VecDeque::from_iter(staging.ctl.start());
        Self { manager, staging, handles }
    }

    pub fn commit(self) {
        let mut g = self.manager.inner.write();
        let prev = std::mem::replace(&mut g.current, self.staging);
        g.handles.extend(self.handles);
        prev.ctl.stop();
        g.current.ctl.make_active();
    }

    pub fn cancel(self) {
        self.staging.ctl.stop();
        for handle in self.handles {
            handle.join().unwrap();
        }
        self.staging.ctl.delete();
    }
}

// impl Drop for StagingConsensus<'_> {
//     fn drop(&mut self) {
//         todo!()
//     }
// }

impl Deref for StagingConsensus<'_> {
    type Target = ConsensusInstance;

    fn deref(&self) -> &Self::Target {
        &self.staging.consensus
    }
}

#[derive(Clone)]
pub struct ConsensusInstance {
    session_lock: Arc<TokioRwLock<()>>,
    consensus: DynConsensus,
}

impl ConsensusInstance {
    pub fn new(session_lock: Arc<TokioRwLock<()>>, consensus: DynConsensus) -> Self {
        Self { session_lock, consensus }
    }

    pub async fn session(&self) -> ConsensusSession<'_> {
        let g = self.session_lock.read().await;
        ConsensusSession::new(g, self.consensus.clone())
    }
}

pub struct ConsensusSession<'a> {
    _session_guard: TokioRwLockReadGuard<'a, ()>,
    consensus: DynConsensus,
}

impl<'a> ConsensusSession<'a> {
    pub fn new(session_guard: TokioRwLockReadGuard<'a, ()>, consensus: DynConsensus) -> Self {
        Self { _session_guard: session_guard, consensus }
    }
}

impl Deref for ConsensusSession<'_> {
    type Target = dyn ConsensusApi; // We avoid exposing the Arc itself by ref since it can be easily cloned and misused

    fn deref(&self) -> &Self::Target {
        self.consensus.as_ref()
    }
}
