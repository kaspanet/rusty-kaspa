use std::sync::{atomic::AtomicBool, Arc};

#[derive(Debug)]
pub struct MiningRules {
    pub no_transactions: Arc<AtomicBool>,
    pub blue_parents_only: Arc<AtomicBool>,
}

impl MiningRules {
    pub fn new() -> Self {
        Self { no_transactions: Arc::new(AtomicBool::new(false)), blue_parents_only: Arc::new(AtomicBool::new(false)) }
    }
}

impl Default for MiningRules {
    fn default() -> Self {
        Self::new()
    }
}
