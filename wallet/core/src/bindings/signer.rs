use crate::result::Result;
use kaspa_consensus_client::{sign_with_multiple_v3, Transaction};
use kaspa_consensus_core::tx::PopulatedTransaction;
use kaspa_consensus_core::{hashing::sighash_type::SIG_HASH_ALL, sign::verify};
use kaspa_hashes::Hash;

pub fn sign_transaction<'a>(tx: &'a Transaction, private_keys: &[[u8; 32]], verify_sig: bool) -> Result<&'a Transaction> {
    let tx = sign(tx, private_keys)?;
    if verify_sig {
        let (cctx, utxos) = tx.tx_and_utxos()?;
        let populated_transaction = PopulatedTransaction::new(&cctx, utxos);
        verify(&populated_transaction)?;
    }
    Ok(tx)
}

/// Sign a transaction using schnorr, returns a new transaction with the signatures added.
/// The resulting transaction may be partially signed if the supplied keys are not sufficient
/// to sign all of its inputs.
pub fn sign<'a>(tx: &'a Transaction, privkeys: &[[u8; 32]]) -> Result<&'a Transaction> {
    Ok(sign_with_multiple_v3(tx, privkeys)?.unwrap())
}

pub fn sign_hash(sig_hash: Hash, privkey: &[u8; 32]) -> Result<Vec<u8>> {
    let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice())?;
    let schnorr_key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, privkey)?;
    let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
    let signature = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
    Ok(signature)
}
