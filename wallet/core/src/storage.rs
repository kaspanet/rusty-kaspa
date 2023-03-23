use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_bip32::SecretKey;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
#[allow(unused_imports)]
use workflow_core::channel::{Channel, Receiver};

pub struct PrivateKey(Vec<SecretKey>);

#[derive(Default, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct WalletAccount {
    name: String,
    private_key_index: u32,
}

// pub enum WalletAccountVersion {
//     V1(WalletAccount),
// }

pub type WalletAccountList = Arc<Mutex<Vec<WalletAccount>>>;

#[derive(Default, Clone)]
pub struct Wallet {
    pub accounts: WalletAccountList,
}

impl Wallet {
    pub fn new() -> Wallet {
        Wallet { accounts: Arc::new(Mutex::new(Vec::new())) }
    }
}
