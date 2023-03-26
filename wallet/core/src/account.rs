use crate::accounts::WalletAccountTrait;

use crate::result::Result;
use crate::utxo::UtxoSet;
use std::sync::{atomic::AtomicU64, Arc};
pub struct Account {
    pub generator: Arc<dyn WalletAccountTrait>,
    pub utxos: UtxoSet,
    pub balance: AtomicU64,
}

impl Account {
    pub async fn update_balance(&mut self) -> Result<u64> {
        let balance = self.utxos.calculate_balance().await?;
        self.balance.store(self.utxos.calculate_balance().await?, std::sync::atomic::Ordering::SeqCst);
        Ok(balance)
    }
}
