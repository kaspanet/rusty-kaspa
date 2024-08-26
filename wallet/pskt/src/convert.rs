use crate::error::Error;
use crate::input::{Input, InputBuilder};
use crate::output::{Output, OutputBuilder};
use crate::pskt::{Global, Inner};
use kaspa_consensus_client::{Transaction, TransactionInput, TransactionInputInner, TransactionOutput, TransactionOutputInner};
use kaspa_consensus_core::tx as cctx;

impl TryFrom<Transaction> for Inner {
    type Error = Error;
    fn try_from(_transaction: Transaction) -> Result<Self, Self::Error> {
        Inner::try_from(cctx::Transaction::from(&_transaction))
    }
}

impl TryFrom<TransactionInput> for Input {
    type Error = Error;
    fn try_from(input: TransactionInput) -> std::result::Result<Input, Self::Error> {
        let TransactionInputInner { previous_outpoint, signature_script: _, sequence: _, sig_op_count, utxo } = &*input.inner();

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

        let TransactionOutputInner { value, script_public_key } = &*output.inner();

        let output = OutputBuilder::default()
        .amount(*value)
        .script_public_key(script_public_key.clone())
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
        let inputs: Result<Vec<Input>, Self::Error> = populated_inputs
            .into_iter()
            .map(|(input, utxo)| {
                InputBuilder::default()
                    .utxo_entry(utxo.to_owned().clone())
                    .previous_outpoint(input.previous_outpoint)
                    .sig_op_count(input.sig_op_count)
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
