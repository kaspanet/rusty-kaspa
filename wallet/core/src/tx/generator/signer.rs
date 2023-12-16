use core::str::FromStr;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use kaspa_addresses::Address;
use kaspa_bip32::{ExtendedPublicKey, PrivateKey};
use kaspa_consensus_core::hashing::sighash::{calc_schnorr_signature_hash, SigHashReusedValues};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::{sign::sign_with_multiple_v2, tx::SignableTransaction};
use kaspa_txscript::htlc_redeem_script;
use kaspa_txscript::opcodes::codes::{OpFalse, OpTrue};
use kaspa_txscript::script_builder::ScriptBuilder;

use crate::result::Result;
use crate::runtime::account::{Receiver, Sender};
use crate::runtime::HTLC;
use crate::{runtime::Account, secret::Secret, storage::PrvKeyData};

pub trait SignerT: Send + Sync + 'static {
    fn try_sign(&self, transaction: SignableTransaction, addresses: &[Address]) -> Result<SignableTransaction>;
}

struct Inner {
    keydata: PrvKeyData,
    account: Arc<dyn Account>,
    payment_secret: Option<Secret>,
    // keys : Mutex<HashMap<Address, secp256k1::SecretKey>>,
    keys: Mutex<HashMap<Address, [u8; 32]>>,
}

pub struct Signer {
    inner: Arc<Inner>,
}

impl Signer {
    pub fn new(account: Arc<dyn Account>, keydata: PrvKeyData, payment_secret: Option<Secret>) -> Self {
        Self { inner: Arc::new(Inner { keydata, account, payment_secret, keys: Mutex::new(HashMap::new()) }) }
    }

    fn ingest(&self, addresses: &[Address]) -> Result<()> {
        let mut keys = self.inner.keys.lock().unwrap();
        let addresses = addresses.iter().filter(|a| !keys.contains_key(a)).collect::<Vec<_>>();
        if !addresses.is_empty() {
            let account = self.inner.account.clone().as_derivation_capable().expect("expecting derivation capable");

            let (receive, change) = account.derivation().addresses_indexes(&addresses)?;
            let private_keys = account.create_private_keys(&self.inner.keydata, &self.inner.payment_secret, &receive, &change)?;
            for (address, private_key) in private_keys {
                keys.insert(address.clone(), private_key.to_bytes());
            }
        }

        Ok(())
    }
}

impl SignerT for Signer {
    fn try_sign(&self, mutable_tx: SignableTransaction, addresses: &[Address]) -> Result<SignableTransaction> {
        self.ingest(addresses)?;

        let keys = self.inner.keys.lock().unwrap();
        let keys_for_signing = addresses.iter().map(|address| *keys.get(address).unwrap()).collect::<Vec<_>>();
        Ok(sign_with_multiple_v2(mutable_tx, keys_for_signing))
    }
}

// ---

struct KeydataSignerInner {
    keys: HashMap<Address, [u8; 32]>,
}

pub struct KeydataSigner {
    inner: Arc<KeydataSignerInner>,
}

impl KeydataSigner {
    pub fn new(keydata: Vec<(Address, secp256k1::SecretKey)>) -> Self {
        let keys = keydata.into_iter().map(|(address, key)| (address, key.to_bytes())).collect();
        Self { inner: Arc::new(KeydataSignerInner { keys }) }
    }
}

impl SignerT for KeydataSigner {
    fn try_sign(&self, mutable_tx: SignableTransaction, addresses: &[Address]) -> Result<SignableTransaction> {
        let keys_for_signing = addresses.iter().map(|address| *self.inner.keys.get(address).unwrap()).collect::<Vec<_>>();
        Ok(sign_with_multiple_v2(mutable_tx, keys_for_signing))
    }
}

pub(crate) struct HtlcSenderSigner {
    account: Arc<HTLC<Sender>>,
    keydata: PrvKeyData,
}

impl HtlcSenderSigner {
    pub fn new(account: Arc<HTLC<Sender>>, keydata: PrvKeyData) -> Self {
        Self { account, keydata }
    }
}

impl SignerT for HtlcSenderSigner {
    fn try_sign(&self, mut tx: SignableTransaction, _addresses: &[Address]) -> Result<SignableTransaction> {
        let prv_k = self.keydata.get_xprv(None)?;
        let prv_k = prv_k.private_key();
        let schnorr_key = secp256k1::KeyPair::from_secret_key(secp256k1::SECP256K1, prv_k);
        let mut reused_values = SigHashReusedValues::new();
        tx.tx.lock_time = self.account.locktime;

        for i in 0..tx.tx.inputs.len() {
            let sig_hash = calc_schnorr_signature_hash(&tx.as_verifiable(), i, SIG_HASH_ALL, &mut reused_values); // todo index
            let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
            let sig = schnorr_key.sign_schnorr(msg);
            let mut signature = Vec::new();
            signature.extend_from_slice(sig.as_ref().as_slice());
            signature.push(SIG_HASH_ALL.to_u8());

            let receiver = &self.account.second_party_address.payload;
            let sender = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(&self.account.xpub_key).unwrap();

            let script = htlc_redeem_script(
                receiver.as_slice(),
                sender.public_key.x_only_public_key().0.serialize().as_slice(),
                &self.account.secret_hash.as_bytes(),
                self.account.locktime,
            )
            .unwrap();
            dbg!(faster_hex::hex_string(&script));

            let mut builder = ScriptBuilder::new();
            builder.add_data(&signature).unwrap();
            builder.add_data(sender.public_key.x_only_public_key().0.serialize().as_slice()).unwrap();
            builder.add_op(OpFalse).unwrap();
            builder.add_data(&script).unwrap();

            tx.tx.inputs[i].signature_script = builder.drain();
        }
        Ok(tx)
    }
}

pub(crate) struct HtlcReceiverSigner {
    account: Arc<HTLC<Receiver>>,
    keydata: PrvKeyData,
}

impl HtlcReceiverSigner {
    pub fn new(account: Arc<HTLC<Receiver>>, keydata: PrvKeyData) -> Self {
        Self { account, keydata }
    }
}

impl SignerT for HtlcReceiverSigner {
    fn try_sign(&self, mut tx: SignableTransaction, _addresses: &[Address]) -> Result<SignableTransaction> {
        let prv_k = self.keydata.get_xprv(None)?;
        let prv_k = prv_k.private_key();
        let schnorr_key = secp256k1::KeyPair::from_secret_key(secp256k1::SECP256K1, prv_k);
        let mut reused_values = SigHashReusedValues::new();

        for i in 0..tx.tx.inputs.len() {
            let sig_hash = calc_schnorr_signature_hash(&tx.as_verifiable(), i, SIG_HASH_ALL, &mut reused_values);
            let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
            let sig = schnorr_key.sign_schnorr(msg);
            let mut signature = Vec::new();
            signature.extend_from_slice(sig.as_ref().as_slice());
            signature.push(SIG_HASH_ALL.to_u8());

            let sender = &self.account.second_party_address.payload;
            let receiver = ExtendedPublicKey::<secp256k1::PublicKey>::from_str(&self.account.xpub_key).unwrap();

            let script = htlc_redeem_script(
                receiver.public_key.x_only_public_key().0.serialize().as_slice(),
                sender.as_slice(),
                &self.account.secret_hash.as_bytes(),
                self.account.locktime,
            )
            .unwrap();
            dbg!(faster_hex::hex_string(&script));

            let mut builder = ScriptBuilder::new();
            builder.add_data(&signature).unwrap();
            builder.add_data(receiver.public_key.x_only_public_key().0.serialize().as_slice()).unwrap();
            builder.add_data(self.account.secret.as_ref().unwrap().as_slice()).unwrap();
            builder.add_op(OpTrue).unwrap();
            builder.add_data(&script).unwrap();

            tx.tx.inputs[i].signature_script = builder.drain();
        }
        Ok(tx)
    }
}
