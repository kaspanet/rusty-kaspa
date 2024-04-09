use crate::tasks::Task;
use async_trait::async_trait;
use kaspa_core::{task::tick::TickService, warn};
use kaspa_utils::triggers::SingleTrigger;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct TickTask {
    tick_service: Arc<TickService>,
}

impl TickTask {
    pub fn new(tick_service: Arc<TickService>) -> Self {
        Self { tick_service }
    }

    pub fn build(tick_service: Arc<TickService>) -> Arc<Self> {
        Arc::new(Self::new(tick_service))
    }

    pub fn service(&self) -> Arc<TickService> {
        self.tick_service.clone()
    }
}

#[async_trait]
impl Task for TickTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        let tick_service = self.service();
        let task = tokio::spawn(async move {
            warn!("Tick task starting...");
            stop_signal.listener.await;
            tick_service.shutdown();
            warn!("Tick task exited");
        });
        vec![task]
    }
}
