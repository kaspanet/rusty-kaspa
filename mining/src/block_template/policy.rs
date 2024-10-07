/// Policy houses the policy (configuration parameters) which is used to control
/// the generation of block templates. See the documentation for
/// NewBlockTemplate for more details on how each of these parameters are used.
#[derive(Clone)]
pub struct Policy {
    /// max_block_mass is the maximum block mass to be used when generating a block template.
    pub(crate) max_block_mass: u64,
}

impl Policy {
    pub fn new(max_block_mass: u64) -> Self {
        Self { max_block_mass }
    }
}
