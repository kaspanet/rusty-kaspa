use kaspa_consensus_core::{
    api::{ConsensusApi, DynConsensus},
    config::Config,
};
use kaspa_core::{core::Core, service::Service};
use parking_lot::RwLock;
use std::{collections::VecDeque, ops::Deref, sync::Arc, thread::JoinHandle};
use tokio::sync::{RwLock as TokioRwLock, RwLockReadGuard as TokioRwLockReadGuard};

/// Consensus controller trait. Includes methods required to start/stop/control consensus, but which should not
/// be exposed to ordinary users
pub trait ConsensusCtl: Service {
    // TODO
    // fn set_notification_root(&self, root: Arc<ConsensusNotificationRoot>);
}

pub type DynConsensusCtl = Arc<dyn ConsensusCtl>;

pub trait ConsensusFactory: Sync + Send {
    fn new_consensus(&self, config: &Config) -> (DynConsensus, DynConsensusCtl);
}

struct MockFactory;

impl ConsensusFactory for MockFactory {
    fn new_consensus(&self, _config: &Config) -> (DynConsensus, DynConsensusCtl) {
        unimplemented!()
    }
}

struct Inner {
    /// Consensus instances
    current_consensus: ConsensusInstance,
    _staging_consensus: Option<ConsensusInstance>,

    /// Consensus service controllers
    current_ctl: DynConsensusCtl,
    _staging_ctl: Option<DynConsensusCtl>,

    /// Service join handles
    handles: VecDeque<JoinHandle<()>>,
}

impl Inner {
    fn new(consensus: DynConsensus, ctl: DynConsensusCtl) -> Self {
        Self {
            current_consensus: ConsensusInstance::new(consensus),
            _staging_consensus: None,
            current_ctl: ctl,
            _staging_ctl: None,
            handles: Default::default(),
        }
    }
}

pub struct ConsensusManager {
    _factory: Arc<dyn ConsensusFactory>,
    _config: Option<Config>,
    inner: RwLock<Inner>,
}

impl ConsensusManager {
    pub fn new(factory: Arc<dyn ConsensusFactory>, config: &Config) -> Self {
        let (consensus, ctl) = factory.new_consensus(config);
        Self { _factory: factory, _config: Some(config.clone()), inner: RwLock::new(Inner::new(consensus, ctl)) }
    }

    /// Creates a consensus manager with a fixed consensus. Will panic if staging API is used. To be
    /// used for test purposes only.
    pub fn from_consensus<T: ConsensusApi + ConsensusCtl>(consensus: Arc<T>) -> Self {
        let (consensus, ctl) = (consensus.clone() as DynConsensus, consensus as DynConsensusCtl);
        Self { _factory: Arc::new(MockFactory), _config: None, inner: RwLock::new(Inner::new(consensus, ctl)) }
    }

    pub fn consensus(&self) -> ConsensusInstance {
        self.inner.read().current_consensus.clone()
    }

    pub fn new_staging_consensus(&self) -> StagingConsensus<'_> {
        todo!()
    }

    fn worker(&self, core: Arc<Core>) {
        let handles = self.inner.read().current_ctl.clone().start(core);
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

    fn start(self: Arc<Self>, core: Arc<Core>) -> Vec<JoinHandle<()>> {
        vec![std::thread::spawn(move || self.worker(core))]
    }

    fn stop(self: Arc<Self>) {
        // TODO: staging
        self.inner.read().current_ctl.clone().stop();
    }
}

pub struct StagingConsensus<'a> {
    _manager: &'a ConsensusManager,
    staging: ConsensusInstance,
}

impl<'a> StagingConsensus<'a> {
    pub fn new(manager: &'a ConsensusManager, staging: ConsensusInstance) -> Self {
        Self { _manager: manager, staging }
    }

    pub fn commit(&self) {
        todo!()
    }

    pub fn cancel(self) {
        todo!()
    }
}

impl Deref for StagingConsensus<'_> {
    type Target = ConsensusInstance;

    fn deref(&self) -> &Self::Target {
        &self.staging
    }
}

#[derive(Clone)]
pub struct ConsensusInstance {
    session_lock: Arc<TokioRwLock<()>>,
    consensus: DynConsensus,
}

impl ConsensusInstance {
    pub fn new(consensus: DynConsensus) -> Self {
        Self { session_lock: Arc::new(TokioRwLock::new(())), consensus }
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
