use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

#[derive(Default, Clone, Debug)]
pub struct GrpcCounters {
    pub bytes_tx: Arc<AtomicUsize>,
    pub bytes_rx: Arc<AtomicUsize>,
}
