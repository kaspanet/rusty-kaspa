use std::sync::Arc;
use std::time::Duration;

use super::super::model::UtxoSetDiffByScriptPublicKey;
use super::super::utxoindex::UtxoIndex;
use super::notifications::UtxoIndexNotification;
use async_trait::async_trait;
use consensus_core::BlockHashSet;
use tokio::sync::mpsc::error::TrySendError;

#[async_trait]
pub trait UtxoIndexNotifier: Send + Sync {
    async fn notify_new_utxo_diff_by_script_public_key(&self, utxo_diff_by_script_public_key: UtxoSetDiffByScriptPublicKey);

    async fn notify_new_circulating_supply(&self, circulating_supply: u64);

    async fn notify_new_tips(&self, tips: BlockHashSet);
}

#[async_trait]
impl UtxoIndexNotifier for UtxoIndex {
    async fn notify_new_utxo_diff_by_script_public_key(&self, utxo_diff_by_script_public_key: UtxoSetDiffByScriptPublicKey) {
        let notification = UtxoIndexNotification::UtxoByScriptPublicKeyDiffNotification(utxo_diff_by_script_public_key);

        let locked_utxo_diff_by_script_public_key_senders = self.utxo_diff_by_script_public_key_send.lock();

        let mut i = 0;
        while i < locked_utxo_diff_by_script_public_key_senders.len() {
            match locked_utxo_diff_by_script_public_key_senders[i].try_send(notification) {
                //we `try_send`, as to not have a blocking `send` within a mutex.
                Ok(_) => i += 1,
                Err(err_msg) => match err_msg {
                    TrySendError::Full(_) => {
                        //alterntive is to spawn tokio task, perhaps with timeout, which waits on block for capacity to go down.
                        locked_utxo_diff_by_script_public_key_senders.remove(i);
                    }
                    TrySendError::Closed(_) => {
                        locked_utxo_diff_by_script_public_key_senders.remove(i);
                    }
                },
            }
        }
    }

    async fn notify_new_circulating_supply(&self, circulating_supply: u64) {
        let notification = UtxoIndexNotification::CirculatingSupplyUpdateNotification(circulating_supply);

        let locked_circulating_supply_senders = self.circulating_supply_send.lock();

        let mut i = 0;
        while i < locked_circulating_supply_senders.len() {
            match locked_circulating_supply_senders[i].try_send(notification) {
                //we `try_send`, as to not have a blocking `send` within a mutex.
                Ok(_) => i += 1,
                Err(err_msg) => match err_msg {
                    TrySendError::Full(_) => {
                        //alterntive is to spawn tokio task, perhaps with timeout, which waits on block for capacity to go down.
                        locked_circulating_supply_senders.remove(i);
                    }
                    TrySendError::Closed(_) => {
                        locked_circulating_supply_senders.remove(i);
                    }
                },
            }
        }
    }

    async fn notify_new_tips(&self, tips: BlockHashSet) {
        let notification = UtxoIndexNotification::TipsUpdateEvent(tips);

        let locked_tips_senders = self.tips_send.lock();

        let mut i = 0;
        while i < locked_tips_senders.len() {
            match locked_tips_senders[i].try_send(notification) {
                //we `try_send`, as to not have a blocking `send` within a mutex.
                Ok(_) => i += 1,
                Err(err_msg) => match err_msg {
                    TrySendError::Full(_) => {
                        //alterntive is to spawn tokio task, perhaps with timeout, which waits on block for capacity to go down.
                        locked_tips_senders.remove(i);
                    }
                    TrySendError::Closed(_) => {
                        locked_tips_senders.remove(i);
                    }
                },
            }
        }
    }
}
