use async_trait::async_trait;
use futures::Future;
use super::model::UtxoIndexDiff;
use super::{utxoindex::UtxoIndex, ToDo};
use consensus::model::stores::virtual_state::VirtualState;
use super::{stores::store_manager::UtxoIndexStoreManager, model::Listeners};

#[async_trait]
pub trait Processor {
    async fn run(&self) -> dyn Future<Output = i32> ;

    async fn sync_from_scratch(&self);

    fn is_sync() -> bool;
}

#[async_trait]
impl Processor for UtxoIndex {
    async fn run(&self) -> dyn Future<Output = i32> {
        while let Some(event) = self.reciever.recv().await {
            let utxo_index_state_change: UtxoIndexDiff = event.utxo_diff.into().await;    
            self.store.commit(utxo_index_state_change).await;
            self.notifiy(utxo_index_state_change).await;
        }
    }

    async fn sync_from_scratch(&self, chunk_size: usize) {
        //TODO 
    }



}