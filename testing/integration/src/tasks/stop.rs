use crate::tasks::Task;
use async_trait::async_trait;
use kaspa_core::{trace, warn};
use kaspa_utils::triggers::SingleTrigger;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub(super) struct StopTask {
    main_stop_signal: SingleTrigger,
}

impl StopTask {
    pub fn new(main_stop_signal: SingleTrigger) -> Self {
        Self { main_stop_signal }
    }

    pub fn build(main_stop_signal: SingleTrigger) -> Arc<Self> {
        Arc::new(Self::new(main_stop_signal))
    }
}

#[async_trait]
impl Task for StopTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        let main_stop_signal = self.main_stop_signal.clone();
        let task = tokio::spawn(async move {
            warn!("Stop propagator task starting...");
            tokio::select! {
                _ = main_stop_signal.listener.clone() => {
                    trace!("The main stop signal has been triggered");
                    if !stop_signal.listener.is_triggered() {
                        warn!("Stop propagator sending a stop signal to the sub-tasks...");
                    }
                    stop_signal.trigger.trigger();
                }
                _ = stop_signal.listener.clone() => {
                    trace!("The stop signal has been triggered, no need to propagate from main to sub-tasks");
                }
            }
            warn!("Stop propagator task exited");
        });
        vec![task]
    }
}
