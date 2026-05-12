use crate::mempool::{
    Mempool,
    errors::{NonStandardError, NonStandardResult},
};
use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
use kaspa_consensus_core::{
    constants::{MAX_SCRIPT_PUBLIC_KEY_VERSION, MAX_SOMPI},
    mass::NonContextualMasses,
    tx::{MutableTransaction, PopulatedTransaction},
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

        // None of the output public key scripts can be a non-standard script.
        for (i, output) in transaction.tx.outputs.iter().enumerate() {
            if output.script_public_key.version() > MAX_SCRIPT_PUBLIC_KEY_VERSION {
                return Err(NonStandardError::RejectScriptPublicKeyVersion(transaction_id, i));
            }

            if ScriptClass::from_script(&output.script_public_key) == ScriptClass::NonStandard {
                return Err(NonStandardError::RejectOutputScriptClass(transaction_id, i));
            }
        }

        Ok(())
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
