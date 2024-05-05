use crate::imports::*;
use crate::result::Result;
use js_sys::Array;
use kaspa_consensus_client::{sign_with_multiple_v3, Transaction};
use kaspa_consensus_core::tx::PopulatedTransaction;
use kaspa_consensus_core::{hashing::sighash_type::SIG_HASH_ALL, sign::verify};
use kaspa_hashes::Hash;
use kaspa_wallet_keys::privatekey::PrivateKey;
use serde_wasm_bindgen::from_value;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Array, is_type_of = Array::is_array, typescript_type = "(PrivateKey | HexString | Uint8Array)[]")]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub type PrivateKeyArrayT;
}

impl TryFrom<PrivateKeyArrayT> for Vec<PrivateKey> {
    type Error = crate::error::Error;
    fn try_from(keys: PrivateKeyArrayT) -> std::result::Result<Self, Self::Error> {
        let mut private_keys: Vec<PrivateKey> = vec![];
        for key in keys.iter() {
            private_keys
                .push(PrivateKey::try_owned_from(key).map_err(|_| Self::Error::Custom("Unable to cast PrivateKey".to_string()))?);
        }

        Ok(private_keys)
    }
}

/// `signTransaction()` is a helper function to sign a transaction using a private key array or a signer array.
/// @category Wallet SDK
#[wasm_bindgen(js_name = "signTransaction")]
pub fn js_sign_transaction(tx: Transaction, signer: PrivateKeyArrayT, verify_sig: bool) -> Result<Transaction> {
    if signer.is_array() {
        let mut private_keys: Vec<[u8; 32]> = vec![];
        for key in Array::from(&signer).iter() {
            let key = PrivateKey::try_cast_from(key).map_err(|_| Error::Custom("Unable to cast PrivateKey".to_string()))?;
            private_keys.push(key.as_ref().secret_bytes());
        }

        let tx = sign_transaction(tx, &private_keys, verify_sig).map_err(|err| Error::Custom(format!("Unable to sign: {err:?}")))?;
        private_keys.zeroize();
        Ok(tx)
    } else {
        Err(Error::custom("signTransaction() requires an array of signatures"))
    }
}

pub fn sign_transaction(tx: Transaction, private_keys: &[[u8; 32]], verify_sig: bool) -> Result<Transaction> {
    let tx = sign(tx, private_keys)?;
    if verify_sig {
        let (cctx, utxos) = tx.tx_and_utxos();
        let populated_transaction = PopulatedTransaction::new(&cctx, utxos);
        verify(&populated_transaction)?;
    }
    Ok(tx)
}

/// Sign a transaction using schnorr, returns a new transaction with the signatures added.
/// The resulting transaction may be partially signed if the supplied keys are not sufficient
/// to sign all of its inputs.
pub fn sign(tx: Transaction, privkeys: &[[u8; 32]]) -> Result<Transaction> {
    Ok(sign_with_multiple_v3(tx, privkeys)?.unwrap())
}

/// @category Wallet SDK
#[wasm_bindgen(js_name=signScriptHash)]
pub fn sign_script_hash(script_hash: JsValue, privkey: &PrivateKey) -> Result<String> {
    let script_hash = from_value(script_hash)?;
    let result = sign_hash(script_hash, &privkey.into())?;
    Ok(result.to_hex())
}

pub fn sign_hash(sig_hash: Hash, privkey: &[u8; 32]) -> Result<Vec<u8>> {
    let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice())?;
    let schnorr_key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, privkey)?;
    let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
    let signature = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
    Ok(signature)
}
