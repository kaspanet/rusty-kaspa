use crate::result::Result;
use consensus_core::tx::UtxoEntry;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Mutex,
};

#[derive(Clone)]
pub struct AccountUtxoEntry {
    pub utxo: Arc<UtxoEntry>,
}

impl AsRef<UtxoEntry> for AccountUtxoEntry {
    fn as_ref(&self) -> &UtxoEntry {
        &self.utxo
    }
}

// impl AsMut<UtxoEntry> for AccountUtxoEntry {
//     fn as_mut(&mut self) -> &mut UtxoEntry {
//         &mut self.0
//     }
// }

#[derive(Clone, Copy)]
#[repr(u32)]
pub enum UtxoOrdering {
    Unordered,
    AscendingAmount,
    AscendingDaaScore,
}

#[derive(Default)]
pub struct Inner {
    entries: Mutex<Vec<AccountUtxoEntry>>,
    ordered: AtomicU32,
}

#[derive(Clone, Default)]
pub struct UtxoSet {
    pub inner: Arc<Inner>,
}

impl UtxoSet {
    pub fn insert(&mut self, utxo_entry: AccountUtxoEntry) {
        self.inner.entries.lock().unwrap().push(utxo_entry);
        self.inner.ordered.store(UtxoOrdering::Unordered as u32, Ordering::SeqCst);
    }

    pub fn order(&self, order: UtxoOrdering) -> Result<()> {
        match order {
            UtxoOrdering::AscendingAmount => {
                self.inner.entries.lock().unwrap().sort_by(|a, b| a.as_ref().amount.cmp(&b.as_ref().amount));
            }
            UtxoOrdering::AscendingDaaScore => {
                self.inner.entries.lock().unwrap().sort_by(|a, b| a.as_ref().block_daa_score.cmp(&b.as_ref().block_daa_score));
            }
            UtxoOrdering::Unordered => {
                // Ok(self.entries)
            }
        }

        Ok(())
    }

    pub async fn chunks(&self, chunk_size: usize) -> Result<Vec<Vec<AccountUtxoEntry>>> {
        let entries = self.inner.entries.lock().unwrap();
        let l = entries.chunks(chunk_size).map(|v| v.to_owned()).collect();
        Ok(l)
    }

    pub async fn select(&self, amount: u64, order: UtxoOrdering) -> Result<SelectData> {
        if self.inner.ordered.load(Ordering::SeqCst) != order as u32 {
            self.order(order)?;
        }

        let mut entries = vec![];

        let total_amount = self
            .inner
            .entries
            .lock()
            .unwrap()
            .iter()
            .scan(0u64, |total, entry| {
                if *total >= amount {
                    return None;
                }

                entries.push(entry.clone());

                let amount = entry.as_ref().amount;
                *total += amount;
                Some(amount)
            })
            .sum();

        Ok(SelectData { total_amount, entries })

        // TODO - untested!
    }

    pub async fn calculate_balance(&self) -> Result<u64> {
        Ok(self.inner.entries.lock().unwrap().iter().map(|e| e.as_ref().amount).sum())
    }
}

pub struct SelectData {
    pub total_amount: u64,
    pub entries: Vec<AccountUtxoEntry>,
}
