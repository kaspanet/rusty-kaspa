use crate::mempool::{
    Mempool,
    errors::{NonStandardError, NonStandardResult},
};
use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
use kaspa_consensus_core::{
    constants::{MAX_SCRIPT_PUBLIC_KEY_VERSION, MAX_SOMPI},
    mass::{self, NonContextualMasses},
    tx::{MutableTransaction, PopulatedTransaction, TransactionOutput},
};
use kaspa_txscript::{get_sig_op_count_upper_bound, script_class::ScriptClass};

/// MAX_STANDARD_P2SH_SIG_OPS is the maximum number of signature operations
/// that are considered standard in a pay-to-script-hash script.
///
/// The upper-bound execution limit comes from compute mass: some zk opcodes already cost the equivalent
/// of roughly 140-250 signature operations. However, for classic Schnorr/ECDSA signature operations, this
/// standardness limit encourages parallelism across inputs rather than concentrating work in one input.
/// It is also at least as permissive as the previous standard compute-mass limit of 100k,
/// which allowed at most 100 sigops since each sigop costs 1000 grams.
const MAX_STANDARD_P2SH_SIG_OPS: u16 = 100;

impl Mempool {
    pub(crate) fn check_transaction_standard_in_isolation(&self, transaction: &MutableTransaction) -> NonStandardResult<()> {
        let transaction_id = transaction.id();

        // None of the output public key scripts can be a non-standard script or be "dust".
        for (i, output) in transaction.tx.outputs.iter().enumerate() {
            if output.script_public_key.version() > MAX_SCRIPT_PUBLIC_KEY_VERSION {
                return Err(NonStandardError::RejectScriptPublicKeyVersion(transaction_id, i));
            }

            if ScriptClass::from_script(&output.script_public_key) == ScriptClass::NonStandard {
                return Err(NonStandardError::RejectOutputScriptClass(transaction_id, i));
            }

            if self.is_transaction_output_dust(output) {
                return Err(NonStandardError::RejectDust(transaction_id, i, output.value));
            }
        }

        Ok(())
    }

    /// is_transaction_output_dust returns whether or not the passed transaction output
    /// amount is considered dust or not based on the configured minimum transaction
    /// relay fee.
    ///
    /// Dust is defined in terms of the minimum transaction relay fee. In particular,
    /// if the cost to the network to spend coins is more than 1/3 of the minimum
    /// transaction relay fee, it is considered dust.
    ///
    /// It is exposed by [MiningManager] for use by transaction generators and wallets.
    pub(crate) fn is_transaction_output_dust(&self, transaction_output: &TransactionOutput) -> bool {
        // The total serialized size consists of the output and the associated
        // input script to redeem it. Since there is no input script
        // to redeem it yet, use the minimum size of a typical input script.
        //
        // Pay-to-pubkey bytes breakdown:
        //
        //  Output to pubkey (43 bytes):
        //   8 value, 1 script len, 34 script [1 OP_DATA_32,
        //   32 pubkey, 1 OP_CHECKSIG]
        //
        //  Input (105 bytes):
        //   36 prev outpoint, 1 script len, 64 script [1 OP_DATA_64,
        //   64 sig], 4 sequence
        //
        // The most common scripts are pay-to-pubkey, and as per the above
        // breakdown, the minimum size of a p2pk input script is 148 bytes. So
        // that figure is used.
        let total_serialized_size = mass::transaction_output_estimated_serialized_size(transaction_output) + 148;

        // The output is considered dust if the cost to the network to spend the
        // coins is more than 1/3 of the minimum free transaction relay fee.
        // mp.config.MinimumRelayTransactionFee is in sompi/KB, so multiply
        // by 1000 to convert to bytes.
        //
        // Using the typical values for a pay-to-pubkey transaction from
        // the breakdown above and the default minimum free transaction relay
        // fee of 1000, this equates to values less than 546 sompi being
        // considered dust.
        //
        // The following is equivalent to (value/total_serialized_size) * (1/3) * 1000
        // without needing to do floating point math.
        //
        // Since the multiplication may overflow a u64, 2 separate calculation paths
        // are considered to avoid overflowing.
        match transaction_output.value.checked_mul(1000) {
            Some(value_1000) => value_1000 / (3 * total_serialized_size) < self.config.minimum_relay_transaction_fee,
            None => {
                (transaction_output.value as u128 * 1000 / (3 * total_serialized_size as u128))
                    < self.config.minimum_relay_transaction_fee as u128
            }
        }
    }

    /// check_transaction_standard_in_context performs a series of checks on a transaction's
    /// inputs to ensure they are "standard". A standard transaction input within the
    /// context of this function is one whose referenced public key script is of a
    /// standard form and, for pay-to-script-hash, does not have more than
    /// maxStandardP2SHSigOps signature operations.
    /// In addition, makes sure that the transaction's fee is above the minimum for acceptance
    /// into the mempool and relay.
    pub(crate) fn check_transaction_standard_in_context(&self, transaction: &MutableTransaction) -> NonStandardResult<()> {
        let transaction_id = transaction.id();
        for (i, input) in transaction.tx.inputs.iter().enumerate() {
            // It is safe to elide existence and index checks here since
            // they have already been checked prior to calling this
            // function.
            let entry = transaction.entries[i].as_ref().unwrap();
            match ScriptClass::from_script(&entry.script_public_key) {
                ScriptClass::NonStandard => {
                    return Err(NonStandardError::RejectInputScriptClass(transaction_id, i));
                }
                ScriptClass::PubKey => {}
                ScriptClass::PubKeyECDSA => {}
                ScriptClass::ScriptHash => {
                    // TODO: relax due to on the fly sigop calculation
                    // Possible options:
                    //      1. remove all together and rely on compute mass limits
                    //      2. extract an upper bound on the committed value from input.mass and min
                    //         with the static count (relying on validation to fail if the commitment is wrong)
                    let num_sig_ops = get_sig_op_count_upper_bound::<PopulatedTransaction, SigHashReusedValuesUnsync>(
                        &input.signature_script,
                        &entry.script_public_key,
                    );
                    if num_sig_ops > MAX_STANDARD_P2SH_SIG_OPS as u64 {
                        return Err(NonStandardError::RejectSignatureCount(transaction_id, i, num_sig_ops, MAX_STANDARD_P2SH_SIG_OPS));
                    }
                }
            }
        }

        // Minimum relay fee applies to non-contextual mass so block-space usage has a minimum cost,
        // including transient mass. This is especially important after increasing the transient mass limit.
        // Storage mass does not require an additional relay-fee floor here since storage growth is
        // sufficiently protected even under worst-case block-limit usage.
        let NonContextualMasses { compute_mass, transient_mass } = transaction.calculated_non_contextual_masses.unwrap();
        let minimum_fee = self.minimum_required_transaction_relay_fee(compute_mass.max(transient_mass));
        if transaction.calculated_fee.unwrap() < minimum_fee {
            return Err(NonStandardError::RejectInsufficientFee(transaction_id, transaction.calculated_fee.unwrap(), minimum_fee));
        }

        Ok(())
    }

    /// minimum_required_transaction_relay_fee returns the minimum transaction fee required
    /// for a transaction with the passed mass to be accepted into the mempool and relayed.
    fn minimum_required_transaction_relay_fee(&self, mass: u64) -> u64 {
        // Calculate the minimum fee for a transaction to be allowed into the
        // mempool and relayed by scaling the base fee. MinimumRelayTransactionFee is in
        // sompi/kg so multiply by mass (which is in grams) and divide by 1000 to get
        // minimum sompis.
        let mut minimum_fee = (mass * self.config.minimum_relay_transaction_fee) / 1000;

        if minimum_fee == 0 {
            minimum_fee = self.config.minimum_relay_transaction_fee;
        }

        // Set the minimum fee to the maximum possible value if the calculated
        // fee is not in the valid range for monetary amounts.
        minimum_fee = minimum_fee.min(MAX_SOMPI);

        minimum_fee
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MiningCounters,
        mempool::config::{Config, DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE},
    };
    use kaspa_addresses::{Address, Prefix, Version};
    use kaspa_consensus_core::{
        config::params::Params,
        constants::{MAX_TX_IN_SEQUENCE_NUM, SOMPI_PER_KASPA, TX_VERSION},
        mass::NonContextualMasses,
        network::NetworkType,
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
    };
    use kaspa_txscript::{
        opcodes::codes::{OpReturn, OpTrue},
        script_builder::ScriptBuilder,
    };
    use smallvec::smallvec;
    use std::sync::Arc;

    const RELAY_FEE_TEST_MASS: u64 = 500_000;

    #[test]
    fn test_calc_min_required_tx_relay_fee() {
        struct Test {
            name: &'static str,
            size: u64,
            minimum_relay_transaction_fee: u64,
            want: u64,
        }

        let tests = [
            Test {
                // Ensure combination of size and fee that are less than 1000
                // produce a non-zero fee.
                name: "250 bytes with relay fee of 3",
                size: 250,
                minimum_relay_transaction_fee: 3,
                want: 3,
            },
            Test {
                name: "100 bytes with default minimum relay fee",
                size: 100,
                minimum_relay_transaction_fee: DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE,
                want: 100,
            },
            Test {
                name: "large relay fee test mass with default minimum relay fee",
                size: RELAY_FEE_TEST_MASS,
                minimum_relay_transaction_fee: DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE,
                want: RELAY_FEE_TEST_MASS,
            },
            Test { name: "1500 bytes with 5000 relay fee", size: 1500, minimum_relay_transaction_fee: 5000, want: 7500 },
            Test { name: "1500 bytes with 3000 relay fee", size: 1500, minimum_relay_transaction_fee: 3000, want: 4500 },
            Test { name: "782 bytes with 5000 relay fee", size: 782, minimum_relay_transaction_fee: 5000, want: 3910 },
            Test { name: "782 bytes with 3000 relay fee", size: 782, minimum_relay_transaction_fee: 3000, want: 2346 },
            Test { name: "782 bytes with 2550 relay fee", size: 782, minimum_relay_transaction_fee: 2550, want: 1994 },
        ];

        for test in tests.iter() {
            for net in NetworkType::iter() {
                let params: Params = net.into();
                let mut config = Config::build_default(
                    params.target_time_per_block(),
                    false,
                    params.mempool_block_mass_limits(),
                    params.block_lane_limits,
                );
                config.minimum_relay_transaction_fee = test.minimum_relay_transaction_fee;
                let counters = Arc::new(MiningCounters::default());
                let mempool = Mempool::new(Arc::new(config), counters);

                let got = mempool.minimum_required_transaction_relay_fee(test.size);
                if got != test.want {
                    println!("test_calc_min_required_tx_relay_fee test '{}' failed: got {}, want {}", test.name, got, test.want);
                }
                assert_eq!(test.want, got);
            }
        }
    }

    #[test]
    fn test_is_transaction_output_dust() {
        let script_public_key = ScriptPublicKey::new(
            0,
            smallvec![
                0x76, 0xa9, 0x21, 0x03, 0x2f, 0x7e, 0x43, 0x0a, 0xa4, 0xc9, 0xd1, 0x59, 0x43, 0x7e, 0x84, 0xb9, 0x75, 0xdc, 0x76,
                0xd9, 0x00, 0x3b, 0xf0, 0x92, 0x2c, 0xf3, 0xaa, 0x45, 0x28, 0x46, 0x4b, 0xab, 0x78, 0x0d, 0xba, 0x5e
            ],
        );
        struct Test {
            name: &'static str,
            tx_out: TransactionOutput,
            minimum_relay_transaction_fee: u64,
            is_dust: bool,
        }

        let tests = vec![
            // Any value is allowed with a zero relay fee.
            Test {
                name: "zero value with zero relay fee",
                tx_out: TransactionOutput::new(0, script_public_key.clone()),
                minimum_relay_transaction_fee: 0,
                is_dust: false,
            },
            // Zero value is dust with any relay fee"
            Test {
                name: "zero value with very small tx fee",
                tx_out: TransactionOutput::new(0, script_public_key.clone()),
                minimum_relay_transaction_fee: 1,
                is_dust: true,
            },
            Test {
                name: "36 byte public key script with value 605",
                tx_out: TransactionOutput::new(605, script_public_key.clone()),
                minimum_relay_transaction_fee: 1000,
                is_dust: true,
            },
            Test {
                name: "36 byte public key script with value 606",
                tx_out: TransactionOutput::new(606, script_public_key.clone()),
                minimum_relay_transaction_fee: 1000,
                is_dust: false,
            },
            // Maximum allowed value is never dust.
            Test {
                name: "max sompi amount is never dust",
                tx_out: TransactionOutput::new(MAX_SOMPI, script_public_key.clone()),
                minimum_relay_transaction_fee: 1000,
                is_dust: false,
            },
            // Maximum uint64 value causes NO overflow.
            // Rust rewrite: caution, this differs from the golang version
            Test {
                name: "maximum uint64 value",
                tx_out: TransactionOutput::new(u64::MAX, script_public_key),
                minimum_relay_transaction_fee: u64::MAX,
                is_dust: false,
            },
        ];
        for test in tests {
            for net in NetworkType::iter() {
                let params: Params = net.into();
                let mut config = Config::build_default(
                    params.target_time_per_block(),
                    false,
                    params.mempool_block_mass_limits(),
                    params.block_lane_limits,
                );
                config.minimum_relay_transaction_fee = test.minimum_relay_transaction_fee;
                let counters = Arc::new(MiningCounters::default());
                let mempool = Mempool::new(Arc::new(config), counters);

                println!("test_is_transaction_output_dust test '{}' ", test.name);
                let res = mempool.is_transaction_output_dust(&test.tx_out);
                if res != test.is_dust {
                    println!("test_is_transaction_output_dust test '{}' failed: got {}, want {}", test.name, res, test.is_dust);
                }
                assert_eq!(test.is_dust, res);
            }
        }
    }

    #[test]
    fn test_check_transaction_standard_in_isolation() {
        // Create some dummy, but otherwise standard, data for transactions.
        let dummy_prev_out = TransactionOutpoint::new(kaspa_hashes::Hash::from_u64_word(1), 1);
        let dummy_sig_script = vec![0u8; 65];
        let dummy_tx_input = TransactionInput::new(dummy_prev_out, dummy_sig_script, MAX_TX_IN_SEQUENCE_NUM, 1);
        let addr_hash = vec![1u8; 32];

        let addr = Address::new(Prefix::Testnet, Version::PubKey, &addr_hash);
        let dummy_script_public_key = kaspa_txscript::pay_to_address_script(&addr);
        let dummy_tx_out = TransactionOutput::new(SOMPI_PER_KASPA, dummy_script_public_key);

        struct Test {
            name: &'static str,
            mtx: MutableTransaction,
            is_standard: bool,
        }

        fn new_mtx(tx: Transaction, mass: u64) -> MutableTransaction {
            let mut mtx = MutableTransaction::from_tx(tx);
            mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(mass, mass));
            mtx
        }

        let tests = vec![
            Test {
                name: "Typical pay-to-pubkey transaction",
                mtx: new_mtx(
                    Transaction::new(
                        TX_VERSION,
                        vec![dummy_tx_input.clone()],
                        vec![dummy_tx_out.clone()],
                        0,
                        SUBNETWORK_ID_NATIVE,
                        0,
                        vec![],
                    ),
                    1000,
                ),
                is_standard: true,
            },
            Test {
                name: "Transaction version above allowed",
                mtx: new_mtx(
                    Transaction::new(
                        TX_VERSION + 2,
                        vec![dummy_tx_input.clone()],
                        vec![dummy_tx_out.clone()],
                        0,
                        SUBNETWORK_ID_NATIVE,
                        0,
                        vec![],
                    ),
                    1000,
                ),
                is_standard: true, // check_transaction_standard_in_isolation does not check version
            },
            Test {
                name: "Valid but non standard public key script",
                mtx: new_mtx(
                    Transaction::new(
                        TX_VERSION,
                        vec![dummy_tx_input.clone()],
                        vec![TransactionOutput::new(
                            SOMPI_PER_KASPA,
                            ScriptPublicKey::new(
                                MAX_SCRIPT_PUBLIC_KEY_VERSION,
                                ScriptBuilder::new().add_op(OpTrue).unwrap().script().into(),
                            ),
                        )],
                        0,
                        SUBNETWORK_ID_NATIVE,
                        0,
                        vec![],
                    ),
                    1000,
                ),
                is_standard: false,
            },
            Test {
                name: "Dust output",
                mtx: new_mtx(
                    Transaction::new(
                        TX_VERSION,
                        vec![dummy_tx_input.clone()],
                        vec![TransactionOutput::new(0, dummy_tx_out.script_public_key)],
                        0,
                        SUBNETWORK_ID_NATIVE,
                        0,
                        vec![],
                    ),
                    1000,
                ),
                is_standard: false,
            },
            Test {
                name: "Null-data transaction",
                mtx: new_mtx(
                    Transaction::new(
                        TX_VERSION,
                        vec![dummy_tx_input],
                        vec![TransactionOutput::new(
                            SOMPI_PER_KASPA,
                            ScriptPublicKey::new(
                                MAX_SCRIPT_PUBLIC_KEY_VERSION,
                                ScriptBuilder::new().add_op(OpReturn).unwrap().script().into(),
                            ),
                        )],
                        0,
                        SUBNETWORK_ID_NATIVE,
                        0,
                        vec![],
                    ),
                    1000,
                ),
                is_standard: false,
            },
        ];

        for test in tests {
            for net in NetworkType::iter() {
                let params: Params = net.into();
                let config = Config::build_default(
                    params.target_time_per_block(),
                    false,
                    params.mempool_block_mass_limits(),
                    params.block_lane_limits,
                );
                let counters = Arc::new(MiningCounters::default());
                let mempool = Mempool::new(Arc::new(config), counters);

                // Ensure standard-ness is as expected.
                println!("test_check_transaction_standard_in_isolation test '{}' ", test.name);
                let res = mempool.check_transaction_standard_in_isolation(&test.mtx);
                if res.is_ok() && test.is_standard {
                    // Test passes since function returned standard for a
                    // transaction which is intended to be standard.
                    continue;
                }
                if res.is_ok() && !test.is_standard {
                    println!("test_check_transaction_standard_in_isolation ({}): standard when it should not be", test.name);
                }
                if res.is_err() && test.is_standard {
                    println!(
                        "test_check_transaction_standard_in_isolation ({}): nonstandard when it should not be: {:?}",
                        test.name, res
                    );
                }
                assert_eq!(res.is_ok(), test.is_standard, "failed for test '{}': {:?}", test.name, res);
            }
        }
    }

    #[test]
    fn test_check_transaction_standard_in_context() {
        let addr = Address::new(Prefix::Testnet, Version::PubKey, &[1u8; 32]);
        let standard_script_public_key = kaspa_txscript::pay_to_address_script(&addr);
        let non_standard_script_public_key =
            ScriptPublicKey::new(MAX_SCRIPT_PUBLIC_KEY_VERSION, ScriptBuilder::new().add_op(OpTrue).unwrap().script().into());

        enum Expected {
            Standard,
            RejectInputScriptClass,
            RejectInsufficientFee { fee: u64, minimum_fee: u64 },
        }

        struct Test {
            name: &'static str,
            mtx: MutableTransaction,
            expected: Expected,
        }

        fn new_mtx(script_public_key: ScriptPublicKey, masses: NonContextualMasses, fee: u64) -> MutableTransaction {
            let prev_out = TransactionOutpoint::new(kaspa_hashes::Hash::from_u64_word(1), 1);
            let input = TransactionInput::new(prev_out, vec![], MAX_TX_IN_SEQUENCE_NUM, 1);
            let tx = Transaction::new(
                TX_VERSION,
                vec![input],
                vec![TransactionOutput::new(SOMPI_PER_KASPA, script_public_key.clone())],
                0,
                SUBNETWORK_ID_NATIVE,
                0,
                vec![],
            );
            let mut mtx = MutableTransaction::with_entries(
                tx.into(),
                vec![UtxoEntry::new(2 * SOMPI_PER_KASPA, script_public_key, 0, false, None)],
            );
            mtx.calculated_non_contextual_masses = Some(masses);
            mtx.calculated_fee = Some(fee);
            mtx
        }

        let tests = vec![
            Test {
                name: "standard input with sufficient fee",
                mtx: new_mtx(standard_script_public_key.clone(), NonContextualMasses::new(1_000, 500), 1_000),
                expected: Expected::Standard,
            },
            Test {
                name: "non-standard input script class",
                mtx: new_mtx(non_standard_script_public_key, NonContextualMasses::new(1_000, 1_000), 1_000),
                expected: Expected::RejectInputScriptClass,
            },
            Test {
                name: "compute mass triggers insufficient relay fee",
                mtx: new_mtx(standard_script_public_key.clone(), NonContextualMasses::new(10_000, 1), 9_999),
                expected: Expected::RejectInsufficientFee { fee: 9_999, minimum_fee: 10_000 },
            },
            Test {
                name: "transient mass triggers insufficient relay fee",
                mtx: new_mtx(standard_script_public_key, NonContextualMasses::new(1, 10_000), 9_999),
                expected: Expected::RejectInsufficientFee { fee: 9_999, minimum_fee: 10_000 },
            },
        ];

        let params: Params = NetworkType::Simnet.into();
        let config =
            Config::build_default(params.target_time_per_block(), false, params.mempool_block_mass_limits(), params.block_lane_limits);
        let counters = Arc::new(MiningCounters::default());
        let mempool = Mempool::new(Arc::new(config), counters);

        for test in tests {
            let res = mempool.check_transaction_standard_in_context(&test.mtx);
            match test.expected {
                Expected::Standard => assert_eq!(res, Ok(()), "failed for test '{}'", test.name),
                Expected::RejectInputScriptClass => {
                    assert_eq!(res, Err(NonStandardError::RejectInputScriptClass(test.mtx.id(), 0)), "failed for test '{}'", test.name)
                }
                Expected::RejectInsufficientFee { fee, minimum_fee } => assert_eq!(
                    res,
                    Err(NonStandardError::RejectInsufficientFee(test.mtx.id(), fee, minimum_fee)),
                    "failed for test '{}'",
                    test.name
                ),
            }
        }
    }
}
