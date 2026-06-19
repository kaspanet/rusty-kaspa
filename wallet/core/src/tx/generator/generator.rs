//!
//! Transaction generator module used for creating multi-stage transactions
//! optimized for parallelized DAG processing.
//!
//! The [`Generator`] intakes a set of UTXO entries and accumulates them as
//! inputs into a single transaction. If transaction hits mass boundaries
//! before 1) desired amount is reached or 2) all UTXOs are consumed, the
//! transaction is yielded and a "relay" transaction is created.
//!
//! If "relay" transactions are created, the [`Generator`] will aggregate
//! such transactions into a single transaction and repeat the process
//! until 1) desired amount is reached or 2) all UTXOs are consumed.
//!
//! This processing results in a creation of a transaction tree where
//! each level (stage) of this tree is submitted to the network in parallel.
//!
//!```text
//!
//! Tx1 Tx2 Tx3 Tx4 Tx5 Tx6     | stage 0 (relays to stage 1)
//!  |   |   |   |   |   |      |
//!  +---+   +---+   +---+      |
//!    |       |       |        |
//!   Tx7     Tx8     Tx9       | stage 1 (relays to stage 2)
//!    |       |       |        |
//!    +-------+-------+        |
//!            |                |
//!           Tx10              | stage 2 (final outbound transaction)
//!
//!```
//!
//! The generator will produce transactions in the following order:
//! Tx1, Tx2, Tx3, Tx4, Tx5, Tx6, Tx7, Tx8, Tx9, Tx10
//!
//! Transactions within a single stage are independent of one another
//! and as such can be processed in parallel.
//!
//! The [`Generator`] acts as a transaction iterator, yielding transactions
//! for each iteration. These transactions can be obtained via an iterator
//! interface or via an async Stream interface.
//!
//! Q: Why is this not implemented as a single loop?
//!
//! A: There are a number of requirements that need to be handled:
//!
//! 1. UTXO entry consumption while creating inputs may result in
//!    additional fees, requiring additional UTXO entries to cover
//!    the fees. Goto 1. (this is a classic issue, can be solved using padding)
//!
//! 2. The overall design strategy for this processor is to allow
//!    concurrent processing of a large number of transactions and UTXOs.
//!    This implementation avoids in-memory aggregation of all
//!    transactions that may result in OOM conditions.
//!
//! 3. If used with a large UTXO set, the transaction generation process
//!    needs to be asynchronous to avoid blocking the main thread. In the
//!    context of WASM32 SDK, not doing that while working with large
//!    UTXO sets will result in a browser UI freezing.
//!

use super::SignerT;
use crate::imports::*;
use crate::result::Result;
use crate::tx::{
    Fees, GeneratorSettings, GeneratorSummary, PaymentDestination, PendingTransaction, PendingTransactionIterator,
    PendingTransactionStream, mass::*,
};
use crate::utxo::{NetworkParams, UtxoContext, UtxoEntryReference};
use kaspa_consensus_client::UtxoEntry;
use kaspa_consensus_core::constants::UNACCEPTED_DAA_SCORE;
use kaspa_consensus_core::mass::NonContextualMasses;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{ComputeCommit, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
use kaspa_txscript::pay_to_address_script;
use std::collections::VecDeque;

// fee reduction - when a transactions has some storage mass
// and the total mass is below this threshold (as well as
// other conditions), we attempt to accumulate additional
// inputs to reduce storage mass/fees
//
// TODO(post-toccata): remove this const and cleanup pre toccata path
const TRANSACTION_MASS_BOUNDARY_FOR_ADDITIONAL_INPUT_ACCUMULATION_PRE_TOCCATA: u64 =
    MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA / 5 * 4;
const TRANSACTION_MASS_BOUNDARY_FOR_ADDITIONAL_INPUT_ACCUMULATION_POST_TOCCATA: u64 =
    MAXIMUM_STANDARD_TRANSACTION_MASS_POST_TOCCATA / 5 * 4;

// optimization boundary - when aggregating inputs,
// we don't perform any checks until we reach this mass
// or the aggregate input amount reaches the requested
// output amount
//
// TODO(post-toccata): remove this const and cleanup pre toccata path
const TRANSACTION_MASS_BOUNDARY_FOR_STAGE_INPUT_ACCUMULATION_PRE_TOCCATA: u64 = MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA / 5 * 4;
const TRANSACTION_MASS_BOUNDARY_FOR_STAGE_INPUT_ACCUMULATION_POST_TOCCATA: u64 =
    MAXIMUM_STANDARD_TRANSACTION_MASS_POST_TOCCATA / 5 * 4;

// TODO(post-toccata): remove this const and cleanup pre toccata path
const MAXIMUM_TRANSACTION_CRITERIA_MASS_SANITY_CHECK_PRE_TOCCATA: u64 = MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA / 5 * 4;
const MAXIMUM_TRANSACTION_CRITERIA_MASS_SANITY_CHECK_POST_TOCCATA: u64 = MAXIMUM_STANDARD_TRANSACTION_MASS_POST_TOCCATA / 5 * 4;

/// Mutable [`Generator`] state used to track the current transaction generation process.
struct Context {
    /// iterator containing UTXO entries available for transaction generation
    utxo_source_iterator: Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static>,
    /// List of priority UTXO entries, that are consumed before polling the iterator
    priority_utxo_entries: Option<VecDeque<UtxoEntryReference>>,
    /// HashSet containing priority UTXO entries, used for filtering
    /// for potential duplicates from the iterator
    priority_utxo_entry_filter: Option<HashSet<UtxoEntryReference>>,
    /// total number of UTXOs consumed by the single generator instance
    aggregated_utxos: usize,
    /// total fees of all transactions issued by
    /// the single generator instance
    aggregate_fees: u64,
    /// total mass of all transactions issued by
    /// the single generator instance
    aggregate_mass: u64,
    /// number of generated transactions
    number_of_transactions: usize,
    /// Number of generated stages. Stage represents multiple transactions
    /// executed in parallel. Each stage is a tree level in the transaction
    /// tree. When calculating time for submission of transactions, the estimated
    /// time per transaction (either as DAA score or a fee-rate based estimate)
    /// should be multiplied by the number of stages.
    number_of_stages: usize,
    /// current tree stage
    stage: Option<Box<Stage>>,
    /// Rejected or "stashed" UTXO entries that are consumed before polling
    /// the iterator. This store is used in edge cases when UTXO entry from the
    /// iterator has been consumed but was rejected due to mass constraints or
    /// other conditions.
    utxo_stash: VecDeque<UtxoEntryReference>,
    /// final transaction id
    final_transaction_id: Option<TransactionId>,
    /// signifies that the generator is finished
    /// no more items will be produced in the
    /// iterator or a stream
    is_done: bool,
}

/// [`Generator`] stage. A "tree level" processing stage, used to track
/// transactions processed during a stage.
#[derive(Default)]
struct Stage {
    /// iterator containing UTXO entries from the previous tree stage
    utxo_iterator: Option<Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static>>,
    /// UTXOs generated during this stage
    utxo_accumulator: Vec<UtxoEntryReference>,
    /// Total aggregate value of all inputs consumed during this stage
    aggregate_input_value: u64,
    /// Total aggregate value of all fees incurred during this stage
    aggregate_fees: u64,
    /// Total number of transactions generated during this stage
    number_of_transactions: usize,
}

impl Stage {
    fn new(previous: Stage) -> Stage {
        let utxo_iterator: Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static> =
            Box::new(previous.utxo_accumulator.into_iter());

        Stage {
            utxo_iterator: Some(utxo_iterator),
            utxo_accumulator: vec![],
            aggregate_input_value: 0,
            aggregate_fees: 0,
            number_of_transactions: 0,
        }
    }
}

impl std::fmt::Debug for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stage")
            .field("aggregate_input_value", &self.aggregate_input_value)
            .field("aggregate_fees", &self.aggregate_fees)
            .field("number_of_transactions", &self.number_of_transactions)
            .finish()
    }
}

///
///  Indicates the type of data yielded by the generator
///
#[derive(Debug, Copy, Clone)]
pub enum DataKind {
    /// No operation should be performed (abort)
    /// Used for handling exceptions, such as rejecting
    /// to produce dust outputs during sweep transactions.
    NoOp,
    /// A "tree node" or "relay" transaction meant for multi-stage
    /// operations. This transaction combines multiple UTXOs
    /// into a single transaction to the supplied change address.
    Node,
    /// A "tree edge" transaction meant for multi-stage
    /// processing. Signifies completion of the tree level (stage).
    /// This operation will create a new tree level (stage).
    Edge,
    /// Final transaction combining the entire aggregated UTXO set
    /// into a single set of supplied outputs.
    Final,
}

impl DataKind {
    pub fn is_final(&self) -> bool {
        matches!(self, DataKind::Final)
    }
    pub fn is_stage_node(&self) -> bool {
        matches!(self, DataKind::Node)
    }
    pub fn is_stage_edge(&self) -> bool {
        matches!(self, DataKind::Edge)
    }
}

///
/// Single transaction data accumulator.  This structure is used to accumulate
/// and track all necessary transaction data and is then used to create
/// an actual transaction.
///
#[derive(Debug)]
struct Data {
    /// Transaction inputs accumulated during processing
    inputs: Vec<TransactionInput>,
    /// UTXO entries referenced by transaction inputs
    utxo_entry_references: Vec<UtxoEntryReference>,
    /// Addresses referenced by transaction inputs
    addresses: HashSet<Address>,
    /// Aggregate transaction non-contextual masses, inited with blank transaction
    aggregate_non_contextual_masses: NonContextualMasses,
    /// Transaction fees based on the aggregate mass
    transaction_fees: u64,
    /// Aggregate value of all inputs
    aggregate_input_value: u64,
    /// Optional change output value
    change_output_value: Option<u64>,
}

impl Data {
    fn new(calc: &MassCalculator) -> Self {
        let aggregate_non_contextual_masses = calc.blank_transaction_non_contextual_masses();

        Data {
            inputs: vec![],
            utxo_entry_references: vec![],
            addresses: HashSet::default(),
            aggregate_non_contextual_masses,
            transaction_fees: 0,
            aggregate_input_value: 0,
            change_output_value: None,
        }
    }
}

/// Helper struct for passing around transaction value
#[derive(Debug)]
struct FinalTransaction {
    /// Total output value required for the final transaction
    value_no_fees: u64,
    /// Total output value required for the final transaction + priority fees
    value_with_priority_fee: u64,
}

/// Helper struct for obtaining properties related to
/// transaction mass calculations.
struct MassDisposition {
    /// Transaction mass derived from compute and storage mass
    transaction_mass: u64,
    /// Calculated storage mass
    storage_mass: u64,
    /// Calculated transaction fees
    transaction_fees: u64,
    /// Flag signaling that computed values require change to be absorbed to fees.
    /// This occurs when the change is dust or the change is below the fees
    /// produced by the storage mass.
    absorb_change_to_fees: bool,
    // TODO(post-toccata): remove this field
    pre_toccata_non_contextual_mass_exceeded: bool,
}

///
///  Internal Generator settings and references
///
struct Inner {
    // Atomic abortable trigger that will cause the processing to halt with `Error::Aborted`
    abortable: Option<Abortable>,
    // Optional signer that is passed on to the [`PendingTransaction`] allowing [`PendingTransaction`] to expose signing functions for convenience.
    signer: Option<Arc<dyn SignerT>>,
    // Internal mass calculator (pre-configured with network params)
    mass_calculator: MassCalculator,
    // Current network id
    network_id: NetworkId,
    // Current network params
    network_params: &'static NetworkParams,
    // transaction version
    version: u16,
    // Source Utxo Context (Used for source UtxoEntry aggregation)
    source_utxo_context: Option<UtxoContext>,
    // Destination Utxo Context (Used only during transfer transactions)
    destination_utxo_context: Option<UtxoContext>,
    // Event multiplexer
    multiplexer: Option<Multiplexer<Box<Events>>>,
    // input script execution budget commitment
    compute_commit: ComputeCommit,
    // number of minimum signatures required to sign the transaction
    minimum_signatures: u16,
    // change address
    change_address: Address,
    // non-contextual mass of a standard change output
    standard_change_output_non_contextual_masses: NonContextualMasses,
    // fee rate
    fee_rate: Option<f64>,
    /// None means sweep all UTXOs to change address
    ///
    /// Some means user requested a payment destination
    final_transaction: Option<FinalTransaction>,
    // applies only to the final transaction
    final_transaction_priority_fee: Fees,
    // issued only in the final transaction
    final_transaction_outputs: Vec<TransactionOutput>,
    // pre-calculated partial harmonic for user outputs (does not include change)
    final_transaction_outputs_harmonic: u64,
    // non-contextual mass of the final transaction outputs and payload
    final_transaction_non_contextual_masses: NonContextualMasses,
    // final transaction payload
    final_transaction_payload: Vec<u8>,
    // execution context
    context: Mutex<Context>,
    // @TODO(post-toccata): remove
    // TODO: ideally this gets replaced with all used/hardcoded parameter
    // so the engine doesn't implement specificities within it
    is_toccata_active: bool,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("network_id", &self.network_id)
            .field("network_params", &self.network_params)
            .field("version", &self.version)
            // .field("source_utxo_context", &self.source_utxo_context)
            // .field("destination_utxo_context", &self.destination_utxo_context)
            // .field("multiplexer", &self.multiplexer)
            .field("compute_commit", &self.compute_commit)
            .field("minimum_signatures", &self.minimum_signatures)
            .field("change_address", &self.change_address)
            .field("standard_change_output_non_contextual_masses", &self.standard_change_output_non_contextual_masses)
            // .field("final_transaction", &self.final_transaction)
            .field("fee_rate", &self.fee_rate)
            .field("final_transaction_priority_fee", &self.final_transaction_priority_fee)
            .field("final_transaction_outputs", &self.final_transaction_outputs)
            .field("final_transaction_outputs_harmonic", &self.final_transaction_outputs_harmonic)
            .field("final_transaction_non_contextual_masses", &self.final_transaction_non_contextual_masses)
            .field("final_transaction_payload", &self.final_transaction_payload)
            // .field("context", &self.context)
            .finish()
    }
}

///
/// Transaction generator
///
#[derive(Clone)]
pub struct Generator {
    inner: Arc<Inner>,
}

impl Generator {
    /// Create a new [`Generator`] instance using [`GeneratorSettings`].
    pub fn try_new(settings: GeneratorSettings, signer: Option<Arc<dyn SignerT>>, abortable: Option<&Abortable>) -> Result<Self> {
        let GeneratorSettings {
            version,
            network_id,
            multiplexer,
            utxo_iterator,
            source_utxo_context: utxo_context,
            priority_utxo_entries,
            compute_commit,
            minimum_signatures,
            change_address,
            fee_rate,
            final_transaction_priority_fee,
            final_transaction_destination,
            final_transaction_payload,
            destination_utxo_context,
            is_toccata_active,
        } = settings;

        let network_type = NetworkType::from(network_id);
        let network_params = NetworkParams::from(network_id);
        let mass_calculator = MassCalculator::new(&network_id.into());

        if ComputeCommit::version_expects_compute_budget_field(version) && compute_commit.compute_budget().is_none() {
            return Err(Error::custom(format!("transaction version {version} requires computeBudget commit")));
        }

        if ComputeCommit::version_expects_sig_op_count_field(version) && compute_commit.sig_op_count().is_none() {
            return Err(Error::custom(format!("transaction version {version} requires sigOpCount commit")));
        }

        let (final_transaction_outputs, final_transaction_amount) = match final_transaction_destination {
            PaymentDestination::Change => {
                if !final_transaction_priority_fee.is_none() {
                    return Err(Error::GeneratorFeesInSweepTransaction);
                }

                (vec![], None)
            }
            PaymentDestination::PaymentOutputs(outputs) => {
                // sanity checks
                if final_transaction_priority_fee.is_none() {
                    return Err(Error::GeneratorNoFeesForFinalTransaction);
                }

                for output in outputs.iter() {
                    if NetworkType::try_from(output.address.prefix)? != network_type {
                        return Err(Error::GeneratorPaymentOutputNetworkTypeMismatch);
                    }
                    if output.amount == 0 {
                        return Err(Error::GeneratorPaymentOutputZeroAmount);
                    }
                    if output.covenant.is_some() {
                        return Err(Error::GeneratorPaymentOutputCovenantNotAllowed);
                    }
                }

                (
                    outputs
                        .iter()
                        .map(|output| TransactionOutput::new(output.amount, pay_to_address_script(&output.address)))
                        .collect(),
                    Some(outputs.iter().map(|output| output.amount).sum()),
                )
            }
        };

        if final_transaction_outputs.is_empty() && matches!(final_transaction_priority_fee, Fees::ReceiverPays(_)) {
            return Err(Error::GeneratorIncludeFeesRequiresOneOutput);
        }

        // sanity check
        if NetworkType::try_from(change_address.prefix)? != network_type {
            return Err(Error::GeneratorChangeAddressNetworkTypeMismatch);
        }

        let standard_change_output_non_contextual_masses = mass_calculator.calc_non_contextual_masses_for_client_transaction_output(
            &TransactionOutput::new(0, pay_to_address_script(&change_address)),
        );
        let final_transaction_outputs_non_contextual_masses =
            mass_calculator.calc_non_contextual_masses_for_client_transaction_outputs(&final_transaction_outputs);
        let final_transaction_payload = final_transaction_payload.unwrap_or_default();
        let final_transaction_payload_non_contextual_masses =
            mass_calculator.calc_non_contextual_masses_for_payload(final_transaction_payload.len());
        let final_transaction_non_contextual_masses =
            final_transaction_outputs_non_contextual_masses + final_transaction_payload_non_contextual_masses;
        let final_transaction_outputs_harmonic =
            mass_calculator.calc_storage_mass_output_harmonic(&final_transaction_outputs).ok_or(Error::MassCalculationError)?;

        let final_transaction = final_transaction_amount.map(|amount| FinalTransaction {
            value_no_fees: amount,
            value_with_priority_fee: amount + final_transaction_priority_fee.additional(),
        });

        let mass_sanity_check_non_contextual_masses =
            standard_change_output_non_contextual_masses + final_transaction_non_contextual_masses;
        let mass_sanity_check = mass_calculator.calc_standard_non_contextual_mass(&mass_sanity_check_non_contextual_masses);

        let max_sanity_check_mass = if is_toccata_active {
            MAXIMUM_TRANSACTION_CRITERIA_MASS_SANITY_CHECK_POST_TOCCATA
        } else {
            MAXIMUM_TRANSACTION_CRITERIA_MASS_SANITY_CHECK_PRE_TOCCATA
        };

        // check for old limit per dimensions
        if !is_toccata_active {
            if mass_sanity_check_non_contextual_masses.compute_mass > MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA {
                return Err(Error::GeneratorTransactionOutputsAreTooHeavy {
                    mass: mass_sanity_check_non_contextual_masses.compute_mass,
                    kind: "compute mass",
                });
            }
            if mass_sanity_check_non_contextual_masses.transient_mass > MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA {
                return Err(Error::GeneratorTransactionOutputsAreTooHeavy {
                    mass: mass_sanity_check_non_contextual_masses.transient_mass,
                    kind: "transient mass",
                });
            }
        }

        // reject fixed outputs/payload that leave too little mass budget for inputs
        if mass_sanity_check > max_sanity_check_mass {
            return Err(Error::GeneratorTransactionOutputsAreTooHeavy { mass: mass_sanity_check, kind: "compute mass" });
        }

        let priority_utxo_entry_filter = priority_utxo_entries.as_ref().map(|entries| entries.iter().cloned().collect());
        // remap to VecDeque as this list gets drained
        let priority_utxo_entries = priority_utxo_entries.map(|entries| entries.into_iter().collect::<VecDeque<_>>());

        let context = Mutex::new(Context {
            utxo_source_iterator: utxo_iterator,
            priority_utxo_entries,
            priority_utxo_entry_filter,
            number_of_stages: 0,
            number_of_transactions: 0,
            aggregated_utxos: 0,
            aggregate_fees: 0,
            aggregate_mass: 0,
            stage: Some(Box::default()),
            utxo_stash: VecDeque::default(),
            final_transaction_id: None,
            is_done: false,
        });

        let inner = Inner {
            network_id,
            network_params,
            multiplexer,
            context,
            signer,
            abortable: abortable.cloned(),
            mass_calculator,
            source_utxo_context: utxo_context,
            version,
            compute_commit,
            minimum_signatures,
            change_address,
            standard_change_output_non_contextual_masses,
            fee_rate,
            final_transaction,
            final_transaction_priority_fee,
            final_transaction_outputs,
            final_transaction_outputs_harmonic,
            final_transaction_non_contextual_masses,
            final_transaction_payload,
            destination_utxo_context,
            is_toccata_active,
        };

        Ok(Self { inner: Arc::new(inner) })
    }

    /// Returns the current [`NetworkType`]
    #[inline(always)]
    pub fn network_type(&self) -> NetworkType {
        self.inner.network_id.into()
    }

    /// Returns the current [`NetworkId`]
    #[inline(always)]
    pub fn network_id(&self) -> NetworkId {
        self.inner.network_id
    }

    /// Returns current [`NetworkParams`]
    #[inline(always)]
    pub fn network_params(&self) -> &NetworkParams {
        self.inner.network_params
    }

    /// Returns owned mass calculator instance (bound to [`NetworkParams`] of the [`Generator`])
    #[inline(always)]
    pub fn mass_calculator(&self) -> &MassCalculator {
        &self.inner.mass_calculator
    }

    #[inline(always)]
    pub fn compute_commit(&self) -> ComputeCommit {
        self.inner.compute_commit
    }

    /// The underlying [`UtxoContext`] (if available).
    #[inline(always)]
    pub fn source_utxo_context(&self) -> &Option<UtxoContext> {
        &self.inner.source_utxo_context
    }

    /// Signifies that the transaction is a transfer between accounts
    #[inline(always)]
    pub fn destination_utxo_context(&self) -> &Option<UtxoContext> {
        &self.inner.destination_utxo_context
    }

    /// Core [`Multiplexer<Events>`] (if available)
    #[inline(always)]
    pub fn multiplexer(&self) -> &Option<Multiplexer<Box<Events>>> {
        &self.inner.multiplexer
    }

    /// Mutable context used by the generator to track state
    #[inline(always)]
    fn context(&self) -> MutexGuard<'_, Context> {
        self.inner.context.lock().unwrap()
    }

    /// Returns the underlying instance of the [Signer](SignerT)
    #[inline(always)]
    pub(crate) fn signer(&self) -> &Option<Arc<dyn SignerT>> {
        &self.inner.signer
    }

    /// The total amount of fees in SOMPI consumed during the transaction generation process.
    #[inline(always)]
    pub fn aggregate_fees(&self) -> u64 {
        self.context().aggregate_fees
    }

    /// The total number of UTXOs consumed during the transaction generation process.
    #[inline(always)]
    pub fn aggregate_utxos(&self) -> usize {
        self.context().aggregated_utxos
    }

    /// The final transaction amount (if available).
    #[inline(always)]
    pub fn final_transaction_value_no_fees(&self) -> Option<u64> {
        self.inner.final_transaction.as_ref().map(|final_transaction| final_transaction.value_no_fees)
    }

    /// Returns the final transaction id if the generator has finished successfully.
    #[inline(always)]
    pub fn final_transaction_id(&self) -> Option<TransactionId> {
        self.context().final_transaction_id
    }

    /// Returns an async Stream causes the [Generator] to produce
    /// transaction for each stream item request. NOTE: transactions
    /// are generated only when each stream item is polled.
    #[inline(always)]
    pub fn stream(&self) -> impl Stream<Item = Result<PendingTransaction>> + 'static {
        Box::pin(PendingTransactionStream::new(self))
    }

    /// Returns an iterator that causes the [Generator] to produce
    /// transaction for each iterator poll request. NOTE: transactions
    /// are generated only when the returned iterator is iterated.
    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = Result<PendingTransaction>> {
        PendingTransactionIterator::new(self)
    }

    /// Get next UTXO entry. This function obtains UTXO in the following order:
    /// 1. From the UTXO stash (used to store UTxOs that were consumed during previous transaction generation but were rejected due to various conditions, such as mass overflow)
    /// 2. From the current stage
    /// 3. From priority UTXO entries
    /// 4. From the UTXO source iterator (while filtering against priority UTXO entries)
    ///
    /// It skips entries that has a covenant_id defined, this is to avoid unintended lineage discontinuation
    fn get_utxo_entry(&self, context: &mut Context, stage: &mut Stage) -> Option<UtxoEntryReference> {
        loop {
            // 1. from the stash
            if let Some(utxo_entry) = context.utxo_stash.pop_front() {
                if utxo_entry.as_ref().covenant_id.is_none() {
                    return Some(utxo_entry);
                }
                continue;
            }

            // 2. from the current stage
            if let Some(utxo_entry) = stage.utxo_iterator.as_mut().and_then(|utxo_stage_iterator| utxo_stage_iterator.next()) {
                if utxo_entry.as_ref().covenant_id.is_none() {
                    return Some(utxo_entry);
                }
                continue;
            }

            // 3. from priority entries
            if let Some(utxo_entry) = context.priority_utxo_entries.as_mut().and_then(|entries| entries.pop_front()) {
                if utxo_entry.as_ref().covenant_id.is_none() {
                    return Some(utxo_entry);
                }
                continue;
            }

            // 4. from utxo source
            let utxo_entry = context.utxo_source_iterator.next()?;

            if let Some(filter) = context.priority_utxo_entry_filter.as_ref()
                && filter.contains(&utxo_entry)
            {
                // skip the entry from the iterator intake
                // if it has been supplied as a priority entry
                continue;
            }

            if utxo_entry.as_ref().covenant_id.is_some() {
                continue;
            }

            return Some(utxo_entry);
        }
    }

    /// Adds a [`UtxoEntryReference`] to the UTXO stash. UTXO stash
    /// is the first source of UTXO entries.
    pub fn stash(&self, into_iter: impl IntoIterator<Item = UtxoEntryReference>) {
        self.context().utxo_stash.extend(into_iter);
    }

    /// Calculates fees for the current aggregate plus a standard change output
    #[inline(always)]
    fn calc_relay_fees_with_standard_change_output(&self, data: &Data, additional_relay_mass: u64) -> u64 {
        let non_contextual_masses = data.aggregate_non_contextual_masses + self.inner.standard_change_output_non_contextual_masses;
        let relay_mass =
            self.inner.mass_calculator.calc_standard_non_contextual_mass(&non_contextual_masses).saturating_add(additional_relay_mass);
        self.calc_relay_fees_for_mass(relay_mass)
    }

    /// Returns the maximum standard transaction mass
    #[inline(always)]
    fn maximum_standard_transaction_mass(&self) -> u64 {
        if self.inner.is_toccata_active {
            MAXIMUM_STANDARD_TRANSACTION_MASS_POST_TOCCATA
        } else {
            MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA
        }
    }

    #[inline(always)]
    // @TODO(post-toccata): remove
    fn exceeds_pre_toccata_raw_non_contextual_limit(&self, masses: &NonContextualMasses) -> bool {
        !self.inner.is_toccata_active
            && (masses.compute_mass > MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA
                || masses.transient_mass > MAXIMUM_STANDARD_TRANSACTION_MASS_PRE_TOCCATA)
    }

    /// Calculates fees from an estimated non-contextual mass before a concrete transaction is built.
    /// This is useful while deciding whether a candidate reaches a relay boundary.
    /// Once concrete outputs are known, use `calc_transaction_fees_for_outputs`.
    fn calc_relay_fees_for_mass(&self, relay_mass: u64) -> u64 {
        self.inner.mass_calculator.calc_minimum_transaction_fee_from_mass(relay_mass).max(self.calc_fee_rate(relay_mass))
    }

    /// Calculates fees after concrete outputs are selected and contextual storage mass is known.
    fn calc_transaction_fees_for_outputs(
        &self,
        data: &Data,
        outputs: Vec<TransactionOutput>,
        payload: &[u8],
        transaction_mass: u64,
        additional_relay_mass: u64,
    ) -> Result<u64> {
        let tx = Transaction::new(self.inner.version, data.inputs.clone(), outputs, 0, SUBNETWORK_ID_NATIVE, 0, payload.to_vec());
        let masses = self.inner.mass_calculator.calc_unsigned_consensus_transaction_masses(
            &tx,
            &data.utxo_entry_references,
            self.inner.minimum_signatures,
        )?;
        let relay_fee = self.inner.mass_calculator.calc_minimum_relay_fee_with_additional_mass(&masses, additional_relay_mass);

        // TODO(wallet-storage-mass-inconcistency): Price fee_rate from the exact mass calculated above
        Ok(relay_fee.max(self.calc_fee_rate(transaction_mass)))
    }

    /// Main UTXO entry processing loop. This function sources UTXOs from [`Generator::get_utxo_entry()`] and
    /// accumulates consumed UTXO entry data within the [`Context`], [`Stage`] and [`Data`] structures.
    ///
    /// The general processing pattern can be described as follows:
    ///
    /**
    loop {
       1. Obtain UTXO entry from [`Generator::get_utxo_entry()`]
       2. Check if UTXO entries have been depleted, if so, handle sweep processing.
       3. Create a new Input for the transaction from the UTXO entry.
       4. Check if the transaction mass threshold has been reached, if so, yield the transaction.
       5. Register input with the [`Data`] structures.
       6. Check if the final transaction amount has been reached, if so, yield the transaction.

    }
    */
    fn generate_transaction_data(&self, context: &mut Context, stage: &mut Stage) -> Result<(DataKind, Data)> {
        let calc = &self.inner.mass_calculator;
        let mut data = Data::new(calc);

        let max_tx_mass_boundary = if self.inner.is_toccata_active {
            TRANSACTION_MASS_BOUNDARY_FOR_STAGE_INPUT_ACCUMULATION_POST_TOCCATA
        } else {
            TRANSACTION_MASS_BOUNDARY_FOR_STAGE_INPUT_ACCUMULATION_PRE_TOCCATA
        };

        loop {
            if let Some(abortable) = self.inner.abortable.as_ref() {
                abortable.check()?;
            }

            let utxo_entry_reference = if let Some(utxo_entry_reference) = self.get_utxo_entry(context, stage) {
                utxo_entry_reference
            } else {
                // no more UTXO to consume

                // if user demanded a payment request, it cannot be fulfilled
                // else, it's a change request, finish the compound
                if let Some(final_transaction) = &self.inner.final_transaction {
                    // reject transaction
                    return Err(Error::InsufficientFunds {
                        additional_needed: final_transaction.value_with_priority_fee.saturating_sub(stage.aggregate_input_value),
                        origin: "accumulator",
                    });
                } else {
                    // finish compound
                    return self.finish_relay_stage_processing(context, stage, data);
                }
            };

            // while aggregate succeed, it returns None. if it need to compound because an entry doesn't fit, it returns a Node.
            if let Some(node) = self.aggregate_utxo(context, calc, stage, &mut data, utxo_entry_reference) {
                return Ok((node, data));
            }

            if let Some(final_transaction) = &self.inner.final_transaction {
                // try finish a stage or produce a final transaction with target value
                // use basic condition checks to avoid unnecessary processing
                if (calc.calc_standard_non_contextual_mass(&data.aggregate_non_contextual_masses) > max_tx_mass_boundary
                    || (self.inner.final_transaction_priority_fee.sender_pays()
                        && stage.aggregate_input_value >= final_transaction.value_with_priority_fee)
                    || (self.inner.final_transaction_priority_fee.receiver_pays()
                        && stage.aggregate_input_value >= final_transaction.value_no_fees.saturating_sub(context.aggregate_fees)))
                    && let Some(kind) = self.try_finish_standard_stage_processing(context, stage, &mut data, final_transaction)?
                {
                    return Ok((kind, data));
                }
            }
        }
    }

    /// Test if the current state has additional UTXOs. Use with caution as this
    /// function polls the iterator and relocates UTXO into UTXO stash.
    fn has_utxo_entries(&self, context: &mut Context, stage: &mut Stage) -> bool {
        if let Some(utxo_entry_reference) = self.get_utxo_entry(context, stage) {
            context.utxo_stash.push_back(utxo_entry_reference);
            true
        } else {
            false
        }
    }

    /// Add a single input (UTXO) to the transaction accumulator.
    fn aggregate_utxo(
        &self,
        context: &mut Context,
        calc: &MassCalculator,
        stage: &mut Stage,
        data: &mut Data,
        utxo_entry_reference: UtxoEntryReference,
    ) -> Option<DataKind> {
        let UtxoEntryReference { utxo } = &utxo_entry_reference;

        let input = TransactionInput::new_with_mass(utxo.outpoint.clone().into(), vec![], 0, self.inner.compute_commit);
        let input_amount = utxo.amount();
        // input
        let input_non_contextual_masses =
            calc.calc_non_contextual_masses_for_client_transaction_input(&input, self.inner.version, self.inner.minimum_signatures);
        // input + aggregated + change output
        let candidate_relay_non_contextual_masses = data.aggregate_non_contextual_masses
            + input_non_contextual_masses
            + self.inner.standard_change_output_non_contextual_masses;

        let candidate_relay_mass = calc
            .calc_standard_non_contextual_mass(&candidate_relay_non_contextual_masses)
            .saturating_add(self.inner.network_params.additional_compound_transaction_mass());

        let max_std_mass = self.maximum_standard_transaction_mass();

        if candidate_relay_mass > max_std_mass
            || self.exceeds_pre_toccata_raw_non_contextual_limit(&candidate_relay_non_contextual_masses)
        {
            // we're full, stash the tested entry, prepare data, stage and context for a compound
            context.utxo_stash.push_back(utxo_entry_reference);
            data.transaction_fees = self
                .calc_relay_fees_with_standard_change_output(data, self.inner.network_params.additional_compound_transaction_mass());
            data.aggregate_non_contextual_masses += self.inner.standard_change_output_non_contextual_masses;
            stage.aggregate_fees += data.transaction_fees;
            context.aggregate_fees += data.transaction_fees;
            Some(DataKind::Node)
        } else {
            // entry fits, prepare the entry for inclusion in the aggregator
            context.aggregated_utxos += 1;
            stage.aggregate_input_value += input_amount;
            data.aggregate_input_value += input_amount;
            data.aggregate_non_contextual_masses += input_non_contextual_masses;
            data.utxo_entry_references.push(utxo_entry_reference.clone());
            data.inputs.push(input);
            if let Some(address) = utxo.address.as_ref() {
                data.addresses.insert(address.clone());
            }
            None
        }
    }

    /// Resolves sweep/compound generation once no more source UTXOs are available.
    ///
    /// one of: drops the attempt when consolidation cannot operate (noop),
    ///         closes an in-progress compound stage (edge),
    ///         or emits the final compound transaction (final)
    fn finish_relay_stage_processing(&self, context: &mut Context, stage: &mut Stage, mut data: Data) -> Result<(DataKind, Data)> {
        if context.aggregated_utxos < 2 {
            Ok((DataKind::NoOp, data))
        } else if stage.number_of_transactions > 0 {
            data.transaction_fees = self
                .calc_relay_fees_with_standard_change_output(&data, self.inner.network_params.additional_compound_transaction_mass());
            stage.aggregate_fees += data.transaction_fees;
            context.aggregate_fees += data.transaction_fees;
            data.aggregate_non_contextual_masses += self.inner.standard_change_output_non_contextual_masses;
            Ok((DataKind::Edge, data))
        } else {
            data.transaction_fees = self.calc_relay_fees_with_standard_change_output(&data, 0);
            stage.aggregate_fees += data.transaction_fees;
            context.aggregate_fees += data.transaction_fees;

            if data.aggregate_input_value < data.transaction_fees {
                return Err(Error::InsufficientFunds {
                    additional_needed: data.transaction_fees - data.aggregate_input_value,
                    origin: "relay",
                });
            }

            let change_output_value = data.aggregate_input_value - data.transaction_fees;

            if self.inner.mass_calculator.is_dust(change_output_value) {
                // sweep transaction resulting in dust output
                Ok((DataKind::NoOp, data))
            } else {
                data.aggregate_non_contextual_masses += self.inner.standard_change_output_non_contextual_masses;
                data.change_output_value = Some(change_output_value);
                Ok((DataKind::Final, data))
            }
        }
    }

    /// Calculate storage mass using inputs from `Data`
    /// and `output_harmonics` supplied by the user
    fn calc_storage_mass(&self, data: &Data, output_harmonics: u64) -> u64 {
        let calc = &self.inner.mass_calculator;
        // TODO(wallet-storage-mass-inconcistency): Remove this helper from generator decisions. Once candidate
        // outputs are known, calculate storage mass from the concrete outputs
        calc.calc_storage_mass(output_harmonics, data.aggregate_input_value, data.inputs.len() as u64)
    }

    fn calc_fee_rate(&self, mass: u64) -> u64 {
        self.inner.fee_rate.map(|fee_rate| (fee_rate * mass as f64) as u64).unwrap_or(0)
    }

    /// Check if the current state has sufficient funds for the final transaction,
    /// initiate new stage if necessary, or finish stage processing creating the
    /// final transaction.
    fn try_finish_standard_stage_processing(
        &self,
        context: &mut Context,
        stage: &mut Stage,
        data: &mut Data,
        final_transaction: &FinalTransaction,
    ) -> Result<Option<DataKind>> {
        // calculate storage mass
        // TODO(wallet-storage-mass-inconcistency): use exact candidate storage mass before
        // deciding whether this candidate can be finalized or must close the stage as an edge
        let MassDisposition {
            transaction_mass,
            storage_mass,
            transaction_fees,
            absorb_change_to_fees,
            pre_toccata_non_contextual_mass_exceeded,
        } = self.calculate_mass(stage, data, final_transaction.value_with_priority_fee)?;

        let total_stage_value_needed = if self.inner.final_transaction_priority_fee.sender_pays() {
            final_transaction.value_with_priority_fee + stage.aggregate_fees + transaction_fees
        } else {
            final_transaction.value_with_priority_fee
        };

        let reject = match self.inner.final_transaction_priority_fee {
            Fees::SenderPays(_) => stage.aggregate_input_value < total_stage_value_needed,
            Fees::ReceiverPays(_) => stage.aggregate_input_value + context.aggregate_fees < total_stage_value_needed,
            Fees::None => unreachable!("Fees::None can not occur for final transaction"),
        };

        let max_std_mass = self.maximum_standard_transaction_mass();

        let max_tx_mass_for_additional_inputs = if self.inner.is_toccata_active {
            TRANSACTION_MASS_BOUNDARY_FOR_ADDITIONAL_INPUT_ACCUMULATION_POST_TOCCATA
        } else {
            TRANSACTION_MASS_BOUNDARY_FOR_ADDITIONAL_INPUT_ACCUMULATION_PRE_TOCCATA
        };

        if reject {
            // need more value, reject finalization (try adding more inputs)
            Ok(None)
        } else if pre_toccata_non_contextual_mass_exceeded || transaction_mass > max_std_mass || stage.number_of_transactions > 0 {
            self.generate_edge_transaction(context, stage, data)
        } else {
            // ---
            // attempt to aggregate additional UTXOs in an effort to have more inputs and lower storage mass
            // TODO - discuss:
            // this is of questionable value as this can result in both positive and negative impact,
            // also doing this can result in reduction of the wallet UTXO set, which later results
            // in additional fees for the user.
            if storage_mass > 0
                && data.inputs.len() < self.inner.final_transaction_outputs.len() * 2
                && transaction_mass < max_tx_mass_for_additional_inputs
            {
                // fetch UTXO from the iterator and if exists, make it available on the next iteration via utxo_stash.
                if self.has_utxo_entries(context, stage) {
                    return Ok(None);
                }
            }
            // ---

            let (mut transaction_fees, change_output_value) = match self.inner.final_transaction_priority_fee {
                Fees::SenderPays(priority_fees) => {
                    let transaction_fees = transaction_fees + priority_fees;
                    let change_output_value = data.aggregate_input_value - final_transaction.value_no_fees - transaction_fees;
                    (transaction_fees, change_output_value)
                }
                // TODO - currently unreachable at the API level
                Fees::ReceiverPays(priority_fees) => {
                    let transaction_fees = transaction_fees + priority_fees;
                    let change_output_value = data.aggregate_input_value.saturating_sub(final_transaction.value_no_fees);
                    (transaction_fees, change_output_value)
                }
                Fees::None => unreachable!("Fees::None is not allowed for final transactions"),
            };

            // checks output dust threshold in network params
            // note: if it's final, we will generate_transaction, the real final mass will be calculated
            //
            //
            // if is_dust(&self.inner.network_params, change_output_value) {
            if absorb_change_to_fees || change_output_value == 0 {
                transaction_fees += change_output_value;

                data.transaction_fees = transaction_fees;
                stage.aggregate_fees += transaction_fees;
                context.aggregate_fees += transaction_fees;

                Ok(Some(DataKind::Final))
            } else {
                data.transaction_fees = transaction_fees;
                stage.aggregate_fees += transaction_fees;
                context.aggregate_fees += transaction_fees;
                data.change_output_value = Some(change_output_value);

                Ok(Some(DataKind::Final))
            }
        }
    }

    fn calculate_mass(&self, stage: &Stage, data: &Data, transaction_target_value: u64) -> Result<MassDisposition> {
        let calc = &self.inner.mass_calculator;

        let mut absorb_change_to_fees = false;

        let non_contextual_masses_no_change =
            data.aggregate_non_contextual_masses + self.inner.final_transaction_non_contextual_masses;
        let non_contextual_masses_with_change =
            non_contextual_masses_no_change + self.inner.standard_change_output_non_contextual_masses;
        let normalized_non_contextual_mass_with_change = calc.calc_standard_non_contextual_mass(&non_contextual_masses_with_change);

        let mut selected_non_contextual_masses = non_contextual_masses_no_change;
        let mut selected_outputs = self.inner.final_transaction_outputs.clone();
        let mut selected_payload = self.inner.final_transaction_payload.as_slice();

        let mut additional_relay_mass = 0;

        // TODO(wallet-storage-mass-inconcistency): build the candidate outputs first and calculate exact storage mass
        let storage_mass = if stage.number_of_transactions > 0 {
            // calculate for edge transaction boundaries
            // we know that stage.number_of_transactions > 0 will trigger stage generation
            let edge_non_contextual_masses =
                data.aggregate_non_contextual_masses + self.inner.standard_change_output_non_contextual_masses;
            additional_relay_mass = self.inner.network_params.additional_compound_transaction_mass();
            let edge_mass = calc.calc_standard_non_contextual_mass(&edge_non_contextual_masses).saturating_add(additional_relay_mass);
            let edge_fees = self.calc_relay_fees_for_mass(edge_mass);
            let edge_output_value = data.aggregate_input_value.saturating_sub(edge_fees);
            if edge_output_value != 0 {
                selected_outputs = vec![TransactionOutput::new(edge_output_value, pay_to_address_script(&self.inner.change_address))];
                selected_payload = &[];
                selected_non_contextual_masses = edge_non_contextual_masses;
                let edge_output_harmonic = calc.calc_storage_mass_output_harmonic_single(edge_output_value);
                self.calc_storage_mass(data, edge_output_harmonic)
            } else {
                0
            }
        } else if data.aggregate_input_value <= transaction_target_value {
            // calculate for final transaction boundaries
            self.calc_storage_mass(data, self.inner.final_transaction_outputs_harmonic)
        } else {
            // calculate for final transaction boundaries
            let change_value = data.aggregate_input_value - transaction_target_value;

            if self.inner.mass_calculator.is_dust(change_value) {
                absorb_change_to_fees = true;
                self.calc_storage_mass(data, self.inner.final_transaction_outputs_harmonic)
            } else {
                let mut outputs_with_change = self.inner.final_transaction_outputs.clone();
                outputs_with_change.push(TransactionOutput::new(change_value, pay_to_address_script(&self.inner.change_address)));
                let output_harmonic_with_change =
                    calc.calc_storage_mass_output_harmonic_single(change_value) + self.inner.final_transaction_outputs_harmonic;
                let storage_mass_with_change = self.calc_storage_mass(data, output_harmonic_with_change);

                // TODO - review and potentially simplify:
                // this profiles the storage mass with change and without change
                // and decides which one to use based on the fees
                if storage_mass_with_change == 0 || (storage_mass_with_change < normalized_non_contextual_mass_with_change) {
                    selected_outputs = outputs_with_change;
                    selected_non_contextual_masses = non_contextual_masses_with_change;
                    0
                } else {
                    let storage_mass_no_change = self.calc_storage_mass(data, self.inner.final_transaction_outputs_harmonic);
                    if storage_mass_with_change < storage_mass_no_change {
                        selected_outputs = outputs_with_change;
                        selected_non_contextual_masses = non_contextual_masses_with_change;
                        storage_mass_with_change
                    } else {
                        let transaction_mass_with_change =
                            calc.calc_standard_mass_for_parts(&non_contextual_masses_with_change, storage_mass_with_change);
                        let transaction_mass_no_change =
                            calc.calc_standard_mass_for_parts(&non_contextual_masses_no_change, storage_mass_no_change);
                        let fees_with_change = self.calc_transaction_fees_for_outputs(
                            data,
                            outputs_with_change.clone(),
                            &self.inner.final_transaction_payload,
                            transaction_mass_with_change,
                            0,
                        );
                        let fees_no_change = self.calc_transaction_fees_for_outputs(
                            data,
                            self.inner.final_transaction_outputs.clone(),
                            &self.inner.final_transaction_payload,
                            transaction_mass_no_change,
                            0,
                        );
                        let fees_with_change = fees_with_change?;
                        let fees_no_change = fees_no_change?;
                        let difference = fees_with_change.saturating_sub(fees_no_change);

                        if difference > change_value {
                            absorb_change_to_fees = true;
                            storage_mass_no_change
                        } else {
                            selected_outputs = outputs_with_change;
                            selected_non_contextual_masses = non_contextual_masses_with_change;
                            storage_mass_with_change
                        }
                    }
                }
            }
        };

        let max_std_mass = self.maximum_standard_transaction_mass();

        if storage_mass > max_std_mass {
            Err(Error::StorageMassExceedsMaximumTransactionMass { storage_mass })
        } else {
            if absorb_change_to_fees {
                selected_non_contextual_masses = non_contextual_masses_no_change;
            }
            let transaction_mass =
                calc.calc_standard_mass_for_parts(&selected_non_contextual_masses, storage_mass).saturating_add(additional_relay_mass);
            let pre_toccata_non_contextual_mass_exceeded =
                self.exceeds_pre_toccata_raw_non_contextual_limit(&selected_non_contextual_masses);
            let transaction_fees = self.calc_transaction_fees_for_outputs(
                data,
                selected_outputs,
                selected_payload,
                transaction_mass,
                additional_relay_mass,
            )?;

            Ok(MassDisposition {
                transaction_mass,
                transaction_fees,
                storage_mass,
                absorb_change_to_fees,
                pre_toccata_non_contextual_mass_exceeded,
            })
        }
    }

    /// Generate an `Edge` transaction. This function is called when the transaction
    /// processing has aggregated sufficient inputs to match requested outputs.
    fn generate_edge_transaction(&self, context: &mut Context, stage: &mut Stage, data: &mut Data) -> Result<Option<DataKind>> {
        let calc = &self.inner.mass_calculator;

        let edge_non_contextual_masses =
            data.aggregate_non_contextual_masses + self.inner.standard_change_output_non_contextual_masses;
        let additional_relay_mass = self.inner.network_params.additional_compound_transaction_mass();
        let edge_relay_mass =
            calc.calc_standard_non_contextual_mass(&edge_non_contextual_masses).saturating_add(additional_relay_mass);
        let edge_fees = self.calc_relay_fees_for_mass(edge_relay_mass);
        let edge_output_value = data.aggregate_input_value.saturating_sub(edge_fees);

        // TODO(wallet-storage-mass-inconcistency): Calculate this edge mass from the concrete edge output or consider removing this as calculated storage mass should produce `0` value
        let edge_output_harmonic = calc.calc_storage_mass_output_harmonic_single(edge_output_value);
        let storage_mass = self.calc_storage_mass(data, edge_output_harmonic);
        let transaction_mass =
            calc.calc_standard_mass_for_parts(&edge_non_contextual_masses, storage_mass).saturating_add(additional_relay_mass);

        let max_std_mass = self.maximum_standard_transaction_mass();

        if transaction_mass > max_std_mass || self.exceeds_pre_toccata_raw_non_contextual_limit(&edge_non_contextual_masses) {
            // transaction mass is too high... if we have additional
            // UTXOs, reject and try to accumulate more inputs...
            if self.has_utxo_entries(context, stage) {
                Ok(None)
            } else {
                // otherwise we have insufficient funds
                Err(Error::GeneratorTransactionIsTooHeavy)
            }
        } else {
            data.transaction_fees = self.calc_transaction_fees_for_outputs(
                data,
                vec![TransactionOutput::new(edge_output_value, pay_to_address_script(&self.inner.change_address))],
                &[],
                transaction_mass,
                additional_relay_mass,
            )?;
            stage.aggregate_fees += data.transaction_fees;
            context.aggregate_fees += data.transaction_fees;
            Ok(Some(DataKind::Edge))
        }
    }

    /// Generates a single transaction by draining the supplied UTXO iterator.
    /// This function is used by the by the available async Stream and Iterator
    /// implementations to generate a stream of transactions.
    ///
    /// This function returns `None` once the supplied UTXO iterator is depleted.
    ///
    /// This function runs a continuous loop by ingesting inputs from the UTXO
    /// iterator, analyzing the resulting transaction mass, and either producing
    /// an intermediate "batch" transaction sending funds to the change address
    /// or creating a final transaction with the requested set of outputs and the
    /// payload.
    pub fn generate_transaction(&self) -> Result<Option<PendingTransaction>> {
        let mut context = self.context();

        if context.is_done {
            return Ok(None);
        }

        let mut stage = context.stage.take().unwrap();
        let (kind, data) = self.generate_transaction_data(&mut context, &mut stage)?;
        context.stage.replace(stage);

        let max_std_mass = self.maximum_standard_transaction_mass();

        match (kind, data) {
            (DataKind::NoOp, _) => {
                context.is_done = true;
                context.stage.take();
                Ok(None)
            }
            (DataKind::Final, data) => {
                context.is_done = true;
                context.stage.take();

                let Data {
                    inputs,
                    utxo_entry_references,
                    addresses,
                    aggregate_input_value,
                    change_output_value,
                    transaction_fees,
                    ..
                } = data;

                let change_output_value = change_output_value.unwrap_or(0);

                let mut final_outputs = self.inner.final_transaction_outputs.clone();

                if self.inner.final_transaction_priority_fee.receiver_pays() {
                    let output = final_outputs.get_mut(0).expect("include fees requires one output");
                    if aggregate_input_value < output.value {
                        output.value = aggregate_input_value - transaction_fees;
                    } else {
                        output.value -= transaction_fees;
                    }
                }

                let change_output_index = if change_output_value > 0 {
                    let change_output_index = Some(final_outputs.len());
                    final_outputs.push(TransactionOutput::new(change_output_value, pay_to_address_script(&self.inner.change_address)));
                    change_output_index
                } else {
                    None
                };

                let aggregate_output_value = final_outputs.iter().map(|output| output.value).sum::<u64>();
                // TODO - validate that this is still correct
                // `Fees::ReceiverPays` processing can result in outputs being larger than inputs
                if aggregate_output_value > aggregate_input_value {
                    return Err(Error::InsufficientFunds {
                        additional_needed: aggregate_output_value - aggregate_input_value,
                        origin: "final",
                    });
                }

                let tx = Transaction::new(
                    self.inner.version,
                    inputs,
                    final_outputs,
                    0,
                    SUBNETWORK_ID_NATIVE,
                    0,
                    self.inner.final_transaction_payload.clone(),
                );

                let masses = self.inner.mass_calculator.calc_unsigned_consensus_transaction_masses(
                    &tx,
                    &utxo_entry_references,
                    self.inner.minimum_signatures,
                )?;
                let transaction_mass = self.inner.mass_calculator.calc_standard_mass(&masses);
                if transaction_mass > max_std_mass || self.exceeds_pre_toccata_raw_non_contextual_limit(&masses.non_contextual) {
                    // this should never occur as we should not produce transactions higher than the mass limit
                    return Err(Error::MassCalculationError);
                }
                tx.set_storage_mass(masses.contextual.storage_mass);

                context.aggregate_mass += transaction_mass;
                context.final_transaction_id = Some(tx.id());
                context.number_of_stages += 1;
                context.number_of_transactions += 1;

                Ok(Some(PendingTransaction::try_new(
                    self,
                    tx,
                    utxo_entry_references,
                    addresses.into_iter().collect(),
                    self.final_transaction_value_no_fees(),
                    change_output_index,
                    change_output_value,
                    aggregate_input_value,
                    aggregate_output_value,
                    self.inner.minimum_signatures,
                    transaction_mass,
                    transaction_fees,
                    kind,
                )?))
            }
            // intermediary compound transaction
            (kind, data) => {
                let Data {
                    inputs,
                    utxo_entry_references,
                    addresses,
                    aggregate_input_value,
                    transaction_fees,
                    change_output_value,
                    ..
                } = data;

                assert_eq!(change_output_value, None);

                if aggregate_input_value <= transaction_fees {
                    return Err(Error::TransactionFeesAreTooHigh);
                }

                let output_value = aggregate_input_value.saturating_sub(transaction_fees);
                let script_public_key = pay_to_address_script(&self.inner.change_address);

                let output = TransactionOutput::new(output_value, script_public_key.clone());
                let tx = Transaction::new(self.inner.version, inputs, vec![output], 0, SUBNETWORK_ID_NATIVE, 0, vec![]);

                let masses = self.inner.mass_calculator.calc_unsigned_consensus_transaction_masses(
                    &tx,
                    &utxo_entry_references,
                    self.inner.minimum_signatures,
                )?;
                let mut transaction_mass = self.inner.mass_calculator.calc_standard_mass(&masses);
                transaction_mass = transaction_mass.saturating_add(self.inner.network_params.additional_compound_transaction_mass());
                if transaction_mass > max_std_mass || self.exceeds_pre_toccata_raw_non_contextual_limit(&masses.non_contextual) {
                    // this should never occur as we should not produce transactions higher than the mass limit
                    return Err(Error::MassCalculationError);
                }
                tx.set_storage_mass(masses.contextual.storage_mass);

                context.aggregate_mass += transaction_mass;
                context.number_of_transactions += 1;

                let previous_batch_utxo_entry_reference =
                    Self::create_batch_utxo_entry_reference(tx.id(), output_value, script_public_key, &self.inner.change_address);

                match kind {
                    DataKind::Node => {
                        // store resulting UTXO in the current stage
                        let stage = context.stage.as_mut().unwrap();
                        stage.utxo_accumulator.push(previous_batch_utxo_entry_reference);
                        stage.number_of_transactions += 1;
                    }
                    DataKind::Edge => {
                        // store resulting UTXO in the current stage and create a new stage
                        let mut stage = context.stage.take().unwrap();
                        stage.utxo_accumulator.push(previous_batch_utxo_entry_reference);
                        stage.number_of_transactions += 1;
                        context.number_of_stages += 1;
                        context.stage.replace(Box::new(Stage::new(*stage)));
                    }
                    _ => unreachable!(),
                }

                Ok(Some(PendingTransaction::try_new(
                    self,
                    tx,
                    utxo_entry_references,
                    addresses.into_iter().collect(),
                    self.final_transaction_value_no_fees(),
                    None,
                    output_value,
                    aggregate_input_value,
                    output_value,
                    self.inner.minimum_signatures,
                    transaction_mass,
                    transaction_fees,
                    kind,
                )?))
            }
        }
    }

    fn create_batch_utxo_entry_reference(
        txid: TransactionId,
        amount: u64,
        script_public_key: ScriptPublicKey,
        address: &Address,
    ) -> UtxoEntryReference {
        let outpoint = TransactionOutpoint::new(txid, 0);
        let utxo = UtxoEntry {
            address: Some(address.clone()),
            outpoint: outpoint.into(),
            amount,
            script_public_key,
            block_daa_score: UNACCEPTED_DAA_SCORE,
            is_coinbase: false, // entry
            // covenant_id is not allowed by generator
            covenant_id: None,
        };
        UtxoEntryReference { utxo: Arc::new(utxo) }
    }

    /// Produces [`GeneratorSummary`] for the current state of the generator.
    /// This method is useful for creation of transaction estimations.
    pub fn summary(&self) -> GeneratorSummary {
        let context = self.context();

        GeneratorSummary {
            network_id: self.inner.network_id,
            aggregated_utxos: context.aggregated_utxos,
            aggregate_fees: context.aggregate_fees,
            aggregate_mass: context.aggregate_mass,
            final_transaction_amount: self.final_transaction_value_no_fees(),
            final_transaction_id: context.final_transaction_id,
            number_of_generated_transactions: context.number_of_transactions,
            number_of_generated_stages: context.number_of_stages,
        }
    }
}
