#[derive(Debug)]
pub struct MiningRules {}

impl MiningRules {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for MiningRules {
    fn default() -> Self {
        Self::new()
    }
}
