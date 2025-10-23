use crate::opcodes::to_small_int;
use kaspa_consensus_core::{hashing::sighash::SigHashReusedValues, tx::VerifiableTransaction};
use kaspa_txscript_errors::TxScriptError;

use crate::opcodes::OpCodeImplementation;

#[derive(Debug)]
pub struct MultiSigScriptParameters {
    pub required_signatures_count: u8,
    pub signers_count: u8,
    pub signers_pubkey: Vec<secp256k1::XOnlyPublicKey>,
}

/// Extract parameters from a standard multisig script (schnorr):
///   OP_m <pubkey1> ... <pubkeyn> OP_n OP_CHECKMULTISIG
pub fn get_multisig_params<T: VerifiableTransaction, Reused: SigHashReusedValues>(
    opcodes: &[Box<dyn OpCodeImplementation<T, Reused>>],
    checkmultisig_index: usize,
) -> Result<MultiSigScriptParameters, TxScriptError> {
    let op_n = opcodes
        .get(checkmultisig_index.checked_sub(1).ok_or_else(|| TxScriptError::InvalidState("index out of bounds".into()))?)
        .ok_or_else(|| TxScriptError::InvalidState("index out of bounds".into()))?;

    let n = to_small_int(op_n);
    if n == 0 {
        return Err(TxScriptError::InvalidPubKeyCount("n must be >= 1".into()));
    }
    let n_usize = n as usize;

    let pubkeys_end = checkmultisig_index.checked_sub(1).ok_or_else(|| TxScriptError::InvalidState("index out of bounds".into()))?;
    let pubkeys_start = pubkeys_end.checked_sub(n_usize).ok_or_else(|| TxScriptError::InvalidState("index out of bounds".into()))?;

    let pubkeys_ops =
        opcodes.get(pubkeys_start..pubkeys_end).ok_or_else(|| TxScriptError::InvalidState("index out of bounds".into()))?;

    let mut pubkeys = Vec::with_capacity(n_usize);
    for op in pubkeys_ops {
        if !op.is_push_opcode() {
            return Err(TxScriptError::InvalidOpcode("expected push opcode for pubkey".into()));
        }
        let data = op.get_data();

        match data.len() {
            32 => {
                let pk = secp256k1::XOnlyPublicKey::from_slice(data).map_err(|_| TxScriptError::PubKeyFormat)?;
                pubkeys.push(pk);
            }
            _ => return Err(TxScriptError::PubKeyFormat),
        }
    }

    let op_m = opcodes
        .get(pubkeys_start.checked_sub(1).ok_or_else(|| TxScriptError::InvalidState("index out of bounds".into()))?)
        .ok_or_else(|| TxScriptError::InvalidState("index out of bounds".into()))?;

    let m = to_small_int(op_m);

    if m > n {
        return Err(TxScriptError::InvalidSignatureCount("m must be <= n".into()));
    }

    Ok(MultiSigScriptParameters { required_signatures_count: m, signers_count: n, signers_pubkey: pubkeys })
}
