//! Tests that block-limit admission is not standardness: gas and compute rejections happen
//! before consensus in-context validation and script work, while storage rejection happens only
//! after consensus populates contextual mass. Together they protect the selector invariant that
//! every transaction admitted to the pool can fit into a block under the active consensus block
//! limits.

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
    collections::HashMap,
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
    mass_calculator: MassCalculator,
    mempool_mass_cofactors: ForkedParam<MassCofactors>,
    validation_attempts: AtomicU64,
    non_contextual_mass_overrides: RwLock<HashMap<TransactionId, NonContextualMasses>>,
}

impl MassPolicyTestConsensus {
    fn new(params: &Params) -> Self {
        Self {
            mass_calculator: MassCalculator::new_with_consensus_params(params),
            mempool_mass_cofactors: params.mempool_block_mass_cofactors(),
            validation_attempts: AtomicU64::new(0),
            non_contextual_mass_overrides: Default::default(),
        }
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
        mutable_tx.tx.set_storage_mass(contextual_masses.storage_mass);

        let total_in: u64 = mutable_tx.entries.iter().map(|entry| entry.as_ref().unwrap().amount).sum();
        let total_out: u64 = mutable_tx.tx.outputs.iter().map(|output| output.value).sum();
        let fee = total_in - total_out;

        if let Some(threshold) = args.feerate_threshold {
            let mass = Mass::new(non_contextual_masses, contextual_masses);
            let normalized_mass = mass.normalized_max(&self.mempool_mass_cofactors.get(self.get_virtual_daa_score()));
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
        0
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

fn mining_manager(params: &Params) -> MiningManager {
    let config = Config::build_default(TARGET_TIME_PER_BLOCK, true, params.mempool_block_mass_limits(), BLOCK_LANE_LIMITS);
    MiningManager::with_config(config, None, Arc::new(MiningCounters::default()))
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
