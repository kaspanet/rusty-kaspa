use super::model::UtxoSetDiffByScriptPublicKey;
use super::utxoindex::UtxoIndex;
use async_trait::async_trait;
use consensus_core::BlockHashSet;
use tokio::sync::mpsc::error::TrySendError;

pub type UtxoIndexNotificationTypes = Vec<UtxoIndexNotificationType>;

pub enum UtxoIndexNotificationType {
    UtxoByScriptPublicKeyDiffNotificationType,
    CirculatingSupplyUpdateNotificationType,
    TipsUpdateNotificationType,
    All, //for ease of registering / unregistering
}
#[derive(Clone)]
pub enum UtxoIndexNotification {
    UtxoByScriptPublicKeyDiffNotification(UtxoSetDiffByScriptPublicKey),
    CirculatingSupplyUpdateNotification(u64),
    TipsUpdateEvent(BlockHashSet),
}

pub struct Notifier {
    pub utxo_diff_by_script_public_key_send: Arc<RwLock<Vec<Sender>>>,
    pub circulating_supply_send: Arc<RwLock<Vec<Sender>>>,
    pub utxoindex_tips: Arc<RwLock<Vec<Sender>>>,
}

impl Notifier {
    pub fn new() -> Self {
        Self {
            utxo_diff_by_script_public_key_send: Arc::new(RwLock::new(Vev::new())),
            circulating_supply_send: Arc::new(RwLock::new(Vev::new())),
            utxoindex_tips: Arc::new(RwLock::new(Vev::new())),
        }
    }

    pub fn notify_new_utxo_diff_by_script_public_key(&self, utxo_diff_by_script_public_key: UtxoSetDiffByScriptPublicKey) {
        let notification = UtxoIndexNotification::UtxoByScriptPublicKeyDiffNotification(utxo_diff_by_script_public_key);

        let mut locked_utxo_diff_by_script_public_key_senders = self.utxo_diff_by_script_public_key_send.lock();

        let mut i = 0;
        while i < locked_utxo_diff_by_script_public_key_senders.len() {
            match locked_utxo_diff_by_script_public_key_senders[i].try_send(notification.clone()) {
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

    pub fn notify_new_circulating_supply(&self, circulating_supply: u64) {
        let notification = UtxoIndexNotification::CirculatingSupplyUpdateNotification(circulating_supply);

        let mut locked_circulating_supply_senders = self.circulating_supply_send.lock();

        let mut i = 0;
        while i < locked_circulating_supply_senders.len() {
            match locked_circulating_supply_senders[i].try_send(notification.clone()) {
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

    pub fn notify_new_tips(&self, tips: BlockHashSet) {
        let notification = UtxoIndexNotification::TipsUpdateEvent(tips);

        let mut locked_tips_senders = self.tips_send.lock();

        let mut i = 0;
        while i < locked_tips_senders.len() {
            match locked_tips_senders[i].try_send(notification.clone()) {
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
