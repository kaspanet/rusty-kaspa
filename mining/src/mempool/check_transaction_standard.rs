use crate::mempool::{
    Mempool,
    config::LEGACY_MINIMUM_RELAY_TRANSACTION_FEE,
    errors::{NonStandardError, NonStandardResult},
    tx::Priority,
};
use kaspa_consensus_core::{
    constants::{MAX_SCRIPT_PUBLIC_KEY_VERSION, MAX_SOMPI},
    tx::MutableTransaction,
};
use kaspa_txscript::{post_toccata_p2sh_sig_scanner, script_class::ScriptClass};

/// MAX_STANDARD_P2SH_SIG_OPS is the maximum number of signature operations
/// that are considered standard in a pay-to-script-hash script.
///
/// The upper-bound execution limit comes from compute mass: some zk opcodes already cost the equivalent
/// of roughly 140-250 signature operations. However, for classic Schnorr/ECDSA signature operations, this
/// standardness limit encourages parallelism across inputs rather than concentrating work in one input.
const MAX_STANDARD_P2SH_SIG_OPS: u16 = 15;

/// MAXIMUM_STANDARD_TRANSACTION_MASS is the maximum per-dimension transaction mass considered
/// standard prior to Toccata. Until the network is about to activate Toccata (see
/// STANDARD_MASS_RELAXATION_WINDOW_SECONDS), the mempool keeps rejecting higher-mass transactions
/// as non-standard, so that updated nodes don't admit and relay transactions that not-yet-updated
/// peers would drop, leaving them stuck unmined.
pub(crate) const MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA: u64 = 100_000;

/// Window, in seconds, before Toccata activation at which the standard mass cap is relaxed. By this
/// point the network is expected to be fully updated, so high-mass transactions can be admitted and
/// primed in mempools ahead of activation.
const STANDARD_MASS_RELAXATION_WINDOW_SECONDS: u64 = 30 * 60;

impl Mempool {
    /// Returns the per-dimension standard mass cap currently in effect, or `None` once the cap has
    /// been relaxed (within [`STANDARD_MASS_RELAXATION_WINDOW_SECONDS`] before Toccata activation,
    /// and ever after). `early_by` keeps `never`/`always` activations untouched, so networks without
    /// a scheduled Toccata keep the cap and `always`-active ones never apply it.
    ///
    /// TODO(post-toccata): once Toccata is active on all networks, delete this helper together with
    /// MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA, STANDARD_MASS_RELAXATION_WINDOW_SECONDS and the
    /// cap checks in `check_transaction_standard_in_isolation`/`_in_context`. The block-fit limits in
    /// check_transaction_limits then remain the only per-tx mass ceiling.
    fn standard_transaction_mass_cap(&self, virtual_daa_score: u64) -> Option<u64> {
        let window = STANDARD_MASS_RELAXATION_WINDOW_SECONDS.saturating_mul(self.config.network_blocks_per_second);
        let relaxation = self.toccata_activation.early_by(window);
        (!relaxation.is_active(virtual_daa_score)).then_some(MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA)
    }

    pub(crate) fn check_transaction_standard_in_isolation(
        &self,
        transaction: &MutableTransaction,
        virtual_daa_score: u64,
    ) -> NonStandardResult<()> {
        let transaction_id = transaction.id();

        // Until shortly before Toccata activates, keep the legacy per-tx standard mass cap on the
        // non-contextual dimensions so updated nodes don't relay transactions that peers still reject.
        if let Some(cap) = self.standard_transaction_mass_cap(virtual_daa_score) {
            let masses = transaction.calculated_non_contextual_masses.unwrap();
            if masses.compute_mass > cap {
                return Err(NonStandardError::RejectComputeMass(transaction_id, masses.compute_mass, cap));
            }
            if masses.transient_mass > cap {
                return Err(NonStandardError::RejectTransientMass(transaction_id, masses.transient_mass, cap));
            }
        }

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
    /// MAX_STANDARD_P2SH_SIG_OPS signature operations.
    /// In addition, makes sure that the transaction's fee is above the minimum for acceptance
    /// into the mempool and relay.
    pub(crate) fn check_transaction_standard_in_context(
        &self,
        transaction: &MutableTransaction,
        priority: Priority,
        virtual_daa_score: u64,
    ) -> NonStandardResult<()> {
        let transaction_id = transaction.id();

        // Storage mass is only known after contextual population, so the standard mass cap is applied to it here.
        if let Some(cap) = self.standard_transaction_mass_cap(virtual_daa_score) {
            let storage_mass = transaction.tx.storage_mass();
            if storage_mass > cap {
                return Err(NonStandardError::RejectStorageMass(transaction_id, storage_mass, cap));
            }
        }

        for i in 0..transaction.tx.inputs.len() {
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
                    // post-toccata scanner is valid pre-toccata as well
                    let num_sig_ops =
                        post_toccata_p2sh_sig_scanner(&transaction.tx.inputs[i].signature_script, &entry.script_public_key);
                    if num_sig_ops > MAX_STANDARD_P2SH_SIG_OPS as u64 {
                        return Err(NonStandardError::RejectSignatureCount(transaction_id, i, num_sig_ops, MAX_STANDARD_P2SH_SIG_OPS));
                    }
                }
            }
        }

        // Minimum relay fee applies to normalized non-contextual mass so block-space usage has a
        // minimum cost, whether dominated by compute or by transient byte footprint.
        // Storage mass does not require an additional relay-fee floor here since storage growth is
        // sufficiently protected even under worst-case block-limit usage.
        // Use raw_post so all networks, including non-scheduled ones, use the same standardness
        // pricing value and do not fluctuate with activation status.
        let masses = transaction.calculated_non_contextual_masses.unwrap();
        let cofactors = self.config.mempool_mass_cofactors.raw_post();
        let normalized_transient_mass = masses.normalized_transient(&cofactors);

        // TODO(post-toccata): remove `use_prior_p2p_fee_rules` and unconditionally use:
        //   fee_mass = max(compute_mass, normalized_transient_mass)
        //   relay_fee = self.config.minimum_relay_transaction_fee
        // The relaxation allows software to adapt between release and activation, instead of txs being p2p rejected by updated nodes
        let use_prior_p2p_fee_rules = priority == Priority::Low && !self.toccata_activation.is_active(virtual_daa_score);
        let (fee_mass, relay_fee) = if use_prior_p2p_fee_rules {
            // Prior P2P relay-fee policy: legacy base fee over compute mass only.
            (masses.compute_mass, LEGACY_MINIMUM_RELAY_TRANSACTION_FEE)
        } else {
            (masses.compute_mass.max(normalized_transient_mass), self.config.minimum_relay_transaction_fee)
        };
        let minimum_fee = self.minimum_required_transaction_relay_fee(fee_mass, relay_fee);

        let fee = transaction.calculated_fee.unwrap();
        if fee < minimum_fee {
            return if use_prior_p2p_fee_rules || masses.compute_mass >= normalized_transient_mass {
                Err(NonStandardError::RejectInsufficientComputeFee(transaction_id, fee, minimum_fee, masses.compute_mass))
            } else {
                Err(NonStandardError::RejectInsufficientTransientFee(transaction_id, fee, minimum_fee, normalized_transient_mass))
            };
        }
        // end-TODO

        Ok(())
    }

    /// minimum_required_transaction_relay_fee returns the minimum transaction fee required
    /// for a transaction with the passed mass to be accepted into the mempool and relayed.
    fn minimum_required_transaction_relay_fee(&self, mass: u64, minimum_relay_transaction_fee: u64) -> u64 {
        // Calculate the minimum fee for a transaction to be allowed into the
        // mempool and relayed by scaling the base fee. MinimumRelayTransactionFee is in
        // sompi/kg so multiply by mass (which is in grams) and divide by 1000 to get
        // minimum sompis.
        let mut minimum_fee = (mass * minimum_relay_transaction_fee) / 1000;

        if minimum_fee == 0 {
            minimum_fee = minimum_relay_transaction_fee;
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
        config::params::{ForkActivation, Params},
        constants::{MAX_TX_IN_SEQUENCE_NUM, SOMPI_PER_KASPA, TRANSIENT_BYTE_TO_MASS_FACTOR, TX_VERSION},
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
    const fn default_minimum_relay_fee_for_mass(mass: u64) -> u64 {
        mass * DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE / 1000
    }

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
                want: default_minimum_relay_fee_for_mass(100),
            },
            Test {
                name: "large relay fee test mass with default minimum relay fee",
                size: RELAY_FEE_TEST_MASS,
                minimum_relay_transaction_fee: DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE,
                want: default_minimum_relay_fee_for_mass(RELAY_FEE_TEST_MASS),
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
                let minimum_relay_transaction_fee = config.minimum_relay_transaction_fee;
                let counters = Arc::new(MiningCounters::default());
                let mempool = Mempool::new(Arc::new(config), params.toccata_activation, counters);

                let got = mempool.minimum_required_transaction_relay_fee(test.size, minimum_relay_transaction_fee);
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
                let mempool = Mempool::new(Arc::new(config), params.toccata_activation, counters);

                // Ensure standard-ness is as expected.
                println!("test_check_transaction_standard_in_isolation test '{}' ", test.name);
                let res = mempool.check_transaction_standard_in_isolation(&test.mtx, 0);
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
            RejectInsufficientComputeFee { fee: u64, minimum_fee: u64, compute_mass: u64 },
            RejectInsufficientTransientFee { fee: u64, minimum_fee: u64, normalized_transient_mass: u64 },
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

        // Use simnet params so prior and post-activation transient limits differ while the fork is active;
        // this verifies that relay-fee pricing uses stable post-activation cofactors.
        let params: Params = NetworkType::Simnet.into();
        assert_ne!(params.toccata_activation, ForkActivation::never(), "this test requires post-activation cofactors");
        let cofactors = params.mempool_block_mass_cofactors().after();
        let transient = |bytes| bytes * TRANSIENT_BYTE_TO_MASS_FACTOR;
        let normalized_transient = |bytes| NonContextualMasses::new(0, transient(bytes)).normalized_transient(&cofactors);

        let bytes = 5_000;
        let compute = normalized_transient(bytes);
        let boundary_fee = default_minimum_relay_fee_for_mass(compute);
        let insufficient_fee = boundary_fee - 1;

        let tests = vec![
            Test {
                name: "standard input with exactly sufficient relay fee",
                mtx: new_mtx(standard_script_public_key.clone(), NonContextualMasses::new(compute, transient(bytes)), boundary_fee),
                expected: Expected::Standard,
            },
            Test {
                name: "non-standard input script class",
                mtx: new_mtx(
                    non_standard_script_public_key,
                    NonContextualMasses::new(1_000, 1_000),
                    DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE,
                ),
                expected: Expected::RejectInputScriptClass,
            },
            Test {
                name: "compute mass triggers insufficient relay fee",
                mtx: new_mtx(
                    standard_script_public_key.clone(),
                    NonContextualMasses::new(compute, transient(bytes - 1)),
                    insufficient_fee,
                ),
                expected: Expected::RejectInsufficientComputeFee {
                    fee: insufficient_fee,
                    minimum_fee: boundary_fee,
                    compute_mass: compute,
                },
            },
            Test {
                name: "transient mass triggers insufficient relay fee",
                mtx: new_mtx(standard_script_public_key, NonContextualMasses::new(compute - 1, transient(bytes)), insufficient_fee),
                expected: Expected::RejectInsufficientTransientFee {
                    fee: insufficient_fee,
                    minimum_fee: boundary_fee,
                    normalized_transient_mass: compute,
                },
            },
        ];

        let config =
            Config::build_default(params.target_time_per_block(), false, params.mempool_block_mass_limits(), params.block_lane_limits);
        let counters = Arc::new(MiningCounters::default());
        let mempool = Mempool::new(Arc::new(config), params.toccata_activation, counters);

        for test in tests {
            let res = mempool.check_transaction_standard_in_context(&test.mtx, Priority::High, 0);
            match test.expected {
                Expected::Standard => assert_eq!(res, Ok(()), "failed for test '{}'", test.name),
                Expected::RejectInputScriptClass => {
                    assert_eq!(res, Err(NonStandardError::RejectInputScriptClass(test.mtx.id(), 0)), "failed for test '{}'", test.name)
                }
                Expected::RejectInsufficientComputeFee { fee, minimum_fee, compute_mass } => assert_eq!(
                    res,
                    Err(NonStandardError::RejectInsufficientComputeFee(test.mtx.id(), fee, minimum_fee, compute_mass)),
                    "failed for test '{}'",
                    test.name
                ),
                Expected::RejectInsufficientTransientFee { fee, minimum_fee, normalized_transient_mass } => assert_eq!(
                    res,
                    Err(NonStandardError::RejectInsufficientTransientFee(test.mtx.id(), fee, minimum_fee, normalized_transient_mass)),
                    "failed for test '{}'",
                    test.name
                ),
            }
        }

        // TODO(post-toccata): remove this temporary test case
        let params: Params = NetworkType::Mainnet.into();

        let config =
            Config::build_default(params.target_time_per_block(), false, params.mempool_block_mass_limits(), params.block_lane_limits);
        let toccata_activation = ForkActivation::new(10);
        let toccata_daa_activation = toccata_activation.daa_score();
        let cofactors = config.mempool_mass_cofactors.raw_post();
        let mempool = Mempool::new(Arc::new(config), toccata_activation, Arc::new(MiningCounters::default()));

        let compute_mass = 1_000;
        let legacy_minimum_fee = LEGACY_MINIMUM_RELAY_TRANSACTION_FEE;
        let insufficient_legacy_fee = legacy_minimum_fee - 1;
        let masses = NonContextualMasses::new(compute_mass, compute_mass * 4);
        let normalized_transient_mass = masses.normalized_transient(&cofactors);
        assert!(normalized_transient_mass > compute_mass);
        let new_minimum_fee = default_minimum_relay_fee_for_mass(normalized_transient_mass);

        let addr = Address::new(Prefix::Mainnet, Version::PubKey, &[1u8; 32]);
        let script_public_key = kaspa_txscript::pay_to_address_script(&addr);
        let mtx = new_mtx(script_public_key.clone(), masses, legacy_minimum_fee);

        assert_eq!(mempool.check_transaction_standard_in_context(&mtx, Priority::Low, toccata_daa_activation - 1), Ok(()));

        assert_eq!(
            mempool.check_transaction_standard_in_context(
                &new_mtx(script_public_key, masses, insufficient_legacy_fee),
                Priority::Low,
                toccata_daa_activation - 1
            ),
            Err(NonStandardError::RejectInsufficientComputeFee(mtx.id(), insufficient_legacy_fee, legacy_minimum_fee, compute_mass))
        );

        assert_eq!(
            mempool.check_transaction_standard_in_context(&mtx, Priority::Low, toccata_daa_activation),
            Err(NonStandardError::RejectInsufficientTransientFee(
                mtx.id(),
                legacy_minimum_fee,
                new_minimum_fee,
                normalized_transient_mass
            ))
        );

        assert_eq!(
            mempool.check_transaction_standard_in_context(&mtx, Priority::High, toccata_daa_activation - 1),
            Err(NonStandardError::RejectInsufficientTransientFee(
                mtx.id(),
                legacy_minimum_fee,
                new_minimum_fee,
                normalized_transient_mass
            ))
        );

        assert_eq!(
            mempool.check_transaction_standard_in_context(&mtx, Priority::High, toccata_daa_activation),
            Err(NonStandardError::RejectInsufficientTransientFee(
                mtx.id(),
                legacy_minimum_fee,
                new_minimum_fee,
                normalized_transient_mass
            ))
        );
        // end-TODO
    }

    #[test]
    fn test_standard_transaction_mass_cap() {
        // Toccata score far enough out that the relaxation window (30 min * bps) starts at a positive score.
        let toccata_activation = ForkActivation::new(1_000_000);
        let params: Params = NetworkType::Mainnet.into();
        let config =
            Config::build_default(params.target_time_per_block(), false, params.mempool_block_mass_limits(), params.block_lane_limits);
        let mempool = Mempool::new(Arc::new(config), toccata_activation, Arc::new(MiningCounters::default()));

        let addr = Address::new(Prefix::Mainnet, Version::PubKey, &[1u8; 32]);
        let spk = kaspa_txscript::pay_to_address_script(&addr);

        // 0 is before the relaxation window opens; one score below activation is inside it.
        let before_window = 0;
        let in_window = toccata_activation.daa_score() - 1;
        let over = MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA + 1;

        // Non-contextual (compute/transient) dimensions, checked in isolation.
        let isolation_mtx = |compute, transient| {
            let input = TransactionInput::new(
                TransactionOutpoint::new(kaspa_hashes::Hash::from_u64_word(1), 0),
                vec![],
                MAX_TX_IN_SEQUENCE_NUM,
                0,
            );
            let tx = Transaction::new(
                TX_VERSION,
                vec![input],
                vec![TransactionOutput::new(SOMPI_PER_KASPA, spk.clone())],
                0,
                SUBNETWORK_ID_NATIVE,
                0,
                vec![],
            );
            let mut mtx = MutableTransaction::from_tx(tx);
            mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(compute, transient));
            mtx
        };

        let high_compute = isolation_mtx(over, 1_000);
        assert_eq!(
            mempool.check_transaction_standard_in_isolation(&high_compute, before_window),
            Err(NonStandardError::RejectComputeMass(high_compute.id(), over, MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA))
        );
        assert_eq!(mempool.check_transaction_standard_in_isolation(&high_compute, in_window), Ok(()));

        let high_transient = isolation_mtx(1_000, over);
        assert_eq!(
            mempool.check_transaction_standard_in_isolation(&high_transient, before_window),
            Err(NonStandardError::RejectTransientMass(high_transient.id(), over, MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA))
        );
        assert_eq!(mempool.check_transaction_standard_in_isolation(&high_transient, in_window), Ok(()));

        // A transaction at the cap is standard even before the window.
        let within_cap = isolation_mtx(MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA, MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA);
        assert_eq!(mempool.check_transaction_standard_in_isolation(&within_cap, before_window), Ok(()));

        // Contextual storage dimension, checked in context.
        let context_mtx = |storage: u64, fee: u64| {
            let input = TransactionInput::new(
                TransactionOutpoint::new(kaspa_hashes::Hash::from_u64_word(1), 0),
                vec![],
                MAX_TX_IN_SEQUENCE_NUM,
                0,
            );
            let tx = Transaction::new(
                TX_VERSION,
                vec![input],
                vec![TransactionOutput::new(SOMPI_PER_KASPA, spk.clone())],
                0,
                SUBNETWORK_ID_NATIVE,
                0,
                vec![],
            );
            tx.set_storage_mass(storage);
            let mut mtx =
                MutableTransaction::with_entries(tx.into(), vec![UtxoEntry::new(2 * SOMPI_PER_KASPA, spk.clone(), 0, false, None)]);
            mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(1_000, 1_000));
            mtx.calculated_fee = Some(fee);
            mtx
        };

        let high_storage = context_mtx(over, SOMPI_PER_KASPA);
        assert_eq!(
            mempool.check_transaction_standard_in_context(&high_storage, Priority::High, before_window),
            Err(NonStandardError::RejectStorageMass(high_storage.id(), over, MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA))
        );
        // Within the window the storage cap is lifted; the well-funded, standard tx is accepted.
        assert_eq!(mempool.check_transaction_standard_in_context(&high_storage, Priority::High, in_window), Ok(()));
    }
}
