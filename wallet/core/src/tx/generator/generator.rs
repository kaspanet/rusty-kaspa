use crate::imports::*;
use crate::result::Result;
use crate::tx::{
    get_consensus_params_by_address, mass::*, Fees, GeneratorSettings, GeneratorSummary, PaymentDestination, PendingTransaction,
    PendingTransactionIterator, PendingTransactionStream,
};
use crate::utxo::UtxoEntry;
use crate::utxo::{UtxoContext, UtxoEntryReference};
use kaspa_consensus_core::tx as cctx;
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
use kaspa_txscript::pay_to_address_script;
use std::collections::VecDeque;

use super::SignerT;

struct Context {
    utxo_iterator: Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static>,

    aggregated_utxos: usize,
    // total fees of all transactions issued by
    // the single generator instance
    aggregated_fees: u64,
    // number of generated transactions
    number_of_generated_transactions: usize,
    // UTXO entry consumed from the iterator but
    // was not used and remained for the next transaction
    utxo_stash: VecDeque<UtxoEntryReference>,
    // final transaction id
    final_transaction_id: Option<TransactionId>,
    // signifies that the generator is finished
    // no more items will be produced in the
    // iterator or a stream
    is_done: bool,
}

struct Inner {
    abortable: Abortable,
    signer: Option<Arc<dyn SignerT>>,
    mass_calculator: MassCalculator,

    // Utxo Context
    utxo_context: Option<UtxoContext>,
    // typically a number of keys required to sign the transaction
    sig_op_count: u8,
    // number of minimum signatures required to sign the transaction
    minimum_signatures: u16,
    // change address
    change_address: Address,
    // change_output: TransactionOutput,
    change_output_mass: u64,
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
    // execution context
    context: Mutex<Context>,
}

#[derive(Clone)]
pub struct Generator {
    inner: Arc<Inner>,
}

impl Generator {
    pub fn new(settings: GeneratorSettings, signer: Option<Arc<dyn SignerT>>, abortable: &Abortable) -> Self {
        let GeneratorSettings {
            utxo_iterator,
            utxo_context,
            sig_op_count,
            minimum_signatures,
            change_address,
            final_priority_fee: final_transaction_priority_fee,
            final_transaction_destination,
            final_transaction_payload,
        } = settings;

        let mass_calculator = MassCalculator::new(get_consensus_params_by_address(&change_address));

        let (final_transaction_outputs, final_transaction_amount) = match final_transaction_destination {
            // PaymentDestination::Address(address) => (vec![TransactionOutput::new(0, &pay_to_address_script(&address))], None),
            PaymentDestination::Change => (vec![], None),
            PaymentDestination::PaymentOutputs(outputs) => (
                outputs.iter().map(|output| TransactionOutput::new(output.amount, pay_to_address_script(&output.address))).collect(),
                Some(outputs.iter().map(|output| output.amount).sum()),
            ),
        };

        let context = Mutex::new(Context {
            utxo_iterator,
            number_of_generated_transactions: 0,
            aggregated_utxos: 0,
            aggregated_fees: 0,
            utxo_stash: VecDeque::default(),
            final_transaction_id: None,
            is_done: false,
        });
        let change_output_mass =
            mass_calculator.calc_mass_for_output(&TransactionOutput::new(0, pay_to_address_script(&change_address)));

        let final_transaction_outputs_mass = mass_calculator.calc_mass_for_outputs(&final_transaction_outputs);

        let inner = Inner {
            context,
            signer,
            abortable: abortable.clone(),
            mass_calculator,
            utxo_context,
            sig_op_count,
            minimum_signatures,
            change_address,
            change_output_mass,
            final_transaction_amount,
            final_transaction_priority_fee,
            final_transaction_outputs,
            final_transaction_outputs_mass,
            final_transaction_payload: final_transaction_payload.unwrap_or_default(),
        };
        Self { inner: Arc::new(inner) }
    }

    /// The underlying UtxoContext (if available).
    pub fn utxo_context(&self) -> &Option<UtxoContext> {
        &self.inner.utxo_context
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
        self.context().aggregated_fees
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

    /// Generates a single transaction by draining the supplied UTXO iterator.
    /// This function is used by the by the available async Stream and Iterator
    /// implementations to generate a stream of transactions.
    ///
    /// This function returns `None` once the supplied UTXO iterator is depleted.
    ///
    /// This function runs continious loop by ingestin inputs from the UTXO iterator,
    /// analyzing the resulting transaction mass and eithe producing an intermediate
    /// orphan transaction sending funds to the change address, or creating a final
    /// transaction with the requested set of outputs and the payload.
    pub fn generate_transaction(&self) -> Result<Option<PendingTransaction>> {
        let mut context = self.context();

        if context.is_done {
            return Ok(None);
        }
        let calc = &self.inner.mass_calculator;
        let signature_mass_per_input = calc.calc_signature_mass(self.inner.minimum_signatures);
        let final_outputs_mass = self.inner.final_transaction_outputs_mass;
        let change_output_mass = self.inner.change_output_mass;
        let mut transaction_amount_accumulator = 0;
        let mut change_amount = 0;
        // let mut change_amount = 0;
        let mut mass_accumulator = calc.blank_transaction_serialized_mass();
        let payload_mass = calc.calc_mass_for_payload(self.inner.final_transaction_payload.len());

        let mut addresses = HashSet::<Address>::default();
        let mut utxo_entry_references = vec![];
        let mut inputs = vec![];

        let mut sequence = 0;
        let mut is_final = false;
        loop {
            self.inner.abortable.check()?;

            // take utxo from stash or from the iterator
            let utxo_entry_reference = if let Some(utxo_entry_reference) = context.utxo_stash.pop_front() {
                utxo_entry_reference
            } else if let Some(entry) = context.utxo_iterator.next() {
                entry
            } else if self.inner.final_transaction_amount.is_none() {
                // we have now exhausted UTXO iterator. if final amount is None, we are
                // doing a sweep transaction.  Produce a final tx if all looks ok.
                is_final = true;

                let final_tx_mass = mass_accumulator + change_output_mass + payload_mass;
                let final_transaction_fees = calc.calc_minimum_transaction_relay_fee_from_mass(final_tx_mass);
                // let mut final_transaction_fees = calc.calc_minimum_transaction_relay_fee_from_mass(final_tx_mass);

                // We are doing a sweep transaction.  We don't care about "include fees in amount" flag here.
                // if !self.inner.final_transaction_include_fees_in_amount {
                //     final_transaction_fees += self.inner.final_transaction_priority_fee.unwrap_or(0);
                // }

                let change_amount = transaction_amount_accumulator - final_transaction_fees;
                if is_standard_output_amount_dust(change_amount) {
                    return Ok(None);
                }

                break;
            } else {
                return Err(Error::InsufficientFunds);
            };

            let UtxoEntryReference { utxo } = &utxo_entry_reference;

            let input = TransactionInput::new(utxo.outpoint.clone().into(), vec![], sequence, self.inner.sig_op_count);
            let input_amount = utxo.amount();
            let mass_for_input = calc.calc_mass_for_input(&input) + signature_mass_per_input;

            // maximum mass reached, require additional transaction
            if mass_accumulator + mass_for_input + change_output_mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
                context.utxo_stash.push_back(utxo_entry_reference);
                break;
            }
            mass_accumulator += mass_for_input;
            transaction_amount_accumulator += input_amount;
            utxo_entry_references.push(utxo_entry_reference.clone());
            inputs.push(input);
            if let Some(address) = utxo.address.as_ref() {
                addresses.insert(address.clone());
            }
            context.aggregated_utxos += 1;
            sequence += 1;

            // check if we have reached the desired transaction amount
            if let Some(final_transaction_amount) = self.inner.final_transaction_amount {
                let final_tx_mass = mass_accumulator + final_outputs_mass + payload_mass;
                let mut final_transaction_fees = calc.calc_minimum_transaction_relay_fee_from_mass(final_tx_mass);
                workflow_log::log_info!("final_transaction_fees A0: {final_transaction_fees:?}");

                if let Fees::Exclude(fees) = self.inner.final_transaction_priority_fee {
                    final_transaction_fees += fees;
                }

                // if !self.inner.final_transaction_include_fees_in_amount {
                //     final_transaction_fees += self.inner.final_transaction_priority_fee.unwrap_or(0);
                // }
                workflow_log::log_info!("final_transaction_fees A1: {final_transaction_fees:?}");

                let final_transaction_total = final_transaction_amount + final_transaction_fees;
                if transaction_amount_accumulator > final_transaction_total {
                    // ------------------------- WIP

                    change_amount = transaction_amount_accumulator - final_transaction_total;

                    if is_standard_output_amount_dust(change_amount) {
                        change_amount = 0;

                        is_final = final_tx_mass < MAXIMUM_STANDARD_TRANSACTION_MASS;
                    } else {
                        //re-calculate fee with change outputs
                        let mut final_transaction_fees =
                            calc.calc_minimum_transaction_relay_fee_from_mass(final_tx_mass + change_output_mass);
                        workflow_log::log_info!("final_transaction_fees B0: {final_transaction_fees:?}");

                        if let Fees::Exclude(fees) = self.inner.final_transaction_priority_fee {
                            final_transaction_fees += fees;
                        }

                        // if !self.inner.final_transaction_include_fees_in_amount {
                        //     final_transaction_fees += self.inner.final_transaction_priority_fee.unwrap_or(0);
                        // }
                        workflow_log::log_info!("final_transaction_fees B1: {final_transaction_fees:?}");
                        //final_transaction_fees = 10;

                        let final_transaction_total = final_transaction_amount + final_transaction_fees;

                        change_amount = transaction_amount_accumulator - final_transaction_total;

                        if is_standard_output_amount_dust(change_amount) {
                            change_amount = 0;

                            is_final = final_tx_mass < MAXIMUM_STANDARD_TRANSACTION_MASS;
                        } else {
                            is_final = final_tx_mass + change_output_mass < MAXIMUM_STANDARD_TRANSACTION_MASS;
                        }
                    }

                    // ------------------------- WIP

                    break;
                }
            }
        }

        // generate transaction from inputs aggregated so far

        if is_final {
            context.is_done = true;

            let mut final_outputs = self.inner.final_transaction_outputs.clone();
            if change_amount > 0 {
                let output = TransactionOutput::new(change_amount, pay_to_address_script(&self.inner.change_address));
                final_outputs.push(output);
            }

            let mut tx = Transaction::new(
                0,
                inputs,
                final_outputs,
                0,
                SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
                0,
                self.inner.final_transaction_payload.clone(),
            );

            tx.finalize();
            context.final_transaction_id = Some(tx.id());
            context.number_of_generated_transactions += 1;

            Ok(Some(PendingTransaction::try_new(self, tx, utxo_entry_references, addresses.into_iter().collect())?))
        } else {
            let fee = calc.calc_minimum_transaction_relay_fee_from_mass(mass_accumulator + change_output_mass);
            workflow_log::log_info!("fee: {fee}");
            let amount = transaction_amount_accumulator - fee;
            let script_public_key = pay_to_address_script(&self.inner.change_address);
            let output = TransactionOutput::new(amount, script_public_key.clone());

            let mut tx = Transaction::new(
                0,
                inputs,
                vec![output],
                0,
                SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
                0,
                vec![],
            );

            tx.finalize();

            let utxo_entry_reference =
                Self::create_utxo_entry_reference(tx.id(), amount, script_public_key, &self.inner.change_address);
            context.utxo_stash.push_front(utxo_entry_reference);

            context.number_of_generated_transactions += 1;
            Ok(Some(PendingTransaction::try_new(self, tx, utxo_entry_references, addresses.into_iter().collect())?))
        }
    }

    fn create_utxo_entry_reference(
        txid: TransactionId,
        amount: u64,
        script_public_key: ScriptPublicKey,
        address: &Address,
    ) -> UtxoEntryReference {
        let entry = cctx::UtxoEntry { amount, script_public_key, block_daa_score: u64::MAX, is_coinbase: false };
        let outpoint = TransactionOutpoint::new(txid, 0);
        let utxo = UtxoEntry { address: Some(address.clone()), outpoint: outpoint.into(), entry };
        UtxoEntryReference { utxo: Arc::new(utxo) }
    }

    /// Produces [`GeneratorSummary`] for the current state of the generator.
    /// This method is useful for creation of transaction estimations.
    pub fn summary(&self) -> GeneratorSummary {
        let context = self.context();

        GeneratorSummary {
            aggregated_utxos: context.aggregated_utxos,
            aggregated_fees: context.aggregated_fees,
            final_transaction_id: context.final_transaction_id,
            number_of_generated_transactions: context.number_of_generated_transactions,
        }
    }
}
