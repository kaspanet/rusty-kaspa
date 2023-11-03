use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use kaspa_addresses::Address;
use kaspa_bip32::PrivateKey;
use kaspa_consensus_core::{sign::sign_with_multiple_v2, tx::SignableTransaction};

use crate::result::Result;
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
