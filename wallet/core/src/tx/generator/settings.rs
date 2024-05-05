//!
//! Transaction [`GeneratorSettings`] used when
//! constructing and instance of the [`Generator`](crate::tx::Generator).
//!

use crate::events::Events;
use crate::imports::*;
use crate::result::Result;
use crate::tx::{Fees, PaymentDestination};
use crate::utxo::{UtxoContext, UtxoEntryReference, UtxoIterator};
use kaspa_addresses::Address;
use workflow_core::channel::Multiplexer;

pub struct GeneratorSettings {
    // Network type
    pub network_id: NetworkId,
    // Event multiplexer
    pub multiplexer: Option<Multiplexer<Box<Events>>>,
    // Utxo iterator
    pub utxo_iterator: Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static>,
    // Utxo Context
    pub source_utxo_context: Option<UtxoContext>,
    // typically a number of keys required to sign the transaction
    pub sig_op_count: u8,
    // number of minimum signatures required to sign the transaction
    pub minimum_signatures: u16,
    // change address
    pub change_address: Address,
    // applies only to the final transaction
    pub final_transaction_priority_fee: Fees,
    // final transaction outputs
    pub final_transaction_destination: PaymentDestination,
    // payload
    pub final_transaction_payload: Option<Vec<u8>>,
    // transaction is a transfer between accounts
    pub destination_utxo_context: Option<UtxoContext>,
}

// impl std::fmt::Debug for GeneratorSettings {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("GeneratorSettings")
//             .field("network_id", &self.network_id)
//             // .field("multiplexer", &self.multiplexer)
//             .field("utxo_iterator", &"Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static>")
//             // .field("source_utxo_context", &self.source_utxo_context)
//             .field("sig_op_count", &self.sig_op_count)
//             .field("minimum_signatures", &self.minimum_signatures)
//             .field("change_address", &self.change_address)
//             .field("final_transaction_priority_fee", &self.final_transaction_priority_fee)
//             .field("final_transaction_destination", &self.final_transaction_destination)
//             .field("final_transaction_payload", &self.final_transaction_payload)
//             // .field("destination_utxo_context", &self.destination_utxo_context)
//             .finish()
//     }
// }

impl GeneratorSettings {
    pub fn try_new_with_account(
        account: Arc<dyn Account>,
        final_transaction_destination: PaymentDestination,
        final_priority_fee: Fees,
        final_transaction_payload: Option<Vec<u8>>,
    ) -> Result<Self> {
        let network_id = account.utxo_context().processor().network_id()?;
        let change_address = account.change_address()?;
        let multiplexer = account.wallet().multiplexer().clone();
        let sig_op_count = account.sig_op_count();
        let minimum_signatures = account.minimum_signatures();

        let utxo_iterator = UtxoIterator::new(account.utxo_context());

        let settings = GeneratorSettings {
            network_id,
            multiplexer: Some(multiplexer),
            sig_op_count,
            minimum_signatures,
            change_address,
            utxo_iterator: Box::new(utxo_iterator),
            source_utxo_context: Some(account.utxo_context().clone()),

            final_transaction_priority_fee: final_priority_fee,
            final_transaction_destination,
            final_transaction_payload,
            destination_utxo_context: None,
        };

        Ok(settings)
    }

    pub fn try_new_with_context(
        utxo_context: UtxoContext,
        change_address: Address,
        sig_op_count: u8,
        minimum_signatures: u16,
        final_transaction_destination: PaymentDestination,
        final_priority_fee: Fees,
        final_transaction_payload: Option<Vec<u8>>,
        multiplexer: Option<Multiplexer<Box<Events>>>,
    ) -> Result<Self> {
        let network_id = utxo_context.processor().network_id()?;
        let utxo_iterator = UtxoIterator::new(&utxo_context);

        let settings = GeneratorSettings {
            network_id,
            multiplexer,
            sig_op_count,
            minimum_signatures,
            change_address,
            utxo_iterator: Box::new(utxo_iterator),
            source_utxo_context: Some(utxo_context),

            final_transaction_priority_fee: final_priority_fee,
            final_transaction_destination,
            final_transaction_payload,
            destination_utxo_context: None,
        };

        Ok(settings)
    }

    pub fn try_new_with_iterator(
        network_id: NetworkId,
        utxo_iterator: Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static>,
        change_address: Address,
        sig_op_count: u8,
        minimum_signatures: u16,
        final_transaction_destination: PaymentDestination,
        final_priority_fee: Fees,
        final_transaction_payload: Option<Vec<u8>>,
        multiplexer: Option<Multiplexer<Box<Events>>>,
    ) -> Result<Self> {
        let settings = GeneratorSettings {
            network_id,
            multiplexer,
            sig_op_count,
            minimum_signatures,
            change_address,
            utxo_iterator: Box::new(utxo_iterator),
            source_utxo_context: None,

            final_transaction_priority_fee: final_priority_fee,
            final_transaction_destination,
            final_transaction_payload,
            destination_utxo_context: None,
        };

        Ok(settings)
    }

    pub fn utxo_context_transfer(mut self, destination_utxo_context: &UtxoContext) -> Self {
        self.destination_utxo_context = Some(destination_utxo_context.clone());
        self
    }
}
