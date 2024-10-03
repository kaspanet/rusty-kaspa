use crate::protowire;
use crate::{from, try_from};
use kaspa_rpc_core::{FromRpcHex, RpcError, RpcHash, RpcResult, RpcScriptVec, ToRpcHex};
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

from!(item: &kaspa_rpc_core::RpcTransactionInput, protowire::RpcTransactionInput, {
    Self {
        previous_outpoint: Some((&item.previous_outpoint).into()),
        signature_script: item.signature_script.to_rpc_hex(),
        sequence: item.sequence,
        sig_op_count: item.sig_op_count.into(),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionOutput, protowire::RpcTransactionOutput, {
    Self {
        amount: item.value,
        script_public_key: Some((&item.script_public_key).into()),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
    }
});

from!(item: &kaspa_rpc_core::RpcTransactionOutpoint, protowire::RpcOutpoint, {
    Self { transaction_id: item.transaction_id.to_string(), index: item.index }
});

from!(item: &kaspa_rpc_core::RpcUtxoEntry, protowire::RpcUtxoEntry, {
    Self {
        amount: item.amount,
        script_public_key: Some((&item.script_public_key).into()),
        block_daa_score: item.block_daa_score,
        is_coinbase: item.is_coinbase,
    }
});

from!(item: &kaspa_rpc_core::RpcScriptPublicKey, protowire::RpcScriptPublicKey, {
    Self { version: item.version().into(), script_public_key: item.script().to_rpc_hex() }
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

from!(&kaspa_rpc_core::RpcTransactionInputVerboseData, protowire::RpcTransactionInputVerboseData);

from!(item: &kaspa_rpc_core::RpcTransactionOutputVerboseData, protowire::RpcTransactionOutputVerboseData, {
    Self {
        script_public_key_type: item.script_public_key_type.to_string(),
        script_public_key_address: (&item.script_public_key_address).into(),
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

try_from!(&protowire::RpcTransactionInputVerboseData, kaspa_rpc_core::RpcTransactionInputVerboseData);

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
