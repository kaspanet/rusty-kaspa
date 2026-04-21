/// Policy houses the policy (configuration parameters) which is used to control
/// the generation of block templates. See the documentation for
/// NewBlockTemplate for more details on how each of these parameters are used.
#[derive(Clone)]
pub struct Policy {
    /// max_block_mass is the maximum block mass to be used when generating a block template.
    pub(crate) max_block_mass: u64,
    /// lanes_per_block_limit is the maximum number of distinct subnet lanes a block template may include.
    pub(crate) lanes_per_block_limit: usize,
}

impl Policy {
    pub fn new(max_block_mass: u64) -> Self {
        // TODO (before merge): propagate LPB 
        Self { max_block_mass, lanes_per_block_limit: 50 }
    }
}
