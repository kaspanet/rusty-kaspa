// use std::iter;

use crate::imports::*;
use crate::result::Result;
use crate::utxo::{UtxoContext, UtxoEntryReference};
use js_sys::BigInt;
// use kaspa_consensus_core::tx::UtxoEntry;
use workflow_core::time::Instant;

pub struct Selection {
    entries: Vec<UtxoEntryReference>,
    amount: u64,
}

impl Selection {
    pub fn entries(&self) -> &Vec<UtxoEntryReference> {
        &self.entries
    }

    pub fn amount(&self) -> u64 {
        self.amount
    }
}

pub struct Inner {
    utxo_context: UtxoContext,
    selection: Mutex<Selection>,
    // stream: Pin<Box<dyn Stream<Item = UtxoEntryReference> + Send>>,
}

#[wasm_bindgen]
pub struct UtxoSelectionContext {
    inner: Arc<Inner>,
}

impl UtxoSelectionContext {
    pub fn new(utxo_context: &UtxoContext) -> Self {
        Self {
            inner: Arc::new(Inner {
                utxo_context: utxo_context.clone(),
                // stream: Box::pin(UtxoStream::new(utxo_context)),
                selection: Mutex::new(Selection { entries: vec![], amount: 0 }),
            }),
        }
    }

    pub fn utxo_context(&self) -> &UtxoContext {
        &self.inner.utxo_context
    }

    pub fn selection(&self) -> MutexGuard<Selection> {
        self.inner.selection.lock().unwrap()
    }

    pub fn selected_amount(&self) -> u64 {
        self.selection().amount
    }

    pub fn addresses(&self) -> Vec<Address> {
        self.selection().entries.iter().map(|u| u.utxo.address.clone().unwrap()).collect::<Vec<Address>>()
    }

    // pub fn selected_entries(&self) -> Vec<UtxoEntryReference> {
    //     self.selection().entries
    // }

    pub fn iter(&self) -> impl Iterator<Item = UtxoEntryReference> + Send + Sync + 'static {
        UtxoSelectionContextIterator::new(self.inner.clone())
    }

    /// DEPRECATED! - DO NOT USE THIS
    pub fn select(&mut self, selection_amount: u64) -> Result<Vec<UtxoEntryReference>> {
        let mut amount = 0u64;
        let mut vec = vec![];
        // let mut iter = self.iter();
        // while let Some(entry) = iter.next() {
        for entry in self.iter() {
            amount += entry.amount();
            // self.inner.selected_entries.push(entry.clone());
            vec.push(entry);

            if amount >= selection_amount {
                break;
            }
        }

        if amount < selection_amount {
            Err(Error::InsufficientFunds)
        } else {
            // self.inner.selected_amount = amount;
            Ok(vec)
        }
    }
    /*
        pub async fn select(&mut self, selection_amount: u64) -> Result<Vec<UtxoEntryReference>> {
            let mut amount = 0u64;
            let mut vec = vec![];
            while let Some(entry) = self.inner.stream.next().await {
                amount += entry.amount();
                self.inner.selected_entries.push(entry.clone());
                vec.push(entry);

                if amount >= selection_amount {
                    break;
                }
            }

            if amount < selection_amount {
                Err(Error::InsufficientFunds)
            } else {
                self.inner.selected_amount = amount;
                Ok(vec)
            }
        }
    */

    pub fn take_selected_entries(&self) -> Vec<UtxoEntryReference> {
        self.selection().entries.split_off(0)
    }

    pub fn clear_selected_entries(&self) {
        self.selection().entries.clear()
    }

    pub fn commit(self) -> Result<()> {
        let selected_entries = self.take_selected_entries();
        let utxo_context = self.utxo_context();

        let mut inner = utxo_context.context();
        inner.mature.retain(|entry| selected_entries.contains(entry));
        let now = Instant::now();
        selected_entries.into_iter().for_each(|entry| {
            inner.consumed.insert(entry.id(), (entry, &now).into());
        });

        Ok(())
    }
}

pub struct UtxoSelectionContextIterator {
    inner: Arc<Inner>,
    cursor: usize,
}

impl UtxoSelectionContextIterator {
    pub fn new(inner: Arc<Inner>) -> Self {
        Self { inner, cursor: 0 }
    }
}

impl Iterator for UtxoSelectionContextIterator {
    type Item = UtxoEntryReference;

    fn next(&mut self) -> Option<Self::Item> {
        // let mut inner = self.inner.lock().unwrap();
        let entry = self.inner.utxo_context.context().mature.get(self.cursor).cloned();
        self.cursor += 1;
        entry.map(|entry| {
            let mut selection_context = self.inner.selection.lock().unwrap();
            selection_context.amount += entry.amount();
            selection_context.entries.push(entry.clone());
            entry
        })
    }
}

// trait UtxoCommitter {
//     fn commit(&self, utxo_entries : &UtxoEntries) -> Result<()>;
// }

#[wasm_bindgen]
impl UtxoSelectionContext {
    pub fn js_selected_amount(&self) -> BigInt {
        self.selected_amount().into()
    }

    pub fn js_addresses(&self) -> Array {
        self.selection().entries.iter().map(|u| JsValue::from(u.utxo.address.as_ref().unwrap().to_string())).collect::<Array>()
    }

    #[wasm_bindgen(js_name = "getSelectedEntries")]
    pub fn js_selected_entries(&self) -> Array {
        self.selection().entries.clone().into_iter().map(JsValue::from).collect::<Array>()
    }

    #[wasm_bindgen(js_name = "select")]
    pub async fn js_select(&mut self, amount: JsValue) -> Result<Array> {
        let _amount = amount.try_as_u64()?;
        todo!();
        // let entries = self.select(amount).await?;
        // Ok(entries.into_iter().map(JsValue::from).collect::<Array>())
    }

    #[wasm_bindgen(js_name = "commit")]
    pub fn js_commit(self) -> Result<()> {
        self.commit()
    }
}
