//! Conversion of Transaction related types

use crate::{
    RpcError, RpcOptionalTransaction, RpcOptionalTransactionInput, RpcOptionalTransactionOutput, RpcResult, RpcTransaction,
    RpcTransactionInput, RpcTransactionOutput,
};
use kaspa_consensus_core::mass::{ComputeBudget, SigopCount};
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutput, TxInputMass};

struct RpcInputWithVersion {
    version: u16,
    input: RpcTransactionInput,
}

fn invalid_input_mass_variant(field: &str, version: u16) -> RpcError {
    RpcError::General(format!("RpcTransactionInput.{field} is inconsistent with transaction version {version}"))
}

impl TryFrom<RpcInputWithVersion> for TransactionInput {
    type Error = RpcError;

    fn try_from(value: RpcInputWithVersion) -> RpcResult<Self> {
        let mass = if TxInputMass::version_expects_compute_budget_field(value.version) {
            if value.input.sig_op_count != 0 {
                return Err(invalid_input_mass_variant("sig_op_count", value.version));
            }
            ComputeBudget(value.input.compute_budget).into()
        } else {
            if value.input.compute_budget != 0 {
                return Err(invalid_input_mass_variant("compute_budget", value.version));
            }
            SigopCount(value.input.sig_op_count).into()
        };

        Ok(TransactionInput::new_with_mass(
            value.input.previous_outpoint.into(),
            value.input.signature_script,
            value.input.sequence,
            mass,
        ))
    }
}

struct RpcOptionalInputWithVersion {
    version: u16,
    input: RpcOptionalTransactionInput,
}

impl TryFrom<RpcOptionalInputWithVersion> for TransactionInput {
    type Error = RpcError;

    fn try_from(value: RpcOptionalInputWithVersion) -> RpcResult<Self> {
        let previous_outpoint = value
            .input
            .previous_outpoint
            .ok_or(RpcError::MissingRpcFieldError("RpcTransactionInput".to_owned(), "previous_outpoint".to_owned()))?
            .try_into()?;
        let signature_script = value
            .input
            .signature_script
            .ok_or(RpcError::MissingRpcFieldError("RpcTransactionInput".to_owned(), "signature_script".to_owned()))?;
        let sequence =
            value.input.sequence.ok_or(RpcError::MissingRpcFieldError("RpcTransactionInput".to_owned(), "sequence".to_owned()))?;

        let mass = if TxInputMass::version_expects_compute_budget_field(value.version) {
            if value.input.sig_op_count.is_some_and(|v| v != 0) {
                return Err(invalid_input_mass_variant("sig_op_count", value.version));
            }
            ComputeBudget(value.input.compute_budget.unwrap_or_default()).into()
        } else {
            if value.input.compute_budget.is_some_and(|v| v != 0) {
                return Err(invalid_input_mass_variant("compute_budget", value.version));
            }
            SigopCount(value.input.sig_op_count.unwrap_or_default()).into()
        };

        Ok(TransactionInput::new_with_mass(previous_outpoint, signature_script, sequence, mass))
    }
}

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<&Transaction> for RpcTransaction {
    fn from(item: &Transaction) -> Self {
        Self {
            version: item.version,
            inputs: item.inputs.iter().map(RpcTransactionInput::from).collect(),
            outputs: item.outputs.iter().map(RpcTransactionOutput::from).collect(),
            lock_time: item.lock_time,
            subnetwork_id: item.subnetwork_id,
            gas: item.gas,
            payload: item.payload.clone(),
            mass: item.mass(),
            verbose_data: None,
        }
    }
}

impl From<&TransactionOutput> for RpcTransactionOutput {
    fn from(item: &TransactionOutput) -> Self {
        Self {
            value: item.value,
            script_public_key: item.script_public_key.clone(),
            verbose_data: None,
            covenant: item.covenant.map(Into::into),
        }
    }
}

impl From<&TransactionInput> for RpcTransactionInput {
    fn from(item: &TransactionInput) -> Self {
        Self {
            previous_outpoint: item.previous_outpoint.into(),
            signature_script: item.signature_script.clone(),
            sequence: item.sequence,
            sig_op_count: item.mass.sig_op_count().unwrap_or(0),
            compute_budget: item.mass.compute_budget().unwrap_or(0),
            verbose_data: None,
        }
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<RpcTransaction> for Transaction {
    type Error = RpcError;
    fn try_from(item: RpcTransaction) -> RpcResult<Self> {
        let version = item.version;
        let transaction = Transaction::new(
            version,
            item.inputs
                .into_iter()
                .map(|input| RpcInputWithVersion { version, input }.try_into())
                .collect::<RpcResult<Vec<kaspa_consensus_core::tx::TransactionInput>>>()?,
            item.outputs
                .into_iter()
                .map(kaspa_consensus_core::tx::TransactionOutput::try_from)
                .collect::<RpcResult<Vec<kaspa_consensus_core::tx::TransactionOutput>>>()?,
            item.lock_time,
            item.subnetwork_id,
            item.gas,
            item.payload.clone(),
        );
        transaction.set_mass(item.mass);
        Ok(transaction)
    }
}

impl TryFrom<RpcTransactionOutput> for TransactionOutput {
    type Error = RpcError;
    fn try_from(item: RpcTransactionOutput) -> RpcResult<Self> {
        Ok(Self::with_covenant(item.value, item.script_public_key, item.covenant.map(Into::into)))
    }
}

// ----------------------------------------------------------------------------
// consensus_core to optional rpc_core
// ----------------------------------------------------------------------------

impl From<&Transaction> for RpcOptionalTransaction {
    fn from(item: &Transaction) -> Self {
        Self {
            version: Some(item.version),
            inputs: item.inputs.iter().map(RpcOptionalTransactionInput::from).collect(),
            outputs: item.outputs.iter().map(RpcOptionalTransactionOutput::from).collect(),
            lock_time: Some(item.lock_time),
            subnetwork_id: Some(item.subnetwork_id),
            gas: Some(item.gas),
            payload: Some(item.payload.clone()),
            mass: Some(item.mass()),
            verbose_data: None,
        }
    }
}

impl From<&TransactionOutput> for RpcOptionalTransactionOutput {
    fn from(item: &TransactionOutput) -> Self {
        Self {
            value: Some(item.value),
            script_public_key: Some(item.script_public_key.clone()),
            verbose_data: None,
            covenant: item.covenant.map(Into::into),
        }
    }
}

impl From<&TransactionInput> for RpcOptionalTransactionInput {
    fn from(item: &TransactionInput) -> Self {
        Self {
            previous_outpoint: Some(item.previous_outpoint.into()),
            signature_script: Some(item.signature_script.clone()),
            sequence: Some(item.sequence),
            sig_op_count: Some(item.mass.sig_op_count().unwrap_or(0)),
            compute_budget: Some(item.mass.compute_budget().unwrap_or(0)),
            verbose_data: None,
        }
    }
}

// ----------------------------------------------------------------------------
// optional rpc_core to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<RpcOptionalTransaction> for Transaction {
    type Error = RpcError;
    fn try_from(item: RpcOptionalTransaction) -> RpcResult<Self> {
        let version = item.version.ok_or(RpcError::MissingRpcFieldError("RpcTransaction".to_owned(), "version".to_owned()))?;
        let transaction = Transaction::new(
            version,
            item.inputs
                .into_iter()
                .map(|input| RpcOptionalInputWithVersion { version, input }.try_into())
                .collect::<RpcResult<Vec<kaspa_consensus_core::tx::TransactionInput>>>()?,
            item.outputs
                .into_iter()
                .map(kaspa_consensus_core::tx::TransactionOutput::try_from)
                .collect::<RpcResult<Vec<kaspa_consensus_core::tx::TransactionOutput>>>()?,
            item.lock_time.ok_or(RpcError::MissingRpcFieldError("RpcTransaction".to_owned(), "lock_time".to_owned()))?,
            item.subnetwork_id.ok_or(RpcError::MissingRpcFieldError("RpcTransaction".to_owned(), "subnetwork_id".to_owned()))?,
            item.gas.ok_or(RpcError::MissingRpcFieldError("RpcTransaction".to_owned(), "gas".to_owned()))?,
            item.payload.ok_or(RpcError::MissingRpcFieldError("RpcTransaction".to_owned(), "payload".to_owned()))?,
        );
        transaction.set_mass(item.mass.ok_or(RpcError::MissingRpcFieldError("RpcTransaction".to_owned(), "mass".to_owned()))?);
        Ok(transaction)
    }
}

impl TryFrom<RpcOptionalTransactionOutput> for TransactionOutput {
    type Error = RpcError;
    fn try_from(item: RpcOptionalTransactionOutput) -> RpcResult<Self> {
        Ok(Self::new(
            item.value.ok_or(RpcError::MissingRpcFieldError("RpcTransactionOutput".to_owned(), "value".to_owned()))?,
            item.script_public_key
                .ok_or(RpcError::MissingRpcFieldError("RpcTransactionOutput".to_owned(), "script_public_key".to_owned()))?,
        ))
    }
}
