use crate::transaction::Transaction;
use core::iter::once;
use itertools::Itertools;
use kaspa_consensus_core::{
    hashing::{
        sighash::{calc_schnorr_signature_hash, SigHashReusedValues},
        sighash_type::SIG_HASH_ALL,
    },
    tx::PopulatedTransaction,
    //sign::Signed,
};
use std::collections::BTreeMap;

/// A wrapper enum that represents the transaction signed state. A transaction
/// contained by this enum can be either fully signed or partially signed.
pub enum Signed {
    Fully(Transaction),
    Partially(Transaction),
}

impl Signed {
    /// Returns the transaction regardless of whether it is fully or partially signed
    pub fn unwrap(self) -> Transaction {
        match self {
            Signed::Fully(tx) => tx,
            Signed::Partially(tx) => tx,
        }
    }
}

/// TODO (aspect) - merge this with `v1` fn above or refactor wallet core to use the script engine.
/// Sign a transaction using schnorr
#[allow(clippy::result_large_err)]
pub fn sign_with_multiple_v3(tx: Transaction, privkeys: &[[u8; 32]]) -> crate::result::Result<Signed> {
    let mut map = BTreeMap::new();
    for privkey in privkeys {
        let schnorr_key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, privkey).unwrap();
        let schnorr_public_key = schnorr_key.public_key().x_only_public_key().0;
        let script_pub_key_script = once(0x20).chain(schnorr_public_key.serialize().into_iter()).chain(once(0xac)).collect_vec();
        map.insert(script_pub_key_script, schnorr_key);
    }

    let mut reused_values = SigHashReusedValues::new();
    let mut additional_signatures_required = false;
    {
        let input_len = tx.inner().inputs.len();
        let (cctx, utxos) = tx.tx_and_utxos();
        let populated_transaction = PopulatedTransaction::new(&cctx, utxos);
        for i in 0..input_len {
            let script_pub_key = match tx.inner().inputs[i].script_public_key() {
                Some(script) => script,
                None => {
                    return Err(crate::imports::Error::Custom("expected to be called only following full UTXO population".to_string()))
                }
            };
            let script = script_pub_key.script();
            if let Some(schnorr_key) = map.get(script) {
                let sig_hash = calc_schnorr_signature_hash(&populated_transaction, i, SIG_HASH_ALL, &mut reused_values);
                let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice()).unwrap();
                let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
                // This represents OP_DATA_65 <SIGNATURE+SIGHASH_TYPE> (since signature length is 64 bytes and SIGHASH_TYPE is one byte)
                tx.set_signature_script(i, std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect())?;
            } else {
                additional_signatures_required = true;
            }
        }
    }
    if additional_signatures_required {
        Ok(Signed::Partially(tx))
    } else {
        Ok(Signed::Fully(tx))
    }
}
