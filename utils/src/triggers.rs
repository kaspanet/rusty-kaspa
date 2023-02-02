pub use triggered::{Listener, Trigger};

/// Wrapper containing a single Trigger instance
#[derive(Debug, Clone)]
pub struct SingleTrigger {
    pub trigger: Trigger,
    pub listener: Listener,
}

impl SingleTrigger {
    pub fn new() -> SingleTrigger {
        let (trigger, listener) = triggered::trigger();
        SingleTrigger { trigger, listener }
    }
}

impl Default for SingleTrigger {
    fn default() -> Self {
        Self::new()
    }
}

/// Bi-directional trigger meant to function in
/// request/response fashion
#[derive(Debug, Clone, Default)]
pub struct DuplexTrigger {
    pub request: SingleTrigger,
    pub response: SingleTrigger,
}

impl DuplexTrigger {
    pub fn new() -> DuplexTrigger {
        DuplexTrigger { request: SingleTrigger::new(), response: SingleTrigger::new() }
    }
}
