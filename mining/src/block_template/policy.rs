/// Policy houses the policy (configuration parameters) which is used to control
/// the generation of block templates. See the documentation for
/// NewBlockTemplate for more details on how each of these parameters are used.
#[derive(Clone)]
pub struct Policy {
    /// max_block_mass is the maximum block mass to be used when generating a block template.
    pub(crate) max_block_mass: u64,
    /// lanes_per_block_limit is the maximum number of distinct subnet lanes a block template may include.
    pub(crate) lanes_per_block_limit: usize,
    /// gas_per_lane_limit is the maximum total gas per lane in a block template.
    pub(crate) gas_per_lane_limit: u64,
}

impl Policy {
    pub fn new(max_block_mass: u64) -> Self {
        // TODO (before merge): propagate LPB and GPL limits
        Self { max_block_mass, lanes_per_block_limit: 50, gas_per_lane_limit: 500_000 }
    }
}
