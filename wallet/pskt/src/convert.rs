use crate::error::Error;
use crate::input::{Input, InputBuilder};
use crate::output::{Output, OutputBuilder};
use crate::pskt::Inner;
use kaspa_consensus_client::{Transaction, TransactionInput, TransactionInputInner, TransactionOutput, TransactionOutputInner};

impl From<Transaction> for Inner {
    fn from(_transaction: Transaction) -> Inner {
        // Self::Transaction(transaction)

        todo!()
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
