use hashes::HasherBase;

use crate::{
    constants::MAX_SCRIPT_PUBLIC_KEY_VERSION,
    tx::{scriptvec, ScriptPublicKey},
};

// op_true_script returns a P2SH script paying to an anyone-can-spend address,
// The second return value is a redeemScript to be used with txscript.PayToScriptHashSignatureScript
pub fn op_true_script() -> (ScriptPublicKey, Vec<u8>) {
    // TODO: use txscript.OpTrue instead when available
    let redeem_script = vec![81u8];

    // TODO: use txscript.PayToScriptHashScript(redeemScript) when available
    // This is just a hack
    let mut hasher = hashes::TransactionSigningHash::new();
    let redeem_script_hash = hasher.update(redeem_script.clone()).clone().finalize();
    let mut script_public_key_script = scriptvec![170u8];
    script_public_key_script.push(redeem_script_hash.as_bytes().len() as u8);
    script_public_key_script.extend_from_slice(&redeem_script_hash.as_bytes());
    script_public_key_script.push(135u8);

    let script_public_key = ScriptPublicKey::new(MAX_SCRIPT_PUBLIC_KEY_VERSION, script_public_key_script);
    (script_public_key, redeem_script)
}
