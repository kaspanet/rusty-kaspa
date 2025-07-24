use crate::protowire::{self, RpcTransactionVerboseDataVerbosity};
use crate::{from, try_from};
use kaspa_rpc_core::{FromRpcHex, RpcAddress, RpcError, RpcHash, RpcResult, RpcScriptClass, RpcScriptVec, ToRpcHex};
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcTransaction, protowire::RpcTransaction, {
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

from!(item: &kaspa_rpc_core::RpcTransactionVerbosity, protowire::RpcTransactionVerbosity, {
    Self {
        include_version: item.include_version,
        input_verbosity: item.input_verbosity.as_ref().map(protowire::RpcTransactionInputVerbosity::from),
        output_verbosity: item.output_verbosity.as_ref().map(protowire::RpcTransactionOutputVerbosity::from),
        include_lock_time: item.include_lock_time,
        include_subnetwork_id: item.include_subnetwork_id,
        include_gas: item.include_gas,
        include_payload: item.include_payload,
        include_mass: item.include_mass,
        verbose_data_verbosity: item.verbose_data_verbosity.as_ref().map(RpcTransactionVerboseDataVerbosity::from),
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionInput, protowire::RpcTransactionInput, {
    Self {
        previous_outpoint: item.previous_outpoint.as_ref().map(protowire::RpcOutpoint::from),
        signature_script: item.signature_script.as_ref().map(|x| x.to_rpc_hex()).unwrap_or_default(),
        sequence: item.sequence.unwrap_or_default(),
        sig_op_count: item.sig_op_count.map(|x| x.into()).unwrap_or_default(),
        verbose_data: item.verbose_data.as_ref().map(protowire::RpcTransactionInputVerboseData::from),
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionInputVerbosity, protowire::RpcTransactionInputVerbosity, {
    Self {
        include_previous_outpoint: item.include_previous_outpoint,
        include_signature_script: item.include_signature_script,
        include_sequence: item.include_sequence,
        include_sig_op_count: item.include_sig_op_count,
        verbose_data_verbosity: item.verbose_data_verbosity.as_ref().map(protowire::RpcTransactionInputVerboseDataVerbosity::from),
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionOutput, protowire::RpcTransactionOutput, {
    Self {
        amount: item.value.unwrap_or_default(),
        script_public_key: item.script_public_key.as_ref().map(|x| x.into()),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionOutputVerbosity, protowire::RpcTransactionOutputVerbosity, {
    Self {
        include_amount: item.include_amount,
        include_script_public_key: item.include_script_public_key,
        verbose_data_verbosity: item.verbose_data_verbosity.as_ref().map(protowire::RpcTransactionOutputVerboseDataVerbosity::from),
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionOutpoint, protowire::RpcOutpoint, {
    Self { transaction_id: item.transaction_id.as_ref().map(|x| x.to_string()).unwrap_or_default(), index: item.index.unwrap_or_default() }
});

from!(item: &kaspa_rpc_core::RpcUtxoEntry, protowire::RpcUtxoEntry, {
    Self {
        amount: item.amount.unwrap_or_default(),
        script_public_key: item.script_public_key.as_ref().map(|x| x.into()),
        block_daa_score: item.block_daa_score.unwrap_or_default(),
        is_coinbase: item.is_coinbase.unwrap_or_default(),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcUtxoEntryVerboseData, protowire::RpcUtxoEntryVerboseData, {
    Self {
        script_public_key_type: item.script_public_key_type.as_ref().map(|x| x.to_string()).unwrap_or_default(),
        script_public_key_address: item.script_public_key_address.as_ref().map(|x| x.to_string()).unwrap_or_default(),
        }
});

from!(item: &kaspa_rpc_core::RpcUtxoEntryVerbosity, protowire::RpcUtxoEntryVerbosity, {
    Self {
        include_amount: item.include_amount,
        include_script_public_key: item.include_script_public_key,
        include_block_daa_score: item.include_block_daa_score,
        include_is_coinbase: item.include_is_coinbase,
        verbose_data_verbosity: item.verbose_data_verbosity.as_ref().map(protowire::RpcUtxoEntryVerboseDataVerbosity::from),
    }
});

from!(item: &kaspa_rpc_core::RpcUtxoEntryVerboseDataVerbosity, protowire::RpcUtxoEntryVerboseDataVerbosity, {
    Self {
        include_script_public_key_type: item.include_script_public_key_type,
        include_script_public_key_address: item.include_script_public_key_address,
    }
});

from!(item: &kaspa_rpc_core::RpcScriptPublicKey, protowire::RpcScriptPublicKey, {
    Self { version: item.version().into(), script_public_key: item.script().to_rpc_hex() }
});

from!(item: &kaspa_rpc_core::RpcTransactionVerboseData, protowire::RpcTransactionVerboseData, {
    Self {
        transaction_id: item.transaction_id.map(|v| v.to_string()).unwrap_or_default(),
        hash: item.hash.map(|v| v.to_string()).unwrap_or_default(),
        compute_mass: item.compute_mass.unwrap_or_default(),
        block_hash: item.block_hash.map(|v| v.to_string()).unwrap_or_default(),
        block_time: item.block_time.unwrap_or_default(),
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionVerboseDataVerbosity, protowire::RpcTransactionVerboseDataVerbosity, {
    Self {
        include_transaction_id: item.include_transaction_id,
        include_hash: item.include_hash,
        include_compute_mass: item.include_compute_mass,
        include_block_hash: item.include_block_hash,
        include_block_time: item.include_block_time,
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionInputVerboseData, protowire::RpcTransactionInputVerboseData, {
    Self {
        utxo_entry: item.utxo_entry.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionInputVerboseDataVerbosity, protowire::RpcTransactionInputVerboseDataVerbosity, {
    Self {
        utxo_entry_verbosity: item.utxo_entry_verbosity.as_ref().map(protowire::RpcUtxoEntryVerbosity::from),
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionOutputVerboseData, protowire::RpcTransactionOutputVerboseData, {
Self {
    script_public_key_type: item.script_public_key_type.as_ref().map(|x| x.to_string()).unwrap_or_default(),
    script_public_key_address: item.script_public_key_address.as_ref().map(|x| x.to_string()).unwrap_or_default(),
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionOutputVerboseDataVerbosity, protowire::RpcTransactionOutputVerboseDataVerbosity, {
    Self {
        include_script_public_key_type: item.include_script_public_key_type,
        include_script_public_key_address: item.include_script_public_key_address,
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
        version: Some(item.version.try_into()?),
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
        lock_time: Some(item.lock_time),
        subnetwork_id: Some(kaspa_rpc_core::RpcSubnetworkId::from_str(&item
            .subnetwork_id)?),
        gas: Some(item.gas),
        payload: Some(Vec::from_rpc_hex(&item.payload)?),
        mass: Some(item.mass),
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcTransactionVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcTransactionVerbosity, kaspa_rpc_core::RpcTransactionVerbosity, {
    Self {
        include_version: item.include_version,
        input_verbosity: item.input_verbosity.as_ref().map(kaspa_rpc_core::RpcTransactionInputVerbosity::try_from).transpose()?,
        output_verbosity: item.output_verbosity.as_ref().map(kaspa_rpc_core::RpcTransactionOutputVerbosity::try_from).transpose()?,
        include_lock_time: item.include_lock_time,
        include_subnetwork_id: item.include_subnetwork_id,
        include_gas: item.include_gas,
        include_payload: item.include_payload,
        include_mass: item.include_mass,
        verbose_data_verbosity: item.verbose_data_verbosity.as_ref().map(kaspa_rpc_core::RpcTransactionVerboseDataVerbosity::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcTransactionInput, kaspa_rpc_core::RpcTransactionInput, {
    Self {
        previous_outpoint: item
            .previous_outpoint
            .as_ref()
            .map(kaspa_rpc_core::RpcTransactionOutpoint::try_from)
            .transpose()?,
        signature_script: Some(Vec::from_rpc_hex(&item
            .signature_script)?),
        sequence: Some(item.sequence),
        sig_op_count: Some(item.sig_op_count.try_into()?),
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcTransactionInputVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcTransactionInputVerbosity, kaspa_rpc_core::RpcTransactionInputVerbosity, {
    Self {
        include_previous_outpoint: item.include_previous_outpoint,
        include_signature_script: item.include_signature_script,
        include_sequence: item.include_sequence,
        include_sig_op_count: item.include_sig_op_count,
        verbose_data_verbosity: item.verbose_data_verbosity.as_ref().map(kaspa_rpc_core::RpcTransactionInputVerboseDataVerbosity::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcTransactionOutput, kaspa_rpc_core::RpcTransactionOutput, {
    Self {
        value: Some(item.amount),
        script_public_key: item
            .script_public_key
            .as_ref()
            .map(kaspa_rpc_core::RpcScriptPublicKey::try_from)
            .transpose()?,
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcTransactionOutputVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcTransactionOutputVerbosity, kaspa_rpc_core::RpcTransactionOutputVerbosity, {
    Self {
        include_amount: item.include_amount,
        include_script_public_key: item.include_script_public_key,
        verbose_data_verbosity: item.verbose_data_verbosity.as_ref().map(kaspa_rpc_core::RpcTransactionOutputVerboseDataVerbosity::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcOutpoint, kaspa_rpc_core::RpcTransactionOutpoint, {
    Self {
        transaction_id: Some(RpcHash::from_str(&item.transaction_id)?),
        index: Some(item.index),
        }
});

try_from!(item: &protowire::RpcUtxoEntry, kaspa_rpc_core::RpcUtxoEntry, {
    Self {
        amount: Some(item.amount),
        script_public_key: item
            .script_public_key
            .as_ref()
            .map(|x| x.try_into())
            .transpose()?,
        block_daa_score: Some(item.block_daa_score),
        is_coinbase: Some(item.is_coinbase),
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcUtxoEntryVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcUtxoEntryVerboseData, kaspa_rpc_core::RpcUtxoEntryVerboseData, {
    Self {
        script_public_key_type: Some(RpcScriptClass::from_str(&item.script_public_key_type)?),
        script_public_key_address: Some(RpcAddress::try_from(item.script_public_key_address.as_ref())?),
    }
});

try_from!(item: &protowire::RpcUtxoEntryVerbosity, kaspa_rpc_core::RpcUtxoEntryVerbosity, {
    Self {
        include_amount: item.include_amount,
        include_script_public_key: item.include_script_public_key,
        include_block_daa_score: item.include_block_daa_score,
        include_is_coinbase: item.include_is_coinbase,
        verbose_data_verbosity: item.verbose_data_verbosity.as_ref().map(kaspa_rpc_core::RpcUtxoEntryVerboseDataVerbosity::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcUtxoEntryVerboseDataVerbosity, kaspa_rpc_core::RpcUtxoEntryVerboseDataVerbosity, {
    Self {
        include_script_public_key_type: item.include_script_public_key_type,
        include_script_public_key_address: item.include_script_public_key_address,
    }
});

try_from!(item: &protowire::RpcScriptPublicKey, kaspa_rpc_core::RpcScriptPublicKey, {
    Self::new(u16::try_from(item.version)?, RpcScriptVec::from_rpc_hex(item.script_public_key.as_str())?)
});

try_from!(item: &protowire::RpcTransactionVerboseData, kaspa_rpc_core::RpcTransactionVerboseData, {
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

try_from!(item: &protowire::RpcTransactionVerboseDataVerbosity, kaspa_rpc_core::RpcTransactionVerboseDataVerbosity, {
    Self {
        include_transaction_id: item.include_transaction_id,
        include_hash: item.include_hash,
        include_compute_mass: item.include_compute_mass,
        include_block_hash: item.include_block_hash,
        include_block_time: item.include_block_time,
    }
});

try_from!(item: &protowire::RpcTransactionInputVerboseData, kaspa_rpc_core::RpcTransactionInputVerboseData, {
    Self {
        utxo_entry: item.utxo_entry.as_ref().map(kaspa_rpc_core::RpcUtxoEntry::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcTransactionInputVerboseDataVerbosity, kaspa_rpc_core::RpcTransactionInputVerboseDataVerbosity, {
    Self {
        utxo_entry_verbosity: item.utxo_entry_verbosity.as_ref().map(kaspa_rpc_core::RpcUtxoEntryVerbosity::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcTransactionOutputVerboseData, kaspa_rpc_core::RpcTransactionOutputVerboseData, {
    Self {
        script_public_key_type: Some(RpcScriptClass::from_str(item.script_public_key_type.as_ref())?),
        script_public_key_address: Some(RpcAddress::try_from(item.script_public_key_address.as_ref())?),
    }
});

try_from!(item: &protowire::RpcTransactionOutputVerboseDataVerbosity, kaspa_rpc_core::RpcTransactionOutputVerboseDataVerbosity, {
    Self {
        include_script_public_key_type: item.include_script_public_key_type,
        include_script_public_key_address: item.include_script_public_key_address,
    }
});

try_from!(item: &protowire::RpcAcceptedTransactionIds, kaspa_rpc_core::RpcAcceptedTransactionIds, {
    Self {
        accepting_block_hash: RpcHash::from_str(&item.accepting_block_hash)?,
        accepted_transaction_ids: item.accepted_transaction_ids.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
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
