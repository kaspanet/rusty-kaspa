use kaspa_consensus_core::config::params::DAAWindowParams;
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::time::unix_now;
use once_cell::sync::{Lazy, OnceCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub static SYNC_STATE: Lazy<SyncState> = Lazy::new(|| SyncState::default());

#[derive(Default)]
pub struct SyncState {
    pub consensus_manager: OnceCell<Arc<ConsensusManager>>,
    pub daa_window_params: OnceCell<DAAWindowParams>,
    pub has_peers: OnceCell<Box<dyn Fn() -> bool + 'static + Sync + Send>>,

    is_nearly_synced: Arc<AtomicBool>,
}

impl SyncState {
    pub fn is_synced(&self) -> bool {
        self.is_nearly_synced.load(Ordering::Acquire) && self.has_peers.get().is_some_and(|has_peers| has_peers())
    }

    // diff is sink timestamp + expected_daa_window_duration_in_milliseconds(daa_score) - unix_now()
    pub(super) fn is_synced_or(&self, check_diff: impl FnOnce() -> i64) -> bool {
        let (is_nearly_synced, has_peers) =
            (self.is_nearly_synced.load(Ordering::Acquire), self.has_peers.get().is_some_and(|has_peers| has_peers()));
        if !is_nearly_synced && has_peers {
            let diff = check_diff();
            if diff > 0 {
                if let Ok(_) = self.is_nearly_synced.compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed) {
                    self.watch(diff);
                }
            }
        }
        is_nearly_synced && has_peers
    }

    fn watch(&self, mut diff: i64) {
        let is_nearly_synced = Arc::clone(&self.is_nearly_synced);
        let daa_window_params = self.daa_window_params.get().unwrap().clone();
        let consensus_manager = Arc::clone(&self.consensus_manager.get().unwrap());

        tokio::spawn(async move {
            while diff > 0 {
                tokio::time::sleep(Duration::from_millis(diff as u64)).await;

                let session = consensus_manager.consensus().session().await;
                let sink = session.async_get_sink().await;
                let h = session.async_get_header(sink).await.unwrap();
                drop(session);

                diff = -(unix_now() as i64)
                    + daa_window_params.expected_daa_window_duration_in_milliseconds(h.daa_score) as i64
                    + h.timestamp as i64;
            }
            is_nearly_synced.store(false, Ordering::Release);
        });
    }
}
