use crate::imports::*;
use kaspa_consensus_client::{sign_with_multiple_v3, Transaction};
use kaspa_consensus_core::sign::verify;
use kaspa_consensus_core::tx::PopulatedTransaction;
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

pub fn sign_transaction<'a>(tx: &'a Transaction, private_keys: &[[u8; 32]], verify_sig: bool) -> Result<&'a Transaction> {
    let tx = sign(tx, private_keys)?;
    if verify_sig {
        let (cctx, utxos) = tx.tx_and_utxos()?;
        let populated_transaction = PopulatedTransaction::new(&cctx, utxos);
        verify(&populated_transaction)?;
    }
    Ok(tx)
}

pub fn sign<'a>(tx: &'a Transaction, privkeys: &[[u8; 32]]) -> Result<&'a Transaction> {
    Ok(sign_with_multiple_v3(tx, privkeys)?.unwrap())
}
