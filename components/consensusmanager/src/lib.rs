use itertools::Itertools;
use kaspa_consensus_core::api::{ConsensusApi, DynConsensus};
use kaspa_core::{core::Core, debug, service::Service};
use parking_lot::RwLock;
use std::{collections::VecDeque, ops::Deref, sync::Arc, thread::JoinHandle};

mod session;

pub use session::{
    spawn_blocking, ConsensusInstance, ConsensusProxy, ConsensusSessionBlocking, SessionLock, SessionReadGuard, SessionWriteGuard,
};

/// Consensus controller trait. Includes methods required to start/stop/control consensus, but which should not
/// be exposed to ordinary users
pub trait ConsensusCtl: Sync + Send {
    /// Initialize and start workers etc    
    fn start(&self) -> Vec<JoinHandle<()>>;

    /// Shutdown all workers and clear runtime resources
    fn stop(&self);

    /// Set as current active consensus
    fn make_active(&self);
}

pub type DynConsensusCtl = Arc<dyn ConsensusCtl>;

pub trait ConsensusFactory: Sync + Send {
    /// Load an instance of current active consensus or create one if no such exists
    fn new_active_consensus(&self) -> (ConsensusInstance, DynConsensusCtl);

    /// Create a new empty staging consensus
    fn new_staging_consensus(&self) -> (ConsensusInstance, DynConsensusCtl);

    /// Close the factory and cleanup any shared resources used by it
    fn close(&self);

    /// If the node is not configured as archival -- delete inactive consensus entries and their databases  
    fn delete_inactive_consensus_entries(&self);

    /// Delete the staging consensus entry and its database (this is done even if the node is archival
    /// since staging reflects non-final data)
    fn delete_staging_entry(&self);
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

    fn close(&self) {
        unimplemented!()
    }

    fn delete_inactive_consensus_entries(&self) {
        unimplemented!()
    }

    fn delete_staging_entry(&self) {
        unimplemented!()
    }
}

/// Defines a trait which handles consensus resets for external parts of the system. We avoid using
/// the generic notification system since the reset needs to be handled synchronously in order to
/// retain state consistency
pub trait ConsensusResetHandler: Send + Sync {
    fn handle_consensus_reset(&self);
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

    /// Handlers called when the consensus is reset to a staging consensus
    consensus_reset_handlers: Vec<Arc<dyn ConsensusResetHandler>>,
}

impl ManagerInner {
    fn new(consensus: ConsensusInstance, ctl: DynConsensusCtl) -> Self {
        Self {
            current: ConsensusInner::new(consensus, ctl),
            handles: Default::default(),
            consensus_reset_handlers: Default::default(),
        }
    }
}

pub struct ConsensusManager {
    factory: Arc<dyn ConsensusFactory>,
    inner: RwLock<ManagerInner>,
}

impl ConsensusManager {
    pub const IDENT: &'static str = "consensus manager";

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
            inner: RwLock::new(ManagerInner::new(ConsensusInstance::new(SessionLock::new(), consensus), ctl)),
        }
    }

    pub fn consensus(&self) -> ConsensusInstance {
        self.inner.read().current.consensus.clone()
    }

    pub fn new_staging_consensus(self: &Arc<Self>) -> StagingConsensus {
        let (consensus, ctl) = self.factory.new_staging_consensus();
        StagingConsensus::new(self.clone(), ConsensusInner::new(consensus, ctl))
    }

    pub fn register_consensus_reset_handler(&self, handler: Arc<dyn ConsensusResetHandler>) {
        self.inner.write().consensus_reset_handlers.push(handler);
    }

    fn worker(&self) {
        let handles = self.inner.read().current.ctl.clone().start();
        self.inner.write().handles.extend(handles);
        // If current consensus is switched, this loop will join the replaced handles, and will switch to waiting for the new ones
        let mut g = self.inner.write();
        while let Some(handle) = g.handles.pop_front() {
            drop(g);
            handle.join().unwrap();
            g = self.inner.write();
        }

        // All consensus instances have been shutdown and we are exiting, so close the factory. Internally this closes
        // the notification root sender channel, leading to a graceful shutdown of the notification sub-system.
        debug!("[Consensus manager] all consensus threads exited");
        self.factory.close();
    }

    pub fn delete_inactive_consensus_entries(&self) {
        self.factory.delete_inactive_consensus_entries();
    }

    pub fn delete_staging_entry(&self) {
        self.factory.delete_staging_entry();
    }
}

impl Service for ConsensusManager {
    fn ident(self: Arc<Self>) -> &'static str {
        Self::IDENT
    }

    fn start(self: Arc<Self>, _core: Arc<Core>) -> Vec<JoinHandle<()>> {
        vec![std::thread::spawn(move || self.worker())]
    }

    fn stop(self: Arc<Self>) {
        self.inner.read().current.ctl.clone().stop();
    }
}

pub struct StagingConsensus {
    manager: Arc<ConsensusManager>,
    staging: ConsensusInner,
    handles: VecDeque<JoinHandle<()>>,
}

impl StagingConsensus {
    fn new(manager: Arc<ConsensusManager>, staging: ConsensusInner) -> Self {
        let handles = VecDeque::from_iter(staging.ctl.start());
        Self { manager, staging, handles }
    }

    pub fn commit(self) {
        let mut g = self.manager.inner.write();
        let prev = std::mem::replace(&mut g.current, self.staging);
        g.handles.extend(self.handles);
        prev.ctl.stop();
        g.current.ctl.make_active();
        drop(g);
        let handlers = self.manager.inner.read().consensus_reset_handlers.iter().cloned().collect_vec();
        for handler in handlers {
            handler.handle_consensus_reset();
        }
        // Drop `prev` so that deletion below succeeds
        drop(prev);
        // Staging was committed and is now the active consensus so we can delete
        // any pervious, now inactive, consensus entries
        self.manager.delete_inactive_consensus_entries();
    }

    pub fn cancel(self) {
        self.staging.ctl.stop();
        for handle in self.handles {
            handle.join().unwrap();
        }
        // Drop staging (and DB refs therein) so that the delete operation below succeeds
        drop(self.staging);
        // Delete the canceled staging consensus
        self.manager.delete_staging_entry();
    }
}

impl Deref for StagingConsensus {
    type Target = ConsensusInstance;

    fn deref(&self) -> &Self::Target {
        &self.staging.consensus
    }
}
