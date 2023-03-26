use crate::Result;
use consensus_core::{
    hashing::{
        sighash::{calc_schnorr_signature_hash, SigHashReusedValues},
        sighash_type::SIG_HASH_ALL,
    },
    sign::verify,
    tx::SignableTransaction,
    wasm::{
        keypair::PrivateKey,
        signer::{Error as SignerError, Result as SignerResult, Signer as SignerTrait},
        MutableTransaction,
    },
};
use js_sys::Array;
use std::collections::BTreeMap;
use wasm_bindgen::prelude::*;
use workflow_log::log_trace;
use workflow_wasm::abi::TryFromJsValue;

#[derive(TryFromJsValue, Clone, Debug)]
#[wasm_bindgen]
pub struct Signer {
    private_keys: Vec<PrivateKey>,
    pub verify: bool,
}

#[wasm_bindgen]
impl Signer {
    #[wasm_bindgen(constructor)]
    pub fn js_ctor(private_keys: PrivateKeyArray) -> Result<Signer> {
        Ok(Self { private_keys: private_keys.try_into()?, verify: true })
    }

    #[wasm_bindgen(js_name = "signTransaction")]
    pub fn sign_transaction(&self, mtx: MutableTransaction, verify_sig: bool) -> std::result::Result<MutableTransaction, SignerError> {
        sign_transaction(mtx, &self.private_keys.iter().map(|k| k.into()).collect::<Vec<_>>(), verify_sig)
            .map_err(|err| SignerError::Custom(err.to_string()))
    }
}

impl SignerTrait for Signer {
    fn sign(&self, mtx: SignableTransaction) -> SignerResult {
        self.sign_transaction(mtx.try_into()?, self.verify)?.try_into()
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Array, is_type_of = Array::is_array, typescript_type = "PrivateKey[]")]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub type PrivateKeyArray;

    #[wasm_bindgen(extends = js_sys::Object, typescript_type = "PrivateKey[] | Signer")]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub type PrivateKeyArrayOrSigner;
}

impl TryFrom<PrivateKeyArray> for Vec<PrivateKey> {
    type Error = crate::error::Error;
    fn try_from(keys: PrivateKeyArray) -> std::result::Result<Self, Self::Error> {
        let mut private_keys: Vec<PrivateKey> = vec![];
        for key in keys.iter() {
            private_keys.push(PrivateKey::try_from(&key).map_err(|_| Self::Error::String("Unable to cast PrivateKey".to_string()))?);
        }

        Ok(private_keys)
    }
}

#[wasm_bindgen(js_name = "signTransaction")]
pub fn js_sign_transaction(
    mtx: MutableTransaction,
    signer: PrivateKeyArrayOrSigner,
    verify_sig: bool,
) -> std::result::Result<MutableTransaction, SignerError> {
    if signer.is_array() {
        let mut private_keys: Vec<[u8; 32]> = vec![];
        for key in Array::from(&signer).iter() {
            let key = PrivateKey::try_from(&key).map_err(|_| SignerError::Custom("Unable to cast PrivateKey".to_string()))?;
            private_keys.push(key.secret_bytes());
        }

        let mtx =
            sign_transaction(mtx, &private_keys, verify_sig).map_err(|err| SignerError::Custom(format!("Unable to sign: {err:?}")))?;
        return Ok(mtx);
    }

    let signer = Signer::try_from(&JsValue::from(signer)).map_err(|_| SignerError::Custom("Unable to cast Signer".to_string()))?;
    log_trace!("\nSigning via Signer: {signer:?}....\n");
    signer.sign_transaction(mtx, verify_sig)
}

pub fn sign_transaction(mtx: MutableTransaction, private_keys: &Vec<[u8; 32]>, verify_sig: bool) -> Result<MutableTransaction> {
    let mtx = sign(mtx.try_into()?, private_keys)?;
    if verify_sig {
        let mtx_clone = mtx.clone();
        log_trace!("sign_transaction mtx: {mtx_clone:?}");
        let tx_verifiable = mtx_clone.as_verifiable();
        log_trace!("verify...");
        verify(&tx_verifiable)?;
    }
    let mtx = MutableTransaction::try_from(mtx)?;
    Ok(mtx)
}

/// Sign a transaction using schnorr
pub fn sign(mut mutable_tx: SignableTransaction, privkeys: &Vec<[u8; 32]>) -> Result<SignableTransaction> {
    let mut map = BTreeMap::new();
    for privkey in privkeys {
        let schnorr_key = secp256k1::KeyPair::from_seckey_slice(secp256k1::SECP256K1, privkey)?;
        map.insert(schnorr_key.public_key().serialize(), schnorr_key);
    }
    for i in 0..mutable_tx.tx.inputs.len() {
        mutable_tx.tx.inputs[i].sig_op_count = 1;
    }

    let mut reused_values = SigHashReusedValues::new();
    for i in 0..mutable_tx.tx.inputs.len() {
        let script = mutable_tx.entries[i].as_ref().unwrap().script_public_key.script();
        if let Some(schnorr_key) = map.get(script) {
            let sig_hash = calc_schnorr_signature_hash(&mutable_tx.as_verifiable(), i, SIG_HASH_ALL, &mut reused_values);
            let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
            let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
            // This represents OP_DATA_65 <SIGNATURE+SIGHASH_TYPE> (since signature length is 64 bytes and SIGHASH_TYPE is one byte)
            mutable_tx.tx.inputs[i].signature_script = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
        }
    }
    Ok(mutable_tx)
}
