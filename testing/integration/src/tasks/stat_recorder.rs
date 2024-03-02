use crate::tasks::{DynTask, Task};
use async_trait::async_trait;
use kaspa_utils::triggers::SingleTrigger;
use std::{
    io::{BufWriter, Write},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{task::JoinHandle, time::sleep};
use workflow_perf_monitor::mem::{get_process_memory_info, ProcessMemoryInfo};

pub struct StatRecorderTask {
    tick: Duration,
    folder: String,
    file_prefix: String,
    timestamp: bool,
}

impl StatRecorderTask {
    pub fn build(tick: Duration, folder: String, file_prefix: String, timestamp: bool) -> Arc<Self> {
        Arc::new(Self { tick, folder, file_prefix, timestamp })
    }

    pub fn optional(tick: Duration, folder: String, file_prefix: Option<String>, timestamp: bool) -> Option<DynTask> {
        file_prefix.map(|file_prefix| Self::build(tick, folder, file_prefix, timestamp) as DynTask)
    }

    pub fn file_name(&self) -> String {
        match self.timestamp {
            true => format!("{}-{}.csv", self.file_prefix, chrono::Local::now().format("%Y-%m-%d %H-%M-%S")),
            false => format!("{}.csv", self.file_prefix),
        }
    }
}

#[async_trait]
impl Task for StatRecorderTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        let folder = self.folder.clone();
        let file_name = self.file_name();
        let tick = self.tick;
        let task = tokio::spawn(async move {
            kaspa_core::warn!("Stat recorder task starting...");
            std::fs::create_dir_all(PathBuf::from(&folder)).unwrap();
            {
                let file_path = PathBuf::from(&folder).join(file_name).into_os_string();
                kaspa_core::warn!("Recording memory metrics into file {}", file_path.to_str().unwrap());
                let f = std::fs::File::create(file_path).unwrap();
                let mut f = BufWriter::new(f);
                let start_time = Instant::now();
                let mut stopwatch = start_time;
                loop {
                    tokio::select! {
                        biased;
                        _ = stop_signal.listener.clone() => {
                            kaspa_core::trace!("Leaving stat recorder loop");
                            break;
                        }
                        _ = sleep(stopwatch + tick - Instant::now()) => {}
                    }
                    stopwatch = Instant::now();
                    let ProcessMemoryInfo { resident_set_size, .. } = get_process_memory_info().unwrap();
                    writeln!(f, "{}, {}", (stopwatch - start_time).as_millis() as f64 / 1000.0 / 60.0, resident_set_size).unwrap();
                    f.flush().unwrap();
                }
            }
            kaspa_core::warn!("Stat recorder task exited");
        });
        vec![task]
    }
}
