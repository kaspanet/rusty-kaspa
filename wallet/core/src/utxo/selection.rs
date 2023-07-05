use super::{UtxoEntryReference, UtxoProcessor, UtxoSetIterator};
use crate::imports::*;
use crate::result::Result;
use js_sys::BigInt;
use workflow_core::time::Instant;

pub struct Inner {
    utxo_processor: UtxoProcessor,
    stream: Pin<Box<dyn Stream<Item = UtxoEntryReference> + Send>>,
    selected_entries: Vec<UtxoEntryReference>,
    selected_amount: u64,
}

#[wasm_bindgen]
pub struct UtxoSelectionContext {
    inner: Inner,
}

impl UtxoSelectionContext {
    pub fn new(utxo_processor: &UtxoProcessor) -> Self {
        Self {
            inner: Inner {
                utxo_processor: utxo_processor.clone(),
                stream: Box::pin(UtxoSetIterator::new(utxo_processor)),
                selected_entries: Vec::default(),
                selected_amount: 0,
            },
        }
    }

    pub fn selected_amount(&self) -> u64 {
        self.inner.selected_amount
    }

    pub fn addresses(&self) -> Vec<Address> {
        self.inner.selected_entries.iter().map(|u| u.utxo.address.clone().unwrap()).collect::<Vec<Address>>()
    }

    pub fn selected_entries(&self) -> &Vec<UtxoEntryReference> {
        &self.inner.selected_entries
    }

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

    pub fn commit(self) -> Result<()> {
        let mut inner = self.inner.utxo_processor.inner();
        inner.mature.retain(|entry| self.inner.selected_entries.contains(entry));
        let now = Instant::now();
        self.inner.selected_entries.into_iter().for_each(|entry| {
            inner.consumed.insert(entry.id(), (entry, &now).into());
        });

        Ok(())
    }
}

#[wasm_bindgen]
impl UtxoSelectionContext {
    pub fn js_selected_amount(&self) -> BigInt {
        self.selected_amount().into()
    }

    pub fn js_addresses(&self) -> Array {
        self.inner.selected_entries.iter().map(|u| JsValue::from(u.utxo.address.as_ref().unwrap().to_string())).collect::<Array>()
    }

    #[wasm_bindgen(js_name = "getSelectedEntries")]
    pub fn js_selected_entries(&self) -> Array {
        self.inner.selected_entries.clone().into_iter().map(JsValue::from).collect::<Array>()
    }

    #[wasm_bindgen(js_name = "select")]
    pub async fn js_select(&mut self, amount: JsValue) -> Result<Array> {
        let amount = amount.try_as_u64()?;
        let entries = self.select(amount).await?;
        Ok(entries.into_iter().map(JsValue::from).collect::<Array>())
    }

    #[wasm_bindgen(js_name = "commit")]
    pub fn js_commit(self) -> Result<()> {
        self.commit()
    }
}
