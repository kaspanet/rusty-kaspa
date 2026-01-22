use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

#[derive(Default, Clone, Debug)]
pub struct TowerConnectionCounters {
    pub bytes_tx: Arc<AtomicUsize>,
    pub bytes_rx: Arc<AtomicUsize>,
}
