use crate::tasks::Task;
use async_trait::async_trait;
use kaspa_core::{
    info,
    task::tick::{TickReason, TickService},
    warn,
};
use kaspa_utils::triggers::SingleTrigger;
use std::{sync::Arc, time::Duration};
use tokio::task::JoinHandle;
use workflow_perf_monitor::mem::{get_process_memory_info, ProcessMemoryInfo};

pub struct MemoryMonitorTask {
    tick_service: Arc<TickService>,
    name: String,
    fetch_interval: Duration,
    max_memory: u64,
}

impl MemoryMonitorTask {
    pub fn new(tick_service: Arc<TickService>, name: &str, fetch_interval: Duration, max_memory: u64) -> Self {
        Self { tick_service, name: name.to_owned(), fetch_interval, max_memory }
    }

    pub fn build(tick_service: Arc<TickService>, name: &str, fetch_interval: Duration, max_memory: u64) -> Arc<Self> {
        Arc::new(Self::new(tick_service, name, fetch_interval, max_memory))
    }

    async fn worker(&self) {
        #[cfg(feature = "heap")]
        let _profiler = dhat::Profiler::builder().file_name("kaspad-heap.json").build();

        warn!(
            "Starting Memory monitor {} with fetch interval of {} and maximum memory of {}",
            self.name,
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
                info!(
                    ">>> Memory monitor {}: virtual image mem {}, resident set mem {}",
                    self.name, virtual_memory_size, resident_set_size
                );
            }
        }
        warn!("Memory monitor {} with fetch interval of {} exited", self.name, self.fetch_interval.as_secs());

        // Let the system print final logs before exiting
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    pub fn start(self) -> JoinHandle<()> {
        tokio::spawn(Box::pin(async move {
            self.worker().await;
        }))
    }
}

#[async_trait]
impl Task for MemoryMonitorTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        let tick_service = self.tick_service.clone();
        let name = self.name.clone();
        let fetch_interval = self.fetch_interval;
        let max_memory = self.max_memory;
        let task = tokio::spawn(async move {
            #[cfg(feature = "heap")]
            let _profiler = dhat::Profiler::builder().file_name("kaspad-heap.json").build();

            warn!(
                "Starting Memory monitor task {} with fetch interval of {} and maximum memory of {}",
                name,
                fetch_interval.as_secs(),
                max_memory
            );
            while let TickReason::Wakeup = tick_service.as_ref().tick(fetch_interval).await {
                let ProcessMemoryInfo { resident_set_size, virtual_memory_size, .. } = get_process_memory_info().unwrap();

                if resident_set_size > max_memory {
                    warn!(">>> Resident set memory {} exceeded threshold of {}", resident_set_size, max_memory);
                    #[cfg(feature = "heap")]
                    {
                        warn!(">>> Dumping heap profiling data...");
                        drop(_profiler);
                        panic!("Resident set memory {} exceeded threshold of {}", resident_set_size, max_memory);
                    }
                } else {
                    info!(
                        ">>> Memory monitor {}: virtual image mem {}, resident set mem {}",
                        name, virtual_memory_size, resident_set_size
                    );
                }

                if stop_signal.listener.is_triggered() {
                    break;
                }
            }
            warn!("Memory monitor task {} exited", name);
            stop_signal.trigger.trigger();

            // Let the system print final logs before exiting
            tokio::time::sleep(Duration::from_millis(500)).await;
        });
        vec![task]
    }
}
