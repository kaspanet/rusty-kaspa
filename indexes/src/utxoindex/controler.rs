use async_trait::async_trait;

use super::utxoindex::UtxoIndex;
use tokio::task::JoinSet;
use tokio::runtime::Builder;
use super::processor::Processor;


pub trait Controler{
    fn start(&self);

    fn reset(&self);

    fn shutdown(&self);

}
impl Controler for UtxoIndex {
    fn start(&self) {
        self.runtime.spawn(async move { self.run() } );
    }

    fn reset(&self) {
        self.runtime.block_on(async { self.sync_from_scratch()});
    }

    fn shutdown(&self) {
        self.runtime.shutdown_background();
    }
}