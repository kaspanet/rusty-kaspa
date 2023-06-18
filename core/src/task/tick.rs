use std::{sync::Arc, time::Duration};
use tokio::select;
use triggered::{trigger, Listener, Trigger};

use super::service::{AsyncService, AsyncServiceFuture};

const TICK: &str = "tick";

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
    pub async fn tick(&self, duration: Duration) {
        let shutdown_listener = self.shutdown_listener.clone();
        loop {
            select! {
                biased;
                _ = shutdown_listener => { break }
                _ = tokio::time::sleep(duration) => { break }
            }
        }
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
        self.shutdown_trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move { Ok(()) })
    }
}
