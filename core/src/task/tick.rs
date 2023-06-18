use std::{sync::Arc, time::Duration};
use triggered::{trigger, Listener, Trigger};

use super::service::{AsyncService, AsyncServiceFuture};

const TICK: &str = "tick";

pub enum TickReason {
    /// Sleep went to completion, time to wake-up
    Wakeup,

    /// Service was shutdown
    Shutdown,
}

pub struct TickService {
    shutdown_trigger: Trigger,
    shutdown_listener: Listener,
}

impl TickService {
    pub fn new() -> Self {
        let (shutdown, monitor) = trigger();
        Self { shutdown_trigger: shutdown, shutdown_listener: monitor }
    }

    /// Waits until `duration` has elapsed when the service is started.
    ///
    /// Returns immediately when the service is stopped.
    pub async fn tick(&self, duration: Duration) -> TickReason {
        match tokio::time::timeout(duration, self.shutdown_listener.clone()).await {
            Ok(()) => TickReason::Shutdown,
            Err(_) => TickReason::Wakeup,
        }
    }

    pub fn shutdown(&self) {
        self.shutdown_trigger.trigger();
    }
}

impl Default for TickService {
    fn default() -> Self {
        Self::new()
    }
}

// service trait implementation for TickService
impl AsyncService for TickService {
    fn ident(self: Arc<Self>) -> &'static str {
        TICK
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move { Ok(()) })
    }

    fn signal_exit(self: Arc<Self>) {
        self.shutdown();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move { Ok(()) })
    }
}
