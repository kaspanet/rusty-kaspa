use crate::config::constants::consensus::*;
use crate::KType;

/// Calculates the k parameter of the GHOSTDAG protocol such that anticones lager than k will be created
/// with probability less than `delta` (follows eq. 1 from section 4.2 of the PHANTOM paper)
/// `x` is expected to be 2Dλ where D is the maximal network delay and λ is the block mining rate.
/// `delta` is an upper bound for the probability of anticones larger than k.
/// Returns the minimal k such that the above conditions hold.
pub fn calculate_ghostdag_k(x: f64, delta: f64) -> u64 {
    assert!(x > 0.0);
    assert!(delta > 0.0 && delta < 1.0);
    let (mut k_hat, mut sigma, mut fraction, exp) = (0u64, 0.0, 1.0, std::f64::consts::E.powf(-x));
    loop {
        sigma += exp * fraction;
        if 1.0 - sigma < delta {
            return k_hat;
        }
        k_hat += 1;
        fraction *= x / k_hat as f64 // Computes x^k_hat/k_hat!
    }
}

/// Bps-related constants generator for 10-bps networks
pub type TenBps = Bps<10>;

/// Struct representing network blocks-per-second. Provides a bunch of const functions
/// computing various constants which are functions of the BPS value
pub struct Bps<const BPS: u64>;

impl<const BPS: u64> Bps<BPS> {
    pub const fn bps() -> u64 {
        BPS
    }

    /// Returns the GHOSTDAG K value which was pre-computed for this BPS
    /// (see [`calculate_ghostdag_k`] and `gen_ghostdag_table` for the full calculation)
    #[rustfmt::skip]
    pub const fn ghostdag_k() -> KType {
        match BPS {
            1 => 18, 2 => 31, 3 => 43, 4 => 55, 5 => 67, 6 => 79, 7 => 90, 8 => 102, 9 => 113, 10 => 124,
            11 => 135, 12 => 146, 13 => 157, 14 => 168, 15 => 179, 16 => 190, 17 => 201, 18 => 212, 19 => 223, 20 => 234,
            21 => 244, 22 => 255, 23 => 266, 24 => 277, 25 => 288, 26 => 298, 27 => 309, 28 => 320, 29 => 330, 30 => 341,
            31 => 352, 32 => 362,
            _ => panic!("see gen_ghostdag_table for currently supported values"),
        }
    }

    /// Returns the target time per block in milliseconds
    pub const fn target_time_per_block() -> u64 {
        if 1000 % BPS != 0 {
            panic!("target_time_per_block is in milliseconds hence BPS must divide 1000 with no remainder")
        }
        1000 / BPS
    }

    /// Returns the max number of direct parents a block can have
    pub const fn max_block_parents() -> u8 {
        let val = (Self::ghostdag_k() / 2) as u8;
        if val < 10 {
            10
        } else if val > 16 {
            // We currently limit the number of parents by 16 in order to preserve processing performance
            // and to prevent number of parent references per network round from growing quadratically with
            // BPS. As BPS might grow beyond 10 this will mean that blocks will reference less parents than
            // the average number of DAG tips. Which means relying on randomness between network peers for ensuring
            // that all tips are eventually merged. We conjecture that with high probability every block will
            // be merged after a log number of rounds. For mainnet this requires an increase to the value of GHOSTDAG
            // K accompanied by a short security analysis, or moving to the parameterless DAGKNIGHT.
            16
        } else {
            val
        }
    }

    pub const fn mergeset_size_limit() -> u64 {
        let val = Self::ghostdag_k() as u64 * 2;
        if val < 180 {
            180
        } else if val > 512 {
            // Bounded relatively low because we have storage complexity of O(#headers * mergeset_limit) coming
            // from reachability and GHOSTDAG stores (GHOSTDAG is avoidable but reachability is must).
            512
        } else {
            val
        }
    }

    pub const fn merge_depth_bound() -> u64 {
        BPS * MERGE_DEPTH_DURATION
    }

    pub const fn finality_depth() -> u64 {
        BPS * FINALITY_DURATION
    }

    pub const fn pruning_depth() -> u64 {
        // Based on the analysis at https://github.com/kaspanet/docs/blob/main/Reference/prunality/Prunality.pdf
        // and on the decomposition of merge depth (rule R-I therein) from finality depth (φ)
        // We add an additional merge depth unit as a safety margin for anticone finalization
        let lower_bound = Self::finality_depth()
            + Self::merge_depth_bound() * 2
            + 4 * Self::mergeset_size_limit() * Self::ghostdag_k() as u64
            + 2 * Self::ghostdag_k() as u64
            + 2;

        if lower_bound > BPS * PRUNING_DURATION {
            lower_bound
        } else {
            BPS * PRUNING_DURATION
        }
    }

    /// Sample rate for sampling blocks to the median time window (in block units, hence dependent on BPS)
    pub const fn past_median_time_sample_rate() -> u64 {
        BPS * PAST_MEDIAN_TIME_SAMPLE_INTERVAL
    }

    /// Sample rate for sampling blocks to the DA window (in block units, hence dependent on BPS)
    pub const fn difficulty_adjustment_sample_rate() -> u64 {
        BPS * DIFFICULTY_WINDOW_SAMPLE_INTERVAL
    }

    pub const fn coinbase_maturity() -> u64 {
        BPS * COINBASE_MATURITY_SECONDS
    }

    /// DAA score after which the pre-deflationary period switches to the deflationary period.
    ///
    /// This number is calculated as follows:
    ///
    /// - We define a year as 365.25 days
    /// - Half a year in seconds = 365.25 / 2 * 24 * 60 * 60 = 15778800
    /// - The network was down for three days shortly after launch
    /// - Three days in seconds = 3 * 24 * 60 * 60 = 259200
    pub const fn deflationary_phase_daa_score() -> u64 {
        BPS * (15778800 - 259200)
    }

    pub const fn pre_deflationary_phase_base_subsidy() -> u64 {
        50000000000 / BPS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore]
    #[test]
    fn gen_ghostdag_table() {
        println!("[BPS => K]");
        (1..=32).for_each(|bps| {
            let k = calculate_ghostdag_k(2.0 * NETWORK_DELAY_BOUND as f64 * bps as f64, GHOSTDAG_TAIL_DELTA);
            print!("{} => {},{}", bps, k, if bps % 10 == 0 { '\n' } else { ' ' });
        });
        println!();

        /*
           Prints the following table:

           [BPS => K]
            1 => 18, 2 => 31, 3 => 43, 4 => 55, 5 => 67, 6 => 79, 7 => 90, 8 => 102, 9 => 113, 10 => 124,
            11 => 135, 12 => 146, 13 => 157, 14 => 168, 15 => 179, 16 => 190, 17 => 201, 18 => 212, 19 => 223, 20 => 234,
            21 => 244, 22 => 255, 23 => 266, 24 => 277, 25 => 288, 26 => 298, 27 => 309, 28 => 320, 29 => 330, 30 => 341,
            31 => 352, 32 => 362,
        */
    }
}
