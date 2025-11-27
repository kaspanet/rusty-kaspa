use kaspa_consensus_core::{
    coinbase::*,
    config::params::ForkedParam,
    errors::coinbase::{CoinbaseError, CoinbaseResult},
    subnets,
    tx::{ScriptPublicKey, ScriptVec, Transaction, TransactionOutput},
    BlockHashMap, BlockHashSet,
};
use std::convert::TryInto;

use crate::{constants, model::stores::ghostdag::GhostdagData};

const LENGTH_OF_BLUE_SCORE: usize = size_of::<u64>();
const LENGTH_OF_SUBSIDY: usize = size_of::<u64>();
const LENGTH_OF_SCRIPT_PUB_KEY_VERSION: usize = size_of::<u16>();
const LENGTH_OF_SCRIPT_PUB_KEY_LENGTH: usize = size_of::<u8>();

const MIN_PAYLOAD_LENGTH: usize =
    LENGTH_OF_BLUE_SCORE + LENGTH_OF_SUBSIDY + LENGTH_OF_SCRIPT_PUB_KEY_VERSION + LENGTH_OF_SCRIPT_PUB_KEY_LENGTH;

// We define a year as 365.25 days and a month as 365.25 / 12 = 30.4375
// SECONDS_PER_MONTH = 30.4375 * 24 * 60 * 60
const SECONDS_PER_MONTH: u64 = 2629800;

pub const SUBSIDY_BY_MONTH_TABLE_SIZE: usize = 426;
pub type SubsidyByMonthTable = [u64; SUBSIDY_BY_MONTH_TABLE_SIZE];

#[derive(Clone)]
pub struct CoinbaseManager {
    coinbase_payload_script_public_key_max_len: u8,
    max_coinbase_payload_len: usize,
    deflationary_phase_daa_score: u64,
    pre_deflationary_phase_base_subsidy: u64,
    bps: ForkedParam<u64>,

    /// Precomputed subsidy by month tables (for before and after the Crescendo hardfork)
    subsidy_by_month_table_before: SubsidyByMonthTable,
    subsidy_by_month_table_after: SubsidyByMonthTable,

    /// The crescendo activation DAA score where BPS increased from 1 to 10.
    /// This score is required here long-term (and not only for the actual forking), in
    /// order to correctly determine the subsidy month from the live DAA score of the network   
    crescendo_activation_daa_score: u64,
}

/// Struct used to streamline payload parsing
struct PayloadParser<'a> {
    remaining: &'a [u8], // The unparsed remainder
}

impl<'a> PayloadParser<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { remaining: data }
    }

    /// Returns a slice with the first `n` bytes of `remaining`, while setting `remaining` to the remaining part
    fn take(&mut self, n: usize) -> &[u8] {
        let (segment, remaining) = self.remaining.split_at(n);
        self.remaining = remaining;
        segment
    }
}

impl CoinbaseManager {
    pub fn new(
        coinbase_payload_script_public_key_max_len: u8,
        max_coinbase_payload_len: usize,
        deflationary_phase_daa_score: u64,
        pre_deflationary_phase_base_subsidy: u64,
        bps: ForkedParam<u64>,
    ) -> Self {
        // Precomputed subsidy by month table for the actual block per second rate
        // Here values are rounded up so that we keep the same number of rewarding months as in the original 1 BPS table.
        // In a 10 BPS network, the induced increase in total rewards is 51 KAS (see tests::calc_high_bps_total_rewards_delta())
        let subsidy_by_month_table_before: SubsidyByMonthTable =
            core::array::from_fn(|i| SUBSIDY_BY_MONTH_TABLE[i].div_ceil(bps.before()));
        let subsidy_by_month_table_after: SubsidyByMonthTable =
            core::array::from_fn(|i| SUBSIDY_BY_MONTH_TABLE[i].div_ceil(bps.after()));
        Self {
            coinbase_payload_script_public_key_max_len,
            max_coinbase_payload_len,
            deflationary_phase_daa_score,
            pre_deflationary_phase_base_subsidy,
            bps,
            subsidy_by_month_table_before,
            subsidy_by_month_table_after,
            crescendo_activation_daa_score: bps.activation().daa_score(),
        }
    }

    #[cfg(test)]
    #[inline]
    pub fn bps(&self) -> ForkedParam<u64> {
        self.bps
    }

    pub fn expected_coinbase_transaction<T: AsRef<[u8]>>(
        &self,
        daa_score: u64,
        miner_data: MinerData<T>,
        ghostdag_data: &GhostdagData,
        mergeset_rewards: &BlockHashMap<BlockRewardData>,
        mergeset_non_daa: &BlockHashSet,
    ) -> CoinbaseResult<CoinbaseTransactionTemplate> {
        let mut outputs = Vec::with_capacity(ghostdag_data.mergeset_blues.len() + 1); // + 1 for possible red reward

        // Add an output for each mergeset blue block (∩ DAA window), paying to the script reported by the block.
        // Note that combinatorically it is nearly impossible for a blue block to be non-DAA
        for blue in ghostdag_data.mergeset_blues.iter().filter(|h| !mergeset_non_daa.contains(h)) {
            let reward_data = mergeset_rewards.get(blue).unwrap();
            if reward_data.subsidy + reward_data.total_fees > 0 {
                outputs
                    .push(TransactionOutput::new(reward_data.subsidy + reward_data.total_fees, reward_data.script_public_key.clone()));
            }
        }

        // Collect all rewards from mergeset reds ∩ DAA window and create a
        // single output rewarding all to the current block (the "merging" block)
        let mut red_reward = 0u64;

        // bps activation = crescendo activation
        if self.bps.activation().is_active(daa_score) {
            for red in ghostdag_data.mergeset_reds.iter() {
                let reward_data = mergeset_rewards.get(red).unwrap();
                if mergeset_non_daa.contains(red) {
                    red_reward += reward_data.total_fees;
                } else {
                    red_reward += reward_data.subsidy + reward_data.total_fees;
                }
            }
        } else {
            for red in ghostdag_data.mergeset_reds.iter().filter(|h| !mergeset_non_daa.contains(h)) {
                let reward_data = mergeset_rewards.get(red).unwrap();
                red_reward += reward_data.subsidy + reward_data.total_fees;
            }
        }

        if red_reward > 0 {
            outputs.push(TransactionOutput::new(red_reward, miner_data.script_public_key.clone()));
        }

        // Build the current block's payload
        let subsidy = self.calc_block_subsidy(daa_score);
        let payload = self.serialize_coinbase_payload(&CoinbaseData { blue_score: ghostdag_data.blue_score, subsidy, miner_data })?;

        Ok(CoinbaseTransactionTemplate {
            tx: Transaction::new(constants::TX_VERSION, vec![], outputs, 0, subnets::SUBNETWORK_ID_COINBASE, 0, payload),
            has_red_reward: red_reward > 0,
        })
    }

    pub fn serialize_coinbase_payload<T: AsRef<[u8]>>(&self, data: &CoinbaseData<T>) -> CoinbaseResult<Vec<u8>> {
        let script_pub_key_len = data.miner_data.script_public_key.script().len();
        if script_pub_key_len > self.coinbase_payload_script_public_key_max_len as usize {
            return Err(CoinbaseError::PayloadScriptPublicKeyLenAboveMax(
                script_pub_key_len,
                self.coinbase_payload_script_public_key_max_len,
            ));
        }
        let payload: Vec<u8> = data.blue_score.to_le_bytes().iter().copied()                    // Blue score                   (u64)
            .chain(data.subsidy.to_le_bytes().iter().copied())                                  // Subsidy                      (u64)
            .chain(data.miner_data.script_public_key.version().to_le_bytes().iter().copied())   // Script public key version    (u16)
            .chain((script_pub_key_len as u8).to_le_bytes().iter().copied())                    // Script public key length     (u8)
            .chain(data.miner_data.script_public_key.script().iter().copied())                  // Script public key            
            .chain(data.miner_data.extra_data.as_ref().iter().copied())                         // Extra data
            .collect();

        Ok(payload)
    }

    pub fn modify_coinbase_payload<T: AsRef<[u8]>>(&self, mut payload: Vec<u8>, miner_data: &MinerData<T>) -> CoinbaseResult<Vec<u8>> {
        let script_pub_key_len = miner_data.script_public_key.script().len();
        if script_pub_key_len > self.coinbase_payload_script_public_key_max_len as usize {
            return Err(CoinbaseError::PayloadScriptPublicKeyLenAboveMax(
                script_pub_key_len,
                self.coinbase_payload_script_public_key_max_len,
            ));
        }

        // Keep only blue score and subsidy. Note that truncate does not modify capacity, so
        // the usual case where the payloads are the same size will not trigger a reallocation
        payload.truncate(LENGTH_OF_BLUE_SCORE + LENGTH_OF_SUBSIDY);
        payload.extend(
            miner_data.script_public_key.version().to_le_bytes().iter().copied() // Script public key version (u16)
                .chain((script_pub_key_len as u8).to_le_bytes().iter().copied()) // Script public key length  (u8)
                .chain(miner_data.script_public_key.script().iter().copied())    // Script public key
                .chain(miner_data.extra_data.as_ref().iter().copied()), // Extra data
        );

        Ok(payload)
    }

    pub fn deserialize_coinbase_payload<'a>(&self, payload: &'a [u8]) -> CoinbaseResult<CoinbaseData<&'a [u8]>> {
        if payload.len() < MIN_PAYLOAD_LENGTH {
            return Err(CoinbaseError::PayloadLenBelowMin(payload.len(), MIN_PAYLOAD_LENGTH));
        }

        if payload.len() > self.max_coinbase_payload_len {
            return Err(CoinbaseError::PayloadLenAboveMax(payload.len(), self.max_coinbase_payload_len));
        }

        let mut parser = PayloadParser::new(payload);

        let blue_score = u64::from_le_bytes(parser.take(LENGTH_OF_BLUE_SCORE).try_into().unwrap());
        let subsidy = u64::from_le_bytes(parser.take(LENGTH_OF_SUBSIDY).try_into().unwrap());
        let script_pub_key_version = u16::from_le_bytes(parser.take(LENGTH_OF_SCRIPT_PUB_KEY_VERSION).try_into().unwrap());
        let script_pub_key_len = u8::from_le_bytes(parser.take(LENGTH_OF_SCRIPT_PUB_KEY_LENGTH).try_into().unwrap());

        if script_pub_key_len > self.coinbase_payload_script_public_key_max_len {
            return Err(CoinbaseError::PayloadScriptPublicKeyLenAboveMax(
                script_pub_key_len as usize,
                self.coinbase_payload_script_public_key_max_len,
            ));
        }

        if parser.remaining.len() < script_pub_key_len as usize {
            return Err(CoinbaseError::PayloadCantContainScriptPublicKey(
                payload.len(),
                MIN_PAYLOAD_LENGTH + script_pub_key_len as usize,
            ));
        }

        let script_public_key =
            ScriptPublicKey::new(script_pub_key_version, ScriptVec::from_slice(parser.take(script_pub_key_len as usize)));
        let extra_data = parser.remaining;

        Ok(CoinbaseData { blue_score, subsidy, miner_data: MinerData { script_public_key, extra_data } })
    }

    pub fn calc_block_subsidy(&self, daa_score: u64) -> u64 {
        if daa_score < self.deflationary_phase_daa_score {
            return self.pre_deflationary_phase_base_subsidy;
        }

        let subsidy_month = self.subsidy_month(daa_score) as usize;
        let subsidy_table = if self.bps.activation().is_active(daa_score) {
            &self.subsidy_by_month_table_after
        } else {
            &self.subsidy_by_month_table_before
        };
        subsidy_table[subsidy_month.min(subsidy_table.len() - 1)]
    }

    /// Get the subsidy month as function of the current DAA score.
    ///
    /// Note that this function is called only if daa_score >= self.deflationary_phase_daa_score
    fn subsidy_month(&self, daa_score: u64) -> u64 {
        let seconds_since_deflationary_phase_started = if self.crescendo_activation_daa_score < self.deflationary_phase_daa_score {
            // crescendo_activation < deflationary_phase <= daa_score (activated before deflation)
            (daa_score - self.deflationary_phase_daa_score) / self.bps.after()
        } else if daa_score < self.crescendo_activation_daa_score {
            // deflationary_phase <= daa_score < crescendo_activation (pre activation)
            (daa_score - self.deflationary_phase_daa_score) / self.bps.before()
        } else {
            // Else - deflationary_phase <= crescendo_activation <= daa_score.
            // Count seconds differently before and after Crescendo activation
            (self.crescendo_activation_daa_score - self.deflationary_phase_daa_score) / self.bps.before()
                + (daa_score - self.crescendo_activation_daa_score) / self.bps.after()
        };

        seconds_since_deflationary_phase_started / SECONDS_PER_MONTH
    }

    #[cfg(test)]
    pub fn legacy_calc_block_subsidy(&self, daa_score: u64) -> u64 {
        if daa_score < self.deflationary_phase_daa_score {
            return self.pre_deflationary_phase_base_subsidy;
        }

        // Note that this calculation implicitly assumes that block per second = 1 (by assuming daa score diff is in second units).
        let months_since_deflationary_phase_started = (daa_score - self.deflationary_phase_daa_score) / SECONDS_PER_MONTH;
        assert!(months_since_deflationary_phase_started <= usize::MAX as u64);
        let months_since_deflationary_phase_started: usize = months_since_deflationary_phase_started as usize;
        if months_since_deflationary_phase_started >= SUBSIDY_BY_MONTH_TABLE.len() {
            *SUBSIDY_BY_MONTH_TABLE.last().unwrap()
        } else {
            SUBSIDY_BY_MONTH_TABLE[months_since_deflationary_phase_started]
        }
    }
}

/*
    This table was pre-calculated by calling `calcDeflationaryPeriodBlockSubsidyFloatCalc` (in kaspad-go) for all months until reaching 0 subsidy.
    To regenerate this table, run `TestBuildSubsidyTable` in coinbasemanager_test.go (note the `deflationaryPhaseBaseSubsidy` therein).
    These values represent the reward per second for each month (= reward per block for 1 BPS).
*/
#[rustfmt::skip]
const SUBSIDY_BY_MONTH_TABLE: [u64; 426] = [
	44000000000, 41530469757, 39199543598, 36999442271, 34922823143, 32962755691, 31112698372, 29366476791, 27718263097, 26162556530, 24694165062, 23308188075, 22000000000, 20765234878, 19599771799, 18499721135, 17461411571, 16481377845, 15556349186, 14683238395, 13859131548, 13081278265, 12347082531, 11654094037, 11000000000,
	10382617439, 9799885899, 9249860567, 8730705785, 8240688922, 7778174593, 7341619197, 6929565774, 6540639132, 6173541265, 5827047018, 5500000000, 5191308719, 4899942949, 4624930283, 4365352892, 4120344461, 3889087296, 3670809598, 3464782887, 3270319566, 3086770632, 2913523509, 2750000000, 2595654359,
	2449971474, 2312465141, 2182676446, 2060172230, 1944543648, 1835404799, 1732391443, 1635159783, 1543385316, 1456761754, 1375000000, 1297827179, 1224985737, 1156232570, 1091338223, 1030086115, 972271824, 917702399, 866195721, 817579891, 771692658, 728380877, 687500000, 648913589, 612492868,
	578116285, 545669111, 515043057, 486135912, 458851199, 433097860, 408789945, 385846329, 364190438, 343750000, 324456794, 306246434, 289058142, 272834555, 257521528, 243067956, 229425599, 216548930, 204394972, 192923164, 182095219, 171875000, 162228397, 153123217, 144529071,
	136417277, 128760764, 121533978, 114712799, 108274465, 102197486, 96461582, 91047609, 85937500, 81114198, 76561608, 72264535, 68208638, 64380382, 60766989, 57356399, 54137232, 51098743, 48230791, 45523804, 42968750, 40557099, 38280804, 36132267, 34104319,
	32190191, 30383494, 28678199, 27068616, 25549371, 24115395, 22761902, 21484375, 20278549, 19140402, 18066133, 17052159, 16095095, 15191747, 14339099, 13534308, 12774685, 12057697, 11380951, 10742187, 10139274, 9570201, 9033066, 8526079, 8047547,
	7595873, 7169549, 6767154, 6387342, 6028848, 5690475, 5371093, 5069637, 4785100, 4516533, 4263039, 4023773, 3797936, 3584774, 3383577, 3193671, 3014424, 2845237, 2685546, 2534818, 2392550, 2258266, 2131519, 2011886, 1898968,
	1792387, 1691788, 1596835, 1507212, 1422618, 1342773, 1267409, 1196275, 1129133, 1065759, 1005943, 949484, 896193, 845894, 798417, 753606, 711309, 671386, 633704, 598137, 564566, 532879, 502971, 474742, 448096,
	422947, 399208, 376803, 355654, 335693, 316852, 299068, 282283, 266439, 251485, 237371, 224048, 211473, 199604, 188401, 177827, 167846, 158426, 149534, 141141, 133219, 125742, 118685, 112024, 105736,
	99802, 94200, 88913, 83923, 79213, 74767, 70570, 66609, 62871, 59342, 56012, 52868, 49901, 47100, 44456, 41961, 39606, 37383, 35285, 33304, 31435, 29671, 28006, 26434, 24950,
	23550, 22228, 20980, 19803, 18691, 17642, 16652, 15717, 14835, 14003, 13217, 12475, 11775, 11114, 10490, 9901, 9345, 8821, 8326, 7858, 7417, 7001, 6608, 6237, 5887,
	5557, 5245, 4950, 4672, 4410, 4163, 3929, 3708, 3500, 3304, 3118, 2943, 2778, 2622, 2475, 2336, 2205, 2081, 1964, 1854, 1750, 1652, 1559, 1471, 1389,
	1311, 1237, 1168, 1102, 1040, 982, 927, 875, 826, 779, 735, 694, 655, 618, 584, 551, 520, 491, 463, 437, 413, 389, 367, 347, 327,
	309, 292, 275, 260, 245, 231, 218, 206, 194, 183, 173, 163, 154, 146, 137, 130, 122, 115, 109, 103, 97, 91, 86, 81, 77,
	73, 68, 65, 61, 57, 54, 51, 48, 45, 43, 40, 38, 36, 34, 32, 30, 28, 27, 25, 24, 22, 21, 20, 19, 18,
	17, 16, 15, 14, 13, 12, 12, 11, 10, 10, 9, 9, 8, 8, 7, 7, 6, 6, 6, 5, 5, 5, 4, 4, 4,
	4, 3, 3, 3, 3, 3, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
	0,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::MAINNET_PARAMS;
    use kaspa_consensus_core::{
        config::params::{ForkActivation, Params, SIMNET_PARAMS},
        constants::SOMPI_PER_KASPA,
        network::{NetworkId, NetworkType},
        tx::scriptvec,
    };

    #[test]
    fn calc_high_bps_total_rewards_delta() {
        let legacy_cbm = create_legacy_manager();
        let pre_deflationary_rewards = legacy_cbm.pre_deflationary_phase_base_subsidy * legacy_cbm.deflationary_phase_daa_score;
        let total_rewards: u64 = pre_deflationary_rewards + SUBSIDY_BY_MONTH_TABLE.iter().map(|x| x * SECONDS_PER_MONTH).sum::<u64>();
        let testnet_11_bps = SIMNET_PARAMS.bps().after();
        let total_high_bps_rewards_rounded_up: u64 = pre_deflationary_rewards
            + SUBSIDY_BY_MONTH_TABLE.iter().map(|x| (x.div_ceil(testnet_11_bps) * testnet_11_bps) * SECONDS_PER_MONTH).sum::<u64>();

        let cbm = create_manager(&SIMNET_PARAMS);
        let total_high_bps_rewards: u64 = pre_deflationary_rewards
            + cbm.subsidy_by_month_table_before.iter().map(|x| x * SECONDS_PER_MONTH * cbm.bps().before()).sum::<u64>();
        assert_eq!(total_high_bps_rewards_rounded_up, total_high_bps_rewards, "subsidy adjusted to bps must be rounded up");

        let delta = total_high_bps_rewards as i64 - total_rewards as i64;

        println!("Total rewards: {} sompi => {} KAS", total_rewards, total_rewards / SOMPI_PER_KASPA);
        println!("Total high bps rewards: {} sompi => {} KAS", total_high_bps_rewards, total_high_bps_rewards / SOMPI_PER_KASPA);
        println!("Delta: {} sompi => {} KAS", delta, delta / SOMPI_PER_KASPA as i64);
    }

    #[test]
    fn subsidy_by_month_table_test() {
        let cbm = create_legacy_manager();
        cbm.subsidy_by_month_table_before.iter().enumerate().for_each(|(i, x)| {
            assert_eq!(SUBSIDY_BY_MONTH_TABLE[i], *x, "for 1 BPS, const table and precomputed values must match");
        });

        for network_id in NetworkId::iter() {
            let cbm = create_manager(&network_id.into());
            cbm.subsidy_by_month_table_before.iter().enumerate().for_each(|(i, x)| {
                assert_eq!(
                    SUBSIDY_BY_MONTH_TABLE[i].div_ceil(cbm.bps().before()),
                    *x,
                    "{}: locally computed and precomputed values must match",
                    network_id
                );
            });
            cbm.subsidy_by_month_table_after.iter().enumerate().for_each(|(i, x)| {
                assert_eq!(
                    SUBSIDY_BY_MONTH_TABLE[i].div_ceil(cbm.bps().after()),
                    *x,
                    "{}: locally computed and precomputed values must match",
                    network_id
                );
            });
        }
    }

    /// Takes over 60 seconds, run with the following command line:
    /// `cargo test --release --package kaspa-consensus --lib -- processes::coinbase::tests::verify_crescendo_emission_schedule --exact --nocapture --ignored`
    #[test]
    #[ignore = "long"]
    fn verify_crescendo_emission_schedule() {
        // No need to loop over all nets since the relevant params are only
        // deflation and activation DAA scores (and the test is long anyway)
        for network_id in [NetworkId::new(NetworkType::Mainnet)] {
            let mut params: Params = network_id.into();
            params.crescendo_activation = ForkActivation::never();
            let cbm = create_manager(&params);
            let (baseline_epochs, baseline_total) = calculate_emission(cbm);

            let mut activations = vec![10000, 33444444, 120727479];
            for network_id in NetworkId::iter() {
                let activation = Params::from(network_id).crescendo_activation;
                if activation != ForkActivation::never() && activation != ForkActivation::always() {
                    activations.push(activation.daa_score());
                }
            }

            // Loop over a few random activation points + specified activation points for all nets
            for activation in activations {
                params.crescendo_activation = ForkActivation::new(activation);
                let cbm = create_manager(&params);
                let (new_epochs, new_total) = calculate_emission(cbm);

                // Epochs only represents the number of times the subsidy changed (lower after activation due to rounding)
                println!("BASELINE:\t{}\tepochs, total emission: {}", baseline_epochs, baseline_total);
                println!("CRESCENDO:\t{}\tepochs, total emission: {}, activation: {}", new_epochs, new_total, activation);

                let diff = (new_total as i64 - baseline_total as i64) / SOMPI_PER_KASPA as i64;
                assert!(diff.abs() <= 51, "activation: {}", activation);
                println!("DIFF (KAS): {}", diff);
            }
        }
    }

    fn calculate_emission(cbm: CoinbaseManager) -> (u64, u64) {
        let activation = cbm.bps().activation().daa_score();
        let mut current = 0;
        let mut total = 0;
        let mut epoch = 0u64;
        let mut prev = cbm.calc_block_subsidy(0);
        loop {
            let subsidy = cbm.calc_block_subsidy(current);
            // Pre activation we expect the legacy calc (1bps)
            if current < activation {
                assert_eq!(cbm.legacy_calc_block_subsidy(current), subsidy);
            }
            if subsidy == 0 {
                break;
            }
            total += subsidy;
            if subsidy != prev {
                println!("epoch: {}, subsidy: {}", epoch, subsidy);
                prev = subsidy;
                epoch += 1;
            }
            current += 1;
        }

        (epoch, total)
    }

    #[test]
    fn subsidy_test() {
        const PRE_DEFLATIONARY_PHASE_BASE_SUBSIDY: u64 = 50000000000;
        const DEFLATIONARY_PHASE_INITIAL_SUBSIDY: u64 = 44000000000;
        const SECONDS_PER_MONTH: u64 = 2629800;
        const SECONDS_PER_HALVING: u64 = SECONDS_PER_MONTH * 12;

        for network_id in NetworkId::iter() {
            let mut params: Params = network_id.into();
            if params.crescendo_activation != ForkActivation::always() {
                // We test activation scenarios in verify_crescendo_emission_schedule
                params.crescendo_activation = ForkActivation::never();
            }
            let cbm = create_manager(&params);
            let bps = params.bps().before();

            let pre_deflationary_phase_base_subsidy = PRE_DEFLATIONARY_PHASE_BASE_SUBSIDY / bps;
            let deflationary_phase_initial_subsidy = DEFLATIONARY_PHASE_INITIAL_SUBSIDY / bps;
            let blocks_per_halving = SECONDS_PER_HALVING * bps;

            struct Test {
                name: &'static str,
                daa_score: u64,
                expected: u64,
            }

            let tests = vec![
                Test { name: "first mined block", daa_score: 1, expected: pre_deflationary_phase_base_subsidy },
                Test {
                    name: "before deflationary phase",
                    daa_score: params.deflationary_phase_daa_score - 1,
                    expected: pre_deflationary_phase_base_subsidy,
                },
                Test {
                    name: "start of deflationary phase",
                    daa_score: params.deflationary_phase_daa_score,
                    expected: deflationary_phase_initial_subsidy,
                },
                Test {
                    name: "after one halving",
                    daa_score: params.deflationary_phase_daa_score + blocks_per_halving,
                    expected: deflationary_phase_initial_subsidy / 2,
                },
                Test {
                    name: "after 2 halvings",
                    daa_score: params.deflationary_phase_daa_score + 2 * blocks_per_halving,
                    expected: deflationary_phase_initial_subsidy / 4,
                },
                Test {
                    name: "after 5 halvings",
                    daa_score: params.deflationary_phase_daa_score + 5 * blocks_per_halving,
                    expected: deflationary_phase_initial_subsidy / 32,
                },
                Test {
                    name: "after 32 halvings",
                    daa_score: params.deflationary_phase_daa_score + 32 * blocks_per_halving,
                    expected: (DEFLATIONARY_PHASE_INITIAL_SUBSIDY / 2_u64.pow(32)).div_ceil(bps),
                },
                Test {
                    name: "just before subsidy depleted",
                    daa_score: params.deflationary_phase_daa_score + 35 * blocks_per_halving,
                    expected: 1,
                },
                Test {
                    name: "after subsidy depleted",
                    daa_score: params.deflationary_phase_daa_score + 36 * blocks_per_halving,
                    expected: 0,
                },
            ];

            for t in tests {
                assert_eq!(cbm.calc_block_subsidy(t.daa_score), t.expected, "{} test '{}' failed", network_id, t.name);
                if bps == 1 {
                    assert_eq!(cbm.legacy_calc_block_subsidy(t.daa_score), t.expected, "{} test '{}' failed", network_id, t.name);
                }
            }
        }
    }

    #[test]
    fn payload_serialization_test() {
        let cbm = create_manager(&MAINNET_PARAMS);

        let script_data = [33u8, 255];
        let extra_data = [2u8, 3];
        let data = CoinbaseData {
            blue_score: 56,
            subsidy: 44000000000,
            miner_data: MinerData {
                script_public_key: ScriptPublicKey::new(0, ScriptVec::from_slice(&script_data)),
                extra_data: &extra_data as &[u8],
            },
        };

        let payload = cbm.serialize_coinbase_payload(&data).unwrap();
        let deserialized_data = cbm.deserialize_coinbase_payload(&payload).unwrap();

        assert_eq!(data, deserialized_data);

        // Test an actual mainnet payload
        let payload_hex =
            "b612c90100000000041a763e07000000000022202b32443ff740012157716d81216d09aebc39e5493c93a7181d92cb756c02c560ac302e31322e382f";
        let mut payload = vec![0u8; payload_hex.len() / 2];
        faster_hex::hex_decode(payload_hex.as_bytes(), &mut payload).unwrap();
        let deserialized_data = cbm.deserialize_coinbase_payload(&payload).unwrap();

        let expected_data = CoinbaseData {
            blue_score: 29954742,
            subsidy: 31112698372,
            miner_data: MinerData {
                script_public_key: ScriptPublicKey::new(
                    0,
                    scriptvec![
                        32, 43, 50, 68, 63, 247, 64, 1, 33, 87, 113, 109, 129, 33, 109, 9, 174, 188, 57, 229, 73, 60, 147, 167, 24,
                        29, 146, 203, 117, 108, 2, 197, 96, 172,
                    ],
                ),
                extra_data: &[48u8, 46, 49, 50, 46, 56, 47] as &[u8],
            },
        };
        assert_eq!(expected_data, deserialized_data);
    }

    #[test]
    fn modify_payload_test() {
        let cbm = create_manager(&MAINNET_PARAMS);

        let script_data = [33u8, 255];
        let extra_data = [2u8, 3, 23, 98];
        let data = CoinbaseData {
            blue_score: 56345,
            subsidy: 44000000000,
            miner_data: MinerData {
                script_public_key: ScriptPublicKey::new(0, ScriptVec::from_slice(&script_data)),
                extra_data: &extra_data,
            },
        };

        let data2 = CoinbaseData {
            blue_score: data.blue_score,
            subsidy: data.subsidy,
            miner_data: MinerData {
                // Modify only miner data
                script_public_key: ScriptPublicKey::new(0, ScriptVec::from_slice(&[33u8, 255, 33])),
                extra_data: &[2u8, 3, 23, 98, 34, 34] as &[u8],
            },
        };

        let mut payload = cbm.serialize_coinbase_payload(&data).unwrap();
        payload = cbm.modify_coinbase_payload(payload, &data2.miner_data).unwrap(); // Update the payload with the modified miner data
        let deserialized_data = cbm.deserialize_coinbase_payload(&payload).unwrap();

        assert_eq!(data2, deserialized_data);
    }

    fn create_manager(params: &Params) -> CoinbaseManager {
        CoinbaseManager::new(
            params.coinbase_payload_script_public_key_max_len,
            params.max_coinbase_payload_len,
            params.deflationary_phase_daa_score,
            params.pre_deflationary_phase_base_subsidy,
            params.bps(),
        )
    }

    /// Return a CoinbaseManager with legacy golang 1 BPS properties
    fn create_legacy_manager() -> CoinbaseManager {
        CoinbaseManager::new(150, 204, 15778800 - 259200, 50000000000, ForkedParam::new_const(1))
    }
}
