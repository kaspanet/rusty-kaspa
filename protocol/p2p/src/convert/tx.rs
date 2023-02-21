use super::{error::ConversionError, option::TryIntoOptionEx};
use crate::pb as protowire;
use consensus_core::tx::{ScriptPublicKey, TransactionId, TransactionOutpoint, UtxoEntry};

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
