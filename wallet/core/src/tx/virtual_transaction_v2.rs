use crate::imports::*;
// use crate::keypair::PrivateKey;
// use crate::signer::{sign_mutable_transaction, PrivateKeyArrayOrSigner};
use crate::tx::get_consensus_params_by_address;
use crate::tx::{
    // Transaction, TransactionInput, TransactionOutpoint, TransactionOutput,
    // LimitStrategy,
    LimitCalcStrategy,
    // create_transaction,
    MutableTransaction,
    PaymentOutputs,
};
// use crate::utxo::selection::UtxoSelectionContextInterface;
// use crate::utxo::{UtxoEntry, UtxoEntryReference, UtxoSelectionContext};
use crate::Signer;
use kaspa_addresses::Address;
// use kaspa_consensus_core::config::params::Params;
// use kaspa_consensus_core::{subnets::SubnetworkId, tx};
// use kaspa_rpc_core::SubmitTransactionRequest;
// use kaspa_txscript::pay_to_address_script;
// use kaspa_wrpc_client::wasm::RpcClient;
use workflow_core::abortable::Abortable;
// use workflow_wasm::tovalue::to_value;
use crate::utxo::UtxoStream;

/// `VirtualTransaction` envelops a collection of multiple related `kaspa_wallet_core::MutableTransaction` instances.
#[derive(Clone, Debug)]
#[wasm_bindgen]
#[allow(dead_code)] //TODO: remove me
pub struct VirtualTransactionV2 {
    transactions: Vec<MutableTransaction>,
    payload: Vec<u8>,
    // include_fees : bool,
}

impl VirtualTransactionV2 {
    pub async fn try_new(
        sig_op_count: u8,
        minimum_signatures: u16,
        // ctx: &mut UtxoSelectionContext,
        stream: UtxoStream,
        outputs: &PaymentOutputs,
        change_address: &Address,
        priority_fee_sompi: Option<u64>,
        payload: Vec<u8>,
        limit_calc_strategy: LimitCalcStrategy,
        abortable: &Abortable,
    ) -> crate::Result<VirtualTransactionV2> {
        let transaction_amount = outputs.amount() + priority_fee_sompi.as_ref().cloned().unwrap_or_default();
        // ctx.select(transaction_amount).await?;
        // let selected_amount = ctx.selected_amount();

        log_trace!("VirtualTransaction...");
        log_trace!("utxo_selection.transaction_amount: {:?}", transaction_amount);
        // log_trace!("utxo_selection.total_selected_amount: {:?}", selected_amount);
        log_trace!("outputs.outputs: {:?}", outputs.outputs);
        log_trace!("change_address: {:?}", change_address.to_string());

        let consensus_params = get_consensus_params_by_address(change_address);

        let priority_fee = priority_fee_sompi.unwrap_or(0);

        let tansactions = vec![];

        let accumulator: u64 = 0;

        let mut inputs = vec![];

        while let Some(utxo) = stream.try_next().await? {}

        while accumulator < transaction_amount {}

        Ok(VirtualTransactionV2 { transactions: vec![], payload })

        /*
                abortable.check()?;
                let mtx = create_transaction(
                    sig_op_count,
                    ctx,
                    outputs,
                    change_address,
                    minimum_signatures,
                    priority_fee_sompi,
                    Some(payload.clone()),
                )?;

                let tx = mtx.tx().clone();
                abortable.check()?;
                let mass = calculate_mass(&tx, &consensus_params, true, minimum_signatures);
                if mass <= MAXIMUM_STANDARD_TRANSACTION_MASS {
                    return Ok(VirtualTransactionV2 { transactions: vec![mtx], payload });
                }
                abortable.check()?;
                let max_inputs = calculate_chunk_size(&tx, mass, &consensus_params, true, minimum_signatures).await? as usize;
                abortable.check()?;
                let entries = ctx.selected_entries();

                let mut txs =
                    Self::split_utxos(entries, max_inputs, max_inputs, change_address, sig_op_count, minimum_signatures, abortable)
                        .await?;
                abortable.check()?;
                txs.merge(outputs, change_address, priority_fee, payload.clone(), minimum_signatures).await?;
                Ok(VirtualTransactionV2 { transactions: txs.transactions, payload })
        */
    }

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
}
