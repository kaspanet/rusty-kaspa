use kaspa_core::{
    error,
    task::{
        service::{AsyncService, AsyncServiceFuture},
        tick::{TickReason, TickService},
    },
    trace, warn,
};
use workflow_perf_monitor::{
    cpu::{processor_numbers, ProcessStat},
    fd::fd_count_cur,
    io::{get_process_io_stats, IOStats},
    mem::{get_process_memory_info, ProcessMemoryInfo},
};

use std::{sync::Arc, time::Duration, time::Instant};

use crate::counters::CountersSnapshot;
use crate::{counters::Counters, error::Error};

pub mod builder;
pub mod counters;
pub mod error;

pub const SERVICE_NAME: &str = "perf-monitor";

pub struct Monitor<TS: AsRef<TickService>> {
    tick_service: TS,
    fetch_interval: Duration,
    counters: Counters,
    fetch_callback: Option<Box<dyn Fn(CountersSnapshot) + Sync + Send>>,
}

impl<TS: AsRef<TickService>> Monitor<TS> {
    pub fn snapshot(&self) -> CountersSnapshot {
        self.counters.snapshot()
    }

    pub async fn worker(&self) -> Result<(), Error> {
        let mut last_log_time = Instant::now();
        let mut last_read = 0;
        let mut last_written = 0;
        let mut process_stat = ProcessStat::cur()?;
        while let TickReason::Wakeup = self.tick_service.as_ref().tick(self.fetch_interval).await {
            let ProcessMemoryInfo { resident_set_size, virtual_memory_size, .. } = get_process_memory_info()?;
            let core_num = processor_numbers()?;
            let cpu_usage = process_stat.cpu()?;
            let fd_num = fd_count_cur()?;
            let IOStats { read_bytes: disk_io_read_bytes, write_bytes: disk_io_write_bytes, .. } = get_process_io_stats()?;

            let time_delta = last_log_time.elapsed();

            let read_delta = disk_io_read_bytes.checked_sub(last_read).unwrap_or_else(|| {
                warn!("new io read bytes value is less than previous, new: {disk_io_read_bytes}, previous: {last_read}");
                0
            });
            let write_delta = disk_io_write_bytes.checked_sub(last_written).unwrap_or_else(|| {
                warn!("new io write bytes value is less than previous, new: {disk_io_write_bytes}, previous: {last_written}");
                0
            });

            let now = Instant::now();
            last_log_time = now;
            last_written = disk_io_write_bytes;
            last_read = disk_io_read_bytes;

            let counters_snapshot = CountersSnapshot {
                resident_set_size,
                virtual_memory_size,
                core_num,
                cpu_usage,
                fd_num,
                disk_io_read_bytes,
                disk_io_write_bytes,
                disk_io_read_per_sec: read_delta as f64 * 1000.0 / time_delta.as_millis() as f64,
                disk_io_write_per_sec: write_delta as f64 * 1000.0 / time_delta.as_millis() as f64,
            };
            self.counters.update(counters_snapshot);
            if let Some(ref cb) = self.fetch_callback {
                cb(counters_snapshot);
            }
        }
        // Let the system print final logs before exiting
        tokio::time::sleep(Duration::from_millis(500)).await;
        trace!("{SERVICE_NAME} worker exiting");
        Ok(())
    }
}

// service trait implementation for Monitor
impl<TS: AsRef<TickService> + Send + Sync + 'static> AsyncService for Monitor<TS> {
    fn ident(self: Arc<Self>) -> &'static str {
        SERVICE_NAME
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await.unwrap_or_else(|e| {
                error!("worker error: {e:?}");
            });
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", SERVICE_NAME);
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            trace!("{} stopped", SERVICE_NAME);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::Builder;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::sleep;

    #[tokio::test]
    async fn monitor_works() {
        kaspa_core::log::try_init_logger("info, kaspa_perf_monitor=trace");

        let ts = Arc::new(TickService::new());
        let call_count = Arc::new(AtomicUsize::new(0));
        let to_move = call_count.clone();
        let cb = move |counters| {
            trace!("fetch counters: {:?}", counters);
            to_move.fetch_add(1, Ordering::Relaxed);
            let big = vec![0u64; 1_000_000];
            let _ = big.into_iter().sum::<u64>();
        };
        let m = Builder::new().with_fetch_cb(cb).with_tick_service(ts.clone()).build();

        let handle1 = tokio::spawn(async move {
            sleep(Duration::from_millis(3500)).await;
            ts.shutdown()
        });
        let handle2 = tokio::spawn(async move { m.worker().await });

        _ = tokio::join!(handle1, handle2);
        assert_eq!(call_count.load(Ordering::Relaxed), 3);
    }
}
