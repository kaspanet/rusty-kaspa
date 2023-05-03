// work in progress

use crate::imports::*;
use crate::tx::{MutableTransaction, PaymentOutputs, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
use crate::utils::{calculate_minimum_transaction_fee, get_consensus_params_by_address};
use crate::utxo::{SelectionContext, UtxoEntry, UtxoEntryReference};
use kaspa_addresses::Address;
use kaspa_consensus_core::{subnets::SubnetworkId, tx};
use kaspa_txscript::pay_to_address_script;

/// `VirtualTransaction` envelops a collection of multiple related `kaspa_wallet_core::MutableTransaction` instances.
#[derive(Clone, Debug)]
#[wasm_bindgen]
#[allow(dead_code)] //TODO: remove me
pub struct VirtualTransaction {
    transactions: Vec<MutableTransaction>,
    payload: Vec<u8>,
    // include_fees : bool,
}

#[wasm_bindgen]
impl VirtualTransaction {
    #[wasm_bindgen(constructor)]
    pub fn new(
        sig_op_count: u8,
        utxo_selection: SelectionContext,
        outputs: PaymentOutputs,
        change_address: Address,
        priority_fee_sompi: Option<u64>,
        payload: Vec<u8>,
    ) -> crate::Result<VirtualTransaction> {
        log_trace!("VirtualTransaction...");
        log_trace!("utxo_selection.transaction_amount: {:?}", utxo_selection.transaction_amount);
        log_trace!("utxo_selection.total_selected_amount: {:?}", utxo_selection.total_selected_amount);
        log_trace!("outputs.outputs: {:?}", outputs.outputs);
        log_trace!("change_address: {:?}", change_address.to_string());

        let consensus_params = get_consensus_params_by_address(&change_address);

        let entries = &utxo_selection.selected_entries;

        log_trace!("entries.len(): {:?}", entries.len());

        let mut final_inputs = vec![];
        let mut final_utxos = vec![];
        let mut final_amount = 0;

        let mut transactions = if entries.len() <= 80 {
            entries.iter().for_each(|utxo_ref| {
                final_amount += utxo_ref.utxo.amount();
                final_utxos.push(utxo_ref.data());
                final_inputs.push(TransactionInput::new(utxo_ref.utxo.outpoint.clone(), vec![], 0, sig_op_count));
                println!("final_amount: {final_amount}, transaction_id: {}\r\n", utxo_ref.utxo.outpoint.get_transaction_id());
            });

            vec![]
        } else {
            let chunks = entries.chunks(80).collect::<Vec<&[UtxoEntryReference]>>();
            chunks
                .into_iter()
                .filter_map(|chunk| {
                    let utxos = chunk.iter().map(|reference| reference.utxo.clone()).collect::<Vec<Arc<UtxoEntry>>>();

                    let mut amount = 0;
                    let mut entries = vec![];

                    let inputs = utxos
                        .iter()
                        .enumerate()
                        .map(|(sequence, utxo)| {
                            //println!("input txid: {}\r\n", utxo.outpoint.get_transaction_id());
                            amount += utxo.utxo_entry.amount;
                            entries.push(utxo.as_ref().clone());
                            TransactionInput::new(utxo.outpoint.clone(), vec![], sequence as u64, sig_op_count)
                        })
                        .collect::<Vec<TransactionInput>>();

                    let script_public_key = pay_to_address_script(&change_address);
                    let tx = Transaction::new(
                        0,
                        inputs,
                        vec![TransactionOutput::new(amount, &script_public_key)],
                        0,
                        SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
                        0,
                        vec![],
                    )
                    .unwrap();

                    let fee = calculate_minimum_transaction_fee(&tx, &consensus_params, true);
                    if amount <= fee {
                        println!("amount<=fee: {amount}, {fee}\r\n");
                        return None;
                    }
                    let amount_after_fee = amount - fee;

                    tx.inner().outputs[0].set_value(amount_after_fee);
                    if tx.inner().outputs[0].is_dust() {
                        println!("outputs is dust: {}\r\n", amount_after_fee);
                        return None;
                    }

                    let transaction_id = tx.finalize().unwrap().to_str();

                    final_amount += amount_after_fee;
                    println!("final_amount: {final_amount}, transaction_id: {}\r\n", transaction_id);
                    final_utxos.push(UtxoEntry {
                        address: Some(change_address.clone()),
                        outpoint: TransactionOutpoint::new(&transaction_id, 0).unwrap(),
                        utxo_entry: tx::UtxoEntry {
                            amount: amount_after_fee,
                            script_public_key,
                            block_daa_score: u64::MAX,
                            is_coinbase: false,
                        },
                    });
                    final_inputs.push(TransactionInput::new(
                        TransactionOutpoint::new(&transaction_id, 0).unwrap(),
                        vec![],
                        0,
                        sig_op_count,
                    ));

                    Some(MutableTransaction::new(&tx, &entries.into()))
                })
                .collect::<Vec<MutableTransaction>>()
        };

        let priority_fee = priority_fee_sompi.unwrap_or(0);
        if final_amount < priority_fee {
            return Err(format!("final amount({final_amount}) < priority fee({priority_fee})").into());
        }

        let amount_after_priority_fee = final_amount - priority_fee;

        let mut outputs_ = vec![];
        let mut output_amount = 0;
        for output in &outputs.outputs {
            output_amount += output.amount;
            outputs_.push(TransactionOutput::new(output.amount, &pay_to_address_script(&output.address)));
        }

        if output_amount > amount_after_priority_fee {
            return Err(format!("output amount({output_amount}) > amount after priority fee({amount_after_priority_fee})").into());
        }

        let change = amount_after_priority_fee - output_amount;
        let mut change_output = None;
        if change > 0 {
            let output = TransactionOutput::new(change, &pay_to_address_script(&change_address));
            if !output.is_dust() {
                change_output = Some(output.clone());
                outputs_.push(output);
            }
        }

        let tx = Transaction::new(
            0,
            final_inputs,
            outputs_,
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            payload.clone(),
        )?;

        let fee = calculate_minimum_transaction_fee(&tx, &consensus_params, true);
        if change < fee {
            return Err(format!("change({change}) <= minimum fee ({fee})").into());
        }
        if let Some(change_output) = change_output {
            let new_change = change - fee;
            change_output.inner().value = new_change;
            if change_output.is_dust() {
                let _change_output = tx.inner().outputs.pop();
            }

            tx.finalize().unwrap();
        }

        let mtx = MutableTransaction::new(&tx, &final_utxos.into());
        transactions.push(mtx);

        //log_trace!("transactions: {transactions:#?}");

        Ok(VirtualTransaction { transactions, payload })
    }
}

impl VirtualTransaction {
    pub fn transactions(&self) -> Vec<MutableTransaction> {
        self.transactions.clone()
    }
}
