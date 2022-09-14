use std::convert::TryInto;

use consensus_core::tx::{ScriptPublicKey, Transaction};

const UINT64_LEN: usize = 8;
const UINT16_LEN: usize = 2;
const LENGTH_OF_SUBSIDY: usize = UINT64_LEN;
const LENGTH_OF_SCRIPT_PUB_KEY_LENGTH: usize = 1;
const LENGTH_OF_VERSION_SCRIPT_PUB_KEY: usize = UINT16_LEN;

use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum CoinbaseError {
    #[error("coinbase payload length is {0} while the minimum allowed length is {1}")]
    PayloadLenBelowMin(usize, usize),

    #[error("coinbase payload length is {0} while the maximum allowed length is {1}")]
    PayloadLenAboveMax(usize, usize),

    #[error("coinbase payload script public key length is {0} while the maximum allowed length is {1}")]
    PayloadScriptPublicKeyLenAboveMax(u8, u8),

    #[error("coinbase payload script public key length is {0} while the maximum allowed length is {1}")]
    PayloadScriptPublicKeyLen(u8, u8),

    #[error("coinbase payload length is {0} bytes but it needs to be at least {1} bytes long in order of accomodating the script public key")]
    PayloadCantContainScriptPublicKey(usize, usize),
}

pub type CoinbaseResult<T> = std::result::Result<T, CoinbaseError>;

#[derive(Clone)]
pub struct CoinbaseManager {
    coinbase_payload_script_public_key_max_len: u8,
    max_coinbase_payload_len: usize,
    deflationary_phase_daa_score: u64,
    pre_deflationary_phase_base_subsidy: u64,
}

pub struct CoinbaseData<'a> {
    pub blue_score: u64,
    pub subsidy: u64,
    pub miner_data: MinerData<'a>,
}

pub struct MinerData<'a> {
    pub script_public_key: ScriptPublicKey,
    pub extra_data: &'a [u8],
}

impl CoinbaseManager {
    pub fn new(
        coinbase_payload_script_public_key_max_len: u8, max_coinbase_payload_len: usize,
        deflationary_phase_daa_score: u64, pre_deflationary_phase_base_subsidy: u64,
    ) -> Self {
        Self {
            coinbase_payload_script_public_key_max_len,
            max_coinbase_payload_len,
            deflationary_phase_daa_score,
            pre_deflationary_phase_base_subsidy,
        }
    }

    pub fn validate_coinbase_payload_in_isolation_and_extract_coinbase_data<'a>(
        &self, coinbase: &'a Transaction,
    ) -> CoinbaseResult<CoinbaseData<'a>> {
        let payload = &coinbase.payload;
        const MIN_LEN: usize =
            UINT64_LEN + LENGTH_OF_SUBSIDY + LENGTH_OF_VERSION_SCRIPT_PUB_KEY + LENGTH_OF_SCRIPT_PUB_KEY_LENGTH;

        if payload.len() < MIN_LEN {
            return Err(CoinbaseError::PayloadLenBelowMin(coinbase.payload.len(), MIN_LEN));
        }

        if payload.len() > self.max_coinbase_payload_len {
            return Err(CoinbaseError::PayloadLenAboveMax(coinbase.payload.len(), self.max_coinbase_payload_len));
        }

        let blue_score = u64::from_le_bytes(payload[..UINT64_LEN].try_into().unwrap());
        let subsidy = u64::from_le_bytes(
            payload[UINT64_LEN..UINT64_LEN + LENGTH_OF_SUBSIDY]
                .try_into()
                .unwrap(),
        );

        // Because LENGTH_OF_VERSION_SCRIPT_PUB_KEY is two bytes and script_pub_key_version reads only one byte, there's one byte
        // in the middle where the miner can write any arbitrary data. This means the code cannot support script pub key version
        // higher than 255. This can be fixed only via a soft-fork.
        let script_pub_key_version = payload[UINT64_LEN + LENGTH_OF_SUBSIDY] as u16;
        let script_pub_key_len = payload[UINT64_LEN + LENGTH_OF_SUBSIDY + LENGTH_OF_VERSION_SCRIPT_PUB_KEY];
        if script_pub_key_len > self.coinbase_payload_script_public_key_max_len {
            return Err(CoinbaseError::PayloadScriptPublicKeyLenAboveMax(
                script_pub_key_len,
                self.coinbase_payload_script_public_key_max_len,
            ));
        }

        if payload.len() < MIN_LEN + script_pub_key_len as usize {
            return Err(CoinbaseError::PayloadCantContainScriptPublicKey(payload.len(), script_pub_key_len as usize));
        }

        let script_pub_key_script = &payload[UINT64_LEN
            + LENGTH_OF_SUBSIDY
            + LENGTH_OF_VERSION_SCRIPT_PUB_KEY
            + LENGTH_OF_SCRIPT_PUB_KEY_LENGTH
            ..UINT64_LEN
                + LENGTH_OF_SUBSIDY
                + LENGTH_OF_VERSION_SCRIPT_PUB_KEY
                + LENGTH_OF_SCRIPT_PUB_KEY_LENGTH
                + script_pub_key_len as usize];

        let extra_data = &payload[UINT64_LEN
            + LENGTH_OF_SUBSIDY
            + LENGTH_OF_VERSION_SCRIPT_PUB_KEY
            + LENGTH_OF_SCRIPT_PUB_KEY_LENGTH
            + script_pub_key_len as usize..];

        Ok(CoinbaseData {
            blue_score,
            subsidy,
            miner_data: MinerData {
                script_public_key: ScriptPublicKey {
                    script: script_pub_key_script.to_owned(),
                    version: script_pub_key_version,
                },
                extra_data,
            },
        })
    }

    pub fn calc_block_subsidy(&self, daa_score: u64) -> u64 {
        if daa_score < self.deflationary_phase_daa_score {
            return self.pre_deflationary_phase_base_subsidy;
        }

        // We define a year as 365.25 days and a month as 365.25 / 12 = 30.4375
        // SECONDS_PER_MONTH = 30.4375 * 24 * 60 * 60
        const SECONDS_PER_MONTH: u64 = 2629800;

        // Note that this calculation implicitly assumes that block per second = 1 (by assuming daa score diff is in second units).
        let months_since_deflationary_phase_started =
            (daa_score - self.deflationary_phase_daa_score) / SECONDS_PER_MONTH;
        assert!(months_since_deflationary_phase_started <= usize::MAX as u64);
        let months_since_deflationary_phase_started: usize = months_since_deflationary_phase_started as usize;
        if months_since_deflationary_phase_started >= SUBSIDY_BY_MONTH_TABLE.len() {
            *SUBSIDY_BY_MONTH_TABLE.last().unwrap()
        } else {
            SUBSIDY_BY_MONTH_TABLE[months_since_deflationary_phase_started as usize]
        }
    }
}

/*
    This table was pre-calculated by calling `calcDeflationaryPeriodBlockSubsidyFloatCalc` for all months until reaching 0 subsidy.
    To regenerate this table, run `TestBuildSubsidyTable` in coinbasemanager_test.go (note the `deflationaryPhaseBaseSubsidy` therein)
*/
const SUBSIDY_BY_MONTH_TABLE: [u64; 426] = [
    44000000000,
    41530469757,
    39199543598,
    36999442271,
    34922823143,
    32962755691,
    31112698372,
    29366476791,
    27718263097,
    26162556530,
    24694165062,
    23308188075,
    22000000000,
    20765234878,
    19599771799,
    18499721135,
    17461411571,
    16481377845,
    15556349186,
    14683238395,
    13859131548,
    13081278265,
    12347082531,
    11654094037,
    11000000000,
    10382617439,
    9799885899,
    9249860567,
    8730705785,
    8240688922,
    7778174593,
    7341619197,
    6929565774,
    6540639132,
    6173541265,
    5827047018,
    5500000000,
    5191308719,
    4899942949,
    4624930283,
    4365352892,
    4120344461,
    3889087296,
    3670809598,
    3464782887,
    3270319566,
    3086770632,
    2913523509,
    2750000000,
    2595654359,
    2449971474,
    2312465141,
    2182676446,
    2060172230,
    1944543648,
    1835404799,
    1732391443,
    1635159783,
    1543385316,
    1456761754,
    1375000000,
    1297827179,
    1224985737,
    1156232570,
    1091338223,
    1030086115,
    972271824,
    917702399,
    866195721,
    817579891,
    771692658,
    728380877,
    687500000,
    648913589,
    612492868,
    578116285,
    545669111,
    515043057,
    486135912,
    458851199,
    433097860,
    408789945,
    385846329,
    364190438,
    343750000,
    324456794,
    306246434,
    289058142,
    272834555,
    257521528,
    243067956,
    229425599,
    216548930,
    204394972,
    192923164,
    182095219,
    171875000,
    162228397,
    153123217,
    144529071,
    136417277,
    128760764,
    121533978,
    114712799,
    108274465,
    102197486,
    96461582,
    91047609,
    85937500,
    81114198,
    76561608,
    72264535,
    68208638,
    64380382,
    60766989,
    57356399,
    54137232,
    51098743,
    48230791,
    45523804,
    42968750,
    40557099,
    38280804,
    36132267,
    34104319,
    32190191,
    30383494,
    28678199,
    27068616,
    25549371,
    24115395,
    22761902,
    21484375,
    20278549,
    19140402,
    18066133,
    17052159,
    16095095,
    15191747,
    14339099,
    13534308,
    12774685,
    12057697,
    11380951,
    10742187,
    10139274,
    9570201,
    9033066,
    8526079,
    8047547,
    7595873,
    7169549,
    6767154,
    6387342,
    6028848,
    5690475,
    5371093,
    5069637,
    4785100,
    4516533,
    4263039,
    4023773,
    3797936,
    3584774,
    3383577,
    3193671,
    3014424,
    2845237,
    2685546,
    2534818,
    2392550,
    2258266,
    2131519,
    2011886,
    1898968,
    1792387,
    1691788,
    1596835,
    1507212,
    1422618,
    1342773,
    1267409,
    1196275,
    1129133,
    1065759,
    1005943,
    949484,
    896193,
    845894,
    798417,
    753606,
    711309,
    671386,
    633704,
    598137,
    564566,
    532879,
    502971,
    474742,
    448096,
    422947,
    399208,
    376803,
    355654,
    335693,
    316852,
    299068,
    282283,
    266439,
    251485,
    237371,
    224048,
    211473,
    199604,
    188401,
    177827,
    167846,
    158426,
    149534,
    141141,
    133219,
    125742,
    118685,
    112024,
    105736,
    99802,
    94200,
    88913,
    83923,
    79213,
    74767,
    70570,
    66609,
    62871,
    59342,
    56012,
    52868,
    49901,
    47100,
    44456,
    41961,
    39606,
    37383,
    35285,
    33304,
    31435,
    29671,
    28006,
    26434,
    24950,
    23550,
    22228,
    20980,
    19803,
    18691,
    17642,
    16652,
    15717,
    14835,
    14003,
    13217,
    12475,
    11775,
    11114,
    10490,
    9901,
    9345,
    8821,
    8326,
    7858,
    7417,
    7001,
    6608,
    6237,
    5887,
    5557,
    5245,
    4950,
    4672,
    4410,
    4163,
    3929,
    3708,
    3500,
    3304,
    3118,
    2943,
    2778,
    2622,
    2475,
    2336,
    2205,
    2081,
    1964,
    1854,
    1750,
    1652,
    1559,
    1471,
    1389,
    1311,
    1237,
    1168,
    1102,
    1040,
    982,
    927,
    875,
    826,
    779,
    735,
    694,
    655,
    618,
    584,
    551,
    520,
    491,
    463,
    437,
    413,
    389,
    367,
    347,
    327,
    309,
    292,
    275,
    260,
    245,
    231,
    218,
    206,
    194,
    183,
    173,
    163,
    154,
    146,
    137,
    130,
    122,
    115,
    109,
    103,
    97,
    91,
    86,
    81,
    77,
    73,
    68,
    65,
    61,
    57,
    54,
    51,
    48,
    45,
    43,
    40,
    38,
    36,
    34,
    32,
    30,
    28,
    27,
    25,
    24,
    22,
    21,
    20,
    19,
    18,
    17,
    16,
    15,
    14,
    13,
    12,
    12,
    11,
    10,
    10,
    9,
    9,
    8,
    8,
    7,
    7,
    6,
    6,
    6,
    5,
    5,
    5,
    4,
    4,
    4,
    4,
    3,
    3,
    3,
    3,
    3,
    2,
    2,
    2,
    2,
    2,
    2,
    2,
    1,
    1,
    1,
    1,
    1,
    1,
    1,
    1,
    1,
    1,
    1,
    1,
    0,
];

#[cfg(test)]
mod tests {
    use crate::params::MAINNET_PARAMS;

    use super::CoinbaseManager;

    #[test]
    fn subsidy_test() {
        let params = &MAINNET_PARAMS;
        let cbm = CoinbaseManager::new(
            params.coinbase_payload_script_public_key_max_len,
            params.max_coinbase_payload_len,
            params.deflationary_phase_daa_score,
            params.pre_deflationary_phase_base_subsidy,
        );

        const DEFLATIONARY_PHASE_INITIAL_SUBSIDY: u64 = 44000000000;
        const SECONDS_PER_MONTH: u64 = 2629800;
        const SECONDS_PER_HALVING: u64 = SECONDS_PER_MONTH * 12;

        struct Test {
            name: &'static str,
            daa_score: u64,
            expected: u64,
        }

        let tests = vec![
            Test {
                name: "before deflationary phase",
                daa_score: params.deflationary_phase_daa_score - 1,
                expected: params.pre_deflationary_phase_base_subsidy,
            },
            Test {
                name: "start of deflationary phase",
                daa_score: params.deflationary_phase_daa_score,
                expected: DEFLATIONARY_PHASE_INITIAL_SUBSIDY,
            },
            Test {
                name: "after one halving",
                daa_score: params.deflationary_phase_daa_score + SECONDS_PER_HALVING,
                expected: DEFLATIONARY_PHASE_INITIAL_SUBSIDY / 2,
            },
            Test {
                name: "after 2 halvings",
                daa_score: params.deflationary_phase_daa_score + 2 * SECONDS_PER_HALVING,
                expected: DEFLATIONARY_PHASE_INITIAL_SUBSIDY / 4,
            },
            Test {
                name: "after 5 halvings",
                daa_score: params.deflationary_phase_daa_score + 5 * SECONDS_PER_HALVING,
                expected: DEFLATIONARY_PHASE_INITIAL_SUBSIDY / 32,
            },
            Test {
                name: "after 32 halvings",
                daa_score: params.deflationary_phase_daa_score + 32 * SECONDS_PER_HALVING,
                expected: DEFLATIONARY_PHASE_INITIAL_SUBSIDY / 4294967296,
            },
            Test {
                name: "just before subsidy depleted",
                daa_score: params.deflationary_phase_daa_score + 35 * SECONDS_PER_HALVING,
                expected: 1,
            },
            Test {
                name: "after subsidy depleted",
                daa_score: params.deflationary_phase_daa_score + 36 * SECONDS_PER_HALVING,
                expected: 0,
            },
        ];

        for t in tests {
            assert_eq!(cbm.calc_block_subsidy(t.daa_score), t.expected, "test '{}' failed", t.name);
        }
    }
}
