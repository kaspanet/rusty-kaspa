use crate::result::Result;
use crate::runtime::Account;
use crate::tx::PaymentDestination;
use crate::utxo::{UtxoContext, UtxoEntryReference, UtxoSelectionContext};
use kaspa_addresses::Address;
use std::sync::Arc;

pub struct GeneratorSettings {
    // Utxo iterator
    pub utxo_iterator: Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static>,
    // Utxo Context
    pub utxo_context: Option<UtxoContext>,
    // typically a number of keys required to sign the transaction
    pub sig_op_count: u8,
    // number of minimum signatures required to sign the transaction
    pub minimum_signatures: u16,
    // change address
    pub change_address: Address,
    // applies only to the final transaction
    pub final_priority_fee: Option<u64>,
    // applies only to the final transaction
    pub final_include_fees_in_amount: bool,
    // final transaction outputs
    pub final_transaction_destination: PaymentDestination,
    // payload
    pub final_transaction_payload: Option<Vec<u8>>,
}

impl GeneratorSettings {
    pub async fn try_new_with_account(
        account: &Account,
        final_transaction_destination: PaymentDestination,
        final_priority_fee: Option<u64>,
        final_include_fees_in_amount: bool,
        final_transaction_payload: Option<Vec<u8>>,
    ) -> Result<Self> {
        let change_address = account.change_address().await?;
        let inner = account.inner();
        let sig_op_count = inner.stored.pub_key_data.keys.len() as u8;
        let minimum_signatures = inner.stored.minimum_signatures;

        let utxo_selector = Arc::new(UtxoSelectionContext::new(account.utxo_context()));

        let settings = GeneratorSettings {
            sig_op_count,
            minimum_signatures,
            change_address,
            utxo_iterator: Box::new(utxo_selector.iter()),
            utxo_context: Some(account.utxo_context().clone()),

            final_priority_fee,
            final_include_fees_in_amount,
            final_transaction_destination,
            final_transaction_payload,
        };

        Ok(settings)
    }
}
