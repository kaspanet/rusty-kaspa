//!
//! Conversion functions for converting between
//! the [`kaspa_consensus_client`], [`kaspa_consensus_core`]
//! and [`kaspa_wallet_pskt`](crate) types.
//!

use crate::error::Error;
use crate::input::{Input, InputBuilder};
use crate::output::{Output, OutputBuilder};
use crate::pskt::{Global, Inner};
use kaspa_consensus_client::{Transaction, TransactionInput, TransactionInputInner, TransactionOutput, TransactionOutputInner};
use kaspa_consensus_core::tx as cctx;

impl TryFrom<Transaction> for Inner {
    type Error = Error;
    fn try_from(transaction: Transaction) -> Result<Self, Self::Error> {
        Inner::try_from(cctx::Transaction::try_from(&transaction)?)
    }
}

impl TryFrom<TransactionInput> for Input {
    type Error = Error;
    fn try_from(input: TransactionInput) -> std::result::Result<Input, Self::Error> {
        let TransactionInputInner { previous_outpoint, signature_script: _, sequence: _, sig_op_count, compute_budget, utxo } =
            &*input.inner();
        if *compute_budget != 0 {
            return Err(Error::UnsupportedInputComputeBudget);
        }

        let input = InputBuilder::default()
        .utxo_entry(utxo.as_ref().ok_or(Error::MissingUtxoEntry)?.into())
        .previous_outpoint(previous_outpoint.into())
        // .sequence(*sequence)
        // min_time
        // partial_sigs
        // sighash_type
        // redeem_script
        .sig_op_count(*sig_op_count)
        // bip32_derivations
        // final_script_sig
        .build()?;

        Ok(input)
    }
}

impl TryFrom<TransactionOutput> for Output {
    type Error = Error;
    fn try_from(output: TransactionOutput) -> std::result::Result<Output, Self::Error> {
        // Self::Transaction(transaction)

        let TransactionOutputInner { value, script_public_key, covenant } = &*output.inner();

        let output = OutputBuilder::default()
            .amount(*value)
            .script_public_key(script_public_key.clone())
            .covenant(covenant.map(cctx::CovenantBinding::from))
        // .redeem_script
        // .bip32_derivations
        // .proprietaries
        // .unknowns
        .build()?;

        Ok(output)
    }
}

impl TryFrom<(cctx::Transaction, Vec<(&cctx::TransactionInput, &cctx::UtxoEntry)>)> for Inner {
    type Error = Error; // Define your error type

    fn try_from(
        (transaction, populated_inputs): (cctx::Transaction, Vec<(&cctx::TransactionInput, &cctx::UtxoEntry)>),
    ) -> Result<Self, Self::Error> {
        if cctx::TxInputMass::version_expects_compute_budget_field(transaction.version) {
            return Err(Error::UnsupportedInputComputeBudget);
        }
        let inputs: Result<Vec<Input>, Self::Error> = populated_inputs
            .into_iter()
            .map(|(input, utxo)| {
                InputBuilder::default()
                    .utxo_entry(utxo.to_owned().clone())
                    .previous_outpoint(input.previous_outpoint)
                    .sig_op_count(input.mass.sig_op_count().unwrap_or(0)) // TODO: Add support for v1 transactions with TxInputMass::ComputeBudget
                    .build()
                    .map_err(Error::TxToInnerConversionInputBuildingError)
                // Handle the error
            })
            .collect::<Result<_, _>>();

        let outputs: Result<Vec<Output>, Self::Error> = transaction
            .outputs
            .iter()
            .map(|output| {
                Output::try_from(TransactionOutput::from(output.to_owned())).map_err(|e| Error::TxToInnerConversionError(Box::new(e)))
            })
            .collect::<Result<_, _>>();

        Ok(Inner { global: Global::default(), inputs: inputs?, outputs: outputs? })
    }
}

impl TryFrom<cctx::Transaction> for Inner {
    type Error = Error;
    fn try_from(transaction: cctx::Transaction) -> Result<Self, self::Error> {
        if cctx::TxInputMass::version_expects_compute_budget_field(transaction.version) {
            return Err(Error::UnsupportedInputComputeBudget);
        }
        let inputs = transaction
            .inputs
            .iter()
            .map(|input| {
                Input::try_from(TransactionInput::from(input.to_owned())).map_err(|e| Error::TxToInnerConversionError(Box::new(e)))
            })
            .collect::<Result<_, _>>()?;

        let outputs = transaction
            .outputs
            .iter()
            .map(|output| {
                Output::try_from(TransactionOutput::from(output.to_owned())).map_err(|e| Error::TxToInnerConversionError(Box::new(e)))
            })
            .collect::<Result<_, _>>()?;

        Ok(Inner { global: Global::default(), inputs, outputs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_client::{TransactionInput, TransactionOutpoint};
    use kaspa_consensus_core::{
        mass::ComputeBudget,
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{
            ScriptPublicKey, Transaction as CoreTransaction, TransactionId, TransactionInput as CoreTransactionInput,
            TransactionOutput,
        },
    };

    #[test]
    fn transaction_input_with_compute_budget_is_rejected() {
        let input = TransactionInput::new(TransactionOutpoint::new(TransactionId::default(), 0), None, 0, 0, 1, None);
        let err = Input::try_from(input).unwrap_err();
        assert!(matches!(err, Error::UnsupportedInputComputeBudget));
    }

    #[test]
    fn v1_transaction_conversion_to_pskt_is_rejected() {
        let tx = CoreTransaction::new(
            1,
            vec![CoreTransactionInput {
                previous_outpoint: Default::default(),
                signature_script: vec![],
                sequence: 0,
                mass: cctx::TxInputMass::ComputeBudget(ComputeBudget(1)),
            }],
            vec![TransactionOutput::new(0, ScriptPublicKey::new(0, vec![].into()))],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let err = Inner::try_from(tx).unwrap_err();
        assert!(matches!(err, Error::UnsupportedInputComputeBudget));
    }
}
