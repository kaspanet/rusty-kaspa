use crate::{CountersSnapshot, Monitor};
use kaspa_core::task::tick::TickService;
use std::time::Duration;

pub struct Unspecified {}

pub struct Builder<TS, D, CB> {
    tick_service: TS,
    fetch_interval: D,
    fetch_callback: CB,
}

impl Builder<Unspecified, Unspecified, Unspecified> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for Builder<Unspecified, Unspecified, Unspecified> {
    fn default() -> Self {
        Self { tick_service: Unspecified {}, fetch_interval: Unspecified {}, fetch_callback: Unspecified {} }
    }
}

impl<D, CB> Builder<Unspecified, D, CB> {
    pub fn with_tick_service<TS: AsRef<TickService>>(self, tick_service: TS) -> Builder<TS, D, CB> {
        Builder { tick_service, fetch_interval: self.fetch_interval, fetch_callback: self.fetch_callback }
    }
}

impl<TS, CB> Builder<TS, Unspecified, CB> {
    pub fn with_fetch_interval(self, fetch_interval: Duration) -> Builder<TS, Duration, CB> {
        Builder { tick_service: self.tick_service, fetch_interval, fetch_callback: self.fetch_callback }
    }
}

impl<TS, D> Builder<TS, D, Unspecified> {
    pub fn with_fetch_cb<CB: Fn(CountersSnapshot) + Send + Sync + 'static>(
        self,
        fetch_callback: CB,
    ) -> Builder<TS, D, Box<dyn Fn(CountersSnapshot) + Sync + Send>> {
        Builder { tick_service: self.tick_service, fetch_interval: self.fetch_interval, fetch_callback: Box::new(fetch_callback) as _ }
    }
}

impl<TS: AsRef<TickService>> Builder<TS, Unspecified, Unspecified> {
    pub fn build(self) -> Monitor<TS> {
        Monitor {
            tick_service: self.tick_service,
            fetch_interval: Duration::from_secs(1),
            counters: Default::default(),
            fetch_callback: None,
        }
    }
}

impl<TS: AsRef<TickService>> Builder<TS, Duration, Unspecified> {
    pub fn build(self) -> Monitor<TS> {
        Monitor {
            tick_service: self.tick_service,
            fetch_interval: self.fetch_interval,
            counters: Default::default(),
            fetch_callback: None,
        }
    }
}

impl<TS: AsRef<TickService>> Builder<TS, Unspecified, Box<dyn Fn(CountersSnapshot) + Sync + Send>> {
    pub fn build(self) -> Monitor<TS> {
        Monitor {
            tick_service: self.tick_service,
            fetch_interval: Duration::from_secs(1),
            counters: Default::default(),
            fetch_callback: Some(self.fetch_callback),
        }
    }
}

impl<TS: AsRef<TickService>> Builder<TS, Duration, Box<dyn Fn(CountersSnapshot) + Sync + Send>> {
    pub fn build(self) -> Monitor<TS> {
        Monitor {
            tick_service: self.tick_service,
            fetch_interval: self.fetch_interval,
            counters: Default::default(),
            fetch_callback: Some(self.fetch_callback),
        }
    }
}
