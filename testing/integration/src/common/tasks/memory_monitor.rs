use kaspa_core::{
    info,
    task::tick::{TickReason, TickService},
    warn,
};
use std::{sync::Arc, time::Duration};
use tokio::task::JoinHandle;
use workflow_perf_monitor::mem::{get_process_memory_info, ProcessMemoryInfo};

pub struct MemoryMonitor {
    tick_service: Arc<TickService>,
    fetch_interval: Duration,
    max_memory: u64,
}

impl MemoryMonitor {
    pub fn new(tick_service: Arc<TickService>, fetch_interval: Duration, max_memory: u64) -> Self {
        Self { tick_service, fetch_interval, max_memory }
    }

    async fn worker(&self) {
        #[cfg(feature = "heap")]
        let _profiler = dhat::Profiler::builder().file_name("kaspad-heap.json").build();

        warn!(
            ">>> Starting Memory monitor with fetch interval of {} and maximum memory of {}",
            self.fetch_interval.as_secs(),
            self.max_memory
        );
        while let TickReason::Wakeup = self.tick_service.as_ref().tick(self.fetch_interval).await {
            let ProcessMemoryInfo { resident_set_size, virtual_memory_size, .. } = get_process_memory_info().unwrap();

            if resident_set_size > self.max_memory {
                warn!(">>> Resident set memory {} exceeded threshold of {}", resident_set_size, self.max_memory);
                #[cfg(feature = "heap")]
                {
                    warn!(">>> Dumping heap profiling data...");
                    drop(_profiler);
                    panic!("Resident set memory {} exceeded threshold of {}", resident_set_size, self.max_memory);
                }
            } else {
                info!(">>> Memory monitor: virtual image mem {}, resident set mem {}", virtual_memory_size, resident_set_size);
            }
        }
        warn!(">>> Stopping Memory monitor with fetch interval of {}", self.fetch_interval.as_secs());

        // Let the system print final logs before exiting
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    pub fn start(self) -> JoinHandle<()> {
        tokio::spawn(Box::pin(async move {
            self.worker().await;
        }))
    }
}
