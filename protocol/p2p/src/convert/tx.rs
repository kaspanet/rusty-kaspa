use super::{error::ConversionError, option::TryIntoOptionEx};
use crate::pb as protowire;
use consensus_core::{
    subnets::SubnetworkId,
    tx::{ScriptPublicKey, Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
};

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::TransactionId> for TransactionId {
    type Error = ConversionError;

    fn try_from(value: protowire::TransactionId) -> Result<Self, Self::Error> {
        Ok(Self::from_bytes(value.bytes.as_slice().try_into()?))
    }
}

impl TryFrom<protowire::Outpoint> for TransactionOutpoint {
    type Error = ConversionError;

    fn try_from(item: protowire::Outpoint) -> Result<Self, Self::Error> {
        Ok(Self::new(item.transaction_id.try_into_ex()?, item.index))
    }
}

impl TryFrom<protowire::ScriptPublicKey> for ScriptPublicKey {
    type Error = ConversionError;

    fn try_from(value: protowire::ScriptPublicKey) -> Result<Self, Self::Error> {
        Ok(Self::from_vec(value.version.try_into()?, value.script))
    }
}

impl TryFrom<protowire::UtxoEntry> for UtxoEntry {
    type Error = ConversionError;

    fn try_from(value: protowire::UtxoEntry) -> Result<Self, Self::Error> {
        Ok(Self::new(value.amount, value.script_public_key.try_into_ex()?, value.block_daa_score, value.is_coinbase))
    }
}

impl TryFrom<protowire::OutpointAndUtxoEntryPair> for (TransactionOutpoint, UtxoEntry) {
    type Error = ConversionError;

    fn try_from(value: protowire::OutpointAndUtxoEntryPair) -> Result<Self, Self::Error> {
        Ok((value.outpoint.try_into_ex()?, value.utxo_entry.try_into_ex()?))
    }
}

impl TryFrom<protowire::TransactionInput> for TransactionInput {
    type Error = ConversionError;

    fn try_from(value: protowire::TransactionInput) -> Result<Self, Self::Error> {
        Ok(Self::new(value.previous_outpoint.try_into_ex()?, value.signature_script, value.sequence, value.sig_op_count.try_into()?))
    }
}

impl TryFrom<protowire::TransactionOutput> for TransactionOutput {
    type Error = ConversionError;

    fn try_from(output: protowire::TransactionOutput) -> Result<Self, Self::Error> {
        Ok(Self::new(output.value, output.script_public_key.try_into_ex()?))
    }
}

impl TryFrom<protowire::SubnetworkId> for SubnetworkId {
    type Error = ConversionError;

    fn try_from(value: protowire::SubnetworkId) -> Result<Self, Self::Error> {
        Ok(value.bytes.as_slice().try_into()?)
    }
}

impl TryFrom<protowire::TransactionMessage> for Transaction {
    type Error = ConversionError;

    fn try_from(tx: protowire::TransactionMessage) -> Result<Self, Self::Error> {
        Ok(Self::new(
            tx.version.try_into()?,
            tx.inputs.into_iter().map(|i| i.try_into()).collect::<Result<Vec<TransactionInput>, Self::Error>>()?,
            tx.outputs.into_iter().map(|i| i.try_into()).collect::<Result<Vec<TransactionOutput>, Self::Error>>()?,
            tx.lock_time,
            tx.subnetwork_id.try_into_ex()?,
            tx.gas,
            tx.payload,
        ))
    }
}
