use async_trait::async_trait;
use super::super::utxoindex::UtxoIndex;

#[async_trait]
pub trait ConsensusSyncProcessor: Send + Sync {
    async fn sync_from_scratch(&self);
}

#[async_trait]
impl ConsensusSyncProcessor for UtxoIndex {
    async fn sync_from_scratch(&self) {
        //TODO: drain consensus reciever, delete database, start new, sync from consensus database utxo iterator.
        todo!()
    }
}