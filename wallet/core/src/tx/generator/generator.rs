use crate::imports::*;
use crate::result::Result;
use crate::tx::{
    mass::*, Fees, GeneratorSettings, GeneratorSummary, PaymentDestination, PendingTransaction, PendingTransactionIterator,
    PendingTransactionStream,
};
use crate::utxo::{UtxoContext, UtxoEntryReference};
use kaspa_consensus_core::constants::UNACCEPTED_DAA_SCORE;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx as cctx;
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
use kaspa_consensus_wasm::UtxoEntry;
use kaspa_txscript::pay_to_address_script;
use std::collections::VecDeque;

use super::SignerT;

struct Context {
    utxo_source_iterator: Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static>,
    /// utxo_stage_iterator: Option<Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static>>,
    aggregated_utxos: usize,
    /// total fees of all transactions issued by
    /// the single generator instance
    aggregate_fees: u64,
    /// number of generated transactions
    number_of_transactions: usize,
    /// UTXO entry accumulator for each stage
    /// utxo_stage_accumulator: Vec<UtxoEntryReference>,
    stage: Option<Box<Stage>>,
    /// UTXO entry consumed from the iterator but
    /// was not used due to mass threshold and
    /// remained for the next transaction
    utxo_stash: VecDeque<UtxoEntryReference>,
    /// final transaction id
    final_transaction_id: Option<TransactionId>,
    /// signifies that the generator is finished
    /// no more items will be produced in the
    /// iterator or a stream
    is_done: bool,
}

#[derive(Default)]
struct Stage {
    utxo_iterator: Option<Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static>>,
    utxo_accumulator: Vec<UtxoEntryReference>,
    aggregate_input_value: u64,
    // aggregate_mass: u64,
    aggregate_fees: u64,
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
            // aggregate_mass: 0,
            aggregate_fees: 0,
            number_of_transactions: 0,
        }
    }
}

impl std::fmt::Debug for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stage")
            .field("aggregate_input_value", &self.aggregate_input_value)
            // .field("aggregate_mass", &self.aggregate_mass)
            .field("aggregate_fees", &self.aggregate_fees)
            .field("number_of_transactions", &self.number_of_transactions)
            .finish()
    }
}

#[derive(Debug)]
enum DataKind {
    NoOp,
    Node,
    Edge,
    Final,
}

#[derive(Debug)]
struct Data {
    inputs: Vec<TransactionInput>,
    utxo_entry_references: Vec<UtxoEntryReference>,
    addresses: HashSet<Address>,
    aggregate_mass: u64,
    transaction_fees: u64,
    aggregate_input_value: u64,
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

struct Inner {
    abortable: Option<Abortable>,
    signer: Option<Arc<dyn SignerT>>,
    mass_calculator: MassCalculator,
    network_type: NetworkType,

    // Utxo Context
    utxo_context: Option<UtxoContext>,
    // Event multiplexer
    multiplexer: Option<Multiplexer<Events>>,
    // typically a number of keys required to sign the transaction
    sig_op_count: u8,
    // number of minimum signatures required to sign the transaction
    #[allow(dead_code)]
    minimum_signatures: u16,
    // change address
    change_address: Address,
    // change_output: TransactionOutput,
    standard_change_output_mass: u64,
    // signature mass per input
    signature_mass_per_input: u64,
    // transaction amount (`None` results in consumption of all available UTXOs)
    // `None` is used for sweep transactions
    final_transaction_amount: Option<u64>,
    // applies only to the final transaction
    final_transaction_priority_fee: Fees,
    // issued only in the final transaction
    final_transaction_outputs: Vec<TransactionOutput>,
    // mass of the final transaction
    final_transaction_outputs_mass: u64,
    // final transaction payload
    final_transaction_payload: Vec<u8>,
    // final transaction payload mass
    final_transaction_payload_mass: u64,
    // execution context
    context: Mutex<Context>,
}

#[derive(Clone)]
pub struct Generator {
    inner: Arc<Inner>,
}

impl Generator {
    pub fn try_new(settings: GeneratorSettings, signer: Option<Arc<dyn SignerT>>, abortable: Option<&Abortable>) -> Result<Self> {
        let GeneratorSettings {
            network_type,
            multiplexer,
            utxo_iterator,
            utxo_context,
            sig_op_count,
            minimum_signatures,
            change_address,
            final_transaction_priority_fee,
            final_transaction_destination,
            final_transaction_payload,
        } = settings;

        let mass_calculator = MassCalculator::new(&network_type.into());

        let (final_transaction_outputs, final_transaction_amount) = match final_transaction_destination {
            // PaymentDestination::Address(address) => (vec![TransactionOutput::new(0, &pay_to_address_script(&address))], None),
            PaymentDestination::Change => {
                if !final_transaction_priority_fee.is_none() {
                    return Err(Error::GeneratorFeesInSweepTransaction);
                }

                (vec![], None)
            }
            PaymentDestination::PaymentOutputs(outputs) => {
                // sanity check
                for output in outputs.iter() {
                    if NetworkType::try_from(output.address.prefix)? != network_type {
                        return Err(Error::GeneratorPaymentOutputNetworkTypeMismatch);
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

        if final_transaction_outputs.len() != 1 && matches!(final_transaction_priority_fee, Fees::ReceiverPaysTransfer(_)) {
            return Err(Error::GeneratorIncludeFeesRequiresOneOutput);
        }

        // sanity check
        if NetworkType::try_from(change_address.prefix)? != network_type {
            return Err(Error::GeneratorChangeAddressNetworkTypeMismatch);
        }

        // if final_transaction_amount.is_none() && !matches!(final_transaction_priority_fee, Fees::None) {
        // }

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

        let standard_change_output_mass =
            mass_calculator.calc_mass_for_output(&TransactionOutput::new(0, pay_to_address_script(&change_address)));
        let signature_mass_per_input = mass_calculator.calc_signature_mass(minimum_signatures);
        let final_transaction_outputs_mass = mass_calculator.calc_mass_for_outputs(&final_transaction_outputs);
        let final_transaction_payload = final_transaction_payload.unwrap_or_default();
        let final_transaction_payload_mass = mass_calculator.calc_mass_for_payload(final_transaction_payload.len());

        let inner = Inner {
            network_type,
            multiplexer,
            context,
            signer,
            abortable: abortable.cloned(),
            mass_calculator,
            utxo_context,
            sig_op_count,
            minimum_signatures,
            change_address,
            standard_change_output_mass,
            signature_mass_per_input,
            final_transaction_amount,
            final_transaction_priority_fee,
            final_transaction_outputs,
            final_transaction_outputs_mass,
            final_transaction_payload,
            final_transaction_payload_mass,
        };
        Ok(Self { inner: Arc::new(inner) })
    }

    /// The underlying [`UtxoContext`] (if available).
    pub fn utxo_context(&self) -> &Option<UtxoContext> {
        &self.inner.utxo_context
    }

    /// Core [`Multiplexer<Events>`] (if available)
    pub fn multiplexer(&self) -> &Option<Multiplexer<Events>> {
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

    fn get_utxo_entry(&self, context: &mut Context, stage: &mut Stage) -> Option<UtxoEntryReference> {
        context
            .utxo_stash
            .pop_front()
            .or_else(|| stage.utxo_iterator.as_mut().and_then(|utxo_stage_iterator| utxo_stage_iterator.next()))
            .or_else(|| context.utxo_source_iterator.next())
    }

    fn calc_relay_transaction_mass(&self, data: &Data) -> u64 {
        data.aggregate_mass + self.inner.standard_change_output_mass
    }

    fn calc_relay_transaction_relay_fees(&self, data: &Data) -> u64 {
        self.inner.mass_calculator.calc_minimum_transaction_relay_fee_from_mass(self.calc_relay_transaction_mass(data))
    }

    fn generate_transaction_data(&self, context: &mut Context, stage: &mut Stage) -> Result<(DataKind, Data)> {
        let calc = &self.inner.mass_calculator;
        let mut data = Data::new(calc);
        let mut input_sequence = 0;

        loop {
            if let Some(abortable) = self.inner.abortable.as_ref() {
                abortable.check()?;
            }

            let utxo_entry_reference = if let Some(utxo_entry_reference) = self.get_utxo_entry(context, stage) {
                utxo_entry_reference
            } else {
                // UTXO sources are depleted, handle sweep processing
                if self.inner.final_transaction_amount.is_none() {
                    return self.finish_relay_stage_processing(context, stage, data);
                } else {
                    return Err(Error::InsufficientFunds);
                }
            };

            let UtxoEntryReference { utxo } = &utxo_entry_reference;

            let input = TransactionInput::new(utxo.outpoint.clone().into(), vec![], input_sequence, self.inner.sig_op_count);
            let input_amount = utxo.amount();
            let input_mass = calc.calc_mass_for_input(&input) + self.inner.signature_mass_per_input;
            input_sequence += 1;

            // mass threshold reached, yield transaction
            if data.aggregate_mass + input_mass + self.inner.standard_change_output_mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
                context.utxo_stash.push_back(utxo_entry_reference);
                data.aggregate_mass += self.inner.standard_change_output_mass;
                data.transaction_fees = self.calc_relay_transaction_relay_fees(&data);
                stage.aggregate_fees += data.transaction_fees;
                context.aggregate_fees += data.transaction_fees;
                return Ok((DataKind::Node, data));
            }

            context.aggregated_utxos += 1;
            stage.aggregate_input_value += input_amount;
            data.aggregate_input_value += input_amount;
            data.aggregate_mass += input_mass;
            data.utxo_entry_references.push(utxo_entry_reference.clone());
            data.inputs.push(input);
            utxo.address.as_ref().map(|address| data.addresses.insert(address.clone()));

            // standard transaction with target value
            if let Some(final_transaction_value) = self.inner.final_transaction_amount {
                if let Some(kind) = self.try_finish_standard_stage_processing(context, stage, &mut data, final_transaction_value)? {
                    return Ok((kind, data));
                }
            }
        }
    }

    fn finish_relay_stage_processing(&self, context: &mut Context, stage: &mut Stage, mut data: Data) -> Result<(DataKind, Data)> {
        data.transaction_fees = self.calc_relay_transaction_relay_fees(&data);
        stage.aggregate_fees += data.transaction_fees;
        context.aggregate_fees += data.transaction_fees;

        if context.aggregated_utxos < 2 {
            Ok((DataKind::NoOp, data))
        } else if stage.number_of_transactions > 0 {
            data.aggregate_mass += self.inner.standard_change_output_mass;
            data.change_output_value = Some(data.aggregate_input_value - data.transaction_fees);
            Ok((DataKind::Edge, data))
        } else {
            if data.aggregate_input_value < data.transaction_fees {
                Err(Error::InsufficientFunds)
            } else {
                let change_output_value = data.aggregate_input_value - data.transaction_fees;
                if is_standard_output_amount_dust(change_output_value) {
                    // sweep transaction resulting in dust output
                    // we add dust to fees, but the transaction will be
                    // discarded anyways due to `Exception` status.
                    // data.transaction_fees += change_output_value;
                    Ok((DataKind::NoOp, data))
                } else {
                    data.aggregate_mass += self.inner.standard_change_output_mass;
                    data.change_output_value = Some(change_output_value);
                    Ok((DataKind::Final, data))
                }
            }
        }
    }

    fn try_finish_standard_stage_processing(
        &self,
        context: &mut Context,
        stage: &mut Stage,
        data: &mut Data,
        final_transaction_value_no_fees: u64,
    ) -> Result<Option<DataKind>> {
        let calc = &self.inner.mass_calculator;

        let final_transaction_mass = data.aggregate_mass
            + self.inner.standard_change_output_mass
            + self.inner.final_transaction_outputs_mass
            + self.inner.final_transaction_payload_mass;

        let final_transaction_relay_fees = calc.calc_minimum_transaction_relay_fee_from_mass(final_transaction_mass);

        let total_stage_value_needed = match self.inner.final_transaction_priority_fee {
            Fees::SenderPaysAll(priority_fees) => {
                final_transaction_value_no_fees + stage.aggregate_fees + final_transaction_relay_fees + priority_fees
            }
            _ => final_transaction_value_no_fees,
        };

        if total_stage_value_needed > stage.aggregate_input_value {
            Ok(None)
        } else {
            // if final transaction hits mass boundary or this is a stage, generate new stage
            if final_transaction_mass > MAXIMUM_STANDARD_TRANSACTION_MASS || stage.number_of_transactions > 0 {
                data.aggregate_mass += self.inner.standard_change_output_mass;
                data.transaction_fees = calc.calc_minimum_transaction_relay_fee_from_mass(data.aggregate_mass);
                stage.aggregate_fees += data.transaction_fees;
                context.aggregate_fees += data.transaction_fees;
                Ok(Some(DataKind::Edge))
            } else {

                let (mut transaction_fees, change_output_value) = match self.inner.final_transaction_priority_fee {
                    Fees::SenderPaysAll(priority_fees) => {
                        let transaction_fees = final_transaction_relay_fees + priority_fees;
                        let change_output_value = data.aggregate_input_value - final_transaction_value_no_fees - transaction_fees;
                        (transaction_fees, change_output_value)
                    }
                    Fees::ReceiverPaysTransfer(priority_fees) => {
                        let transaction_fees = final_transaction_relay_fees + priority_fees;
                        let change_output_value = data.aggregate_input_value - final_transaction_value_no_fees; 
                        (transaction_fees, change_output_value)
                    }
                    Fees::ReceiverPaysAll(priority_fees) => {
                        let transaction_fees = final_transaction_relay_fees + priority_fees;
                        let change_output_value = data.aggregate_input_value - final_transaction_value_no_fees;
                        (transaction_fees, change_output_value)
                    }
                    Fees::None => unreachable!("Fees::None is not allowed for final transactions"),
                };

                data.change_output_value = if is_standard_output_amount_dust(change_output_value) {
                    data.aggregate_mass += self.inner.final_transaction_outputs_mass + self.inner.final_transaction_payload_mass;
                    transaction_fees += change_output_value;
                    data.transaction_fees = transaction_fees;
                    stage.aggregate_fees += transaction_fees;
                    context.aggregate_fees += transaction_fees;
                    None
                } else {
                    data.aggregate_mass += self.inner.standard_change_output_mass
                        + self.inner.final_transaction_outputs_mass
                        + self.inner.final_transaction_payload_mass;
                    data.transaction_fees = transaction_fees;
                    stage.aggregate_fees += transaction_fees;
                    context.aggregate_fees += transaction_fees;
                    Some(change_output_value)
                };

                Ok(Some(DataKind::Final))
            }
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

                if let Fees::ReceiverPaysTransfer(_) = self.inner.final_transaction_priority_fee {
                    let output = final_outputs.get_mut(0).expect("include fees requires one output");
                    output.value -= transaction_fees;
                }

                if change_output_value > 0 {
                    let output = TransactionOutput::new(change_output_value, pay_to_address_script(&self.inner.change_address));
                    final_outputs.push(output);
                }

                let aggregate_output_value = final_outputs.iter().map(|output| output.value).sum::<u64>();
                // `Fees::ReceiverPays` processing can result in outputs being larger than inputs
                if aggregate_output_value > aggregate_input_value {
                    return Err(Error::InsufficientFunds);
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
                    self.inner.final_transaction_amount,
                    change_output_value,
                    aggregate_input_value,
                    aggregate_output_value,
                    aggregate_mass,
                    transaction_fees,
                    true,
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
                    self.inner.final_transaction_amount,
                    output_value,
                    aggregate_input_value,
                    output_value,
                    aggregate_mass,
                    transaction_fees,
                    false,
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
        let entry = cctx::UtxoEntry { amount, script_public_key, block_daa_score: UNACCEPTED_DAA_SCORE, is_coinbase: false };
        let outpoint = TransactionOutpoint::new(txid, 0);
        let utxo = UtxoEntry { address: Some(address.clone()), outpoint: outpoint.into(), entry };
        UtxoEntryReference { utxo: Arc::new(utxo) }
    }

    /// Produces [`GeneratorSummary`] for the current state of the generator.
    /// This method is useful for creation of transaction estimations.
    pub fn summary(&self) -> GeneratorSummary {
        let context = self.context();

        GeneratorSummary {
            network_type: self.inner.network_type,
            aggregated_utxos: context.aggregated_utxos,
            aggregated_fees: context.aggregate_fees,
            final_transaction_amount: self.inner.final_transaction_amount,
            final_transaction_id: context.final_transaction_id,
            number_of_generated_transactions: context.number_of_transactions,
        }
    }
}
