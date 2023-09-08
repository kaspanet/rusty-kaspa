use kaspa_hashes::{PersonalMessageSigningHash, Hash};
use secp256k1::{XOnlyPublicKey, Error};

#[derive(Clone)]
pub struct PersonalMessage<'a>(&'a str);

impl AsRef<[u8]> for PersonalMessage<'_> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

pub fn sign_message(msg: PersonalMessage, privkey: &[u8; 32]) -> Result<Vec<u8>, Error> {
    let hash = calc_personal_message_hash(msg);

    let msg = secp256k1::Message::from_slice(hash.as_bytes().as_slice())?;
    let schnorr_key = secp256k1::KeyPair::from_seckey_slice(secp256k1::SECP256K1, privkey)?;
    let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();

    Ok(sig.to_vec())
}

pub fn verify_message(msg: PersonalMessage, signature: Vec<u8>, pubkey: XOnlyPublicKey) -> Result<bool, Error> {
    let hash = calc_personal_message_hash(msg);
    let msg = secp256k1::Message::from_slice(hash.as_bytes().as_slice())?;
    let sig = secp256k1::schnorr::Signature::from_slice(signature.as_slice())?;
    let sig_res = sig.verify(&msg, &pubkey);

    match sig_res {
        Ok(_) => Ok(true),
        Err(e) => Err(e)
    }
}

fn calc_personal_message_hash(msg: PersonalMessage) -> Hash {
    let mut hasher = PersonalMessageSigningHash::new();
    hasher.write(msg);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_and_verify_sign() {
        let pm = PersonalMessage("Hello, Kaspa!");
        let privkey: [u8; 32] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03];
        let sig_result = sign_message(pm.clone(), &privkey);

        assert!(sig_result.is_ok());

        if let Ok(sig_result) = sig_result {
            let pubkey = XOnlyPublicKey::from_slice(&[0xF9, 0x30, 0x8A, 0x01, 0x92, 0x58, 0xC3, 0x10, 0x49, 0x34, 0x4F, 0x85, 0xF8, 0x9D, 0x52, 0x29, 0xB5, 0x31, 0xC8, 0x45, 0x83, 0x6F, 0x99, 0xB0, 0x86, 0x01, 0xF1, 0x13, 0xBC, 0xE0, 0x36, 0xF9]).unwrap();
            let verify_result = verify_message(pm, sig_result, pubkey);
            assert!(verify_result.is_ok());
        }
    }
}