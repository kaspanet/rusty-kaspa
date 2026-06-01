//! TODO(post-toccata): Once the transient-mass activation window is behind us, reduce this module
//! to the durable mempool pipeline checks and move/rename it away from Toccata-specific activation
//! coverage.
//!
//! Remove with the activation plumbing:
//! - mined_templates_respect_consensus_transient_mass_across_mempool_delay
//! - mined_template_handles_transactions_added_on_both_sides_of_mempool_delay
//! - rbf_lower_fee_replacement_is_rejected_at_delayed_mempool_activation_boundary
//! - template_limits_reject_transient_tx_until_delayed_mempool_activation
//!
//! Keep as durable pipeline checks:
//! - template_limits_reject_compute_tx_before_consensus_validation
//! - template_limits_reject_storage_tx_after_consensus_validation
//! - template_limits_reject_gas_even_when_non_standard_transactions_are_allowed
//!
//! The durable checks prove that block-limit admission is not standardness: gas and compute
//! rejections happen before consensus in-context validation and script work, while storage
//! rejection happens only after consensus populates contextual mass. They protect the selector
//! invariant that every tx admitted to the pool can fit in a block under the active consensus
//! block limits.

use crate::{
    MiningCounters,
    errors::MiningManagerError,
    manager::MiningManager,
    mempool::{
        config::Config,
        errors::RuleError,
        tx::{Orphan, Priority, RbfPolicy},
    },
};
use kaspa_consensus_core::{
    api::{
        ConsensusApi,
        args::{TransactionValidationArgs, TransactionValidationBatchArgs},
    },
    block::{BlockTemplate, MutableBlock, TemplateBuildMode, TemplateTransactionSelector, VirtualStateApproxId},
    coinbase::MinerData,
    config::{
        constants::consensus::{DEFAULT_GAS_PER_LANE_LIMIT, DEFAULT_LANES_PER_BLOCK_LIMIT},
        params::{ForkActivation, ForkedParam, Params, SIMNET_PARAMS},
    },
    constants::{MAX_TX_IN_SEQUENCE_NUM, SOMPI_PER_KASPA, TX_VERSION},
    errors::{
        block::RuleError as BlockRuleError,
        coinbase::CoinbaseResult,
        tx::{TxResult, TxRuleError},
    },
    header::{CompressedParents, Header},
    mass::{BlockLaneLimits, BlockMassLimits, ContextualMasses, Mass, MassCalculator, MassCofactors, NonContextualMasses},
    merkle::calc_hash_merkle_root,
    subnets::{SUBNETWORK_ID_COINBASE, SUBNETWORK_ID_NATIVE},
    tx::{
        MutableTransaction, ScriptPublicKey, Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput,
        UtxoEntry, scriptvec,
    },
};
use kaspa_core::time::unix_now;
use kaspa_hashes::{Hash, ZERO_HASH};
use parking_lot::RwLock;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

const ACTIVATION_DAA_SCORE: u64 = 10_000;
const PRIOR_BLOCK_MASS_LIMIT: u64 = 500_000;
const NEW_TRANSIENT_LIMIT: u64 = 1_000_000;
const TARGET_TIME_PER_BLOCK: u64 = 100;
const BLOCK_LANE_LIMITS: BlockLaneLimits =
    BlockLaneLimits { lanes_per_block: DEFAULT_LANES_PER_BLOCK_LIMIT, gas_per_lane: DEFAULT_GAS_PER_LANE_LIMIT };

struct MassPolicyTestConsensus {
    virtual_daa_score: AtomicU64,
    mass_calculator: MassCalculator,
    mempool_mass_cofactors: ForkedParam<MassCofactors>,
    validation_attempts: AtomicU64,
    non_contextual_mass_overrides: RwLock<HashMap<TransactionId, NonContextualMasses>>,
    validated_thresholds: RwLock<Vec<(TransactionId, u64, f64)>>,
}

impl MassPolicyTestConsensus {
    fn new(params: &Params) -> Self {
        Self {
            virtual_daa_score: AtomicU64::new(0),
            mass_calculator: MassCalculator::new_with_consensus_params(params),
            mempool_mass_cofactors: params.mempool_block_mass_cofactors(),
            validation_attempts: AtomicU64::new(0),
            non_contextual_mass_overrides: Default::default(),
            validated_thresholds: Default::default(),
        }
    }

    fn set_virtual_daa_score(&self, virtual_daa_score: u64) {
        self.virtual_daa_score.store(virtual_daa_score, Ordering::Relaxed);
    }

    fn validated_thresholds(&self) -> Vec<(TransactionId, u64, f64)> {
        self.validated_thresholds.read().clone()
    }

    fn validation_attempts(&self) -> u64 {
        self.validation_attempts.load(Ordering::Relaxed)
    }

    fn set_non_contextual_masses(&self, transaction_id: TransactionId, masses: NonContextualMasses) {
        self.non_contextual_mass_overrides.write().insert(transaction_id, masses);
    }
}

impl ConsensusApi for MassPolicyTestConsensus {
    fn build_block_template(
        &self,
        miner_data: MinerData,
        mut tx_selector: Box<dyn TemplateTransactionSelector>,
        _build_mode: TemplateBuildMode,
    ) -> Result<BlockTemplate, BlockRuleError> {
        let mut txs = tx_selector.select_transactions();
        let coinbase_miner_data = miner_data.clone();
        txs.insert(
            0,
            Transaction::new(
                TX_VERSION,
                vec![],
                vec![TransactionOutput::new(SOMPI_PER_KASPA, coinbase_miner_data.script_public_key)],
                0,
                SUBNETWORK_ID_COINBASE,
                0,
                coinbase_miner_data.extra_data,
            ),
        );

        let now = unix_now();
        let header = Header::new_finalized(
            0,
            CompressedParents::default(),
            calc_hash_merkle_root(txs.iter()),
            ZERO_HASH,
            ZERO_HASH,
            now,
            0,
            0,
            0,
            0.into(),
            0,
            ZERO_HASH,
        );

        Ok(BlockTemplate::new(MutableBlock::new(header, txs), miner_data, false, now, 0, ZERO_HASH, vec![]))
    }

    fn validate_mempool_transaction(&self, mutable_tx: &mut MutableTransaction, args: &TransactionValidationArgs) -> TxResult<()> {
        self.validation_attempts.fetch_add(1, Ordering::Relaxed);
        if !mutable_tx.is_verifiable() {
            return Err(TxRuleError::MissingTxOutpoints);
        }
        let non_contextual_masses = mutable_tx.calculated_non_contextual_masses.expect("populated by mempool");
        let contextual_masses = self.calculate_transaction_contextual_masses(mutable_tx).ok_or(TxRuleError::MassIncomputable)?;
        mutable_tx.tx.set_mass(contextual_masses.storage_mass);

        let total_in: u64 = mutable_tx.entries.iter().map(|entry| entry.as_ref().unwrap().amount).sum();
        let total_out: u64 = mutable_tx.tx.outputs.iter().map(|output| output.value).sum();
        let fee = total_in - total_out;

        if let Some(threshold) = args.feerate_threshold {
            let mass = Mass::new(non_contextual_masses, contextual_masses);
            let normalized_mass = mass.normalized_max(&self.mempool_mass_cofactors.get(self.get_virtual_daa_score()));
            self.validated_thresholds.write().push((mutable_tx.id(), normalized_mass, threshold));
            if fee as f64 / normalized_mass as f64 <= threshold {
                return Err(TxRuleError::FeerateTooLow);
            }
        }

        mutable_tx.calculated_fee = Some(fee);
        Ok(())
    }

    fn validate_mempool_transactions_in_parallel(
        &self,
        transactions: &mut [MutableTransaction],
        args: &TransactionValidationBatchArgs,
    ) -> Vec<TxResult<()>> {
        transactions.iter_mut().map(|tx| self.validate_mempool_transaction(tx, args.get(&tx.id()))).collect()
    }

    fn populate_mempool_transactions_in_parallel(&self, transactions: &mut [MutableTransaction]) -> Vec<TxResult<()>> {
        transactions.iter_mut().map(|tx| self.validate_mempool_transaction(tx, &Default::default())).collect()
    }

    fn calculate_transaction_non_contextual_masses(&self, transaction: &Transaction) -> TxResult<NonContextualMasses> {
        Ok(self
            .non_contextual_mass_overrides
            .read()
            .get(&transaction.id())
            .copied()
            .unwrap_or_else(|| NonContextualMasses::new(1, transaction.payload.len() as u64)))
    }

    fn calculate_transaction_contextual_masses(&self, transaction: &MutableTransaction) -> Option<ContextualMasses> {
        self.mass_calculator.calc_contextual_masses(&transaction.as_verifiable())
    }

    fn get_virtual_daa_score(&self) -> u64 {
        self.virtual_daa_score.load(Ordering::Relaxed)
    }

    fn get_virtual_state_approx_id(&self) -> VirtualStateApproxId {
        VirtualStateApproxId::new(self.get_virtual_daa_score(), 0.into(), ZERO_HASH)
    }

    fn modify_coinbase_payload(&self, payload: Vec<u8>, _miner_data: &MinerData) -> CoinbaseResult<Vec<u8>> {
        Ok(payload)
    }

    fn calc_transaction_hash_merkle_root(&self, txs: &[Transaction]) -> Hash {
        calc_hash_merkle_root(txs.iter())
    }
}

#[test]
fn mined_templates_respect_consensus_transient_mass_across_mempool_delay() {
    let params = transient_activation_params();
    let delay_daa_score = mempool_delay_daa_score(&params);
    let cases = [
        ("pre activation", ACTIVATION_DAA_SCORE - 1, 2usize, PRIOR_BLOCK_MASS_LIMIT),
        ("at activation", ACTIVATION_DAA_SCORE, 2, PRIOR_BLOCK_MASS_LIMIT),
        ("before delayed mempool activation", ACTIVATION_DAA_SCORE + delay_daa_score - 1, 2, PRIOR_BLOCK_MASS_LIMIT),
        ("at delayed mempool activation", ACTIVATION_DAA_SCORE + delay_daa_score, 4, NEW_TRANSIENT_LIMIT),
        ("after delayed mempool activation", ACTIVATION_DAA_SCORE + delay_daa_score + 1, 4, NEW_TRANSIENT_LIMIT),
    ];

    for (name, virtual_daa_score, expected_selected_txs, expected_transient_mass) in cases {
        let consensus = Arc::new(MassPolicyTestConsensus::new(&params));
        let mining_manager = mining_manager(&params);
        consensus.set_virtual_daa_score(virtual_daa_score);

        let txs = (0..4).map(|i| test_transaction(i, 250_000, 10_000)).collect::<Vec<_>>();
        for tx in txs {
            insert_transaction(&mining_manager, consensus.as_ref(), tx, RbfPolicy::Forbidden).unwrap();
        }

        let selected_txs = selected_template_transactions(&mining_manager, consensus.as_ref());
        let consensus_limits = params.block_mass_limits().get(virtual_daa_score);
        assert_eq!(selected_txs.len(), expected_selected_txs, "{name}: unexpected selected tx count");
        assert_transient_dominates(name, &selected_txs);
        assert_eq!(total_transient_mass(&selected_txs), expected_transient_mass, "{name}: unexpected selected transient mass");
        assert!(
            total_transient_mass(&selected_txs) <= consensus_limits.transient,
            "{name}: template transient mass exceeded consensus limit"
        );
        assert!(
            total_compute_mass(&selected_txs) <= consensus_limits.compute,
            "{name}: template compute mass exceeded consensus limit"
        );
    }
}

#[test]
fn mined_template_handles_transactions_added_on_both_sides_of_mempool_delay() {
    let params = transient_activation_params();
    let delay_daa_score = mempool_delay_daa_score(&params);
    let consensus = Arc::new(MassPolicyTestConsensus::new(&params));
    let mining_manager = mining_manager(&params);

    consensus.set_virtual_daa_score(ACTIVATION_DAA_SCORE + delay_daa_score - 1);
    let old_tx = test_transaction(0, 250_000, 10_000);
    insert_transaction(&mining_manager, consensus.as_ref(), old_tx.clone(), RbfPolicy::Forbidden).unwrap();

    consensus.set_virtual_daa_score(ACTIVATION_DAA_SCORE + delay_daa_score);
    let new_txs = [test_transaction(1, 250_000, 10_000), test_transaction(2, 250_000, 10_000)];
    for tx in new_txs.iter().cloned() {
        insert_transaction(&mining_manager, consensus.as_ref(), tx, RbfPolicy::Forbidden).unwrap();
    }

    let selected_txs = selected_template_transactions(&mining_manager, consensus.as_ref());
    let selected_ids = selected_txs.iter().map(Transaction::id).collect::<HashSet<_>>();
    assert_eq!(selected_txs.len(), 3);
    assert!(selected_ids.contains(&old_tx.id()), "old pre-delay transaction should still be selectable");
    for tx in new_txs {
        assert!(selected_ids.contains(&tx.id()), "new post-delay transaction should be selectable");
    }
    assert_transient_dominates("mixed boundary", &selected_txs);
    assert_eq!(total_transient_mass(&selected_txs), 750_000);
    assert!(total_transient_mass(&selected_txs) <= params.block_mass_limits().get(consensus.get_virtual_daa_score()).transient);
}

#[test]
fn rbf_lower_fee_replacement_is_rejected_at_delayed_mempool_activation_boundary() {
    let params = transient_activation_params();
    let delay_daa_score = mempool_delay_daa_score(&params);
    let boundary_daa_score = ACTIVATION_DAA_SCORE + delay_daa_score;
    let consensus = Arc::new(MassPolicyTestConsensus::new(&params));
    let mining_manager = mining_manager(&params);

    consensus.set_virtual_daa_score(boundary_daa_score - 1);
    let owner = test_transaction(0, 500_000, 1_000);
    insert_transaction(&mining_manager, consensus.as_ref(), owner.clone(), RbfPolicy::Forbidden).unwrap();

    let replacement_before_boundary = double_spend_transaction(1, &owner, 500_000, 900);
    assert!(
        insert_transaction(&mining_manager, consensus.as_ref(), replacement_before_boundary, RbfPolicy::Allowed).is_err(),
        "lower-fee RBF must fail before the delayed mempool activation"
    );

    consensus.set_virtual_daa_score(boundary_daa_score);
    let replacement = double_spend_transaction(2, &owner, 500_000, 900);
    assert!(
        insert_transaction(&mining_manager, consensus.as_ref(), replacement.clone(), RbfPolicy::Allowed).is_err(),
        "lower-fee RBF must still fail once both transactions are compared under the same relaxed mempool policy"
    );

    let threshold_checks = consensus.validated_thresholds();
    assert!(
        threshold_checks.iter().any(|(tx_id, normalized_mass, _)| *tx_id == replacement.id() && *normalized_mass == 250_000),
        "replacement should have been checked against the post-delay normalized transient mass"
    );
    assert!(mining_manager.has_transaction(&owner.id(), crate::model::tx_query::TransactionQuery::All));
    assert!(!mining_manager.has_transaction(&replacement.id(), crate::model::tx_query::TransactionQuery::All));
}

#[test]
fn template_limits_reject_transient_tx_until_delayed_mempool_activation() {
    let params = transient_activation_params();
    let delay_daa_score = mempool_delay_daa_score(&params);
    let boundary_daa_score = ACTIVATION_DAA_SCORE + delay_daa_score;
    let consensus = Arc::new(MassPolicyTestConsensus::new(&params));
    let mining_manager = mining_manager(&params);
    let tx = test_transaction(0, 750_000, 10_000);

    consensus.set_virtual_daa_score(boundary_daa_score - 1);
    let err = match insert_transaction(&mining_manager, consensus.as_ref(), tx.clone(), RbfPolicy::Forbidden) {
        Ok(_) => panic!("transient-heavy tx should exceed the pre-delay template mass limit"),
        Err(err) => err,
    };
    assert!(
        matches!(err, MiningManagerError::MempoolError(RuleError::RejectTransientMass(tx_id, 750_000, PRIOR_BLOCK_MASS_LIMIT)) if tx_id == tx.id()),
        "expected transient-heavy tx to exceed pre-delay template mass limit, got {err:?}"
    );
    assert_eq!(consensus.validation_attempts(), 0, "transient limit rejection should happen before consensus in-context validation");

    consensus.set_virtual_daa_score(boundary_daa_score);
    insert_transaction(&mining_manager, consensus.as_ref(), tx.clone(), RbfPolicy::Forbidden)
        .expect("same tx should fit once the delayed mempool transient limit activates");
    assert!(mining_manager.has_transaction(&tx.id(), crate::model::tx_query::TransactionQuery::All));
}

#[test]
fn template_limits_reject_compute_tx_before_consensus_validation() {
    let params = transient_activation_params();
    let consensus = Arc::new(MassPolicyTestConsensus::new(&params));
    let mining_manager = mining_manager(&params);
    let tx = test_transaction(0, 1, 10_000);
    consensus.set_non_contextual_masses(tx.id(), NonContextualMasses::new(PRIOR_BLOCK_MASS_LIMIT + 1, 1));

    let err = match insert_transaction(&mining_manager, consensus.as_ref(), tx.clone(), RbfPolicy::Forbidden) {
        Ok(_) => panic!("compute-heavy tx should exceed the block-template compute limit"),
        Err(err) => err,
    };
    assert!(
        matches!(err, MiningManagerError::MempoolError(RuleError::RejectComputeMass(tx_id, compute, PRIOR_BLOCK_MASS_LIMIT))
            if tx_id == tx.id() && compute == PRIOR_BLOCK_MASS_LIMIT + 1),
        "expected tx to exceed block-template compute limit, got {err:?}"
    );
    assert_eq!(consensus.validation_attempts(), 0, "compute limit rejection should happen before consensus in-context validation");
}

#[test]
fn template_limits_reject_storage_tx_after_consensus_validation() {
    let params = transient_activation_params();
    let consensus = Arc::new(MassPolicyTestConsensus::new(&params));
    let mining_manager = mining_manager(&params);
    let tx = test_transaction_with_input_amount(0, 1, 1, 2);

    let err = match insert_transaction(&mining_manager, consensus.as_ref(), tx.clone(), RbfPolicy::Forbidden) {
        Ok(_) => panic!("tiny-output tx should exceed the block-template storage mass limit"),
        Err(err) => err,
    };
    assert!(
        matches!(err, MiningManagerError::MempoolError(RuleError::RejectStorageMass(tx_id, storage, PRIOR_BLOCK_MASS_LIMIT))
            if tx_id == tx.id() && storage > PRIOR_BLOCK_MASS_LIMIT),
        "expected tx to exceed block-template storage mass limit, got {err:?}"
    );
    assert_eq!(consensus.validation_attempts(), 1, "storage limit rejection should happen after consensus in-context validation");
}

#[test]
fn template_limits_reject_gas_even_when_non_standard_transactions_are_allowed() {
    let params = transient_activation_params();
    let consensus = Arc::new(MassPolicyTestConsensus::new(&params));
    let mining_manager = mining_manager(&params);
    let tx = test_transaction_with_gas(0, 10_000, 10_000, DEFAULT_GAS_PER_LANE_LIMIT + 1);

    let err = match insert_transaction(&mining_manager, consensus.as_ref(), tx.clone(), RbfPolicy::Forbidden) {
        Ok(_) => panic!("gas-heavy tx should exceed the block-template gas limit"),
        Err(err) => err,
    };
    assert!(
        matches!(
            err,
            MiningManagerError::MempoolError(RuleError::RejectGas(tx_id, gas, DEFAULT_GAS_PER_LANE_LIMIT))
                if tx_id == tx.id() && gas == DEFAULT_GAS_PER_LANE_LIMIT + 1
        ),
        "expected tx to exceed block-template gas limit, got {err:?}"
    );
    assert_eq!(consensus.validation_attempts(), 0, "gas limit rejection should happen before consensus in-context validation");
}

fn transient_activation_params() -> Params {
    let mut params = SIMNET_PARAMS.clone();
    params.prior_block_mass_limits = BlockMassLimits::with_shared_limit(PRIOR_BLOCK_MASS_LIMIT);
    params.new_transient_mass_limit = NEW_TRANSIENT_LIMIT;
    params.toccata_activation = ForkActivation::new(ACTIVATION_DAA_SCORE);
    params
}

fn mempool_delay_daa_score(params: &Params) -> u64 {
    24 * 60 * 60 * params.bps()
}

fn mining_manager(params: &Params) -> MiningManager {
    let config = Config::build_default(TARGET_TIME_PER_BLOCK, true, params.mempool_block_mass_limits(), BLOCK_LANE_LIMITS);
    MiningManager::with_config(config, params.toccata_activation, None, Arc::new(MiningCounters::default()))
}

fn test_transaction(n: u64, transient_mass: u64, fee: u64) -> MutableTransaction {
    test_transaction_with_gas(n, transient_mass, fee, 0)
}

fn test_transaction_with_input_amount(n: u64, transient_mass: u64, fee: u64, input_amount: u64) -> MutableTransaction {
    transaction_spending_outpoint(n, outpoint(n), transient_mass, fee, input_amount, 0)
}

fn test_transaction_with_gas(n: u64, transient_mass: u64, fee: u64, gas: u64) -> MutableTransaction {
    transaction_spending_outpoint(n, outpoint(n), transient_mass, fee, 10 * SOMPI_PER_KASPA, gas)
}

fn double_spend_transaction(n: u64, owner: &MutableTransaction, transient_mass: u64, fee: u64) -> MutableTransaction {
    transaction_spending_outpoint(n, owner.tx.inputs[0].previous_outpoint, transient_mass, fee, 10 * SOMPI_PER_KASPA, 0)
}

fn transaction_spending_outpoint(
    n: u64,
    outpoint: TransactionOutpoint,
    transient_mass: u64,
    fee: u64,
    input_amount: u64,
    gas: u64,
) -> MutableTransaction {
    let script_public_key = ScriptPublicKey::new(0, scriptvec![0x51]);
    let input = TransactionInput::new(outpoint, vec![], MAX_TX_IN_SEQUENCE_NUM, 0);
    let output = TransactionOutput::new(input_amount - fee, script_public_key.clone());
    let tx =
        Transaction::new(TX_VERSION, vec![input], vec![output], 0, SUBNETWORK_ID_NATIVE, gas, vec![n as u8; transient_mass as usize]);
    let entry = UtxoEntry::new(input_amount, script_public_key, 0, false, None);
    MutableTransaction::with_entries(tx.into(), vec![entry])
}

fn outpoint(n: u64) -> TransactionOutpoint {
    TransactionOutpoint::new(Hash::from_u64_word(n), 0)
}

fn insert_transaction(
    mining_manager: &MiningManager,
    consensus: &dyn ConsensusApi,
    tx: MutableTransaction,
    rbf_policy: RbfPolicy,
) -> crate::errors::MiningManagerResult<crate::model::tx_insert::TransactionInsertion> {
    mining_manager.validate_and_insert_mutable_transaction(consensus, tx, Priority::Low, Orphan::Forbidden, rbf_policy)
}

fn selected_template_transactions(mining_manager: &MiningManager, consensus: &dyn ConsensusApi) -> Vec<Transaction> {
    let template =
        mining_manager.get_block_template(consensus, &MinerData::new(ScriptPublicKey::new(0, scriptvec![]), vec![])).unwrap();
    template.block.transactions.into_iter().skip(1).collect()
}

fn total_transient_mass(txs: &[Transaction]) -> u64 {
    txs.iter().map(|tx| tx.payload.len() as u64).sum()
}

fn total_compute_mass(txs: &[Transaction]) -> u64 {
    txs.len() as u64
}

fn assert_transient_dominates(name: &str, txs: &[Transaction]) {
    assert!(total_transient_mass(txs) > total_compute_mass(txs) * 100_000, "{name}: expected transient mass to dominate compute mass");
}
