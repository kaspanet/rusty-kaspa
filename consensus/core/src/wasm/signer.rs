// use secp256k1::SecretKey;

// use crate::hashing::{
//     sighash::{calc_schnorr_signature_hash, SigHashReusedValues},
//     sighash_type::SIG_HASH_ALL,
// };

use super::*;

// use thiserror::Error;
// #[derive(Error, Debug, Clone)]
// pub enum Error {
//     #[error("{0}")]
//     Message(String),
// }

pub use super::error::Error;
pub type Result = std::result::Result<SignableTransaction, super::error::Error>;

// TODO - prototype Signer trait that should be used by signing primitives
pub trait Signer {
    // fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error>;
    // fn public_key(&self) -> Result<PublicKey, Error>;

    //fn private_key(&self) -> Result<SecretKey, Error>;

    fn sign(&self, _mtx: SignableTransaction) -> Result;
}

// /// Sign a transaction using schnorr
// pub fn sign_tx(mut signable_tx: SignableTransaction, signer: Box<dyn Signer>) -> Result<SignableTransaction, Error> {
//     // pub fn sign_tx(mut signable_tx: SignableTransaction, privkey: [u8; 32]) -> SignableTransaction {
//     for i in 0..signable_tx.tx.inputs.len() {
//         signable_tx.tx.inputs[i].sig_op_count = 1;
//     }

//     let privkey = signer.private_key()?;

//     let schnorr_key = secp256k1::KeyPair::from_seckey_slice(secp256k1::SECP256K1, &privkey.secret_bytes()).unwrap();
//     let mut reused_values = SigHashReusedValues::new();
//     for i in 0..signable_tx.tx.inputs.len() {
//         let sig_hash = calc_schnorr_signature_hash(&signable_tx.as_verifiable(), i, SIG_HASH_ALL, &mut reused_values);
//         let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
//         let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
//         // This represents OP_DATA_65 <SIGNATURE+SIGHASH_TYPE> (since signature length is 64 bytes and SIGHASH_TYPE is one byte)
//         signable_tx.tx.inputs[i].signature_script = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
//     }
//     Ok(signable_tx)
// }
