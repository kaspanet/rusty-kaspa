use crate::imports::*;
use crate::keypair::PrivateKey;
use crate::result::Result;
use crate::signable::*;
use js_sys::Array;
use kaspa_consensus_core::{
    hashing::sighash_type::SIG_HASH_ALL,
    sign::{sign_with_multiple_v2, verify},
    tx,
};
use kaspa_hashes::Hash;
use serde_wasm_bindgen::from_value;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Array, is_type_of = Array::is_array, typescript_type = "PrivateKey[]")]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub type PrivateKeyArray;
}

impl TryFrom<PrivateKeyArray> for Vec<PrivateKey> {
    type Error = crate::error::Error;
    fn try_from(keys: PrivateKeyArray) -> std::result::Result<Self, Self::Error> {
        let mut private_keys: Vec<PrivateKey> = vec![];
        for key in keys.iter() {
            private_keys.push(PrivateKey::try_from(key).map_err(|_| Self::Error::Custom("Unable to cast PrivateKey".to_string()))?);
        }

        Ok(private_keys)
    }
}

/// `signTransaction()` is a helper function to sign a transaction using a private key array or a signer array.
#[wasm_bindgen(js_name = "signTransaction")]
pub fn js_sign_transaction(mtx: SignableTransaction, signer: PrivateKeyArray, verify_sig: bool) -> Result<SignableTransaction> {
    if signer.is_array() {
        let mut private_keys: Vec<[u8; 32]> = vec![];
        for key in Array::from(&signer).iter() {
            let key = PrivateKey::try_from(key).map_err(|_| Error::Custom("Unable to cast PrivateKey".to_string()))?;
            private_keys.push(key.secret_bytes());
        }

        let mtx = sign_transaction(mtx, private_keys, verify_sig).map_err(|err| Error::Custom(format!("Unable to sign: {err:?}")))?;
        Ok(mtx)
    } else {
        Err(Error::custom("signTransaction() requires an array of signatures"))
    }
}

pub fn sign_transaction(mtx: SignableTransaction, private_keys: Vec<[u8; 32]>, verify_sig: bool) -> Result<SignableTransaction> {
    let entries = mtx.entries.clone();
    let mtx = sign_transaction_impl(mtx.into(), private_keys, verify_sig)?;
    let mtx = SignableTransaction::try_from((mtx, entries))?;
    Ok(mtx)
}

fn sign_transaction_impl(
    mtx: tx::SignableTransaction,
    private_keys: Vec<[u8; 32]>,
    verify_sig: bool,
) -> Result<tx::SignableTransaction> {
    let mtx = sign(mtx, private_keys)?;
    if verify_sig {
        let tx_verifiable = mtx.as_verifiable();
        verify(&tx_verifiable)?;
    }
    Ok(mtx)
}

/// Sign a transaction using schnorr, returns a new transaction with the signatures added.
pub fn sign(mutable_tx: tx::SignableTransaction, privkeys: Vec<[u8; 32]>) -> Result<tx::SignableTransaction> {
    Ok(sign_with_multiple_v2(mutable_tx, privkeys))
}

#[wasm_bindgen(js_name=signScriptHash)]
pub fn sign_script_hash(script_hash: JsValue, privkey: &PrivateKey) -> Result<String> {
    let script_hash = from_value(script_hash)?;
    let result = sign_hash(script_hash, &privkey.into())?;
    Ok(result.to_hex())
}

pub fn sign_hash(sig_hash: Hash, privkey: &[u8; 32]) -> Result<Vec<u8>> {
    let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice())?;
    let schnorr_key = secp256k1::KeyPair::from_seckey_slice(secp256k1::SECP256K1, privkey)?;
    let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
    let signature = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
    Ok(signature)
}
