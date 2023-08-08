use crate::result::Result;
use crate::runtime::Account;
use crate::tx::{Fees, PaymentDestination};
use crate::utxo::{UtxoContext, UtxoEntryReference, UtxoSelectionContext};
use crate::Events;
use kaspa_addresses::Address;
use kaspa_consensus_core::networktype::NetworkType;
use std::sync::Arc;
use workflow_core::channel::Multiplexer;

pub struct GeneratorSettings {
    // Network type
    pub network_type: NetworkType,
    // Event multiplexer
    pub multiplexer: Option<Multiplexer<Events>>,
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
    pub final_priority_fee: Fees,
    // final transaction outputs
    pub final_transaction_destination: PaymentDestination,
    // payload
    pub final_transaction_payload: Option<Vec<u8>>,
}

impl GeneratorSettings {
    pub async fn try_new_with_account(
        account: &Account,
        final_transaction_destination: PaymentDestination,
        final_priority_fee: Fees,
        final_transaction_payload: Option<Vec<u8>>,
    ) -> Result<Self> {
        let network_type = account.utxo_context().processor().network_id()?.into();
        let change_address = account.change_address().await?;
        let multiplexer = account.wallet().multiplexer().clone();
        let inner = account.inner();
        let sig_op_count = inner.stored.pub_key_data.keys.len() as u8;
        let minimum_signatures = inner.stored.minimum_signatures;

        let utxo_selector = Arc::new(UtxoSelectionContext::new(account.utxo_context()));

        let settings = GeneratorSettings {
            network_type,
            multiplexer: Some(multiplexer),
            sig_op_count,
            minimum_signatures,
            change_address,
            utxo_iterator: Box::new(utxo_selector.iter()),
            utxo_context: Some(account.utxo_context().clone()),

            final_priority_fee,
            final_transaction_destination,
            final_transaction_payload,
        };

        Ok(settings)
    }

    pub fn try_new_with_iterator(
        utxo_iterator: Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static>,
        multiplexer: Option<Multiplexer<Events>>,
        sig_op_count: u8,
        minimum_signatures: u16,
        change_address: Address,
        final_transaction_destination: PaymentDestination,
        final_priority_fee: Fees,
        final_transaction_payload: Option<Vec<u8>>,
    ) -> Result<Self> {
        let network_type = NetworkType::try_from(change_address.prefix)?;

        let settings = GeneratorSettings {
            network_type,
            multiplexer,
            sig_op_count,
            minimum_signatures,
            change_address,
            utxo_iterator: Box::new(utxo_iterator),
            utxo_context: None,

            final_priority_fee,
            final_transaction_destination,
            final_transaction_payload,
        };

        Ok(settings)
    }
}
