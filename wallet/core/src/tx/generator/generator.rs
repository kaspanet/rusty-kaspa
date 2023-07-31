use crate::imports::*;
use crate::result::Result;
use crate::tx::{
    get_consensus_params_by_address, limits::*, GeneratorSettings, PaymentDestination, PendingTransaction, PendingTransactionIterator,
    PendingTransactionStream, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoContext, UtxoEntryReference,
};
use crate::utxo::UtxoEntry;
use kaspa_consensus_core::tx as cctx;
use kaspa_txscript::pay_to_address_script;
use std::collections::VecDeque;

struct Context {
    utxo_iterator: Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static>,

    aggregated_utxos: usize,
    // total fees of all transactions issued by
    // the single generator instance
    #[allow(dead_code)]
    aggregated_fees: u64,
    // UTXO entry consumed from the iterator but
    // was not used and remained for the next transaction
    utxo_stash: VecDeque<UtxoEntryReference>,
    is_done: bool,
}

struct Inner {
    abortable: Abortable,
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
    #[allow(dead_code)]
    final_transaction_priority_fee: Option<u64>,
    // applies only to the final transaction
    #[allow(dead_code)]
    final_transaction_include_fees_in_amount: bool,
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
    pub fn new(settings: GeneratorSettings, abortable: &Abortable) -> Self {
        let GeneratorSettings {
            utxo_iterator,
            utxo_context,
            sig_op_count,
            minimum_signatures,
            change_address,
            final_priority_fee: final_transaction_priority_fee,
            final_include_fees_in_amount: final_transaction_include_fees_in_amount,
            final_transaction_destination,
            final_transaction_payload,
        } = settings;

        let mass_calculator = MassCalculator::new(get_consensus_params_by_address(&change_address));

        let (final_transaction_outputs, final_transaction_amount) = match final_transaction_destination {
            // PaymentDestination::Address(address) => (vec![TransactionOutput::new(0, &pay_to_address_script(&address))], None),
            PaymentDestination::Change => (vec![TransactionOutput::new(0, &pay_to_address_script(&change_address))], None),
            PaymentDestination::PaymentOutputs(outputs) => (
                outputs.iter().map(|output| TransactionOutput::new(output.amount, &pay_to_address_script(&output.address))).collect(),
                Some(outputs.iter().map(|output| output.amount).sum()),
            ),
        };

        let context = Mutex::new(Context {
            utxo_iterator,
            aggregated_utxos: 0,
            aggregated_fees: 0,
            utxo_stash: VecDeque::default(),
            is_done: false,
        });

        let change_output_mass =
            mass_calculator.calc_mass_for_output(&TransactionOutput::new(0, &pay_to_address_script(&change_address)));
        let final_transaction_outputs_mass = mass_calculator.calc_mass_for_outputs(&final_transaction_outputs);

        let inner = Inner {
            context,
            abortable: abortable.clone(),
            mass_calculator,
            utxo_context,
            sig_op_count,
            minimum_signatures,
            change_address,
            change_output_mass,
            final_transaction_amount,
            final_transaction_priority_fee,
            final_transaction_include_fees_in_amount,
            final_transaction_outputs,
            final_transaction_outputs_mass,
            final_transaction_payload: final_transaction_payload.unwrap_or_default(),
        };

        Self { inner: Arc::new(inner) }
    }

    pub fn utxo_context(&self) -> &Option<UtxoContext> {
        &self.inner.utxo_context
    }

    fn context(&self) -> MutexGuard<Context> {
        self.inner.context.lock().unwrap()
    }

    pub fn aggregate_fees(&self) -> u64 {
        self.context().aggregated_fees
    }

    pub fn aggregate_utxos(&self) -> usize {
        self.context().aggregated_utxos
    }

    pub fn stream(&self) -> impl Stream<Item = Result<PendingTransaction>> {
        Box::pin(PendingTransactionStream::new(self))
        // PendingTransactionStream::new(self)
    }

    pub fn iter(&self) -> impl Iterator<Item = Result<PendingTransaction>> {
        PendingTransactionIterator::new(self)
    }

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

        let mut utxo_entry_references = vec![];
        let mut inputs = vec![];

        let mut sequence = 0;
        let mut is_final = false;
        loop {
            self.inner.abortable.check()?;

            // take utxo from stash or from the iterator
            let utxo_entry_reference = if let Some(utxo_entry_reference) = context.utxo_stash.pop_front() {
                utxo_entry_reference
            } else {
                context.utxo_iterator.next().ok_or(Error::InsufficientFunds)?
            };

            context.aggregated_utxos += 1;
            let UtxoEntryReference { utxo } = &utxo_entry_reference;

            let input = TransactionInput::new(utxo.outpoint.clone(), vec![], sequence, self.inner.sig_op_count);
            let input_amount = utxo.amount();
            let mass_for_input = calc.calc_mass_for_input(&input) + signature_mass_per_input;

            // let next_mass = mass + mass_for_input + final_outputs_mass;

            // maximum mass reached, require additional transaction
            if mass_accumulator + mass_for_input + change_output_mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
                context.utxo_stash.push_back(utxo_entry_reference);
                break;
            }

            mass_accumulator += mass_for_input;
            transaction_amount_accumulator += input_amount;
            utxo_entry_references.push(utxo_entry_reference);
            inputs.push(input);
            sequence += 1;

            // TODO - WIP
            let mut final_transaction_fees = calc.calc_minimum_transaction_relay_fee_from_mass(mass_accumulator);
            if !self.inner.final_transaction_include_fees_in_amount {
                final_transaction_fees += self.inner.final_transaction_priority_fee.unwrap_or(0);
            }

            // check if we have and we have reached the desired transaction amount
            if let Some(final_transaction_amount) = self.inner.final_transaction_amount {
                let final_transaction_total = final_transaction_amount + final_transaction_fees;
                if transaction_amount_accumulator > final_transaction_total {
                    // ------------------------- WIP

                    change_amount = transaction_amount_accumulator - final_transaction_total;

                    // - TODO - REFACTOR TO USE CUSTOM CONSTANT DERIVED FROM STANDARD OUTPUT
                    //const STANDARD_OUTPUT_DUST_LIMIT: u64 = MINIMUM_RELAY_TRANSACTION_FEE;
                    if is_standard_output_amount_dust(change_amount) {
                        change_amount = 0;

                        is_final = mass_accumulator + final_outputs_mass + payload_mass < MAXIMUM_STANDARD_TRANSACTION_MASS;
                    } else {
                        is_final = mass_accumulator + change_output_mass + final_outputs_mass + payload_mass
                            < MAXIMUM_STANDARD_TRANSACTION_MASS;
                    }

                    // ------------------------- WIP

                    break;
                }
            }
        }

        // -----------------------------
        // - TODO adjust fees
        // - TODO pre-check dust outputs
        // -----------------------------

        if is_final {
            context.is_done = true;

            let mut final_outputs = self.inner.final_transaction_outputs.clone();
            if change_amount > 0 {
                let script_public_key = pay_to_address_script(&self.inner.change_address);
                let output = TransactionOutput::new(change_amount, &script_public_key);
                final_outputs.push(output);
            }

            let tx = Transaction::new(
                0,
                inputs,
                final_outputs,
                0,
                SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
                0,
                self.inner.final_transaction_payload.clone(),
            )?;

            Ok(Some(PendingTransaction::new(self, tx, utxo_entry_references)))
        } else {
            let amount = transaction_amount_accumulator - MINIMUM_RELAY_TRANSACTION_FEE;
            let script_public_key = pay_to_address_script(&self.inner.change_address);
            let output = TransactionOutput::new(amount, &script_public_key);

            let tx = Transaction::new(
                0,
                inputs,
                vec![output],
                0,
                SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
                0,
                vec![],
            )?;

            let utxo_entry_reference =
                Self::create_utxo_entry_reference(tx.id(), amount, script_public_key, &self.inner.change_address);
            context.utxo_stash.push_front(utxo_entry_reference);

            Ok(Some(PendingTransaction::new(self, tx, utxo_entry_references)))
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
        let utxo = UtxoEntry { address: Some(address.clone()), outpoint, entry };
        UtxoEntryReference { utxo: Arc::new(utxo) }
    }
}
