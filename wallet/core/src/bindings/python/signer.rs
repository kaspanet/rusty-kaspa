use crate::bindings::signer::{sign_hash, sign_transaction};
use crate::imports::*;
use kaspa_consensus_client::Transaction;
use kaspa_consensus_core::hashing::wasm::SighashType;
use kaspa_consensus_core::sign::sign_input;
use kaspa_consensus_core::tx::PopulatedTransaction;
use kaspa_hashes::Hash;
use kaspa_wallet_keys::privatekey::PrivateKey;

#[pyfunction]
#[pyo3(name = "sign_transaction")]
pub fn py_sign_transaction(tx: &Transaction, signer: Vec<PrivateKey>, verify_sig: bool) -> PyResult<Transaction> {
    let mut private_keys: Vec<[u8; 32]> = vec![];
    for key in signer.iter() {
        private_keys.push(key.secret_bytes());
    }

    let tx =
        sign_transaction(tx, &private_keys, verify_sig).map_err(|err| PyException::new_err(format!("Unable to sign: {err:?}")))?;
    private_keys.zeroize();
    Ok(tx.clone())
}

#[pyfunction]
#[pyo3(signature = (tx, input_index, private_key, sighash_type=None))]
pub fn create_input_signature(
    tx: &Transaction,
    input_index: u8,
    private_key: &PrivateKey,
    sighash_type: Option<SighashType>,
) -> PyResult<String> {
    let (cctx, utxos) = tx.tx_and_utxos()?;
    let populated_transaction = PopulatedTransaction::new(&cctx, utxos);

    let signature = sign_input(
        &populated_transaction,
        input_index.into(),
        &private_key.secret_bytes(),
        sighash_type.unwrap_or(SighashType::All).into(),
    );

    Ok(signature.to_hex().into())
}

#[pyfunction]
pub fn sign_script_hash(script_hash: String, privkey: &PrivateKey) -> Result<String> {
    let script_hash = Hash::from_str(&script_hash)?;
    let result = sign_hash(script_hash, &privkey.into())?;
    Ok(result.to_hex())
}
