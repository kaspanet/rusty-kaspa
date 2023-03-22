
use crate::hashing::sighash::{SigHashReusedValues, calc_schnorr_signature_hash};

use super::*;

// TODO - prototype Signer trait that should be used by signing primitives
pub trait Signer {
    // fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error>;
    // fn public_key(&self) -> Result<PublicKey, Error>;


    fn sign(mtx : &SignableTransaction) {

    }

}


/// Sign a transaction using schnorr
pub fn sign_tx(mut signable_tx: SignableTransaction, signer : Box<dyn Signer>) -> SignableTransaction {
    // pub fn sign_tx(mut signable_tx: SignableTransaction, privkey: [u8; 32]) -> SignableTransaction {
        for i in 0..signable_tx.tx.inputs.len() {
            signable_tx.tx.inputs[i].sig_op_count = 1;
        }
    
        let privkey = signer.privkey();

        let schnorr_key = secp256k1::KeyPair::from_seckey_slice(secp256k1::SECP256K1, &privkey).unwrap();
        let mut reused_values = SigHashReusedValues::new();
        for i in 0..signable_tx.tx.inputs.len() {
            let sig_hash = calc_schnorr_signature_hash(&signable_tx.as_verifiable(), i, SIG_HASH_ALL, &mut reused_values);
            let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
            let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
            // This represents OP_DATA_65 <SIGNATURE+SIGHASH_TYPE> (since signature length is 64 bytes and SIGHASH_TYPE is one byte)
            signable_tx.tx.inputs[i].signature_script = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
        }
        signable_tx
    }
    
    