use crate::accounts::WalletAccountTrait;

use crate::result::Result;
use crate::utxo::UtxoSet;
use std::sync::{atomic::AtomicU64, Arc};
use wasm_bindgen::prelude::*;

// Wallet Account structure

#[wasm_bindgen(inspectable)]
pub struct Account {
    // TODO bind with accounts/ primitives
    _generator: Arc<dyn WalletAccountTrait>,
    utxos: UtxoSet,
    balance: AtomicU64,
}

impl Account {
    pub async fn update_balance(&mut self) -> Result<u64> {
        let balance = self.utxos.calculate_balance().await?;
        self.balance.store(self.utxos.calculate_balance().await?, std::sync::atomic::Ordering::SeqCst);
        Ok(balance)
    }
}

#[wasm_bindgen]
impl Account {
    #[wasm_bindgen(getter)]
    pub fn balance(&self) -> u64 {
        self.balance.load(std::sync::atomic::Ordering::SeqCst)
    }
}
