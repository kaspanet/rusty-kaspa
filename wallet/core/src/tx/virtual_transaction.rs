use crate::tx::transaction::Transaction;
use crate::tx::MutableTransaction;
use crate::tx::Outputs;
use crate::tx::TransactionInput;
use crate::tx::TransactionOutpoint;
use crate::tx::TransactionOutput;
use crate::utxo::SelectionContext;
use crate::utxo::UtxoEntry;
use crate::utxo::UtxoEntryReference;
use kaspa_addresses::Address;
use kaspa_consensus_core::{subnets::SubnetworkId, tx};
use kaspa_txscript::pay_to_address_script;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
use workflow_log::log_trace;

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
        utxo_selection: SelectionContext,
        outputs: Outputs,
        change_address: Address,
        payload: Vec<u8>,
    ) -> crate::Result<VirtualTransaction> {
        log_trace!("VirtualTransaction...");
        log_trace!("utxo_selection.transaction_amount: {:?}", utxo_selection.transaction_amount);
        log_trace!("outputs.outputs: {:?}", outputs.outputs);
        log_trace!("change_address: {change_address:?}");

        let entries = &utxo_selection.selected_entries;

        let chunks = entries.chunks(80).collect::<Vec<&[UtxoEntryReference]>>();

        //let mut mutable: std::vec::Vec<T> = vec![];

        // ---------------------------------------------
        // TODO - get a set of destination addresses
        //let secp = Secp256k1::new();
        //let (_secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        //let script_pub_key = ScriptVec::from_slice(&public_key.serialize());
        //let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();
        // ---------------------------------------------
        let mut final_inputs = vec![];
        let mut final_utxos = vec![];
        let mut final_amount = 0;
        let mut transactions = chunks
            .into_iter()
            .map(|chunk| {
                let utxos = chunk.iter().map(|reference| reference.utxo.clone()).collect::<Vec<Arc<UtxoEntry>>>();

                // let prev_tx_id = TransactionId::default();
                let mut amount = 0;
                let mut entries = vec![];

                let inputs = utxos
                    .iter()
                    .enumerate()
                    .map(|(sequence, utxo)| {
                        amount += utxo.utxo_entry.amount;
                        entries.push(utxo.as_ref().clone());
                        TransactionInput::new(utxo.outpoint, vec![], sequence as u64, 0)
                    })
                    .collect::<Vec<TransactionInput>>();

                let amount_after_fee = amount - 500; //TODO: calculate Fee

                let script_public_key = pay_to_address_script(&change_address);
                let tx = Transaction::new(
                    0,
                    inputs,
                    vec![TransactionOutput::new(amount_after_fee, &script_public_key)],
                    0,
                    SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
                    0,
                    vec![],
                );

                let transaction_id = tx.id();

                final_amount += amount_after_fee;
                final_utxos.push(UtxoEntry {
                    address: change_address.clone(),
                    outpoint: TransactionOutpoint::new(&transaction_id, 0),
                    utxo_entry: tx::UtxoEntry {
                        amount: amount_after_fee,
                        script_public_key,
                        block_daa_score: u64::MAX,
                        is_coinbase: false,
                    },
                });
                final_inputs.push(TransactionInput::new(
                    TransactionOutpoint::new(&transaction_id, 0),
                    vec![],
                    final_inputs.len() as u64,
                    0,
                ));

                MutableTransaction { tx: Arc::new(Mutex::new(tx)), entries: entries.into() }
            })
            .collect::<Vec<MutableTransaction>>();

        let fee = 500; //TODO: calculate Fee
        let amount_after_fee = final_amount - fee;

        let mut outputs_ = vec![];
        let mut total_amount = 0;
        for output in &outputs.outputs {
            total_amount += output.amount;
            outputs_.push(TransactionOutput::new(output.amount, &pay_to_address_script(&output.address)));
        }

        if total_amount > amount_after_fee {
            return Err("total_amount({total_amount}) > amount_after_fee({amount_after_fee})".to_string().into());
        }

        let change = amount_after_fee - total_amount;
        let dust = 500;
        if change > dust {
            outputs_.push(TransactionOutput::new(change, &pay_to_address_script(&change_address)));
        }

        let tx = Transaction::new(
            0,
            final_inputs,
            outputs_,
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            payload.clone(),
        );

        let mtx = MutableTransaction { tx: Arc::new(Mutex::new(tx)), entries: final_utxos.into() };
        transactions.push(mtx);

        log_trace!("transactions: {transactions:#?}");

        Ok(VirtualTransaction { transactions, payload })
    }
}
