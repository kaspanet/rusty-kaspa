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
//! A: There are a number of requirements that need to be handled:
//!
//!   1. UTXO entry consumption while creating inputs may results in
//!   additional fees, requiring additional UTXO entries to cover
//!   the fees. Goto 1. (this is a classic issue, can be solved using padding)
//!
//!   2. The overall design strategy for this processor is to allow
//!   concurrent processing of a large number of transactions and UTXOs.
//!   This implementation avoids in-memory aggregation of all
//!   transactions that may result in OOM conditions.
//!
//!   3. If used with a large UTXO set, the transaction generation process
//!   needs to be asynchronous to avoid blocking the main thread. In the
//!   context of WASM32 SDK, not doing that while working with large
//!   UTXO sets will result in a browser UI freezing.
//!

use crate::imports::*;
use crate::result::Result;
use crate::tx::{
    mass::*, Fees, GeneratorSettings, GeneratorSummary, PaymentDestination, PendingTransaction, PendingTransactionIterator,
    PendingTransactionStream,
};
use crate::utxo::{NetworkParams, UtxoContext, UtxoEntryReference};
use kaspa_consensus_client::UtxoEntry;
use kaspa_consensus_core::constants::UNACCEPTED_DAA_SCORE;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
use kaspa_txscript::pay_to_address_script;
use std::collections::VecDeque;

use super::SignerT;

// fee reduction - when a transactions has some storage mass
// and the total mass is below this threshold (as well as
// other conditions), we attempt to accumulate additional
// inputs to reduce storage mass/fees
const TRANSACTION_MASS_BOUNDARY_FOR_ADDITIONAL_INPUT_ACCUMULATION: u64 = MAXIMUM_STANDARD_TRANSACTION_MASS / 5 * 4;
// optimization boundary - when aggregating inputs,
// we don't perform any checks until we reach this mass
// or the aggregate input amount reaches the requested
// output amount
const TRANSACTION_MASS_BOUNDARY_FOR_STAGE_INPUT_ACCUMULATION: u64 = MAXIMUM_STANDARD_TRANSACTION_MASS / 5 * 4;

/// Mutable [`Generator`] state used to track the current transaction generation process.
struct Context {
    /// iterator containing UTXO entries available for transaction generation
    utxo_source_iterator: Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static>,
    /// total number of UTXOs consumed by the single generator instance
    aggregated_utxos: usize,
    /// total fees of all transactions issued by
    /// the single generator instance
    aggregate_fees: u64,
    /// number of generated transactions
    number_of_transactions: usize,
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
    /// Aggregate transaction mass
    aggregate_mass: u64,
    /// Transaction fees based on the aggregate mass
    transaction_fees: u64,
    /// Aggregate value of all inputs
    aggregate_input_value: u64,
    /// Optional change output value
    change_output_value: Option<u64>,
}

impl Data {
    fn new(calc: &MassCalculator) -> Self {
        let aggregate_mass = calc.blank_transaction_mass();

        Data {
            inputs: vec![],
            utxo_entry_references: vec![],
            addresses: HashSet::default(),
            aggregate_mass,
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
    network_params: NetworkParams,

    // Source Utxo Context (Used for source UtxoEntry aggregation)
    source_utxo_context: Option<UtxoContext>,
    // Destination Utxo Context (Used only during transfer transactions)
    destination_utxo_context: Option<UtxoContext>,
    // Event multiplexer
    multiplexer: Option<Multiplexer<Box<Events>>>,
    // typically a number of keys required to sign the transaction
    sig_op_count: u8,
    // number of minimum signatures required to sign the transaction
    #[allow(dead_code)]
    minimum_signatures: u16,
    // change address
    change_address: Address,
    // change_output: TransactionOutput,
    standard_change_output_compute_mass: u64,
    // signature mass per input
    signature_mass_per_input: u64,
    // final transaction amount and fees
    // `None` is used for sweep transactions
    final_transaction: Option<FinalTransaction>,
    // applies only to the final transaction
    final_transaction_priority_fee: Fees,
    // issued only in the final transaction
    final_transaction_outputs: Vec<TransactionOutput>,
    // pre-calculated partial harmonic for user outputs (does not include change)
    final_transaction_outputs_harmonic: u64,
    // mass of the final transaction
    final_transaction_outputs_compute_mass: u64,
    // final transaction payload
    final_transaction_payload: Vec<u8>,
    // final transaction payload mass
    final_transaction_payload_mass: u64,
    // execution context
    context: Mutex<Context>,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("network_id", &self.network_id)
            .field("network_params", &self.network_params)
            // .field("source_utxo_context", &self.source_utxo_context)
            // .field("destination_utxo_context", &self.destination_utxo_context)
            // .field("multiplexer", &self.multiplexer)
            .field("sig_op_count", &self.sig_op_count)
            .field("minimum_signatures", &self.minimum_signatures)
            .field("change_address", &self.change_address)
            .field("standard_change_output_compute_mass", &self.standard_change_output_compute_mass)
            .field("signature_mass_per_input", &self.signature_mass_per_input)
            // .field("final_transaction", &self.final_transaction)
            .field("final_transaction_priority_fee", &self.final_transaction_priority_fee)
            .field("final_transaction_outputs", &self.final_transaction_outputs)
            .field("final_transaction_outputs_harmonic", &self.final_transaction_outputs_harmonic)
            .field("final_transaction_outputs_compute_mass", &self.final_transaction_outputs_compute_mass)
            .field("final_transaction_payload", &self.final_transaction_payload)
            .field("final_transaction_payload_mass", &self.final_transaction_payload_mass)
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
            network_id,
            multiplexer,
            utxo_iterator,
            source_utxo_context: utxo_context,
            sig_op_count,
            minimum_signatures,
            change_address,
            final_transaction_priority_fee,
            final_transaction_destination,
            final_transaction_payload,
            destination_utxo_context,
        } = settings;

        let network_type = NetworkType::from(network_id);
        let network_params = NetworkParams::from(network_id);
        let mass_calculator = MassCalculator::new(&network_id.into(), &network_params);

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

        let standard_change_output_mass =
            mass_calculator.calc_mass_for_output(&TransactionOutput::new(0, pay_to_address_script(&change_address)));
        let signature_mass_per_input = mass_calculator.calc_signature_mass(minimum_signatures);
        let final_transaction_outputs_compute_mass = mass_calculator.calc_mass_for_outputs(&final_transaction_outputs);
        let final_transaction_payload = final_transaction_payload.unwrap_or_default();
        let final_transaction_payload_mass = mass_calculator.calc_mass_for_payload(final_transaction_payload.len());
        let final_transaction_outputs_harmonic =
            mass_calculator.calc_storage_mass_output_harmonic(&final_transaction_outputs).ok_or(Error::MassCalculationError)?;

        // reject transactions where the payload and outputs are more than 2/3rds of the maximum tx mass
        let final_transaction = final_transaction_amount.map(|amount| FinalTransaction {
            value_no_fees: amount,
            value_with_priority_fee: amount + final_transaction_priority_fee.additional(),
        });

        let mass_sanity_check = standard_change_output_mass + final_transaction_outputs_compute_mass + final_transaction_payload_mass;
        if mass_sanity_check > MAXIMUM_STANDARD_TRANSACTION_MASS / 5 * 4 {
            return Err(Error::GeneratorTransactionOutputsAreTooHeavy { mass: mass_sanity_check, kind: "compute mass" });
        }

        let context = Mutex::new(Context {
            utxo_source_iterator: utxo_iterator,
            number_of_transactions: 0,
            aggregated_utxos: 0,
            aggregate_fees: 0,
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
            sig_op_count,
            minimum_signatures,
            change_address,
            standard_change_output_compute_mass: standard_change_output_mass,
            signature_mass_per_input,
            final_transaction,
            final_transaction_priority_fee,
            final_transaction_outputs,
            final_transaction_outputs_harmonic,
            final_transaction_outputs_compute_mass,
            final_transaction_payload,
            final_transaction_payload_mass,
            destination_utxo_context,
        };

        Ok(Self { inner: Arc::new(inner) })
    }

    /// Returns the current [`NetworkType`]
    pub fn network_type(&self) -> NetworkType {
        self.inner.network_id.into()
    }

    /// Returns the current [`NetworkId`]
    pub fn network_id(&self) -> NetworkId {
        self.inner.network_id
    }

    /// Returns current [`NetworkParams`]
    pub fn network_params(&self) -> &NetworkParams {
        &self.inner.network_params
    }

    /// The underlying [`UtxoContext`] (if available).
    pub fn source_utxo_context(&self) -> &Option<UtxoContext> {
        &self.inner.source_utxo_context
    }

    /// Signifies that the transaction is a transfer between accounts
    pub fn destination_utxo_context(&self) -> &Option<UtxoContext> {
        &self.inner.destination_utxo_context
    }

    /// Core [`Multiplexer<Events>`] (if available)
    pub fn multiplexer(&self) -> &Option<Multiplexer<Box<Events>>> {
        &self.inner.multiplexer
    }

    /// Mutable context used by the generator to track state
    fn context(&self) -> MutexGuard<Context> {
        self.inner.context.lock().unwrap()
    }

    /// Returns the underlying instance of the [Signer](SignerT)
    pub(crate) fn signer(&self) -> &Option<Arc<dyn SignerT>> {
        &self.inner.signer
    }

    /// The total amount of fees in SOMPI consumed during the transaction generation process.
    pub fn aggregate_fees(&self) -> u64 {
        self.context().aggregate_fees
    }

    /// The total number of UTXOs consumed during the transaction generation process.
    pub fn aggregate_utxos(&self) -> usize {
        self.context().aggregated_utxos
    }

    /// The final transaction amount (if available).
    pub fn final_transaction_value_no_fees(&self) -> Option<u64> {
        self.inner.final_transaction.as_ref().map(|final_transaction| final_transaction.value_no_fees)
    }

    /// Returns the final transaction id if the generator has finished successfully.
    pub fn final_transaction_id(&self) -> Option<TransactionId> {
        self.context().final_transaction_id
    }

    /// Returns an async Stream causes the [Generator] to produce
    /// transaction for each stream item request. NOTE: transactions
    /// are generated only when each stream item is polled.
    pub fn stream(&self) -> impl Stream<Item = Result<PendingTransaction>> {
        Box::pin(PendingTransactionStream::new(self))
    }

    /// Returns an iterator that causes the [Generator] to produce
    /// transaction for each iterator poll request. NOTE: transactions
    /// are generated only when the returned iterator is iterated.
    pub fn iter(&self) -> impl Iterator<Item = Result<PendingTransaction>> {
        PendingTransactionIterator::new(self)
    }

    /// Get next UTXO entry. This function obtains UTXO in the following order:
    /// 1. From the UTXO stash (used to store UTxOs that were not used in the previous transaction)
    /// 2. From the current stage
    /// 3. From the UTXO source iterator
    fn get_utxo_entry(&self, context: &mut Context, stage: &mut Stage) -> Option<UtxoEntryReference> {
        context
            .utxo_stash
            .pop_front()
            .or_else(|| stage.utxo_iterator.as_mut().and_then(|utxo_stage_iterator| utxo_stage_iterator.next()))
            .or_else(|| context.utxo_source_iterator.next())
    }

    /// Calculate relay transaction mass for the current transaction `data`
    fn calc_relay_transaction_mass(&self, data: &Data) -> u64 {
        data.aggregate_mass + self.inner.standard_change_output_compute_mass
    }

    /// Calculate relay transaction fees for the current transaction `data`
    fn calc_relay_transaction_compute_fees(&self, data: &Data) -> u64 {
        self.inner.mass_calculator.calc_minimum_transaction_fee_from_mass(self.calc_relay_transaction_mass(data))
    }

    /// Main UTXO entry processing loop. This function sources UTXOs from [`Generator::get_utxo_entry()`] and
    /// accumulates consumed UTXO entry data within the [`Context`], [`Stage`] and [`Data`] structures.
    ///
    /// The general processing pattern can be described as follows:
    ///
    /// loop {
    ///   1. Obtain UTXO entry from [`Generator::get_utxo_entry()`]
    ///   2. Check if UTXO entries have been depleted, if so, handle sweep processing.
    ///   3. Create a new Input for the transaction from the UTXO entry.
    ///   4. Check if the transaction mass threshold has been reached, if so, yield the transaction.
    ///   5. Register input with the [`Data`] structures.
    ///   6. Check if the final transaction amount has been reached, if so, yield the transaction.
    /// }
    ///
    ///
    fn generate_transaction_data(&self, context: &mut Context, stage: &mut Stage) -> Result<(DataKind, Data)> {
        let calc = &self.inner.mass_calculator;
        let mut data = Data::new(calc);

        loop {
            if let Some(abortable) = self.inner.abortable.as_ref() {
                abortable.check()?;
            }

            let utxo_entry_reference = if let Some(utxo_entry_reference) = self.get_utxo_entry(context, stage) {
                utxo_entry_reference
            } else {
                // UTXO sources are depleted
                if let Some(final_transaction) = &self.inner.final_transaction {
                    // reject transaction
                    return Err(Error::InsufficientFunds {
                        additional_needed: final_transaction.value_with_priority_fee.saturating_sub(stage.aggregate_input_value),
                        origin: "accumulator",
                    });
                } else {
                    // finish sweep processing
                    return self.finish_relay_stage_processing(context, stage, data);
                }
            };

            if let Some(node) = self.aggregate_utxo(context, calc, stage, &mut data, utxo_entry_reference) {
                return Ok((node, data));
            }

            if let Some(final_transaction) = &self.inner.final_transaction {
                // try finish a stage or produce a final transaction with target value
                // use basic condition checks to avoid unnecessary processing
                if data.aggregate_mass > TRANSACTION_MASS_BOUNDARY_FOR_STAGE_INPUT_ACCUMULATION
                    || (self.inner.final_transaction_priority_fee.sender_pays()
                        && stage.aggregate_input_value >= final_transaction.value_with_priority_fee)
                    || (self.inner.final_transaction_priority_fee.receiver_pays()
                        && stage.aggregate_input_value >= final_transaction.value_no_fees.saturating_sub(context.aggregate_fees))
                {
                    if let Some(kind) = self.try_finish_standard_stage_processing(context, stage, &mut data, final_transaction)? {
                        return Ok((kind, data));
                    }
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

        let input = TransactionInput::new(utxo.outpoint.clone().into(), vec![], 0, self.inner.sig_op_count);
        let input_amount = utxo.amount();
        let input_compute_mass = calc.calc_mass_for_input(&input) + self.inner.signature_mass_per_input;

        // NOTE: relay transactions have no storage mass
        // mass threshold reached, yield transaction
        if data.aggregate_mass
            + input_compute_mass
            + self.inner.standard_change_output_compute_mass
            + self.inner.network_params.additional_compound_transaction_mass
            > MAXIMUM_STANDARD_TRANSACTION_MASS
        {
            // note, we've used input for mass boundary calc and now abandon it
            // while preserving the UTXO entry reference to be used in the next iteration

            context.utxo_stash.push_back(utxo_entry_reference);
            data.aggregate_mass +=
                self.inner.standard_change_output_compute_mass + self.inner.network_params.additional_compound_transaction_mass;
            data.transaction_fees = self.calc_relay_transaction_compute_fees(data);
            stage.aggregate_fees += data.transaction_fees;
            context.aggregate_fees += data.transaction_fees;
            Some(DataKind::Node)
        } else {
            context.aggregated_utxos += 1;
            stage.aggregate_input_value += input_amount;
            data.aggregate_input_value += input_amount;
            data.aggregate_mass += input_compute_mass;
            data.utxo_entry_references.push(utxo_entry_reference.clone());
            data.inputs.push(input);
            utxo.address.as_ref().map(|address| data.addresses.insert(address.clone()));
            None
        }
    }

    /// Check current state and either 1) initiate a new stage or 2) finish stage accumulation processing
    fn finish_relay_stage_processing(&self, context: &mut Context, stage: &mut Stage, mut data: Data) -> Result<(DataKind, Data)> {
        data.transaction_fees = self.calc_relay_transaction_compute_fees(&data);
        stage.aggregate_fees += data.transaction_fees;
        context.aggregate_fees += data.transaction_fees;

        if context.aggregated_utxos < 2 {
            Ok((DataKind::NoOp, data))
        } else if stage.number_of_transactions > 0 {
            data.aggregate_mass += self.inner.standard_change_output_compute_mass;
            data.change_output_value = Some(data.aggregate_input_value - data.transaction_fees);
            Ok((DataKind::Edge, data))
        } else if data.aggregate_input_value < data.transaction_fees {
            Err(Error::InsufficientFunds { additional_needed: data.transaction_fees - data.aggregate_input_value, origin: "relay" })
        } else {
            let change_output_value = data.aggregate_input_value - data.transaction_fees;

            if self.inner.mass_calculator.is_dust(change_output_value) {
                // sweep transaction resulting in dust output
                Ok((DataKind::NoOp, data))
            } else {
                data.aggregate_mass += self.inner.standard_change_output_compute_mass;
                data.change_output_value = Some(change_output_value);
                Ok((DataKind::Final, data))
            }
        }
    }

    /// Calculate storage mass using inputs from `Data`
    /// and `output_harmonics` supplied by the user
    fn calc_storage_mass(&self, data: &Data, output_harmonics: u64) -> u64 {
        let calc = &self.inner.mass_calculator;
        calc.calc_storage_mass(output_harmonics, data.aggregate_input_value, data.inputs.len() as u64)
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
        let calc = &self.inner.mass_calculator;

        // calculate storage mass
        let MassDisposition { transaction_mass, storage_mass, transaction_fees, absorb_change_to_fees } =
            self.calculate_mass(stage, data, final_transaction.value_with_priority_fee)?;

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

        if reject {
            // need more value, reject finalization (try adding more inputs)
            Ok(None)
        } else if transaction_mass > MAXIMUM_STANDARD_TRANSACTION_MASS || stage.number_of_transactions > 0 {
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
                && transaction_mass < TRANSACTION_MASS_BOUNDARY_FOR_ADDITIONAL_INPUT_ACCUMULATION
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
            // if is_dust(&self.inner.network_params, change_output_value) {
            if absorb_change_to_fees || change_output_value == 0 {
                transaction_fees += change_output_value;

                // as we might absorb an input as a part of the receiver
                // pays fee reduction, we should update the mass to make
                // sure internal metrics and unit tests check out.
                let compute_mass = data.aggregate_mass
                    + self.inner.final_transaction_outputs_compute_mass
                    + self.inner.final_transaction_payload_mass;
                let storage_mass = self.calc_storage_mass(data, self.inner.final_transaction_outputs_harmonic);

                data.aggregate_mass = calc.combine_mass(compute_mass, storage_mass);

                transaction_fees += change_output_value;
                data.transaction_fees = transaction_fees;
                stage.aggregate_fees += transaction_fees;
                context.aggregate_fees += transaction_fees;

                Ok(Some(DataKind::Final))
            } else {
                data.aggregate_mass = transaction_mass;
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

        let compute_mass_with_change = data.aggregate_mass
            + self.inner.standard_change_output_compute_mass
            + self.inner.final_transaction_outputs_compute_mass
            + self.inner.final_transaction_payload_mass;

        let storage_mass = if stage.number_of_transactions > 0 {
            // calculate for edge transaction boundaries
            // we know that stage.number_of_transactions > 0 will trigger stage generation
            let edge_compute_mass = data.aggregate_mass + self.inner.standard_change_output_compute_mass; //self.inner.final_transaction_outputs_compute_mass + self.inner.final_transaction_payload_mass;
            let edge_fees = calc.calc_minimum_transaction_fee_from_mass(edge_compute_mass);
            let edge_output_value = data.aggregate_input_value.saturating_sub(edge_fees);
            if edge_output_value != 0 {
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
                let output_harmonic_with_change =
                    calc.calc_storage_mass_output_harmonic_single(change_value) + self.inner.final_transaction_outputs_harmonic;
                let storage_mass_with_change = self.calc_storage_mass(data, output_harmonic_with_change);

                if storage_mass_with_change == 0
                    || (self.inner.network_params.mass_combination_strategy == MassCombinationStrategy::Max
                        && storage_mass_with_change < compute_mass_with_change)
                {
                    0
                } else {
                    let storage_mass_no_change = self.calc_storage_mass(data, self.inner.final_transaction_outputs_harmonic);
                    if storage_mass_with_change < storage_mass_no_change {
                        storage_mass_with_change
                    } else {
                        let fees_with_change = calc.calc_fee_for_storage_mass(storage_mass_with_change);
                        let fees_no_change = calc.calc_fee_for_storage_mass(storage_mass_no_change);
                        let difference = fees_with_change.saturating_sub(fees_no_change);

                        if difference > change_value {
                            absorb_change_to_fees = true;
                            storage_mass_no_change
                        } else {
                            storage_mass_with_change
                        }
                    }
                }
            }
        };

        if storage_mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
            Err(Error::StorageMassExceedsMaximumTransactionMass { storage_mass })
        } else {
            let transaction_mass = calc.combine_mass(compute_mass_with_change, storage_mass);
            let transaction_fees = calc.calc_minimum_transaction_fee_from_mass(transaction_mass);

            Ok(MassDisposition { transaction_mass, transaction_fees, storage_mass, absorb_change_to_fees })
        }
    }

    /// Generate an `Edge` transaction. This function is called when the transaction
    /// processing has aggregated sufficient inputs to match requested outputs.
    fn generate_edge_transaction(&self, context: &mut Context, stage: &mut Stage, data: &mut Data) -> Result<Option<DataKind>> {
        let calc = &self.inner.mass_calculator;

        let compute_mass = data.aggregate_mass
            + self.inner.standard_change_output_compute_mass
            + self.inner.network_params.additional_compound_transaction_mass;
        let compute_fees = calc.calc_minimum_transaction_fee_from_mass(compute_mass);

        // TODO - consider removing this as calculated storage mass should produce `0` value
        let edge_output_harmonic =
            calc.calc_storage_mass_output_harmonic_single(data.aggregate_input_value.saturating_sub(compute_fees));
        let storage_mass = self.calc_storage_mass(data, edge_output_harmonic);
        let transaction_mass = calc.combine_mass(compute_mass, storage_mass);

        if transaction_mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
            // transaction mass is too high... if we have additional
            // UTXOs, reject and try to accumulate more inputs...
            if self.has_utxo_entries(context, stage) {
                Ok(None)
            } else {
                // otherwise we have insufficient funds
                Err(Error::GeneratorTransactionIsTooHeavy)
            }
        } else {
            data.aggregate_mass = transaction_mass;
            data.transaction_fees = calc.calc_minimum_transaction_fee_from_mass(transaction_mass);
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
                    aggregate_mass,
                    transaction_fees,
                    ..
                } = data;

                let change_output_value = change_output_value.unwrap_or(0);

                let mut final_outputs = self.inner.final_transaction_outputs.clone();
                // let mut final_outputs = context.final_transaction_outputs.clone();

                if self.inner.final_transaction_priority_fee.receiver_pays() {
                    let output = final_outputs.get_mut(0).expect("include fees requires one output");
                    if aggregate_input_value < output.value {
                        output.value = aggregate_input_value - transaction_fees;
                    } else {
                        output.value -= transaction_fees;
                    }
                }

                if change_output_value > 0 {
                    let output = TransactionOutput::new(change_output_value, pay_to_address_script(&self.inner.change_address));
                    final_outputs.push(output);
                }

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
                    0,
                    inputs,
                    final_outputs,
                    0,
                    SUBNETWORK_ID_NATIVE,
                    0,
                    self.inner.final_transaction_payload.clone(),
                );

                context.final_transaction_id = Some(tx.id());
                context.number_of_transactions += 1;

                Ok(Some(PendingTransaction::try_new(
                    self,
                    tx,
                    utxo_entry_references,
                    addresses.into_iter().collect(),
                    self.final_transaction_value_no_fees(),
                    change_output_value,
                    aggregate_input_value,
                    aggregate_output_value,
                    aggregate_mass,
                    transaction_fees,
                    kind,
                )?))
            }
            (kind, data) => {
                let Data {
                    inputs,
                    utxo_entry_references,
                    addresses,
                    aggregate_input_value,
                    aggregate_mass,
                    transaction_fees,
                    change_output_value,
                    ..
                } = data;

                assert_eq!(change_output_value, None);

                let output_value = aggregate_input_value - transaction_fees;
                let script_public_key = pay_to_address_script(&self.inner.change_address);
                let output = TransactionOutput::new(output_value, script_public_key.clone());
                let tx = Transaction::new(0, inputs, vec![output], 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
                context.number_of_transactions += 1;

                let utxo_entry_reference =
                    Self::create_batch_utxo_entry_reference(tx.id(), output_value, script_public_key, &self.inner.change_address);

                match kind {
                    DataKind::Node => {
                        // store resulting UTXO in the current stage
                        let stage = context.stage.as_mut().unwrap();
                        stage.utxo_accumulator.push(utxo_entry_reference);
                        stage.number_of_transactions += 1;
                    }
                    DataKind::Edge => {
                        // store resulting UTXO in the current stage and create a new stage
                        let mut stage = context.stage.take().unwrap();
                        stage.utxo_accumulator.push(utxo_entry_reference);
                        stage.number_of_transactions += 1;
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
                    output_value,
                    aggregate_input_value,
                    output_value,
                    aggregate_mass,
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
            aggregated_fees: context.aggregate_fees,
            final_transaction_amount: self.final_transaction_value_no_fees(),
            final_transaction_id: context.final_transaction_id,
            number_of_generated_transactions: context.number_of_transactions,
        }
    }
}
