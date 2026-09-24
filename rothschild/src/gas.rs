use kaspa_consensus_core::config::constants::consensus::DEFAULT_GAS_PER_LANE_LIMIT;
use rand::{
    Rng,
    distributions::{Distribution, WeightedIndex},
};
use rand_distr::Beta;

// means per cluster
const MAIN_GAS_MEAN: f64 = 1.0 / 200.0; // ~200 txs of same lane can fit in one block, still satisfying gas limit (DEFAULT_GAS_PER_LANE_LIMIT)
const THIRD_GAS_MEAN: f64 = 1.0 / 3.0;
const HALF_GAS_MEAN: f64 = 1.0 / 2.0;

// concentrations per cluster
// Var(X) = mean * (1 - mean) / (concentration + 1)
const MAIN_GAS_CONCENTRATION: f64 = 200_000.0;
const TAIL_GAS_CONCENTRATION: f64 = 5_000.0;

// ------ DISTRIBUTIION (over 10_000) ------
const COMMON_GAS_WEIGHT: u32 = 9_500; // 95%
const ZERO_GAS_WEIGHT: u32 = 200; // 2%
const THIRD_GAS_WEIGHT: u32 = 200; // 2%
const HALF_GAS_WEIGHT: u32 = 90; // 0.9%
const FULL_GAS_WEIGHT: u32 = 10; // 0.1%

pub(crate) struct GasDistribution {
    chooser: WeightedIndex<u32>,
    common: Beta<f64>,
    third: Beta<f64>,
    half: Beta<f64>,
}

impl GasDistribution {
    pub(crate) fn new() -> Self {
        Self {
            chooser: WeightedIndex::new([COMMON_GAS_WEIGHT, ZERO_GAS_WEIGHT, THIRD_GAS_WEIGHT, HALF_GAS_WEIGHT, FULL_GAS_WEIGHT])
                .expect("valid gas distribution weights"),
            common: beta_around(MAIN_GAS_MEAN, MAIN_GAS_CONCENTRATION),
            third: beta_around(THIRD_GAS_MEAN, TAIL_GAS_CONCENTRATION),
            half: beta_around(HALF_GAS_MEAN, TAIL_GAS_CONCENTRATION),
        }
    }

    pub(crate) fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> u64 {
        let gas_fraction = match self.chooser.sample(rng) {
            0 => self.common.sample(rng),
            1 => return 0,
            2 => self.third.sample(rng),
            3 => self.half.sample(rng),
            4 => return DEFAULT_GAS_PER_LANE_LIMIT,
            _ => unreachable!("gas distribution branch out of range"),
        };

        ((gas_fraction * DEFAULT_GAS_PER_LANE_LIMIT as f64).round() as u64).clamp(1, DEFAULT_GAS_PER_LANE_LIMIT)
    }
}

impl Default for GasDistribution {
    fn default() -> Self {
        Self::new()
    }
}

// mean must be ]0;1[
// concentration must be ]0;+inf]
fn beta_around(mean: f64, concentration: f64) -> Beta<f64> {
    let alpha = mean * concentration;
    let beta = (1.0 - mean) * concentration;

    Beta::new(alpha, beta).expect("valid beta distribution parameters")
}
