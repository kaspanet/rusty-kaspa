use crate::{
    constants::{MAX_TX_IN_SEQUENCE_NUM, TX_VERSION},
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput},
};

use super::op_true_script::*;

// create_transaction create a transaction that spends the first output of provided transaction.
// Assumes that the output being spent has opTrueScript as it's scriptPublicKey.
// Creates the value of the spent output minus provided `fee` (in sompi).
pub fn create_transaction(tx_to_spend: &Transaction, fee: u64) -> Transaction {
    let (script_public_key, redeem_script) = op_true_script();

    // TODO: call txscript.PayToScriptHashSignatureScript(redeemScript, nil) when available
    let signature_script = pay_to_script_hash_signature_script(redeem_script, vec![]);

    let previous_outpoint = TransactionOutpoint::new(tx_to_spend.id(), 0);
    let input = TransactionInput::new(previous_outpoint, signature_script, MAX_TX_IN_SEQUENCE_NUM, 1);
    let output = TransactionOutput::new(tx_to_spend.outputs[0].value - fee, script_public_key);
    Transaction::new(TX_VERSION, vec![input], vec![output], 0, SUBNETWORK_ID_NATIVE, 0, vec![])
}

pub fn pay_to_script_hash_signature_script(redeem_script: Vec<u8>, signature: Vec<u8>) -> Vec<u8> {
    // TODO: replace all calls to this fn with the txscript PayToScriptHashSignatureScript equivalent
    let mut signature_script = vec![redeem_script.len() as u8];
    signature_script.extend_from_slice(&redeem_script);
    signature_script.push(signature.len() as u8);
    signature_script.extend_from_slice(&signature);

    signature_script
}
