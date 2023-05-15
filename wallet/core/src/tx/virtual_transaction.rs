// work in progress

use crate::imports::*;
use crate::keypair::PrivateKey;
use crate::signer::{sign_mutable_transaction, PrivateKeyArrayOrSigner};
use crate::tx::{
    create_transaction, MutableTransaction, PaymentOutputs, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput,
};
use crate::utils::{
    calculate_mass, calculate_minimum_transaction_fee, get_consensus_params_by_address, MAXIMUM_STANDARD_TRANSACTION_MASS,
};
use crate::utxo::{SelectionContext, UtxoEntry, UtxoEntryReference};
use crate::Signer;
use kaspa_addresses::Address;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::{subnets::SubnetworkId, tx};
use kaspa_rpc_core::SubmitTransactionRequest;
use kaspa_txscript::pay_to_address_script;
use kaspa_wrpc_client::wasm::RpcClient;
use workflow_wasm::tovalue::to_value;

#[wasm_bindgen]
pub struct LimitCalcStrategy {
    #[wasm_bindgen(skip)]
    pub strategy: LimitStrategy,
}

#[wasm_bindgen]
impl LimitCalcStrategy {
    pub fn calculated() -> LimitCalcStrategy {
        LimitStrategy::Calculated.into()
    }
    pub fn inputs(inputs: u8) -> LimitCalcStrategy {
        LimitStrategy::Inputs(inputs).into()
    }
}

pub enum LimitStrategy {
    Calculated,
    Inputs(u8),
}

impl From<LimitStrategy> for LimitCalcStrategy {
    fn from(strategy: LimitStrategy) -> Self {
        Self { strategy }
    }
}
impl From<LimitCalcStrategy> for LimitStrategy {
    fn from(value: LimitCalcStrategy) -> Self {
        value.strategy
    }
}

pub struct Transactions {
    pub transactions: Vec<MutableTransaction>,
    pub inputs: Vec<TransactionInput>,
    pub utxos: Vec<UtxoEntry>,
    pub amount: u64,
}

impl Transactions {
    pub async fn merge(
        &mut self,
        outputs: &PaymentOutputs,
        change_address: &Address,
        priority_fee: u64,
        payload: Vec<u8>,
        minimum_signatures: u16,
    ) -> crate::Result<bool> {
        if self.amount < priority_fee {
            return Err(format!("final amount({}) < priority fee({priority_fee})", self.amount).into());
        }

        let amount_after_priority_fee = self.amount - priority_fee;

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
            let output = TransactionOutput::new(change, &pay_to_address_script(change_address));
            if !output.is_dust() {
                change_output = Some(output.clone());
                outputs_.push(output);
            }
        }

        let tx = Transaction::new(
            0,
            self.inputs.clone(),
            outputs_,
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            payload,
        )?;

        let consensus_params = get_consensus_params_by_address(change_address);

        let fee = calculate_minimum_transaction_fee(&tx, &consensus_params, true, minimum_signatures);
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

        let mtx = MutableTransaction::new(&tx, &self.utxos.clone().into());
        self.transactions.push(mtx);

        Ok(true)
    }
}

pub async fn calculate_chunk_size(
    tx: &Transaction,
    total_mass: u64,
    params: &Params,
    estimate_signature_mass: bool,
    minimum_signatures: u16,
) -> crate::Result<u64> {
    let (mass_per_input, mass_without_inputs) =
        mass_per_input_and_mass_without_inputs(tx, total_mass, params, estimate_signature_mass, minimum_signatures);

    let output = match tx.inner().outputs.get(0).cloned() {
        Some(output) => output,
        None => {
            return Err("Minimum one output is require to calculate chunk size".to_string().into());
        }
    };

    let split_tx_without_inputs = Transaction::new(
        0,
        vec![],
        vec![output],
        0,
        SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        0,
        vec![],
    )
    .unwrap();

    let split_tx_mass_without_inputs = calculate_mass(&split_tx_without_inputs, params, estimate_signature_mass, minimum_signatures);

    log_trace!("mass_per_input: {mass_per_input}");
    log_trace!("total_mass: {total_mass}");
    log_trace!("mass_without_inputs: {mass_without_inputs}");
    log_trace!("split_tx_mass_without_inputs: {split_tx_mass_without_inputs}");
    let inputs_max_mass = MAXIMUM_STANDARD_TRANSACTION_MASS - split_tx_mass_without_inputs;
    log_trace!("inputs_max_mass: {inputs_max_mass}");
    log_trace!("chunk_size: {}", inputs_max_mass / mass_per_input);
    Ok(inputs_max_mass / mass_per_input)
}

pub fn mass_per_input_and_mass_without_inputs(
    tx: &Transaction,
    total_mass: u64,
    params: &Params,
    estimate_signature_mass: bool,
    minimum_signatures: u16,
) -> (u64, u64) {
    //let total_mass = calculate_mass(tx, params, estimate_signature_mass);
    let mut tx_inner_clone = tx.inner().clone();
    tx_inner_clone.inputs = vec![];
    let tx_clone = Transaction::new_with_inner(tx_inner_clone);

    let mass_without_inputs = calculate_mass(&tx_clone, params, estimate_signature_mass, minimum_signatures);

    let input_mass = total_mass - mass_without_inputs;
    let input_count = tx.inner().inputs.len() as u64;
    let mut mass_per_input = input_mass / input_count;
    if input_mass % input_count > 0 {
        mass_per_input += 1;
    }

    (mass_per_input, mass_without_inputs)
}

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
    pub async fn new(
        sig_op_count: u8,
        minimum_signatures: u16,
        utxo_selection: &SelectionContext,
        outputs: &PaymentOutputs,
        change_address: &Address,
        priority_fee_sompi: Option<u64>,
        payload: Vec<u8>,
        limit_calc_strategy: LimitCalcStrategy,
    ) -> crate::Result<VirtualTransaction> {
        log_trace!("VirtualTransaction...");
        log_trace!("utxo_selection.transaction_amount: {:?}", utxo_selection.transaction_amount);
        log_trace!("utxo_selection.total_selected_amount: {:?}", utxo_selection.total_selected_amount);
        log_trace!("outputs.outputs: {:?}", outputs.outputs);
        log_trace!("change_address: {:?}", change_address.to_string());

        let consensus_params = get_consensus_params_by_address(change_address);

        let entries = &utxo_selection.selected_entries;

        log_trace!("entries.len(): {:?}", entries.len());

        let priority_fee = priority_fee_sompi.unwrap_or(0);

        match limit_calc_strategy.strategy {
            LimitStrategy::Calculated => {
                let mtx = create_transaction(
                    sig_op_count,
                    utxo_selection,
                    outputs,
                    change_address,
                    minimum_signatures,
                    priority_fee_sompi,
                    Some(payload.clone()),
                )?;

                let tx = mtx.tx().clone();

                let mass = calculate_mass(&tx, &consensus_params, true, minimum_signatures);
                if mass <= MAXIMUM_STANDARD_TRANSACTION_MASS {
                    return Ok(VirtualTransaction { transactions: vec![mtx], payload });
                }

                let max_inputs = calculate_chunk_size(&tx, mass, &consensus_params, true, minimum_signatures).await? as usize;
                let mut txs =
                    Self::split_utxos(entries, max_inputs, max_inputs, change_address, sig_op_count, minimum_signatures).await?;
                txs.merge(outputs, change_address, priority_fee, payload.clone(), minimum_signatures).await?;
                Ok(VirtualTransaction { transactions: txs.transactions, payload })
            }
            LimitStrategy::Inputs(inputs) => {
                let max_inputs = inputs as usize;
                let mut txs =
                    Self::split_utxos(entries, max_inputs, max_inputs, change_address, sig_op_count, minimum_signatures).await?;
                txs.merge(outputs, change_address, priority_fee, payload.clone(), minimum_signatures).await?;
                Ok(VirtualTransaction { transactions: txs.transactions, payload })
            }
        }
    }

    #[wasm_bindgen(js_name = "transactions")]
    pub fn transaction_array(&self) -> Array {
        Array::from_iter(self.transactions.clone().into_iter().map(JsValue::from))
    }

    #[wasm_bindgen(js_name = "sign")]
    pub fn js_sign(&mut self, signer: PrivateKeyArrayOrSigner, verify_sig: bool) -> crate::Result<()> {
        if signer.is_array() {
            let mut private_keys: Vec<[u8; 32]> = vec![];
            for key in Array::from(&signer).iter() {
                let key = PrivateKey::try_from(&key).map_err(|_| Error::Custom("Unable to cast PrivateKey".to_string()))?;
                private_keys.push(key.secret_bytes());
            }
            self.sign(&private_keys, verify_sig)?;
        } else {
            let signer = Signer::try_from(&JsValue::from(signer)).map_err(|_| Error::Custom("Unable to cast Signer".to_string()))?;
            log_trace!("\nSigning via Signer: {signer:?}....\n");
            self.sign_with_signer(&signer, verify_sig)?;
        }
        Ok(())
    }

    #[wasm_bindgen(js_name = "submit")]
    pub async fn js_submit(&mut self, rpc: &RpcClient, allow_orphan: bool) -> crate::Result<Array> {
        let result = Array::new();
        for transaction in self.transactions.clone() {
            result.push(&to_value(
                &rpc.submit_transaction(SubmitTransactionRequest { transaction: transaction.try_into()?, allow_orphan }).await?,
            )?);
        }

        Ok(result)
    }
}

impl VirtualTransaction {
    pub fn sign_with_signer(&mut self, signer: &Signer, verify_sig: bool) -> crate::Result<()> {
        let mut transactions = vec![];
        for mtx in self.transactions.clone() {
            transactions.push(signer.sign_transaction(mtx, verify_sig)?);
        }
        self.transactions = transactions;
        Ok(())
    }

    pub fn sign(&mut self, private_keys: &Vec<[u8; 32]>, verify_sig: bool) -> crate::Result<()> {
        let mut transactions = vec![];
        for mtx in self.transactions.clone() {
            transactions.push(sign_mutable_transaction(mtx, private_keys, verify_sig)?);
        }
        self.transactions = transactions;
        Ok(())
    }

    pub fn transactions(&self) -> &Vec<MutableTransaction> {
        &self.transactions
    }

    pub async fn split_utxos(
        utxos_entries: &Vec<UtxoEntryReference>,
        chunk_size: usize,
        max_inputs: usize,
        change_address: &Address,
        sig_op_count: u8,
        minimum_signatures: u16,
    ) -> crate::Result<Transactions> {
        let mut final_inputs = vec![];
        let mut final_utxos = vec![];
        let mut final_amount = 0;
        let mut transactions = vec![];

        if utxos_entries.len() <= max_inputs {
            utxos_entries.iter().for_each(|utxo_ref| {
                final_amount += utxo_ref.utxo.amount();
                final_utxos.push(utxo_ref.data());
                final_inputs.push(TransactionInput::new(utxo_ref.utxo.outpoint.clone(), vec![], 0, sig_op_count));
                log_debug!("final_amount: {final_amount}, transaction_id: {}\r\n", utxo_ref.utxo.outpoint.get_transaction_id());
            });

            return Ok(Transactions { transactions, inputs: final_inputs, utxos: final_utxos, amount: final_amount });
        }

        let consensus_params = get_consensus_params_by_address(change_address);

        let chunks = utxos_entries.chunks(chunk_size).collect::<Vec<&[UtxoEntryReference]>>();
        for chunk in chunks {
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

            let script_public_key = pay_to_address_script(change_address);
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

            let fee = calculate_minimum_transaction_fee(&tx, &consensus_params, true, minimum_signatures);
            if amount <= fee {
                log_debug!("amount<=fee: {amount}, {fee}\r\n");
                continue;
            }
            let amount_after_fee = amount - fee;

            tx.inner().outputs[0].set_value(amount_after_fee);
            if tx.inner().outputs[0].is_dust() {
                log_debug!("outputs is dust: {}\r\n", amount_after_fee);
                continue;
            }

            let transaction_id = tx.finalize().unwrap().to_str();

            final_amount += amount_after_fee;
            log_debug!("final_amount: {final_amount}, transaction_id: {}\r\n", transaction_id);
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
            final_inputs.push(TransactionInput::new(TransactionOutpoint::new(&transaction_id, 0).unwrap(), vec![], 0, sig_op_count));

            transactions.push(MutableTransaction::new(&tx, &entries.into()));
        }

        Ok(Transactions { transactions, inputs: final_inputs, utxos: final_utxos, amount: final_amount })
    }
}
