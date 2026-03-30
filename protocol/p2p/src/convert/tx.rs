use super::{error::ConversionError, option::TryIntoOptionEx};
use crate::pb as protowire;
use kaspa_consensus_core::{
    subnets::SubnetworkId,
    tx::{
        CovenantBinding, ScriptPublicKey, Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput,
        TxInputMass, UtxoEntry,
    },
};
use kaspa_hashes::Hash;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<Hash> for protowire::TransactionId {
    fn from(hash: Hash) -> Self {
        Self { bytes: Vec::from(hash.as_bytes()) }
    }
}

impl From<&Hash> for protowire::TransactionId {
    fn from(hash: &Hash) -> Self {
        Self { bytes: Vec::from(hash.as_bytes()) }
    }
}

impl From<&SubnetworkId> for protowire::SubnetworkId {
    fn from(id: &SubnetworkId) -> Self {
        Self { bytes: Vec::from(<SubnetworkId as AsRef<[u8]>>::as_ref(id)) }
    }
}

impl From<&TransactionOutpoint> for protowire::Outpoint {
    fn from(outpoint: &TransactionOutpoint) -> Self {
        Self { transaction_id: Some(outpoint.transaction_id.into()), index: outpoint.index }
    }
}

impl From<&ScriptPublicKey> for protowire::ScriptPublicKey {
    fn from(script_public_key: &ScriptPublicKey) -> Self {
        Self { script: script_public_key.script().to_vec(), version: script_public_key.version() as u32 }
    }
}

impl From<&TransactionInput> for protowire::TransactionInput {
    fn from(input: &TransactionInput) -> Self {
        Self {
            previous_outpoint: Some((&input.previous_outpoint).into()),
            signature_script: input.signature_script.clone(),
            sequence: input.sequence,
            mass: match input.mass {
                TxInputMass::SigopCount(count) => u8::from(count) as u32,
                TxInputMass::ComputeBudget(budget) => u16::from(budget) as u32,
            },
        }
    }
}

impl From<&CovenantBinding> for protowire::CovenantBinding {
    fn from(covenant: &CovenantBinding) -> Self {
        Self { authorizing_input: covenant.authorizing_input as u32, covenant_id: Some(covenant.covenant_id.into()) }
    }
}

impl From<&TransactionOutput> for protowire::TransactionOutput {
    fn from(output: &TransactionOutput) -> Self {
        Self {
            value: output.value,
            script_public_key: Some((&output.script_public_key).into()),
            covenant: output.covenant.as_ref().map(protowire::CovenantBinding::from),
        }
    }
}

impl From<&Transaction> for protowire::TransactionMessage {
    fn from(tx: &Transaction) -> Self {
        Self {
            version: tx.version as u32,
            inputs: tx.inputs.iter().map(|input| input.into()).collect(),
            outputs: tx.outputs.iter().map(|output| output.into()).collect(),
            lock_time: tx.lock_time,
            subnetwork_id: Some((&tx.subnetwork_id).into()),
            gas: tx.gas,
            payload: tx.payload.clone(),
            mass: tx.mass(),
        }
    }
}

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
        Ok(Self::new(
            value.amount,
            value.script_public_key.try_into_ex()?,
            value.block_daa_score,
            value.is_coinbase,
            value.covenant_id.map(|x| x.try_into()).transpose()?,
        ))
    }
}

impl TryFrom<protowire::OutpointAndUtxoEntryPair> for (TransactionOutpoint, UtxoEntry) {
    type Error = ConversionError;

    fn try_from(value: protowire::OutpointAndUtxoEntryPair) -> Result<Self, Self::Error> {
        Ok((value.outpoint.try_into_ex()?, value.utxo_entry.try_into_ex()?))
    }
}

struct ProtoInputWithVersion {
    version: u32,
    input: protowire::TransactionInput,
}

impl TryFrom<ProtoInputWithVersion> for TransactionInput {
    type Error = ConversionError;

    fn try_from(value: ProtoInputWithVersion) -> Result<Self, Self::Error> {
        Ok(Self {
            previous_outpoint: value.input.previous_outpoint.try_into_ex()?,
            signature_script: value.input.signature_script,
            sequence: value.input.sequence,
            mass: if TxInputMass::has_compute_budget_field(value.version as u16) {
                TxInputMass::ComputeBudget(u16::try_from(value.input.mass)?.into())
            } else {
                TxInputMass::SigopCount(u8::try_from(value.input.mass)?.into())
            },
        })
    }
}

impl TryFrom<protowire::TransactionOutput> for TransactionOutput {
    type Error = ConversionError;

    fn try_from(output: protowire::TransactionOutput) -> Result<Self, Self::Error> {
        Ok(Self::with_covenant(
            output.value,
            output.script_public_key.try_into_ex()?,
            output.covenant.map(|c| c.try_into()).transpose()?,
        ))
    }
}

impl TryFrom<protowire::CovenantBinding> for CovenantBinding {
    type Error = ConversionError;

    fn try_from(covenant: protowire::CovenantBinding) -> Result<Self, Self::Error> {
        Ok(CovenantBinding {
            authorizing_input: covenant.authorizing_input.try_into()?,
            covenant_id: covenant.covenant_id.try_into_ex()?,
        })
    }
}

impl TryFrom<protowire::TransactionMessage> for Transaction {
    type Error = ConversionError;

    fn try_from(tx: protowire::TransactionMessage) -> Result<Self, Self::Error> {
        let version = tx.version;
        let transaction = Self::new(
            tx.version.try_into()?,
            tx.inputs
                .into_iter()
                .map(|i| ProtoInputWithVersion { version, input: i }.try_into())
                .collect::<Result<Vec<TransactionInput>, Self::Error>>()?,
            tx.outputs.into_iter().map(|i| i.try_into()).collect::<Result<Vec<TransactionOutput>, Self::Error>>()?,
            tx.lock_time,
            tx.subnetwork_id.try_into_ex()?,
            tx.gas,
            tx.payload,
        );
        transaction.set_mass(tx.mass);
        Ok(transaction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_message_compute_budget_roundtrip() {
        let tx = Transaction::new(
            1,
            vec![TransactionInput::new_with_mass(
                TransactionOutpoint::new(Hash::from_u64_word(1), 0),
                vec![],
                0,
                TxInputMass::ComputeBudget(12_345.into()),
            )],
            vec![],
            42,
            SubnetworkId::from_bytes([3; 20]),
            7,
            vec![1, 2, 3],
        );
        tx.set_mass(54_321);

        let message: protowire::TransactionMessage = (&tx).into();
        assert_eq!(message.inputs[0].mass, 12_345);

        let received = Transaction::try_from(message).unwrap();
        assert_eq!(received.inputs.len(), 1);
        assert_eq!(received.inputs[0].mass.compute_budget(), Some(12_345));
        assert_eq!(received.mass(), 54_321);
    }
}
