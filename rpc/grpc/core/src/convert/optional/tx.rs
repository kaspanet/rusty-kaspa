use crate::protowire;
use crate::{from, try_from};
use kaspa_rpc_core::{FromRpcHex, RpcAddress, RpcError, RpcResult, RpcScriptClass, RpcSubnetworkId, ToRpcHex};
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcOptionalTransaction, protowire::RpcOptionalTransaction, {
    Self {
        version: item.version.map(|x| x.into()),
        inputs: item.inputs.iter().map(protowire::RpcOptionalTransactionInput::from).collect(),
        outputs: item.outputs.iter().map(protowire::RpcOptionalTransactionOutput::from).collect(),
        lock_time: item.lock_time,
        subnetwork_id: item.subnetwork_id.as_ref().map(|x| x.to_string()),
        gas: item.gas,
        payload: item.payload.as_ref().map(|x| x.to_rpc_hex()),
        mass: item.mass,
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcOptionalTransactionInput, protowire::RpcOptionalTransactionInput, {
    Self {
        previous_outpoint: item.previous_outpoint.as_ref().map(|x| x.into()),
        signature_script: item.signature_script.as_ref().map(|x| x.to_rpc_hex()),
        sequence: item.sequence,
        sig_op_count: item.sig_op_count.map(|x| x.into()),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcOptionalTransactionOutput, protowire::RpcOptionalTransactionOutput, {
    Self {
        value: item.value,
        script_public_key: item.script_public_key.as_ref().map(|x| x.into()),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcOptionalTransactionOutpoint, protowire::RpcOptionalTransactionOutpoint, {
    Self {
        transaction_id: item.transaction_id.map(|x| x.to_string()),
        index: item.index,
    }
});

from!(item: &kaspa_rpc_core::RpcOptionalTransactionVerboseData, protowire::RpcOptionalTransactionVerboseData, {
    Self {
        transaction_id: item.transaction_id.map(|v| v.to_string()),
        hash: item.hash.map(|v| v.to_string()),
        compute_mass: item.compute_mass,
        block_hash: item.block_hash.map(|v| v.to_string()),
        block_time: item.block_time,
    }
});

from!(item: &kaspa_rpc_core::RpcOptionalTransactionInputVerboseData, protowire::RpcOptionalTransactionInputVerboseData, {
    Self {
        utxo_entry: item.utxo_entry.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcOptionalTransactionOutputVerboseData, protowire::RpcOptionalTransactionOutputVerboseData, {
    Self {
        script_public_key_type: item.script_public_key_type.as_ref().map(|x| x.to_string()),
        script_public_key_address: item.script_public_key_address.as_ref().map(|x| x.to_string()),
    }
});

from!(item: &kaspa_rpc_core::RpcOptionalUtxoEntry, protowire::RpcOptionalUtxoEntry, {
    Self {
        amount: item.amount,
        script_public_key: item.script_public_key.as_ref().map(|x| x.into()),
        block_daa_score: item.block_daa_score,
        is_coinbase: item.is_coinbase,
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcOptionalUtxoEntryVerboseData, protowire::RpcOptionalUtxoEntryVerboseData, {
    Self {
        script_public_key_type: item.script_public_key_type.as_ref().map(|x| x.to_string()),
        script_public_key_address: item.script_public_key_address.as_ref().map(|x| x.to_string()),
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcOptionalTransaction, kaspa_rpc_core::RpcOptionalTransaction, {
    Self {
        version: item.version.map(|x| x as u16),
        inputs: item.inputs.iter().map(kaspa_rpc_core::RpcOptionalTransactionInput::try_from).collect::<RpcResult<_>>()?,
        outputs: item.outputs.iter().map(kaspa_rpc_core::RpcOptionalTransactionOutput::try_from).collect::<RpcResult<_>>()?,
        lock_time: item.lock_time,
        subnetwork_id: item.subnetwork_id.as_ref().map(|x| RpcSubnetworkId::from_str(x)).transpose()?,
        gas: item.gas,
        payload: item.payload.as_ref().map(|x| Vec::from_rpc_hex(x)).transpose()?,
        mass: item.mass,
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcOptionalTransactionVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcOptionalTransactionInput, kaspa_rpc_core::RpcOptionalTransactionInput, {
    Self {
        previous_outpoint: item.previous_outpoint.as_ref().map(kaspa_rpc_core::RpcOptionalTransactionOutpoint::try_from).transpose()?,
        signature_script: item.signature_script.as_ref().map(|x| Vec::from_rpc_hex(x)).transpose()?,
        sequence: item.sequence,
        sig_op_count: item.sig_op_count.map(|x| x as u8),
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcOptionalTransactionInputVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcOptionalTransactionOutput, kaspa_rpc_core::RpcOptionalTransactionOutput, {
    Self {
        value: item.value,
        script_public_key: item.script_public_key.as_ref().map(kaspa_rpc_core::RpcScriptPublicKey::try_from).transpose()?,
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcOptionalTransactionOutputVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcOptionalTransactionOutpoint, kaspa_rpc_core::RpcOptionalTransactionOutpoint, {
    Self {
        transaction_id: item.transaction_id.as_ref().map(|x| kaspa_rpc_core::RpcHash::from_str(x)).transpose()?,
        index: item.index,
    }
});

try_from!(item: &protowire::RpcOptionalTransactionVerboseData, kaspa_rpc_core::RpcOptionalTransactionVerboseData, {
    Self {
        transaction_id: item.transaction_id.as_ref().map(|x| kaspa_rpc_core::RpcHash::from_str(x)).transpose()?,
        hash: item.hash.as_ref().map(|x| kaspa_rpc_core::RpcHash::from_str(x)).transpose()?,
        compute_mass: item.compute_mass,
        block_hash: item.block_hash.as_ref().map(|x| kaspa_rpc_core::RpcHash::from_str(x)).transpose()?,
        block_time: item.block_time,
    }
});

try_from!(item: &protowire::RpcOptionalTransactionInputVerboseData, kaspa_rpc_core::RpcOptionalTransactionInputVerboseData, {
    Self {
        utxo_entry: item.utxo_entry.as_ref().map(kaspa_rpc_core::RpcOptionalUtxoEntry::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcOptionalTransactionOutputVerboseData, kaspa_rpc_core::RpcOptionalTransactionOutputVerboseData, {
    Self {
        script_public_key_type: item.script_public_key_type.as_ref().map(|x| RpcScriptClass::from_str(x)).transpose()?,
        script_public_key_address: item.script_public_key_address.as_ref().map(|x| RpcAddress::try_from(x.as_str())).transpose()?,
    }
});

try_from!(item: &protowire::RpcOptionalUtxoEntry, kaspa_rpc_core::RpcOptionalUtxoEntry, {
    Self {
        amount: item.amount,
        script_public_key: item.script_public_key.as_ref().map(|x| x.try_into()).transpose()?,
        block_daa_score: item.block_daa_score,
        is_coinbase: item.is_coinbase,
        verbose_data: item.verbose_data.as_ref().map(|x| x.try_into()).transpose()?,
    }
});

try_from!(item: &protowire::RpcOptionalUtxoEntryVerboseData, kaspa_rpc_core::RpcOptionalUtxoEntryVerboseData, {
    Self {
        script_public_key_type: item.script_public_key_type.as_ref().map(|x| RpcScriptClass::from_str(x)).transpose()?,
        script_public_key_address: item.script_public_key_address.as_ref().map(|x| RpcAddress::try_from(x.as_str())).transpose()?,
    }
});
