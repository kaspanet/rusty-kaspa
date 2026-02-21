use crate::protowire::{self};
use crate::{from, try_from};
use kaspa_rpc_core::{FromRpcHex, RpcAddress, RpcError, RpcHash, RpcResult, RpcScriptClass, RpcScriptVec, ToRpcHex};
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcTransaction, protowire::RpcTransaction, {
    Self {
        version: item.version.into(),
        inputs: item.inputs.iter().map(protowire::RpcTransactionInput::from).collect(),
        outputs: item.outputs.iter().map(protowire::RpcTransactionOutput::from).collect(),
        lock_time: item.lock_time,
        subnetwork_id: item.subnetwork_id.to_string(),
        gas: item.gas,
        payload: item.payload.to_rpc_hex(),
        mass: item.mass,
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcOptionalTransaction, protowire::RpcTransaction, {
    Self {
        version: item.version.unwrap_or_default().into(),
        inputs: item.inputs.iter().map(protowire::RpcTransactionInput::from).collect(),
        outputs: item.outputs.iter().map(protowire::RpcTransactionOutput::from).collect(),
        lock_time: item.lock_time.unwrap_or_default(),
        subnetwork_id: item.subnetwork_id.as_ref().map(|x| x.to_string()).unwrap_or_default(),
        gas: item.gas.unwrap_or_default(),
        payload: item.payload.as_ref().map(|x| x.to_rpc_hex()).unwrap_or_default(),
        mass: item.mass.unwrap_or_default(),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionInput, protowire::RpcTransactionInput, {
    Self {
        previous_outpoint: Some((&item.previous_outpoint).into()),
        signature_script: item.signature_script.to_rpc_hex(),
        sequence: item.sequence,
        sig_op_count: item.sig_op_count.into(),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcOptionalTransactionInput, protowire::RpcTransactionInput, {
    Self {
        previous_outpoint: item.previous_outpoint.as_ref().map(protowire::RpcOutpoint::from),
        signature_script: item.signature_script.as_ref().map(|x| x.to_rpc_hex()).unwrap_or_default(),
        sequence: item.sequence.unwrap_or_default(),
        sig_op_count: item.sig_op_count.map(|x| x.into()).unwrap_or_default(),
        verbose_data: item.verbose_data.as_ref().map(protowire::RpcTransactionInputVerboseData::from),
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionOutput, protowire::RpcTransactionOutput, {
    Self {
        amount: item.value,
        script_public_key: Some((&item.script_public_key).into()),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcOptionalTransactionOutput, protowire::RpcTransactionOutput, {
    Self {
        amount: item.value.unwrap_or_default(),
        script_public_key: item.script_public_key.as_ref().map(|x| x.into()),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionOutpoint, protowire::RpcOutpoint, {
    Self { transaction_id: item.transaction_id.to_string(), index: item.index }
});

from!(item: &kaspa_rpc_core::RpcOptionalTransactionOutpoint, protowire::RpcOutpoint, {
    Self { transaction_id: item.transaction_id.as_ref().map(|x| x.to_string()).unwrap_or_default(), index: item.index.unwrap_or_default() }
});

from!(item: &kaspa_rpc_core::RpcUtxoEntry, protowire::RpcUtxoEntry, {
    Self {
        amount: item.amount,
        script_public_key: Some((&item.script_public_key).into()),
        block_daa_score: item.block_daa_score,
        is_coinbase: item.is_coinbase,
        verbose_data: None,
    }
});

from!(item: &kaspa_rpc_core::RpcOptionalUtxoEntry, protowire::RpcUtxoEntry, {
    Self {
        amount: item.amount.unwrap_or_default(),
        script_public_key: item.script_public_key.as_ref().map(|x| x.into()),
        block_daa_score: item.block_daa_score.unwrap_or_default(),
        is_coinbase: item.is_coinbase.unwrap_or_default(),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcOptionalUtxoEntryVerboseData, protowire::RpcUtxoEntryVerboseData, {
    Self {
        script_public_key_type: item.script_public_key_type.as_ref().map(|x| x.to_string()).unwrap_or_default(),
        script_public_key_address: item.script_public_key_address.as_ref().map(|x| x.to_string()).unwrap_or_default(),
    }
});

from!(item: &kaspa_rpc_core::RpcChainBlockAcceptedTransactions, protowire::RpcChainBlockAcceptedTransactions, {
    Self {
        chain_block_header: Some(protowire::RpcOptionalHeader::from(&item.chain_block_header)),
        accepted_transactions: item.accepted_transactions.iter().map(protowire::RpcOptionalTransaction::from).collect(),
    }
});

from!(item: &kaspa_rpc_core::RpcScriptPublicKey, protowire::RpcScriptPublicKey, {
    Self { version: item.version().into(), script_public_key: item.script().to_rpc_hex() }
});

from!(item: &kaspa_rpc_core::RpcOptionalTransactionVerboseData, protowire::RpcTransactionVerboseData, {
    Self {
        transaction_id: item.transaction_id.map(|v| v.to_string()).unwrap_or_default(),
        hash: item.hash.map(|v| v.to_string()).unwrap_or_default(),
        compute_mass: item.compute_mass.unwrap_or_default(),
        block_hash: item.block_hash.map(|v| v.to_string()).unwrap_or_default(),
        block_time: item.block_time.unwrap_or_default(),
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionVerboseData, protowire::RpcTransactionVerboseData, {
    Self {
        transaction_id: item.transaction_id.to_string(),
        hash: item.hash.to_string(),
        compute_mass: item.compute_mass,
        block_hash: item.block_hash.to_string(),
        block_time: item.block_time,
    }
});

from!(item: &kaspa_rpc_core::RpcOptionalTransactionInputVerboseData, protowire::RpcTransactionInputVerboseData, {
    Self {
        utxo_entry: item.utxo_entry.as_ref().map(|x| x.into()),
    }
});

from!(_item: &kaspa_rpc_core::RpcTransactionInputVerboseData, protowire::RpcTransactionInputVerboseData, {
    Self {
        utxo_entry: None,
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionOutputVerboseData, protowire::RpcTransactionOutputVerboseData, {
    Self {
        script_public_key_type: item.script_public_key_type.to_string(),
        script_public_key_address: (&item.script_public_key_address).into(),
    }
});

from!(item: &kaspa_rpc_core::RpcOptionalTransactionOutputVerboseData, protowire::RpcTransactionOutputVerboseData, {
Self {
    script_public_key_type: item.script_public_key_type.as_ref().map(|x| x.to_string()).unwrap_or_default(),
    script_public_key_address: item.script_public_key_address.as_ref().map(|x| x.to_string()).unwrap_or_default(),
    }
});

from!(item: &kaspa_rpc_core::RpcAcceptedTransactionIds, protowire::RpcAcceptedTransactionIds, {
    Self {
        accepting_block_hash: item.accepting_block_hash.to_string(),
        accepted_transaction_ids: item.accepted_transaction_ids.iter().map(|x| x.to_string()).collect(),
    }
});

from!(item: &kaspa_rpc_core::RpcUtxosByAddressesEntry, protowire::RpcUtxosByAddressesEntry, {
    Self {
        address: item.address.as_ref().map_or("".to_string(), |x| x.into()),
        outpoint: Some((&item.outpoint).into()),
        utxo_entry: Some((&item.utxo_entry).into()),
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcTransaction, kaspa_rpc_core::RpcTransaction, {
    Self {
        version: item.version.try_into()?,
        inputs: item
            .inputs
            .iter()
            .map(kaspa_rpc_core::RpcTransactionInput::try_from)
            .collect::<RpcResult<Vec<kaspa_rpc_core::RpcTransactionInput>>>()?,
        outputs: item
            .outputs
            .iter()
            .map(kaspa_rpc_core::RpcTransactionOutput::try_from)
            .collect::<RpcResult<Vec<kaspa_rpc_core::RpcTransactionOutput>>>()?,
        lock_time: item.lock_time,
        subnetwork_id: kaspa_rpc_core::RpcSubnetworkId::from_str(&item.subnetwork_id)?,
        gas: item.gas,
        payload: Vec::from_rpc_hex(&item.payload)?,
        mass: item.mass,
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcTransactionVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcTransaction, kaspa_rpc_core::RpcOptionalTransaction, {
    Self {
        version: Some(item.version.try_into()?),
        inputs: item
            .inputs
            .iter()
            .map(kaspa_rpc_core::RpcOptionalTransactionInput::try_from)
            .collect::<RpcResult<Vec<kaspa_rpc_core::RpcOptionalTransactionInput>>>()?,
        outputs: item
            .outputs
            .iter()
            .map(kaspa_rpc_core::RpcOptionalTransactionOutput::try_from)
            .collect::<RpcResult<Vec<kaspa_rpc_core::RpcOptionalTransactionOutput>>>()?,
        lock_time: Some(item.lock_time),
        subnetwork_id: Some(kaspa_rpc_core::RpcSubnetworkId::from_str(&item
            .subnetwork_id)?),
        gas: Some(item.gas),
        payload: Some(Vec::from_rpc_hex(&item.payload)?),
        mass: Some(item.mass),
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcOptionalTransactionVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcTransactionInput, kaspa_rpc_core::RpcOptionalTransactionInput, {
    Self {
        previous_outpoint: item
            .previous_outpoint
            .as_ref()
            .map(kaspa_rpc_core::RpcOptionalTransactionOutpoint::try_from)
            .transpose()?,
        signature_script: Some(Vec::from_rpc_hex(&item
            .signature_script)?),
        sequence: Some(item.sequence),
        sig_op_count: Some(item.sig_op_count.try_into()?),
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcOptionalTransactionInputVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcTransactionInput, kaspa_rpc_core::RpcTransactionInput, {
    Self {
        previous_outpoint: item
            .previous_outpoint
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcTransactionInput".to_string(), "previous_outpoint".to_string()))?
            .try_into()?,
        signature_script: Vec::from_rpc_hex(&item.signature_script)?,
        sequence: item.sequence,
        sig_op_count: item.sig_op_count.try_into()?,
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcTransactionInputVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcTransactionOutput, kaspa_rpc_core::RpcTransactionOutput, {
    Self {
        value: item.amount,
        script_public_key: item
            .script_public_key
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcTransactionOutput".to_string(), "script_public_key".to_string()))?
            .try_into()?,
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcTransactionOutputVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcTransactionOutput, kaspa_rpc_core::RpcOptionalTransactionOutput, {
    Self {
        value: Some(item.amount),
        script_public_key: item
            .script_public_key
            .as_ref()
            .map(kaspa_rpc_core::RpcScriptPublicKey::try_from)
            .transpose()?,
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcOptionalTransactionOutputVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcOutpoint, kaspa_rpc_core::RpcOptionalTransactionOutpoint, {
    Self {
        transaction_id: Some(RpcHash::from_str(&item.transaction_id)?),
        index: Some(item.index),
        }
});

try_from!(item: &protowire::RpcOutpoint, kaspa_rpc_core::RpcTransactionOutpoint, {
    Self { transaction_id: RpcHash::from_str(&item.transaction_id)?, index: item.index }
});

try_from!(item: &protowire::RpcUtxoEntry, kaspa_rpc_core::RpcUtxoEntry, {
    Self {
        amount: item.amount,
        script_public_key: item
            .script_public_key
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcTransactionOutput".to_string(), "script_public_key".to_string()))?
            .try_into()?,
        block_daa_score: item.block_daa_score,
        is_coinbase: item.is_coinbase,
    }
});

try_from!(item: &protowire::RpcUtxoEntry, kaspa_rpc_core::RpcOptionalUtxoEntry, {
    Self {
        amount: Some(item.amount),
        script_public_key: item
            .script_public_key
            .as_ref()
            .map(|x| x.try_into())
            .transpose()?,
        block_daa_score: Some(item.block_daa_score),
        is_coinbase: Some(item.is_coinbase),
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcOptionalUtxoEntryVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcUtxoEntryVerboseData, kaspa_rpc_core::RpcOptionalUtxoEntryVerboseData, {
    Self {
        script_public_key_type: Some(RpcScriptClass::from_str(&item.script_public_key_type)?),
        script_public_key_address: Some(RpcAddress::try_from(item.script_public_key_address.as_ref())?),
    }
});

try_from!(item: &protowire::RpcScriptPublicKey, kaspa_rpc_core::RpcScriptPublicKey, {
    Self::new(u16::try_from(item.version)?, RpcScriptVec::from_rpc_hex(item.script_public_key.as_str())?)
});

try_from!(item: &protowire::RpcTransactionVerboseData, kaspa_rpc_core::RpcTransactionVerboseData, {
    Self {
        transaction_id: RpcHash::from_str(&item.transaction_id)?,
        hash: RpcHash::from_str(&item.hash)?,
        compute_mass: item.compute_mass,
        block_hash: RpcHash::from_str(&item.block_hash)?,
        block_time: item.block_time,
    }
});

try_from!(item: &protowire::RpcTransactionVerboseData, kaspa_rpc_core::RpcOptionalTransactionVerboseData, {
    Self {
        transaction_id: Some(RpcHash::from_str(item.transaction_id.as_ref())?),
        hash: Some(RpcHash::from_str(item.hash.as_ref())?),
        compute_mass: Some(item.compute_mass),
        block_hash: if item.block_hash.is_empty() {
            None
        } else {
            Some(RpcHash::from_str(item.block_hash.as_ref())?)
        },
        block_time: Some(item.block_time),
    }
});

try_from!(&protowire::RpcTransactionInputVerboseData, kaspa_rpc_core::RpcTransactionInputVerboseData);

try_from!(item: &protowire::RpcTransactionInputVerboseData, kaspa_rpc_core::RpcOptionalTransactionInputVerboseData, {
    Self {
        utxo_entry: item.utxo_entry.as_ref().map(kaspa_rpc_core::RpcOptionalUtxoEntry::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcTransactionOutputVerboseData, kaspa_rpc_core::RpcOptionalTransactionOutputVerboseData, {
    Self {
        script_public_key_type: Some(RpcScriptClass::from_str(item.script_public_key_type.as_ref())?),
        script_public_key_address: Some(RpcAddress::try_from(item.script_public_key_address.as_ref())?),
    }
});

try_from!(item: &protowire::RpcTransactionOutputVerboseData, kaspa_rpc_core::RpcTransactionOutputVerboseData, {
    Self {
        script_public_key_type: item.script_public_key_type.as_str().try_into()?,
        script_public_key_address: item.script_public_key_address.as_str().try_into()?,
    }
});

try_from!(item: &protowire::RpcAcceptedTransactionIds, kaspa_rpc_core::RpcAcceptedTransactionIds, {
    Self {
        accepting_block_hash: RpcHash::from_str(&item.accepting_block_hash)?,
        accepted_transaction_ids: item.accepted_transaction_ids.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
    }
});

try_from!(item: &protowire::RpcChainBlockAcceptedTransactions, kaspa_rpc_core::RpcChainBlockAcceptedTransactions, {
    Self {
        chain_block_header: item
            .chain_block_header
            .as_ref()
            .map(kaspa_rpc_core::RpcOptionalHeader::try_from)
            .transpose()?
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcChainBlockAcceptedTransactions".to_string(), "chain_block_header".to_string()))?,
        accepted_transactions: item.accepted_transactions.iter().map(kaspa_rpc_core::RpcOptionalTransaction::try_from).collect::<Result<_, _>>()?,
    }
});

try_from!(item: &protowire::RpcUtxosByAddressesEntry, kaspa_rpc_core::RpcUtxosByAddressesEntry, {
    let address = if item.address.is_empty() { None } else { Some(item.address.as_str().try_into()?) };
    Self {
        address,
        outpoint: item
            .outpoint
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("UtxosByAddressesEntry".to_string(), "outpoint".to_string()))?
            .try_into()?,
        utxo_entry: item
            .utxo_entry
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("UtxosByAddressesEntry".to_string(), "utxo_entry".to_string()))?
            .try_into()?,
    }
});
