use std::sync::atomic::Ordering;

use crate::model::{CompactUtxoCollection, CompactUtxoEntry, UtxoSetDiffByScriptPublicKey};
use consensus::model::stores::virtual_state::VirtualState;
use consensus_core::{utxo::utxo_diff::UtxoDiff, BlockHashSet};
use hashes::Hash;

use super::super::{events::UtxoIndexEvent, notify::notifier::UtxoIndexNotifier, utxoindex::UtxoIndex};
use super::process_handler::UtxoIndexState;
use async_trait::async_trait;

#[async_trait]
pub trait ConsensusEventProcessor: UtxoIndexNotifier + Send + Sync {
    async fn process_consensus_event(&self, consensus_event: VirtualState);
    fn extract_utxo_index_events(&self, utxo_diff: UtxoDiff, tips: Vec<Hash>) -> Vec<Option<UtxoIndexEvent>>;
}

#[async_trait]
impl ConsensusEventProcessor for UtxoIndex {
    async fn process_consensus_event(&self, consensus_event: VirtualState) {
        for utxoindex_event in self.extract_utxo_index_events(consensus_event.utxo_diff, consensus_event.parents).into_iter() {
            match utxoindex_event {
                Some(utxoindex_event) => match utxoindex_event {
                    UtxoIndexEvent::UtxoByScriptPublicKeyDiffEvent(utxo_diff_by_script_public_key) => {
                        //TODO: self.utxos_by_script_public_key_store.insert(utxo_diff_by_script_public_key).await;
                        self.notify_new_utxo_diff_by_script_public_key(utxo_diff_by_script_public_key).await
                    }
                    UtxoIndexEvent::CirculatingSupplyDiffEvent(circulating_suppy) => {
                        //TODO: let updated circulating supply = self.circulating_suppy_store.update(circulating_suppy).await;
                        self.notify_new_circulating_supply(circulating_suppy as u64).await
                    }
                    UtxoIndexEvent::TipsUpdateEvent(tips) => {
                        //TODO: let updated tips = self.tips_store.update(tips).await;
                        self.notify_new_tips(tips).await
                    }
                },
                _ => (),
            }
        }
    }

    fn extract_utxo_index_events(&self, virtual_utxo_diff: UtxoDiff, virtual_tips: Vec<Hash>) -> Vec<Option<UtxoIndexEvent>> {
        let mut utxo_diff_by_script_public_key = UtxoSetDiffByScriptPublicKey::new();
        let mut circulating_supply_diff: i64;

        virtual_utxo_diff.add.into_iter().for_each(|(transaction_output, utxo)| {
            let putxo = CompactUtxoEntry::from(utxo);
            circulating_supply_diff += putxo.amount as i64;
            //For Future: check if https://doc.rust-lang.org/std/collections/struct.HashMap.html#method.try_insert can be utilized
            let collection = CompactUtxoCollection::new();
            collection.insert(transaction_output, putxo).unwrap();
            match utxo_diff_by_script_public_key.remove.insert(utxo.script_public_key, collection) {
                Some(partial_utxo_collection) => {
                    partial_utxo_collection.insert(transaction_output, putxo).unwrap();
                }
                _ => (),
            }
        });

        virtual_utxo_diff.remove.into_iter().for_each(|(transaction_output, utxo)| {
            let putxo = CompactUtxoEntry::from(utxo);
            circulating_supply_diff -= putxo.amount as i64;
            //For Future: check if https://doc.rust-lang.org/std/collections/struct.HashMap.html#method.try_insert can be utilized
            let collection = CompactUtxoCollection::new();
            collection.insert(transaction_output, putxo).unwrap();
            match utxo_diff_by_script_public_key.remove.insert(utxo.script_public_key, collection) {
                Some(partial_utxo_collection) => {
                    partial_utxo_collection.insert(transaction_output, putxo).unwrap();
                }
                _ => (),
            }
        });

        //Keep order of UtxoIndexUtxoDiffEvent, CirculatingSupplyUpdateEvent, TipsUpdateEvent, since this also signals the processing order.
        //Utxos should be updated first, since they are of highest priority,
        //Tips last, since they pertain to the index's sync status (we want to make sure rest is commited first)
        vec![
            if !utxo_diff_by_script_public_key.is_empty() {
                Some(UtxoIndexEvent::UtxoByScriptPublicKeyDiffEvent(utxo_diff_by_script_public_key))
            } else {
                None
            },
            if circulating_supply_diff > 0 {
                //make sure circulating supply is monotonic.
                Some(UtxoIndexEvent::CirculatingSupplyDiffEvent(circulating_supply_diff))
            //fetch_add makes sense since virtual sompi is not spendable.
            } else {
                None
            },
            Some(UtxoIndexEvent::TipsUpdateEvent(BlockHashSet::from_iter(virtual_tips))),
        ]
    }
}
